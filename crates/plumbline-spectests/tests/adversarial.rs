//! Adversarial tests, built on real data from the official vectors (Electra, minimal preset).
//!
//! Every tampering is refused with its typed code, a refused update leaves the store unchanged,
//! and a self-consistent fabricated chain is refused on the signature alone.

use blst::min_pk::{AggregatePublicKey, AggregateSignature, PublicKey, SecretKey, Signature};
use plumbline_core::{
    bls::{Aggregation, BlsBackend, Blst, Zkcrypto, DST},
    fork::Fork,
    merkle::{
        compute_merkle_branch_root, subtree_index, EXECUTION_PAYLOAD_GINDEX, FINALIZED_ROOT_GINDEX_ELECTRA, ZERO,
    },
    policy::FinalizedStore,
    store::{LightClientStore, StoreCore},
    types::{
        BeaconBlockHeader, BlsPubkey, BlsSignature, Execution, ExecutionPayloadHeader, LightClientHeader,
        LightClientUpdate, SyncAggregate, SyncCommittee,
    },
    validate::{is_finality_update, sync_committee_signing_root, Context},
    Error, Root,
};
use plumbline_spectests::{load_sync_case, vectors_dir, LoadedCase};

static BLST: Blst = Blst::new(Aggregation::Auto);

fn backends() -> Vec<(&'static str, Box<dyn BlsBackend>)> {
    vec![
        ("blst/auto", Box::new(Blst::new(Aggregation::Auto))),
        ("blst/additive", Box::new(Blst::new(Aggregation::Additive))),
        ("blst/subtractive", Box::new(Blst::new(Aggregation::Subtractive))),
        ("zkcrypto/auto", Box::new(Zkcrypto::new(Aggregation::Auto))),
        ("zkcrypto/additive", Box::new(Zkcrypto::new(Aggregation::Additive))),
        ("zkcrypto/subtractive", Box::new(Zkcrypto::new(Aggregation::Subtractive))),
    ]
}

struct Fixture {
    case: LoadedCase,
    store: FinalizedStore,
}

fn fixture() -> Fixture {
    let case = load_sync_case(&vectors_dir().join("electra/light_client/sync/pyspec_tests/light_client_sync")).unwrap();
    let store = FinalizedStore::initialize(
        &case.trusted_block_root,
        &case.bootstrap,
        case.store_fork,
        &case.config.spec,
        &BLST,
    )
    .unwrap();
    Fixture { case, store }
}

impl Fixture {
    fn ctx<'a>(&'a self, bls: &'a dyn BlsBackend) -> Context<'a> {
        Context { spec: &self.case.config.spec, genesis_validators_root: self.case.genesis_validators_root, bls }
    }

    /// The case's first update: a finality update that also carries the next committee.
    fn first(&self) -> (LightClientUpdate, u64) {
        let (u, slot) = self.case.updates[0].clone();
        assert!(is_finality_update(&u));
        (u, slot)
    }

    /// Submit to a copy of the store; a refusal must leave the store unchanged.
    fn submit_to(
        &self,
        store: &FinalizedStore,
        update: &LightClientUpdate,
        slot: u64,
        bls: &dyn BlsBackend,
    ) -> Result<(), Error> {
        let mut s = store.clone();
        let result = s.process_update(update, slot, &self.ctx(bls)).map(|_| ());
        if result.is_err() {
            assert_eq!(&s, store, "a refused update changed the store");
        }
        result
    }

    fn submit(&self, update: &LightClientUpdate, slot: u64) -> Result<(), Error> {
        self.submit_to(&self.store, update, slot, &BLST)
    }
}

#[test]
fn genuine_update_is_accepted_by_every_backend() {
    let f = fixture();
    let (u, slot) = f.first();
    for (name, bls) in backends() {
        assert_eq!(f.submit_to(&f.store, &u, slot, bls.as_ref()), Ok(()), "{name}");
    }
}

#[test]
fn signature_over_another_header_is_refused() {
    let f = fixture();
    let (mut u, slot) = f.first();
    u.sync_aggregate.signature = f.case.updates[1].0.sync_aggregate.signature;
    for (name, bls) in backends() {
        assert_eq!(f.submit_to(&f.store, &u, slot, bls.as_ref()), Err(Error::SignatureInvalid), "{name}");
    }
}

#[test]
fn corrupted_signature_bytes_are_refused() {
    let f = fixture();
    let (u, slot) = f.first();
    for byte in [0usize, 1, 20, 47, 48, 70, 95] {
        let mut t = u.clone();
        t.sync_aggregate.signature[byte] ^= 0x04;
        let e = f.submit(&t, slot).unwrap_err();
        assert!(matches!(e, Error::SignatureMalformed | Error::SignatureInvalid), "byte {byte}: {e}");
    }
}

#[test]
fn changed_participation_bits_are_refused() {
    let f = fixture();
    let (u, slot) = f.first();
    let bits = &u.sync_aggregate.bits;
    let n = bits.len() as u64;
    let signers = u.sync_aggregate.participants();
    // Drop one signer: the claimed signing set no longer matches the signature.
    let mut t = u.clone();
    let i = bits.iter().position(|b| *b).unwrap();
    t.sync_aggregate.bits[i] = false;
    let expected = if (signers - 1) * 3 >= n * 2 { Error::SignatureInvalid } else { Error::BelowSupermajority };
    assert_eq!(f.submit(&t, slot), Err(expected));
    // Claim a member who did not sign.
    if let Some(j) = bits.iter().position(|b| !*b) {
        let mut t = u.clone();
        t.sync_aggregate.bits[j] = true;
        assert_eq!(f.submit(&t, slot), Err(Error::SignatureInvalid));
    }
}

#[test]
fn tampered_finality_branch_is_refused_at_every_depth() {
    let f = fixture();
    let (u, slot) = f.first();
    for depth in 0..u.finality_branch.len() {
        let mut t = u.clone();
        t.finality_branch[depth][0] ^= 1;
        assert_eq!(f.submit(&t, slot), Err(Error::FinalityBranchInvalid), "depth {depth}");
    }
}

#[test]
fn tampered_next_committee_or_its_branch_is_refused() {
    let f = fixture();
    let (u, slot) = f.first();
    for depth in 0..u.next_sync_committee_branch.len() {
        let mut t = u.clone();
        t.next_sync_committee_branch[depth][31] ^= 1;
        assert_eq!(f.submit(&t, slot), Err(Error::NextCommitteeBranchInvalid), "depth {depth}");
    }
    let mut t = u.clone();
    t.next_sync_committee.pubkeys[5][10] ^= 1;
    assert_eq!(f.submit(&t, slot), Err(Error::NextCommitteeBranchInvalid));
    let mut t = u.clone();
    t.next_sync_committee.aggregate_pubkey[10] ^= 1;
    assert_eq!(f.submit(&t, slot), Err(Error::NextCommitteeBranchInvalid));
}

#[test]
fn tampered_headers_are_refused() {
    let f = fixture();
    let (u, slot) = f.first();
    // Finalized beacon header: its root no longer matches the finality branch.
    let mut t = u.clone();
    t.finalized_header.beacon.state_root[0] ^= 1;
    assert_eq!(f.submit(&t, slot), Err(Error::FinalityBranchInvalid));
    // Finalized execution payload: the execution branch no longer reaches body_root.
    let mut t = u.clone();
    let Execution::Payload { header, .. } = &mut t.finalized_header.execution else { panic!("electra layout") };
    header.receipts_root[0] ^= 1;
    assert_eq!(f.submit(&t, slot), Err(Error::FinalizedHeaderInvalid));
    // Attested execution payload.
    let mut t = u.clone();
    let Execution::Payload { header, .. } = &mut t.attested_header.execution else { panic!("electra layout") };
    header.block_number += 1;
    assert_eq!(f.submit(&t, slot), Err(Error::AttestedHeaderInvalid));
    // Attested beacon header: a different header than the committee signed.
    let mut t = u.clone();
    t.attested_header.beacon.proposer_index += 1;
    assert_eq!(f.submit(&t, slot), Err(Error::SignatureInvalid));
}

#[test]
fn another_chain_or_fork_version_is_refused() {
    let f = fixture();
    let (u, slot) = f.first();
    // Chain binding: genesis_validators_root is part of the signing domain.
    let mut ctx = f.ctx(&BLST);
    ctx.genesis_validators_root[0] ^= 1;
    assert_eq!(f.store.clone().process_update(&u, slot, &ctx), Err(Error::SignatureInvalid));
    // A wrong fork version for the signature slot.
    let mut spec = f.case.config.spec.clone();
    spec.versions[Fork::Electra as usize][0] ^= 0x10;
    let ctx = Context { spec: &spec, genesis_validators_root: f.case.genesis_validators_root, bls: &BLST };
    assert_eq!(f.store.clone().process_update(&u, slot, &ctx), Err(Error::SignatureInvalid));
}

#[test]
fn updates_from_the_future_or_out_of_order_are_refused() {
    let f = fixture();
    let (u, _) = f.first();
    assert_eq!(f.submit(&u, u.signature_slot - 1), Err(Error::SlotOrderInvalid));
    let mut t = u.clone();
    t.signature_slot = t.attested_header.beacon.slot;
    assert_eq!(f.submit(&t, u.signature_slot), Err(Error::SlotOrderInvalid));
}

#[test]
fn policy_refuses_non_finality_and_replayed_updates() {
    let f = fixture();
    let (nf, slot) = f.case.updates.iter().find(|(u, _)| !is_finality_update(u)).cloned().unwrap();
    assert_eq!(f.submit(&nf, slot), Err(Error::NotFinalized));

    let (u, slot) = f.first();
    let mut store = f.store.clone();
    store.process_update(&u, slot, &f.ctx(&BLST)).unwrap();
    let after = store.clone();
    assert_eq!(store.process_update(&u, slot, &f.ctx(&BLST)), Err(Error::NotNewer));
    assert_eq!(store, after);
}

// ---------------------------------------------------------------------------------------------
// A fabricated chain (RFP-021 Verification Security #1).

struct FakeCommittee {
    keys: Vec<SecretKey>,
    committee: SyncCommittee,
}

fn fake_committee(n: usize, seed: u8) -> FakeCommittee {
    let keys: Vec<SecretKey> = (0..n)
        .map(|i| {
            let mut ikm = [seed; 32];
            ikm[0] = i as u8;
            SecretKey::key_gen(&ikm, &[]).unwrap()
        })
        .collect();
    let pks: Vec<PublicKey> = keys.iter().map(SecretKey::sk_to_pk).collect();
    let refs: Vec<&PublicKey> = pks.iter().collect();
    let aggregate = AggregatePublicKey::aggregate(&refs, false).unwrap().to_public_key();
    FakeCommittee {
        keys,
        committee: SyncCommittee {
            pubkeys: pks.iter().map(PublicKey::compress).collect(),
            aggregate_pubkey: aggregate.compress(),
        },
    }
}

fn sign(fake: &FakeCommittee, bits: &[bool], message: &Root) -> BlsSignature {
    let sigs: Vec<Signature> =
        fake.keys.iter().zip(bits).filter(|(_, b)| **b).map(|(sk, _)| sk.sign(message, DST, &[])).collect();
    let refs: Vec<&Signature> = sigs.iter().collect();
    AggregateSignature::aggregate(&refs, false).unwrap().to_signature().compress()
}

fn filler(tag: u8) -> Root {
    [tag; 32]
}

/// A header whose execution branch verifies against its own body root.
fn fabricated_header(slot: u64, tag: u8) -> LightClientHeader {
    let mut payload = ExecutionPayloadHeader::empty();
    payload.block_number = 1_000_000 + slot;
    payload.block_hash = filler(tag);
    payload.state_root = filler(tag.wrapping_add(1));
    payload.receipts_root = filler(tag.wrapping_add(2));
    payload.timestamp = 1_700_000_000 + 6 * slot;
    let branch: Vec<Root> = (0..4).map(|i| filler(tag.wrapping_add(10 + i))).collect();
    let body_root = compute_merkle_branch_root(
        &payload.hash_tree_root_deneb(),
        &branch,
        4,
        subtree_index(EXECUTION_PAYLOAD_GINDEX),
    )
    .unwrap();
    LightClientHeader {
        beacon: BeaconBlockHeader {
            slot,
            proposer_index: 7,
            parent_root: filler(tag.wrapping_add(20)),
            state_root: filler(tag.wrapping_add(21)),
            body_root,
        },
        execution: Execution::Payload { header: payload, branch },
    }
}

/// A finality update about a chain that never existed, with every branch self-consistent.
fn fabricated_update(
    template: &LightClientUpdate,
    fake: &FakeCommittee,
    signers: usize,
    ctx: &Context<'_>,
) -> LightClientUpdate {
    let n = fake.committee.pubkeys.len();
    let finalized = fabricated_header(template.finalized_header.beacon.slot, 40);
    let mut attested = fabricated_header(template.attested_header.beacon.slot, 80);
    let finality_branch: Vec<Root> = (0..7).map(|i| filler(120 + i)).collect();
    attested.beacon.state_root = compute_merkle_branch_root(
        &finalized.beacon.hash_tree_root(),
        &finality_branch,
        7,
        subtree_index(FINALIZED_ROOT_GINDEX_ELECTRA),
    )
    .unwrap();
    let bits: Vec<bool> = (0..n).map(|i| i < signers).collect();
    let mut update = LightClientUpdate {
        attested_header: attested,
        next_sync_committee: SyncCommittee::empty(n),
        next_sync_committee_branch: vec![ZERO; 6],
        finalized_header: finalized,
        finality_branch,
        sync_aggregate: SyncAggregate { bits: bits.clone(), signature: [0; 96] },
        signature_slot: template.signature_slot,
    };
    update.sync_aggregate.signature = sign(fake, &bits, &sync_committee_signing_root(&update, ctx));
    update
}

/// The real store with its committee replaced by `fake` (as if `fake` were the true committee).
fn store_trusting(f: &Fixture, fake: &FakeCommittee) -> FinalizedStore {
    FinalizedStore { core: StoreCore { current_sync_committee: fake.committee.clone(), ..f.store.core.clone() } }
}

#[test]
fn fabricated_chain_is_refused_on_the_signature() {
    let f = fixture();
    let (template, slot) = f.first();
    let n = template.sync_aggregate.bits.len();
    let fake = fake_committee(n, 0xa5);
    let update = fabricated_update(&template, &fake, n, &f.ctx(&BLST));
    for (name, bls) in backends() {
        // Positive control: the forgery is internally consistent; only the trust root differs.
        assert_eq!(f.submit_to(&store_trusting(&f, &fake), &update, slot, bls.as_ref()), Ok(()), "{name}");
        // Against the real committee it is refused, and only on the signature.
        assert_eq!(f.submit_to(&f.store, &update, slot, bls.as_ref()), Err(Error::SignatureInvalid), "{name}");
    }
}

#[test]
fn participation_floor_is_exactly_two_thirds() {
    let f = fixture();
    let (template, slot) = f.first();
    let n = template.sync_aggregate.bits.len(); // 32 in the minimal preset
    let floor = (2 * n).div_ceil(3); // 22
    let fake = fake_committee(n, 0x3c);
    let trusting = store_trusting(&f, &fake);
    for (signers, expected) in
        [(floor - 1, Err(Error::BelowSupermajority)), (floor, Ok(())), (floor + 1, Ok(())), (n, Ok(()))]
    {
        let update = fabricated_update(&template, &fake, signers, &f.ctx(&BLST));
        for (name, bls) in backends() {
            assert_eq!(f.submit_to(&trusting, &update, slot, bls.as_ref()), expected, "{signers} signers, {name}");
        }
    }
    // The spec store (which also tracks optimistic headers) needs only MIN_SYNC_COMMITTEE_PARTICIPANTS.
    let mut spec_store = LightClientStore::initialize(
        &f.case.trusted_block_root,
        &f.case.bootstrap,
        f.case.store_fork,
        &f.case.config.spec,
    )
    .unwrap();
    spec_store.core.current_sync_committee = fake.committee.clone();
    let mut none = fabricated_update(&template, &fake, 1, &f.ctx(&BLST));
    none.sync_aggregate.bits = vec![false; n];
    assert_eq!(spec_store.process_update(&none, slot, &f.ctx(&BLST)), Err(Error::ParticipationLow));
}

// ---------------------------------------------------------------------------------------------
// The signing-key composition (audit target #1), checked on hand-built edge cases.

fn keypair(seed: u8) -> (SecretKey, BlsPubkey) {
    let sk = SecretKey::key_gen(&[seed; 32], &[]).unwrap();
    let pk = sk.sk_to_pk().compress();
    (sk, pk)
}

/// The compressed encoding of the negated point: flip the y-sign flag.
fn negate(pk: &BlsPubkey) -> BlsPubkey {
    let mut out = *pk;
    out[0] ^= 0x20;
    out
}

fn identity_g1() -> BlsPubkey {
    let mut out = [0u8; 48];
    out[0] = 0xc0;
    out
}

fn identity_g2() -> BlsSignature {
    let mut out = [0u8; 96];
    out[0] = 0xc0;
    out
}

/// (0, 2) is on y² = x³ + 4 and has order 3: a curve point outside the prime-order subgroup.
fn order_three_point() -> BlsPubkey {
    let mut out = [0u8; 48];
    out[0] = 0x80;
    out
}

/// A curve point outside the prime-order subgroup with x ≠ 0 (blst special-cases x = 0).
fn non_subgroup_point() -> BlsPubkey {
    (1u8..=255)
        .map(|x| {
            let mut k = [0u8; 48];
            k[0] = 0x80;
            k[47] = x;
            k
        })
        .find(|k| PublicKey::uncompress(k).is_ok_and(|pk| pk.validate().is_err()))
        .unwrap()
}

#[test]
fn identity_signing_key_is_refused() {
    // {pk, −pk} both signing: the aggregate is the identity, and the identity signature would
    // "verify" any message if this were not checked.
    let (_, pk) = keypair(1);
    let committee = SyncCommittee { pubkeys: vec![pk, negate(&pk)], aggregate_pubkey: identity_g1() };
    for (name, bls) in backends() {
        assert_eq!(bls.check_committee(&committee), Ok(()), "{name}: the committee itself is consistent");
        assert_eq!(
            bls.verify_sync_aggregate(&committee, &[true, true], &[9; 32], &identity_g2()),
            Err(Error::SigningKeyIdentity),
            "{name}"
        );
    }
}

#[test]
fn signing_key_outside_the_subgroup_is_refused() {
    let (_, pk) = keypair(2);
    let t = non_subgroup_point();
    // Additive: the signers are {T, pk}. Subtractive: aggregate T, non-signer −pk, so T + pk.
    let additive = SyncCommittee { pubkeys: vec![t, pk], aggregate_pubkey: identity_g1() };
    let subtractive = SyncCommittee { pubkeys: vec![pk, negate(&pk)], aggregate_pubkey: t };
    let msg = [3u8; 32];
    for (name, bls) in backends() {
        let (committee, bits) = match name.rsplit('/').next() {
            Some("subtractive") => (&subtractive, [true, false]),
            Some("additive") => (&additive, [true, true]),
            _ => continue, // `auto` is one of the two above
        };
        assert_eq!(
            bls.verify_sync_aggregate(committee, &bits, &msg, &identity_g2()),
            Err(Error::SigningKeyNotInGroup),
            "{name}"
        );
    }
}

#[test]
fn order_three_point_is_refused_by_both_libraries_with_different_codes() {
    // Differential finding: blst rejects the x = 0 points (order 3) while decoding, zkcrypto
    // decodes them and rejects the aggregate at the subgroup check. Both refuse; the codes
    // differ, which is why the program pins one backend for its error codes.
    let (_, pk) = keypair(3);
    let committee = SyncCommittee { pubkeys: vec![order_three_point(), pk], aggregate_pubkey: identity_g1() };
    let msg = [4u8; 32];
    let blst = Blst::new(Aggregation::Additive).verify_sync_aggregate(&committee, &[true, true], &msg, &identity_g2());
    let zk =
        Zkcrypto::new(Aggregation::Additive).verify_sync_aggregate(&committee, &[true, true], &msg, &identity_g2());
    assert_eq!(blst, Err(Error::PubkeyInvalid));
    assert_eq!(zk, Err(Error::SigningKeyNotInGroup));
}

#[test]
fn off_curve_keys_and_inconsistent_committees_are_refused() {
    // Find an x for which x³ + 4 is not a square: that compressed key is not on the curve.
    let off_curve = (1u8..=255)
        .map(|x| {
            let mut k = [0u8; 48];
            k[0] = 0x80;
            k[47] = x;
            k
        })
        .find(|k| PublicKey::uncompress(k).is_err())
        .unwrap();
    let (_, a) = keypair(4);
    let (_, b) = keypair(5);
    let bad = SyncCommittee { pubkeys: vec![a, off_curve], aggregate_pubkey: a };
    // Wrong aggregate: valid points that do not sum to it.
    let wrong_sum = SyncCommittee { pubkeys: vec![a, b], aggregate_pubkey: a };
    for (name, bls) in backends() {
        assert_eq!(bls.check_committee(&bad), Err(Error::CommitteeInvalid), "{name}");
        assert_eq!(bls.check_committee(&wrong_sum), Err(Error::CommitteeInvalid), "{name}");
    }
    // Decoding an off-curve key on the signing path is refused too (additive: as a signer).
    let additive = Blst::new(Aggregation::Additive);
    assert_eq!(
        additive.verify_sync_aggregate(&bad, &[true, true], &[0; 32], &identity_g2()),
        Err(Error::PubkeyInvalid)
    );
    let additive = Zkcrypto::new(Aggregation::Additive);
    assert_eq!(
        additive.verify_sync_aggregate(&bad, &[true, true], &[0; 32], &identity_g2()),
        Err(Error::PubkeyInvalid)
    );
}

#[test]
fn malformed_signatures_are_refused() {
    let (sk, pk) = keypair(6);
    let committee = SyncCommittee { pubkeys: vec![pk], aggregate_pubkey: pk };
    let msg = [7u8; 32];
    let good = sk.sign(&msg, DST, &[]).compress();
    let mut not_compressed = good;
    not_compressed[0] &= 0x7f; // clear the compression flag
    for (name, bls) in backends() {
        assert_eq!(bls.verify_sync_aggregate(&committee, &[true], &msg, &good), Ok(()), "{name}");
        assert_eq!(
            bls.verify_sync_aggregate(&committee, &[true], &[8; 32], &good),
            Err(Error::SignatureInvalid),
            "{name}"
        );
        assert_eq!(
            bls.verify_sync_aggregate(&committee, &[true], &msg, &not_compressed),
            Err(Error::SignatureMalformed),
            "{name}"
        );
        assert_eq!(
            bls.verify_sync_aggregate(&committee, &[false], &msg, &good),
            Err(Error::ParticipationLow),
            "{name}"
        );
    }
}
