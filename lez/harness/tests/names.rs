//! The test network's names program, `ferrata_names` (guests/names; the design in `ferrata_lez_names_wire`): every
//! action and every refusal, through LEZ's own state machine and fee settlement, each transaction signed by the account
//! that acts (and paid by the chain's fee payer). Writes `results/names-*.md` (each action's cycles, gas and fee) and
//! `results/names-vectors.json` (the encodings the JS resolver in `names/js/` is tested against).
//!
//! v2 (the review's fixes): the oldest commitment wins, look-alikes share an address, held names are a list that the
//! account itself prunes with `Forget`, deadlines hold for the whole validity window. The review's own cases are in
//! `names_review.rs`.
//!
//! `RISC0_DEV_MODE=1 cargo test -p harness --features names --locked --test names -- --nocapture --test-threads 1`
#![cfg(feature = "names")]

use std::{borrow::Cow, fs};

use ferrata_lez_names_wire::{
    check_name, code, commitment, event_selector, name_actor, name_seed, read, sequence_actor, sequence_seed, skeleton, user_actor,
    NameRecord, NamesMessage as M, PendingCommit, PrivateReceiver, UserState, CLOCK_SLACK_MS, COMMIT_SLACK_MS,
    CONTEST_MS, EVENT_KINDS, GRACE_MS, MAX_NAMES, MAX_REVEAL_WINDOW_MS, MIN_REVEAL_DELAY_MS, NPK_LEN, TERM_MS, VPK_LEN,
};
use harness::{grouped, results_dir, Chain, TxReport};
use lee::{program::Program, Account, AccountId, Actor, PrivateKey, PublicAccountEvidence, PublicKey, V03State};

/// A block time in 2027 (ms); each action's `now_ms`, its block 12 s later.
const T0: u64 = 1_800_000_000_000;
const DAY: u64 = 86_400_000;
const LAG: u64 = 12_000;

fn program() -> AccountId {
    AccountId::from_builtin_program(methods_names::FERRATA_NAMES_ID)
}

struct User {
    key: PrivateKey,
    id: AccountId,
}

fn account(b: u8) -> User {
    let key = PrivateKey::try_new([b; 32]).unwrap();
    let id = AccountId::from(&PublicKey::new_from_private_key(&key));
    User { key, id }
}

struct World {
    chain: Chain,
    alice: User,
    bob: User,
    carol: User,
    mallory: User,
    /// `act` sends the signer's `Commit` (5 minutes before) ahead of every `Register` (the reveal).
    auto_commit: bool,
}

/// The salt of the tests' reveals.
const SALT: [u8; 32] = [0x5a; 32];
/// How long before a reveal `act` commits: past the least delay (`COMMIT_SLACK_MS + MIN_REVEAL_DELAY_MS`).
const LEAD: u64 = 300_000;

fn world() -> World {
    let (alice, bob, carol, mallory) = (account(0xA1), account(0xB2), account(0xC3), account(0xD4));
    let mut chain = Chain::new();
    let state = std::mem::replace(&mut chain.state, V03State::new());
    let funded = [alice.id, bob.id, carol.id, mallory.id].map(|id| (id, Account::funded(1)));
    let names = Program::new_unchecked(methods_names::FERRATA_NAMES_ID, Cow::Borrowed(methods_names::FERRATA_NAMES_ELF));
    chain.state = state.with_public_accounts(funded).with_genesis_programs([(names, true)]);
    World { chain, alice, bob, carol, mallory, auto_commit: true }
}

fn register(name: &str, now_ms: u64, target: Option<AccountId>) -> M {
    M::Register { name: name.into(), now_ms, target, receiver: None, salt: SALT }
}

fn commit_of(name: &str, who: &User, salt: [u8; 32], now_ms: u64) -> M {
    M::Commit { commitment: commitment(name, who.id, &salt), now_ms }
}

fn receiver(fill: u8) -> PrivateReceiver {
    PrivateReceiver { npk: [fill; NPK_LEN], vpk: vec![fill; VPK_LEN] }
}

impl World {
    /// `message` signed by `who`, its root at `user(who)`; the name's actor declared (with its PDA evidence while it
    /// holds no record), and `extra` actors. Settled at `time` (the block's timestamp).
    fn act(&mut self, label: &str, who: &User, message: &M, extra: &[Actor], time: u64) -> TxReport {
        if let (true, M::Register { name, now_ms, salt, .. }) = (self.auto_commit, message) {
            let c = M::Commit { commitment: commitment(name, who.id, salt), now_ms: now_ms - LEAD };
            ok(&self.act(&format!("commit to {name:?}"), who, &c, &[], now_ms - LEAD + LAG));
        }
        let (names, sequence) = targets(message);
        self.act_raw(label, who, borsh::to_vec(message).unwrap(), &names, sequence, extra, time, true)
    }

    /// Declares `user(who)`, the actors of `names` and, with `sequence`, the counter, each with its PDA evidence
    /// while it holds no state.
    #[allow(clippy::too_many_arguments)]
    fn act_raw(
        &mut self,
        label: &str,
        who: &User,
        bytes: Vec<u8>,
        names: &[String],
        sequence: bool,
        extra: &[Actor],
        time: u64,
        sign: bool,
    ) -> TxReport {
        let p = program();
        let root = user_actor(p, who.id);
        let mut actors = vec![root];
        let mut evidence = vec![];
        let mut pdas: Vec<(Actor, _)> = names.iter().map(|n| (name_actor(p, n), name_seed(n))).collect();
        if sequence {
            pdas.push((sequence_actor(p), sequence_seed()));
        }
        for (at, seed) in pdas {
            if actors.contains(&at) {
                continue;
            }
            actors.push(at);
            if self.chain.actor_state(at).is_empty() {
                evidence.push(PublicAccountEvidence::Pda { program: p, seed });
            }
        }
        actors.extend_from_slice(extra);
        let signers: Vec<&PrivateKey> = if sign { vec![&who.key] } else { vec![] };
        self.chain.submit_signed(label, root, bytes, actors, evidence, time, &signers)
    }

    fn record(&self, name: &str) -> Option<NameRecord> {
        let s = self.chain.actor_state(name_actor(program(), name));
        (!s.is_empty()).then(|| borsh::from_slice(&s).expect("a record"))
    }

    fn user(&self, u: &User) -> UserState {
        let s = self.chain.actor_state(user_actor(program(), u.id));
        if s.is_empty() {
            UserState::default()
        } else {
            borsh::from_slice(&s).expect("a user state")
        }
    }

    fn resolve(&self, name: &str, now_ms: u64) -> Option<NameRecord> {
        read::resolve(&self.chain.actor_state(name_actor(program(), name)), name, now_ms)
    }

    fn held(&self, u: &User) -> Vec<String> {
        self.user(u).names
    }

    fn reverse(&self, u: &User, now_ms: u64) -> Option<String> {
        let primary = self.user(u).primary?;
        let user_state = self.chain.actor_state(user_actor(program(), u.id));
        read::reverse(&user_state, &self.chain.actor_state(name_actor(program(), &primary)), u.id, now_ms)
    }
}

/// The actors a message touches besides `user(signer)`: the names' and, for `Commit`, the counter.
fn targets(m: &M) -> (Vec<String>, bool) {
    match m {
        M::Forget { names } => (names.clone(), false),
        M::Commit { .. } => (vec![], true),
        _ => (m.name().map(|n| vec![n.to_string()]).unwrap_or_default(), false),
    }
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn ok(r: &TxReport) {
    assert!(r.ok(), "{}: exit {:?}, {:?}", r.label, r.exit_code, r.charge.revert);
    println!("applied: {} ({} gas)", r.label, grouped(r.charge.gas_exec));
}

fn refused(r: &TxReport, c: u8) {
    assert!(!r.ok(), "{} was applied", r.label);
    assert_eq!(r.exit_code, Some(u32::from(c)), "{}: {:?}", r.label, r.charge.revert);
    println!("refused as expected: {} → exit {c} ({})", r.label, code_name(c));
}

fn code_name(c: u8) -> &'static str {
    code::ALL.iter().find(|(_, v)| *v == c).map_or("?", |(n, _)| n)
}

fn emitted(r: &TxReport, kind: &str) -> bool {
    r.events.iter().any(|e| e.event.selector == event_selector(kind))
}

/// Each transaction's cost, as LEZ charged it.
fn table(reports: &[TxReport]) -> String {
    let mut out = String::from(
        "| # | action | handlers (cycles each) | cycles | execution gas | storage gas (tx bytes) | base fee | result |\n\
         |---|---|---|---:|---:|---:|---:|---|\n",
    );
    for (i, r) in reports.iter().enumerate() {
        let handlers: Vec<String> = r.transitions.iter().map(|t| grouped(t.cycles)).collect();
        let cycles: u64 = r.transitions.iter().map(|t| t.cycles).sum();
        let result = match (r.exit_code, &r.charge.revert) {
            (Some(c), _) => format!("refused: exit {c} ({})", code_name(c as u8)),
            (None, Some(revert)) => format!("reverted: {revert}"),
            (None, None) => "applied".into(),
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            i + 1,
            r.label,
            handlers.join(" → "),
            grouped(cycles),
            grouped(r.charge.gas_exec),
            grouped(r.charge.gas_stor),
            grouped(r.charge.fee_base as u64),
            result,
        ));
    }
    out
}

fn write(file: &str, title: &str, reports: &[TxReport]) {
    let text = format!(
        "# {title}\n\nGenerated by `harness/tests/names.rs` (`RISC0_DEV_MODE=1 cargo test -p harness --features names \
         --test names`), LEZ's state machine and fee settlement; measured. Cycles are the guest's, per handler run \
         (`user(a)` first, then `name(n)`, then the bookkeeping at `user(..)`).\n\n{}",
        table(reports)
    );
    fs::write(results_dir().join(file), text).unwrap();
}

#[test]
fn register_resolve_reverse_update_renew_transfer_release() {
    let mut w = world();
    let (a, b) = (w.alice.id, w.bob.id);
    let mut rs = Vec::new();
    let mut t = T0;

    let alice = std::mem::replace(&mut w.alice, account(0xA1));
    let bob = std::mem::replace(&mut w.bob, account(0xB2));
    let carol = std::mem::replace(&mut w.carol, account(0xC3));

    w.auto_commit = false;
    let r = w.act("commit to `alice` (a hash: the name stays hidden)", &alice, &commit_of("alice", &alice, SALT, t - LEAD), &[], t - LEAD + LAG);
    ok(&r);
    rs.push(r);
    w.auto_commit = true;
    let r = w.act_raw("reveal: register `alice` → alice's account", &alice, borsh::to_vec(&register("alice", t, Some(a))).unwrap(), &strs(&["alice"]), false, &[], t + LAG, true);
    ok(&r);
    assert!(emitted(&r, "Registered"));
    rs.push(r);
    let rec = w.resolve("alice", t + LAG).expect("alice resolves");
    assert_eq!((rec.owner, rec.target, rec.expires_ms), (a, Some(a), t + TERM_MS));
    assert_eq!(w.held(&alice), strs(&["alice"]));
    assert_eq!(rec.final_at(), t + CONTEST_MS);

    let r = w.act("set primary name `alice` (reverse record)", &alice, &M::SetPrimary { name: Some("alice".into()), now_ms: t }, &[], t + LAG);
    ok(&r);
    rs.push(r);
    assert_eq!(w.reverse(&alice, t + LAG).as_deref(), Some("alice"));

    t += DAY;
    let set = M::SetTarget { name: "alice".into(), target: None, receiver: Some(receiver(7)), now_ms: t };
    let r = w.act("update: no public target, a private receiver (npk + 1,184-byte vpk)", &alice, &set, &[], t + LAG);
    ok(&r);
    assert!(emitted(&r, "Updated"));
    rs.push(r);
    let rec = w.resolve("alice", t + LAG).unwrap();
    assert_eq!((rec.target, rec.receiver), (None, Some(receiver(7))));
    // The primary name stays stored but no longer reverse-resolves: the name does not point at alice.
    assert_eq!(w.reverse(&alice, t + LAG), None);

    let set = M::SetTarget { name: "alice".into(), target: Some(a), receiver: None, now_ms: t };
    let r = w.act("update: back to alice's account", &alice, &set, &[], t + LAG);
    ok(&r);
    rs.push(r);
    assert_eq!(w.reverse(&alice, t + LAG).as_deref(), Some("alice"));

    t = T0 + 100 * DAY;
    let r = w.act("renew (100 days in)", &alice, &M::Renew { name: "alice".into(), now_ms: t }, &[], t + LAG);
    ok(&r);
    assert!(emitted(&r, "Renewed"));
    rs.push(r);
    assert_eq!(w.record("alice").unwrap().expires_ms, t + TERM_MS);

    let r = w.act("offer `alice` to carol", &alice, &M::Transfer { name: "alice".into(), to: carol.id, now_ms: t }, &[], t + LAG);
    ok(&r);
    assert!(emitted(&r, "Offered"));
    rs.push(r);
    assert_eq!(w.record("alice").unwrap().offer, Some(carol.id));
    let r = w.act("withdraw the offer (offer to oneself)", &alice, &M::Transfer { name: "alice".into(), to: a, now_ms: t }, &[], t + LAG);
    ok(&r);
    rs.push(r);
    assert_eq!(w.record("alice").unwrap().offer, None);
    let r = w.act("carol accepts the withdrawn offer", &carol, &M::Accept { name: "alice".into(), now_ms: t }, &[], t + LAG);
    refused(&r, code::NOT_TARGET);
    rs.push(r);
    let r = w.act("offer `alice` to bob", &alice, &M::Transfer { name: "alice".into(), to: b, now_ms: t }, &[], t + LAG);
    ok(&r);
    rs.push(r);
    // Until bob accepts, nothing else changes.
    let rec = w.resolve("alice", t + LAG).unwrap();
    assert_eq!((rec.owner, rec.target, rec.offer), (a, Some(a), Some(b)));
    assert_eq!(w.held(&bob), Vec::<String>::new());
    let r = w.act("bob accepts `alice` (alice's actor not in the transaction)", &bob, &M::Accept { name: "alice".into(), now_ms: t }, &[], t + LAG);
    ok(&r);
    assert!(emitted(&r, "Transferred"));
    rs.push(r);
    let rec = w.resolve("alice", t + LAG).unwrap();
    assert_eq!((rec.owner, rec.target, rec.receiver.is_none(), rec.offer), (b, Some(b), true, None));
    assert_eq!(w.held(&bob), strs(&["alice"]));
    // alice keeps `alice` in her list (and as her stored primary name, which no longer reverse-resolves) until she
    // forgets it: the transfer did not need her actor.
    assert_eq!(w.held(&alice), strs(&["alice"]));
    assert_eq!(w.reverse(&alice, t + LAG), None);
    let r = w.act("alice forgets `alice` (it left her list without her)", &alice, &M::Forget { names: strs(&["alice"]) }, &[], t + LAG);
    ok(&r);
    rs.push(r);
    assert_eq!(w.user(&alice), UserState::default());

    let r = w.act("bob sets primary name `alice`", &bob, &M::SetPrimary { name: Some("alice".into()), now_ms: t }, &[], t + LAG);
    ok(&r);
    rs.push(r);
    assert_eq!(w.reverse(&bob, t + LAG).as_deref(), Some("alice"));

    let r = w.act("clear primary name", &bob, &M::SetPrimary { name: None, now_ms: t }, &[], t + LAG);
    ok(&r);
    rs.push(r);
    assert_eq!(w.reverse(&bob, t + LAG), None);

    let r = w.act("release `alice`", &bob, &M::Release { name: "alice".into() }, &[], t + LAG);
    ok(&r);
    assert!(emitted(&r, "Released"));
    rs.push(r);
    assert_eq!(w.record("alice"), None);
    assert_eq!(w.user(&bob), UserState::default());
    // The name's account stays in LEZ's state with nothing in it (LEZ removes no account): one account per name address
    // ever registered, paid for by the fees of the transactions that wrote it (names/README.md).
    let at = name_actor(program(), "alice").account_id;
    assert!(w.chain.state.get_account_by_id_ref(at).is_some(), "the released name's account is still in the state");

    let r = w.act("register `alice` again after its release (carol)", &carol, &register("alice", t, Some(carol.id)), &[], t + LAG);
    ok(&r);
    rs.push(r);
    assert_eq!(w.resolve("alice", t + LAG).unwrap().owner, carol.id);

    let long = "a".repeat(32);
    let r = w.act("register a 32-byte name", &carol, &register(&long, t, Some(carol.id)), &[], t + LAG);
    ok(&r);
    rs.push(r);

    write("names-lifecycle.md", "ferrata_names: every action, applied", &rs);
}

#[test]
fn every_rule_refuses() {
    let mut w = world();
    let alice = std::mem::replace(&mut w.alice, account(0xA1));
    let bob = std::mem::replace(&mut w.bob, account(0xB2));
    let carol = std::mem::replace(&mut w.carol, account(0xC3));
    let mallory = std::mem::replace(&mut w.mallory, account(0xD4));
    let (a, b) = (alice.id, bob.id);
    let t = T0;
    let at = t + LAG;
    let mut rs = Vec::new();

    ok(&w.act("register `alice`", &alice, &register("alice", t, Some(a)), &[], at));

    for (bad, why) in [
        ("ab", "too short"),
        ("abcdefghijklmnopqrstuvwxyz0123456", "33 bytes"),
        ("Alice", "upper case"),
        ("al ice", "a space"),
        ("al_ice", "an underscore"),
        ("-alice", "leading hyphen"),
        ("alice-", "trailing hyphen"),
        ("al--ice", "double hyphen"),
        ("аlice", "Cyrillic а (a look-alike)"),
        ("alice.eth", "a dot"),
    ] {
        let r = w.act(&format!("register {bad:?} ({why})"), &bob, &register(bad, t, Some(b)), &[], at);
        refused(&r, code::INVALID_NAME);
        rs.push(r);
    }
    let r = w.act("register `faucet` (reserved)", &mallory, &register("faucet", t, Some(mallory.id)), &[], at);
    refused(&r, code::RESERVED_NAME);
    rs.push(r);
    for variant in ["faucet1", "wa11et", "ferrata-team", "0fficial"] {
        let r = w.act(&format!("register `{variant}` (a variant of a reserved name)"), &mallory, &register(variant, t, Some(mallory.id)), &[], at);
        refused(&r, code::RESERVED_NAME);
        rs.push(r);
    }

    let r = w.act("bob registers `alice` (held by alice)", &bob, &register("alice", t, Some(b)), &[], at);
    refused(&r, code::TAKEN);
    rs.push(r);
    assert_eq!(skeleton("a1ice"), "alice");
    let r = w.act("bob registers `a1ice` (digit one: the look-alike of `alice`)", &bob, &register("a1ice", t, Some(b)), &[], at);
    refused(&r, code::CONFUSABLE);
    rs.push(r);
    assert_eq!(w.resolve("a1ice", at), None);

    for (label, m, extra) in [
        ("mallory transfers `alice` to herself", M::Transfer { name: "alice".into(), to: mallory.id, now_ms: t }, vec![]),
        ("mallory points `alice` at her account", M::SetTarget { name: "alice".into(), target: Some(mallory.id), receiver: None, now_ms: t }, vec![]),
        ("mallory renews `alice`", M::Renew { name: "alice".into(), now_ms: t }, vec![]),
        ("mallory releases `alice`", M::Release { name: "alice".into() }, vec![]),
    ] {
        let r = w.act(label, &mallory, &m, &extra, at);
        refused(&r, code::NOT_OWNER);
        rs.push(r);
    }
    ok(&w.act("alice offers `alice` to bob", &alice, &M::Transfer { name: "alice".into(), to: b, now_ms: t }, &[], at));
    let r = w.act("mallory accepts `alice` (offered to bob)", &mallory, &M::Accept { name: "alice".into(), now_ms: t }, &[], at);
    refused(&r, code::NOT_TARGET);
    rs.push(r);
    let r = w.act("mallory accepts `nobody` (not registered)", &mallory, &M::Accept { name: "nobody".into(), now_ms: t }, &[], at);
    refused(&r, code::NOT_FOUND);
    rs.push(r);
    ok(&w.act("alice withdraws the offer", &alice, &M::Transfer { name: "alice".into(), to: a, now_ms: t }, &[], at));
    let r = w.act("mallory sets `alice` (pointing at alice) as her primary name", &mallory, &M::SetPrimary { name: Some("alice".into()), now_ms: t }, &[], at);
    refused(&r, code::NOT_TARGET);
    rs.push(r);
    assert_eq!(w.reverse(&mallory, at), None);

    let r = w.act_raw("a Register at user(alice), not signed by alice", &alice, borsh::to_vec(&register("zzz", t, Some(a))).unwrap(), &strs(&["zzz"]), false, &[], at, false);
    refused(&r, code::NOT_SIGNED);
    rs.push(r);
    for (label, m) in [
        ("mallory signs the program's own Credit", M::Credit { name: "alice".into() }),
        ("mallory signs the program's own Claim (a reveal without a commitment)", M::Claim { name: "alice".into(), now_ms: t, target: None, receiver: None, seq: 1 }),
        ("mallory signs the program's own Sequenced", M::Sequenced { seq: 1 }),
        ("mallory signs the program's own Tick", M::Tick),
    ] {
        let r = w.act(label, &mallory, &m, &[], at);
        refused(&r, code::WRONG_SENDER);
        rs.push(r);
    }
    let r = w.act_raw("a message that does not decode", &mallory, vec![0xff, 1, 2], &[], false, &[], at, true);
    refused(&r, code::BAD_MESSAGE);
    rs.push(r);
    let bad_keys = M::Register { name: "keys".into(), now_ms: t, target: None, receiver: Some(PrivateReceiver { npk: [1; NPK_LEN], vpk: vec![1; 32] }), salt: SALT };
    let r = w.act("register with a 32-byte vpk (not ML-KEM-768's 1,184)", &bob, &bad_keys, &[], at);
    refused(&r, code::BAD_MESSAGE);
    rs.push(r);
    let r = w.act("renew `nobody` (not registered)", &bob, &M::Renew { name: "nobody".into(), now_ms: t }, &[], at);
    refused(&r, code::NOT_FOUND);
    rs.push(r);
    let r = w.act("alice transfers `alice` to herself (no offer to withdraw)", &alice, &M::Transfer { name: "alice".into(), to: a, now_ms: t }, &[], at);
    refused(&r, code::SAME_OWNER);
    rs.push(r);

    // Forget: only names in the signer's list, and only once they are no longer the signer's.
    let r = w.act("alice forgets `alice` (still hers)", &alice, &M::Forget { names: strs(&["alice"]) }, &[], at);
    refused(&r, code::STILL_HELD);
    rs.push(r);
    let r = w.act("mallory forgets `alice` (never in her list)", &mallory, &M::Forget { names: strs(&["alice"]) }, &[], at);
    refused(&r, code::NOT_HELD);
    rs.push(r);
    let r = w.act("alice forgets no names", &alice, &M::Forget { names: vec![] }, &[], at);
    refused(&r, code::BAD_MESSAGE);
    rs.push(r);
    let r = w.act("alice forgets `alice` twice in one message", &alice, &M::Forget { names: strs(&["alice", "alice"]) }, &[], at);
    refused(&r, code::BAD_MESSAGE);
    rs.push(r);

    // The squatting limit: MAX_NAMES per account, by registration or by transfer.
    for i in 1..MAX_NAMES {
        ok(&w.act(&format!("alice registers `alice{i}`"), &alice, &register(&format!("alice{i}"), t, Some(a)), &[], at));
    }
    assert_eq!(w.held(&alice).len(), MAX_NAMES);
    let r = w.act("alice registers a sixth name", &alice, &register("alice6", t, Some(a)), &[], at);
    refused(&r, code::TOO_MANY);
    rs.push(r);
    ok(&w.act("carol registers `carol`", &carol, &register("carol", t, Some(carol.id)), &[], at));
    ok(&w.act("carol offers `carol` to alice", &carol, &M::Transfer { name: "carol".into(), to: a, now_ms: t }, &[], at));
    let r = w.act("alice accepts `carol` (she holds five)", &alice, &M::Accept { name: "carol".into(), now_ms: t }, &[], at);
    refused(&r, code::TOO_MANY);
    rs.push(r);
    ok(&w.act("alice releases `alice1`", &alice, &M::Release { name: "alice1".into() }, &[], at));
    ok(&w.act("alice accepts `carol` (now four)", &alice, &M::Accept { name: "carol".into(), now_ms: t }, &[], at));
    assert_eq!((w.held(&alice).len(), w.held(&carol)), (MAX_NAMES, strs(&["carol"])));
    ok(&w.act("carol forgets `carol`", &carol, &M::Forget { names: strs(&["carol"]) }, &[], at));
    assert_eq!(w.held(&carol), Vec::<String>::new());

    // Time: `now_ms` must be the block's time to within the window (2 minutes for a commit and a reveal, 10 for the
    // rest), never later than it. Such a failure has no code and is charged the whole gas limit.
    let outside = |r: &TxReport| {
        assert!(!r.ok() && r.exit_code.is_none(), "{}: {:?}", r.label, r.charge.revert);
        println!("refused as expected: {} → {:?}", r.label, r.charge.revert);
    };
    let r = w.act("reveal with now_ms 1 minute after the block's time", &bob, &register("bobby", at + 60_000, Some(b)), &[], at);
    outside(&r);
    rs.push(r);
    let r = w.act("reveal with now_ms 2 minutes before the block's time", &bob, &register("bobby", at - COMMIT_SLACK_MS, Some(b)), &[], at);
    outside(&r);
    rs.push(r);
    assert_eq!(w.record("bobby"), None);
    ok(&w.act("reveal with now_ms 2 minutes less 1 ms before the block's time", &bob, &register("bobby", at - COMMIT_SLACK_MS + 1, Some(b)), &[], at));
    let r = w.act("renew with now_ms 10 minutes before the block's time", &bob, &M::Renew { name: "bobby".into(), now_ms: at - CLOCK_SLACK_MS }, &[], at);
    outside(&r);
    rs.push(r);
    ok(&w.act("renew with now_ms 10 minutes less 1 ms before the block's time", &bob, &M::Renew { name: "bobby".into(), now_ms: at - CLOCK_SLACK_MS + 1 }, &[], at));

    write("names-refusals.md", "ferrata_names: every refusal", &rs);
}

#[test]
fn expiry_grace_and_lapse() {
    let mut w = world();
    let alice = std::mem::replace(&mut w.alice, account(0xA1));
    let bob = std::mem::replace(&mut w.bob, account(0xB2));
    let (a, b) = (alice.id, bob.id);
    let mut rs = Vec::new();
    let t = T0;

    ok(&w.act("register `zed`", &alice, &register("zed", t, Some(a)), &[], t + LAG));
    ok(&w.act("primary `zed`", &alice, &M::SetPrimary { name: Some("zed".into()), now_ms: t }, &[], t + LAG));
    ok(&w.act("offer `zed` to bob", &alice, &M::Transfer { name: "zed".into(), to: b, now_ms: t }, &[], t + LAG));
    let expiry = t + TERM_MS;
    assert!(w.resolve("zed", expiry - 1).is_some());
    assert_eq!(w.reverse(&alice, expiry - 1).as_deref(), Some("zed"));
    assert!(w.resolve("zed", expiry).is_none(), "an expired name does not resolve");
    assert_eq!(w.reverse(&alice, expiry), None);

    let now = expiry;
    let r = w.act("transfer `zed` after its expiry", &alice, &M::Transfer { name: "zed".into(), to: b, now_ms: now }, &[], now + LAG);
    refused(&r, code::EXPIRED);
    rs.push(r);
    let r = w.act("bob accepts the offer of `zed` after its expiry", &bob, &M::Accept { name: "zed".into(), now_ms: now }, &[], now + LAG);
    refused(&r, code::EXPIRED);
    rs.push(r);
    let r = w.act("update `zed` after its expiry", &alice, &M::SetTarget { name: "zed".into(), target: Some(b), receiver: None, now_ms: now }, &[], now + LAG);
    refused(&r, code::EXPIRED);
    rs.push(r);
    let r = w.act("bob registers `zed` within the grace period", &bob, &register("zed", now, Some(b)), &[], now + LAG);
    refused(&r, code::TAKEN);
    rs.push(r);

    // A renewal must land in the grace period by block time: its whole validity window (10 minutes) inside it.
    let now = expiry + GRACE_MS - CLOCK_SLACK_MS + 1;
    let r = w.act("alice renews `zed` with a window reaching 1 ms past the grace period", &alice, &M::Renew { name: "zed".into(), now_ms: now }, &[], now + LAG);
    refused(&r, code::LAPSED);
    rs.push(r);
    let now = expiry + GRACE_MS - CLOCK_SLACK_MS;
    let r = w.act("alice renews `zed` with a window ending as the grace period ends", &alice, &M::Renew { name: "zed".into(), now_ms: now }, &[], now + LAG);
    ok(&r);
    rs.push(r);
    let expiry = now + TERM_MS;
    assert_eq!(w.record("zed").unwrap().expires_ms, expiry);
    assert_eq!(w.record("zed").unwrap().offer, None, "the offer did not survive the expiry");
    let r = w.act("bob accepts the stale offer of `zed` after its renewal", &bob, &M::Accept { name: "zed".into(), now_ms: now }, &[], now + LAG);
    refused(&r, code::NOT_TARGET);
    rs.push(r);
    assert_eq!(w.reverse(&alice, now + LAG).as_deref(), Some("zed"));

    let now = expiry + GRACE_MS;
    let r = w.act("alice renews `zed` after the grace period", &alice, &M::Renew { name: "zed".into(), now_ms: now }, &[], now + LAG);
    refused(&r, code::LAPSED);
    rs.push(r);
    assert_eq!(w.held(&alice), strs(&["zed"]), "a lapsed name counts until someone takes it or it is released");
    let r = w.act("bob registers the lapsed `zed` (alice's actor not in the transaction)", &bob, &register("zed", now, Some(b)), &[], now + LAG);
    ok(&r);
    rs.push(r);
    assert_eq!(w.held(&bob), strs(&["zed"]));
    assert_eq!(w.resolve("zed", now + LAG).unwrap().owner, b);
    assert_eq!(w.reverse(&alice, now + LAG), None);
    let r = w.act("alice forgets the lapsed `zed`, now bob's", &alice, &M::Forget { names: strs(&["zed"]) }, &[], now + LAG);
    ok(&r);
    rs.push(r);
    assert_eq!(w.user(&alice), UserState::default(), "her list and her primary name are cleared");

    // The old owner taking back their own lapsed name: their count does not move.
    let later = now + TERM_MS + GRACE_MS;
    ok(&w.act("bob lets `zed` lapse and registers it again", &bob, &register("zed", later, Some(b)), &[], later + LAG));
    assert_eq!(w.held(&bob), strs(&["zed"]));

    write("names-expiry.md", "ferrata_names: expiry, grace and lapse", &rs);
}

/// The encodings `names/js/` must reproduce, for a program at the harness's address and at a fixed one.
#[test]
fn js_vectors() {
    let fixed = AccountId::new([7; 32]);
    let a = account(0xA1).id;
    let mut names = Vec::new();
    for (p, label) in [(fixed, "fixed"), (program(), "harness")] {
        for n in ["alice", "bob", "a-1", "abcdefghijklmnopqrstuvwxyz012345", "a1ice", "rnallory", "vvoody"] {
            names.push(format!(
                "{{\"program\":\"{}\",\"programLabel\":\"{label}\",\"name\":\"{n}\",\"skeleton\":\"{}\",\"seed\":\"{}\",\"account\":\"{}\"}}",
                hex::encode(p.as_ref()),
                skeleton(n),
                hex::encode(name_seed(n).as_bytes()),
                hex::encode(name_actor(p, n).account_id.as_ref())
            ));
        }
    }
    let messages = [
        register("alice", T0, Some(a)),
        M::Register { name: "alice".into(), now_ms: T0, target: None, receiver: Some(receiver(7)), salt: SALT },
        M::Renew { name: "alice".into(), now_ms: T0 },
        M::Transfer { name: "alice".into(), to: a, now_ms: T0 },
        M::SetTarget { name: "alice".into(), target: Some(a), receiver: None, now_ms: T0 },
        M::Release { name: "alice".into() },
        M::SetPrimary { name: Some("alice".into()), now_ms: T0 },
        M::SetPrimary { name: None, now_ms: T0 },
        M::Accept { name: "alice".into(), now_ms: T0 },
        M::Commit { commitment: commitment("alice", a, &SALT), now_ms: T0 },
        M::Forget { names: strs(&["alice", "bob"]) },
        M::Credit { name: "alice".into() },
        M::Debit { name: "alice".into() },
        M::Primary { name: "alice".into() },
        M::Claim { name: "alice".into(), now_ms: T0, target: Some(a), receiver: None, seq: 7 },
        M::Disown { name: "alice".into() },
        M::Tick,
        M::Sequenced { seq: 7 },
    ];
    let messages: Vec<String> = messages.iter().map(|m| format!("\"{}\"", hex::encode(borsh::to_vec(m).unwrap()))).collect();
    let record = NameRecord {
        name: "alice".into(),
        owner: a,
        target: Some(a),
        receiver: Some(receiver(7)),
        registered_ms: T0,
        expires_ms: T0 + TERM_MS,
        offer: Some(a),
        commit_seq: 7,
    };
    let user = UserState {
        names: strs(&["alice", "bob"]),
        primary: Some("alice".into()),
        commit: Some(PendingCommit { hash: commitment("alice", a, &SALT), seq: 7, not_before_ms: T0 + 240_000, expires_ms: T0 + 3_600_000 }),
    };
    let selectors: Vec<String> =
        EVENT_KINDS.iter().map(|k| format!("\"{k}\":\"{}\"", hex::encode(event_selector(k)))).collect();
    let json = format!(
        "{{\n\"account\":\"{}\",\n\"t0\":{T0},\n\"salt\":\"{}\",\n\"commitment\":\"{}\",\n\"sequence\":{{\"program\":\"{}\",\"seed\":\"{}\",\"account\":\"{}\"}},\n\"names\":[\n{}\n],\n\"messages\":[\n{}\n],\n\"record\":\"{}\",\n\"user\":\"{}\",\n\"selectors\":{{{}}}\n}}\n",
        hex::encode(a.as_ref()),
        hex::encode(SALT),
        hex::encode(commitment("alice", a, &SALT)),
        hex::encode(fixed.as_ref()),
        hex::encode(sequence_seed().as_bytes()),
        hex::encode(sequence_actor(fixed).account_id.as_ref()),
        names.join(",\n"),
        messages.join(",\n"),
        hex::encode(borsh::to_vec(&record).unwrap()),
        hex::encode(borsh::to_vec(&user).unwrap()),
        selectors.join(",")
    );
    fs::write(results_dir().join("names-vectors.json"), json).unwrap();
}

#[test]
fn commit_and_reveal() {
    let mut w = world();
    w.auto_commit = false;
    let alice = std::mem::replace(&mut w.alice, account(0xA1));
    let bob = std::mem::replace(&mut w.bob, account(0xB2));
    let mallory = std::mem::replace(&mut w.mallory, account(0xD4));
    let a = alice.id;
    let mut rs = Vec::new();
    let t = T0;
    let other_salt = [0x11; 32];

    let r = w.act("reveal `alice` with no commitment", &alice, &register("alice", t, Some(a)), &[], t + LAG);
    refused(&r, code::NO_COMMIT);
    rs.push(r);
    let r = w.act("commit to `alice`", &alice, &commit_of("alice", &alice, SALT, t), &[], t + LAG);
    ok(&r);
    rs.push(r);
    let c = w.user(&alice).commit.expect("a pending commitment");
    assert_eq!((c.not_before_ms, c.expires_ms), (t + COMMIT_SLACK_MS + MIN_REVEAL_DELAY_MS, t + MAX_REVEAL_WINDOW_MS));
    let early = c.not_before_ms - 1;
    let r = w.act("reveal 1 ms before the window opens", &alice, &register("alice", early, Some(a)), &[], early + LAG);
    refused(&r, code::TOO_EARLY);
    rs.push(r);
    let open = c.not_before_ms;
    let wrong = M::Register { name: "alice".into(), now_ms: open, target: Some(a), receiver: None, salt: other_salt };
    let r = w.act("reveal with another salt", &alice, &wrong, &[], open + LAG);
    refused(&r, code::COMMIT_MISMATCH);
    rs.push(r);
    let r = w.act("reveal of another name", &alice, &register("alicia", open, Some(a)), &[], open + LAG);
    refused(&r, code::COMMIT_MISMATCH);
    rs.push(r);
    // Alice's reveal (name and salt) copied by others.
    let r = w.act("mallory sends alice's reveal (no commitment of her own)", &mallory, &register("alice", open, Some(mallory.id)), &[], open + LAG);
    refused(&r, code::NO_COMMIT);
    rs.push(r);
    ok(&w.act("mallory commits to something", &mallory, &commit_of("other", &mallory, other_salt, open), &[], open + LAG));
    let r = w.act("mallory sends alice's reveal (her commitment binds her account)", &mallory, &register("alice", open, Some(mallory.id)), &[], open + LAG);
    refused(&r, code::COMMIT_MISMATCH);
    rs.push(r);
    let late = c.expires_ms;
    let r = w.act("reveal whose window reaches past the commitment's hour", &alice, &register("alice", late - COMMIT_SLACK_MS + 1, Some(a)), &[], late - COMMIT_SLACK_MS + 1 + LAG);
    refused(&r, code::COMMIT_EXPIRED);
    rs.push(r);
    assert_eq!(w.record("alice"), None, "an expired commitment holds nothing");

    // A new commitment replaces the pending one.
    let t1 = late + 1_000;
    ok(&w.act("commit to `alice2`", &alice, &commit_of("alice2", &alice, SALT, t1), &[], t1 + LAG));
    ok(&w.act("commit to `alice` (replaces it)", &alice, &commit_of("alice", &alice, SALT, t1), &[], t1 + LAG));
    let open = t1 + COMMIT_SLACK_MS + MIN_REVEAL_DELAY_MS;
    let r = w.act("reveal the replaced `alice2`", &alice, &register("alice2", open, Some(a)), &[], open + LAG);
    refused(&r, code::COMMIT_MISMATCH);
    rs.push(r);
    ok(&w.act("reveal `alice` at the window's first ms", &alice, &register("alice", open, Some(a)), &[], open + LAG));
    assert_eq!(w.user(&alice).commit, None, "a reveal spends the commitment");
    let r = w.act("the same reveal again", &alice, &register("alice", open, Some(a)), &[], open + LAG);
    refused(&r, code::NO_COMMIT);
    rs.push(r);

    // Front-running: bob reveals `bob-name` with now_ms n. Mallory sees it and commits in the very block of time n, with
    // the earliest now_ms its window allows; her reveal is still too early at the last ms bob's reveal can land.
    let c0 = open + 3_600_000;
    ok(&w.act("bob commits to `bob-name`", &bob, &commit_of("bob-name", &bob, SALT, c0), &[], c0 + LAG));
    let n = c0 + LEAD;
    let seen = M::Commit { commitment: commitment("bob-name", mallory.id, &other_salt), now_ms: n - COMMIT_SLACK_MS + 1 };
    ok(&w.act("mallory commits to `bob-name` in the block of time n (now_ms n - 2 min + 1 ms)", &mallory, &seen, &[], n));
    let last = n + COMMIT_SLACK_MS - 1;
    let copy = M::Register { name: "bob-name".into(), now_ms: last, target: Some(mallory.id), receiver: None, salt: other_salt };
    let r = w.act("mallory reveals at the last ms of bob's reveal window", &mallory, &copy, &[], last);
    refused(&r, code::TOO_EARLY);
    rs.push(r);
    ok(&w.act("bob's reveal (now_ms n) lands at that last ms", &bob, &register("bob-name", n, Some(bob.id)), &[], last));
    let ready = w.user(&mallory).commit.unwrap().not_before_ms;
    assert!(ready > last, "her window opens after bob's has closed");
    let copy = M::Register { name: "bob-name".into(), now_ms: ready, target: Some(mallory.id), receiver: None, salt: other_salt };
    let r = w.act("mallory reveals when her window opens", &mallory, &copy, &[], ready + LAG);
    refused(&r, code::TAKEN);
    rs.push(r);
    assert_eq!(w.resolve("bob-name", ready).unwrap().owner, bob.id);

    // The oldest commitment wins: carol commits to `slow` first, mallory after; mallory reveals first and registers
    // it; carol's reveal, later but within her hour, takes it from her (event Displaced). Mallory keeps `slow` in her
    // list until she forgets it.
    let carol = account(0xC3);
    let c1 = c0 + 3_600_000;
    ok(&w.act("carol commits to `slow`", &carol, &commit_of("slow", &carol, SALT, c1), &[], c1 + LAG));
    let c2 = c1 + 60_000;
    ok(&w.act("mallory commits to `slow`, after carol", &mallory, &commit_of("slow", &mallory, other_salt, c2), &[], c2 + LAG));
    let m_open = c2 + COMMIT_SLACK_MS + MIN_REVEAL_DELAY_MS + 30_000;
    let mine = M::Register { name: "slow".into(), now_ms: m_open, target: Some(mallory.id), receiver: None, salt: other_salt };
    let r = w.act("mallory reveals `slow` first", &mallory, &mine, &[], m_open + LAG);
    ok(&r);
    rs.push(r);
    assert!(w.record("slow").unwrap().commit_seq > 0);
    let c_late = c1 + MAX_REVEAL_WINDOW_MS - COMMIT_SLACK_MS;
    let r = w.act("carol reveals `slow` at the end of her hour (her commitment is older)", &carol, &register("slow", c_late, Some(carol.id)), &[], c_late + LAG);
    ok(&r);
    assert!(emitted(&r, "Displaced"));
    rs.push(r);
    assert_eq!(w.record("slow").unwrap().owner, carol.id);
    assert_eq!(w.held(&mallory), strs(&["slow"]));
    let r = w.act("mallory forgets `slow`", &mallory, &M::Forget { names: strs(&["slow"]) }, &[], c_late + LAG);
    ok(&r);
    rs.push(r);
    assert_eq!(w.held(&mallory), Vec::<String>::new());
    // A younger commitment does not take a name from an older one, even within the hour.
    let c3 = c_late + 1_000;
    ok(&w.act("mallory commits to `slow` again", &mallory, &commit_of("slow", &mallory, other_salt, c3), &[], c3 + LAG));
    let m3 = c3 + COMMIT_SLACK_MS + MIN_REVEAL_DELAY_MS;
    let again = M::Register { name: "slow".into(), now_ms: m3, target: Some(mallory.id), receiver: None, salt: other_salt };
    let r = w.act("mallory reveals again (younger than carol's)", &mallory, &again, &[], m3 + LAG);
    refused(&r, code::TAKEN);
    rs.push(r);

    write("names-commit-reveal.md", "ferrata_names: commit and reveal", &rs);
}

/// The shapes of 32-byte names the worst cases are measured with (the costs depend on the characters: the skeleton's
/// replacements and the reserved-name check scan the name): `j` below 26 gives distinct names of one shape.
const SHAPES: [&str; 6] = ["parts", "n", "r", "v", "rn", "1"];

fn long_name(shape: &str, j: usize) -> String {
    let letters = b"abcdefghijklmnopqrstuvwxyz";
    let last = (letters[j % 26] as char).to_string();
    let n = match shape {
        // 16 one- or two-letter parts: the most `-`-separated parts a 32-byte name can have.
        "parts" => {
            let mut parts: Vec<String> = (0..16).map(|k| (letters[(j * 3 + k) % 26] as char).to_string()).collect();
            parts[0] = parts[0].repeat(2);
            parts.join("-")
        }
        "rn" => format!("{}r{last}", "rn".repeat(15)),
        one => format!("{}{last}", one.repeat(31)),
    };
    assert_eq!(n.len(), 32, "{n}");
    assert_eq!(check_name(&n), Ok(()), "{n}");
    n
}

/// The gas limits the names clients send each action with: `names/js/names.js` `GAS` (the CLI and the wallet call its
/// `gasFor`), read from the source so that this test fails when a limit is below a measured worst case.
fn client_gas(action: &str, n: usize) -> u64 {
    let js = fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../names/js/names.js")).unwrap();
    let block = &js[js.find("export const GAS = Object.freeze({").expect("names.js GAS")..];
    let block = &block[..block.find("});").unwrap()];
    let value = |key: &str| -> u64 {
        let at = block.find(&format!("{key}: ")).unwrap_or_else(|| panic!("GAS.{key}")) + key.len() + 2;
        block[at..].chars().take_while(|c| c.is_ascii_digit() || *c == '_').filter(|c| *c != '_').collect::<String>().parse().unwrap()
    };
    if action == "Forget" {
        value("ForgetBase") + value("ForgetPerName") * n as u64
    } else {
        value(action)
    }
}

fn cycles(r: &TxReport) -> u64 {
    ok(r);
    r.transitions.iter().map(|t| t.cycles).sum()
}

/// The worst case of every action a client sends, measured through LEZ's state machine for each name shape: the
/// largest states each handler decodes and re-encodes (32-byte names, records with a private receiver and a pending
/// offer, full lists of five such names, a 32-byte primary name, a pending commitment), a reveal that displaces a
/// registration, a `Forget` of 1 to 5 names. Writes `results/names-worst.md` (the worst shape of each action) and
/// checks that the clients' gas limit for each covers every measurement.
#[test]
fn worst_cases_fit_the_clients_gas() {
    // (action, names, cycles, shape, label)
    let mut rows: Vec<(String, usize, u64, String, String)> = Vec::new();
    let other = [0x11; 32];
    let t = T0;
    for shape in SHAPES {
        let name = |j: usize| long_name(shape, j);
        let mut push = |action: &str, n: usize, r: &TxReport| rows.push((action.into(), n, cycles(r), shape.into(), r.label.clone()));

        // Alice: four long names with receivers and a long primary name; a fifth by displacing bob's registration.
        let mut w = world();
        let alice = std::mem::replace(&mut w.alice, account(0xA1));
        let bob = std::mem::replace(&mut w.bob, account(0xB2));
        let carol = std::mem::replace(&mut w.carol, account(0xC3));
        let (a, b, c) = (alice.id, bob.id, carol.id);
        for j in 0..4 {
            let m = M::Register { name: name(j), now_ms: t, target: Some(a), receiver: Some(receiver(j as u8)), salt: SALT };
            ok(&w.act("alice registers a long name", &alice, &m, &[], t + LAG));
        }
        ok(&w.act("alice sets a long primary name", &alice, &M::SetPrimary { name: Some(name(0)), now_ms: t }, &[], t + LAG));
        let x = name(5);
        w.auto_commit = false;
        let t1 = t + 600_000;
        ok(&w.act("alice commits to x", &alice, &commit_of(&x, &alice, SALT, t1), &[], t1 + LAG));
        ok(&w.act("bob commits to x, after alice", &bob, &commit_of(&x, &bob, other, t1 + 1_000), &[], t1 + 1_000 + LAG));
        let open = t1 + 1_000 + LEAD;
        let theirs = M::Register { name: x.clone(), now_ms: open, target: Some(b), receiver: Some(receiver(5)), salt: other };
        ok(&w.act("bob reveals x first, with a receiver", &bob, &theirs, &[], open + LAG));
        ok(&w.act("bob offers x to carol", &bob, &M::Transfer { name: x.clone(), to: c, now_ms: open }, &[], open + LAG));
        let mine = M::Register { name: x.clone(), now_ms: open + 1_000, target: Some(a), receiver: Some(receiver(6)), salt: SALT };
        let r = w.act("alice's reveal displaces a record with a receiver and an offer; her fifth name", &alice, &mine, &[], open + 1_000 + LAG);
        assert!(emitted(&r, "Displaced"));
        push("Register", 1, &r);
        let t2 = open + 600_000;
        let r = w.act("alice commits, her state full", &alice, &commit_of("next", &alice, SALT, t2), &[], t2 + LAG);
        push("Commit", 0, &r);
        let r = w.act("alice commits again, replacing a pending commitment", &alice, &commit_of("later", &alice, SALT, t2), &[], t2 + LAG);
        push("Commit", 0, &r);
        let r = w.act("alice offers x to carol (a record with a receiver)", &alice, &M::Transfer { name: x.clone(), to: c, now_ms: t2 }, &[], t2 + LAG);
        push("Transfer", 0, &r);
        let r = w.act("alice offers x to carol again (a record with a receiver and an offer)", &alice, &M::Transfer { name: x.clone(), to: c, now_ms: t2 }, &[], t2 + LAG);
        push("Transfer", 0, &r);
        let r = w.act("alice renews x (a record with a receiver and an offer)", &alice, &M::Renew { name: x.clone(), now_ms: t2 }, &[], t2 + LAG);
        push("Renew", 0, &r);
        let set = M::SetTarget { name: x.clone(), target: Some(a), receiver: Some(receiver(7)), now_ms: t2 };
        let r = w.act("alice replaces x's receiver (a record with a receiver and an offer)", &alice, &set, &[], t2 + LAG);
        push("SetTarget", 0, &r);
        // Carol: four long names with receivers, a long primary name, a pending commitment; she accepts x.
        w.auto_commit = true;
        for j in 6..10 {
            let m = M::Register { name: name(j), now_ms: t2, target: Some(c), receiver: Some(receiver(j as u8)), salt: SALT };
            ok(&w.act("carol registers a long name", &carol, &m, &[], t2 + LAG));
        }
        ok(&w.act("carol sets a long primary name", &carol, &M::SetPrimary { name: Some(name(6)), now_ms: t2 }, &[], t2 + LAG));
        w.auto_commit = false;
        ok(&w.act("carol commits", &carol, &commit_of("pending", &carol, SALT, t2), &[], t2 + LAG));
        let r = w.act("carol accepts x (a record with a receiver; her fifth name; her state full)", &carol, &M::Accept { name: x.clone(), now_ms: t2 }, &[], t2 + LAG);
        push("Accept", 0, &r);
        let set = M::SetTarget { name: x.clone(), target: Some(c), receiver: Some(receiver(8)), now_ms: t2 };
        let r = w.act("carol adds a receiver to x", &carol, &set, &[], t2 + LAG);
        push("SetTarget", 0, &r);
        ok(&w.act("carol offers x to bob", &carol, &M::Transfer { name: x.clone(), to: b, now_ms: t2 }, &[], t2 + LAG));
        let r = w.act("carol makes x her primary name (a record with a receiver and an offer; her state full)", &carol, &M::SetPrimary { name: Some(x.clone()), now_ms: t2 }, &[], t2 + LAG);
        push("SetPrimary", 0, &r);
        let r = w.act("carol clears her primary name (her state full)", &carol, &M::SetPrimary { name: None, now_ms: t2 }, &[], t2 + LAG);
        push("SetPrimary", 0, &r);
        let r = w.act("carol releases x (a record with a receiver and an offer; her list full)", &carol, &M::Release { name: x.clone() }, &[], t2 + LAG);
        push("Release", 0, &r);

        // Forget of 1 to 5 names, each now another account's with a receiver and an offer, the forgetting account's
        // state full.
        for n in 1..=5 {
            let mut w = world();
            let mallory = std::mem::replace(&mut w.mallory, account(0xD4));
            let bob = std::mem::replace(&mut w.bob, account(0xB2));
            let (m, b, c) = (mallory.id, bob.id, w.carol.id);
            let names: Vec<String> = (10..15).map(name).collect();
            for (i, nm) in names.iter().enumerate() {
                let reg = M::Register { name: nm.clone(), now_ms: t, target: Some(m), receiver: Some(receiver(i as u8)), salt: SALT };
                ok(&w.act("mallory registers a long name", &mallory, &reg, &[], t + LAG));
            }
            ok(&w.act("mallory sets a long primary name", &mallory, &M::SetPrimary { name: Some(names[0].clone()), now_ms: t }, &[], t + LAG));
            for nm in &names {
                ok(&w.act("mallory offers it to bob", &mallory, &M::Transfer { name: nm.clone(), to: b, now_ms: t }, &[], t + LAG));
                ok(&w.act("bob accepts", &bob, &M::Accept { name: nm.clone(), now_ms: t }, &[], t + LAG));
                let set = M::SetTarget { name: nm.clone(), target: Some(b), receiver: Some(receiver(9)), now_ms: t };
                ok(&w.act("bob adds a receiver", &bob, &set, &[], t + LAG));
                ok(&w.act("bob offers it to carol", &bob, &M::Transfer { name: nm.clone(), to: c, now_ms: t }, &[], t + LAG));
            }
            w.auto_commit = false;
            ok(&w.act("mallory commits", &mallory, &commit_of("pending", &mallory, SALT, t), &[], t + LAG));
            let forget = M::Forget { names: names[..n].to_vec() };
            let r = w.act(&format!("mallory forgets {n} of her five lost names (records with a receiver and an offer)"), &mallory, &forget, &[], t + LAG);
            push("Forget", n, &r);
            assert_eq!(w.held(&mallory).len(), 5 - n);
        }
    }

    // The worst shape of each (action, names), and every measurement against the clients' limit.
    let mut keys: Vec<(String, usize)> = rows.iter().map(|r| (r.0.clone(), r.1)).collect();
    keys.dedup();
    keys.sort_by_key(|(a, n)| (["Commit", "Register", "Renew", "Transfer", "SetTarget", "Accept", "SetPrimary", "Release", "Forget"].iter().position(|x| x == a), *n));
    keys.dedup();
    let mut text = format!(
        "# ferrata_names: the worst case of every action, against the clients' gas limits\n\nGenerated by \
         `harness/tests/names.rs` (`worst_cases_fit_the_clients_gas`), LEZ's state machine; measured. Execution gas = \
         the guest's cycles. Each action measured with 32-byte names of {} shapes ({}); the table shows the costliest. \
         The limit is the one the clients send the action with (`names/js/names.js`, `gasFor`).\n\n\
         | action | names | worst case | shape | cycles | client's gas limit | fits |\n|---|---:|---|---|---:|---:|---|\n",
        SHAPES.len(),
        SHAPES.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>().join(", ")
    );
    for (action, n) in &keys {
        let worst = rows.iter().filter(|r| &r.0 == action && r.1 == *n).max_by_key(|r| r.2).unwrap();
        let limit = client_gas(action, *n);
        text.push_str(&format!(
            "| {action} | {n} | {} | `{}` | {} | {} | {} |\n",
            worst.4,
            worst.3,
            grouped(worst.2),
            grouped(limit),
            if worst.2 <= limit { "yes" } else { "**no**" }
        ));
    }
    fs::write(results_dir().join("names-worst.md"), text).unwrap();
    let over: Vec<String> = rows
        .iter()
        .filter(|r| r.2 > client_gas(&r.0, r.1))
        .map(|r| format!("{} ({} names, shape {}): {} cycles > {}", r.0, r.1, r.3, r.2, client_gas(&r.0, r.1)))
        .collect();
    assert!(over.is_empty(), "the clients' gas limit is below a measured worst case: {over:?}");
}
