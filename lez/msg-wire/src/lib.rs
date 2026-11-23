//! `ferrata_msg`: encrypted wallet-to-wallet messages on LEZ (design: `docs/design/messages.md`). Prototype, unaudited.
//!
//! One actor per account: `(account, ferrata_msg)`. Its state, a [`Mailbox`], holds the account's published messaging
//! key, its delivery policy, contacts and block lists, its inbox of ciphertexts, and its rate counters as a sender.
//!
//! * The owner (a transaction signed by the account, sent straight to its actor) sends `Publish`, `SetPolicy`,
//!   `Contacts`, `Block`, `Delete`, `Clear` and `Send`.
//! * `Send { to, now_ms, body }` checks the sender's rate, binds the transaction to blocks stamped
//!   `[now_ms, now_ms + CLOCK_SKEW_MS)`, and passes `Deliver { now_ms, body }` to the actor `(to, ferrata_msg)`.
//! * `Deliver` is accepted only from another actor of this program (so its sender is the account that signed `Send`),
//!   and stored only if every spam rule holds ([`handle`]). Every refusal is a coded exit: nothing is written.
//!
//! The program never sees a plaintext: an envelope is opaque bytes whose length must be a padding bucket and whose
//! first five bytes (version, key epoch) must match the recipient's published key.
//!
//! The logic is here, without LEZ types, so that the guest (built against LEZ v0.4.0-rc3) and the harness share it.

use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};

/// A LEZ account id (32 bytes).
pub type Id = [u8; 32];

/// The envelope lengths a message may have: every envelope is padded to one of them.
pub const BUCKETS: [usize; 4] = [512, 1024, 2048, 4096];
/// Envelope version 1: X25519 + XChaCha20-Poly1305.
pub const ENVELOPE_X25519: u8 = 1;
/// Envelope version 2: X25519 + ML-KEM-768 (hybrid) + XChaCha20-Poly1305.
pub const ENVELOPE_HYBRID: u8 = 2;
/// An ML-KEM-768 encapsulation key (FIPS 203).
pub const MLKEM768_PK_LEN: usize = 1184;
/// Messages an inbox holds at most.
pub const MAX_MESSAGES: usize = 32;
/// Envelope bytes an inbox holds at most.
pub const MAX_INBOX_BYTES: usize = 64 * 1024;
/// Slots non-contacts may hold together in one inbox (contacts always find room).
pub const STRANGER_SLOTS: usize = 8;
/// Unread messages one non-contact may hold in one inbox.
pub const STRANGER_UNREAD: usize = 2;
/// The sender's rate: at most `RATE_MAX` sends per `RATE_WINDOW_MS`.
pub const RATE_WINDOW_MS: u64 = 3_600_000;
pub const RATE_MAX: u32 = 30;
/// A `Send`'s transaction is valid only in blocks stamped `[now_ms, now_ms + CLOCK_SKEW_MS)`.
pub const CLOCK_SKEW_MS: u64 = 600_000;
/// Entries in a contacts or block list at most.
pub const MAX_LIST: usize = 64;

/// Exit codes (a refusal writes nothing).
pub mod code {
    pub const BAD_MESSAGE: u8 = 0xA1;
    pub const BAD_STATE: u8 = 0xA2;
    /// An owner's action not signed by the owner, or not sent straight by the transaction.
    pub const NOT_AUTHORIZED: u8 = 0xA3;
    /// `Deliver` not from another actor of this program.
    pub const WRONG_SENDER: u8 = 0xA4;
    /// The recipient has published no key.
    pub const NO_KEY: u8 = 0xA5;
    /// A key that is malformed, or whose epoch does not move forward.
    pub const BAD_KEY: u8 = 0xA6;
    pub const BLOCKED: u8 = 0xA7;
    /// The recipient accepts contacts only.
    pub const NOT_A_CONTACT: u8 = 0xA8;
    /// An envelope whose length is not a bucket.
    pub const BAD_SIZE: u8 = 0xA9;
    /// An unknown envelope version, or a hybrid envelope to a key without ML-KEM.
    pub const BAD_VERSION: u8 = 0xAA;
    /// An envelope for another key epoch than the recipient's.
    pub const STALE_KEY: u8 = 0xAB;
    /// An X25519-only envelope to a recipient that published an ML-KEM key.
    pub const DOWNGRADE: u8 = 0xAC;
    pub const INBOX_FULL: u8 = 0xAD;
    /// This non-contact already holds `STRANGER_UNREAD` messages here.
    pub const STRANGER_LIMIT: u8 = 0xAE;
    /// Non-contacts already hold `STRANGER_SLOTS` messages here.
    pub const STRANGERS_FULL: u8 = 0xAF;
    pub const RATE_LIMITED: u8 = 0xB0;
    /// `now_ms` before the sender's previous send.
    pub const CLOCK_BACKWARDS: u8 = 0xB1;
    pub const SELF_SEND: u8 = 0xB2;
    pub const LIST_FULL: u8 = 0xB3;
    /// `Delete` of a sequence number the inbox does not hold.
    pub const NOT_FOUND: u8 = 0xB4;

    /// Every code with its name, for the harness's tables and the clients.
    pub const ALL: [(u8, &str); 20] = [
        (BAD_MESSAGE, "BAD_MESSAGE"),
        (BAD_STATE, "BAD_STATE"),
        (NOT_AUTHORIZED, "NOT_AUTHORIZED"),
        (WRONG_SENDER, "WRONG_SENDER"),
        (NO_KEY, "NO_KEY"),
        (BAD_KEY, "BAD_KEY"),
        (BLOCKED, "BLOCKED"),
        (NOT_A_CONTACT, "NOT_A_CONTACT"),
        (BAD_SIZE, "BAD_SIZE"),
        (BAD_VERSION, "BAD_VERSION"),
        (STALE_KEY, "STALE_KEY"),
        (DOWNGRADE, "DOWNGRADE"),
        (INBOX_FULL, "INBOX_FULL"),
        (STRANGER_LIMIT, "STRANGER_LIMIT"),
        (STRANGERS_FULL, "STRANGERS_FULL"),
        (RATE_LIMITED, "RATE_LIMITED"),
        (CLOCK_BACKWARDS, "CLOCK_BACKWARDS"),
        (SELF_SEND, "SELF_SEND"),
        (LIST_FULL, "LIST_FULL"),
        (NOT_FOUND, "NOT_FOUND"),
    ];

    pub fn name(c: u8) -> &'static str {
        ALL.iter().find(|(k, _)| *k == c).map_or("?", |(_, n)| n)
    }
}

/// A published messaging key: X25519, and ML-KEM-768 for hybrid envelopes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MsgKey {
    pub epoch: u32,
    pub x25519: [u8; 32],
    pub mlkem768: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Policy {
    /// Anyone not blocked, within the stranger limits.
    #[default]
    Open,
    /// Contacts only.
    ContactsOnly,
}

/// One delivered envelope.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Stored {
    pub seq: u64,
    pub from: Id,
    /// The sender's `now_ms` (the block's time to within `CLOCK_SKEW_MS`).
    pub at_ms: u64,
    pub body: Vec<u8>,
}

/// The state of the actor `(account, ferrata_msg)`.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Mailbox {
    pub key: Option<MsgKey>,
    pub policy: Policy,
    pub contacts: Vec<Id>,
    pub blocked: Vec<Id>,
    /// The next delivered message's sequence number.
    pub next_seq: u64,
    pub inbox: Vec<Stored>,
    /// As a sender: the start of the current rate window, sends in it, the last `now_ms`, sends in all.
    pub window_start_ms: u64,
    pub sent_in_window: u32,
    pub last_send_ms: u64,
    pub sent_total: u64,
}

impl Mailbox {
    pub fn inbox_bytes(&self) -> usize {
        self.inbox.iter().map(|m| m.body.len()).sum()
    }
}

/// The program's messages (borsh; the variant index is the first byte).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MsgMessage {
    /// Owner: publish (or rotate to) a messaging key.
    Publish { key: MsgKey },
    /// Owner: who may deliver.
    SetPolicy { policy: Policy },
    /// Owner: edit the contacts list (removals first).
    Contacts { add: Vec<Id>, remove: Vec<Id> },
    /// Owner: edit the block list (removals first).
    Block { add: Vec<Id>, remove: Vec<Id> },
    /// Owner, as a sender: send `body` (an envelope) to `to`'s inbox.
    Send { to: Id, now_ms: u64, body: Vec<u8> },
    /// From the sender's actor of this program only: store `body`.
    Deliver { now_ms: u64, body: Vec<u8> },
    /// Owner: delete these messages.
    Delete { seqs: Vec<u64> },
    /// Owner: delete every message up to and including `upto`.
    Clear { upto: u64 },
}

/// Event data of `Delivered` (selector [`selector`]`("Delivered")`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Delivered {
    pub from: Id,
    pub to: Id,
    pub seq: u64,
    pub len: u32,
}

/// Event data of `KeyPublished`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct KeyPublished {
    pub account: Id,
    pub epoch: u32,
    pub hybrid: bool,
}

/// LEZ's event selector convention: the first 8 bytes of `sha256("<program>::<EventName>")`.
pub fn selector(event: &str) -> [u8; 8] {
    let h = Sha256::new().chain_update(b"ferrata_msg::").chain_update(event.as_bytes()).finalize();
    h[..8].try_into().expect("8 bytes")
}

/// What the actor is given (LEZ's `ReceiveInput`, without LEZ's types).
pub struct Input<'a> {
    /// The receiving actor's account, and this program's account.
    pub receiver: Id,
    pub program: Id,
    /// The sending actor (account, program), `None` when the transaction sent the message itself.
    pub from: Option<(Id, Id)>,
    pub is_authorized: bool,
    pub pre_state: &'a [u8],
    pub message: &'a [u8],
}

/// What the actor answers (LEZ's `Response`, without LEZ's types).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The new state, or `None` to keep it.
    pub state: Option<Vec<u8>>,
    /// A synchronous message to the actor `(account, this program)`: (account, borsh message).
    pub deliver: Option<(Id, Vec<u8>)>,
    pub events: Vec<([u8; 8], Vec<u8>)>,
    /// The transaction's timestamp validity window `[from, to)`.
    pub timestamp_window: Option<(u64, u64)>,
}

fn encode<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

/// The envelope's version and key epoch, if it is long enough to have them.
pub fn envelope_header(body: &[u8]) -> Option<(u8, u32)> {
    let version = *body.first()?;
    let epoch = u32::from_le_bytes(body.get(1..5)?.try_into().ok()?);
    Some((version, epoch))
}

fn check_body(body: &[u8]) -> Result<(u8, u32), u8> {
    if !BUCKETS.contains(&body.len()) {
        return Err(code::BAD_SIZE);
    }
    let (version, epoch) = envelope_header(body).ok_or(code::BAD_SIZE)?;
    if version != ENVELOPE_X25519 && version != ENVELOPE_HYBRID {
        return Err(code::BAD_VERSION);
    }
    Ok((version, epoch))
}

fn edit_list(list: &mut Vec<Id>, add: &[Id], remove: &[Id]) -> Result<(), u8> {
    list.retain(|x| !remove.contains(x));
    for a in add {
        if !list.contains(a) {
            list.push(*a);
        }
    }
    if list.len() > MAX_LIST {
        return Err(code::LIST_FULL);
    }
    Ok(())
}

/// The whole program.
pub fn handle(input: &Input) -> Result<Outcome, u8> {
    let message: MsgMessage = borsh::from_slice(input.message).map_err(|_| code::BAD_MESSAGE)?;
    let mut mailbox: Mailbox = if input.pre_state.is_empty() {
        Mailbox::default()
    } else {
        borsh::from_slice(input.pre_state).map_err(|_| code::BAD_STATE)?
    };
    // Every message but `Deliver` is the owner's: signed by the account and sent by the transaction itself, so that no
    // other program can act for the owner within a transaction the owner signed for something else.
    let owner = input.is_authorized && input.from.is_none();
    let mut out = Outcome::default();
    match message {
        MsgMessage::Deliver { now_ms, body } => {
            let sender = match input.from {
                Some((account, program)) if program == input.program && account != input.receiver => account,
                _ => return Err(code::WRONG_SENDER),
            };
            let key = mailbox.key.as_ref().ok_or(code::NO_KEY)?;
            if mailbox.blocked.contains(&sender) {
                return Err(code::BLOCKED);
            }
            let contact = mailbox.contacts.contains(&sender);
            if mailbox.policy == Policy::ContactsOnly && !contact {
                return Err(code::NOT_A_CONTACT);
            }
            let (version, epoch) = check_body(&body)?;
            match (version, key.mlkem768.is_some()) {
                (ENVELOPE_HYBRID, false) => return Err(code::BAD_VERSION),
                (ENVELOPE_X25519, true) => return Err(code::DOWNGRADE),
                _ => {}
            }
            if epoch != key.epoch {
                return Err(code::STALE_KEY);
            }
            if mailbox.inbox.len() >= MAX_MESSAGES || mailbox.inbox_bytes() + body.len() > MAX_INBOX_BYTES {
                return Err(code::INBOX_FULL);
            }
            if !contact {
                if mailbox.inbox.iter().filter(|m| m.from == sender).count() >= STRANGER_UNREAD {
                    return Err(code::STRANGER_LIMIT);
                }
                let strangers = mailbox.inbox.iter().filter(|m| !mailbox.contacts.contains(&m.from)).count();
                if strangers >= STRANGER_SLOTS {
                    return Err(code::STRANGERS_FULL);
                }
            }
            let seq = mailbox.next_seq;
            let len = body.len() as u32;
            mailbox.inbox.push(Stored { seq, from: sender, at_ms: now_ms, body });
            mailbox.next_seq += 1;
            out.events.push((selector("Delivered"), encode(&Delivered { from: sender, to: input.receiver, seq, len })));
        }
        _ if !owner => return Err(code::NOT_AUTHORIZED),
        MsgMessage::Publish { key } => {
            if key.x25519 == [0; 32] || key.mlkem768.as_ref().is_some_and(|k| k.len() != MLKEM768_PK_LEN) {
                return Err(code::BAD_KEY);
            }
            if mailbox.key.as_ref().is_some_and(|old| key.epoch <= old.epoch) {
                return Err(code::BAD_KEY);
            }
            let published =
                KeyPublished { account: input.receiver, epoch: key.epoch, hybrid: key.mlkem768.is_some() };
            out.events.push((selector("KeyPublished"), encode(&published)));
            mailbox.key = Some(key);
        }
        MsgMessage::SetPolicy { policy } => mailbox.policy = policy,
        MsgMessage::Contacts { add, remove } => edit_list(&mut mailbox.contacts, &add, &remove)?,
        MsgMessage::Block { add, remove } => edit_list(&mut mailbox.blocked, &add, &remove)?,
        MsgMessage::Send { to, now_ms, body } => {
            if to == input.receiver {
                return Err(code::SELF_SEND);
            }
            check_body(&body)?;
            if now_ms < mailbox.last_send_ms {
                return Err(code::CLOCK_BACKWARDS);
            }
            if now_ms >= mailbox.window_start_ms.saturating_add(RATE_WINDOW_MS) {
                mailbox.window_start_ms = now_ms;
                mailbox.sent_in_window = 0;
            }
            if mailbox.sent_in_window >= RATE_MAX {
                return Err(code::RATE_LIMITED);
            }
            let until = now_ms.checked_add(CLOCK_SKEW_MS).ok_or(code::BAD_MESSAGE)?;
            mailbox.sent_in_window += 1;
            mailbox.last_send_ms = now_ms;
            mailbox.sent_total += 1;
            out.timestamp_window = Some((now_ms, until));
            out.deliver = Some((to, encode(&MsgMessage::Deliver { now_ms, body })));
        }
        MsgMessage::Delete { seqs } => {
            for s in &seqs {
                if !mailbox.inbox.iter().any(|m| m.seq == *s) {
                    return Err(code::NOT_FOUND);
                }
            }
            mailbox.inbox.retain(|m| !seqs.contains(&m.seq));
        }
        MsgMessage::Clear { upto } => mailbox.inbox.retain(|m| m.seq > upto),
    }
    out.state = Some(encode(&mailbox));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: Id = [0xEE; 32];
    const A: Id = [1; 32];
    const B: Id = [2; 32];
    const C: Id = [3; 32];

    fn env(version: u8, epoch: u32, len: usize) -> Vec<u8> {
        let mut b = vec![0x55; len];
        b[0] = version;
        b[1..5].copy_from_slice(&epoch.to_le_bytes());
        b
    }

    fn owner(state: &[u8], at: Id, m: &MsgMessage) -> Result<Outcome, u8> {
        let msg = encode(m);
        handle(&Input { receiver: at, program: P, from: None, is_authorized: true, pre_state: state, message: &msg })
    }

    fn deliver(state: &[u8], to: Id, from: Id, body: Vec<u8>) -> Result<Outcome, u8> {
        let msg = encode(&MsgMessage::Deliver { now_ms: 5, body });
        let i = Input { receiver: to, program: P, from: Some((from, P)), is_authorized: false, pre_state: state, message: &msg };
        handle(&i)
    }

    fn published(hybrid: bool) -> Vec<u8> {
        let key = MsgKey { epoch: 0, x25519: [9; 32], mlkem768: hybrid.then(|| vec![7; MLKEM768_PK_LEN]) };
        owner(&[], B, &MsgMessage::Publish { key }).unwrap().state.unwrap()
    }

    #[test]
    fn selectors_are_distinct() {
        assert_ne!(selector("Delivered"), selector("KeyPublished"));
    }

    #[test]
    fn send_then_deliver() {
        let out = owner(&[], A, &MsgMessage::Send { to: B, now_ms: 1000, body: env(2, 0, 2048) }).unwrap();
        assert_eq!(out.timestamp_window, Some((1000, 1000 + CLOCK_SKEW_MS)));
        let (to, msg) = out.deliver.unwrap();
        assert_eq!(to, B);
        let state = published(true);
        let i = Input { receiver: B, program: P, from: Some((A, P)), is_authorized: false, pre_state: &state, message: &msg };
        let got = handle(&i).unwrap();
        let mb: Mailbox = borsh::from_slice(&got.state.unwrap()).unwrap();
        assert_eq!(mb.inbox.len(), 1);
        assert_eq!(mb.inbox[0].from, A);
        assert_eq!(mb.inbox[0].at_ms, 1000);
    }

    #[test]
    fn refusals() {
        let hybrid = published(true);
        let plain = published(false);
        assert_eq!(deliver(&[], B, A, env(1, 0, 512)), Err(code::NO_KEY));
        assert_eq!(deliver(&hybrid, B, A, env(1, 0, 512)), Err(code::DOWNGRADE));
        assert_eq!(deliver(&plain, B, A, env(2, 0, 2048)), Err(code::BAD_VERSION));
        assert_eq!(deliver(&plain, B, A, env(1, 0, 513)), Err(code::BAD_SIZE));
        assert_eq!(deliver(&plain, B, A, env(3, 0, 512)), Err(code::BAD_VERSION));
        assert_eq!(deliver(&plain, B, A, env(1, 1, 512)), Err(code::STALE_KEY));
        // From the transaction itself, or from another program: refused.
        let msg = encode(&MsgMessage::Deliver { now_ms: 5, body: env(1, 0, 512) });
        let i = Input { receiver: B, program: P, from: None, is_authorized: true, pre_state: &plain, message: &msg };
        assert_eq!(handle(&i), Err(code::WRONG_SENDER));
        let i = Input { receiver: B, program: P, from: Some((A, [1; 32])), is_authorized: true, pre_state: &plain, message: &msg };
        assert_eq!(handle(&i), Err(code::WRONG_SENDER));
        // Owner actions need the owner's signature, sent by the transaction itself.
        let msg = encode(&MsgMessage::Clear { upto: 9 });
        let i = Input { receiver: B, program: P, from: None, is_authorized: false, pre_state: &plain, message: &msg };
        assert_eq!(handle(&i), Err(code::NOT_AUTHORIZED));
        let i = Input { receiver: B, program: P, from: Some((C, P)), is_authorized: true, pre_state: &plain, message: &msg };
        assert_eq!(handle(&i), Err(code::NOT_AUTHORIZED));
    }

    #[test]
    fn stranger_limits_and_contacts() {
        let mut s = published(false);
        for _ in 0..STRANGER_UNREAD {
            s = deliver(&s, B, A, env(1, 0, 512)).unwrap().state.unwrap();
        }
        assert_eq!(deliver(&s, B, A, env(1, 0, 512)), Err(code::STRANGER_LIMIT));
        for i in 0..(STRANGER_SLOTS - STRANGER_UNREAD) {
            s = deliver(&s, B, [10 + i as u8; 32], env(1, 0, 512)).unwrap().state.unwrap();
        }
        assert_eq!(deliver(&s, B, C, env(1, 0, 512)), Err(code::STRANGERS_FULL));
        s = owner(&s, B, &MsgMessage::Contacts { add: vec![C], remove: vec![] }).unwrap().state.unwrap();
        for _ in 0..(MAX_MESSAGES - STRANGER_SLOTS) {
            s = deliver(&s, B, C, env(1, 0, 512)).unwrap().state.unwrap();
        }
        assert_eq!(deliver(&s, B, C, env(1, 0, 512)), Err(code::INBOX_FULL));
        s = owner(&s, B, &MsgMessage::Clear { upto: 0 }).unwrap().state.unwrap();
        s = deliver(&s, B, C, env(1, 0, 512)).unwrap().state.unwrap();
        s = owner(&s, B, &MsgMessage::Block { add: vec![C], remove: vec![] }).unwrap().state.unwrap();
        s = owner(&s, B, &MsgMessage::Delete { seqs: vec![1] }).unwrap().state.unwrap();
        assert_eq!(deliver(&s, B, C, env(1, 0, 512)), Err(code::BLOCKED));
        s = owner(&s, B, &MsgMessage::SetPolicy { policy: Policy::ContactsOnly }).unwrap().state.unwrap();
        assert_eq!(deliver(&s, B, [99; 32], env(1, 0, 512)), Err(code::NOT_A_CONTACT));
        assert_eq!(owner(&s, B, &MsgMessage::Delete { seqs: vec![1] }), Err(code::NOT_FOUND));
    }

    #[test]
    fn rate_and_clock() {
        let mut s = Vec::new();
        for i in 0..RATE_MAX as u64 {
            s = owner(&s, A, &MsgMessage::Send { to: B, now_ms: 10 + i, body: env(1, 0, 512) }).unwrap().state.unwrap();
        }
        let send = |s: &[u8], now_ms| owner(s, A, &MsgMessage::Send { to: B, now_ms, body: env(1, 0, 512) });
        assert_eq!(send(&s, 100), Err(code::RATE_LIMITED));
        assert_eq!(send(&s, 5), Err(code::CLOCK_BACKWARDS));
        assert!(send(&s, 10 + RATE_WINDOW_MS).is_ok());
        assert_eq!(owner(&s, A, &MsgMessage::Send { to: A, now_ms: 100, body: env(1, 0, 512) }), Err(code::SELF_SEND));
    }

    #[test]
    fn key_rotation_moves_forward() {
        let s = published(false);
        let k = |epoch| MsgKey { epoch, x25519: [9; 32], mlkem768: None };
        assert_eq!(owner(&s, B, &MsgMessage::Publish { key: k(0) }), Err(code::BAD_KEY));
        assert!(owner(&s, B, &MsgMessage::Publish { key: k(1) }).is_ok());
        let bad = MsgKey { epoch: 0, x25519: [9; 32], mlkem768: Some(vec![1; 10]) };
        assert_eq!(owner(&[], B, &MsgMessage::Publish { key: bad }), Err(code::BAD_KEY));
    }
}
