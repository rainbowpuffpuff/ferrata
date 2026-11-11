//! The decision logic, the attestation audit, the node view and the state file, without a chain.

use std::collections::BTreeMap;

use attestor_core::{key_set_hash, sign_certificate, Deployment, FinalTip, SignedAttestation};
use ed25519_dalek::SigningKey;
use ferrata_monitor::check::Candidate as Cand;
use ferrata_monitor::check::{
    attested_by, audit_attestations, audit_finalized, evaluate, AttestationFinding, Candidate, Policy, Reason, Rule,
    Verdict,
};
use ferrata_monitor::node::{threshold, KeySet, NodeView, TipState, H32};
use ferrata_monitor::store::{Status, Store, Tracked};

fn hb(tag: &str, i: u64) -> H32 {
    let mut b = [0u8; 32];
    let s = format!("{tag}/{i}");
    for (k, c) in s.bytes().enumerate() {
        b[k % 32] ^= c.wrapping_mul(31).wrapping_add(k as u8);
    }
    b[31] ^= i as u8;
    b[30] ^= (i >> 8) as u8;
    H32(b)
}

fn channel_sk(i: u8) -> SigningKey {
    SigningKey::from_bytes(&[i + 1; 32])
}

fn keys(n: u8) -> Vec<H32> {
    (0..n).map(|i| H32(channel_sk(i).verifying_key().to_bytes())).collect()
}

/// An honest node: blocks 1..=final_height final, hashes hb("block", id); four keys from block 0.
fn node(name: &str, final_height: u64) -> NodeView {
    NodeView {
        node: name.into(),
        final_height,
        final_blocks: (1..=final_height).map(|i| (i, hb("block", i))).collect(),
        key_sets: vec![KeySet { from_block: 0, keys: keys(4), posting_timeframe: 900 }],
        states: BTreeMap::new(),
        attestations: Vec::new(),
    }
}

fn cand(tip_id: u64, tip_hash: H32, n: u32, t: u32, submitted_at: u64) -> Candidate {
    Candidate {
        new_state: hb("state", tip_id),
        parent_state: hb("state", 0),
        burn_root: hb("burnroot", tip_id),
        burn_count: tip_id,
        imported_leaf_count: 2,
        sunset: false,
        delta_at_submit: 3600,
        tip_id,
        tip_hash,
        n,
        t,
        submitted_at,
    }
}

const B: Policy = Policy { rule: Rule::B, turn_seconds: 0, b_lag_grace: 300, delta_g: 7200, margin: 120 };
const A: Policy = Policy { rule: Rule::A, turn_seconds: 900, b_lag_grace: 300, delta_g: 7200, margin: 120 };
const T0: u64 = 1_000_000;

#[test]
fn threshold_matches_lez() {
    let got: Vec<u32> = (1..=10).map(threshold).collect();
    assert_eq!(got, vec![1, 1, 2, 3, 4, 4, 5, 6, 6, 7]);
}

#[test]
fn b_honest_candidate_passes() {
    let v = node("n1", 20);
    let c = cand(10, hb("block", 10), 4, 3, T0);
    assert_eq!(evaluate(&B, &c, &[v], T0), Verdict::Pass);
}

#[test]
fn b_conflicting_tip_is_cancelled_at_once() {
    let v = node("n1", 20);
    let c = cand(10, hb("forged", 10), 4, 3, T0);
    match evaluate(&B, &c, &[v], T0) {
        Verdict::Cancel { reason: Reason::TipConflict { tip_id, final_hash, .. } } => {
            assert_eq!(tip_id, 10);
            assert_eq!(final_hash, hb("block", 10));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn b_tip_not_final_waits_for_the_lag_allowance_then_cancels() {
    let v = node("n1", 20);
    let c = cand(25, hb("block", 25), 4, 3, T0);
    assert!(matches!(evaluate(&B, &c, &[v.clone()], T0 + 10), Verdict::Wait { until, .. } if until == T0 + 300));
    assert!(matches!(
        evaluate(&B, &c, &[v.clone()], T0 + 300),
        Verdict::Cancel { reason: Reason::TipNotFinal { final_height: 20, .. } }
    ));
    // The node catches up in time: the same candidate passes.
    assert_eq!(evaluate(&B, &c, &[node("n1", 30)], T0 + 299), Verdict::Pass);
}

#[test]
fn a_wait_never_runs_past_the_deadline_minus_the_margin() {
    let v = node("n1", 20);
    let c = cand(25, hb("block", 25), 4, 3, T0);
    let long = Policy { b_lag_grace: 100_000, ..B };
    assert!(matches!(evaluate(&long, &c, &[v.clone()], T0), Verdict::Wait { until, .. } if until == T0 + 3600 - 120));
    assert!(matches!(evaluate(&long, &c, &[v], T0 + 3480), Verdict::Cancel { .. }));
}

#[test]
fn key_set_not_live_is_cancelled_under_both_rules() {
    let v = node("n1", 20);
    // A stale, smaller set (n = 3, t = 2) against the live four keys.
    let c = cand(10, hb("block", 10), 3, 2, T0);
    for p in [B, A] {
        assert!(matches!(
            evaluate(&p, &c, &[v.clone()], T0),
            Verdict::Cancel {
                reason: Reason::KeySetNotLive { live_n: 4, live_t: 3, candidate_n: 3, candidate_t: 2, .. }
            }
        ));
    }
    // A rotation to five keys from block 15: a tip at 16 must carry n = 5, t = 4; a tip at 12 the old set.
    let mut r = node("n1", 20);
    r.key_sets.push(KeySet { from_block: 15, keys: keys(5), posting_timeframe: 900 });
    assert!(matches!(evaluate(&B, &cand(16, hb("block", 16), 4, 3, T0), &[r.clone()], T0), Verdict::Cancel { .. }));
    assert_eq!(evaluate(&B, &cand(16, hb("block", 16), 5, 4, T0), &[r.clone()], T0), Verdict::Pass);
    assert_eq!(evaluate(&B, &cand(12, hb("block", 12), 4, 3, T0), &[r], T0), Verdict::Pass);
}

#[test]
fn a_tip_must_be_final_by_delta_g() {
    let v = node("n1", 20);
    let c = cand(25, hb("block", 25), 4, 3, T0);
    // W = 3600 + 2 * 900 = 5400 < Δ_g here, so the deadline bounds the wait (the vault enforces W >= Δ_g + ...).
    assert_eq!(c.deadline(900), T0 + 5400);
    assert!(
        matches!(evaluate(&A, &c, &[v.clone()], T0 + 1000), Verdict::Wait { until, .. } if until == T0 + 5400 - 120)
    );
    let a_short = Policy { delta_g: 2000, ..A };
    assert!(
        matches!(evaluate(&a_short, &c, &[v.clone()], T0 + 1999), Verdict::Wait { until, .. } if until == T0 + 2000)
    );
    assert!(matches!(evaluate(&a_short, &c, &[v], T0 + 2000), Verdict::Cancel { reason: Reason::TipNotFinal { .. } }));
}

#[test]
fn state_fields_are_compared_when_the_node_computes_them() {
    let mut v = node("n1", 20);
    let c = cand(10, hb("block", 10), 4, 3, T0);
    v.states.insert(
        10,
        TipState {
            new_state: Some(c.new_state),
            burn_root: c.burn_root,
            burn_count: 10,
            imported_leaf_count: 2,
            sunset: false,
        },
    );
    assert_eq!(evaluate(&B, &c, &[v.clone()], T0), Verdict::Pass);
    v.states.get_mut(&10).unwrap().burn_root = hb("other root", 10);
    assert!(matches!(
        evaluate(&B, &c, &[v.clone()], T0),
        Verdict::Cancel { reason: Reason::StateMismatch { ref field, .. } } if field == "burnRoot"
    ));
    v.states.get_mut(&10).unwrap().burn_root = c.burn_root;
    v.states.get_mut(&10).unwrap().imported_leaf_count = 3;
    assert!(matches!(
        evaluate(&B, &c, &[v], T0),
        Verdict::Cancel { reason: Reason::StateMismatch { ref field, .. } } if field == "importedLeafCount"
    ));
}

#[test]
fn several_nodes_must_all_agree() {
    let c = cand(10, hb("block", 10), 4, 3, T0);
    let behind = node("n2", 8);
    assert!(matches!(evaluate(&B, &c, &[node("n1", 20), behind.clone()], T0), Verdict::Wait { .. }));
    let mut forked = node("n3", 20);
    forked.final_blocks.insert(10, hb("fork", 10));
    assert!(matches!(
        evaluate(&B, &c, &[node("n1", 20), forked], T0),
        Verdict::Cancel { reason: Reason::TipConflict { ref node, .. } } if node == "n3"
    ));
    assert_eq!(evaluate(&B, &c, &[], T0), Verdict::Blind);
}

#[test]
fn a_finalised_wrong_checkpoint_is_reported_for_revocation() {
    let v = node("n1", 20);
    assert_eq!(audit_finalized(&cand(10, hb("block", 10), 4, 3, T0), &[v.clone()], T0), None);
    assert!(matches!(
        audit_finalized(&cand(10, hb("forged", 10), 4, 3, T0), &[v.clone()], T0),
        Some(Reason::TipConflict { .. })
    ));
    assert!(matches!(
        audit_finalized(&cand(40, hb("block", 40), 4, 3, T0), &[v], T0),
        Some(Reason::TipNotFinal { .. })
    ));
}

fn deployment() -> Deployment {
    let mut evm_chain_id = [0u8; 32];
    evm_chain_id[24..].copy_from_slice(&31337u64.to_be_bytes());
    Deployment { evm_chain_id, vault: [0x11; 20], lez_config_id: [0x22; 32], channel_id: [0x33; 32] }
}

fn attestation(i: u8, d: &Deployment, ksh: &[u8; 32], tip_id: u64, tip_hash: H32) -> String {
    let ch = channel_sk(i);
    let att = SigningKey::from_bytes(&[0x80 + i; 32]);
    let cert = sign_certificate(&ch, d, &att.verifying_key().to_bytes());
    let tip = FinalTip { tip_id, tip_hash: tip_hash.0, final_msg_id: hb("msg", tip_id).0 };
    let sa = SignedAttestation::new(&att, ch.verifying_key().to_bytes(), cert, d, ksh, tip);
    format!("0x{}", hex::encode(sa.encode()))
}

#[test]
fn attestations_are_audited_with_the_attestor_core() {
    let d = deployment();
    let mut v = node("n1", 20);
    let live: Vec<[u8; 32]> = keys(4).iter().map(|k| k.0).collect();
    let ksh = key_set_hash(&live, 900);
    // Three honest attestations of tip 10: t(4) = 3.
    for i in 0..3 {
        v.attestations.push(attestation(i, &d, &ksh, 10, hb("block", 10)));
    }
    assert_eq!(attested_by(&v, &d, 10, &hb("block", 10)), 3);
    assert!(audit_attestations(&v, &d).iter().all(AttestationFinding::is_good));

    // Key 3 signs a conflicting tip 12: evidence against an attestor.
    v.attestations.push(attestation(3, &d, &ksh, 12, hb("forged", 12)));
    // Key 0 signs under a stale key set (three keys): its message does not verify under the live set.
    let stale: Vec<[u8; 32]> = keys(3).iter().map(|k| k.0).collect();
    v.attestations.push(attestation(0, &d, &key_set_hash(&stale, 900), 11, hb("block", 11)));
    // A key outside the live set.
    v.attestations.push(attestation(9, &d, &ksh, 10, hb("block", 10)));
    // Another deployment's certificate.
    let mut other = d;
    other.vault = [0x44; 20];
    let mut swapped = hex::decode(&attestation(1, &other, &ksh, 10, hb("block", 10))[2..]).unwrap();
    let honest = hex::decode(&attestation(1, &d, &ksh, 10, hb("block", 10))[2..]).unwrap();
    swapped[128..].copy_from_slice(&honest[128..]); // keep the honest message and signature, bad certificate
    v.attestations.push(format!("0x{}", hex::encode(&swapped)));
    v.attestations.push("0x1234".into());

    let f = audit_attestations(&v, &d);
    assert!(matches!(f[3], AttestationFinding::ConflictingTip { tip_id: 12, .. }), "{:?}", f[3]);
    assert!(f[3].is_misbehaviour());
    assert!(matches!(f[4], AttestationFinding::BadSignature { tip_id: 11, .. }), "{:?}", f[4]);
    assert!(matches!(f[5], AttestationFinding::ChannelKeyNotLive { .. }), "{:?}", f[5]);
    assert!(matches!(f[6], AttestationFinding::BadCertificate { .. }), "{:?}", f[6]);
    assert!(matches!(f[7], AttestationFinding::Malformed { .. }), "{:?}", f[7]);
    // Only the three good ones count for tip 10.
    assert_eq!(attested_by(&v, &d, 10, &hb("block", 10)), 3);
}

#[test]
fn node_view_json_and_validation() {
    let k = keys(4);
    let doc = serde_json::json!({
        "node": "lez-1",
        "finalHeight": 12,
        "finalBlocks": { "11": hb("block", 11), "12": hb("block", 12) },
        "keySets": [ { "fromBlock": 0, "keys": k, "postingTimeframe": 900 } ],
        "states": { "12": { "burnRoot": hb("r", 12), "burnCount": 3, "importedLeafCount": 2, "sunset": false } }
    });
    let v: NodeView = serde_json::from_value(doc).unwrap();
    v.validate().unwrap();
    assert_eq!(v.final_blocks[&12], hb("block", 12));
    assert_eq!(v.live_key_set(12).unwrap().t(), 3);
    let mut bad = v.clone();
    bad.final_blocks.insert(13, hb("block", 13));
    assert!(bad.validate().is_err());
    let unknown = serde_json::json!({ "node": "x", "finalHeight": 1, "finalBlocks": {}, "extra": 1 });
    assert!(serde_json::from_value::<NodeView>(unknown).is_err());
}

#[test]
fn state_file_round_trip_and_vault_guard() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let path = dir.join("monitor-state-test.json");
    let _ = std::fs::remove_file(&path);
    let (mut s, resumed) = Store::load_or_new(&path, "0xabc", 7).unwrap();
    assert!(!resumed);
    assert_eq!(s.next_block, 7);
    let c = cand(10, hb("block", 10), 4, 3, T0);
    s.candidates.insert(
        c.new_state.to_string(),
        Tracked {
            candidate: c.clone(),
            status: Status::Pending,
            verdict: Some(Verdict::Pass),
            first_seen_at: T0,
            cancel_tx: None,
            cancelled_at: None,
        },
    );
    s.cancel_events.entry("0x01".into()).or_default().insert("0xtx".into(), 9);
    assert_eq!(s.cancels_before("0x01", 9), 0);
    assert_eq!(s.cancels_before("0x01", 10), 1);
    s.next_block = 42;
    s.save(&path).unwrap();
    let (back, resumed) = Store::load_or_new(&path, "0xABC", 0).unwrap();
    assert!(resumed);
    assert_eq!(back, s);
    assert!(Store::load_or_new(&path, "0xdef", 0).is_err());
    assert!(!path.with_extension("tmp").exists());
}

/// The node view from a real LEZ v0.4.0-rc1 sequencer's record (the lab's devnet, `lez/lab/results/record`).
#[test]
fn a_real_sequencer_record_as_the_node_view() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../lez/lab/results/record/chain.tsv");
    let text = std::fs::read_to_string(path).unwrap();
    let v = NodeView::from_chain_tsv("lab", &text, Vec::new()).unwrap();
    assert_eq!(v.final_height, 258);
    assert_eq!(v.final_blocks.len(), 258);
    let tip: H32 = "0xad17ee54bb76f415fb16862d9d09c328424fa10d481ea507662335658e3a9db2".parse().unwrap();
    assert_eq!(v.final_blocks[&258], tip);
    // No key sets in the record: the key-set check is skipped, the tip checks apply.
    assert_eq!(evaluate(&B, &cand(258, tip, 1, 1, T0), &[v.clone()], T0), Verdict::Pass);
    assert!(matches!(
        evaluate(&B, &cand(258, hb("forged", 258), 1, 1, T0), &[v.clone()], T0),
        Verdict::Cancel { reason: Reason::TipConflict { .. } }
    ));
    assert!(matches!(evaluate(&B, &cand(259, hb("x", 259), 1, 1, T0), &[v], T0), Verdict::Wait { .. }));

    // The same record with blocks 257 and 258 only Safe and Pending: the final tier ends at 256.
    let partial: String = text
        .lines()
        .map(|l| {
            let mut f: Vec<String> = l.split('\t').map(str::to_owned).collect();
            match f[0].as_str() {
                "257" => *f.last_mut().unwrap() = "Safe".into(),
                "258" => *f.last_mut().unwrap() = "Pending".into(),
                _ => {}
            }
            f.join("\t") + "\n"
        })
        .collect();
    let p = NodeView::from_chain_tsv("lab", &partial, Vec::new()).unwrap();
    assert_eq!(p.final_height, 256);
    let c = cand(258, tip, 1, 1, T0);
    assert!(matches!(evaluate(&B, &c, &[p.clone()], T0 + 299), Verdict::Wait { .. }));
    assert!(matches!(
        evaluate(&B, &c, &[p], T0 + 300),
        Verdict::Cancel { reason: Reason::TipNotFinal { final_height: 256, .. } }
    ));
    // A gap in the record is refused.
    let gap: String = text.lines().filter(|l| !l.starts_with("100\t")).map(|l| format!("{l}\n")).collect();
    assert!(NodeView::from_chain_tsv("lab", &gap, Vec::new()).is_err());
}

/// The live round trip's rule (B) checkpoint (Sepolia, 11 Nov 2026, `e2e/runs/crown-live-1`): the real
/// attestation its attestor published (the devnet sequencer's channel key certified the attestation key,
/// which signed the final-tier message v2 for tip 252) and the 17-word journal the vault verified.
#[test]
fn the_live_round_trips_attestation_and_checkpoint() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../e2e/runs/crown-live-1/checkpoint");
    let a: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/input/attestor.json")).unwrap()).unwrap();
    let h = |k: &str| -> H32 { a[k].as_str().unwrap().parse().unwrap() };
    let sig = |k: &str| -> [u8; 64] { hex::decode(&a[k].as_str().unwrap()[2..]).unwrap().try_into().unwrap() };
    let dep = &a["deployment"];
    let dh = |k: &str| -> H32 { dep[k].as_str().unwrap().parse().unwrap() };
    let mut evm_chain_id = [0u8; 32];
    evm_chain_id[24..].copy_from_slice(&dep["evm_chain_id"].as_u64().unwrap().to_be_bytes());
    let d = Deployment {
        evm_chain_id,
        vault: hex::decode(&dep["vault"].as_str().unwrap()[2..]).unwrap().try_into().unwrap(),
        lez_config_id: dh("lez_config_id").0,
        channel_id: dh("channel_id").0,
    };
    let (tip_id, tip_hash) = (a["tip_id"].as_u64().unwrap(), h("tip_hash"));
    let ks = KeySet {
        from_block: 0,
        keys: vec![h("channel_key")],
        posting_timeframe: a["posting_timeframe"].as_u64().unwrap() as u32,
    };
    assert_eq!(H32(ks.hash()), h("key_set_hash"), "the key-set hash the attestor signed");
    let sa = SignedAttestation {
        channel_key: h("channel_key").0,
        att_key: h("attestation_key").0,
        certificate: sig("certificate"),
        tip: FinalTip { tip_id, tip_hash: tip_hash.0, final_msg_id: h("final_msg_id").0 },
        signature: sig("signature"),
    };
    let mut view = NodeView {
        node: "crown-devnet".into(),
        final_height: tip_id,
        final_blocks: [(tip_id, tip_hash)].into(),
        key_sets: vec![ks],
        states: BTreeMap::new(),
        attestations: vec![format!("0x{}", hex::encode(sa.encode()))],
    };
    view.validate().unwrap();
    let f = audit_attestations(&view, &d);
    assert!(f[0].is_good(), "{f:?}");
    assert_eq!(attested_by(&view, &d, tip_id, &tip_hash), 1);

    // The journal the vault verified: the candidate the watcher would have seen.
    let j = hex::decode(
        std::fs::read_to_string(format!("{dir}/receipt/journal.hex")).unwrap().trim().trim_start_matches("0x"),
    )
    .unwrap();
    assert_eq!(j.len(), 17 * 32);
    let w = |i: usize| -> [u8; 32] { j[i * 32..(i + 1) * 32].try_into().unwrap() };
    let u = |i: usize| -> u64 { u64::from_be_bytes(w(i)[24..].try_into().unwrap()) };
    let c = Cand {
        new_state: H32(w(7)),
        parent_state: H32(w(6)),
        burn_root: H32(w(8)),
        burn_count: u(9),
        imported_leaf_count: u(10),
        sunset: u(11) == 1,
        delta_at_submit: 3600,
        tip_id: u(13),
        tip_hash: H32(w(14)),
        n: u(15) as u32,
        t: u(16) as u32,
        submitted_at: T0,
    };
    assert_eq!((c.tip_id, c.tip_hash, c.n, c.t), (tip_id, tip_hash, 1, 1));
    assert_eq!(evaluate(&B, &c, &[view.clone()], T0), Verdict::Pass);
    assert_eq!(audit_finalized(&c, &[view.clone()], T0), None);

    // One flipped byte in the certificate, the signature or the signed tip, and the audit refuses it.
    for (at, want) in [(64usize, "badCertificate"), (200, "badSignature"), (140, "badSignature")] {
        let mut b = sa.encode();
        b[at] ^= 1;
        view.attestations = vec![format!("0x{}", hex::encode(b))];
        let got = serde_json::to_value(&audit_attestations(&view, &d)[0]).unwrap();
        assert_eq!(got["finding"], want, "byte {at}: {got}");
    }
}
