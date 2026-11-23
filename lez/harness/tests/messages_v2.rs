//! `ferrata_msg_v2` (guests/msg-v2, its logic in msg-v2-wire; `docs/design/messages.md`) driven through LEZ's state
//! machine and fee settlement, one signed public transaction per action, each in a block of its own.
//!
//! Delivery (the envelope account written once, the inbox entry, the event, the recipient's profile untouched),
//! deletion (the envelope's state emptied, its account kept for good: review N4), every spam rule, the clock bound by
//! the transaction's validity window, and every exit code (21 of 21; `BAD_STATE` by a send addressed to one of the
//! program's own inbox or envelope accounts: review N3), each refusal leaving every actor it names unchanged. Then the
//! worst cases of each action (full lists, keys, sending records and inboxes; list edits of every size up to 64 ids
//! removed and 64 added), which must fit the gas limits the clients use (`GAS_V2` in `deploy/testnet/web/msg/client.js`,
//! read here: an edit's limit grows with the ids it names): review findings M2 and N2.
//! Writes `results/messages-v2.md`.
//!
//! `RISC0_DEV_MODE=1 cargo test -p harness --features msg --locked --test messages_v2 -- --nocapture`
#![cfg(feature = "msg")]

mod msg_common;

use std::{collections::BTreeSet, fs};

use ferrata_lez_msg_v2_wire::{
    code, envelope_seed, inbox_seed, selector, Delivered, Envelope, MsgKey, MsgMessage as M, Policy, CLOCK_SKEW_MS,
    MAX_LIST, MAX_MESSAGES, MLKEM768_PK_LEN, RATE_MAX, RATE_WINDOW_MS, STRANGER_SLOTS,
};
use harness::{results_dir, TxReport};
use lee::AccountId;
use lee_core::program::PdaSeed;
use msg_common::{body, client_gas, id, user, User, V2};

/// What a transaction did, for the gas check.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Publish,
    /// A policy, contacts or block edit naming this many ids.
    Edit(usize),
    Send,
    /// A `Delete` or `Clear` that empties this many envelopes.
    Remove(usize),
}

struct Run {
    w: V2,
    kinds: Vec<(usize, Kind)>,
}

impl Run {
    fn note(&mut self, kind: Kind) {
        self.kinds.push((self.w.0.reports.len() - 1, kind));
    }

    fn owner(&mut self, label: &str, u: &User, m: M) -> TxReport {
        let kind = match &m {
            M::Publish { .. } => Kind::Publish,
            M::Delete { ns } => Kind::Remove(ns.len()),
            M::Clear { upto } => Kind::Remove(self.w.inbox(u.id).map_or(0, |i| i.entries.iter().filter(|e| e.n <= *upto).count())),
            M::Contacts { add, remove } | M::Block { add, remove } => Kind::Edit(add.len() + remove.len()),
            _ => Kind::Edit(0),
        };
        let r = self.w.owner(label, u, m);
        self.note(kind);
        r
    }

    fn send(&mut self, label: &str, from: &User, to: AccountId, hybrid: bool, len: usize) -> TxReport {
        let r = self.w.send(label, from, to, hybrid, len);
        self.note(Kind::Send);
        r
    }

    fn send_body(&mut self, label: &str, from: &User, to: AccountId, b: Vec<u8>) -> TxReport {
        let now = self.w.0.now;
        let r = self.w.send_body_at(label, from, to, b, now);
        self.note(Kind::Send);
        r
    }

    fn ok(&self, r: &TxReport) {
        assert!(r.ok(), "{}: {}", r.label, msg_common::outcome(r, code::name));
    }

    /// The state of `who`'s profile and inbox, and of the envelope `env` names, before and after `f`: `f` must be
    /// refused with `expected` and change none of them.
    fn refused(&mut self, expected: u8, who: &[AccountId], f: impl FnOnce(&mut Self) -> TxReport) {
        let snap = |w: &V2| -> Vec<Vec<u8>> {
            who.iter().flat_map(|a| [w.0.state(*a), w.0.state(w.inbox_account(*a))]).collect()
        };
        let before = snap(&self.w);
        let r = f(self);
        assert!(!r.ok(), "{}: must be refused", r.label);
        assert_eq!(r.exit_code, Some(u32::from(expected)), "{}: {}", r.label, msg_common::outcome(&r, code::name));
        assert_eq!(before, snap(&self.w), "{}: a refusal must change nothing", r.label);
    }
}

fn hybrid_key(epoch: u32, seed: u8) -> MsgKey {
    MsgKey { epoch, x25519: [seed; 32], mlkem768: Some(vec![seed; MLKEM768_PK_LEN]) }
}

fn ids(range: std::ops::Range<u32>) -> Vec<[u8; 32]> {
    range.map(id).collect()
}

#[test]
fn messages_v2() {
    let alice = user(1);
    let bob = user(2);
    let carol = user(3);
    let dave = user(4);
    let strangers: Vec<User> = (0..STRANGER_SLOTS as u32 + 1).map(|i| user(100 + i)).collect();
    let fans: Vec<User> = (0..4).map(|i| user(200 + i)).collect();
    let heavy = user(5); // the worst cases: a sender with every list full
    let target = user(6); // and a recipient with every list full and an inbox one envelope from full
    let contacts: Vec<User> = (0..4).map(|i| user(300 + i)).collect();
    let late = user(7); // publishes only after filling its lists
    let mut all: Vec<&User> = vec![&alice, &bob, &carol, &dave, &heavy, &target, &late];
    all.extend(strangers.iter());
    all.extend(fans.iter());
    all.extend(contacts.iter());
    let mut run = Run { w: V2::new(&all), kinds: Vec::new() };

    // --- The PDAs are LEZ's -------------------------------------------------------------------------------------------
    let lez_pda = |seed: [u8; 32]| AccountId::for_public_pda(&run.w.0.program, &PdaSeed::new(seed));
    assert_eq!(run.w.inbox_account(bob.id), lez_pda(inbox_seed(bob.id.value())));
    assert_eq!(run.w.envelope_account(bob.id, alice.id, 7), lez_pda(envelope_seed(bob.id.value(), alice.id.value(), 7)));

    // --- Keys --------------------------------------------------------------------------------------------------------
    let r = run.owner("bob publishes a hybrid key (epoch 0): his inbox opens", &bob, M::Publish { key: hybrid_key(0, 9) });
    run.ok(&r);
    assert_eq!(run.w.profile(bob.id).key, Some(hybrid_key(0, 9)));
    assert_eq!(run.w.inbox(bob.id).map(|i| (i.owner, i.next_n, i.entries.len())), Some((*bob.id.value(), 0, 0)));
    run.refused(code::BAD_KEY, &[bob.id], |r| r.owner("bob republishes epoch 0 (must move forward)", &bob, M::Publish { key: hybrid_key(0, 8) }));
    let short = MsgKey { epoch: 5, x25519: [3; 32], mlkem768: Some(vec![3; 100]) };
    run.refused(code::BAD_KEY, &[bob.id], |r| r.owner("bob publishes a 100-byte ML-KEM key", &bob, M::Publish { key: short }));
    run.refused(code::NOT_AUTHORIZED, &[bob.id], |r| {
        let m = borsh::to_vec(&M::Publish { key: hybrid_key(5, 1) }).unwrap();
        let actors = [r.w.inbox_account(bob.id)];
        let x = r.w.0.tx("alice signs a Publish sent to bob's profile", Some(&alice), bob.id, m, &actors);
        r.note(Kind::Publish);
        x
    });

    // --- Delivery ----------------------------------------------------------------------------------------------------
    let bob_profile = run.w.0.state(bob.id);
    let r = run.send("alice -> bob, hybrid envelope, 2048 B", &alice, bob.id, true, 2048);
    run.ok(&r);
    assert_eq!(run.w.0.state(bob.id), bob_profile, "delivery reads the recipient's profile and writes nothing there");
    let inbox = run.w.inbox(bob.id).unwrap();
    assert_eq!(inbox.entries.len(), 1);
    let e = &inbox.entries[0];
    assert_eq!((e.n, e.from, e.seq, e.len, e.contact), (0, *alice.id.value(), 0, 2048, false));
    let stored: Envelope = borsh::from_slice(&run.w.0.state(run.w.envelope_account(bob.id, alice.id, 0))).unwrap();
    assert_eq!((stored.recipient, stored.from, stored.seq, stored.body), (*bob.id.value(), *alice.id.value(), 0, body(4, 0, 2048)));
    let ev = r.events.iter().find(|e| e.event.selector == selector("Delivered")).expect("a Delivered event");
    let delivered = Delivered { from: *alice.id.value(), to: *bob.id.value(), n: 0, len: 2048 };
    assert_eq!((ev.account_id, ev.event.data.clone()), (run.w.0.program, borsh::to_vec(&delivered).unwrap()));
    assert_eq!(run.w.profile(alice.id).sent_total, 1);
    assert!(run.w.inbox(alice.id).is_none(), "a sender needs no key and no inbox");

    // --- Refusals of a send ------------------------------------------------------------------------------------------
    run.refused(code::NO_KEY, &[alice.id, carol.id], |r| r.send("alice -> carol (no key published)", &alice, carol.id, true, 2048));
    run.refused(code::DOWNGRADE, &[alice.id, bob.id], |r| r.send("alice -> bob, X25519-only envelope (downgrade)", &alice, bob.id, false, 512));
    run.refused(code::BAD_SIZE, &[alice.id, bob.id], |r| r.send_body("alice -> bob, 2000 B (not a bucket)", &alice, bob.id, body(4, 0, 2000)));
    run.refused(code::BAD_SIZE, &[alice.id, bob.id], |r| r.send_body("alice -> bob, hybrid in 1024 B (below 2048: L6)", &alice, bob.id, body(4, 0, 1024)));
    run.refused(code::BAD_VERSION, &[alice.id, bob.id], |r| r.send_body("alice -> bob, version 7", &alice, bob.id, body(7, 0, 2048)));
    run.refused(code::BAD_VERSION, &[alice.id, bob.id], |r| r.send_body("alice -> bob, v1's hybrid version 2", &alice, bob.id, body(2, 0, 2048)));
    run.refused(code::STALE_KEY, &[alice.id, bob.id], |r| r.send_body("alice -> bob, epoch 3 (bob is at 0)", &alice, bob.id, body(4, 3, 2048)));
    run.refused(code::SELF_SEND, &[alice.id], |r| r.send("alice -> alice", &alice, alice.id, true, 2048));
    run.refused(code::STALE_SEQ, &[alice.id, bob.id], |r| {
        let now = r.w.0.now;
        let m = M::Send { to: bob.id.into_value(), now_ms: now, seq: 0, body: body(4, 0, 2048) };
        let actors = r.w.send_actors(alice.id, bob.id, 0);
        let x = r.w.0.tx("alice -> bob with seq 0 again (her counter is 1)", Some(&alice), alice.id, borsh::to_vec(&m).unwrap(), &actors);
        r.note(Kind::Send);
        x
    });
    run.refused(code::NOT_AUTHORIZED, &[alice.id, bob.id], |r| {
        let now = r.w.0.now;
        let m = M::Send { to: bob.id.into_value(), now_ms: now, seq: 1, body: body(4, 0, 2048) };
        let actors = r.w.send_actors(alice.id, bob.id, 1);
        let x = r.w.0.tx("Send at alice's profile, unsigned", None, alice.id, borsh::to_vec(&m).unwrap(), &actors);
        r.note(Kind::Send);
        x
    });
    run.refused(code::WRONG_SENDER, &[bob.id], |r| {
        let now = r.w.0.now;
        let m = M::Deliver { now_ms: now, seq: 9, body: body(4, 0, 2048) };
        let actors = [r.w.inbox_account(bob.id), r.w.envelope_account(bob.id, alice.id, 9)];
        let x = r.w.0.tx("a forged Deliver straight to bob's profile, signed by alice", Some(&alice), bob.id, borsh::to_vec(&m).unwrap(), &actors);
        r.note(Kind::Edit(0));
        x
    });
    run.refused(code::WRONG_SENDER, &[bob.id], |r| {
        let m = M::Admit { from: *alice.id.value(), seq: 9, at_ms: 1, len: 2048, contact: true };
        let inbox = r.w.inbox_account(bob.id);
        let x = r.w.0.tx("a forged Admit straight to bob's inbox, signed by bob", Some(&bob), inbox, borsh::to_vec(&m).unwrap(), &[]);
        r.note(Kind::Edit(0));
        x
    });
    // A send addressed to one of the program's own accounts: its state is not a profile (review N3).
    let bob_inbox = run.w.inbox_account(bob.id);
    let alice_env = run.w.envelope_account(bob.id, alice.id, 0);
    for (label, to) in [("alice -> bob's inbox account (a PDA of the program)", bob_inbox), ("alice -> her envelope account at bob (a PDA)", alice_env)] {
        run.refused(code::BAD_STATE, &[alice.id, bob.id], |r| {
            let now = r.w.0.now;
            let x = r.w.send_body_at(label, &alice, to, body(4, 0, 2048), now);
            r.note(Kind::Send);
            x
        });
    }
    run.refused(code::BAD_MESSAGE, &[bob.id], |r| {
        let x = r.w.0.tx("an unknown message (variant 42)", Some(&bob), bob.id, vec![42, 0, 0, 0], &[]);
        r.note(Kind::Edit(0));
        x
    });

    // The clock: `now_ms` ahead of the block, or older than the skew, makes the transaction invalid in its block.
    // (dave has never sent, so the program's monotonic check passes and only LEZ's window refuses.)
    for (label, ahead) in [("dave -> bob, now_ms 1 min ahead of the block", true), ("dave -> bob, now_ms 11 min behind the block", false)] {
        let before = (run.w.0.state(dave.id), run.w.inbox(bob.id));
        let now = run.w.0.now;
        let now_ms = if ahead { now + 60_000 } else { now - CLOCK_SKEW_MS - 60_000 };
        let r = run.w.send_at(label, &dave, bob.id, true, 0, 2048, now_ms);
        run.note(Kind::Send);
        assert!(!r.ok() && r.exit_code.is_none(), "{label}: {}", msg_common::outcome(&r, code::name));
        assert_eq!(before, (run.w.0.state(dave.id), run.w.inbox(bob.id)));
    }

    // --- Strangers, contacts, block, contacts-only, inbox full, delete and clear -----------------------------------------
    run.refused(code::STRANGER_LIMIT, &[alice.id, bob.id], |r| r.send("alice -> bob, a second unread from a stranger", &alice, bob.id, true, 2048));
    for (i, s) in strangers.iter().take(STRANGER_SLOTS - 1).enumerate() {
        let r = run.send(&format!("stranger {i} -> bob, 2048 B"), s, bob.id, true, 2048);
        run.ok(&r);
    }
    let last = &strangers[STRANGER_SLOTS - 1];
    run.refused(code::STRANGERS_FULL, &[last.id, bob.id], |r| r.send("a ninth stranger -> bob (stranger slots full)", last, bob.id, true, 2048));
    let mut add = vec![carol.id.into_value()];
    add.extend(contacts.iter().map(|u| u.id.into_value()));
    let r = run.owner("bob adds carol and the four contacts", &bob, M::Contacts { add, remove: vec![] });
    run.ok(&r);
    // 8 stranger envelopes (16 KiB) are in; contacts fill the rest: 24 more of 2048 B reach 32 messages and 64 KiB.
    let mut i = 0;
    while run.w.inbox(bob.id).unwrap().entries.len() < MAX_MESSAGES {
        let c = &contacts[i % contacts.len()];
        let r = run.send(&format!("contact {} -> bob #{i}", i % contacts.len()), c, bob.id, true, 2048);
        run.ok(&r);
        i += 1;
    }
    assert_eq!(run.w.inbox(bob.id).unwrap().bytes(), 64 * 1024);
    run.refused(code::INBOX_FULL, &[carol.id, bob.id], |r| r.send("carol -> bob (inbox full: 32 messages, 64 KiB)", &carol, bob.id, true, 2048));
    let r = run.owner("bob deletes messages 1 and 3", &bob, M::Delete { ns: vec![1, 3] });
    run.ok(&r);
    for n in [1u64, 3] {
        let e = run.w.inbox(bob.id).unwrap();
        assert!(!e.entries.iter().any(|x| x.n == n));
    }
    let env1 = run.w.envelope_account(bob.id, strangers[0].id, 0);
    assert!(run.w.0.state(env1).is_empty(), "message 1's envelope is emptied");
    assert!(run.w.0.chain.state.get_account_by_id_ref(env1).is_some(), "its account stays in the state for good (review N4)");
    run.refused(code::NOT_FOUND, &[bob.id], |r| r.owner("bob deletes message 3 again", &bob, M::Delete { ns: vec![3] }));
    run.refused(code::BAD_MESSAGE, &[bob.id], |r| r.owner("bob deletes [5, 4] (not ascending)", &bob, M::Delete { ns: vec![5, 4] }));
    run.refused(code::NO_KEY, &[carol.id], |r| r.owner("carol deletes message 0 (no inbox)", &carol, M::Delete { ns: vec![0] }));
    run.refused(code::NOT_AUTHORIZED, &[bob.id], |r| {
        let m = borsh::to_vec(&M::Clear { upto: 99 }).unwrap();
        let actors = [r.w.inbox_account(bob.id)];
        let x = r.w.0.tx("alice signs a Clear sent to bob's profile", Some(&alice), bob.id, m, &actors);
        r.note(Kind::Remove(0));
        x
    });
    let r = run.owner("bob clears up to message 9", &bob, M::Clear { upto: 9 });
    run.ok(&r);
    let r = run.send("carol (contact) -> bob, 4096 B after the clear", &carol, bob.id, true, 4096);
    run.ok(&r);
    let r = run.owner("bob blocks carol", &bob, M::Block { add: vec![carol.id.into_value()], remove: vec![] });
    run.ok(&r);
    run.refused(code::BLOCKED, &[carol.id, bob.id], |r| r.send("carol (blocked) -> bob", &carol, bob.id, true, 2048));
    let r = run.owner("bob: contacts only", &bob, M::SetPolicy { policy: Policy::ContactsOnly });
    run.ok(&r);
    run.refused(code::NOT_A_CONTACT, &[dave.id, bob.id], |r| r.send("dave (not a contact) -> bob", &dave, bob.id, true, 2048));
    let r = run.send("contact 0 -> bob under contacts-only", &contacts[0], bob.id, true, 2048);
    run.ok(&r);
    run.refused(code::LIST_FULL, &[bob.id], |r| r.owner("bob adds 64 more contacts (65 + carol's 5 > 64)", &bob, M::Contacts { add: ids(1000..1000 + MAX_LIST as u32), remove: vec![] }));

    // --- Key rotation: the old epoch is refused, the new one stored ------------------------------------------------------
    let r = run.owner("bob rotates to epoch 1 (his inbox is left alone)", &bob, M::Publish { key: hybrid_key(1, 10) });
    run.ok(&r);
    let r = run.owner("bob: open again", &bob, M::SetPolicy { policy: Policy::Open });
    run.ok(&r);
    run.refused(code::STALE_KEY, &[dave.id, bob.id], |r| r.send_body("dave -> bob, epoch 0 after the rotation", &dave, bob.id, body(4, 0, 2048)));
    let r = run.send("dave -> bob, epoch 1", &dave, bob.id, true, 2048);
    run.ok(&r);
    let r = run.owner("fan 0 publishes an X25519-only key", &fans[0], M::Publish { key: MsgKey { epoch: 0, x25519: [7; 32], mlkem768: None } });
    run.ok(&r);
    run.refused(code::BAD_VERSION, &[alice.id, fans[0].id], |r| r.send("alice -> fan 0, hybrid to an X25519-only key", &alice, fans[0].id, true, 2048));

    // --- The sender's rate: a sliding hour, across what a fixed window would reset (L7) -----------------------------------
    // dave sends to fans 1-3, who make him a contact (32 slots each); the window holds his declared times.
    for f in &fans[1..] {
        let r = run.owner("a fan publishes an X25519 key and adds dave", f, M::Publish { key: MsgKey { epoch: 0, x25519: [7; 32], mlkem768: None } });
        run.ok(&r);
        let r = run.owner("the fan adds dave as a contact", f, M::Contacts { add: vec![dave.id.into_value()], remove: vec![] });
        run.ok(&r);
    }
    let mut sent = run.w.profile(dave.id).sent_at.len();
    let mut k = 0;
    while sent < RATE_MAX {
        let f = &fans[1 + k % 3];
        let r = run.send(&format!("dave -> fan {} (rate)", 1 + k % 3), &dave, f.id, false, 512);
        run.ok(&r);
        sent += 1;
        k += 1;
    }
    run.refused(code::RATE_LIMITED, &[dave.id, fans[1].id], |r| r.send("dave's 31st send within an hour", &dave, fans[1].id, false, 512));
    let oldest = *run.w.profile(dave.id).sent_at.first().unwrap();
    run.w.0.now = oldest + RATE_WINDOW_MS - 30_000;
    run.refused(code::RATE_LIMITED, &[dave.id, fans[1].id], |r| r.send("dave, 30 s before his oldest send is an hour old", &dave, fans[1].id, false, 512));
    run.w.0.now = oldest + RATE_WINDOW_MS;
    let r = run.send("dave, when his oldest send is an hour old: one slot free", &dave, fans[2].id, false, 512);
    run.ok(&r);
    run.refused(code::RATE_LIMITED, &[dave.id, fans[3].id], |r| r.send("dave again at once", &dave, fans[3].id, false, 512));
    let last_ms = run.w.profile(dave.id).last_send_ms;
    run.refused(code::CLOCK_BACKWARDS, &[dave.id, fans[3].id], |r| {
        let x = r.w.send_at("dave declares a time before his last send", &dave, fans[3].id, false, 0, 512, last_ms - 1);
        r.note(Kind::Send);
        x
    });

    // --- Worst cases for the gas limits (M2) ------------------------------------------------------------------------------
    // heavy: hybrid key, 64 contacts, 64 blocked, 29 sends in the window; target: the same lists, an inbox at 30 x 2048 B.
    let full_lists = |r: &mut Run, u: &User, base: u32| {
        let a = r.owner("full lists: publish", u, M::Publish { key: hybrid_key(0, 4) });
        r.ok(&a);
        let mut add: Vec<[u8; 32]> = ids(base..base + MAX_LIST as u32 - contacts.len() as u32 - 1);
        add.extend(contacts.iter().map(|c| c.id.into_value()));
        add.push(heavy.id.into_value());
        add.retain(|x| *x != u.id.into_value());
        while add.len() < MAX_LIST {
            add.push(id(base + 500 + add.len() as u32));
        }
        let a = r.owner("full lists: 64 contacts", u, M::Contacts { add, remove: vec![] });
        r.ok(&a);
        let a = r.owner("full lists: 64 blocked", u, M::Block { add: ids(base + 1000..base + 1000 + MAX_LIST as u32), remove: vec![] });
        r.ok(&a);
    };
    full_lists(&mut run, &heavy, 10_000);
    full_lists(&mut run, &target, 20_000);
    let sink = &contacts[1];
    let r = run.owner("contact 1 publishes", sink, M::Publish { key: hybrid_key(0, 6) });
    run.ok(&r);
    let r = run.owner("contact 1 adds heavy", sink, M::Contacts { add: vec![heavy.id.into_value()], remove: vec![] });
    run.ok(&r);
    run.w.0.now += RATE_WINDOW_MS;
    for i in 0..RATE_MAX - 1 {
        let r = run.send(&format!("heavy -> contact 1 #{i} (filling heavy's sending record)"), &heavy, sink.id, true, 2048);
        run.ok(&r);
    }
    let mut i = 0;
    while run.w.inbox(target.id).unwrap().bytes() + 2048 <= 64 * 1024 - 4096 {
        let c = &contacts[i % contacts.len()];
        let r = run.send(&format!("contact {} -> target #{i} (filling target's inbox)", i % contacts.len()), c, target.id, true, 2048);
        run.ok(&r);
        i += 1;
    }
    let r = run.send("WORST send: heavy (full lists, 29 recent sends) -> target (full lists, 30 x 2048 B), 4096 B", &heavy, target.id, true, 4096);
    run.ok(&r);
    assert_eq!(run.w.inbox(target.id).unwrap().bytes(), 64 * 1024);
    let r = run.owner("WORST edit: heavy replaces a contact (64 contacts, 64 blocked, 30 sends)", &heavy, M::Contacts { add: vec![id(99_999)], remove: vec![id(10_000)] });
    run.ok(&r);
    let r = run.owner("WORST publish: heavy rotates (full lists)", &heavy, M::Publish { key: hybrid_key(1, 4) });
    run.ok(&r);
    // List edits of growing size on full lists (review N2): k blocked replaced; all 64 replaced in either list; 64
    // absent ids removed while the 64 present are re-added (no change, the most comparisons).
    for k in [8usize, 32] {
        let remove: Vec<[u8; 32]> = run.w.profile(heavy.id).blocked[..k].to_vec();
        let r = run.owner(&format!("heavy replaces {k} blocked"), &heavy, M::Block { add: ids(40_000 + 100 * k as u32..40_000 + 100 * k as u32 + k as u32), remove });
        run.ok(&r);
    }
    let remove = run.w.profile(heavy.id).blocked.clone();
    let r = run.owner("WORST edit: heavy replaces all 64 blocked (remove 64, add 64 others)", &heavy, M::Block { add: ids(41_000..41_064), remove });
    run.ok(&r);
    let remove = run.w.profile(heavy.id).contacts.clone();
    let r = run.owner("heavy replaces all 64 contacts (remove 64, add 64 others)", &heavy, M::Contacts { add: ids(42_000..42_064), remove });
    run.ok(&r);
    let add = run.w.profile(heavy.id).blocked.clone();
    let r = run.owner("heavy removes 64 absent ids and re-adds its 64 blocked", &heavy, M::Block { add, remove: ids(43_000..43_064) });
    run.ok(&r);
    assert_eq!((run.w.profile(heavy.id).contacts.len(), run.w.profile(heavy.id).blocked.len()), (64, 64));
    // Ids that share 31 bytes: every list comparison runs to the last byte (an owner's own choice; real account ids are
    // hashes, but the limit must hold anyway).
    let close = |i: usize| -> [u8; 32] {
        let mut b = [0x5A; 32];
        b[31] = i as u8;
        b
    };
    let remove = run.w.profile(heavy.id).blocked.clone();
    let r = run.owner("heavy blocks 64 ids sharing 31 bytes (remove 64, add 64)", &heavy, M::Block { add: (0..64).map(close).collect(), remove });
    run.ok(&r);
    let r = run.owner("WORST edit: heavy removes 64 absent ids sharing 31 bytes with its 64 blocked, re-adds those 64", &heavy, M::Block { add: (0..64).map(close).collect(), remove: (64..128).map(close).collect() });
    run.ok(&r);
    let r = run.owner("heavy removes 64 absent ids sharing 31 bytes with its 64 blocked (nothing else)", &heavy, M::Block { add: vec![], remove: (128..192).map(close).collect() });
    run.ok(&r);
    let ns: Vec<u64> = run.w.inbox(target.id).unwrap().entries.iter().filter(|e| e.len == 2048).map(|e| e.n).take(15).collect();
    let r = run.owner("target deletes 15 envelopes of 2048 B", &target, M::Delete { ns });
    run.ok(&r);
    let upto = run.w.inbox(target.id).unwrap().entries.last().unwrap().n;
    let r = run.owner("target clears the rest (16 envelopes, one of 4096 B)", &target, M::Clear { upto });
    run.ok(&r);
    assert!(run.w.inbox(target.id).unwrap().entries.is_empty());
    // 16 envelopes of 4096 B (the byte cap), then one Clear of all 16; and 32 of 2048 B, cleared at once.
    let r = run.owner("contact 2 publishes", &contacts[2], M::Publish { key: hybrid_key(0, 6) });
    run.ok(&r);
    let r = run.owner("contact 2 adds contacts 0 and 3", &contacts[2], M::Contacts { add: vec![contacts[0].id.into_value(), contacts[3].id.into_value()], remove: vec![] });
    run.ok(&r);
    run.w.0.now += RATE_WINDOW_MS;
    for i in 0..16 {
        let c = &contacts[if i % 2 == 0 { 0 } else { 3 }];
        let r = run.send(&format!("-> contact 2, 4096 B #{i}"), c, contacts[2].id, true, 4096);
        run.ok(&r);
    }
    let upto = run.w.inbox(contacts[2].id).unwrap().entries.last().unwrap().n;
    let r = run.owner("WORST clear by bytes: 16 envelopes of 4096 B", &contacts[2], M::Clear { upto });
    run.ok(&r);
    run.w.0.now += RATE_WINDOW_MS;
    for i in 0..32 {
        let c = &contacts[if i % 2 == 0 { 0 } else { 3 }];
        let r = run.send(&format!("-> contact 2, 2048 B #{i}"), c, contacts[2].id, true, 2048);
        run.ok(&r);
    }
    let upto = run.w.inbox(contacts[2].id).unwrap().entries.last().unwrap().n;
    let r = run.owner("WORST clear by count: 32 envelopes of 2048 B", &contacts[2], M::Clear { upto });
    run.ok(&r);

    // A first publish on full lists (it opens the inbox too), then a clear of 32 envelopes by that full-list owner.
    let r = run.owner("late: 64 contacts before any key (4 of them real)", &late, M::Contacts {
        add: ids(80_000..80_060).into_iter().chain(contacts.iter().map(|c| c.id.into_value())).collect(),
        remove: vec![],
    });
    run.ok(&r);
    let r = run.owner("late: 64 blocked", &late, M::Block { add: ids(81_000..81_064), remove: vec![] });
    run.ok(&r);
    let r = run.owner("WORST publish: late's first key, on full lists (opens the inbox)", &late, M::Publish { key: hybrid_key(0, 5) });
    run.ok(&r);
    run.w.0.now += RATE_WINDOW_MS;
    for i in 0..32 {
        let c = &contacts[i % contacts.len()];
        let r = run.send(&format!("-> late, 2048 B #{i}"), c, late.id, true, 2048);
        run.ok(&r);
    }
    let upto = run.w.inbox(late.id).unwrap().entries.last().unwrap().n;
    let r = run.owner("WORST clear: late (full lists) clears 32 envelopes of 2048 B", &late, M::Clear { upto });
    run.ok(&r);

    // --- Codes: every one, BAD_STATE included (M4, N3) -----------------------------------------------------------------
    let reached = run.w.0.codes();
    let expected: BTreeSet<u8> = code::ALL.iter().map(|(c, _)| *c).filter(|c| !code::UNREACHABLE.contains(c)).collect();
    assert_eq!(reached, expected, "codes reached vs every code");

    // --- The record, then the gas limits ---------------------------------------------------------------------------------
    let mut max: std::collections::BTreeMap<String, (u64, String)> = Default::default();
    for (i, kind) in &run.kinds {
        let (label, r) = &run.w.0.reports[*i];
        if !r.ok() {
            continue;
        }
        let key = match kind {
            Kind::Remove(k) => format!("remove {k}"),
            Kind::Edit(n) => format!("edit {n}"),
            other => format!("{other:?}").to_lowercase(),
        };
        let e = max.entry(key).or_insert((0, String::new()));
        if r.charge.cycles > e.0 {
            *e = (r.charge.cycles, label.clone());
        }
    }
    let mut md = run.w.0.record(
        "ferrata_msg_v2 in LEZ's state machine and fee settlement (harness/tests/messages_v2.rs)",
        &format!(
            "Measured: one signed public transaction per row, each in its own block (RISC0_DEV_MODE executor; cycles as LEZ \
             meters them, every actor a transaction reaches included). Generated by `cargo test -p harness --features msg \
             --test messages_v2`. Program `ferrata_msg_v2` image id `{}`. Codes reached: {} of {} ({}; `BAD_STATE` by a \
             send addressed to one of the program's own accounts).",
            harness::image_id_hex(methods_msg::FERRATA_MSG_V2_ID),
            reached.len(),
            code::ALL.len(),
            reached.iter().map(|c| code::name(*c)).collect::<Vec<_>>().join(", ")
        ),
        code::name,
    );
    md += "\n## Largest applied transaction of each kind\n\n| kind | cycles | transaction |\n|---|---|---|\n";
    for (k, (c, l)) in &max {
        md += &format!("| {k} | {c} | {l} |\n");
    }
    println!("{md}");
    fs::write(results_dir().join("messages-v2.md"), &md).unwrap();

    let gas = client_gas("GAS_V2");
    for (i, kind) in &run.kinds {
        let (label, r) = &run.w.0.reports[*i];
        if !r.ok() {
            continue;
        }
        let limit = match kind {
            Kind::Publish => gas["publish"],
            Kind::Edit(n) => gas["edit"] + gas["editPer"] * *n as u64,
            Kind::Send => gas["send"],
            Kind::Remove(k) => gas["removeBase"] + gas["removePer"] * *k as u64,
        };
        assert!(r.charge.cycles <= limit, "{label}: {} cycles over the clients' limit {limit} ({kind:?})", r.charge.cycles);
    }
}
