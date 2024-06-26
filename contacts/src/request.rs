use crate::contact_book::{Contact, PeerStatus};
use kinode_process_lib::Address;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Request {
    Local(LocalContactsRequest),
    Remote(RemoteContactsRequest),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum LocalContactsRequest {
    NewBook(String),
    RemoveBook(Uuid),
    CreateInvite(Uuid, Address, PeerStatus),
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
pub enum RemoteContactsRequest {
    /// Sync between remote peers. In the future, should add options
    /// to sync without sending all data.
    Sync {
        book_id: Uuid,
        data: Vec<u8>,
    },
    Invite {
        book_id: Uuid,
        name: String,
        status: PeerStatus,
        data: Vec<u8>,
    },
    InviteResponse {
        book_id: Uuid,
        accepted: bool,
    },
}
