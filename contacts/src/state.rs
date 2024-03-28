use automerge::AutoCommit;
use kinode_process_lib::{Address, Message, Request};
use serde::{ser::SerializeStruct, Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Invite {
    pub from: Address,
    pub book_name: String,
    pub book_owner: Address,
}

#[derive(Debug, Default)]
pub struct State {
    /// Our different contact books, stored in an automerge document
    books: HashMap<Uuid, AutoCommit>,
    /// An invite to become a peer in a new contact book, and who it's from
    pending_invites: HashMap<Uuid, Invite>,
    /// Invites we've sent out that haven't been accepted or rejected yet
    outgoing_invites: HashMap<Uuid, Address>,
    /// Book-syncing messages that failed to send. We retry these periodically until
    /// either they succeed or the peer is removed from the book.
    pub failed_messages: HashMap<Address, Vec<Message>>,
}

impl State {
    pub fn add_book(&mut self, book_id: Uuid, book: AutoCommit) {
        self.books.insert(book_id, book);
    }
    pub fn remove_book(&mut self, book_id: &Uuid) {
        self.books.remove(book_id);
    }
    pub fn get_book(&self, book_id: &Uuid) -> Option<&AutoCommit> {
        self.books.get(book_id)
    }
    pub fn get_book_mut(&mut self, book_id: &Uuid) -> Option<&mut AutoCommit> {
        self.books.get_mut(book_id)
    }
    pub fn get_books_hydrated(&self) -> HashMap<Uuid, crate::ContactBook> {
        self.books
            .iter()
            .map(|(k, v)| (*k, autosurgeon::hydrate(v).unwrap()))
            .collect()
    }
    pub fn add_invite(&mut self, book_id: Uuid, invite: Invite) {
        self.pending_invites.insert(book_id, invite);
    }
    pub fn remove_invite(&mut self, book_id: &Uuid) -> Option<Invite> {
        self.pending_invites.remove(book_id)
    }
    pub fn get_invites(&self) -> &HashMap<Uuid, Invite> {
        &self.pending_invites
    }
    pub fn add_outgoing_invite(&mut self, book_id: Uuid, address: Address) {
        self.outgoing_invites.insert(book_id, address);
    }
    pub fn get_outgoing_invite(&self, book_id: &Uuid) -> Option<&Address> {
        self.outgoing_invites.get(book_id)
    }
    pub fn remove_outgoing_invite(&mut self, book_id: &Uuid) {
        self.outgoing_invites.remove(book_id);
    }
    pub fn retry_all_failed_messages(&mut self) -> anyhow::Result<()> {
        for (target, failed_messages) in self.failed_messages.drain() {
            println!("retrying {} messages to {}", failed_messages.len(), target);
            for message in failed_messages {
                Request::to(&target)
                    .body(message.body())
                    .context(target.to_string())
                    .expects_response(30)
                    .send()?;
            }
        }
        Ok(())
    }
}

impl Serialize for State {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut ser = serializer.serialize_struct("State", 2)?;
        let books_as_bytes: HashMap<Uuid, Vec<u8>> = self
            .books
            .iter()
            .map(|(k, v)| (*k, v.clone().save()))
            .collect();
        ser.serialize_field("books", &books_as_bytes)?;
        ser.serialize_field("pending_invites", &self.pending_invites)?;
        ser.serialize_field("failed_messages", &self.failed_messages)?;
        ser.end()
    }
}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct StateHelper {
            books: HashMap<Uuid, Vec<u8>>,
            pending_invites: HashMap<Uuid, Invite>,
            outgoing_invites: HashMap<Uuid, Address>,
            failed_messages: HashMap<Address, Vec<Message>>,
        }

        let helper = StateHelper::deserialize(deserializer)?;
        let books: Result<HashMap<Uuid, AutoCommit>, _> = helper
            .books
            .into_iter()
            .map(|(k, v)| match AutoCommit::load(&v) {
                Ok(auto_commit) => Ok((k, auto_commit)),
                Err(e) => Err(e),
            })
            .collect();
        let books = books.map_err(serde::de::Error::custom)?;
        Ok(State {
            books,
            pending_invites: helper.pending_invites,
            outgoing_invites: helper.outgoing_invites,
            failed_messages: helper.failed_messages,
        })
    }
}
