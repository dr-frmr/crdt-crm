#![feature(let_chains)]
use crate::{
    contact_book::{Contact, ContactBook, PeerStatus},
    request::{LocalContactsRequest, RemoteContactsRequest, Update},
    response::{ContactsError, ContactsResponse},
    state::{Invite, State},
};
use automerge::AutoCommit;
use autosurgeon::{hydrate, reconcile};
use kinode_process_lib::{await_message, println, Address, Message, Request, Response};
use std::collections::HashSet;
use uuid::Uuid;

mod contact_book;
mod frontend;
mod request;
mod response;
mod state;

wit_bindgen::generate!({
    path: "target/wit",
    world: "process-v0",
});

const TIMEOUT: u64 = 30;

kinode_process_lib::call_init!(init);
fn init(our: Address) {
    let mut state = if let Some(state) = kinode_process_lib::get_state()
        && let Ok(state) = serde_json::from_slice::<State>(&state)
    {
        println!("loading saved state");
        state
    } else {
        println!("generating new state");
        let state = State::new(&our);
        state
    };

    let mut ws_channels: HashSet<u32> = HashSet::new();
    frontend::serve(&our);

    kinode_process_lib::timer::set_timer(30_000, None);

    loop {
        handle_message(&our, &mut state, &mut ws_channels)
            .map_err(|e| println!("error: {:?}", e))
            .ok();
    }
}

fn handle_message(
    our: &Address,
    state: &mut State,
    ws_channels: &mut HashSet<u32>,
) -> anyhow::Result<()> {
    match await_message() {
        Ok(message) => {
            if message.is_local(our) {
                if message.is_process("timer:distro:sys") {
                    // every 30 seconds, try re-sending failed messages
                    // should really do some exponential backoff here
                    state.retry_all_failed_messages()?;
                    kinode_process_lib::timer::set_timer(30_000, None);
                    Ok(())
                } else if message.is_process("http_server:distro:sys") {
                    // handle http requests
                    frontend::handle_http_request(our, message, state, ws_channels)
                } else {
                    // no need to handle any other responses
                    if message.is_request() {
                        handle_local_request(our, serde_json::from_slice(message.body())?, state)?;
                        frontend::send_ws_updates(&state, ws_channels);
                        state.persist();
                        Ok(())
                    } else {
                        Ok(())
                    }
                }
            } else {
                // no need to handle remote responses: fact that we've received them is enough
                if message.is_request() {
                    handle_remote_message(our, message, state)?;
                    frontend::send_ws_updates(&state, ws_channels);
                    state.persist();
                    Ok(())
                } else {
                    Ok(())
                }
            }
        }
        Err(send_error) => {
            // if a message fails to send, keep trying it! this is just a simple example,
            // but you need to find a way to try and stay synced here or otherwise achieve
            // eventual consistency
            if send_error.message.is_request() {
                // TODO replace this in next wit update with just error.target
                let Some(context) = send_error.context() else {
                    return Ok(());
                };
                let target: Address = std::str::from_utf8(context)?.parse()?;
                state.failed_messages.insert(target, send_error.message);
            }
            Ok(())
        }
    }
}

fn handle_local_request(
    our: &Address,
    request: LocalContactsRequest,
    state: &mut State,
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
        LocalContactsRequest::CreateInvite(book_id, address, status) => {
            let Some(crdt) = state.get_book_mut(&book_id) else {
                return Err(anyhow::anyhow!("book not found"));
            };
            let contact_book: ContactBook = hydrate(crdt)?;
            let data = crdt.save();
            state.add_outgoing_invite(book_id, address.clone(), status.clone());
            Request::to(&address)
                .body(serde_json::to_vec(&RemoteContactsRequest::Invite {
                    book_id,
                    name: contact_book.name,
                    status,
                    data,
                })?)
                .context(address.to_string())
                .expects_response(TIMEOUT)
                .send()?;
        }
        LocalContactsRequest::AcceptInvite(book_id) => {
            let Some(invite) = state.remove_invite(&book_id) else {
                return Err(anyhow::anyhow!("invite not found"));
            };

            state.add_book(book_id, AutoCommit::load(&invite.data)?);

            Request::to(&invite.from)
                .body(serde_json::to_vec(
                    &RemoteContactsRequest::InviteResponse {
                        book_id,
                        accepted: true,
                    },
                )?)
                .context(invite.from.to_string())
                .expects_response(TIMEOUT)
                .send()?;
        }
        LocalContactsRequest::RejectInvite(book_id) => {
            let Some(invite) = state.remove_invite(&book_id) else {
                return Err(anyhow::anyhow!("invite not found"));
            };

            Request::to(&invite.from)
                .body(serde_json::to_vec(
                    &RemoteContactsRequest::InviteResponse {
                        book_id,
                        accepted: false,
                    },
                )?)
                .context(invite.from.to_string())
                .expects_response(TIMEOUT)
                .send()?;
        }
    }
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

    // if we just removed ourself, as the owner, set a new owner
    // so that the book is not stuck
    if removed == Some(our.clone()) && contact_book.peers.len() > 0 {
        let new_owner = contact_book.peers.keys().next().unwrap().clone();
        contact_book.owner = new_owner.parse()?;
    }

    reconcile(crdt, &contact_book).unwrap();

    let sync_request = serde_json::to_vec(&RemoteContactsRequest::Sync {
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
                .expects_response(TIMEOUT)
                .send()?;
        }
    }

    // if update was to remove a peer, send them the sync too, one final time
    if let Some(address) = removed
        && address != *our
    {
        Request::to(&address)
            .body(sync_request)
            .context(address.to_string())
            .expects_response(TIMEOUT)
            .send()?;
    }
    Ok(())
}

fn handle_remote_message(our: &Address, message: Message, state: &mut State) -> anyhow::Result<()> {
    match serde_json::from_slice::<RemoteContactsRequest>(message.body())? {
        RemoteContactsRequest::Sync { book_id, data } => {
            let Some(crdt) = state.get_book_mut(&book_id) else {
                return respond_with_err(ContactsError::BadSync);
            };
            let contact_book: ContactBook = hydrate(crdt)?;

            let Some(status) = contact_book.peers.get(&message.source().to_string()) else {
                return respond_with_err(ContactsError::UnknownPeer);
            };
            if *status == PeerStatus::ReadOnly {
                return respond_with_err(ContactsError::ReadOnlyPeer);
            };

            let mut their_fork =
                AutoCommit::load(&data)?.with_actor(message.source().node().as_bytes().into());

            println!("merging update from {}", message.source().node());
            crdt.merge(&mut their_fork)?;
        }
        RemoteContactsRequest::Invite {
            book_id,
            name,
            status,
            data,
        } => {
            let invite = Invite {
                from: message.source().clone(),
                name,
                status,
                data,
            };
            state.add_invite(book_id, invite);
        }
        RemoteContactsRequest::InviteResponse { book_id, accepted } => {
            let Some((address, status)) = state.get_outgoing_invite(&book_id) else {
                return respond_with_err(ContactsError::UnknownPeer);
            };
            if address != message.source() {
                return respond_with_err(ContactsError::UnknownPeer);
            }
            let update = Update::AddPeer(message.source().to_owned(), status.to_owned());
            state.remove_outgoing_invite(&book_id);
            if accepted {
                // send an AddPeer update to ourselves
                return handle_update(our, book_id, update, state);
            }
        }
    }
    Response::new()
        .body(serde_json::to_vec(&ContactsResponse::Ok(()))?)
        .send()
}

fn respond_with_err(err: ContactsError) -> anyhow::Result<()> {
    Response::new()
        .body(serde_json::to_vec(&ContactsResponse::Err(err))?)
        .send()
}
