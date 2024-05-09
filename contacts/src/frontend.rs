use crate::{LocalContactsRequest, State};
use kinode_process_lib::{
    http,
    http::{HttpServerRequest, IncomingHttpRequest, Method, StatusCode},
    Address, Message,
};
use std::collections::HashSet;

pub fn serve(our: &Address) {
    http::serve_ui(our, "ui", true, false, vec!["/"]).expect("couldn't serve UI");
    http::bind_http_path("/state", true, false).expect("couldn't bind HTTP state path");
    http::bind_http_path("/post", true, false).expect("couldn't bind HTTP post path");
    http::bind_ws_path("/updates", true, false).expect("couldn't bind WS updates path");

    // add icon to homepage
    kinode_process_lib::homepage::add_to_homepage("Contacts", None, Some("/"), None);
}

pub fn send_ws_updates(state: &State, ws_channels: &HashSet<u32>) {
    if ws_channels.is_empty() {
        return;
    }
    let bytes = serde_json::json!({
        "books": state.get_books_hydrated(),
        "pending_invites": state.get_invites(),
    })
    .to_string()
    .as_bytes()
    .to_vec();
    for channel_id in ws_channels.iter() {
        http::send_ws_push(
            *channel_id,
            http::WsMessageType::Text,
            kinode_process_lib::LazyLoadBlob {
                mime: Some("application/json".to_string()),
                bytes: bytes.clone(),
            },
        );
    }
}

pub fn handle_http_request(
    our: &Address,
    message: Message,
    state: &mut State,
    ws_channels: &mut HashSet<u32>,
) -> anyhow::Result<()> {
    if !message.is_request() {
        return Ok(());
    }
    let Ok(req) = serde_json::from_slice::<HttpServerRequest>(message.body()) else {
        return Err(anyhow::anyhow!("failed to parse incoming http request"));
    };

    match req {
        HttpServerRequest::Http(req) => match serve_http_paths(our, req, state, ws_channels) {
            Ok((status_code, body)) => http::send_response(
                status_code,
                Some(std::collections::HashMap::from([(
                    String::from("Content-Type"),
                    String::from("application/json"),
                )])),
                body,
            ),
            Err(e) => {
                http::send_response(StatusCode::INTERNAL_SERVER_ERROR, None, vec![]);
                return Err(e);
            }
        },
        HttpServerRequest::WebSocketOpen { channel_id, .. } => {
            // save channel id for pushing
            ws_channels.insert(channel_id);
        }
        HttpServerRequest::WebSocketClose(channel_id) => {
            // remove channel id
            ws_channels.remove(&channel_id);
        }
        HttpServerRequest::WebSocketPush { .. } => {
            // ignore for now
        }
    }
    Ok(())
}

pub fn serve_http_paths(
    our: &Address,
    req: IncomingHttpRequest,
    state: &mut State,
    ws_channels: &mut HashSet<u32>,
) -> anyhow::Result<(StatusCode, Vec<u8>)> {
    let method = req.method()?;
    // strips first section of path, which is the process name
    let bound_path = req.path()?;
    let _url_params = req.url_params();

    match bound_path.as_str() {
        "/state" => {
            if method != Method::GET {
                return Ok((StatusCode::METHOD_NOT_ALLOWED, vec![]));
            }
            Ok((
                StatusCode::OK,
                serde_json::json!({
                    "books": state.get_books_hydrated(),
                    "pending_invites": state.get_invites(),
                })
                .to_string()
                .as_bytes()
                .to_vec(),
            ))
        }
        "/post" => {
            if method != Method::POST {
                return Ok((StatusCode::METHOD_NOT_ALLOWED, vec![]));
            }
            let json_bytes = kinode_process_lib::get_blob()
                .ok_or(anyhow::anyhow!("http POST without body"))?
                .bytes;
            let request: LocalContactsRequest = serde_json::from_slice(&json_bytes)?;
            crate::handle_local_request(&our, request, state)?;
            send_ws_updates(state, ws_channels);
            state.persist();
            Ok((StatusCode::OK, vec![]))
        }
        _ => Ok((StatusCode::NOT_FOUND, vec![])),
    }
}
