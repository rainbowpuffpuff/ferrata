//! The review's program findings (23 November 2026), each measured the same way on `ferrata_msg`
//! (v1, the program live on the test network) and on `ferrata_msg_v2`: every check must fail on v1, as the review found
//! it, and pass on v2. So each fix has a test that fails before it.
//!
//! * M2: a send between two near-full inboxes must fit the clients' gas limit for a send.
//! * M3: strangers' messages must not raise what the recipient's own actions cost.
//! * L6: a "hybrid" envelope must not be stored in a bucket too small to hold a hybrid header.
//! * L7: at most 30 sends must be accepted within any hour (v1's fixed window let 59 through within 90 s).
//! * L8: the accounts it takes to occupy every stranger slot (v1: 4; v2: 8).
//!
//! Writes `results/messages-findings.md`.
//!
//! `RISC0_DEV_MODE=1 cargo test -p harness --features msg --locked --test messages_findings -- --nocapture`
#![cfg(feature = "msg")]

mod msg_common;

use std::fs;

use ferrata_lez_msg_v2_wire as v2;
use ferrata_lez_msg_wire as v1;
use harness::{results_dir, TxReport};
use lee::AccountId;
use msg_common::{client_gas, user, User, World, V1, V2};

/// The gas limit the CLI and the wallet used for a v1 send when the review measured (`client.js` before its fixes).
const V1_SEND_LIMIT_AT_REVIEW: u64 = 1_500_000;
const HOUR: u64 = 3_600_000;

trait Prog {
    const NAME: &'static str;
    const STRANGER_UNREAD: usize;
    fn world(&mut self) -> &mut World;
    fn publish(&mut self, label: &str, u: &User, hybrid: bool) -> TxReport;
    fn send_at(&mut self, label: &str, from: &User, to: AccountId, hybrid: bool, len: usize, now_ms: u64) -> TxReport;
    fn policy(&mut self, label: &str, u: &User, contacts_only: bool) -> TxReport;
    fn contacts(&mut self, label: &str, u: &User, add: &[&User]) -> TxReport;
    fn send(&mut self, label: &str, from: &User, to: AccountId, hybrid: bool, len: usize) -> TxReport {
        let now = self.world().now;
        self.send_at(label, from, to, hybrid, len, now)
    }
}

impl Prog for V1 {
    const NAME: &'static str = "ferrata_msg (v1, live)";
    const STRANGER_UNREAD: usize = v1::STRANGER_UNREAD;
    fn world(&mut self) -> &mut World {
        &mut self.0
    }
    fn publish(&mut self, label: &str, u: &User, hybrid: bool) -> TxReport {
        V1::publish(self, label, u, 0, hybrid)
    }
    fn send_at(&mut self, label: &str, from: &User, to: AccountId, hybrid: bool, len: usize, now_ms: u64) -> TxReport {
        V1::send_at(self, label, from, to, hybrid, 0, len, now_ms)
    }
    fn policy(&mut self, label: &str, u: &User, contacts_only: bool) -> TxReport {
        let policy = if contacts_only { v1::Policy::ContactsOnly } else { v1::Policy::Open };
        self.owner(label, u, v1::MsgMessage::SetPolicy { policy })
    }
    fn contacts(&mut self, label: &str, u: &User, add: &[&User]) -> TxReport {
        self.owner(label, u, v1::MsgMessage::Contacts { add: add.iter().map(|c| c.id.into_value()).collect(), remove: vec![] })
    }
}

impl Prog for V2 {
    const NAME: &'static str = "ferrata_msg_v2";
    const STRANGER_UNREAD: usize = v2::STRANGER_UNREAD;
    fn world(&mut self) -> &mut World {
        &mut self.0
    }
    fn publish(&mut self, label: &str, u: &User, hybrid: bool) -> TxReport {
        V2::publish(self, label, u, 0, hybrid)
    }
    fn send_at(&mut self, label: &str, from: &User, to: AccountId, hybrid: bool, len: usize, now_ms: u64) -> TxReport {
        V2::send_at(self, label, from, to, hybrid, 0, len, now_ms)
    }
    fn policy(&mut self, label: &str, u: &User, contacts_only: bool) -> TxReport {
        let policy = if contacts_only { v2::Policy::ContactsOnly } else { v2::Policy::Open };
        self.owner(label, u, v2::MsgMessage::SetPolicy { policy })
    }
    fn contacts(&mut self, label: &str, u: &User, add: &[&User]) -> TxReport {
        self.owner(label, u, v2::MsgMessage::Contacts { add: add.iter().map(|c| c.id.into_value()).collect(), remove: vec![] })
    }
}

fn ok(r: &TxReport) {
    assert!(r.ok(), "{}: exit {:?} {:?}", r.label, r.exit_code, r.charge.revert);
}

/// Everyone the measurements use.
struct Cast {
    bob: User,
    carol: User,
    carol2: User,
    alice: User,
    contacts: Vec<User>,
    strangers: Vec<User>,
}

impl Cast {
    fn new() -> Self {
        Cast {
            bob: user(2),
            carol: user(3),
            carol2: user(4),
            alice: user(1),
            contacts: (0..4).map(|i| user(300 + i)).collect(),
            strangers: (0..10).map(|i| user(100 + i)).collect(),
        }
    }
    fn all(&self) -> Vec<&User> {
        let mut v = vec![&self.bob, &self.carol, &self.carol2, &self.alice];
        v.extend(self.contacts.iter());
        v.extend(self.strangers.iter());
        v
    }
}

/// M2: bob's inbox at 64 KiB (16 envelopes of 4096 B from contacts), carol's at 60 KiB; bob sends 4096 B to carol.
fn m2<P: Prog>(p: &mut P, c: &Cast) -> u64 {
    let cs: Vec<&User> = c.contacts.iter().collect();
    for u in [&c.bob, &c.carol] {
        ok(&p.publish("publish (hybrid)", u, true));
        ok(&p.contacts("add the four contacts", u, &cs));
    }
    for i in 0..16 {
        ok(&p.send("contact -> bob, 4096 B", &c.contacts[i % 4], c.bob.id, true, 4096));
    }
    for i in 0..15 {
        ok(&p.send("contact -> carol, 4096 B", &c.contacts[i % 4], c.carol.id, true, 4096));
    }
    let r = p.send("bob (64 KiB inbox) -> carol (60 KiB inbox), 4096 B", &c.bob, c.carol.id, true, 4096);
    ok(&r);
    r.charge.cycles
}

/// M3: bob's `SetPolicy` and his own send (to a fresh recipient), with or without 32 KiB of strangers' messages in
/// his inbox, every other step the same.
fn m3<P: Prog>(p: &mut P, c: &Cast, strangers: bool) -> (u64, u64) {
    for u in [&c.bob, &c.carol, &c.carol2] {
        ok(&p.publish("publish (X25519)", u, false));
    }
    ok(&p.policy("bob: SetPolicy", &c.bob, false));
    ok(&p.send("bob -> carol", &c.bob, c.carol.id, false, 512));
    if strangers {
        for i in 0..8 {
            ok(&p.send("stranger -> bob, 4096 B", &c.strangers[i / P::STRANGER_UNREAD], c.bob.id, false, 4096));
        }
    }
    let x = p.policy("bob: SetPolicy (measured)", &c.bob, false);
    let y = p.send("bob -> carol2 (measured)", &c.bob, c.carol2.id, false, 512);
    ok(&x);
    ok(&y);
    (x.charge.cycles, y.charge.cycles)
}

/// L6: a hybrid-version envelope of 512 bytes; was it stored?
fn l6<P: Prog>(p: &mut P, c: &Cast) -> bool {
    ok(&p.publish("bob publishes (hybrid)", &c.bob, true));
    p.send("alice -> bob, hybrid version in 512 B", &c.alice, c.bob.id, true, 512).ok()
}

/// L7: one send at t0, 29 a minute before t0 + 1 h, then 30 more from t0 + 1 h: how many land in those 90 seconds?
fn l7<P: Prog>(p: &mut P, c: &Cast) -> usize {
    let cs: Vec<&User> = vec![&c.alice];
    for u in [&c.bob, &c.carol, &c.carol2] {
        ok(&p.publish("publish (X25519)", u, false));
        ok(&p.contacts("add alice", u, &cs));
    }
    let t0 = p.world().now;
    ok(&p.send_at("alice -> bob at t0", &c.alice, c.bob.id, false, 512, t0));
    let to = [c.bob.id, c.carol.id, c.carol2.id];
    let mut landed = 0;
    for i in 0..59u64 {
        let at = if i < 29 { t0 + HOUR - 60_000 + i } else { t0 + HOUR + i };
        p.world().now = at;
        if p.send_at("alice, around t0 + 1 h", &c.alice, to[(i % 3) as usize], false, 512, at).ok() {
            landed += 1;
        }
    }
    landed
}

/// L8: distinct strangers sending as much as each may, until a new stranger is refused.
fn l8<P: Prog>(p: &mut P, c: &Cast) -> usize {
    ok(&p.publish("bob publishes (X25519)", &c.bob, false));
    for (i, s) in c.strangers.iter().enumerate() {
        for _ in 0..P::STRANGER_UNREAD {
            let r = p.send("stranger -> bob, 512 B", s, c.bob.id, false, 512);
            if !r.ok() {
                return i;
            }
        }
    }
    c.strangers.len()
}

struct Found {
    m2: u64,
    m3: [u64; 4],
    l6: bool,
    l7: usize,
    l8: usize,
}

fn measure<P: Prog>(new: impl Fn(&[&User]) -> P) -> Found {
    let c = Cast::new();
    let fresh = || new(&c.all());
    Found {
        m2: m2(&mut fresh(), &c),
        m3: {
            let (x0, y0) = m3(&mut fresh(), &c, false);
            let (x1, y1) = m3(&mut fresh(), &c, true);
            [x0, x1, y0, y1]
        },
        l6: l6(&mut fresh(), &c),
        l7: l7(&mut fresh(), &c),
        l8: l8(&mut fresh(), &c),
    }
}

#[test]
fn findings_fail_on_v1_and_pass_on_v2() {
    let gas_v1 = client_gas("GAS_V1");
    let gas_v2 = client_gas("GAS_V2");
    let a = measure(V1::new);
    let b = measure(V2::new);
    let ratio = |x: u64, y: u64| y as f64 / x as f64;
    let rows = [
        (
            "M2 send between near-full inboxes fits the send limit",
            format!("{} cycles > {} (limit at review); GAS_V1.send {}", a.m2, V1_SEND_LIMIT_AT_REVIEW, gas_v1["send"]),
            format!("{} cycles <= GAS_V2.send {}", b.m2, gas_v2["send"]),
        ),
        (
            "M3 owner's SetPolicy without / with 32 KiB of strangers' messages",
            format!("{} -> {} ({:.1}x)", a.m3[0], a.m3[1], ratio(a.m3[0], a.m3[1])),
            format!("{} -> {} ({:.2}x)", b.m3[0], b.m3[1], ratio(b.m3[0], b.m3[1])),
        ),
        (
            "M3 owner's own send without / with 32 KiB of strangers' messages",
            format!("{} -> {} ({:.1}x)", a.m3[2], a.m3[3], ratio(a.m3[2], a.m3[3])),
            format!("{} -> {} ({:.2}x)", b.m3[2], b.m3[3], ratio(b.m3[2], b.m3[3])),
        ),
        ("L6 hybrid envelope in 512 B refused", format!("stored: {}", a.l6), format!("stored: {}", b.l6)),
        ("L7 at most 30 sends within any hour", format!("{} landed in 90 s", a.l7), format!("{} landed in 90 s", b.l7)),
        ("L8 accounts to occupy every stranger slot", format!("{}", a.l8), format!("{}", b.l8)),
    ];
    let mut md = String::from(
        "# The review's program findings on v1 (live) and v2 (harness/tests/messages_findings.rs)\n\n\
         Measured in LEZ's state machine (RISC0_DEV_MODE executor), the same scenario on both programs. Every check fails \
         on `ferrata_msg` (v1) and passes on `ferrata_msg_v2`.\n\n| check | v1 | v2 |\n|---|---|---|\n",
    );
    for (check, x, y) in &rows {
        md += &format!("| {check} | {x} | {y} |\n");
    }
    println!("{md}");
    fs::write(results_dir().join("messages-findings.md"), &md).unwrap();

    // v1, as the review found it.
    assert!(a.m2 > V1_SEND_LIMIT_AT_REVIEW, "M2 on v1: {}", a.m2);
    assert!(a.m2 <= gas_v1["send"], "the clients' v1 limit must now cover it: {} > {}", a.m2, gas_v1["send"]);
    assert!(ratio(a.m3[0], a.m3[1]) > 4.0 && ratio(a.m3[2], a.m3[3]) > 4.0, "M3 on v1: {:?}", a.m3);
    assert!(a.l6, "L6 on v1");
    assert!(a.l7 > 30, "L7 on v1: {}", a.l7);
    assert_eq!(a.l8, 4, "L8 on v1");
    // v2: fixed.
    assert!(b.m2 <= gas_v2["send"], "M2 on v2: {} > {}", b.m2, gas_v2["send"]);
    assert!(ratio(b.m3[0], b.m3[1]) <= 1.01 && ratio(b.m3[2], b.m3[3]) <= 1.01, "M3 on v2: the owner's costs must not move: {:?}", b.m3);
    assert!(!b.l6, "L6 on v2");
    assert_eq!(b.l7, 30 - 1 + 1, "L7 on v2: 29 before the hour of the first send, then one as it leaves the window");
    assert_eq!(b.l8, v2::STRANGER_SLOTS / v2::STRANGER_UNREAD, "L8 on v2");
}
