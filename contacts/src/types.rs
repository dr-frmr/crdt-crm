use autosurgeon::{Hydrate, Reconcile};
use kinode_process_lib::Address;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Reconcile, Hydrate, Serialize, Deserialize)]
pub struct ContactBook {
    /// The contacts in the address book.
    pub contacts: HashMap<String, Contact>,
    /// The peers that have a copy of the address book and can make changes.
    #[autosurgeon(with = "autosurgeon::map_with_parseable_keys")]
    pub peers: HashMap<Address, PeerStatus>,
}

impl ContactBook {
    pub fn default(our: &Address) -> Self {
        Self {
            contacts: HashMap::new(),
            peers: HashMap::from([(our.clone(), PeerStatus::ReadWrite)]),
        }
    }

    pub fn apply_update(&mut self, update: Update) -> anyhow::Result<()> {
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
pub enum PeerStatus {
    #[default]
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Default, Clone, Reconcile, Hydrate, Serialize, Deserialize)]
pub struct Contact {
    description: String,
    socials: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Update {
    AddContact(String, Contact),
    RemoveContact(String),
    EditContactDescription(String, String),
    EditContactSocial(String, String, String),
    RemoveContactSocial(String, String),
    AddPeer(Address, PeerStatus),
    RemovePeer(Address),
}

pub type ContactResponse = Result<(), ContactError>;

#[derive(Serialize, Deserialize)]
pub enum ContactError {
    UnknownPeer,
    ReadOnlyPeer,
    BadUpdate,
}
