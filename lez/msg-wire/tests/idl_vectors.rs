//! The vectors `idl/check.py` decodes with `idl/ferrata_msg.idl.json` alone (v1, the program live on the test network):
//! one per message variant (no wildcard arm below), the actor state, each event, and the program's codes. Review
//! finding M4: until these, no committed input backed the IDL's claim.
//!
//! `FERRATA_IDL_WRITE=1 cargo test -p ferrata-lez-msg-wire --test idl_vectors` rewrites the file; without it the test
//! fails if the committed file differs from what the wire encodes now.

use ferrata_lez_msg_wire::{code, Delivered, KeyPublished, Mailbox, MsgKey, MsgMessage as M, Policy, Stored};

fn r(n: u8) -> [u8; 32] {
    core::array::from_fn(|i| n.wrapping_mul(7).wrapping_add(i as u8))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn label(m: &M) -> &'static str {
    match m {
        M::Publish { .. } => "Publish",
        M::SetPolicy { .. } => "SetPolicy",
        M::Contacts { .. } => "Contacts",
        M::Block { .. } => "Block",
        M::Send { .. } => "Send",
        M::Deliver { .. } => "Deliver",
        M::Delete { .. } => "Delete",
        M::Clear { .. } => "Clear",
    }
}

#[test]
fn idl_vectors() {
    let hybrid = MsgKey { epoch: 1, x25519: r(1), mlkem768: Some((0..1184).map(|i| (i % 251) as u8).collect()) };
    let messages = vec![
        M::Publish { key: hybrid.clone() },
        M::Publish { key: MsgKey { epoch: 0, x25519: r(2), mlkem768: None } },
        M::SetPolicy { policy: Policy::Open },
        M::SetPolicy { policy: Policy::ContactsOnly },
        M::Contacts { add: vec![r(3)], remove: vec![r(4)] },
        M::Block { add: vec![], remove: vec![r(5)] },
        M::Send { to: r(6), now_ms: 1_790_000_000_000, body: vec![2, 0, 0, 0, 0, 1] },
        M::Deliver { now_ms: 1_790_000_000_000, body: vec![1, 0, 0, 0, 0, 1] },
        M::Delete { seqs: vec![0, 4] },
        M::Clear { upto: 7 },
    ];
    let mut rows: Vec<(String, &str, Vec<u8>, String)> = Vec::new();
    let mut seen = std::collections::BTreeMap::<&str, usize>::new();
    for m in &messages {
        let n = seen.entry(label(m)).or_default();
        *n += 1;
        rows.push((format!("MsgMessage::{}#{n}", label(m)), "MsgMessage", borsh::to_vec(m).unwrap(), format!("{m:?}")));
    }
    let mailbox = Mailbox {
        key: Some(hybrid),
        policy: Policy::ContactsOnly,
        contacts: vec![r(7)],
        blocked: vec![r(8)],
        next_seq: 2,
        inbox: vec![Stored { seq: 1, from: r(9), at_ms: 1_790_000_000_000, body: vec![2, 1, 0, 0, 0, 5] }],
        window_start_ms: 1_790_000_000_000,
        sent_in_window: 3,
        last_send_ms: 1_790_000_000_500,
        sent_total: 9,
    };
    rows.push(("Mailbox#1".into(), "Mailbox", borsh::to_vec(&mailbox).unwrap(), format!("{mailbox:?}")));
    let d = Delivered { from: r(9), to: r(10), seq: 1, len: 2048 };
    rows.push(("Delivered#1".into(), "Delivered", borsh::to_vec(&d).unwrap(), format!("{d:?}")));
    let k = KeyPublished { account: r(10), epoch: 1, hybrid: true };
    rows.push(("KeyPublished#1".into(), "KeyPublished", borsh::to_vec(&k).unwrap(), format!("{k:?}")));
    let mut s = String::from(
        "{\n  \"program\": \"ferrata_msg\",\n  \"generator\": \"lez/msg-wire/tests/idl_vectors.rs (FERRATA_IDL_WRITE=1 to rewrite)\",\n  \"encoding\": \"borsh 1.x, as the program decodes its messages and its actor state\",\n  \"program_codes\": [\n",
    );
    for (i, (c, n)) in code::ALL.iter().enumerate() {
        s += &format!("    [\"{n}\", {c}]{}\n", if i + 1 < code::ALL.len() { "," } else { "" });
    }
    s += "  ],\n  \"vectors\": [\n";
    for (i, (name, ty, bytes, debug)) in rows.iter().enumerate() {
        assert!(!debug.contains('"') && !debug.contains('\\'));
        s += &format!(
            "    {{\"name\": \"{name}\", \"type\": \"{ty}\", \"hex\": \"{}\",\n     \"debug\": \"{debug}\"}}{}\n",
            hex(bytes),
            if i + 1 < rows.len() { "," } else { "" }
        );
    }
    s += "  ]\n}\n";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../idl/vectors/ferrata_msg.vectors.json");
    if std::env::var("FERRATA_IDL_WRITE").as_deref() == Ok("1") {
        std::fs::write(&path, &s).unwrap();
    } else {
        let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(committed, s, "the v1 IDL vectors differ from the wire: rerun with FERRATA_IDL_WRITE=1");
    }
}
