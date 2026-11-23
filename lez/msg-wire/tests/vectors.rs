//! The JS library (deploy/testnet/web/msg/ferrata-msg.js) and the program agree on every byte: each message the
//! library encodes in `test/vectors.json` decodes with the program's own types to the expected value and re-encodes to
//! the same bytes; so does the mailbox (whose one envelope the JS test then opens).

use ferrata_lez_msg_wire::{Mailbox, MsgKey, MsgMessage, Policy, Stored, MLKEM768_PK_LEN};

const A: [u8; 32] = [0x41; 32];
const B: [u8; 32] = [0x42; 32];
const NOW: u64 = 1_790_000_000_000;

fn vectors() -> serde_json::Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../deploy/testnet/web/msg/test/vectors-v1.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn bytes(v: &serde_json::Value) -> Vec<u8> {
    hex::decode(v.as_str().unwrap()).unwrap()
}

#[test]
fn messages_and_mailbox_round_trip() {
    let v = vectors();
    let key = MsgKey {
        epoch: 0,
        x25519: bytes(&v["keys"]["x25519"]).try_into().unwrap(),
        mlkem768: Some(bytes(&v["keys"]["mlkem768"])),
    };
    assert_eq!(key.mlkem768.as_ref().unwrap().len(), MLKEM768_PK_LEN);
    let m = &v["messages"];
    let expected = [
        ("publish", MsgMessage::Publish { key: key.clone() }),
        ("set_policy_contacts_only", MsgMessage::SetPolicy { policy: Policy::ContactsOnly }),
        ("contacts_add_a_remove_b", MsgMessage::Contacts { add: vec![A], remove: vec![B] }),
        ("block_b", MsgMessage::Block { add: vec![B], remove: vec![] }),
        ("send", MsgMessage::Send { to: B, now_ms: NOW, body: vec![1, 0, 0, 0, 0, 9] }),
        ("delete_3_5", MsgMessage::Delete { seqs: vec![3, 5] }),
        ("clear_9", MsgMessage::Clear { upto: 9 }),
    ];
    for (name, want) in expected {
        let b = bytes(&m[name]);
        assert_eq!(borsh::from_slice::<MsgMessage>(&b).unwrap(), want, "{name}");
        assert_eq!(borsh::to_vec(&want).unwrap(), b, "{name}");
    }
    let mb_bytes = bytes(&v["mailbox"]["hex"]);
    let mb: Mailbox = borsh::from_slice(&mb_bytes).unwrap();
    assert_eq!(borsh::to_vec(&mb).unwrap(), mb_bytes);
    let body = mb.inbox[0].body.clone();
    assert_eq!(body.len() as u64, v["mailbox"]["body_len"].as_u64().unwrap());
    let want = Mailbox {
        key: Some(key),
        policy: Policy::ContactsOnly,
        contacts: vec![A],
        blocked: vec![],
        next_seq: 1,
        inbox: vec![Stored { seq: 0, from: A, at_ms: NOW, body }],
        window_start_ms: NOW,
        sent_in_window: 3,
        last_send_ms: NOW,
        sent_total: 3,
    };
    assert_eq!(mb, want);
}
