//! Encrypted wallet-to-wallet messages on the test network (`docs/design/messages.md`): the program `ferrata_msg`
//! (guests/msg, its logic in msg-wire) driven through LEZ's state machine and fee settlement, one signed public
//! transaction per action, each in a block of its own with its own timestamp.
//!
//! Delivery (a hybrid envelope, its event, the sender's counters), every spam rule (stranger limits, contacts, block,
//! contacts-only, inbox full, the sender's rate, the clock bound by the transaction's validity window), every code no
//! input is kept from reaching (19 of 20: `BAD_STATE` is defensive), and that no refusal changes any state. Writes
//! `results/messages.md` (each transaction: outcome, cycles, bytes, base fee). v1 is the program live on the test
//! network; `tests/messages_v2.rs` is its successor and `tests/messages_findings.rs` compares the two.
//!
//! `RISC0_DEV_MODE=1 cargo test -p harness --features msg --locked --test messages -- --nocapture`
#![cfg(feature = "msg")]

use std::{borrow::Cow, fs};

use ferrata_lez_msg_wire::{
    code, selector, Delivered, Mailbox, MsgKey, MsgMessage, Policy, CLOCK_SKEW_MS, MAX_MESSAGES, MLKEM768_PK_LEN,
    RATE_MAX, RATE_WINDOW_MS, STRANGER_SLOTS, STRANGER_UNREAD,
};
use harness::{results_dir, Chain, TxReport};

use lee::{program::Program, Account, AccountId, Actor, PrivateKey, PublicKey, V03State};

const T0: u64 = 1_790_000_000_000;

fn program_account() -> AccountId {
    AccountId::from_builtin_program(methods_msg::FERRATA_MSG_ID)
}

struct User {
    key: PrivateKey,
    id: AccountId,
}

fn user(seed: u8) -> User {
    let key = PrivateKey::try_new([seed; 32]).unwrap();
    let id = AccountId::from(&PublicKey::new_from_private_key(&key));
    User { key, id }
}

fn actor(id: AccountId) -> Actor {
    Actor::new(id, program_account())
}

fn envelope(version: u8, epoch: u32, len: usize) -> Vec<u8> {
    let mut b: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
    b[0] = version;
    b[1..5].copy_from_slice(&epoch.to_le_bytes());
    b
}

fn hybrid_key(epoch: u32, seed: u8) -> MsgKey {
    MsgKey { epoch, x25519: [seed; 32], mlkem768: Some(vec![seed; MLKEM768_PK_LEN]) }
}

struct World {
    chain: Chain,
    reports: Vec<(String, TxReport)>,
    now: u64,
}

impl World {
    fn new(users: &[&User]) -> Self {
        let mut chain = Chain::new();
        let state = std::mem::replace(&mut chain.state, V03State::new());
        let program = Program::new_unchecked(methods_msg::FERRATA_MSG_ID, Cow::Borrowed(methods_msg::FERRATA_MSG_ELF));
        chain.state = state
            .with_public_accounts(users.iter().map(|u| (u.id, Account::funded(1 << 60))))
            .with_genesis_programs([(program, true)]);
        Self { chain, reports: Vec::new(), now: T0 }
    }

    fn mailbox(&self, id: AccountId) -> Mailbox {
        let s = self.chain.actor_state(actor(id));
        if s.is_empty() {
            Mailbox::default()
        } else {
            borsh::from_slice(&s).unwrap()
        }
    }

    /// One transaction at block time `self.now` (then one second later): `message` to `root`'s actor, signed by
    /// `signer` if any, with `extra` actors.
    fn tx(&mut self, label: &str, signer: Option<&User>, root: AccountId, message: &MsgMessage, extra: &[AccountId]) -> TxReport {
        let mut actors = vec![actor(root)];
        actors.extend(extra.iter().map(|id| actor(*id)));
        let signers: Vec<&PrivateKey> = signer.map(|u| &u.key).into_iter().collect();
        let ts = self.now;
        self.now += 1000;
        let r = self.chain.submit_signed(label, actor(root), borsh::to_vec(message).unwrap(), actors, vec![], ts, &signers);
        self.reports.push((label.to_string(), r.clone()));
        r
    }

    fn send(&mut self, label: &str, from: &User, to: AccountId, body: Vec<u8>) -> TxReport {
        let m = MsgMessage::Send { to: to.into_value(), now_ms: self.now, body };
        self.tx(label, Some(from), from.id, &m, &[to])
    }

    /// `f` must be refused with `expected` (a coded exit), and change neither `watch` actor.
    fn refused(&mut self, expected: u8, watch: &[AccountId], f: impl FnOnce(&mut Self) -> TxReport) {
        let before: Vec<_> = watch.iter().map(|id| self.chain.actor_state(actor(*id))).collect();
        let r = f(self);
        assert!(!r.ok(), "{}: must be refused", r.label);
        assert_eq!(r.exit_code, Some(u32::from(expected)), "{}: exit {:?}", r.label, r.exit_code);
        let after: Vec<_> = watch.iter().map(|id| self.chain.actor_state(actor(*id))).collect();
        assert_eq!(before, after, "{}: a refusal must change nothing", r.label);
    }
}

fn outcome(r: &TxReport) -> String {
    match (r.ok(), r.exit_code) {
        (true, _) => "stored".into(),
        (false, Some(c)) => format!("refused {} (0x{c:02X})", code::name(c as u8)),
        (false, None) => format!("reverted: {}", r.charge.revert.clone().unwrap_or_default()),
    }
}

#[test]
fn messages() {
    let (alice, bob, carol, dave) = (user(0x41), user(0x42), user(0x43), user(0x44));
    let strangers: Vec<User> = (0..STRANGER_SLOTS as u8).map(|i| user(0x60 + i)).collect();
    let fans: Vec<User> = (0..16u8).map(|i| user(0x80 + i)).collect();
    let mut all = vec![&alice, &bob, &carol, &dave];
    all.extend(strangers.iter());
    all.extend(fans.iter());
    let mut w = World::new(&all);

    // --- Keys ----------------------------------------------------------------------------------------------------
    let r = w.tx("bob publishes a hybrid key (epoch 0)", Some(&bob), bob.id, &MsgMessage::Publish { key: hybrid_key(0, 9) }, &[]);
    assert!(r.ok(), "{}", outcome(&r));
    assert_eq!(w.mailbox(bob.id).key, Some(hybrid_key(0, 9)));
    w.refused(code::BAD_KEY, &[bob.id], |w| {
        w.tx("bob republishes epoch 0 (must move forward)", Some(&bob), bob.id, &MsgMessage::Publish { key: hybrid_key(0, 8) }, &[])
    });
    w.refused(code::NOT_AUTHORIZED, &[bob.id], |w| {
        w.tx("alice publishes into bob's actor (not bob's signature)", Some(&alice), bob.id, &MsgMessage::Publish { key: hybrid_key(5, 1) }, &[])
    });

    // --- Delivery ------------------------------------------------------------------------------------------------
    let body = envelope(2, 0, 2048);
    let r = w.send("alice -> bob, hybrid envelope, 2048 B", &alice, bob.id, body.clone());
    assert!(r.ok(), "{}", outcome(&r));
    let mb = w.mailbox(bob.id);
    assert_eq!(mb.inbox.len(), 1);
    assert_eq!((mb.inbox[0].seq, mb.inbox[0].from, &mb.inbox[0].body), (0, *alice.id.value(), &body));
    let delivered = Delivered { from: *alice.id.value(), to: *bob.id.value(), seq: 0, len: 2048 };
    let ev = r.events.iter().find(|e| e.event.selector == selector("Delivered")).expect("a Delivered event");
    assert_eq!((ev.account_id, &ev.event.data), (program_account(), &borsh::to_vec(&delivered).unwrap()));
    let a = w.mailbox(alice.id);
    assert_eq!((a.sent_total, a.sent_in_window, a.key.is_none()), (1, 1, true));

    // --- Refusals of delivery ------------------------------------------------------------------------------------
    w.refused(code::NO_KEY, &[alice.id, carol.id], |w| w.send("alice -> carol (no key published)", &alice, carol.id, envelope(1, 0, 512)));
    w.refused(code::DOWNGRADE, &[alice.id, bob.id], |w| w.send("alice -> bob, X25519-only envelope (downgrade)", &alice, bob.id, envelope(1, 0, 512)));
    w.refused(code::BAD_SIZE, &[alice.id, bob.id], |w| w.send("alice -> bob, 2000 B (not a bucket)", &alice, bob.id, envelope(2, 0, 2000)));
    w.refused(code::BAD_VERSION, &[alice.id, bob.id], |w| w.send("alice -> bob, version 7", &alice, bob.id, envelope(7, 0, 2048)));
    w.refused(code::STALE_KEY, &[alice.id, bob.id], |w| w.send("alice -> bob, epoch 3 (bob is at 0)", &alice, bob.id, envelope(2, 3, 2048)));
    w.refused(code::SELF_SEND, &[alice.id], |w| w.send("alice -> alice", &alice, alice.id, envelope(2, 0, 2048)));
    w.refused(code::NOT_AUTHORIZED, &[alice.id, bob.id], |w| {
        let m = MsgMessage::Send { to: bob.id.into_value(), now_ms: w.now, body: envelope(2, 0, 2048) };
        w.tx("Send from alice's actor, unsigned", None, alice.id, &m, &[bob.id])
    });
    w.refused(code::WRONG_SENDER, &[bob.id], |w| {
        let m = MsgMessage::Deliver { now_ms: w.now, body: envelope(2, 0, 2048) };
        w.tx("a forged Deliver straight to bob's actor, signed by alice", Some(&alice), bob.id, &m, &[])
    });
    w.refused(code::BAD_MESSAGE, &[bob.id], |w| {
        let mut chain_msg = borsh::to_vec(&MsgMessage::Clear { upto: 0 }).unwrap();
        chain_msg[0] = 42;
        let ts = w.now;
        w.now += 1000;
        let r = w.chain.submit_signed("an unknown message", actor(bob.id), chain_msg, vec![actor(bob.id)], vec![], ts, &[&bob.key]);
        w.reports.push((r.label.clone(), r.clone()));
        r
    });

    // The clock: `now_ms` ahead of the block, or older than the skew, makes the transaction invalid in its block.
    // (dave has never sent, so the program's own monotonic check passes and only LEZ's window refuses.)
    for (label, now_ms) in [("dave -> bob, now_ms 1 min ahead of the block", w.now + 60_000), ("dave -> bob, now_ms 11 min behind the block", w.now - CLOCK_SKEW_MS - 60_000)] {
        let before = (w.chain.actor_state(actor(dave.id)), w.chain.actor_state(actor(bob.id)));
        let m = MsgMessage::Send { to: bob.id.into_value(), now_ms, body: envelope(2, 0, 2048) };
        let r = w.tx(label, Some(&dave), dave.id, &m, &[bob.id]);
        assert!(!r.ok() && r.exit_code.is_none(), "{label}: {}", outcome(&r));
        assert_eq!(before, (w.chain.actor_state(actor(dave.id)), w.chain.actor_state(actor(bob.id))));
    }

    // --- Stranger limits, contacts, block, contacts-only, inbox full, delete and clear ------------------------------
    let r = w.send("alice -> bob, second message", &alice, bob.id, envelope(2, 0, 2048));
    assert!(r.ok(), "{}", outcome(&r));
    assert_eq!(STRANGER_UNREAD, 2);
    w.refused(code::STRANGER_LIMIT, &[alice.id, bob.id], |w| w.send("alice -> bob, third unread from a stranger", &alice, bob.id, envelope(2, 0, 2048)));
    for (i, s) in strangers.iter().take(STRANGER_SLOTS - 2).enumerate() {
        let r = w.send(&format!("stranger {i} -> bob"), s, bob.id, envelope(2, 0, 2048));
        assert!(r.ok(), "{}", outcome(&r));
    }
    let last = &strangers[STRANGER_SLOTS - 1];
    w.refused(code::STRANGERS_FULL, &[last.id, bob.id], |w| w.send("one more stranger -> bob (stranger slots full)", last, bob.id, envelope(2, 0, 2048)));
    let r = w.tx("bob adds carol as a contact", Some(&bob), bob.id, &MsgMessage::Contacts { add: vec![carol.id.into_value()], remove: vec![] }, &[]);
    assert!(r.ok());
    // 8 stranger messages of 2048 B (16 KiB) are in; carol fills the rest of 64 KiB: 24 more of 2048 B = 48 KiB.
    for i in 0..(MAX_MESSAGES - STRANGER_SLOTS) {
        let r = w.send(&format!("carol (contact) -> bob #{i}"), &carol, bob.id, envelope(2, 0, 2048));
        assert!(r.ok(), "carol #{i}: {}", outcome(&r));
    }
    assert_eq!(w.mailbox(bob.id).inbox.len(), MAX_MESSAGES);
    assert_eq!(w.mailbox(bob.id).inbox_bytes(), 64 * 1024);
    w.refused(code::INBOX_FULL, &[carol.id, bob.id], |w| w.send("carol -> bob (inbox full: 32 messages, 64 KiB)", &carol, bob.id, envelope(2, 0, 2048)));
    let r = w.tx("bob clears messages 0..=9", Some(&bob), bob.id, &MsgMessage::Clear { upto: 9 }, &[]);
    assert!(r.ok());
    let r = w.tx("bob deletes messages 10 and 12", Some(&bob), bob.id, &MsgMessage::Delete { seqs: vec![10, 12] }, &[]);
    assert!(r.ok());
    assert_eq!(w.mailbox(bob.id).inbox.len(), MAX_MESSAGES - 12);
    w.refused(code::NOT_FOUND, &[bob.id], |w| w.tx("bob deletes message 10 again", Some(&bob), bob.id, &MsgMessage::Delete { seqs: vec![10] }, &[]));
    w.refused(code::NOT_AUTHORIZED, &[bob.id], |w| w.tx("alice clears bob's inbox", Some(&alice), bob.id, &MsgMessage::Clear { upto: 99 }, &[]));
    let r = w.send("carol -> bob after the clear", &carol, bob.id, envelope(2, 0, 4096));
    assert!(r.ok(), "{}", outcome(&r));
    let r = w.tx("bob blocks carol", Some(&bob), bob.id, &MsgMessage::Block { add: vec![carol.id.into_value()], remove: vec![] }, &[]);
    assert!(r.ok());
    w.refused(code::BLOCKED, &[carol.id, bob.id], |w| w.send("carol (blocked) -> bob", &carol, bob.id, envelope(2, 0, 2048)));
    let r = w.tx("bob: contacts only", Some(&bob), bob.id, &MsgMessage::SetPolicy { policy: Policy::ContactsOnly }, &[]);
    assert!(r.ok());
    w.refused(code::NOT_A_CONTACT, &[dave.id, bob.id], |w| w.send("dave (not a contact) -> bob", &dave, bob.id, envelope(2, 0, 2048)));

    // --- Key rotation: the old epoch is refused, the new one stored ----------------------------------------------------
    let r = w.tx("bob rotates to epoch 1", Some(&bob), bob.id, &MsgMessage::Publish { key: hybrid_key(1, 10) }, &[]);
    assert!(r.ok());
    let r = w.tx("bob: open again", Some(&bob), bob.id, &MsgMessage::SetPolicy { policy: Policy::Open }, &[]);
    assert!(r.ok());
    w.refused(code::STALE_KEY, &[dave.id, bob.id], |w| w.send("dave -> bob, epoch 0 after the rotation", &dave, bob.id, envelope(2, 0, 2048)));
    let r = w.send("dave -> bob, epoch 1", &dave, bob.id, envelope(2, 1, 2048));
    assert!(r.ok(), "{}", outcome(&r));

    // --- The sender's rate: 30 per hour, then refused until the window moves -------------------------------------------
    for f in &fans {
        let r = w.tx("a fan publishes an X25519 key", Some(f), f.id, &MsgMessage::Publish { key: MsgKey { epoch: 0, x25519: [7; 32], mlkem768: None } }, &[]);
        assert!(r.ok());
    }
    let sent = w.mailbox(dave.id).sent_in_window;
    for i in 0..(RATE_MAX - sent) as usize {
        let f = &fans[i / 2];
        let r = w.send(&format!("dave -> fan {} (rate)", i / 2), &dave, f.id, envelope(1, 0, 512));
        assert!(r.ok(), "rate #{i}: {}", outcome(&r));
    }
    let f15 = fans[15].id;
    w.refused(code::RATE_LIMITED, &[dave.id, f15], |w| w.send("dave's 31st send within the hour", &dave, f15, envelope(1, 0, 512)));
    w.now += RATE_WINDOW_MS;
    let r = w.send("dave -> fan 15, an hour later", &dave, f15, envelope(1, 0, 512));
    assert!(r.ok(), "{}", outcome(&r));

    // --- The last two codes (review finding M4: no test reached them on chain) -----------------------------------------
    let last = w.mailbox(dave.id).last_send_ms;
    w.refused(code::CLOCK_BACKWARDS, &[dave.id, f15], |w| {
        let m = MsgMessage::Send { to: f15.into_value(), now_ms: last - 1, body: envelope(1, 0, 512) };
        w.tx("dave declares a time before his last send", Some(&dave), dave.id, &m, &[f15])
    });
    let many: Vec<[u8; 32]> = (0..64u8).map(|i| [0xC0 ^ i; 32]).collect();
    w.refused(code::LIST_FULL, &[bob.id], |w| {
        w.tx("bob adds 64 more contacts (65 > 64)", Some(&bob), bob.id, &MsgMessage::Contacts { add: many, remove: vec![] }, &[])
    });
    // Every code the program can answer, but BAD_STATE (only the program writes the state it decodes).
    let reached: std::collections::BTreeSet<u32> = w.reports.iter().filter_map(|(_, r)| r.exit_code).collect();
    let reachable: std::collections::BTreeSet<u32> =
        code::ALL.iter().map(|(c, _)| u32::from(*c)).filter(|c| *c != u32::from(code::BAD_STATE)).collect();
    assert_eq!(reached, reachable, "every reachable code (19 of 20) on chain");

    // --- The record ------------------------------------------------------------------------------------------------
    let mut md = String::from(
        "# ferrata_msg in LEZ's state machine and fee settlement (harness/tests/messages.rs)\n\n\
         Measured: one signed public transaction per row, each in its own block (RISC0_DEV_MODE executor; cycles as LEZ \
         meters them). Generated by `cargo test -p harness --features msg --test messages`.\n\n\
         | # | action | outcome | cycles | tx bytes | base fee |\n|---|---|---|---|---|---|\n",
    );
    md = md.replacen(
        "\n\n|",
        &format!("\n\nProgram `ferrata_msg` image id `{}`.\n\n|", harness::image_id_hex(methods_msg::FERRATA_MSG_ID)),
        1,
    );
    for (i, (label, r)) in w.reports.iter().enumerate() {
        md += &format!("| {i} | {label} | {} | {} | {} | {} |\n", outcome(r), r.charge.cycles, r.tx_bytes, r.charge.fee_base);
    }
    let ok = w.reports.iter().filter(|(_, r)| r.ok()).count();
    md += &format!("\n{} transactions, {ok} applied, {} refused or reverted with no state change.\n", w.reports.len(), w.reports.len() - ok);
    let names: Vec<&str> = reached.iter().map(|c| code::name(*c as u8)).collect();
    md += &format!("Codes reached: {} of {} ({}; `BAD_STATE` is defensive).\n", reached.len(), code::ALL.len(), names.join(", "));
    println!("{md}");
    fs::write(results_dir().join("messages.md"), md).unwrap();
}
