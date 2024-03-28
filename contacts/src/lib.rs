#![feature(let_chains)]
use crate::{
    api::*,
    state::{Invite, State},
    types::*,
};
use automerge::AutoCommit;
use autosurgeon::{hydrate, reconcile};
use kinode_process_lib::{await_message, println, Address, Message, Request, Response};
use std::collections::HashSet;
use uuid::Uuid;

mod api;
mod frontend;
mod state;
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

    let mut state = if let Some(state) = kinode_process_lib::get_state()
        && let Ok(state) = serde_json::from_slice::<State>(&state)
    {
        state
    } else {
        println!("generating new state");
        State::default()
    };

    let mut ws_channels: HashSet<u32> = HashSet::new();
    frontend::serve(&our);

    kinode_process_lib::timer::set_timer(30_000, None);

    loop {
        match handle_message(&our, &mut state, &mut ws_channels) {
            Ok(()) => {}
            Err(e) => {
                println!("error: {:?}", e);
            }
        };
    }
}

fn handle_message(
    our: &Address,
    state: &mut State,
    ws_channels: &mut HashSet<u32>,
) -> anyhow::Result<()> {
    match await_message() {
        Ok(message) => {
            if message.source().node() == our.node() {
                if message.source().process == "timer:distro:sys" {
                    // every 30 seconds, try re-sending failed messages
                    // should really do some exponential backoff here
                    kinode_process_lib::timer::set_timer(30_000, None);
                    state.retry_all_failed_messages()?;
                    Ok(())
                } else if message.source().process == "http_server:distro:sys" {
                    // handle http requests
                    frontend::handle_http_request(our, message, state, ws_channels)
                } else {
                    if !message.is_request() {
                        return Ok(());
                    }
                    handle_local_message(
                        our,
                        serde_json::from_slice(message.body())?,
                        state,
                        ws_channels,
                    )
                    .or(respond(ContactsResponse::Err(ContactsError::BadSync)))
                }
            } else {
                if !message.is_request() {
                    return Ok(());
                }
                handle_remote_message(message, state, ws_channels)
                    .or(respond(ContactsResponse::Err(ContactsError::BadSync)))
            }
        }
        Err(send_error) => {
            // if a message fails to send, keep trying it! this is an example,
            // but you need to find a way to try and stay syned here.
            if send_error.message.is_request() {
                let Some(context) = send_error.context() else {
                    return Ok(());
                };
                let target: Address = std::str::from_utf8(context)?.parse()?;
                state
                    .failed_messages
                    .entry(target)
                    .or_default()
                    .push(send_error.message);
            }
            Ok(())
        }
    }
}

fn handle_local_message(
    our: &Address,
    request: LocalContactsRequest,
    state: &mut State,
    ws_channels: &mut HashSet<u32>,
) -> anyhow::Result<()> {
    match request {
        LocalContactsRequest::Update(book_id, update) => {
            handle_update(our, book_id, update, state)?;
        }
        LocalContactsRequest::NewBook(name) => {
            let book_id = Uuid::new_v4();
            let mut crdt = AutoCommit::default();
            let contact_book = ContactBook::new(name, our);
            reconcile(&mut crdt, &contact_book)?;
            state.add_book(book_id, crdt);
        }
        LocalContactsRequest::RemoveBook(book_id) => {
            handle_update(our, book_id, Update::RemovePeer(our.clone()), state)?;
            state.remove_book(&book_id);
        }
        LocalContactsRequest::CreateInvite(book_id, address) => {
            let Some(crdt) = state.get_book(&book_id) else {
                return Err(anyhow::anyhow!("book not found"));
            };
            let contact_book: ContactBook = hydrate(crdt)?;

            state.add_outgoing_invite(book_id, address.clone());
            Request::to(&address)
                .body(serde_json::to_vec(&ContactsRequest::Invite {
                    book_id,
                    name: contact_book.name,
                    owner: contact_book.owner,
                })?)
                .context(address.to_string())
                .expects_response(30)
                .send()?;
        }
        LocalContactsRequest::AcceptInvite(book_id) => {
            let Some(invite) = state.remove_invite(&book_id) else {
                return Err(anyhow::anyhow!("invite not found"));
            };

            let mut crdt = AutoCommit::default();
            let contact_book = ContactBook::new(invite.book_name, &invite.book_owner);
            reconcile(&mut crdt, &contact_book)?;
            state.add_book(book_id, crdt);

            Request::to(invite.from)
                .body(serde_json::to_vec(&ContactsRequest::InviteResponse {
                    book_id,
                    accepted: true,
                })?)
                .expects_response(30)
                .send()?;
        }
        LocalContactsRequest::RejectInvite(book_id) => {
            let Some(invite) = state.remove_invite(&book_id) else {
                return Err(anyhow::anyhow!("invite not found"));
            };

            Request::to(invite.from)
                .body(serde_json::to_vec(&ContactsRequest::InviteResponse {
                    book_id,
                    accepted: false,
                })?)
                .expects_response(30)
                .send()?;
        }
    }
    frontend::send_ws_updates(&state, ws_channels);
    kinode_process_lib::set_state(&serde_json::to_vec(&state)?);
    Ok(())
}

fn handle_update(
    our: &Address,
    book_id: Uuid,
    update: Update,
    state: &mut State,
) -> anyhow::Result<()> {
    let Some(crdt) = state.get_book_mut(&book_id) else {
        return Err(anyhow::anyhow!("book not found"));
    };

    let removed = match &update {
        Update::RemovePeer(address) => Some(address.clone()),
        _ => None,
    };

    let mut contact_book: ContactBook = hydrate(crdt)?;
    contact_book.apply_update(update)?;
    reconcile(crdt, &contact_book).unwrap();

    let sync_request = serde_json::to_vec(&ContactsRequest::Sync {
        book_id,
        data: crdt.save(),
    })?;

    // send sync to all peers
    for (peer, _status) in &contact_book.peers {
        let peer_addr: Address = peer.parse()?;
        if &peer_addr != our {
            Request::to(peer_addr)
                .body(sync_request.clone())
                .context(peer.to_string())
                .expects_response(30)
                .send()?;
        }
    }

    // if update was to remove a peer, send them the sync too, one final time
    if let Some(address) = removed {
        Request::to(&address)
            .body(sync_request)
            .context(address.to_string())
            .expects_response(30)
            .send()?;
    }
    Ok(())
}

fn handle_remote_message(
    message: Message,
    state: &mut State,
    ws_channels: &mut HashSet<u32>,
) -> anyhow::Result<()> {
    match serde_json::from_slice::<ContactsRequest>(message.body())? {
        ContactsRequest::Sync { book_id, data } => {
            let Some(crdt) = state.get_book_mut(&book_id) else {
                return respond(ContactsResponse::Err(ContactsError::BadSync));
            };
            let contact_book: ContactBook = hydrate(crdt)?;

            let Some(status) = contact_book.peers.get(&message.source().to_string()) else {
                return respond(ContactsResponse::Err(ContactsError::UnknownPeer));
            };
            if *status != PeerStatus::ReadWrite {
                return respond(ContactsResponse::Err(ContactsError::ReadOnlyPeer));
            };

            let mut their_fork =
                AutoCommit::load(&data)?.with_actor(message.source().node().as_bytes().into());

            println!("merging update from {}", message.source().node());
            crdt.merge(&mut their_fork)?;
        }
        ContactsRequest::Invite {
            book_id,
            name,
            owner,
        } => {
            let invite = Invite {
                from: message.source().clone(),
                book_name: name,
                book_owner: owner,
            };
            state.add_invite(book_id, invite);
        }
        ContactsRequest::InviteResponse { book_id, accepted } => {
            let Some(address) = state.get_outgoing_invite(&book_id) else {
                return respond(ContactsResponse::Err(ContactsError::UnknownPeer));
            };
            if address != message.source() {
                return respond(ContactsResponse::Err(ContactsError::UnknownPeer));
            }
            state.remove_outgoing_invite(&book_id);
            if accepted {
                // send an AddPeer update to ourselves
                return handle_update(
                    &message.source(),
                    book_id,
                    Update::AddPeer(message.source().clone(), PeerStatus::ReadWrite),
                    state,
                );
            }
        }
    }
    respond(Ok(()))?;
    frontend::send_ws_updates(&state, ws_channels);
    kinode_process_lib::set_state(&serde_json::to_vec(&state)?);
    Ok(())
}

fn respond(response: ContactsResponse) -> anyhow::Result<()> {
    Response::new()
        .body(serde_json::to_vec(&response).unwrap())
        .send()
}
