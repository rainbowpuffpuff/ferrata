//! The JS library (deploy/testnet/web/msg/ferrata-msg.js) and the program agree on every byte: each message the
//! library encodes in `test/vectors-v2.json` decodes with the program's own types to the expected value and re-encodes
//! to the same bytes; so does each actor state (whose envelope the JS test then opens); and the library's PDAs are the
//! program's.

use ferrata_lez_msg_v2_wire::{envelope_account, inbox_account, Entry, Envelope, Inbox, MsgKey, MsgMessage, Policy, Profile, MLKEM768_PK_LEN};

const A: [u8; 32] = [0x41; 32];
const B: [u8; 32] = [0x42; 32];
const P: [u8; 32] = [0xEE; 32];
const NOW: u64 = 1_790_000_000_000;

fn vectors() -> serde_json::Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../deploy/testnet/web/msg/test/vectors-v2.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn bytes(v: &serde_json::Value) -> Vec<u8> {
    hex::decode(v.as_str().unwrap()).unwrap()
}

fn round_trip<T: borsh::BorshSerialize + borsh::BorshDeserialize + PartialEq + std::fmt::Debug>(name: &str, b: &[u8], want: &T) {
    assert_eq!(&borsh::from_slice::<T>(b).unwrap(), want, "{name}");
    assert_eq!(borsh::to_vec(want).unwrap(), b, "{name}");
}

#[test]
fn messages_states_and_pdas_round_trip() {
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
        ("send", MsgMessage::Send { to: B, now_ms: NOW, seq: 5, body: vec![3, 0, 0, 0, 0, 9] }),
        ("delete_3_5", MsgMessage::Delete { ns: vec![3, 5] }),
        ("clear_9", MsgMessage::Clear { upto: 9 }),
    ];
    for (name, want) in &expected {
        round_trip(name, &bytes(&m[*name]), want);
    }
    let s = &v["states"];
    let len = s["body_len"].as_u64().unwrap();
    let profile = Profile {
        key: Some(key),
        policy: Policy::ContactsOnly,
        contacts: vec![A],
        blocked: vec![],
        sent_at: vec![NOW],
        last_send_ms: NOW,
        sent_total: 3,
    };
    round_trip("profile", &bytes(&s["profile"]), &profile);
    let inbox = Inbox { owner: B, next_n: 2, entries: vec![Entry { n: 1, from: A, seq: 0, at_ms: NOW, len: len as u32, contact: true }] };
    round_trip("inbox", &bytes(&s["inbox"]), &inbox);
    let env_bytes = bytes(&s["envelope"]);
    let env: Envelope = borsh::from_slice(&env_bytes).unwrap();
    assert_eq!((env.recipient, env.from, env.seq, env.at_ms, env.body.len() as u64), (B, A, 0, NOW, len));
    round_trip("envelope", &env_bytes, &env);
    let p = &v["pdas"];
    assert_eq!(bytes(&p["program"]), P);
    assert_eq!(bytes(&p["inbox_b"]), inbox_account(&P, &B));
    assert_eq!(bytes(&p["envelope_b_a_0"]), envelope_account(&P, &B, &A, 0));
    assert_eq!(bytes(&p["envelope_b_a_7"]), envelope_account(&P, &B, &A, 7));
}
