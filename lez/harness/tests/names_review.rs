//! The findings of the independent review of `ferrata_names` (23 November 2026) as tests of the
//! behaviour the program must have. Written first against the program as reviewed (v1, image `0xee9578ca…f790`),
//! where they fail; the fixes (v2) make them pass. M1 and L13 are the review's own cases.
//!
//! The log of the first run, on v1: `names/e2e/results/review-before/harness.log` (8 of 9 fail; BAD_STATE was already
//! reachable). Writes `results/names-review.md`.
//!
//! `RISC0_DEV_MODE=1 cargo test -p harness --features names --test names_review -- --nocapture --test-threads 1`
#![cfg(feature = "names")]

use std::{borrow::Cow, fs, sync::Mutex};

use ferrata_lez_names_wire::{
    code, commitment, name_actor, name_seed, sequence_actor, sequence_seed, user_actor, NameRecord, NamesMessage as M,
    UserState, CLOCK_SLACK_MS, COMMIT_SLACK_MS, GRACE_MS, MIN_REVEAL_DELAY_MS, TERM_MS,
};
use harness::{grouped, results_dir, Chain, TxReport};
use lee::{program::Program, Account, AccountId, Actor, PrivateKey, PublicAccountEvidence, PublicKey, V03State};

const T0: u64 = 1_800_000_000_000;

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
}
fn world(users: &[&User]) -> World {
    let mut chain = Chain::new();
    let state = std::mem::replace(&mut chain.state, V03State::new());
    let funded: Vec<_> = users.iter().map(|u| (u.id, Account::funded(1))).collect();
    let names = Program::new_unchecked(methods_names::FERRATA_NAMES_ID, Cow::Borrowed(methods_names::FERRATA_NAMES_ELF));
    chain.state = state.with_public_accounts(funded).with_genesis_programs([(names, true)]);
    World { chain }
}
impl World {
    /// `message` signed by `who` at block time `time`, declaring `user(who)`, the name's actor (with its PDA evidence
    /// while it holds no record) and `extra`.
    fn act_with(&mut self, label: &str, who: &User, message: &M, extra: &[Actor], time: u64) -> TxReport {
        let p = program();
        let root = user_actor(p, who.id);
        let mut actors = vec![root];
        let mut evidence = vec![];
        let pda = match message {
            M::Commit { .. } => Some((sequence_actor(p), sequence_seed())),
            m => m.name().map(|n| (name_actor(p, n), name_seed(n))),
        };
        if let Some((at, seed)) = pda {
            actors.push(at);
            if self.chain.actor_state(at).is_empty() {
                evidence.push(PublicAccountEvidence::Pda { program: p, seed });
            }
        }
        actors.extend_from_slice(extra);
        let r = self.chain.submit_signed(label, root, borsh::to_vec(message).unwrap(), actors, evidence, time, &[&who.key]);
        println!("ROW | t+{:>10} ms | {label} | ok {} | exit {:?} | revert {:?}", time - T0, r.ok(), r.exit_code, r.charge.revert);
        record(&r);
        r
    }
    fn act(&mut self, label: &str, who: &User, message: &M, time: u64) -> TxReport {
        self.act_with(label, who, message, &[], time)
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
            borsh::from_slice(&s).unwrap()
        }
    }
    /// Commit at `now_ms` (block 2 s later) and reveal at the commitment's first ms (block 2 s later).
    fn register(&mut self, who: &User, name: &str, salt: [u8; 32], now_ms: u64) -> TxReport {
        let c = M::Commit { commitment: commitment(name, who.id, &salt), now_ms };
        assert!(self.act(&format!("commit to `{name}`"), who, &c, now_ms + 2_000).ok());
        let open = self.user(who).commit.expect("a pending commitment").not_before_ms;
        self.act(&format!("reveal `{name}`"), who, &reveal(name, open, who.id, salt), open + 2_000)
    }
}
/// Every transaction of every test, for `results/names-review.md` (tests run one at a time: `--test-threads 1`).
static ROWS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn record(r: &TxReport) {
    let cycles: u64 = r.transitions.iter().map(|t| t.cycles).sum();
    let result = match (r.exit_code, &r.charge.revert) {
        (Some(c), _) => format!("refused: exit {c} ({})", code::ALL.iter().find(|(_, v)| u32::from(*v) == c).map_or("?", |(n, _)| n)),
        (None, Some(revert)) => format!("reverted: {revert}"),
        (None, None) => "applied".into(),
    };
    ROWS.lock().unwrap().push(format!("| {} | {} | {} | {} |", r.label, grouped(cycles), grouped(r.charge.fee_base as u64), result));
}

fn write_rows(test: &str) {
    let mut rows = ROWS.lock().unwrap();
    let path = results_dir().join("names-review.md");
    let mut text = fs::read_to_string(&path).unwrap_or_default();
    if text.is_empty() || test == FIRST {
        text = "# ferrata_names: the review's cases (v2)\n\nGenerated by `harness/tests/names_review.rs`, LEZ's state machine and fee \
                settlement; measured.\n"
            .into();
    }
    text.push_str(&format!("\n## {test}\n\n| transaction | cycles | base fee | result |\n|---|---:|---:|---|\n{}\n", rows.join("\n")));
    rows.clear();
    fs::write(path, text).unwrap();
}

/// The first test in name order (the order `cargo test` runs them one at a time): it starts the file afresh.
const FIRST: &str = "l12_look_alikes_are_refused";

fn reveal(name: &str, now_ms: u64, target: AccountId, salt: [u8; 32]) -> M {
    M::Register { name: name.into(), now_ms, target: Some(target), receiver: None, salt }
}
fn ok(r: &TxReport) {
    assert!(r.ok(), "{}: exit {:?}, {:?}", r.label, r.exit_code, r.charge.revert);
}

/// M1. Alice's reveal is sent too early: refused (TOO_EARLY), but the transaction is in a block, name and all.
/// Mallory reads it, commits with `now_ms` as far back as the commit window allows, and reveals when her window opens,
/// before alice's. Alice's commitment is older than mallory's: when alice reveals in her window, she gets the name.
#[test]
fn m1_a_refused_reveal_does_not_hand_the_name_to_a_copier() {
    let (alice, mallory) = (account(0xA1), account(0xD4));
    let mut w = world(&[&alice, &mallory]);
    let (salt_a, salt_m) = ([0x5a; 32], [0x11; 32]);
    let c = T0;
    ok(&w.act("alice commits to `alice`", &alice, &M::Commit { commitment: commitment("alice", alice.id, &salt_a), now_ms: c }, c + 10_000));
    let alice_window = w.user(&alice).commit.unwrap().not_before_ms;
    let t_leak = c + 20_000;
    let r = w.act("alice reveals too early (refused; the name is now in a block)", &alice, &reveal("alice", t_leak, alice.id, salt_a), t_leak + 2_000);
    assert_eq!(r.exit_code, Some(u32::from(code::TOO_EARLY)));
    let seen = t_leak + 4_000;
    let m_now = seen - COMMIT_SLACK_MS + 1;
    ok(&w.act("mallory commits to `alice` (now_ms 2 min back)", &mallory, &M::Commit { commitment: commitment("alice", mallory.id, &salt_m), now_ms: m_now }, seen));
    let mallory_window = w.user(&mallory).commit.unwrap().not_before_ms;
    assert!(mallory_window < alice_window, "the copier's window still opens first");
    let _ = w.act("mallory reveals `alice` when her window opens", &mallory, &reveal("alice", mallory_window, mallory.id, salt_m), mallory_window + 2_000);
    let r = w.act("alice reveals when her window opens", &alice, &reveal("alice", alice_window, alice.id, salt_a), alice_window + 2_000);
    ok(&r);
    assert_eq!(w.record("alice").unwrap().owner, alice.id, "the older commitment wins");
    assert_eq!(w.user(&mallory).names, vec!["alice".to_string()], "mallory keeps it in her list until she forgets it");
    write_rows("m1_a_refused_reveal_does_not_hand_the_name_to_a_copier");
}

/// M1, second case: alice's first reveal in her window is refused for another reason (TOO_MANY, she holds five), she
/// releases a name and reveals again 3 minutes later, after a copier who read the name has revealed.
#[test]
fn m1_an_owner_whose_reveal_was_refused_can_still_take_the_name() {
    let (alice, mallory) = (account(0xA1), account(0xD4));
    let mut w = world(&[&alice, &mallory]);
    let salt = [0x5a; 32];
    let mut t = T0;
    for i in 0..5 {
        ok(&w.register(&alice, &format!("held-{i}"), salt, t));
        t += COMMIT_SLACK_MS + MIN_REVEAL_DELAY_MS + 10_000;
    }
    ok(&w.act("alice commits to `prize`", &alice, &M::Commit { commitment: commitment("prize", alice.id, &salt), now_ms: t }, t + 2_000));
    let open = w.user(&alice).commit.unwrap().not_before_ms;
    let r = w.act("alice reveals `prize` (refused TOO_MANY; the name is now public)", &alice, &reveal("prize", open, alice.id, salt), open + 2_000);
    assert_eq!(r.exit_code, Some(u32::from(code::TOO_MANY)));
    let seen = open + 4_000;
    let salt_m = [0x11; 32];
    ok(&w.act("mallory commits to `prize`", &mallory, &M::Commit { commitment: commitment("prize", mallory.id, &salt_m), now_ms: seen - COMMIT_SLACK_MS + 1 }, seen));
    let m_open = w.user(&mallory).commit.unwrap().not_before_ms;
    let _ = w.act("mallory reveals `prize` when her window opens", &mallory, &reveal("prize", m_open, mallory.id, salt_m), m_open + 1_000);
    ok(&w.act("alice releases held-0", &alice, &M::Release { name: "held-0".into() }, m_open + 30_000));
    let late = m_open + 40_000;
    let r = w.act("alice reveals `prize` again, 3 minutes after her first attempt", &alice, &reveal("prize", late, alice.id, salt), late + 1_000);
    ok(&r);
    assert_eq!(w.record("prize").unwrap().owner, alice.id);
    write_rows("m1_an_owner_whose_reveal_was_refused_can_still_take_the_name");
}

/// L13. A deadline holds for the block, not for `now_ms`: a renewal whose validity window reaches past the end of the
/// grace period is refused (the review's case landed 599 s after it).
#[test]
fn l13_no_renewal_lands_after_the_grace_period() {
    let alice = account(0xA1);
    let mut w = world(&[&alice]);
    ok(&w.register(&alice, "zed", [0x5a; 32], T0));
    let expires = w.record("zed").unwrap().expires_ms;
    let end = expires + GRACE_MS;
    let block = end + CLOCK_SLACK_MS - 1_000;
    let r = w.act("renew with now_ms 1 ms before the grace period ends, landing 9 min 59 s after it", &alice, &M::Renew { name: "zed".into(), now_ms: end - 1 }, block);
    assert!(!r.ok(), "a renewal landed {} s after the grace period ended", (block - end) / 1000);
    assert_eq!(r.exit_code, Some(u32::from(code::LAPSED)));
    assert_eq!(w.record("zed").unwrap().expires_ms, expires);
    let _ = TERM_MS;
    write_rows("l13_no_renewal_lands_after_the_grace_period");
}

/// L13 for the expiry: no transfer offer lands after the name has expired.
#[test]
fn l13_no_offer_lands_after_expiry() {
    let (alice, bob) = (account(0xA1), account(0xB2));
    let mut w = world(&[&alice, &bob]);
    ok(&w.register(&alice, "zed", [0x5a; 32], T0));
    let expires = w.record("zed").unwrap().expires_ms;
    let block = expires + CLOCK_SLACK_MS - 1_000;
    let r = w.act("offer with now_ms 1 ms before expiry, landing 9 min 59 s after it", &alice, &M::Transfer { name: "zed".into(), to: bob.id, now_ms: expires - 1 }, block);
    assert!(!r.ok(), "an offer landed after expiry");
    assert_eq!(r.exit_code, Some(u32::from(code::EXPIRED)));
    write_rows("l13_no_offer_lands_after_expiry");
}

/// L12. One name per look-alike: `a1ice` (digit one) cannot be registered beside `alice`, nor `rnallory` beside
/// `mallory`.
#[test]
fn l12_look_alikes_are_refused() {
    let (alice, bob) = (account(0xA1), account(0xB2));
    let mut w = world(&[&alice, &bob]);
    let mut t = T0;
    // (The first run had `vvallet-co`/`wallet-co` as the third pair; v2 reserves both as variants of `wallet`.)
    for (name, copy) in [("alice", "a1ice"), ("mallory", "rnallory"), ("woody", "vvoody")] {
        ok(&w.register(&alice, name, [0x5a; 32], t));
        t += 600_000;
        let r = w.register(&bob, copy, [0x11; 32], t);
        assert!(!r.ok(), "`{copy}` registered beside `{name}`");
        assert_eq!(r.exit_code, Some(u32::from(code::CONFUSABLE)));
        t += 600_000;
    }
    write_rows("l12_look_alikes_are_refused");
}

/// L12. Variants of reserved names are reserved: a digit added, digits for letters, a hyphenated prefix or suffix.
#[test]
fn l12_variants_of_reserved_names_are_refused() {
    let bob = account(0xB2);
    let mut w = world(&[&bob]);
    let mut t = T0;
    for variant in ["faucet1", "wa11et", "ferrata-team", "team-ferrata", "2faucet", "0fficial", "support-desk"] {
        let r = w.register(&bob, variant, [0x11; 32], t);
        assert!(!r.ok(), "`{variant}` registered");
        assert_eq!(r.exit_code, Some(u32::from(code::RESERVED_NAME)));
        t += 600_000;
    }
    write_rows("l12_variants_of_reserved_names_are_refused");
}

/// L14. A name leaving an account without that account signing must not need that account's actor in the
/// transaction (a private account's actor cannot be admitted in a public transaction): here `Accept` declares only
/// the recipient and the name.
#[test]
fn l14_accept_does_not_need_the_old_owners_actor() {
    let (alice, bob) = (account(0xA1), account(0xB2));
    let mut w = world(&[&alice, &bob]);
    ok(&w.register(&alice, "gift", [0x5a; 32], T0));
    let t = T0 + 600_000;
    ok(&w.act("alice offers `gift` to bob", &alice, &M::Transfer { name: "gift".into(), to: bob.id, now_ms: t }, t + 2_000));
    let r = w.act("bob accepts, his actor and the name's only", &bob, &M::Accept { name: "gift".into(), now_ms: t }, t + 4_000);
    ok(&r);
    assert_eq!(w.record("gift").unwrap().owner, bob.id);
    write_rows("l14_accept_does_not_need_the_old_owners_actor");
}

/// L14. The same for a lapsed name registered afresh: the old owner's actor is not in the transaction.
#[test]
fn l14_a_lapsed_name_is_taken_without_the_old_owners_actor() {
    let (alice, bob) = (account(0xA1), account(0xB2));
    let mut w = world(&[&alice, &bob]);
    ok(&w.register(&alice, "zed", [0x5a; 32], T0));
    let lapsed = w.record("zed").unwrap().expires_ms + GRACE_MS + 1_000;
    let r = w.register(&bob, "zed", [0x11; 32], lapsed);
    ok(&r);
    assert_eq!(w.record("zed").unwrap().owner, bob.id);
    write_rows("l14_a_lapsed_name_is_taken_without_the_old_owners_actor");
}

/// M4: BAD_STATE (241) is defensive; reached here with a name record that does not decode, put in genesis.
#[test]
fn m4_bad_state_is_reached_with_a_corrupt_record() {
    let alice = account(0xA1);
    let p = program();
    let mut chain = Chain::new();
    let state = std::mem::replace(&mut chain.state, V03State::new());
    let corrupt = Account::default().with_actor_state(p, vec![0xff, 0x00, 0x01].into());
    let names = Program::new_unchecked(methods_names::FERRATA_NAMES_ID, Cow::Borrowed(methods_names::FERRATA_NAMES_ELF));
    chain.state = state
        .with_public_accounts([(alice.id, Account::funded(1)), (name_actor(p, "broken").account_id, corrupt)])
        .with_genesis_programs([(names, true)]);
    let mut w = World { chain };
    let r = w.act("renew `broken`, whose record does not decode", &alice, &M::Renew { name: "broken".into(), now_ms: T0 }, T0 + 2_000);
    assert_eq!(r.exit_code, Some(u32::from(code::BAD_STATE)), "{:?}", r.charge.revert);
    write_rows("m4_bad_state_is_reached_with_a_corrupt_record");
}
