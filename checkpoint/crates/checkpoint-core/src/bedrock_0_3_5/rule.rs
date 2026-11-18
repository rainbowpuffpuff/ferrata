//! Rule (A) and rule (B) over a zone on Logos Blockchain 0.3.5-rc.1: [`crate::rule::run_checkpoint`] and
//! [`crate::rule_b::run_checkpoint_b`], step for step, with the Mantle transactions read by [`super::mantle`] (the fork
//! digest framed, hashed and required to be the deployment's; an unreachable configuration threshold refused).
//!
//! The steps are copied from `rule.rs` and `rule_b.rs` (whose module docs give them in full) rather than shared,
//! because those files' panic locations are compiled into the deployed guests: changing them would change
//! `checkpoint`'s and `checkpoint_b`'s image ids. Block linking and extraction ([`crate::rule::link_block`]), the burn
//! tree, the state, the attestations' checks and the journal are the default build's own code. The test
//! `tests/bedrock_0_3_5.rs` checks that both rules give the default build's journals over the same blocks.

use alloc::vec::Vec;

use attestor_core::{Deployment, VerifyError};
use ed25519_dalek::VerifyingKey;

use super::mantle::{check, config_op};
use super::{fork_digest, Error};
use crate::hash::threshold;
use crate::input::{CheckpointInput, CheckpointInputB};
use crate::journal::{CheckpointJournal, SegmentJournal};
use crate::mantle::{verify_strict, ConfigChange};
use crate::rule::{link_block, Outcome};
use crate::rule_b::OutcomeB;
use crate::state::{key_set_hash, Config, State};
use crate::tree::{burn_leaf, zero_hashes};
use crate::wire::BridgeMessage;
use crate::Hash;

/// As `rule.rs`'s `apply_message`.
fn apply_message(state: &mut State, config_id: &Hash, message: &[u8]) -> Result<(), Error> {
    let Ok(Some(msg)) = BridgeMessage::decode(message) else {
        return Ok(());
    };
    if msg.config_id() != Some(config_id) {
        return Ok(());
    }
    match msg {
        BridgeMessage::Burn(b) => {
            let leaf = burn_leaf(config_id, state.burns.count, &b.token, &b.amount, &b.c_out, &b.r);
            state.burns.append(leaf)?;
        }
        BridgeMessage::ImportRoot(m) if !state.sunset => {
            state.imported_leaf_count = state.imported_leaf_count.max(m.leaf_count);
        }
        BridgeMessage::Sunset { .. } => state.sunset = true,
        BridgeMessage::ImportRoot(_) | BridgeMessage::Echo { .. } => {}
    }
    Ok(())
}

/// As `rule.rs`'s `extend`.
fn extend(state: &mut State, config_id: &Hash, bridge: &Hash, blocks: &[&[u8]]) -> Result<(), Error> {
    for (i, bytes) in blocks.iter().enumerate() {
        let index = u32::try_from(i).map_err(|_| crate::Error::BadTipIndex)?;
        let block = link_block(index, bytes, state.tip_id, &state.tip_hash, bridge)?;
        for (message, reverted) in &block.calls {
            if !reverted {
                apply_message(state, config_id, message)?;
            }
        }
        state.tip_id = block.id;
        state.tip_hash = block.hash;
    }
    Ok(())
}

/// As `rule.rs`'s `check_key_set`.
fn check_key_set(keys: &[Hash], config: &Config) -> Result<(), Error> {
    if keys.iter().enumerate().any(|(i, k)| keys[..i].contains(k)) {
        return Err(crate::Error::ConfigMalformed.into());
    }
    let n = u32::try_from(keys.len()).unwrap_or(u32::MAX);
    if n < config.n_min {
        return Err(crate::Error::KeySetTooSmall { n, n_min: config.n_min }.into());
    }
    let t = threshold(n);
    if t < config.t_min {
        return Err(crate::Error::ThresholdTooLow { t, t_min: config.t_min }.into());
    }
    Ok(())
}

/// As `rule.rs`'s `apply_config_change`, with the transaction read as 0.3.5-rc.1's ([`config_op`]: the fork digest
/// required, the configuration decoded as 0.3.5-rc.1 decodes it, the hash over the digest and the operations).
fn apply_config_change(
    state: &mut State,
    change: &ConfigChange<'_>,
    channel_id: &Hash,
    digest: &Hash,
) -> Result<(), Error> {
    let (op, tx_hash) = config_op(change, digest)?;
    if op.channel != channel_id {
        return Err(crate::Error::WrongChannel.into());
    }
    if op.parent != &state.config_tip_hash {
        return Err(crate::Error::ConfigParentMismatch.into());
    }
    if op.configuration_threshold == 0 || op.transfer_threshold == 0 || op.keys.is_empty() {
        return Err(crate::Error::ConfigMalformed.into());
    }
    if op.posting_timeframe != state.posting_timeframe || op.posting_timeframe == 0 {
        return Err(
            crate::Error::ConfigTimeframe { current: state.posting_timeframe, proposed: op.posting_timeframe }.into()
        );
    }
    for key in &op.keys {
        let key = VerifyingKey::from_bytes(key).map_err(|_| crate::Error::InvalidKey)?;
        if key.is_weak() {
            return Err(crate::Error::InvalidKey.into());
        }
    }
    let mut seen = Vec::with_capacity(change.signatures.len());
    for (index, signature) in &change.signatures {
        if seen.contains(index) {
            return Err(crate::Error::ConfigBadIndex.into());
        }
        seen.push(*index);
        let key = state.keys.get(usize::from(*index)).ok_or(crate::Error::ConfigBadIndex)?;
        verify_strict(key, &tx_hash, signature)?;
    }
    let have = u32::try_from(seen.len()).unwrap_or(u32::MAX);
    let need = threshold(u32::try_from(state.keys.len()).unwrap_or(u32::MAX));
    if have < need {
        return Err(crate::Error::ConfigThresholdUnmet { have, need }.into());
    }
    state.keys = op.keys.iter().map(|k| **k).collect();
    state.config_tip_hash = op.id();
    Ok(())
}

/// What steps 1–3 leave (as `rule.rs`'s `UpToTip`).
struct UpToTip {
    config_id: Hash,
    parent_state: Hash,
    state: State,
    t_index: usize,
}

/// As `rule.rs`'s `up_to_tip`, with [`apply_config_change`] above.
fn up_to_tip(
    config: &Config,
    parent: &State,
    segments: &[SegmentJournal],
    config_changes: &[ConfigChange<'_>],
    blocks: &[&[u8]],
    t_index: u32,
) -> Result<UpToTip, Error> {
    let config_id = config.config_id();
    if !parent.burns.is_canonical() {
        return Err(crate::Error::BadParentFrontier.into());
    }
    let continued_from = parent.digest();
    let parent_state = match (segments.first(), segments.last()) {
        (Some(first), Some(last)) => {
            if segments.iter().any(|s| s.lez_config_id != config_id) {
                return Err(crate::Error::SegmentConfigMismatch.into());
            }
            let chained = segments.windows(2).all(|w| w[0].end_state == w[1].start_state);
            if !chained || last.end_state != continued_from {
                return Err(crate::Error::SegmentChainBroken.into());
            }
            first.start_state
        }
        _ => continued_from,
    };
    let mut state = parent.clone();
    for change in config_changes {
        apply_config_change(&mut state, change, &config.channel_id, fork_digest(config))?;
        check_key_set(&state.keys, config)?;
    }
    check_key_set(&state.keys, config)?;
    let t_index = usize::try_from(t_index).map_err(|_| crate::Error::BadTipIndex)?;
    if t_index >= blocks.len() {
        return Err(crate::Error::BadTipIndex.into());
    }
    extend(&mut state, &config_id, &config.bridge_header, &blocks[..=t_index])?;
    Ok(UpToTip { config_id, parent_state, state, t_index })
}

/// Rule (A) ([`crate::rule::run_checkpoint`]) over a zone on 0.3.5-rc.1.
pub fn run_checkpoint(input: &CheckpointInput<'_>) -> Result<Outcome, Error> {
    let config = &input.config;
    let digest = fork_digest(config);
    let UpToTip { config_id, parent_state, state, t_index } =
        up_to_tip(config, &input.parent, &input.segments, &input.config_changes, &input.blocks, input.t_index)?;

    let echo_wanted = input.echo_nonce != [0u8; 32];
    let mut y_index = if echo_wanted { None } else { Some(input.t_index) };
    let (mut prev_id, mut prev_hash) = (state.tip_id, state.tip_hash);
    for (i, bytes) in input.blocks.iter().enumerate().skip(t_index + 1) {
        let index = u32::try_from(i).map_err(|_| crate::Error::BadTipIndex)?;
        let block = link_block(index, bytes, prev_id, &prev_hash, &config.bridge_header)?;
        if y_index.is_none() {
            let carries = block.calls.iter().any(|(message, _)| {
                matches!(BridgeMessage::decode(message), Ok(Some(BridgeMessage::Echo { nonce })) if nonce == input.echo_nonce)
            });
            if carries {
                y_index = Some(index);
            }
        }
        (prev_id, prev_hash) = (block.id, block.hash);
    }
    let y_index = y_index.ok_or(crate::Error::EchoNotFound)?;

    let producer = check(&input.producer, input.blocks[t_index], &config.channel_id, digest)?;
    if !state.keys.contains(&producer.signer) {
        return Err(crate::Error::NotAccredited.into());
    }

    let n = u32::try_from(state.keys.len()).unwrap_or(u32::MAX);
    let need = threshold(n);
    let single_key = n == 1 && config.allow_single_key();
    let mut counted: Vec<Hash> = Vec::with_capacity(input.support.len());
    for entry in &input.support {
        let block_index = usize::try_from(entry.block_index).map_err(|_| crate::Error::BadTipIndex)?;
        if entry.block_index <= y_index || block_index >= input.blocks.len() {
            return Err(crate::Error::SupportOutOfRange { block_index: entry.block_index }.into());
        }
        let inscription = check(&entry.inscription, input.blocks[block_index], &config.channel_id, digest)?;
        if !state.keys.contains(&inscription.signer) {
            return Err(crate::Error::NotAccredited.into());
        }
        let excluded = inscription.signer == producer.signer && !single_key;
        if !excluded && !counted.contains(&inscription.signer) {
            counted.push(inscription.signer);
        }
    }
    let have = u32::try_from(counted.len()).unwrap_or(u32::MAX);
    if have < need {
        return Err(crate::Error::InsufficientSupport { have, need }.into());
    }

    let journal = CheckpointJournal {
        evm_chain_id: config.evm_chain_id,
        vault: config.vault,
        lez_config_id: config_id,
        n_min: config.n_min,
        t_min: config.t_min,
        parent_state,
        new_state: state.digest(),
        burn_root: state.burns.root(&zero_hashes()),
        burn_count: state.burns.count,
        imported_leaf_count: state.imported_leaf_count,
        sunset: state.sunset,
        echo_nonce: input.echo_nonce,
        tip_id: state.tip_id,
        tip_hash: state.tip_hash,
        n,
        t: need,
    };
    Ok(Outcome { journal, new_state: state, y_index, producer: producer.signer, support_keys: have, threshold: need })
}

/// Rule (B) ([`crate::rule_b::run_checkpoint_b`]) over a zone on 0.3.5-rc.1.
pub fn run_checkpoint_b(input: &CheckpointInputB<'_>) -> Result<OutcomeB, Error> {
    let config = &input.config;
    let UpToTip { config_id, parent_state, state, t_index } =
        up_to_tip(config, &input.parent, &input.segments, &input.config_changes, &input.blocks, input.t_index)?;

    let mut path: Vec<Hash> = Vec::with_capacity(input.blocks.len() - t_index);
    path.push(state.tip_hash);
    let (mut prev_id, mut prev_hash) = (state.tip_id, state.tip_hash);
    for (i, bytes) in input.blocks.iter().enumerate().skip(t_index + 1) {
        let index = u32::try_from(i).map_err(|_| crate::Error::BadTipIndex)?;
        let block = link_block(index, bytes, prev_id, &prev_hash, &config.bridge_header)?;
        (prev_id, prev_hash) = (block.id, block.hash);
        path.push(block.hash);
    }

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
            return Err(crate::Error::NotAccredited.into());
        }
        if channel_keys.contains(&&a.channel_key) || att_keys.contains(&&a.att_key) {
            return Err(crate::Error::DuplicateAttestor.into());
        }
        channel_keys.push(&a.channel_key);
        att_keys.push(&a.att_key);
        let on_path = a
            .tip
            .tip_id
            .checked_sub(state.tip_id)
            .and_then(|k| usize::try_from(k).ok())
            .and_then(|k| path.get(k))
            .is_some_and(|hash| *hash == a.tip.tip_hash);
        if !on_path {
            return Err(crate::Error::NotADescendant { tip_id: a.tip.tip_id }.into());
        }
        a.verify(&deployment, &live_key_set).map_err(|e| match e {
            VerifyError::Certificate => crate::Error::BadCertificate,
            VerifyError::Attestation => crate::Error::BadAttestation,
        })?;
    }

    let n = u32::try_from(state.keys.len()).unwrap_or(u32::MAX);
    let need = threshold(n);
    let have = u32::try_from(input.attestations.len()).unwrap_or(u32::MAX);
    if have < need {
        return Err(crate::Error::InsufficientAttestations { have, need }.into());
    }

    let producer = check(&input.producer, input.blocks[t_index], &config.channel_id, fork_digest(config))?;
    if !state.keys.contains(&producer.signer) {
        return Err(crate::Error::NotAccredited.into());
    }

    let journal = CheckpointJournal {
        evm_chain_id: config.evm_chain_id,
        vault: config.vault,
        lez_config_id: config_id,
        n_min: config.n_min,
        t_min: config.t_min,
        parent_state,
        new_state: state.digest(),
        burn_root: state.burns.root(&zero_hashes()),
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
