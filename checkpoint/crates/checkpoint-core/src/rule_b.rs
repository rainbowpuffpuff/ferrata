//! Rule (B), final-tier attestations (SPEC-v3 D7 and D13; `docs/design/bridge.md` §8.3), as a pure
//! function from the input to the journal.
//!
//! Steps 1–3 are rule (A)'s ([`crate::rule::up_to_tip`]): `lezConfigId`, the parent and its
//! segments, the key rotations with their floors, the blocks up to `T` linked and extracted. Then:
//!
//! 4. The blocks after `T` are linked like the others (parsed, re-hashed, id + 1,
//!    `prev_block_hash`); with `T` they form the hash-linked path from `T`.
//! 5. Attestations. `K` is the key set after this proof's rotations, the one in `newState`. Each
//!    entry names a channel key, which must be in `K` (a certificate dies when its key leaves
//!    `K`); an attestation key that this channel key certified for this deployment; and a final
//!    tip, which must be `T` or a block after `T` on the path. The attestation key must have signed
//!    the final-tier message v2 over this deployment, `keySetHash(K)` and that tip
//!    ([`attestor_core`]). A channel key or an attestation key that appears twice refuses the
//!    input. At least `t(n)` entries are needed. No exclusion, no beacon: `echoNonce` is 0.
//! 6. The producer: one inscription of `T` is checked exactly as rule (A) checks it (a well-framed
//!    Mantle transaction inscribing `T`'s bytes on this channel, its signer's `verify_strict` over
//!    the transaction hash), and its signer must be in `K`. So the attestation keys alone cannot
//!    certify a history: one channel key of `K` must also have signed an inscription of `T`.
//! 7. The 17-word journal of SPEC-v3 D3 item 5, word for word rule (A)'s candidate journal over the
//!    same blocks and key set. The vault tells the rules apart by the guest's image id.
//!
//! Checks run cheapest first (membership, repetition, the path, then the two signatures of each
//! attestation, the count, then `T`'s inscription, which hashes `T` whole), and the first failure
//! refuses the input.

use alloc::vec::Vec;

use attestor_core::{Deployment, VerifyError};

use crate::hash::threshold;
use crate::input::CheckpointInputB;
use crate::journal::CheckpointJournal;
use crate::rule::{link_block, up_to_tip, UpToTip};
use crate::state::{key_set_hash, State};
use crate::{Error, Hash};

/// What a successful rule (B) checkpoint produces besides its journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeB {
    pub journal: CheckpointJournal,
    pub new_state: State,
    /// The channel key that inscribed `T`.
    pub producer: Hash,
    /// Attestations counted (all of them: every entry is checked and none may repeat a key).
    pub attestors: u32,
    /// The threshold they had to meet.
    pub threshold: u32,
    /// Blocks after `T` on the path (the furthest an attestation could reach).
    pub path_after_tip: u32,
}

/// Rule (B) (module docs). Refuses (returns `Err`) rather than producing a weaker journal.
pub fn run_checkpoint_b(input: &CheckpointInputB<'_>) -> Result<OutcomeB, Error> {
    let config = &input.config;
    let UpToTip { config_id, parent_state, state, t_index } =
        up_to_tip(config, &input.parent, &input.segments, &input.config_changes, &input.blocks, input.t_index)?;

    // The hash-linked path from T: path[k] is the hash of block `T.id + k`.
    let mut path: Vec<Hash> = Vec::with_capacity(input.blocks.len() - t_index);
    path.push(state.tip_hash);
    let (mut prev_id, mut prev_hash) = (state.tip_id, state.tip_hash);
    for (i, bytes) in input.blocks.iter().enumerate().skip(t_index + 1) {
        let index = u32::try_from(i).map_err(|_| Error::BadTipIndex)?;
        let block = link_block(index, bytes, prev_id, &prev_hash, &config.bridge_header)?;
        (prev_id, prev_hash) = (block.id, block.hash);
        path.push(block.hash);
    }

    // K: the live key set after the rotations this proof applied; every attestor signed its hash.
    let live_key_set = key_set_hash(&state.keys, state.posting_timeframe);
    let deployment = Deployment {
        evm_chain_id: config.evm_chain_id,
        vault: config.vault,
        lez_config_id: config_id,
        channel_id: config.channel_id,
    };
    let mut channel_keys: Vec<&Hash> = Vec::with_capacity(input.attestations.len());
    let mut att_keys: Vec<&Hash> = Vec::with_capacity(input.attestations.len());
    for a in &input.attestations {
        if !state.keys.contains(&a.channel_key) {
            return Err(Error::NotAccredited);
        }
        if channel_keys.contains(&&a.channel_key) || att_keys.contains(&&a.att_key) {
            return Err(Error::DuplicateAttestor);
        }
        channel_keys.push(&a.channel_key);
        att_keys.push(&a.att_key);
        // T or a descendant of T: the block `tip_id - T.id` steps along the path has this hash.
        let on_path = a
            .tip
            .tip_id
            .checked_sub(state.tip_id)
            .and_then(|k| usize::try_from(k).ok())
            .and_then(|k| path.get(k))
            .is_some_and(|hash| *hash == a.tip.tip_hash);
        if !on_path {
            return Err(Error::NotADescendant { tip_id: a.tip.tip_id });
        }
        a.verify(&deployment, &live_key_set).map_err(|e| match e {
            VerifyError::Certificate => Error::BadCertificate,
            VerifyError::Attestation => Error::BadAttestation,
        })?;
    }

    let n = u32::try_from(state.keys.len()).unwrap_or(u32::MAX);
    let need = threshold(n);
    let have = u32::try_from(input.attestations.len()).unwrap_or(u32::MAX);
    if have < need {
        return Err(Error::InsufficientAttestations { have, need });
    }

    // The producer: one checked inscription of T, by a key of K (rule (A)'s check).
    let producer = input.producer.check(input.blocks[t_index], &config.channel_id)?;
    if !state.keys.contains(&producer.signer) {
        return Err(Error::NotAccredited);
    }

    let journal = CheckpointJournal {
        evm_chain_id: config.evm_chain_id,
        vault: config.vault,
        lez_config_id: config_id,
        n_min: config.n_min,
        t_min: config.t_min,
        parent_state,
        new_state: state.digest(),
        burn_root: state.burns.root(&crate::tree::zero_hashes()),
        burn_count: state.burns.count,
        imported_leaf_count: state.imported_leaf_count,
        sunset: state.sunset,
        echo_nonce: [0u8; 32],
        tip_id: state.tip_id,
        tip_hash: state.tip_hash,
        n,
        t: need,
    };
    let path_after_tip = u32::try_from(path.len() - 1).unwrap_or(u32::MAX);
    Ok(OutcomeB {
        journal,
        new_state: state,
        producer: producer.signer,
        attestors: have,
        threshold: need,
        path_after_tip,
    })
}
