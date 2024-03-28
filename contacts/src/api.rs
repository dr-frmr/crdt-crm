use crate::types::{Contact, PeerStatus};
use kinode_process_lib::Address;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub enum LocalContactsRequest {
    NewBook(String),
    RemoveBook(Uuid),
    CreateInvite(Uuid, Address),
    AcceptInvite(Uuid),
    RejectInvite(Uuid),
    Update(Uuid, Update),
}

/// Only used locally. This is how we modify an existing book.
#[derive(Debug, Serialize, Deserialize)]
pub enum Update {
    AddContact(String, Contact),
    RemoveContact(String),
    EditContactDescription(String, String),
    EditContactSocial(String, String, String),
    RemoveContactSocial(String, String),
    /// This should not be used by frontend. User should create invite,
    /// then when invite has been accepted, backend will perform this action.
    AddPeer(Address, PeerStatus),
    RemovePeer(Address),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ContactsRequest {
    /// Sync between remote peers. In the future, should add options
    /// to sync without sending all data.
    Sync {
        book_id: Uuid,
        data: Vec<u8>,
    },
    Invite {
        book_id: Uuid,
        name: String,
        owner: Address,
    },
    InviteResponse {
        book_id: Uuid,
        accepted: bool,
    },
}

pub type ContactsResponse = Result<(), ContactsError>;

#[derive(Serialize, Deserialize)]
pub enum ContactsError {
    UnknownPeer,
    ReadOnlyPeer,
    BadSync,
}
