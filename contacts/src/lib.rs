#![feature(let_chains)]
use crate::types::*;
use automerge::AutoCommit;
use autosurgeon::{hydrate, reconcile};
use kinode_process_lib::{await_message, http, println, Address, Message, Request, Response};
use std::collections::HashSet;

mod types;

wit_bindgen::generate!({
    path: "wit",
    world: "process",
    exports: {
        world: Component,
    },
});

kinode_process_lib::call_init!(init);
fn init(our: Address) {
    println!("start");

    let mut crdt = if let Some(state) = kinode_process_lib::get_state()
        && let Ok(crdt) = AutoCommit::load(&state)
    {
        crdt
    } else {
        let mut crdt = AutoCommit::new();
        let state = ContactBook::default(&our);
        reconcile(&mut crdt, &state).unwrap();
        crdt
    };

    let mut failed_messages: Vec<(Address, Message)> = vec![];

    let mut ws_channels: HashSet<u32> = HashSet::new();

    http::serve_ui(&our, "ui", true, false, vec!["/"]).expect("couldn't serve UI");
    http::bind_http_path("/state", true, false).expect("couldn't bind HTTP state path");
    http::bind_http_path("/post", true, false).expect("couldn't bind HTTP post path");
    http::bind_ws_path("/updates", true, false).expect("couldn't bind WS updates path");

    kinode_process_lib::timer::set_timer(10_000, None);

    loop {
        match handle_message(&our, &mut crdt, &mut failed_messages, &mut ws_channels) {
            Ok(()) => {}
            Err(e) => {
                println!("error: {:?}", e);
            }
        };
    }
}

fn handle_message(
    our: &Address,
    crdt: &mut AutoCommit,
    failed_messages: &mut Vec<(Address, Message)>,
    ws_channels: &mut HashSet<u32>,
) -> anyhow::Result<()> {
    match await_message() {
        Ok(message) => {
            if message.source().node() == our.node() {
                if message.source().process == "timer:distro:sys" {
                    // every 10 seconds, try re-sending failed messages
                    // can do cleaner stuff here like mapping between peers
                    // to avoid a buildup of messages
                    kinode_process_lib::timer::set_timer(10_000, None);
                    for (target, failed_message) in failed_messages.drain(..) {
                        println!("retrying message to {}", target);
                        Request::to(&target)
                            .body(failed_message.body())
                            .context(target.to_string())
                            .expects_response(30)
                            .send()?;
                    }
                    Ok(())
                } else if message.source().process == "http_server:distro:sys" {
                    // handle http requests
                    handle_http_request(our, message, crdt, ws_channels)
                } else {
                    if !message.is_request() {
                        return Ok(());
                    }
                    handle_local_message(
                        our,
                        serde_json::from_slice(message.body())?,
                        crdt,
                        ws_channels,
                    )
                    .or(respond(ContactResponse::Err(ContactError::BadUpdate)))
                }
            } else {
                if !message.is_request() {
                    return Ok(());
                }
                handle_remote_message(message, crdt, ws_channels)
                    .or(respond(ContactResponse::Err(ContactError::BadUpdate)))
            }
        }
        Err(send_error) => {
            // if a message fails to send, keep trying it! this is an example,
            // but you need to find a way to try and stay syned here.
            if send_error.message.is_request() {
                let context = send_error.context().unwrap_or_default();
                let target: Address = std::str::from_utf8(context)?.parse()?;
                failed_messages.push((target, send_error.message));
            }
            Ok(())
        }
    }
}

fn handle_local_message(
    our: &Address,
    update: Update,
    crdt: &mut AutoCommit,
    ws_channels: &mut HashSet<u32>,
) -> anyhow::Result<()> {
    let mut contact_book: ContactBook = hydrate(crdt)?;

    // if update was to add a peer, send them the full state
    if let Update::AddPeer(peer, _status) = &update {
        Request::to(peer)
            .body(serde_json::to_vec(&contact_book)?)
            .context(peer.to_string())
            .expects_response(30)
            .send()?;
    }

    let serialized_update = serde_json::to_vec(&update)?;
    contact_book.apply_update(update)?;
    reconcile(crdt, &contact_book).unwrap();

    // send update to all peers
    for (peer, _status) in &contact_book.peers {
        if peer != our {
            Request::to(peer)
                .body(serialized_update.clone())
                .context(peer.to_string())
                .expects_response(30)
                .send()?;
        }
    }

    for channel_id in ws_channels.iter() {
        println!("sending ws update");
        http::send_ws_push(
            *channel_id,
            http::WsMessageType::Text,
            kinode_process_lib::LazyLoadBlob {
                mime: Some("application/json".to_string()),
                bytes: serde_json::to_vec(&contact_book).unwrap(),
            },
        );
    }

    println!("state: {:?}", contact_book);
    kinode_process_lib::set_state(&crdt.save_nocompress());
    Ok(())
}

fn handle_remote_message(
    message: Message,
    crdt: &mut AutoCommit,
    ws_channels: &mut HashSet<u32>,
) -> anyhow::Result<()> {
    let mut contact_book: ContactBook = hydrate(crdt)?;

    let Some(status) = contact_book.peers.get(&message.source()) else {
        return respond(ContactResponse::Err(ContactError::UnknownPeer));
    };
    if *status != PeerStatus::ReadWrite {
        return respond(ContactResponse::Err(ContactError::ReadOnlyPeer));
    };

    match serde_json::from_slice::<Update>(message.body()) {
        Ok(book_update) => {
            contact_book.apply_update(book_update)?;
            reconcile(crdt, &contact_book)?;
        }
        Err(_) => {
            let full_book_sync: ContactBook = serde_json::from_slice(message.body())?;
            reconcile(crdt, &full_book_sync)?;
        }
    };

    for channel_id in ws_channels.iter() {
        println!("sending ws update");
        http::send_ws_push(
            *channel_id,
            http::WsMessageType::Text,
            kinode_process_lib::LazyLoadBlob {
                mime: Some("application/json".to_string()),
                bytes: serde_json::to_vec(&contact_book).unwrap(),
            },
        );
    }

    println!("state: {:?}", contact_book);
    kinode_process_lib::set_state(&crdt.save_nocompress());
    Ok(())
}

fn respond(response: ContactResponse) -> anyhow::Result<()> {
    Response::new()
        .body(serde_json::to_vec(&response).unwrap())
        .send()
}

fn handle_http_request(
    our: &Address,
    message: Message,
    crdt: &mut AutoCommit,
    ws_channels: &mut HashSet<u32>,
) -> anyhow::Result<()> {
    if !message.is_request() {
        return Ok(());
    }
    let Ok(req) = serde_json::from_slice::<http::HttpServerRequest>(message.body()) else {
        return Err(anyhow::anyhow!("failed to parse incoming http request"));
    };

    let mut contact_book: ContactBook = hydrate(crdt)?;

    match req {
        http::HttpServerRequest::Http(req) => {
            match serve_http_paths(our, req, &mut contact_book, crdt, ws_channels) {
                Ok((status_code, body)) => http::send_response(
                    status_code,
                    Some(std::collections::HashMap::from([(
                        String::from("Content-Type"),
                        String::from("application/json"),
                    )])),
                    body,
                ),
                Err(e) => {
                    http::send_response(http::StatusCode::INTERNAL_SERVER_ERROR, None, vec![]);
                    return Err(e);
                }
            }
        }
        http::HttpServerRequest::WebSocketOpen { channel_id, .. } => {
            // save channel id for pushing
            ws_channels.insert(channel_id);
        }
        http::HttpServerRequest::WebSocketClose(channel_id) => {
            // remove channel id
            ws_channels.remove(&channel_id);
        }
        http::HttpServerRequest::WebSocketPush { .. } => {
            // ignore for now
        }
    }
    Ok(())
}

fn serve_http_paths(
    our: &Address,
    req: http::IncomingHttpRequest,
    contact_book: &mut ContactBook,
    crdt: &mut AutoCommit,
    ws_channels: &mut HashSet<u32>,
) -> anyhow::Result<(http::StatusCode, Vec<u8>)> {
    let method = req.method()?;
    // strips first section of path, which is the process name
    let bound_path = req.path()?;
    let _url_params = req.url_params();

    match bound_path.as_str() {
        "/state" => {
            if method != http::Method::GET {
                return Ok((http::StatusCode::METHOD_NOT_ALLOWED, vec![]));
            }
            Ok((http::StatusCode::OK, serde_json::to_vec(&contact_book)?))
        }
        "/post" => {
            if method != http::Method::POST {
                return Ok((http::StatusCode::METHOD_NOT_ALLOWED, vec![]));
            }
            let json_bytes = kinode_process_lib::get_blob()
                .ok_or(anyhow::anyhow!("http POST without body"))?
                .bytes;
            println!("json: {}", std::str::from_utf8(&json_bytes)?);
            let update: Update = serde_json::from_slice(&json_bytes)?;
            handle_local_message(&our, update, crdt, ws_channels)?;
            Ok((http::StatusCode::OK, vec![]))
        }
        _ => Ok((http::StatusCode::NOT_FOUND, vec![])),
    }
}
