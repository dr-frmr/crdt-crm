use crate::api::Update;
use autosurgeon::{Hydrate, Reconcile};
use kinode_process_lib::Address;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A "rolodex". A collection of contacts and peers that can make changes.
/// The owner is the user that originally created the book. Only they
/// can change the name. Everything else can be changed by any ReadWrite peer.
/// The owner cannot change or be removed from the peers list.
#[derive(Debug, Clone, Reconcile, Hydrate, Serialize, Deserialize)]
pub struct ContactBook {
    pub name: String,
    #[autosurgeon(with = "autosurgeon_address")]
    pub owner: Address,
    /// The contacts in the address book.
    pub contacts: BTreeMap<String, Contact>,
    /// The peers that have a copy of the address book and can make changes.
    /// keys are addresses.to_string()
    pub peers: BTreeMap<String, PeerStatus>,
}

impl ContactBook {
    pub fn new(name: String, our: &Address) -> Self {
        Self {
            name,
            owner: our.clone(),
            contacts: BTreeMap::new(),
            peers: BTreeMap::from([(our.to_string(), PeerStatus::Owner)]),
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
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Reconcile, Hydrate, Serialize, Deserialize, PartialEq)]
pub enum PeerStatus {
    #[default]
    ReadOnly,
    ReadWrite,
    Owner,
}

#[derive(Debug, Default, Clone, Reconcile, Hydrate, Serialize, Deserialize)]
pub struct Contact {
    pub description: Option<String>,
    pub socials: BTreeMap<String, String>,
}

mod autosurgeon_address {
    use autosurgeon::{Hydrate, HydrateError, Prop, ReadDoc, Reconciler};
    use kinode_process_lib::Address;
    pub(super) fn hydrate<'a, D: ReadDoc>(
        doc: &D,
        obj: &automerge::ObjId,
        prop: Prop<'a>,
    ) -> Result<Address, HydrateError> {
        let inner = String::hydrate(doc, obj, prop)?;
        inner.parse().map_err(|e| {
            HydrateError::unexpected(
                "a valid address",
                format!("an address which failed to parse due to {}", e),
            )
        })
    }

    pub(super) fn reconcile<R: Reconciler>(
        path: &Address,
        mut reconciler: R,
    ) -> Result<(), R::Error> {
        reconciler.str(path.to_string())
    }
}
