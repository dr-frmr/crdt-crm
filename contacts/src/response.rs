use serde::{Deserialize, Serialize};

pub type ContactsResponse = Result<(), ContactsError>;

#[derive(Serialize, Deserialize)]
pub enum ContactsError {
    UnknownPeer,
    ReadOnlyPeer,
    BadSync,
}
