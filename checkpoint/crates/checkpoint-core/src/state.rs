//! The configuration a checkpoint follows and the LEZ checkpoint state `S` (SPEC-v2 §1, §4.1).

use alloc::vec::Vec;

use crate::hash::{sha256_parts, TAG_CONFIG, TAG_LEZ_STATE};
use crate::tree::Frontier;
use crate::{Address, Hash};

/// The preimage of `configId` plus the deployment constants of §4.2.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub bedrock_genesis_id: Hash,
    pub channel_id: Hash,
    pub bridge_header: Hash,
    pub bridge_image_id: Hash,
    /// `uint256`, big-endian.
    pub evm_chain_id: Hash,
    pub vault: Address,
    pub attestation_root_id: Hash,
    pub salt: Hash,
    /// Smallest key set the program follows.
    pub n_min: u32,
    /// Smallest threshold the program accepts.
    pub t_min: u32,
}

impl Config {
    /// `configId = H("ferrata/config/v1" ‖ bedrockGenesisId ‖ channelId ‖ bridgeHeader ‖
    /// bridgeImageId ‖ evmChainId ‖ vault ‖ attestationRootId ‖ salt)`.
    pub fn config_id(&self) -> Hash {
        sha256_parts(&[
            TAG_CONFIG,
            &self.bedrock_genesis_id,
            &self.channel_id,
            &self.bridge_header,
            &self.bridge_image_id,
            &self.evm_chain_id,
            &self.vault,
            &self.attestation_root_id,
            &self.salt,
        ])
    }

    /// Single-key support (`n = 1`) is allowed exactly when the deployment's floor allows a
    /// one-key set. SPEC-v2's separate `allowSingleKey` constant is not in the journal, so a
    /// prover could set it; tying it to `nMin` (which is in the journal and pinned by the vault)
    /// closes that.
    pub const fn allow_single_key(&self) -> bool {
        self.n_min <= 1
    }
}

/// The checkpoint state of §4.1, in full (its hash `S` is what the vault stores).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub tip_id: u64,
    pub tip_hash: Hash,
    /// The channel's accredited keys, in `accredited_keys` order.
    pub keys: Vec<Hash>,
    /// The channel's `posting_timeframe` (slots per turn), bound into `keySetHash` (SPEC-v3 D3).
    pub posting_timeframe: u32,
    pub config_tip_hash: Hash,
    pub burns: Frontier,
    pub imported_leaf_count: u64,
    pub sunset: bool,
}

/// `keySetHash = H("ferrata/keyset/v2" ‖ n:u32 ‖ postingTimeframe:u32 ‖ key_1 ‖ … ‖ key_n)`
/// (SPEC-v3 D3). Defined once, in the attestor core, because under rule (B) every attestor signs it.
pub use attestor_core::key_set_hash;

/// `S = H("ferrata/lez-state/v1" ‖ tipId:u64 ‖ tipHash ‖ keySetHash ‖ configTipHash ‖
/// burnCount:u64 ‖ burnFrontierHash ‖ importedLeafCount:u64 ‖ sunset:u8)`, from its parts.
#[allow(clippy::too_many_arguments)]
pub fn state_digest(
    tip_id: u64,
    tip_hash: &Hash,
    key_set_hash: &Hash,
    config_tip_hash: &Hash,
    burn_count: u64,
    burn_frontier_hash: &Hash,
    imported_leaf_count: u64,
    sunset: bool,
) -> Hash {
    sha256_parts(&[
        TAG_LEZ_STATE,
        &tip_id.to_be_bytes(),
        tip_hash,
        key_set_hash,
        config_tip_hash,
        &burn_count.to_be_bytes(),
        burn_frontier_hash,
        &imported_leaf_count.to_be_bytes(),
        &[u8::from(sunset)],
    ])
}

impl State {
    /// `S` of this state ([`state_digest`]).
    pub fn digest(&self) -> Hash {
        state_digest(
            self.tip_id,
            &self.tip_hash,
            &key_set_hash(&self.keys, self.posting_timeframe),
            &self.config_tip_hash,
            self.burns.count,
            &self.burns.hash(),
            self.imported_leaf_count,
            self.sunset,
        )
    }
}
