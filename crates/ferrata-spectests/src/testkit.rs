//! Building blocks for tests that need data no real chain produced: committees we hold the keys
//! of, and beacon headers and states whose Merkle branches verify by construction.

use std::collections::BTreeMap;

use blst::min_pk::{AggregatePublicKey, AggregateSignature, PublicKey, SecretKey, Signature};
use ferrata_core::{
    bls::DST,
    fork::LcFork,
    hash::{hash_pair, sha256},
    merkle::{
        CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, EXECUTION_PAYLOAD_GINDEX, FINALIZED_ROOT_GINDEX_ELECTRA,
        NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA, ZERO,
    },
    types::{
        BeaconBlockHeader, BlsSignature, Execution, ExecutionPayloadHeader, LightClientBootstrap, LightClientHeader,
        LightClientUpdate, SyncAggregate, SyncCommittee,
    },
    validate::{sync_committee_signing_root, Context},
    Root,
};

/// A committee whose secret keys the test holds.
pub struct FakeCommittee {
    pub keys: Vec<SecretKey>,
    pub committee: SyncCommittee,
}

/// `n` deterministic keys derived from `seed`, with their correct aggregate.
pub fn fake_committee(n: usize, seed: u8) -> FakeCommittee {
    let keys: Vec<SecretKey> = (0..n)
        .map(|i| {
            let mut ikm = [seed; 32];
            ikm[0] = i as u8;
            ikm[1] = (i >> 8) as u8;
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

/// The same keys with an `aggregate_pubkey` they do not sum to.
pub fn with_wrong_aggregate(committee: &SyncCommittee) -> SyncCommittee {
    SyncCommittee { pubkeys: committee.pubkeys.clone(), aggregate_pubkey: committee.pubkeys[0] }
}

/// The aggregate signature of the members whose bit is set.
pub fn sign(fake: &FakeCommittee, bits: &[bool], message: &Root) -> BlsSignature {
    let sigs: Vec<Signature> =
        fake.keys.iter().zip(bits).filter(|(_, b)| **b).map(|(sk, _)| sk.sign(message, DST, &[])).collect();
    let refs: Vec<&Signature> = sigs.iter().collect();
    AggregateSignature::aggregate(&refs, false).unwrap().to_signature().compress()
}

/// A distinct filler chunk per tag.
pub fn filler(tag: u64) -> Root {
    sha256(&tag.to_le_bytes())
}

/// A tree in which each generalized index in `leaves` holds its value and every other node is
/// filler. Returns the root and, for each leaf in order, its branch (leaf level first).
pub fn sparse_tree(leaves: &[(u64, Root)], salt: u64) -> (Root, Vec<Vec<Root>>) {
    fn node(g: u64, leaves: &BTreeMap<u64, Root>, salt: u64, memo: &mut BTreeMap<u64, Root>) -> Root {
        if let Some(v) = leaves.get(&g) {
            return *v;
        }
        if let Some(v) = memo.get(&g) {
            return *v;
        }
        let is_ancestor = leaves.keys().any(|&leaf| {
            let mut x = leaf;
            while x > g {
                x /= 2;
            }
            x == g
        });
        let v = if is_ancestor {
            let left = node(2 * g, leaves, salt, memo);
            let right = node(2 * g + 1, leaves, salt, memo);
            hash_pair(&left, &right)
        } else {
            filler(salt.wrapping_mul(1_000_003) ^ g)
        };
        memo.insert(g, v);
        v
    }
    let map: BTreeMap<u64, Root> = leaves.iter().copied().collect();
    let mut memo = BTreeMap::new();
    let root = node(1, &map, salt, &mut memo);
    let branches = leaves
        .iter()
        .map(|(g, _)| {
            let mut branch = Vec::new();
            let mut x = *g;
            while x > 1 {
                branch.push(node(x ^ 1, &map, salt, &mut memo));
                x /= 2;
            }
            branch
        })
        .collect();
    (root, branches)
}

/// A Deneb-layout execution payload header.
pub fn payload_header(slot: u64, tag: u64) -> ExecutionPayloadHeader {
    let mut payload = ExecutionPayloadHeader::empty();
    payload.block_number = 1_000_000 + slot;
    payload.block_hash = filler(tag);
    payload.state_root = filler(tag + 1);
    payload.receipts_root = filler(tag + 2);
    payload.timestamp = 1_700_000_000 + 6 * slot;
    payload
}

/// An Electra-layout header whose execution branch verifies against its own body root.
pub fn fabricated_header(slot: u64, tag: u64) -> LightClientHeader {
    header_with_payload(slot, tag, payload_header(slot, tag))
}

/// An Electra-layout header carrying `payload` (any execution state: a test's own tries), whose
/// execution branch verifies against its own body root.
pub fn header_with_payload(slot: u64, tag: u64, payload: ExecutionPayloadHeader) -> LightClientHeader {
    let (body_root, branches) = sparse_tree(&[(EXECUTION_PAYLOAD_GINDEX, payload.hash_tree_root_deneb())], tag);
    LightClientHeader {
        beacon: BeaconBlockHeader {
            slot,
            proposer_index: 7,
            parent_root: filler(tag + 20),
            state_root: filler(tag + 21),
            body_root,
        },
        execution: Execution::Payload { header: payload, branch: branches[0].clone() },
    }
}

/// What to fabricate.
pub struct Fabrication<'a> {
    /// `None` for an update without finality.
    pub finalized_slot: Option<u64>,
    pub attested_slot: u64,
    pub signature_slot: u64,
    /// The first `signers` members sign.
    pub signers: usize,
    /// A next committee to prove in the attested state, if any.
    pub next_committee: Option<&'a SyncCommittee>,
}

/// An Electra-layout update about a chain that never existed: every branch verifies, and the
/// signature is by `fake` under `ctx`'s chain and fork schedule.
pub fn fabricated_update(f: &Fabrication<'_>, fake: &FakeCommittee, ctx: &Context<'_>) -> LightClientUpdate {
    let finalized = f.finalized_slot.map(|slot| fabricated_header(slot, 40 + slot));
    let attested = fabricated_header(f.attested_slot, 80 + f.attested_slot);
    update_over(finalized, attested, f.next_committee, f.signature_slot, f.signers, fake, ctx)
}

/// An Electra-layout update over the given headers (`None`: an update without finality): the
/// attested state proves the finalized header and `next_committee`, and the first `signers`
/// members of `fake` sign at `signature_slot` under `ctx`'s chain and fork schedule.
pub fn update_over(
    finalized: Option<LightClientHeader>,
    mut attested: LightClientHeader,
    next_committee: Option<&SyncCommittee>,
    signature_slot: u64,
    signers: usize,
    fake: &FakeCommittee,
    ctx: &Context<'_>,
) -> LightClientUpdate {
    let n = fake.committee.pubkeys.len();
    let mut leaves = Vec::new();
    if let Some(h) = &finalized {
        leaves.push((FINALIZED_ROOT_GINDEX_ELECTRA, h.beacon.hash_tree_root()));
    }
    if let Some(c) = next_committee {
        leaves.push((NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA, c.hash_tree_root()));
    }
    let (state_root, branches) = sparse_tree(&leaves, 7_000 + attested.beacon.slot);
    attested.beacon.state_root = state_root;
    let mut branches = branches.into_iter();
    let finality_branch = if finalized.is_some() { branches.next().unwrap() } else { vec![ZERO; 7] };
    let next_sync_committee_branch = if next_committee.is_some() { branches.next().unwrap() } else { vec![ZERO; 6] };
    let bits: Vec<bool> = (0..n).map(|i| i < signers).collect();
    let mut update = LightClientUpdate {
        attested_header: attested,
        next_sync_committee: next_committee.cloned().unwrap_or_else(|| SyncCommittee::empty(n)),
        next_sync_committee_branch,
        finalized_header: finalized.unwrap_or_else(|| LightClientHeader::empty(LcFork::Electra)),
        finality_branch,
        sync_aggregate: SyncAggregate { bits: bits.clone(), signature: [0; 96] },
        signature_slot,
    };
    update.sync_aggregate.signature = sign(fake, &bits, &sync_committee_signing_root(&update, ctx));
    update
}

/// A bootstrap at `header` whose state proves `fake`'s committee as the current one (Electra
/// layout): a checkpoint of a chain whose keys the test holds.
pub fn fabricated_bootstrap(mut header: LightClientHeader, fake: &FakeCommittee, salt: u64) -> LightClientBootstrap {
    let (state_root, branches) =
        sparse_tree(&[(CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, fake.committee.hash_tree_root())], salt);
    header.beacon.state_root = state_root;
    LightClientBootstrap {
        header,
        current_sync_committee: fake.committee.clone(),
        current_sync_committee_branch: branches[0].clone(),
    }
}
