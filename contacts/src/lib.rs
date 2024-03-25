#![feature(let_chains)]
use automerge::AutoCommit;
use autosurgeon::{hydrate, reconcile, Hydrate, Reconcile};
use kinode_process_lib::{await_message, call_init, println, Address, Message, Request, Response};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

wit_bindgen::generate!({
    path: "wit",
    world: "process",
    exports: {
        world: Component,
    },
});

#[derive(Debug, Clone, Reconcile, Hydrate, Serialize, Deserialize)]
struct ContactBook {
    /// The contacts in the address book.
    contacts: HashMap<String, Contact>,
    /// The peers that have a copy of the address book and can make changes.
    #[autosurgeon(with = "autosurgeon::map_with_parseable_keys")]
    peers: HashMap<Address, PeerStatus>,
}

impl ContactBook {
    fn default(our: &Address) -> Self {
        Self {
            contacts: HashMap::new(),
            peers: HashMap::from([(our.clone(), PeerStatus::ReadWrite)]),
        }
    }

    fn apply_update(&mut self, update: Update) -> anyhow::Result<()> {
        match update {
            Update::AddContact(id, contact) => {
                self.contacts.insert(id, contact);
            }
            Update::RemoveContact(id) => {
                self.contacts
                    .remove(&id)
                    .ok_or(anyhow::anyhow!("contact not found"))?;
            }
            Update::EditContactDescription(id, description) => {
                self.contacts
                    .get_mut(&id)
                    .map(|c| c.description = description)
                    .ok_or(anyhow::anyhow!("contact not found"))?;
            }
            Update::EditContactSocial(id, key, value) => {
                self.contacts
                    .get_mut(&id)
                    .map(|c| c.socials.insert(key, value))
                    .ok_or(anyhow::anyhow!("contact not found"))?;
            }
            Update::RemoveContactSocial(id, key) => {
                self.contacts
                    .get_mut(&id)
                    .map(|c| c.socials.remove(&key))
                    .ok_or(anyhow::anyhow!("contact not found"))?;
            }
            Update::AddPeer(address, status) => {
                self.peers.insert(address, status);
            }
            Update::RemovePeer(address) => {
                self.peers
                    .remove(&address)
                    .ok_or(anyhow::anyhow!("peer not found"))?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Reconcile, Hydrate, Serialize, Deserialize, PartialEq)]
enum PeerStatus {
    #[default]
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Default, Clone, Reconcile, Hydrate, Serialize, Deserialize)]
struct Contact {
    description: String,
    socials: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
enum Update {
    AddContact(String, Contact),
    RemoveContact(String),
    EditContactDescription(String, String),
    EditContactSocial(String, String, String),
    RemoveContactSocial(String, String),
    AddPeer(Address, PeerStatus),
    RemovePeer(Address),
}

type ContactResponse = Result<(), ContactError>;

#[derive(Serialize, Deserialize)]
enum ContactError {
    UnknownPeer,
    ReadOnlyPeer,
    BadUpdate,
}

call_init!(init);
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

    let contact_book: ContactBook = hydrate(&crdt).unwrap();
    println!("state: {:?}", contact_book);

    let mut failed_messages: Vec<(Address, Message)> = vec![];

    kinode_process_lib::timer::set_timer(10_000, None);

    loop {
        match handle_message(&our, &mut crdt, &mut failed_messages) {
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
                } else {
                    if !message.is_request() {
                        return Ok(());
                    }
                    handle_local_message(our, message, crdt)
                }
            } else {
                if !message.is_request() {
                    return Ok(());
                }
                handle_remote_message(message, crdt)
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
    message: Message,
    crdt: &mut AutoCommit,
) -> anyhow::Result<()> {
    let book_update: Update = serde_json::from_slice(message.body())?;

    let mut contact_book: ContactBook = hydrate(crdt)?;

    // if update was to add a peer, send them the full state
    if let Update::AddPeer(peer, _status) = &book_update {
        Request::to(peer)
            .body(serde_json::to_vec(&contact_book)?)
            .context(peer.to_string())
            .expects_response(30)
            .send()?;
    }

    contact_book.apply_update(book_update)?;
    reconcile(crdt, &contact_book).unwrap();

    // send update to all peers
    for (peer, _status) in &contact_book.peers {
        if peer != our {
            Request::to(peer)
                .body(message.body().to_vec())
                .context(peer.to_string())
                .expects_response(30)
                .send()?;
        }
    }

    println!("state: {:?}", contact_book);
    kinode_process_lib::set_state(&crdt.save_nocompress());
    Ok(())
}

fn handle_remote_message(message: Message, crdt: &mut AutoCommit) -> anyhow::Result<()> {
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

    println!("state: {:?}", contact_book);
    kinode_process_lib::set_state(&crdt.save_nocompress());
    Ok(())
}

fn respond(response: ContactResponse) -> anyhow::Result<()> {
    Response::new()
        .body(serde_json::to_vec(&response).unwrap())
        .send()
}
