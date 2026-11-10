//! The checkpoint rule of SPEC-v2 §4.2, as a pure function from the input to the journal.
//!
//! Order of work in [`run_checkpoint`]:
//! 1. `lezConfigId` from its preimage; the parent state's frontier must be canonical; with
//!    segments, the segments must chain from `P` to the state the input continues from.
//! 2. Channel configuration changes, in `configTipHash` order, each signed by at least
//!    `t(current n)` current keys and keeping the posting timeframe (non-zero) bound in the key-set
//!    hash (SPEC-v3 D3); every key set reached must satisfy `nMin` and `tMin`.
//! 3. Blocks `P.tip+1 … E`: each parsed strictly, re-hashed, checked against its stored hash, its
//!    id (+1) and its `prev_block_hash`. For blocks up to `T`, the bridge messages of
//!    non-reverted transactions are applied in block, transaction and call order. After `T`, the
//!    first block carrying `Echo{echoNonce}` is `Y` (`Y = T` when `echoNonce = 0`).
//! 4. Producer and support (SPEC-v3 D3 item 4): one inscription of `T` is checked exactly like a
//!    counted one; its signer `D` must be in the key set `K`. Support: checked inscriptions of
//!    blocks in `(Y, E]` by at least `t(n)` distinct keys of `K` other than `D` (entries by `D` are
//!    checked but not counted). With `n = 1`, allowed only when `nMin ≤ 1`, the one key is `D` and
//!    the supporter.
//! 5. The 17-word journal (SPEC-v3 D3 item 5).
//!
//! Steps 1–3 are [`up_to_tip`], which rule (B) ([`crate::rule_b`]) shares.

use alloc::vec::Vec;

use ed25519_dalek::VerifyingKey;

use crate::hash::threshold;
use crate::input::{CheckpointInput, SegmentInput};
use crate::journal::{CheckpointJournal, SegmentJournal};
use crate::lez::{hash_parsed_block, parse_block, Actor, Origin, Visitor};
use crate::mantle::{verify_strict, ConfigChange};
use crate::state::{Config, State};
use crate::tree::{burn_leaf, zero_hashes};
use crate::wire::BridgeMessage;
use crate::{Error, Hash};

/// What a successful checkpoint produces besides its journal (for the host and the tests).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub journal: CheckpointJournal,
    pub new_state: State,
    /// Index of `Y` in the input's blocks.
    pub y_index: u32,
    /// The key that inscribed `T`.
    pub producer: Hash,
    /// Distinct supporting keys counted.
    pub support_keys: u32,
    /// The threshold they had to meet.
    pub threshold: u32,
}

/// Collects the calls to one program, with their transaction index and origin.
struct CallsTo<'p, 'a> {
    program: &'p Hash,
    calls: Vec<(u32, Origin, &'a [u8])>,
}

impl<'a> Visitor<'a> for CallsTo<'_, 'a> {
    fn call(&mut self, index: u32, origin: Origin, _from: Option<Actor<'a>>, to: Actor<'a>, message: &'a [u8]) {
        if to.program_account_id == self.program && origin.is_public_execution() {
            self.calls.push((index, origin, message));
        }
    }
}

/// One block, checked against its predecessor `(prev_id, prev_hash)`.
pub(crate) struct LinkedBlock<'a> {
    pub(crate) id: u64,
    pub(crate) hash: Hash,
    /// Calls to the bridge program executed publicly, with whether their transaction reverted.
    pub(crate) calls: Vec<(&'a [u8], bool)>,
}

/// Parses `bytes` strictly, recomputes its hash and checks it against the stored one, its id
/// (`prev_id + 1`) and its `prev_block_hash`.
pub(crate) fn link_block<'a>(
    index: u32,
    bytes: &'a [u8],
    prev_id: u64,
    prev_hash: &Hash,
    bridge: &Hash,
) -> Result<LinkedBlock<'a>, Error> {
    let mut visitor = CallsTo { program: bridge, calls: Vec::new() };
    let summary = parse_block(bytes, &mut visitor)?;
    let expected = prev_id.checked_add(1).ok_or(Error::BadBlockId { index, expected: u64::MAX, got: 0 })?;
    if summary.header.block_id != expected {
        return Err(Error::BadBlockId { index, expected, got: summary.header.block_id });
    }
    if &summary.header.prev_block_hash != prev_hash {
        return Err(Error::BrokenLink { index });
    }
    let hash = hash_parsed_block(bytes)?;
    if hash != summary.header.hash {
        return Err(Error::HashMismatch { index });
    }
    let calls = visitor.calls.into_iter().map(|(tx, _, msg)| (msg, summary.is_reverted(tx))).collect();
    Ok(LinkedBlock { id: summary.header.block_id, hash, calls })
}

/// Applies one bridge message of a non-reverted transaction (SPEC-v2 §4.2 item 2): only messages
/// for `config_id` count; anything that is not a well-formed `Burn`/`ImportRoot`/`Sunset` (the
/// bridge program, decoding with the same code, would refuse it) is ignored.
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
        // One-way: after `Sunset` the program refuses imports, so none can count. The count never
        // goes down: every leaf below any imported count may already have been claimed on LEZ.
        BridgeMessage::ImportRoot(m) if !state.sunset => {
            state.imported_leaf_count = state.imported_leaf_count.max(m.leaf_count);
        }
        BridgeMessage::Sunset { .. } => state.sunset = true,
        BridgeMessage::ImportRoot(_) | BridgeMessage::Echo { .. } => {}
    }
    Ok(())
}

/// Hashes, links and extracts `blocks` onto `state` (which must be at their parent), moving the
/// tip to the last of them.
fn extend(state: &mut State, config_id: &Hash, bridge: &Hash, blocks: &[&[u8]]) -> Result<(), Error> {
    for (i, bytes) in blocks.iter().enumerate() {
        let index = u32::try_from(i).map_err(|_| Error::BadTipIndex)?;
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

/// The floors of §4.2 item 3, and distinct keys (Bedrock's key list is a vector and would accept
/// a repeated key, which would let one key fill several signature indices; refused here).
fn check_key_set(keys: &[Hash], config: &Config) -> Result<(), Error> {
    if keys.iter().enumerate().any(|(i, k)| keys[..i].contains(k)) {
        return Err(Error::ConfigMalformed);
    }
    let n = u32::try_from(keys.len()).unwrap_or(u32::MAX);
    if n < config.n_min {
        return Err(Error::KeySetTooSmall { n, n_min: config.n_min });
    }
    let t = threshold(n);
    if t < config.t_min {
        return Err(Error::ThresholdTooLow { t, t_min: config.t_min });
    }
    Ok(())
}

/// Applies one channel configuration change (Bedrock's `ChannelConfigOp` rule at the pinned rev,
/// with SPEC-v2's threshold and SPEC-v3 D3's pinned turn length): the op is for our channel and
/// extends the configuration tip; its thresholds are non-zero and its keys non-empty, valid and not
/// of small order (as `VerifiedChannelKeys` decoding and preverify require); its
/// `posting_timeframe` equals the current one and is not 0; at least `t(current n)` signatures by
/// distinct current keys (indices into the current list) verify over the transaction hash.
fn apply_config_change(state: &mut State, change: &ConfigChange<'_>, channel_id: &Hash) -> Result<(), Error> {
    let (op, tx_hash) = change.op()?;
    if op.channel != channel_id {
        return Err(Error::WrongChannel);
    }
    if op.parent != &state.config_tip_hash {
        return Err(Error::ConfigParentMismatch);
    }
    if op.configuration_threshold == 0 || op.transfer_threshold == 0 || op.keys.is_empty() {
        return Err(Error::ConfigMalformed);
    }
    // SPEC-v3 D3: the turn length is pinned. The vault's wait has a term (t − 1) × turn, sound only
    // if the turn cannot change under it; with 0 a key that keeps posting holds its turn forever.
    if op.posting_timeframe != state.posting_timeframe || op.posting_timeframe == 0 {
        return Err(Error::ConfigTimeframe { current: state.posting_timeframe, proposed: op.posting_timeframe });
    }
    for key in &op.keys {
        let key = VerifyingKey::from_bytes(key).map_err(|_| Error::InvalidKey)?;
        if key.is_weak() {
            return Err(Error::InvalidKey);
        }
    }
    let mut seen = Vec::with_capacity(change.signatures.len());
    for (index, signature) in &change.signatures {
        if seen.contains(index) {
            return Err(Error::ConfigBadIndex);
        }
        seen.push(*index);
        let key = state.keys.get(usize::from(*index)).ok_or(Error::ConfigBadIndex)?;
        verify_strict(key, &tx_hash, signature)?;
    }
    let have = u32::try_from(seen.len()).unwrap_or(u32::MAX);
    let need = threshold(u32::try_from(state.keys.len()).unwrap_or(u32::MAX));
    if have < need {
        return Err(Error::ConfigThresholdUnmet { have, need });
    }
    state.keys = op.keys.iter().map(|k| **k).collect();
    state.config_tip_hash = op.id();
    Ok(())
}

/// What steps 1–3 leave for the support check of either rule.
pub(crate) struct UpToTip {
    pub(crate) config_id: Hash,
    /// `P`, the journal's `parentState`.
    pub(crate) parent_state: Hash,
    /// The state at `T`, with the key set after this proof's rotations (the set in `newState`).
    pub(crate) state: State,
    pub(crate) t_index: usize,
}

/// Steps 1–3 of the module docs, shared by both rules: `lezConfigId`; the parent's frontier; the
/// segments chaining to the parent; the key rotations with the floors on every set reached; the
/// blocks up to `T` linked and extracted.
pub(crate) fn up_to_tip(
    config: &Config,
    parent: &State,
    segments: &[SegmentJournal],
    config_changes: &[ConfigChange<'_>],
    blocks: &[&[u8]],
    t_index: u32,
) -> Result<UpToTip, Error> {
    let config_id = config.config_id();
    if !parent.burns.is_canonical() {
        return Err(Error::BadParentFrontier);
    }

    // P: the parent itself, or the start of the first segment when segments lead up to it.
    let continued_from = parent.digest();
    let parent_state = match (segments.first(), segments.last()) {
        (Some(first), Some(last)) => {
            if segments.iter().any(|s| s.lez_config_id != config_id) {
                return Err(Error::SegmentConfigMismatch);
            }
            let chained = segments.windows(2).all(|w| w[0].end_state == w[1].start_state);
            if !chained || last.end_state != continued_from {
                return Err(Error::SegmentChainBroken);
            }
            first.start_state
        }
        _ => continued_from,
    };

    // Key rotations.
    // Every key set reached must meet the floors; the parent's own set is checked only when no
    // change replaces it (a genesis channel may start below them and rotate up in its first
    // checkpoint).
    let mut state = parent.clone();
    for change in config_changes {
        apply_config_change(&mut state, change, &config.channel_id)?;
        check_key_set(&state.keys, config)?;
    }
    check_key_set(&state.keys, config)?;

    // Blocks up to T: link and extract.
    let t_index = usize::try_from(t_index).map_err(|_| Error::BadTipIndex)?;
    if t_index >= blocks.len() {
        return Err(Error::BadTipIndex);
    }
    extend(&mut state, &config_id, &config.bridge_header, &blocks[..=t_index])?;
    Ok(UpToTip { config_id, parent_state, state, t_index })
}

/// The full checkpoint rule (module docs). Refuses (returns `Err`) rather than producing a weaker
/// journal.
pub fn run_checkpoint(input: &CheckpointInput<'_>) -> Result<Outcome, Error> {
    let config = &input.config;
    let UpToTip { config_id, parent_state, state, t_index } =
        up_to_tip(config, &input.parent, &input.segments, &input.config_changes, &input.blocks, input.t_index)?;

    // Blocks after T: link, and find Y.
    let echo_wanted = input.echo_nonce != [0u8; 32];
    let mut y_index = if echo_wanted { None } else { Some(input.t_index) };
    let (mut prev_id, mut prev_hash) = (state.tip_id, state.tip_hash);
    for (i, bytes) in input.blocks.iter().enumerate().skip(t_index + 1) {
        let index = u32::try_from(i).map_err(|_| Error::BadTipIndex)?;
        let block = link_block(index, bytes, prev_id, &prev_hash, &config.bridge_header)?;
        if y_index.is_none() {
            // Any call to the bridge counts, reverted or not: inclusion is what dates the block.
            let carries = block.calls.iter().any(|(message, _)| {
                matches!(BridgeMessage::decode(message), Ok(Some(BridgeMessage::Echo { nonce })) if nonce == input.echo_nonce)
            });
            if carries {
                y_index = Some(index);
            }
        }
        (prev_id, prev_hash) = (block.id, block.hash);
    }
    let y_index = y_index.ok_or(Error::EchoNotFound)?;

    // The producer D: one checked inscription of T, by a key of K.
    let producer = input.producer.check(input.blocks[t_index], &config.channel_id)?;
    if !state.keys.contains(&producer.signer) {
        return Err(Error::NotAccredited);
    }

    // Support: distinct keys of K other than D (D itself when n = 1 and single-key is allowed).
    let n = u32::try_from(state.keys.len()).unwrap_or(u32::MAX);
    let need = threshold(n);
    let single_key = n == 1 && config.allow_single_key();
    let mut counted: Vec<Hash> = Vec::with_capacity(input.support.len());
    for entry in &input.support {
        let block_index = usize::try_from(entry.block_index).map_err(|_| Error::BadTipIndex)?;
        if entry.block_index <= y_index || block_index >= input.blocks.len() {
            return Err(Error::SupportOutOfRange { block_index: entry.block_index });
        }
        // Every supplied inscription must check, counted or not.
        let inscription = entry.inscription.check(input.blocks[block_index], &config.channel_id)?;
        if !state.keys.contains(&inscription.signer) {
            return Err(Error::NotAccredited);
        }
        let excluded = inscription.signer == producer.signer && !single_key;
        if !excluded && !counted.contains(&inscription.signer) {
            counted.push(inscription.signer);
        }
    }
    let have = u32::try_from(counted.len()).unwrap_or(u32::MAX);
    if have < need {
        return Err(Error::InsufficientSupport { have, need });
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

/// A segment: `parent → state at the last block`, by hashing, linking and extracting only. Its
/// journal is accepted by nothing but a checkpoint guest that verifies it (composition).
pub fn run_segment(input: &SegmentInput<'_>) -> Result<(SegmentJournal, State), Error> {
    if !input.parent.burns.is_canonical() {
        return Err(Error::BadParentFrontier);
    }
    if input.blocks.is_empty() {
        return Err(Error::BadTipIndex);
    }
    let config_id = input.config.config_id();
    let start_state = input.parent.digest();
    let mut state = input.parent.clone();
    extend(&mut state, &config_id, &input.config.bridge_header, &input.blocks)?;
    Ok((SegmentJournal { lez_config_id: config_id, start_state, end_state: state.digest() }, state))
}
