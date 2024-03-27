use autosurgeon::{Hydrate, Reconcile};
use kinode_process_lib::Address;
use serde::{Deserialize, Serialize};
use std::collections::{btree_map::Entry, BTreeMap};

#[derive(Debug, Clone, Reconcile, Hydrate, Serialize, Deserialize)]
pub struct ContactBook {
    /// The contacts in the address book.
    pub contacts: BTreeMap<String, Contact>,
    /// The peers that have a copy of the address book and can make changes.
    /// keys are addresses.to_string()
    pub peers: BTreeMap<String, PeerStatus>,
}

impl ContactBook {
    pub fn default(our: &Address) -> Self {
        Self {
            contacts: BTreeMap::new(),
            peers: BTreeMap::from([(our.to_string(), PeerStatus::ReadWrite)]),
        }
    }

    pub fn apply_update(&mut self, update: Update) -> anyhow::Result<()> {
        match update {
            Update::AddContact(id, contact) => {
                let entry = self.contacts.entry(id);
                match entry {
                    Entry::Occupied(_) => {
                        entry.and_modify(|c| {
                            if contact.description.is_some() {
                                c.description = contact.description;
                            }
                            c.socials.extend(contact.socials);
                        });
                    }
                    Entry::Vacant(_) => {
                        entry.or_insert(contact);
                    }
                }
            }
            Update::RemoveContact(id) => {
                self.contacts
                    .remove(&id)
                    .ok_or(anyhow::anyhow!("contact not found"))?;
            }
            Update::EditContactDescription(id, description) => {
                self.contacts
                    .get_mut(&id)
                    .map(|c| c.description = Some(description))
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
                self.peers.insert(address.to_string(), status);
            }
            Update::RemovePeer(address) => {
                self.peers
                    .remove(&address.to_string())
                    .ok_or(anyhow::anyhow!("peer not found"))?;
            }
            Update::ClearState(our_address) => {
                self.contacts.clear();
                self.peers = BTreeMap::from([(our_address, PeerStatus::ReadWrite)]);
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
    description: Option<String>,
    socials: BTreeMap<String, String>,
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
    /// Only usable locally. Our own address as string should be argument.
    ClearState(String),
}

pub type ContactResponse = Result<(), ContactError>;

#[derive(Serialize, Deserialize)]
pub enum ContactError {
    UnknownPeer,
    ReadOnlyPeer,
    BadUpdate,
}
