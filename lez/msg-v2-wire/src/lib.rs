//! `ferrata_msg_v2`: encrypted wallet-to-wallet messages on LEZ, one account per message (design:
//! `docs/design/messages.md`). Prototype, unaudited. It replaces `ferrata_msg` (v1, `lez/msg-wire`), which keeps a
//! whole inbox and the owner's counters in one actor state, so that strangers raise the owner's costs and a send
//! between two full inboxes costs more than clients budget (review findings M2, M3).
//!
//! Three kinds of actor of the program `P`, each touched only by what changes it:
//!
//! * **profile** `(account, P)`: the account's published key, policy, contacts and block lists, and its sending
//!   record (the declared times of its last sends, for a sliding-window rate, and its send counter). Only the owner's
//!   own signed transactions write it; delivery to it reads it and writes nothing.
//! * **inbox** `(pda(P, seed("inbox", owner)), P)`: the index of the owner's messages (number, sender, sender's
//!   sequence number, time, length, contact or not), created by the owner's first `Publish`.
//! * **envelope** `(pda(P, seed("envelope", recipient, sender, seq)), P)`: one ciphertext, written once by delivery,
//!   emptied when the owner deletes it. `seq` is the sender's own counter, so a sender knows the address beforehand.
//!
//! A send is `Send` at the sender's profile (signed): rate, clock, size; then `Deliver` at the recipient's profile:
//! key, block list, policy, version, epoch; then `Admit` at the recipient's inbox (capacity, stranger limits) and
//! `Store` at the new envelope. The program creates both PDAs itself (LEZ admits a PDA its program names by seed), so
//! a transaction carries no evidence, only the declared actors. Every refusal is a coded exit and writes nothing.
//!
//! The program never sees a plaintext: an envelope is opaque bytes whose length must be a padding bucket (at least
//! 2,048 bytes for a hybrid one) and whose first five bytes (version, key epoch) must match the recipient's key.
//!
//! The logic is here, without LEZ types, so that the guest (LEZ v0.4.0-rc3) and the harness (rc1) share it.

use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};

/// A LEZ account id (32 bytes).
pub type Id = [u8; 32];

/// The envelope lengths a message may have: every envelope is padded to one of them.
pub const BUCKETS: [usize; 4] = [512, 1024, 2048, 4096];
/// Envelope version 3: X25519 + XChaCha20-Poly1305, the HKDF transcript hashed (versions 1 and 2 are v1's).
pub const ENVELOPE_X25519: u8 = 3;
/// Envelope version 4: X25519 + ML-KEM-768 (hybrid) + XChaCha20-Poly1305, the HKDF transcript hashed.
pub const ENVELOPE_HYBRID: u8 = 4;
/// A hybrid header is 1,149 bytes: its smallest bucket is 2,048.
pub const HYBRID_MIN_BUCKET: usize = 2048;
/// An ML-KEM-768 encapsulation key (FIPS 203).
pub const MLKEM768_PK_LEN: usize = 1184;
/// Messages an inbox holds at most.
pub const MAX_MESSAGES: usize = 32;
/// Envelope bytes an inbox holds at most.
pub const MAX_INBOX_BYTES: usize = 64 * 1024;
/// Slots non-contacts may hold together in one inbox: contacts always keep the other 24 and at least 32 KiB.
pub const STRANGER_SLOTS: usize = 8;
/// Unread messages one non-contact may hold in one inbox.
pub const STRANGER_UNREAD: usize = 1;
/// The sender's rate: at most `RATE_MAX` sends whose declared times lie within any `RATE_WINDOW_MS`.
pub const RATE_WINDOW_MS: u64 = 3_600_000;
pub const RATE_MAX: usize = 30;
/// A `Send`'s transaction is valid only in blocks stamped `[now_ms, now_ms + CLOCK_SKEW_MS)`.
pub const CLOCK_SKEW_MS: u64 = 600_000;
/// Entries in a contacts or block list at most.
pub const MAX_LIST: usize = 64;

/// The PDA domain: `seed(tag, parts) = SHA-256(PDA_DOMAIN || tag || parts)`, u64 parts little-endian.
pub const PDA_DOMAIN: &[u8] = b"ferrata-msg/v2/";
/// LEZ v0.4's `AccountId::for_public_pda` prefix (32 bytes).
const PDA_PREFIX: &[u8; 32] = b"/LEE/v0.2/AccountId/PDA/\x00\x00\x00\x00\x00\x00\x00\x00";

/// Exit codes (a refusal writes nothing). v1's twenty keep their numbers; `STALE_SEQ` is new.
pub mod code {
    pub const BAD_MESSAGE: u8 = 0xA1;
    /// An actor state that does not decode, or a write-once actor written twice: answers a send addressed to one of the
    /// program's own accounts (an inbox or an envelope, whose state is not a profile); nothing is written.
    pub const BAD_STATE: u8 = 0xA2;
    /// An owner's action not signed by the owner, or not sent straight by the transaction.
    pub const NOT_AUTHORIZED: u8 = 0xA3;
    /// A message that did not come from the one actor allowed to send it to this receiver.
    pub const WRONG_SENDER: u8 = 0xA4;
    /// The recipient has published no key (or, for the owner's `Delete`/`Clear`, has no inbox yet).
    pub const NO_KEY: u8 = 0xA5;
    /// A key that is malformed, or whose epoch does not move forward.
    pub const BAD_KEY: u8 = 0xA6;
    pub const BLOCKED: u8 = 0xA7;
    /// The recipient accepts contacts only.
    pub const NOT_A_CONTACT: u8 = 0xA8;
    /// An envelope whose length is not a bucket, or a hybrid one below 2,048 bytes.
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
    /// `Delete` of a message number the inbox does not hold.
    pub const NOT_FOUND: u8 = 0xB4;
    /// `Send` whose `seq` is not the sender's send counter (a concurrent or replayed send).
    pub const STALE_SEQ: u8 = 0xB5;

    /// Every code with its name, for the harness's tables, the IDL and the clients.
    pub const ALL: [(u8, &str); 21] = [
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
        (STALE_SEQ, "STALE_SEQ"),
    ];

    /// The codes no input can reach: none (BAD_STATE answers a send to one of the program's own accounts).
    pub const UNREACHABLE: [u8; 0] = [];

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

/// The state of the profile actor `(account, P)`.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Profile {
    pub key: Option<MsgKey>,
    pub policy: Policy,
    pub contacts: Vec<Id>,
    pub blocked: Vec<Id>,
    /// The declared times of this account's sends within the last `RATE_WINDOW_MS` (at most `RATE_MAX`), oldest first.
    pub sent_at: Vec<u64>,
    pub last_send_ms: u64,
    /// Sends so far: the next send's `seq`.
    pub sent_total: u64,
}

/// One message of an inbox.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Entry {
    /// The message's number in this inbox.
    pub n: u64,
    pub from: Id,
    /// The sender's sequence number: with the recipient and the sender, the envelope's address.
    pub seq: u64,
    /// The sender's `now_ms` (the block's time to within `CLOCK_SKEW_MS`).
    pub at_ms: u64,
    pub len: u32,
    /// Whether the sender was a contact when it was delivered (strangers' messages count against the stranger limits).
    pub contact: bool,
}

/// The state of the inbox PDA of `owner`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Inbox {
    pub owner: Id,
    pub next_n: u64,
    pub entries: Vec<Entry>,
}

impl Inbox {
    pub fn bytes(&self) -> usize {
        self.entries.iter().map(|e| e.len as usize).sum()
    }
}

/// The state of an envelope PDA (empty once deleted).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Envelope {
    pub recipient: Id,
    pub from: Id,
    pub seq: u64,
    pub at_ms: u64,
    pub body: Vec<u8>,
}

/// The program's messages (borsh; the variant index is the first byte). 0-6 are the owner's (a transaction it signs,
/// sent straight to its profile); 7-13 pass between the program's own actors.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MsgMessage {
    /// Owner: publish (or rotate to) a messaging key; the first one also opens the inbox.
    Publish { key: MsgKey },
    /// Owner: who may deliver.
    SetPolicy { policy: Policy },
    /// Owner: edit the contacts list (removals first).
    Contacts { add: Vec<Id>, remove: Vec<Id> },
    /// Owner: edit the block list (removals first).
    Block { add: Vec<Id>, remove: Vec<Id> },
    /// Owner, as a sender: send `body` (an envelope) to `to`; `seq` must be the sender's send counter.
    Send { to: Id, now_ms: u64, seq: u64, body: Vec<u8> },
    /// Owner: delete these messages (numbers strictly ascending, at most `MAX_MESSAGES`).
    Delete { ns: Vec<u64> },
    /// Owner: delete every message numbered up to and including `upto`.
    Clear { upto: u64 },
    /// Sender's profile to recipient's profile.
    Deliver { now_ms: u64, seq: u64, body: Vec<u8> },
    /// Owner's profile to its inbox: create it.
    Open,
    /// Recipient's profile to its inbox: index a message.
    Admit { from: Id, seq: u64, at_ms: u64, len: u32, contact: bool },
    /// Recipient's profile to a new envelope: keep the ciphertext.
    Store { from: Id, seq: u64, at_ms: u64, body: Vec<u8> },
    /// Owner's profile to its inbox: delete these messages.
    Remove { ns: Vec<u64> },
    /// Owner's profile to its inbox: delete every message numbered up to `upto`.
    RemoveUpTo { upto: u64 },
    /// Inbox to one of its envelopes: empty it.
    Erase,
}

/// Event data of `Delivered` (selector [`selector`]`("Delivered")`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Delivered {
    pub from: Id,
    pub to: Id,
    pub n: u64,
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
    let h = Sha256::new().chain_update(b"ferrata_msg_v2::").chain_update(event.as_bytes()).finalize();
    h[..8].try_into().expect("8 bytes")
}

/// `SHA-256(PDA_DOMAIN || tag || parts)`.
pub fn seed(tag: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new().chain_update(PDA_DOMAIN).chain_update(tag);
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

pub fn inbox_seed(owner: &Id) -> [u8; 32] {
    seed(b"inbox", &[owner])
}

pub fn envelope_seed(recipient: &Id, sender: &Id, seq: u64) -> [u8; 32] {
    seed(b"envelope", &[recipient, sender, &seq.to_le_bytes()])
}

/// LEZ v0.4's `AccountId::for_public_pda(program, seed)`.
pub fn pda(program: &Id, seed: &[u8; 32]) -> Id {
    Sha256::new().chain_update(PDA_PREFIX).chain_update(program).chain_update(seed).finalize().into()
}

/// The account of `owner`'s inbox actor.
pub fn inbox_account(program: &Id, owner: &Id) -> Id {
    pda(program, &inbox_seed(owner))
}

/// The account of the envelope actor of `sender`'s message number `seq` to `recipient`.
pub fn envelope_account(program: &Id, recipient: &Id, sender: &Id, seq: u64) -> Id {
    pda(program, &envelope_seed(recipient, sender, seq))
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

/// A synchronous message to the actor `(to, this program)`, with the PDA seed that admits and authorizes it if the
/// receiver is one of the program's PDAs.
#[derive(Debug, PartialEq, Eq)]
pub struct Call {
    pub to: Id,
    pub message: Vec<u8>,
    pub seed: Option<[u8; 32]>,
}

/// What the actor answers (LEZ's `Response`, without LEZ's types).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The new state, or `None` to keep it.
    pub state: Option<Vec<u8>>,
    pub calls: Vec<Call>,
    pub events: Vec<([u8; 8], Vec<u8>)>,
    /// The transaction's timestamp validity window `[from, to)`.
    pub timestamp_window: Option<(u64, u64)>,
}

fn encode<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

fn decode<T: BorshDeserialize>(bytes: &[u8]) -> Result<T, u8> {
    borsh::from_slice(bytes).map_err(|_| code::BAD_STATE)
}

fn profile_of(pre_state: &[u8]) -> Result<Profile, u8> {
    if pre_state.is_empty() {
        Ok(Profile::default())
    } else {
        decode(pre_state)
    }
}

/// The envelope's version and key epoch, if it is long enough to have them.
pub fn envelope_header(body: &[u8]) -> Option<(u8, u32)> {
    let version = *body.first()?;
    let epoch = u32::from_le_bytes(body.get(1..5)?.try_into().ok()?);
    Some((version, epoch))
}

/// Length a bucket, a known version, and a hybrid one at least `HYBRID_MIN_BUCKET`.
pub fn check_body(body: &[u8]) -> Result<(u8, u32), u8> {
    if !BUCKETS.contains(&body.len()) {
        return Err(code::BAD_SIZE);
    }
    let (version, epoch) = envelope_header(body).ok_or(code::BAD_SIZE)?;
    if version != ENVELOPE_X25519 && version != ENVELOPE_HYBRID {
        return Err(code::BAD_VERSION);
    }
    if version == ENVELOPE_HYBRID && body.len() < HYBRID_MIN_BUCKET {
        return Err(code::BAD_SIZE);
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

/// Message numbers for `Delete`: one to `MAX_MESSAGES` of them, strictly ascending.
fn check_ns(ns: &[u64]) -> Result<(), u8> {
    if ns.is_empty() || ns.len() > MAX_MESSAGES || ns.windows(2).any(|w| w[0] >= w[1]) {
        return Err(code::BAD_MESSAGE);
    }
    Ok(())
}

/// The sender of an internal message, when it is another actor of this program.
fn own_sender(input: &Input) -> Option<Id> {
    match input.from {
        Some((account, program)) if program == input.program => Some(account),
        _ => None,
    }
}

/// The owner whose profile sent this to its own inbox, which must be the receiver.
fn inbox_owner(input: &Input) -> Result<Id, u8> {
    let owner = own_sender(input).ok_or(code::WRONG_SENDER)?;
    if inbox_account(&input.program, &owner) != input.receiver {
        return Err(code::WRONG_SENDER);
    }
    Ok(owner)
}

/// The inbox's state, owned by `owner`.
fn inbox_of(input: &Input, owner: &Id) -> Result<Inbox, u8> {
    if input.pre_state.is_empty() {
        return Err(code::BAD_STATE);
    }
    let inbox: Inbox = decode(input.pre_state)?;
    if inbox.owner != *owner {
        return Err(code::BAD_STATE);
    }
    Ok(inbox)
}

/// `Erase` calls for removed entries, and the inbox's new state.
fn removed(input: &Input, owner: &Id, inbox: &Inbox, gone: Vec<Entry>, out: &mut Outcome) {
    for e in gone {
        let message = encode(&MsgMessage::Erase);
        let seed = envelope_seed(owner, &e.from, e.seq);
        out.calls.push(Call { to: pda(&input.program, &seed), message, seed: Some(seed) });
    }
    out.state = Some(encode(inbox));
}

/// The whole program.
pub fn handle(input: &Input) -> Result<Outcome, u8> {
    let message: MsgMessage = borsh::from_slice(input.message).map_err(|_| code::BAD_MESSAGE)?;
    // The owner's messages are signed by the account and sent by the transaction itself, so that no other program
    // can act for the owner within a transaction the owner signed for something else.
    let owner = input.is_authorized && input.from.is_none();
    let me = input.receiver;
    let mut out = Outcome::default();
    match message {
        MsgMessage::Publish { .. }
        | MsgMessage::SetPolicy { .. }
        | MsgMessage::Contacts { .. }
        | MsgMessage::Block { .. }
        | MsgMessage::Send { .. }
        | MsgMessage::Delete { .. }
        | MsgMessage::Clear { .. }
            if !owner =>
        {
            return Err(code::NOT_AUTHORIZED)
        }
        MsgMessage::Publish { key } => {
            let mut profile = profile_of(input.pre_state)?;
            if key.x25519 == [0; 32] || key.mlkem768.as_ref().is_some_and(|k| k.len() != MLKEM768_PK_LEN) {
                return Err(code::BAD_KEY);
            }
            if profile.key.as_ref().is_some_and(|old| key.epoch <= old.epoch) {
                return Err(code::BAD_KEY);
            }
            if profile.key.is_none() {
                let seed = inbox_seed(&me);
                out.calls.push(Call { to: pda(&input.program, &seed), message: encode(&MsgMessage::Open), seed: Some(seed) });
            }
            let published = KeyPublished { account: me, epoch: key.epoch, hybrid: key.mlkem768.is_some() };
            out.events.push((selector("KeyPublished"), encode(&published)));
            profile.key = Some(key);
            out.state = Some(encode(&profile));
        }
        MsgMessage::SetPolicy { policy } => {
            let mut profile = profile_of(input.pre_state)?;
            profile.policy = policy;
            out.state = Some(encode(&profile));
        }
        MsgMessage::Contacts { add, remove } => {
            let mut profile = profile_of(input.pre_state)?;
            edit_list(&mut profile.contacts, &add, &remove)?;
            out.state = Some(encode(&profile));
        }
        MsgMessage::Block { add, remove } => {
            let mut profile = profile_of(input.pre_state)?;
            edit_list(&mut profile.blocked, &add, &remove)?;
            out.state = Some(encode(&profile));
        }
        MsgMessage::Send { to, now_ms, seq, body } => {
            let mut profile = profile_of(input.pre_state)?;
            if to == me {
                return Err(code::SELF_SEND);
            }
            if seq != profile.sent_total {
                return Err(code::STALE_SEQ);
            }
            check_body(&body)?;
            if now_ms < profile.last_send_ms {
                return Err(code::CLOCK_BACKWARDS);
            }
            // Every kept time is <= last_send_ms <= now_ms.
            profile.sent_at.retain(|t| now_ms - t < RATE_WINDOW_MS);
            if profile.sent_at.len() >= RATE_MAX {
                return Err(code::RATE_LIMITED);
            }
            let until = now_ms.checked_add(CLOCK_SKEW_MS).ok_or(code::BAD_MESSAGE)?;
            profile.sent_at.push(now_ms);
            profile.last_send_ms = now_ms;
            profile.sent_total += 1;
            out.timestamp_window = Some((now_ms, until));
            out.calls.push(Call { to, message: encode(&MsgMessage::Deliver { now_ms, seq, body }), seed: None });
            out.state = Some(encode(&profile));
        }
        MsgMessage::Delete { ns } => {
            check_ns(&ns)?;
            if profile_of(input.pre_state)?.key.is_none() {
                return Err(code::NO_KEY);
            }
            let seed = inbox_seed(&me);
            out.calls.push(Call { to: pda(&input.program, &seed), message: encode(&MsgMessage::Remove { ns }), seed: Some(seed) });
        }
        MsgMessage::Clear { upto } => {
            if profile_of(input.pre_state)?.key.is_none() {
                return Err(code::NO_KEY);
            }
            let seed = inbox_seed(&me);
            let message = encode(&MsgMessage::RemoveUpTo { upto });
            out.calls.push(Call { to: pda(&input.program, &seed), message, seed: Some(seed) });
        }
        MsgMessage::Deliver { now_ms, seq, body } => {
            let sender = match own_sender(input) {
                Some(account) if account != me => account,
                _ => return Err(code::WRONG_SENDER),
            };
            let profile = profile_of(input.pre_state)?;
            let key = profile.key.as_ref().ok_or(code::NO_KEY)?;
            if profile.blocked.contains(&sender) {
                return Err(code::BLOCKED);
            }
            let contact = profile.contacts.contains(&sender);
            if profile.policy == Policy::ContactsOnly && !contact {
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
            let len = body.len() as u32;
            let inbox = inbox_seed(&me);
            let admit = MsgMessage::Admit { from: sender, seq, at_ms: now_ms, len, contact };
            out.calls.push(Call { to: pda(&input.program, &inbox), message: encode(&admit), seed: Some(inbox) });
            let envelope = envelope_seed(&me, &sender, seq);
            let store = MsgMessage::Store { from: sender, seq, at_ms: now_ms, body };
            out.calls.push(Call { to: pda(&input.program, &envelope), message: encode(&store), seed: Some(envelope) });
            // The recipient's profile is read, not written.
        }
        MsgMessage::Open => {
            let owner = inbox_owner(input)?;
            if !input.pre_state.is_empty() {
                return Err(code::BAD_STATE);
            }
            out.state = Some(encode(&Inbox { owner, next_n: 0, entries: Vec::new() }));
        }
        MsgMessage::Admit { from, seq, at_ms, len, contact } => {
            let owner = inbox_owner(input)?;
            let mut inbox = inbox_of(input, &owner)?;
            if inbox.entries.len() >= MAX_MESSAGES || inbox.bytes() + len as usize > MAX_INBOX_BYTES {
                return Err(code::INBOX_FULL);
            }
            if !contact {
                if inbox.entries.iter().filter(|e| !e.contact && e.from == from).count() >= STRANGER_UNREAD {
                    return Err(code::STRANGER_LIMIT);
                }
                if inbox.entries.iter().filter(|e| !e.contact).count() >= STRANGER_SLOTS {
                    return Err(code::STRANGERS_FULL);
                }
            }
            let n = inbox.next_n;
            inbox.entries.push(Entry { n, from, seq, at_ms, len, contact });
            inbox.next_n += 1;
            out.events.push((selector("Delivered"), encode(&Delivered { from, to: owner, n, len })));
            out.state = Some(encode(&inbox));
        }
        MsgMessage::Store { from, seq, at_ms, body } => {
            let recipient = own_sender(input).ok_or(code::WRONG_SENDER)?;
            if envelope_account(&input.program, &recipient, &from, seq) != me {
                return Err(code::WRONG_SENDER);
            }
            if !input.pre_state.is_empty() {
                return Err(code::BAD_STATE);
            }
            out.state = Some(encode(&Envelope { recipient, from, seq, at_ms, body }));
        }
        MsgMessage::Remove { ns } => {
            let owner = inbox_owner(input)?;
            check_ns(&ns)?;
            let mut inbox = inbox_of(input, &owner)?;
            if ns.iter().any(|n| !inbox.entries.iter().any(|e| e.n == *n)) {
                return Err(code::NOT_FOUND);
            }
            let (gone, kept): (Vec<Entry>, Vec<Entry>) = inbox.entries.into_iter().partition(|e| ns.contains(&e.n));
            inbox.entries = kept;
            removed(input, &owner, &inbox, gone, &mut out);
        }
        MsgMessage::RemoveUpTo { upto } => {
            let owner = inbox_owner(input)?;
            let mut inbox = inbox_of(input, &owner)?;
            let (gone, kept): (Vec<Entry>, Vec<Entry>) = inbox.entries.into_iter().partition(|e| e.n <= upto);
            inbox.entries = kept;
            removed(input, &owner, &inbox, gone, &mut out);
        }
        MsgMessage::Erase => {
            let inbox = own_sender(input).ok_or(code::WRONG_SENDER)?;
            if input.pre_state.is_empty() {
                return Err(code::BAD_STATE);
            }
            let envelope: Envelope = decode(input.pre_state)?;
            if inbox_account(&input.program, &envelope.recipient) != inbox
                || envelope_account(&input.program, &envelope.recipient, &envelope.from, envelope.seq) != me
            {
                return Err(code::WRONG_SENDER);
            }
            out.state = Some(Vec::new());
        }
    }
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

    /// A tiny in-memory chain: actor states by account, and the sync calls run depth first as LEZ runs them; a
    /// refusal anywhere discards the whole transaction.
    #[derive(Default, Clone)]
    struct Chain {
        states: std::collections::BTreeMap<Id, Vec<u8>>,
    }

    impl Chain {
        fn run(&mut self, root: Id, signed: bool, m: &MsgMessage) -> Result<Vec<([u8; 8], Vec<u8>)>, u8> {
            let mut next = self.clone();
            let mut events = Vec::new();
            next.deliver(root, None, signed, encode(m), &mut events)?;
            *self = next;
            Ok(events)
        }

        fn deliver(&mut self, to: Id, from: Option<Id>, authorized: bool, message: Vec<u8>, ev: &mut Vec<([u8; 8], Vec<u8>)>) -> Result<(), u8> {
            let pre = self.states.get(&to).cloned().unwrap_or_default();
            let input = Input { receiver: to, program: P, from: from.map(|f| (f, P)), is_authorized: authorized, pre_state: &pre, message: &message };
            let out = handle(&input)?;
            if let Some(s) = out.state {
                self.states.insert(to, s);
            }
            ev.extend(out.events);
            for c in out.calls {
                // A seeded call authorizes the PDA it names; a plain one reaches an account's profile unauthorized.
                let authorized = c.seed.is_some_and(|s| pda(&P, &s) == c.to);
                self.deliver(c.to, Some(to), authorized, c.message, ev)?;
            }
            Ok(())
        }

        fn profile(&self, a: Id) -> Profile {
            self.states.get(&a).map(|s| borsh::from_slice(s).unwrap()).unwrap_or_default()
        }

        fn inbox(&self, a: Id) -> Inbox {
            borsh::from_slice(&self.states[&inbox_account(&P, &a)]).unwrap()
        }

        fn send(&mut self, from: Id, to: Id, now_ms: u64, body: Vec<u8>) -> Result<(), u8> {
            let seq = self.profile(from).sent_total;
            self.run(from, true, &MsgMessage::Send { to, now_ms, seq, body }).map(|_| ())
        }

        fn publish(&mut self, a: Id, epoch: u32, hybrid: bool) -> Result<(), u8> {
            let key = MsgKey { epoch, x25519: [9; 32], mlkem768: hybrid.then(|| vec![7; MLKEM768_PK_LEN]) };
            self.run(a, true, &MsgMessage::Publish { key }).map(|_| ())
        }
    }

    #[test]
    fn delivery_writes_an_envelope_and_an_entry_and_not_the_recipients_profile() {
        let mut c = Chain::default();
        c.publish(B, 0, true).unwrap();
        let before = c.states[&B].clone();
        c.send(A, B, 1000, env(4, 0, 2048)).unwrap();
        assert_eq!(c.states[&B], before, "the recipient's profile is read, not written");
        let inbox = c.inbox(B);
        assert_eq!(inbox.entries, vec![Entry { n: 0, from: A, seq: 0, at_ms: 1000, len: 2048, contact: false }]);
        let e: Envelope = borsh::from_slice(&c.states[&envelope_account(&P, &B, &A, 0)]).unwrap();
        assert_eq!((e.recipient, e.from, e.seq, e.body.len()), (B, A, 0, 2048));
        assert_eq!(c.profile(A).sent_total, 1);
        // Deleting empties the envelope and the entry.
        c.run(B, true, &MsgMessage::Delete { ns: vec![0] }).unwrap();
        assert!(c.inbox(B).entries.is_empty());
        assert!(c.states[&envelope_account(&P, &B, &A, 0)].is_empty());
    }

    #[test]
    fn refusals() {
        let mut c = Chain::default();
        assert_eq!(c.send(A, B, 1, env(3, 0, 512)), Err(code::NO_KEY));
        c.publish(B, 0, true).unwrap();
        assert_eq!(c.send(A, B, 1, env(3, 0, 512)), Err(code::DOWNGRADE));
        assert_eq!(c.send(A, B, 1, env(4, 0, 1024)), Err(code::BAD_SIZE), "a hybrid envelope below 2,048 bytes (L6)");
        assert_eq!(c.send(A, B, 1, env(4, 0, 513)), Err(code::BAD_SIZE));
        assert_eq!(c.send(A, B, 1, env(1, 0, 2048)), Err(code::BAD_VERSION), "v1's versions are not v2's");
        assert_eq!(c.send(A, B, 1, env(4, 1, 2048)), Err(code::STALE_KEY));
        assert_eq!(c.send(A, A, 1, env(4, 0, 2048)), Err(code::SELF_SEND));
        c.publish(C, 0, false).unwrap();
        assert_eq!(c.send(A, C, 1, env(4, 0, 2048)), Err(code::BAD_VERSION));
        let seq = c.profile(A).sent_total;
        assert_eq!(c.run(A, true, &MsgMessage::Send { to: B, now_ms: 1, seq: seq + 1, body: env(4, 0, 2048) }), Err(code::STALE_SEQ));
        assert_eq!(c.run(A, false, &MsgMessage::Send { to: B, now_ms: 1, seq, body: env(4, 0, 2048) }), Err(code::NOT_AUTHORIZED));
        // Internal messages from the transaction itself, or to the wrong receiver.
        for m in [MsgMessage::Open, MsgMessage::Erase, MsgMessage::Deliver { now_ms: 1, seq: 0, body: env(4, 0, 2048) }] {
            assert_eq!(c.run(B, true, &m), Err(code::WRONG_SENDER));
        }
        let store = MsgMessage::Store { from: A, seq: 9, at_ms: 1, body: env(4, 0, 2048) };
        let input = Input { receiver: [0x77; 32], program: P, from: Some((B, P)), is_authorized: true, pre_state: &[], message: &encode(&store) };
        assert_eq!(handle(&input), Err(code::WRONG_SENDER));
        assert_eq!(c.run(A, true, &MsgMessage::Delete { ns: vec![0] }), Err(code::NO_KEY));
        assert_eq!(c.run(B, true, &MsgMessage::Delete { ns: vec![0] }), Err(code::NOT_FOUND));
        assert_eq!(c.run(B, true, &MsgMessage::Delete { ns: vec![2, 1] }), Err(code::BAD_MESSAGE));
        assert_eq!(c.run(B, true, &MsgMessage::Delete { ns: vec![] }), Err(code::BAD_MESSAGE));
    }

    #[test]
    fn stranger_limits_contacts_block_policy() {
        let mut c = Chain::default();
        c.publish(B, 0, false).unwrap();
        c.send(A, B, 1, env(3, 0, 512)).unwrap();
        assert_eq!(c.send(A, B, 2, env(3, 0, 512)), Err(code::STRANGER_LIMIT));
        for i in 0..(STRANGER_SLOTS - 1) as u8 {
            c.send([10 + i; 32], B, 3, env(3, 0, 4096)).unwrap();
        }
        assert_eq!(c.send(C, B, 4, env(3, 0, 512)), Err(code::STRANGERS_FULL));
        c.run(B, true, &MsgMessage::Contacts { add: vec![C], remove: vec![] }).unwrap();
        // Contacts fill the rest: 8 strangers (28.5 KiB) in; contact C until the slots or the bytes run out.
        let mut sent = 0;
        while c.send(C, B, 5 + sent, env(3, 0, 1024)).is_ok() {
            sent += 1;
        }
        assert_eq!(c.send(C, B, 100, env(3, 0, 512)), Err(code::INBOX_FULL));
        assert_eq!(c.inbox(B).entries.len(), MAX_MESSAGES);
        c.run(B, true, &MsgMessage::Clear { upto: 0 }).unwrap();
        c.run(B, true, &MsgMessage::Block { add: vec![C], remove: vec![] }).unwrap();
        assert_eq!(c.send(C, B, 200, env(3, 0, 512)), Err(code::BLOCKED));
        c.run(B, true, &MsgMessage::SetPolicy { policy: Policy::ContactsOnly }).unwrap();
        assert_eq!(c.send([99; 32], B, 300, env(3, 0, 512)), Err(code::NOT_A_CONTACT));
        let many: Vec<Id> = (0..=MAX_LIST as u8).map(|i| [i; 32]).collect();
        assert_eq!(c.run(B, true, &MsgMessage::Contacts { add: many, remove: vec![] }), Err(code::LIST_FULL));
    }

    #[test]
    fn sliding_window_rate_and_clock() {
        let mut c = Chain::default();
        let recipients: Vec<Id> = (0..40u8).map(|i| [100 + i; 32]).collect();
        for r in &recipients {
            c.publish(*r, 0, false).unwrap();
        }
        // 30 sends just before an hour boundary of the first, then the 31st just after it: a fixed window would
        // reset there (L7); the sliding window does not.
        let t0 = 10_000_000;
        for (i, r) in recipients.iter().take(RATE_MAX).enumerate() {
            c.send(A, *r, t0 + RATE_WINDOW_MS - 60_000 + i as u64, env(3, 0, 512)).unwrap();
        }
        assert_eq!(c.send(A, recipients[30], t0 + RATE_WINDOW_MS + 30_000, env(3, 0, 512)), Err(code::RATE_LIMITED));
        assert_eq!(c.send(A, recipients[30], t0, env(3, 0, 512)), Err(code::CLOCK_BACKWARDS));
        // An hour after the first of the 30, one slot is free again.
        let first = t0 + RATE_WINDOW_MS - 60_000;
        assert_eq!(c.send(A, recipients[30], first + RATE_WINDOW_MS - 1, env(3, 0, 512)), Err(code::RATE_LIMITED));
        c.send(A, recipients[30], first + RATE_WINDOW_MS, env(3, 0, 512)).unwrap();
        assert_eq!(c.send(A, recipients[31], first + RATE_WINDOW_MS, env(3, 0, 512)), Err(code::RATE_LIMITED));
    }

    #[test]
    fn rotation_and_keys() {
        let mut c = Chain::default();
        c.publish(B, 0, true).unwrap();
        assert_eq!(c.publish(B, 0, true), Err(code::BAD_KEY));
        let inbox_before = c.states[&inbox_account(&P, &B)].clone();
        c.publish(B, 1, true).unwrap();
        assert_eq!(c.states[&inbox_account(&P, &B)], inbox_before, "a rotation leaves the inbox alone");
        let bad = MsgKey { epoch: 0, x25519: [9; 32], mlkem768: Some(vec![1; 10]) };
        assert_eq!(c.run(A, true, &MsgMessage::Publish { key: bad }), Err(code::BAD_KEY));
        assert_eq!(c.send(A, B, 1, env(4, 0, 2048)), Err(code::STALE_KEY));
        c.send(A, B, 1, env(4, 1, 2048)).unwrap();
    }

    #[test]
    fn selectors_and_seeds_are_distinct() {
        assert_ne!(selector("Delivered"), selector("KeyPublished"));
        assert_ne!(inbox_seed(&A), envelope_seed(&A, &A, 0));
        assert_ne!(envelope_seed(&A, &B, 0), envelope_seed(&B, &A, 0));
    }
}
