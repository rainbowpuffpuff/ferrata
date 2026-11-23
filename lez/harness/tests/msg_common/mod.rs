//! Shared by the messages tests (`tests/messages*.rs`): drivers for `ferrata_msg` (v1, live on the test network) and
//! `ferrata_msg_v2` in LEZ's state machine and fee settlement, one signed public transaction per action, each in a block
//! of its own with its own timestamp; the clients' gas limits read from `deploy/testnet/web/msg/client.js`; and the
//! record each test writes.
#![allow(dead_code)]

use std::{borrow::Cow, collections::BTreeMap, fs, path::PathBuf};

use ferrata_lez_msg_v2_wire as v2;
use ferrata_lez_msg_wire as v1;
use harness::{Chain, TxReport};
use lee::{program::Program, Account, AccountId, Actor, PrivateKey, PublicKey, V03State};
use sha2::{Digest, Sha256};

pub const T0: u64 = 1_790_000_000_000;

pub struct User {
    pub key: PrivateKey,
    pub id: AccountId,
}

/// A signing account from a number (its key: SHA-256 of a label and the number).
pub fn user(n: u32) -> User {
    let bytes: [u8; 32] = Sha256::new().chain_update(b"ferrata-msg test user").chain_update(n.to_le_bytes()).finalize().into();
    let key = PrivateKey::try_new(bytes).unwrap();
    let id = AccountId::from(&PublicKey::new_from_private_key(&key));
    User { key, id }
}

/// An id that signs nothing (list entries).
pub fn id(n: u32) -> [u8; 32] {
    Sha256::new().chain_update(b"ferrata-msg test id").chain_update(n.to_le_bytes()).finalize().into()
}

/// An envelope as the program sees it: version, epoch, then filler.
pub fn body(version: u8, epoch: u32, len: usize) -> Vec<u8> {
    let mut b: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
    b[0] = version;
    b[1..5].copy_from_slice(&epoch.to_le_bytes());
    b
}

pub fn outcome(r: &TxReport, name: fn(u8) -> &'static str) -> String {
    match (r.ok(), r.exit_code) {
        (true, _) => "applied".into(),
        (false, Some(c)) => format!("refused {} (0x{c:02X})", name(c as u8)),
        (false, None) => format!("reverted: {}", r.charge.revert.clone().unwrap_or_default()),
    }
}

/// The gas limits a client file declares, `export const NAME = { key: 123_000n, ... };`.
pub fn client_gas(name: &str) -> BTreeMap<String, u64> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../deploy/testnet/web/msg/client.js");
    let text = fs::read_to_string(&path).unwrap();
    let start = text.find(&format!("export const {name} = {{")).unwrap_or_else(|| panic!("{name} not in client.js"));
    let rest = &text[start..];
    let inner = &rest[rest.find('{').unwrap() + 1..rest.find('}').unwrap()];
    inner
        .split(',')
        .filter(|kv| kv.contains(':'))
        .map(|kv| {
            let (k, v) = kv.split_once(':').unwrap();
            (k.trim().to_string(), v.trim().trim_end_matches('n').replace('_', "").parse().unwrap())
        })
        .collect()
}

/// A world of funded accounts and one program.
pub struct World {
    pub chain: Chain,
    pub program: AccountId,
    pub now: u64,
    pub reports: Vec<(String, TxReport)>,
}

impl World {
    pub fn new(users: &[&User], id: [u32; 8], elf: &'static [u8]) -> Self {
        let mut chain = Chain::new();
        let state = std::mem::replace(&mut chain.state, V03State::new());
        let program = Program::new_unchecked(id, Cow::Borrowed(elf));
        chain.state = state
            .with_public_accounts(users.iter().map(|u| (u.id, Account::funded(1 << 60))))
            .with_genesis_programs([(program, true)]);
        Self { chain, program: AccountId::from_builtin_program(id), now: T0, reports: Vec::new() }
    }

    pub fn actor(&self, account: AccountId) -> Actor {
        Actor::new(account, self.program)
    }

    pub fn state(&self, account: AccountId) -> Vec<u8> {
        self.chain.actor_state(self.actor(account)).into_inner()
    }

    /// One transaction at block time `self.now` (then one second later): `message` to `root`'s actor, signed by
    /// `signer` if any, with the declared `actors` (root first).
    pub fn tx(&mut self, label: &str, signer: Option<&User>, root: AccountId, message: Vec<u8>, actors: &[AccountId]) -> TxReport {
        let mut declared = vec![self.actor(root)];
        declared.extend(actors.iter().filter(|a| **a != root).map(|a| self.actor(*a)));
        let signers: Vec<&PrivateKey> = signer.map(|u| &u.key).into_iter().collect();
        let ts = self.now;
        self.now += 1000;
        let r = self.chain.submit_signed(label, self.actor(root), message, declared, vec![], ts, &signers);
        self.reports.push((label.to_string(), r.clone()));
        r
    }

    /// The record: every transaction with its outcome, cycles, bytes and base fee.
    pub fn record(&self, title: &str, preamble: &str, name: fn(u8) -> &'static str) -> String {
        let mut md = format!("# {title}\n\n{preamble}\n\n| # | action | outcome | cycles | tx bytes | base fee |\n|---|---|---|---|---|---|\n");
        for (i, (label, r)) in self.reports.iter().enumerate() {
            md += &format!("| {i} | {label} | {} | {} | {} | {} |\n", outcome(r, name), r.charge.cycles, r.tx_bytes, r.charge.fee_base);
        }
        let ok = self.reports.iter().filter(|(_, r)| r.ok()).count();
        let coded = self.reports.iter().filter(|(_, r)| !r.ok() && r.exit_code.is_some()).count();
        md += &format!(
            "\n{} transactions: {ok} applied, {coded} refused by the program with a code, {} reverted by LEZ (validity window); none of the refused or reverted changed any state.\n",
            self.reports.len(),
            self.reports.len() - ok - coded
        );
        md
    }

    /// Every exit code the transactions reached.
    pub fn codes(&self) -> std::collections::BTreeSet<u8> {
        self.reports.iter().filter_map(|(_, r)| r.exit_code).map(|c| c as u8).collect()
    }
}

// ---------------------------------------------------------------------------------------------------------------
// v1 (`ferrata_msg`, live)
// ---------------------------------------------------------------------------------------------------------------

pub struct V1(pub World);

impl V1 {
    pub fn new(users: &[&User]) -> Self {
        V1(World::new(users, methods_msg::FERRATA_MSG_ID, methods_msg::FERRATA_MSG_ELF))
    }

    pub fn mailbox(&self, a: AccountId) -> v1::Mailbox {
        let s = self.0.state(a);
        if s.is_empty() {
            v1::Mailbox::default()
        } else {
            borsh::from_slice(&s).unwrap()
        }
    }

    pub fn owner(&mut self, label: &str, u: &User, m: v1::MsgMessage) -> TxReport {
        self.0.tx(label, Some(u), u.id, borsh::to_vec(&m).unwrap(), &[])
    }

    pub fn publish(&mut self, label: &str, u: &User, epoch: u32, hybrid: bool) -> TxReport {
        let key = v1::MsgKey { epoch, x25519: [9; 32], mlkem768: hybrid.then(|| vec![7; v1::MLKEM768_PK_LEN]) };
        self.owner(label, u, v1::MsgMessage::Publish { key })
    }

    /// A send of an envelope of `len` bytes, hybrid (version 2) or not (version 1), at the declared time `now_ms`.
    pub fn send_at(&mut self, label: &str, from: &User, to: AccountId, hybrid: bool, epoch: u32, len: usize, now_ms: u64) -> TxReport {
        let m = v1::MsgMessage::Send { to: to.into_value(), now_ms, body: body(if hybrid { 2 } else { 1 }, epoch, len) };
        self.0.tx(label, Some(from), from.id, borsh::to_vec(&m).unwrap(), &[to])
    }

    pub fn send(&mut self, label: &str, from: &User, to: AccountId, hybrid: bool, len: usize) -> TxReport {
        let now = self.0.now;
        self.send_at(label, from, to, hybrid, 0, len, now)
    }
}

// ---------------------------------------------------------------------------------------------------------------
// v2 (`ferrata_msg_v2`)
// ---------------------------------------------------------------------------------------------------------------

pub struct V2(pub World);

impl V2 {
    pub fn new(users: &[&User]) -> Self {
        V2(World::new(users, methods_msg::FERRATA_MSG_V2_ID, methods_msg::FERRATA_MSG_V2_ELF))
    }

    pub fn program_id(&self) -> [u8; 32] {
        self.0.program.into_value()
    }

    pub fn inbox_account(&self, owner: AccountId) -> AccountId {
        AccountId::new(v2::inbox_account(&self.program_id(), owner.value()))
    }

    pub fn envelope_account(&self, recipient: AccountId, sender: AccountId, seq: u64) -> AccountId {
        AccountId::new(v2::envelope_account(&self.program_id(), recipient.value(), sender.value(), seq))
    }

    pub fn profile(&self, a: AccountId) -> v2::Profile {
        let s = self.0.state(a);
        if s.is_empty() {
            v2::Profile::default()
        } else {
            borsh::from_slice(&s).unwrap()
        }
    }

    pub fn inbox(&self, owner: AccountId) -> Option<v2::Inbox> {
        let s = self.0.state(self.inbox_account(owner));
        (!s.is_empty()).then(|| borsh::from_slice(&s).unwrap())
    }

    /// An owner's message, signed, sent straight to its profile, with the actors it reaches declared.
    pub fn owner(&mut self, label: &str, u: &User, m: v2::MsgMessage) -> TxReport {
        let mut actors = Vec::new();
        match &m {
            v2::MsgMessage::Publish { .. } => actors.push(self.inbox_account(u.id)),
            v2::MsgMessage::Delete { ns } => {
                actors.push(self.inbox_account(u.id));
                let entries = self.inbox(u.id).map(|i| i.entries).unwrap_or_default();
                for e in entries.iter().filter(|e| ns.contains(&e.n)) {
                    actors.push(self.envelope_account(u.id, AccountId::new(e.from), e.seq));
                }
            }
            v2::MsgMessage::Clear { upto } => {
                actors.push(self.inbox_account(u.id));
                let entries = self.inbox(u.id).map(|i| i.entries).unwrap_or_default();
                for e in entries.iter().filter(|e| e.n <= *upto) {
                    actors.push(self.envelope_account(u.id, AccountId::new(e.from), e.seq));
                }
            }
            _ => {}
        }
        self.0.tx(label, Some(u), u.id, borsh::to_vec(&m).unwrap(), &actors)
    }

    pub fn publish(&mut self, label: &str, u: &User, epoch: u32, hybrid: bool) -> TxReport {
        let key = v2::MsgKey { epoch, x25519: [9; 32], mlkem768: hybrid.then(|| vec![7; v2::MLKEM768_PK_LEN]) };
        self.owner(label, u, v2::MsgMessage::Publish { key })
    }

    /// The actors a send from `from` to `to` with the sender's counter `seq` reaches.
    pub fn send_actors(&self, from: AccountId, to: AccountId, seq: u64) -> Vec<AccountId> {
        vec![to, self.inbox_account(to), self.envelope_account(to, from, seq)]
    }

    /// A send of `body` at the declared time `now_ms`, with the sender's current counter.
    pub fn send_body_at(&mut self, label: &str, from: &User, to: AccountId, body: Vec<u8>, now_ms: u64) -> TxReport {
        let seq = self.profile(from.id).sent_total;
        let m = v2::MsgMessage::Send { to: to.into_value(), now_ms, seq, body };
        let actors = self.send_actors(from.id, to, seq);
        self.0.tx(label, Some(from), from.id, borsh::to_vec(&m).unwrap(), &actors)
    }

    /// A send of an envelope of `len` bytes, hybrid (version 4) or not (version 3).
    pub fn send_at(&mut self, label: &str, from: &User, to: AccountId, hybrid: bool, epoch: u32, len: usize, now_ms: u64) -> TxReport {
        self.send_body_at(label, from, to, body(if hybrid { 4 } else { 3 }, epoch, len), now_ms)
    }

    pub fn send(&mut self, label: &str, from: &User, to: AccountId, hybrid: bool, len: usize) -> TxReport {
        let now = self.0.now;
        let epoch = self.profile(to).key.map_or(0, |k| k.epoch);
        self.send_at(label, from, to, hybrid, epoch, len, now)
    }
}
