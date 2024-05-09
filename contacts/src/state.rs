use crate::{request::Update, Contact, ContactBook, PeerStatus};
use automerge::AutoCommit;
use kinode_process_lib::{Address, Message, Request};
use serde::{ser::SerializeStruct, Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct State {
    /// Our different contact books, stored in an automerge document
    books: HashMap<Uuid, AutoCommit>,
    /// An invite to become a peer in a new contact book, and who it's from
    pending_invites: HashMap<Uuid, Invite>,
    /// Invites we've sent out that haven't been accepted or rejected yet
    outgoing_invites: HashMap<Uuid, (Address, PeerStatus)>,
    /// Book-syncing messages that failed to send. We retry these periodically until
    /// either they succeed or the peer is removed from the book.
    pub failed_messages: HashMap<Address, Message>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Invite {
    pub from: Address,
    pub name: String,
    pub status: PeerStatus,
    pub data: Vec<u8>,
}

impl State {
    pub fn new(our: &Address) -> Self {
        let mut state = Self::default();
        let book_id = Uuid::new_v4();
        let mut crdt = AutoCommit::default();
        let mut contact_book = ContactBook::new("My Contacts".to_string(), our);
        contact_book
            .apply_update(Update::AddContact(
                "Doria".to_string(),
                Contact {
                    description: Some("Developer @ Kinode".to_string()),
                    socials: BTreeMap::from([(
                        "twitter".to_string(),
                        "https://twitter.com/m_e_doria".to_string(),
                    )]),
                },
            ))
            .unwrap();
        autosurgeon::reconcile(&mut crdt, &contact_book).unwrap();
        state.add_book(book_id, crdt);
        state
    }
    pub fn add_book(&mut self, book_id: Uuid, book: AutoCommit) {
        self.books.insert(book_id, book);
    }
    pub fn remove_book(&mut self, book_id: &Uuid) {
        self.books.remove(book_id);
    }
    pub fn get_book_mut(&mut self, book_id: &Uuid) -> Option<&mut AutoCommit> {
        self.books.get_mut(book_id)
    }
    pub fn get_books_hydrated(&self) -> HashMap<Uuid, ContactBook> {
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
    pub fn add_outgoing_invite(&mut self, book_id: Uuid, address: Address, status: PeerStatus) {
        self.outgoing_invites.insert(book_id, (address, status));
    }
    pub fn get_outgoing_invite(&self, book_id: &Uuid) -> Option<&(Address, PeerStatus)> {
        self.outgoing_invites.get(book_id)
    }
    pub fn remove_outgoing_invite(&mut self, book_id: &Uuid) {
        self.outgoing_invites.remove(book_id);
    }
    pub fn retry_all_failed_messages(&mut self) -> anyhow::Result<()> {
        for (target, failed_message) in self.failed_messages.drain() {
            println!("retrying message to {}", target);
            Request::to(&target)
                .body(failed_message.body())
                .context(target.to_string())
                .expects_response(crate::TIMEOUT)
                .send()?;
        }
        Ok(())
    }
    pub fn persist(&self) {
        kinode_process_lib::set_state(
            &serde_json::to_vec(self).expect("failed to serialize state!"),
        );
    }
}

impl Serialize for State {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut ser = serializer.serialize_struct("State", 3)?;
        let books_as_bytes: HashMap<Uuid, Vec<u8>> = self
            .books
            .iter()
            .map(|(k, v)| (*k, v.clone().save()))
            .collect();
        ser.serialize_field("books", &books_as_bytes)?;
        ser.serialize_field("pending_invites", &self.pending_invites)?;
        ser.serialize_field("outgoing_invites", &self.outgoing_invites)?;
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
            outgoing_invites: HashMap<Uuid, (Address, PeerStatus)>,
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
            failed_messages: HashMap::new(),
        })
    }
}
