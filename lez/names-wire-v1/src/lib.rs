//! `ferrata_names`: human-readable names for LEZ accounts. EXPERIMENTAL, UNAUDITED, for the test network.
//!
//! A name (`alice`) resolves to a record: its owner, an optional public account it points to (the target), an
//! optional private receiver (the nullifier public key and viewing public key a sender needs to pay the name's holder
//! privately, without learning any of the holder's accounts), and an expiry. Reverse resolution (account → name) is
//! the account's own choice of primary name, checked against the forward record by every reader.
//!
//! # Actors (P is the program's account id, wherever it is deployed)
//!
//! * `user(a)` = `Actor(a, P)`, the account's own actor of this program. Every action is a transaction signed by `a`
//!   whose root message goes to `user(a)` (LEZ marks it authorized because `a` signed it); `user(a)` forwards it to the
//!   name's actor, which therefore sees the caller as `from = user(a)`: nobody can act for an account they cannot sign
//!   for. Its state ([`UserState`]) counts the names `a` holds (the squatting limit) and holds `a`'s primary name.
//! * `name(n)` = the public PDA of P with seed `sha256("ferrata-names/v1/name/" || n)` ([`name_seed`]): its state is
//!   the [`NameRecord`], or empty when the name is free. The address commits to the name: a record for `n` can only be
//!   written at `name(n)`.
//!
//! # Rules (all enforced by the program; [`check_name`] and the constants)
//!
//! * A name is 3 to 32 bytes of `a-z`, `0-9` and `-`, neither starting nor ending with `-`, without `--` (no upper
//!   case, no Unicode: no look-alike names), and not one of [`RESERVED`].
//! * First come, first served, by **commit and reveal**; no fee on the test network. An account holds at most
//!   [`MAX_NAMES`] names.
//! * Registration takes two transactions. `Commit` stores at `user(a)` a hash of the name, the account and a 32-byte
//!   salt ([`commitment`]): it reveals nothing about the name. `Register` (the reveal) carries the name and the salt; it
//!   is accepted only from the account that committed, with the same salt, from [`MIN_REVEAL_DELAY_MS`] after the
//!   latest time the commit can have landed (its `now_ms + COMMIT_SLACK_MS`) and before [`MAX_REVEAL_WINDOW_MS`] after
//!   its `now_ms`. Both transactions are valid only in blocks within [`COMMIT_SLACK_MS`] of their `now_ms`. Why this
//!   stops front-running: someone who learns the name from a reveal whose `now_ms` is `n` can commit only in a block
//!   at or after `n`, so their commit's `now_ms` is above `n - COMMIT_SLACK_MS` and their own reveal is valid only from
//!   `n + MIN_REVEAL_DELAY_MS` on; the honest reveal is valid only before `n + COMMIT_SLACK_MS`. With
//!   `MIN_REVEAL_DELAY_MS >= COMMIT_SLACK_MS`, a copier's reveal can never land while the honest one still can: only a
//!   sequencer that withholds the honest reveal for its whole window (censorship) could take the name. A commit holds
//!   nothing: an expired or replaced commit frees nothing and blocks nobody; one pending commit per account, a new
//!   `Commit` replaces it.
//! * A registration lasts [`TERM_MS`] (180 days) from the block it lands in; the owner renews it (to 180 days from the
//!   renewal) at any time until the grace period ([`GRACE_MS`], 30 days) after expiry ends. Past expiry the name no
//!   longer resolves and only `Renew` and `Release` work; past the grace period anyone (the old owner too) may register
//!   it afresh, and it leaves the old owner's count.
//! * Only the owner offers (transfers), updates, renews or releases. A transfer takes two steps: the owner's `Transfer`
//!   offers the name to an account, which takes it with `Accept` (nobody can push a name, or fill another account's
//!   count, without its consent; a mistyped account never gets the name). A newer offer replaces the pending one; a
//!   `Transfer` to the owner withdraws it. An offer does not survive the name's expiry: `Accept` is refused once the
//!   name has expired, and a renewal of an expired name drops the offer. On acceptance the name points at the new owner and the private receiver is
//!   dropped (the old owner's keys must not receive the new owner's payments).
//! * Time: an action that depends on it carries the caller's `now_ms` and sets the transaction's timestamp validity
//!   window to `[now_ms, now_ms + CLOCK_SLACK_MS)`, so LEZ applies it only in a block whose timestamp is in that window:
//!   `now_ms` is the block's time to within [`CLOCK_SLACK_MS`], and never later than it.
//!
//! # What it does not do
//!
//! The owner of a name is a public account (it signs). A name can point at no public account and carry only a private
//! receiver: then senders learn nothing about the holder's accounts, but the record still shows the owning account,
//! which should then be an account used for nothing else. The private receiver's keys are stored as given (their
//! lengths are checked, not their validity).

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ProgramEvent, ReceiveInput, Response},
};
use sha2::{Digest, Sha256};

/// The least and greatest length of a name, in bytes.
pub const MIN_LEN: usize = 3;
pub const MAX_LEN: usize = 32;
/// Names nobody can register on the test network (impersonation of the network's own services).
pub const RESERVED: &[&str] = &[
    "admin", "administrator", "bridge", "explorer", "faucet", "ferrata", "help", "lez", "logos", "null", "official",
    "root", "security", "sequencer", "support", "system", "treasury", "validator", "wallet",
];
/// Names one account may hold at once (expired ones count until they are released or registered by someone else).
pub const MAX_NAMES: u8 = 5;
const DAY_MS: u64 = 86_400_000;
/// How long a registration or renewal lasts.
pub const TERM_MS: u64 = 180 * DAY_MS;
/// After expiry, how long only the owner may still renew it.
pub const GRACE_MS: u64 = 30 * DAY_MS;
/// The width of the timestamp validity window an action sets from its `now_ms`.
pub const CLOCK_SLACK_MS: u64 = 600_000;
/// The validity window of `Commit` and of the reveal (`Register`): tighter than [`CLOCK_SLACK_MS`], since it bounds how
/// long a reveal can be pending (2 minutes: about 60 blocks at the test network's 2-second blocks).
pub const COMMIT_SLACK_MS: u64 = 120_000;
/// The least time from the latest moment a commit can have landed to its reveal: at least [`COMMIT_SLACK_MS`] (the
/// front-running bound above), so a reveal comes about 4 minutes after its commit at the earliest.
pub const MIN_REVEAL_DELAY_MS: u64 = 120_000;
/// How long after its `now_ms` a commit can be revealed (24 hours: enough to come back the next day; past it the
/// commit is void and a fresh one is needed).
pub const MAX_REVEAL_WINDOW_MS: u64 = 86_400_000;
const _: () = assert!(MIN_REVEAL_DELAY_MS >= COMMIT_SLACK_MS, "the front-running bound");
/// The sizes of a private receiver's keys: a nullifier public key and an ML-KEM-768 encapsulation key (LEZ's
/// `NullifierPublicKey` and `ViewingPublicKey`, as LEZ's wallet takes them in `--to-npk` / `--to-vpk`).
pub const NPK_LEN: usize = 32;
pub const VPK_LEN: usize = 1184;

const SEED_DOMAIN: &[u8] = b"ferrata-names/v1/name/";
const COMMIT_DOMAIN: &[u8] = b"ferrata-names/v1/commit/";

/// What `Commit` stores: `sha256("ferrata-names/v1/commit/" || owner || salt || name)`. The owner is the account that
/// will reveal (and own the name): a reveal by any other account, or with another salt or name, does not match.
pub fn commitment(name: &str, owner: AccountId, salt: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(COMMIT_DOMAIN);
    h.update(owner.as_ref());
    h.update(salt);
    h.update(name.as_bytes());
    h.finalize().into()
}

/// Whether `name` is well formed (reserved names are well formed; see [`check_name`]).
pub fn well_formed(name: &str) -> bool {
    let b = name.as_bytes();
    (MIN_LEN..=MAX_LEN).contains(&b.len())
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
        && b[0] != b'-'
        && b[b.len() - 1] != b'-'
        && !b.windows(2).any(|w| w == b"--")
}

/// `Ok` if `name` may be registered: well formed and not reserved.
pub fn check_name(name: &str) -> Result<(), u8> {
    if !well_formed(name) {
        return Err(code::INVALID_NAME);
    }
    if RESERVED.contains(&name) {
        return Err(code::RESERVED_NAME);
    }
    Ok(())
}

pub fn name_seed(name: &str) -> PdaSeed {
    let mut h = Sha256::new();
    h.update(SEED_DOMAIN);
    h.update(name.as_bytes());
    PdaSeed::new(h.finalize().into())
}

/// `name(n)` of the program deployed at `program`.
pub fn name_actor(program: AccountId, name: &str) -> Actor {
    Actor::new(AccountId::for_public_pda(&program, &name_seed(name)), program)
}

/// `user(a)` of the program deployed at `program`.
pub fn user_actor(program: AccountId, account: AccountId) -> Actor {
    Actor::new(account, program)
}

/// What a sender needs to pay the name's holder privately (LEZ's private transfer to a foreign account).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PrivateReceiver {
    pub npk: [u8; NPK_LEN],
    /// [`VPK_LEN`] bytes.
    pub vpk: Vec<u8>,
}

/// The state of `name(n)`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NameRecord {
    pub name: String,
    pub owner: AccountId,
    /// The public account the name resolves to, if any.
    pub target: Option<AccountId>,
    pub receiver: Option<PrivateReceiver>,
    /// When this registration (by this owner) began, and when it expires (block time, ms since the epoch).
    pub registered_ms: u64,
    pub expires_ms: u64,
    /// The account the owner has offered the name to, until it accepts or the owner withdraws.
    pub offer: Option<AccountId>,
}

impl NameRecord {
    /// Whether the name resolves at `now_ms` (before expiry).
    pub fn live_at(&self, now_ms: u64) -> bool {
        now_ms < self.expires_ms
    }
    /// Whether the owner can still renew it at `now_ms` (before the grace period ends).
    pub fn renewable_at(&self, now_ms: u64) -> bool {
        now_ms < self.expires_ms.saturating_add(GRACE_MS)
    }
}

/// The state of `user(a)`.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct UserState {
    /// Names held (live, expired or in grace) until released or registered by another.
    pub owned: u8,
    /// The account's chosen name, for reverse resolution. Readers check it against the forward record: it counts only
    /// while `name(primary)` is live and points at this account.
    pub primary: Option<String>,
    /// The account's pending commitment, if any (one at a time).
    pub commit: Option<PendingCommit>,
}

/// A commitment waiting for its reveal.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PendingCommit {
    pub hash: [u8; 32],
    /// The reveal's `now_ms` must be at least this (`now_ms + COMMIT_SLACK_MS + MIN_REVEAL_DELAY_MS` of the commit)...
    pub not_before_ms: u64,
    /// ... and below this (`now_ms + MAX_REVEAL_WINDOW_MS` of the commit).
    pub expires_ms: u64,
}

/// Every message of the program. `Register` to `SetPrimary`, and `Accept`, are what an account signs (root message at
/// `user(a)`); `user(a)` forwards them to the name's actor. `Credit`, `Debit` and `Primary` are the program's own
/// bookkeeping, accepted only from the actor of the name they carry.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NamesMessage {
    /// The reveal: register a free (or lapsed) name, owned by the signer, pointing at `target`; the signer's pending
    /// commitment must be `commitment(name, signer, salt)`, and `now_ms` within its reveal window.
    Register { name: String, now_ms: u64, target: Option<AccountId>, receiver: Option<PrivateReceiver>, salt: [u8; 32] },
    /// Extend to `now_ms + TERM_MS` (owner; until the grace period ends).
    Renew { name: String, now_ms: u64 },
    /// Offer the name to `to` (owner; before expiry), who takes it with `Accept`; `to` = the owner withdraws the offer.
    Transfer { name: String, to: AccountId, now_ms: u64 },
    /// Change what the name resolves to (owner; before expiry).
    SetTarget { name: String, target: Option<AccountId>, receiver: Option<PrivateReceiver>, now_ms: u64 },
    /// Free the name (owner; any time).
    Release { name: String },
    /// Set the signer's primary name (the name must be live and point at the signer), or clear it (`None`).
    SetPrimary { name: Option<String>, now_ms: u64 },
    /// `name(name)` to `user(a)`: `a` now holds `name`.
    Credit { name: String },
    /// `name(name)` to `user(a)`: `a` no longer holds `name`.
    Debit { name: String },
    /// `name(name)` to `user(a)`: `name` is live and points at `a`; make it `a`'s primary name.
    Primary { name: String },
    /// Take a name offered to the signer (before expiry). It then points at the signer, with no private receiver.
    Accept { name: String, now_ms: u64 },
    /// Commit to registering a name: `commitment` = [`commitment`]`(name, signer, salt)`. Replaces a pending one.
    Commit { commitment: [u8; 32], now_ms: u64 },
}

impl NamesMessage {
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Register { name, .. }
            | Self::Renew { name, .. }
            | Self::Transfer { name, .. }
            | Self::SetTarget { name, .. }
            | Self::Release { name }
            | Self::Accept { name, .. }
            | Self::Credit { name }
            | Self::Debit { name }
            | Self::Primary { name } => Some(name),
            Self::SetPrimary { name, .. } => name.as_deref(),
            Self::Commit { .. } => None,
        }
    }
    fn is_bookkeeping(&self) -> bool {
        matches!(self, Self::Credit { .. } | Self::Debit { .. } | Self::Primary { .. })
    }
}

/// The event every change of a record emits (`ProgramEvent::data`), for explorers and indexers.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NameEvent {
    pub name: String,
    pub owner: Option<AccountId>,
    pub target: Option<AccountId>,
    pub expires_ms: u64,
}

/// The events' kinds; the selector is the first 8 bytes of `sha256("ferrata_names::<kind>")`.
pub const EVENT_KINDS: [&str; 6] = ["Registered", "Renewed", "Offered", "Transferred", "Updated", "Released"];

pub fn event_selector(kind: &str) -> [u8; 8] {
    let mut h = Sha256::new();
    h.update(b"ferrata_names::");
    h.update(kind.as_bytes());
    let d: [u8; 32] = h.finalize().into();
    let mut s = [0; 8];
    s.copy_from_slice(&d[..8]);
    s
}

fn encode<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

fn event(kind: &str, record: Option<&NameRecord>, name: &str) -> ProgramEvent {
    let data = NameEvent {
        name: name.into(),
        owner: record.map(|r| r.owner),
        target: record.and_then(|r| r.target),
        expires_ms: record.map_or(0, |r| r.expires_ms),
    };
    ProgramEvent { selector: event_selector(kind), data: encode(&data) }
}

/// Valid only in a block whose timestamp is in `[now_ms, now_ms + COMMIT_SLACK_MS)` (a commit and a reveal).
fn commit_window(response: Response, now_ms: u64) -> Result<Response, u8> {
    let end = now_ms.checked_add(COMMIT_SLACK_MS).ok_or(code::BAD_MESSAGE)?;
    response.try_timestamp_window(now_ms..end).map_err(|_| code::BAD_MESSAGE)
}

/// Valid only in a block whose timestamp is in `[now_ms, now_ms + CLOCK_SLACK_MS)`.
fn at_time(response: Response, now_ms: u64) -> Result<Response, u8> {
    let end = now_ms.checked_add(CLOCK_SLACK_MS).ok_or(code::BAD_MESSAGE)?;
    response.try_timestamp_window(now_ms..end).map_err(|_| code::BAD_MESSAGE)
}

fn check_receiver(receiver: &Option<PrivateReceiver>) -> Result<(), u8> {
    match receiver {
        Some(r) if r.vpk.len() != VPK_LEN => Err(code::BAD_MESSAGE),
        _ => Ok(()),
    }
}

fn decode_user(state: &[u8]) -> Result<UserState, u8> {
    if state.is_empty() {
        return Ok(UserState::default());
    }
    borsh::from_slice(state).map_err(|_| code::BAD_STATE)
}

fn decode_record(state: &[u8]) -> Result<Option<NameRecord>, u8> {
    if state.is_empty() {
        return Ok(None);
    }
    borsh::from_slice(state).map(Some).map_err(|_| code::BAD_STATE)
}

/// The program: one delivery in, the actor's response or a refusal (exit code) out.
pub fn handle(input: &ReceiveInput) -> Result<Response, u8> {
    let program = input.receiver.program_account_id;
    let message: NamesMessage = borsh::from_slice(&input.message).map_err(|_| code::BAD_MESSAGE)?;
    match input.from {
        None => signed(input, program, message),
        Some(from) if from.program_account_id != program => Err(code::WRONG_SENDER),
        Some(from) if message.is_bookkeeping() => bookkeeping(input, program, from, message),
        Some(from) => at_name(input, program, from.account_id, message),
    }
}

/// At `user(a)`: the transaction's root, signed by `a`.
fn signed(input: &ReceiveInput, program: AccountId, message: NamesMessage) -> Result<Response, u8> {
    if !input.is_authorized {
        return Err(code::NOT_SIGNED);
    }
    if message.is_bookkeeping() {
        return Err(code::WRONG_SENDER);
    }
    let me = input.receiver.account_id;
    if let NamesMessage::Commit { commitment, now_ms } = message {
        let mut user = decode_user(&input.pre_state)?;
        let not_before_ms = now_ms
            .checked_add(COMMIT_SLACK_MS + MIN_REVEAL_DELAY_MS)
            .ok_or(code::BAD_MESSAGE)?;
        let expires_ms = now_ms.checked_add(MAX_REVEAL_WINDOW_MS).ok_or(code::BAD_MESSAGE)?;
        user.commit = Some(PendingCommit { hash: commitment, not_before_ms, expires_ms });
        return commit_window(Response::set_state(encode(&user)), now_ms);
    }
    let Some(name) = message.name() else {
        // `SetPrimary { name: None }`: clear the primary name; nothing else to ask.
        let mut user = decode_user(&input.pre_state)?;
        user.primary = None;
        return Ok(Response::set_state(encode(&user)));
    };
    if let NamesMessage::Register { now_ms, salt, .. } = &message {
        check_name(name)?;
        // The reveal: the signer's own commitment, this name and salt, inside its window; then it is spent.
        let mut user = decode_user(&input.pre_state)?;
        let pending = user.commit.take().ok_or(code::NO_COMMIT)?;
        if pending.hash != commitment(name, me, salt) {
            return Err(code::COMMIT_MISMATCH);
        }
        if *now_ms < pending.not_before_ms {
            return Err(code::TOO_EARLY);
        }
        if *now_ms >= pending.expires_ms {
            return Err(code::COMMIT_EXPIRED);
        }
        let response = commit_window(Response::set_state(encode(&user)), *now_ms)?;
        return Ok(response.send_sync(name_actor(program, name), &message));
    }
    if !well_formed(name) {
        return Err(code::INVALID_NAME);
    }
    Ok(Response::keep_state().send_sync(name_actor(program, name), &message))
}

/// At `name(n)`: a signed action forwarded by `user(caller)`.
fn at_name(input: &ReceiveInput, program: AccountId, caller: AccountId, message: NamesMessage) -> Result<Response, u8> {
    let name = message.name().ok_or(code::BAD_MESSAGE)?;
    if input.receiver != name_actor(program, name) {
        return Err(code::WRONG_RECEIVER);
    }
    let record = decode_record(&input.pre_state)?;
    let owned = |record: Option<NameRecord>| -> Result<NameRecord, u8> {
        let r = record.ok_or(code::NOT_FOUND)?;
        if r.owner != caller {
            return Err(code::NOT_OWNER);
        }
        Ok(r)
    };
    let me = user_actor(program, caller);
    match message {
        NamesMessage::Register { name, now_ms, target, receiver, .. } => {
            check_receiver(&receiver)?;
            let previous = match record {
                Some(r) if r.renewable_at(now_ms) => return Err(code::TAKEN),
                Some(r) => Some(r.owner),
                None => None,
            };
            let expires_ms = now_ms.checked_add(TERM_MS).ok_or(code::BAD_MESSAGE)?;
            let r = NameRecord { name, owner: caller, target, receiver, registered_ms: now_ms, expires_ms, offer: None };
            let mut response = at_time(Response::set_state(encode(&r)), now_ms)?;
            if let Some(previous) = previous {
                // Lapsed: it leaves the old owner's count (and primary name) first.
                response = response.send_sync(user_actor(program, previous), &NamesMessage::Debit { name: r.name.clone() });
            }
            Ok(response
                .send_sync(me, &NamesMessage::Credit { name: r.name.clone() })
                .event(event("Registered", Some(&r), &r.name)))
        }
        NamesMessage::Renew { now_ms, .. } => {
            let mut r = owned(record)?;
            if !r.renewable_at(now_ms) {
                return Err(code::LAPSED);
            }
            if !r.live_at(now_ms) {
                // An offer does not survive the name's expiry.
                r.offer = None;
            }
            r.expires_ms = r.expires_ms.max(now_ms.checked_add(TERM_MS).ok_or(code::BAD_MESSAGE)?);
            let response = at_time(Response::set_state(encode(&r)), now_ms)?;
            Ok(response.event(event("Renewed", Some(&r), &r.name)))
        }
        NamesMessage::Transfer { to, now_ms, .. } => {
            let mut r = owned(record)?;
            if !r.live_at(now_ms) {
                return Err(code::EXPIRED);
            }
            if to == caller {
                // Withdraw the pending offer.
                if r.offer.take().is_none() {
                    return Err(code::SAME_OWNER);
                }
            } else {
                r.offer = Some(to);
            }
            let response = at_time(Response::set_state(encode(&r)), now_ms)?;
            Ok(response.event(event("Offered", Some(&r), &r.name)))
        }
        NamesMessage::Accept { now_ms, .. } => {
            let mut r = record.ok_or(code::NOT_FOUND)?;
            if r.offer != Some(caller) {
                return Err(code::NOT_TARGET);
            }
            if !r.live_at(now_ms) {
                return Err(code::EXPIRED);
            }
            let previous = r.owner;
            r.owner = caller;
            r.target = Some(caller);
            r.receiver = None;
            r.offer = None;
            let response = at_time(Response::set_state(encode(&r)), now_ms)?;
            Ok(response
                .send_sync(user_actor(program, previous), &NamesMessage::Debit { name: r.name.clone() })
                .send_sync(me, &NamesMessage::Credit { name: r.name.clone() })
                .event(event("Transferred", Some(&r), &r.name)))
        }
        NamesMessage::SetTarget { target, receiver, now_ms, .. } => {
            let mut r = owned(record)?;
            if !r.live_at(now_ms) {
                return Err(code::EXPIRED);
            }
            check_receiver(&receiver)?;
            r.target = target;
            r.receiver = receiver;
            let response = at_time(Response::set_state(encode(&r)), now_ms)?;
            Ok(response.event(event("Updated", Some(&r), &r.name)))
        }
        NamesMessage::Release { name } => {
            owned(record)?;
            Ok(Response::set_state(Vec::new())
                .send_sync(me, &NamesMessage::Debit { name: name.clone() })
                .event(event("Released", None, &name)))
        }
        NamesMessage::SetPrimary { name: Some(name), now_ms } => {
            let r = record.ok_or(code::NOT_FOUND)?;
            if r.target != Some(caller) {
                return Err(code::NOT_TARGET);
            }
            if !r.live_at(now_ms) {
                return Err(code::EXPIRED);
            }
            let response = at_time(Response::keep_state(), now_ms)?;
            Ok(response.send_sync(me, &NamesMessage::Primary { name }))
        }
        _ => Err(code::BAD_MESSAGE),
    }
}

/// At `user(a)`: the count and the primary name, from the actor of the name concerned.
fn bookkeeping(input: &ReceiveInput, program: AccountId, from: Actor, message: NamesMessage) -> Result<Response, u8> {
    let name = message.name().ok_or(code::BAD_MESSAGE)?;
    if from != name_actor(program, name) {
        return Err(code::WRONG_SENDER);
    }
    let mut user = decode_user(&input.pre_state)?;
    match message {
        NamesMessage::Credit { .. } => {
            user.owned = user.owned.saturating_add(1);
            if user.owned > MAX_NAMES {
                return Err(code::TOO_MANY);
            }
        }
        NamesMessage::Debit { name } => {
            user.owned = user.owned.saturating_sub(1);
            if user.primary.as_deref() == Some(name.as_str()) {
                user.primary = None;
            }
        }
        NamesMessage::Primary { name } => user.primary = Some(name),
        _ => return Err(code::BAD_MESSAGE),
    }
    Ok(Response::set_state(encode(&user)))
}

/// What a reader concludes from the stored states (the JS resolver does the same).
pub mod read {
    use super::*;

    /// `name`'s record at `now_ms` from `name(name)`'s state, if it resolves (registered and not expired).
    pub fn resolve(state: &[u8], now_ms: u64) -> Option<NameRecord> {
        decode_record(state).ok().flatten().filter(|r| r.live_at(now_ms))
    }

    /// `account`'s name from `user(account)`'s state and the state of `name(primary)`: only if that name is live
    /// and points at `account`.
    pub fn reverse(user_state: &[u8], name_state: &[u8], account: AccountId, now_ms: u64) -> Option<String> {
        let primary = decode_user(user_state).ok()?.primary?;
        let r = resolve(name_state, now_ms)?;
        (r.name == primary && r.target == Some(account)).then_some(primary)
    }
}

/// The program's exit codes: 230–254 (230–233 the commit and reveal). Codes are per program (LEZ reports which program's handler exited); the bridge
/// program uses 240–255 too, so a reader names a code by the program that returned it.
pub const CODE_BLOCK: core::ops::RangeInclusive<u8> = 230..=254;

macro_rules! exit_codes {
    ($($(#[doc = $doc:literal])* $name:ident = $value:literal;)*) => {
        $($(#[doc = $doc])* pub const $name: u8 = $value;)*
        /// Every code of this module, with its name.
        pub const ALL: &[(&str, u8)] = &[$((stringify!($name), $name)),*];
    };
}

/// Exit codes of `ferrata_names`.
pub mod code {
    exit_codes! {
        /// `Register` by an account with no pending commitment (a reveal copied by another account that has none).
        NO_COMMIT = 230;
        /// `Register` that does not match the signer's commitment: another name or salt, or a reveal copied by another
        /// account (the commitment binds the owner).
        COMMIT_MISMATCH = 231;
        /// `Register` before the commitment's reveal window opens.
        TOO_EARLY = 232;
        /// `Register` after the commitment's reveal window closed (the commitment is void).
        COMMIT_EXPIRED = 233;
        /// The message does not decode, a time out of range, or a private receiver's keys of the wrong size.
        BAD_MESSAGE = 240;
        /// Stored state that does not decode.
        BAD_STATE = 241;
        /// A root message to `user(a)` that `a` did not sign.
        NOT_SIGNED = 242;
        /// A message from another program, or bookkeeping from an actor other than the name's.
        WRONG_SENDER = 243;
        /// A forwarded action at an actor other than `name(n)`.
        WRONG_RECEIVER = 244;
        /// A name that breaks the rules (length, characters, hyphens).
        INVALID_NAME = 245;
        /// A reserved name.
        RESERVED_NAME = 246;
        /// `Register` of a name that is held (live, or expired but within the grace period).
        TAKEN = 247;
        /// An action on a name by an account that does not own it.
        NOT_OWNER = 248;
        /// `Transfer`, `Accept`, `SetTarget` or `SetPrimary` of an expired name.
        EXPIRED = 249;
        /// `Renew` after the grace period.
        LAPSED = 250;
        /// The account would hold more than `MAX_NAMES` names.
        TOO_MANY = 251;
        /// An action on a name that is not registered.
        NOT_FOUND = 252;
        /// `Transfer` to the owner with no offer pending to withdraw.
        SAME_OWNER = 253;
        /// `SetPrimary` of a name that does not point at the signer, or `Accept` of a name not offered to it.
        NOT_TARGET = 254;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_rules() {
        for ok in ["abc", "alice", "a-b", "0x1", "123", "a1-b2-c3", "abcdefghijklmnopqrstuvwxyz012345"] {
            assert_eq!(check_name(ok), Ok(()), "{ok}");
        }
        for bad in [
            "", "ab", "Alice", "al ice", "-ab", "ab-", "a--b", "alice.eth", "ålice", "аlice", "abcdefghijklmnopqrstuvwxyz0123456",
            "al_ice", "a\u{0}b",
        ] {
            assert_eq!(check_name(bad), Err(code::INVALID_NAME), "{bad:?}");
        }
        for reserved in RESERVED {
            assert_eq!(check_name(reserved), Err(code::RESERVED_NAME), "{reserved}");
            assert!(well_formed(reserved));
        }
    }

    #[test]
    fn codes_are_distinct_and_in_the_block() {
        for (i, (n, c)) in code::ALL.iter().enumerate() {
            assert!(CODE_BLOCK.contains(c), "{n}");
            assert!(code::ALL[i + 1..].iter().all(|(_, d)| d != c), "{n}");
        }
    }
}
