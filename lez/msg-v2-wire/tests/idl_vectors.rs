//! The vectors `idl/check.py` decodes with `idl/ferrata_msg_v2.idl.json` alone: one per message variant (the match
//! below has no wildcard arm, so a new variant stops this test from compiling until it has a vector and an IDL entry),
//! each actor state, each event, and the program's codes; and the PDAs it rederives from the IDL's documented seeds.
//!
//! `FERRATA_IDL_WRITE=1 cargo test -p ferrata-lez-msg-v2-wire --test idl_vectors` rewrites the files; without it the
//! test fails if the committed files differ from what the wire encodes now.

use ferrata_lez_msg_v2_wire::{
    code, envelope_account, inbox_account, Delivered, Entry, Envelope, Inbox, KeyPublished, MsgKey, MsgMessage as M,
    Policy, Profile,
};

fn r(n: u8) -> [u8; 32] {
    core::array::from_fn(|i| n.wrapping_mul(7).wrapping_add(i as u8))
}

struct Vector {
    name: String,
    ty: &'static str,
    hex: String,
    debug: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn vector<T: borsh::BorshSerialize + core::fmt::Debug>(name: &str, ty: &'static str, value: &T) -> Vector {
    let debug = format!("{value:?}");
    assert!(!debug.contains('"') && !debug.contains('\\'), "{name}: Debug text needs JSON escaping");
    Vector { name: name.to_string(), ty, hex: hex(&borsh::to_vec(value).unwrap()), debug }
}

fn label(m: &M) -> &'static str {
    match m {
        M::Publish { .. } => "Publish",
        M::SetPolicy { .. } => "SetPolicy",
        M::Contacts { .. } => "Contacts",
        M::Block { .. } => "Block",
        M::Send { .. } => "Send",
        M::Delete { .. } => "Delete",
        M::Clear { .. } => "Clear",
        M::Deliver { .. } => "Deliver",
        M::Open => "Open",
        M::Admit { .. } => "Admit",
        M::Store { .. } => "Store",
        M::Remove { .. } => "Remove",
        M::RemoveUpTo { .. } => "RemoveUpTo",
        M::Erase => "Erase",
    }
}

fn file(program: &str, codes: &[(&str, u8)], vectors: &[Vector]) -> String {
    let mut s = format!(
        "{{\n  \"program\": \"{program}\",\n  \"generator\": \"lez/msg-v2-wire/tests/idl_vectors.rs (FERRATA_IDL_WRITE=1 to rewrite)\",\n  \"encoding\": \"borsh 1.x, as the program decodes its messages and its actor states\",\n  \"program_codes\": [\n"
    );
    for (i, (name, c)) in codes.iter().enumerate() {
        s += &format!("    [\"{name}\", {c}]{}\n", if i + 1 < codes.len() { "," } else { "" });
    }
    s += "  ],\n  \"vectors\": [\n";
    for (i, v) in vectors.iter().enumerate() {
        s += &format!(
            "    {{\"name\": \"{}\", \"type\": \"{}\", \"hex\": \"{}\",\n     \"debug\": \"{}\"}}{}\n",
            v.name,
            v.ty,
            v.hex,
            v.debug,
            if i + 1 < vectors.len() { "," } else { "" }
        );
    }
    s + "  ]\n}\n"
}

fn check_or_write(name: &str, text: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../idl/vectors").join(name);
    if std::env::var("FERRATA_IDL_WRITE").as_deref() == Ok("1") {
        std::fs::write(&path, text).unwrap();
        eprintln!("wrote {}", path.display());
    } else {
        let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(committed, text, "{name} differs from the wire: rerun with FERRATA_IDL_WRITE=1");
    }
}

#[test]
fn idl_vectors() {
    let hybrid = MsgKey { epoch: 2, x25519: r(1), mlkem768: Some((0..1184).map(|i| (i % 251) as u8).collect()) };
    let classic = MsgKey { epoch: 0, x25519: r(2), mlkem768: None };
    let messages = vec![
        M::Publish { key: hybrid.clone() },
        M::Publish { key: classic.clone() },
        M::SetPolicy { policy: Policy::Open },
        M::SetPolicy { policy: Policy::ContactsOnly },
        M::Contacts { add: vec![r(3), r(4)], remove: vec![r(5)] },
        M::Block { add: vec![r(6)], remove: vec![] },
        M::Send { to: r(7), now_ms: 1_790_000_000_000, seq: 4, body: vec![4, 2, 0, 0, 0, 9, 9] },
        M::Delete { ns: vec![1, 3, 8] },
        M::Clear { upto: 12 },
        M::Deliver { now_ms: 1_790_000_000_001, seq: 4, body: vec![3, 0, 0, 0, 0, 1] },
        M::Open,
        M::Admit { from: r(8), seq: 4, at_ms: 1_790_000_000_001, len: 2048, contact: false },
        M::Store { from: r(8), seq: 4, at_ms: 1_790_000_000_001, body: vec![4, 0, 0, 0, 0, 7] },
        M::Remove { ns: vec![2, 5] },
        M::RemoveUpTo { upto: 30 },
        M::Erase,
    ];
    let mut vectors: Vec<Vector> = Vec::new();
    let mut seen = std::collections::BTreeMap::<&str, usize>::new();
    for m in &messages {
        let n = seen.entry(label(m)).or_default();
        *n += 1;
        vectors.push(vector(&format!("MsgMessage::{}#{n}", label(m)), "MsgMessage", m));
    }
    let profile = Profile {
        key: Some(hybrid),
        policy: Policy::ContactsOnly,
        contacts: vec![r(9)],
        blocked: vec![r(10), r(11)],
        sent_at: vec![1_790_000_000_000, 1_790_000_060_000],
        last_send_ms: 1_790_000_060_000,
        sent_total: 17,
    };
    vectors.push(vector("Profile#1", "Profile", &profile));
    vectors.push(vector("Profile#2 (empty)", "Profile", &Profile::default()));
    let inbox = Inbox {
        owner: r(12),
        next_n: 3,
        entries: vec![
            Entry { n: 0, from: r(13), seq: 0, at_ms: 1_790_000_000_000, len: 512, contact: true },
            Entry { n: 2, from: r(14), seq: 9, at_ms: 1_790_000_000_500, len: 4096, contact: false },
        ],
    };
    vectors.push(vector("Inbox#1", "Inbox", &inbox));
    let envelope = Envelope { recipient: r(12), from: r(14), seq: 9, at_ms: 1_790_000_000_500, body: vec![4, 0, 0, 0, 0, 1, 2, 3] };
    vectors.push(vector("Envelope#1", "Envelope", &envelope));
    vectors.push(vector("Delivered#1", "Delivered", &Delivered { from: r(14), to: r(12), n: 2, len: 4096 }));
    vectors.push(vector("KeyPublished#1", "KeyPublished", &KeyPublished { account: r(12), epoch: 2, hybrid: true }));
    let codes: Vec<(&str, u8)> = code::ALL.iter().map(|(c, n)| (*n, *c)).collect();
    check_or_write("ferrata_msg_v2.vectors.json", &file("ferrata_msg_v2", &codes, &vectors));

    // The PDAs: the inbox of r(12), and envelopes, from the wire's own functions.
    let program = r(20);
    let pda = |actor: &str, inputs: &[(&str, &str, String)], account: [u8; 32]| {
        let ins: Vec<String> = inputs.iter().map(|(n, e, v)| format!("{{\"name\": \"{n}\", \"encoding\": \"{e}\", \"value\": \"{v}\"}}")).collect();
        format!(
            "    {{\"program\": \"ferrata_msg_v2\", \"actor\": \"{actor}\", \"program_id\": \"{}\", \"inputs\": [{}], \"account\": \"{}\"}}",
            hex(&program),
            ins.join(", "),
            hex(&account)
        )
    };
    let entries = [
        pda("inbox", &[("owner", "bytes", hex(&r(12)))], inbox_account(&program, &r(12))),
        pda("envelope", &[("recipient", "bytes", hex(&r(12))), ("sender", "bytes", hex(&r(14))), ("seq", "u64le", "9".into())], envelope_account(&program, &r(12), &r(14), 9)),
        pda("envelope", &[("recipient", "bytes", hex(&r(12))), ("sender", "bytes", hex(&r(13))), ("seq", "u64le", "0".into())], envelope_account(&program, &r(12), &r(13), 0)),
    ];
    let pdas = format!(
        "{{\n  \"generator\": \"lez/msg-v2-wire/tests/idl_vectors.rs (FERRATA_IDL_WRITE=1 to rewrite)\",\n  \"pdas\": [\n{}\n  ]\n}}\n",
        entries.join(",\n")
    );
    check_or_write("ferrata_msg_v2.pdas.json", &pdas);
}
