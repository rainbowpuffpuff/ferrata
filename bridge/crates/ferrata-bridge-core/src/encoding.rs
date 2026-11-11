//! Every byte layout of the bridge, specification v2 (`notes/design/SPEC-v2.md`), defined once.
//!
//! Conventions:
//! * integers are big-endian at the stated width (`u8`, `u32`, `u64`); `uint256` is 32 bytes;
//!   `evmChainId` is a `uint256` (32 bytes) in every preimage;
//! * an Ethereum address is its 20 bytes, except inside ABI-encoded journals, where it is a 32-byte
//!   word with 12 leading zero bytes (as `abi.encode` writes it);
//! * `‖` is plain concatenation, with no length prefixes;
//! * the deposit and burn leaf tags and the two vault journal domains are the SHA-256 of their ASCII
//!   string; every other tag is its ASCII string itself, prefixed byte for byte.
//!
//! Domain separation: deposit leaves start with `0x00`, tree nodes with `0x01`, burn leaves with
//! `0x02`; every other SHA-256 preimage and every HMAC message starts with one of the ASCII tags
//! below (first byte `f`). No tag is a prefix of another (tested), so preimages of different kinds
//! differ inside the tag even where their lengths coincide (`C` and `N` are both 77 bytes).
//!
//! | value | function | bytes | preimage / message |
//! |---|---|---|---|
//! | `kAuth`, `kView` | HMAC(bridgeSeed) | 15 | `"ferrata/auth/v1"`, `"ferrata/view/v1"` |
//! | `configId` | SHA-256 | 261 | `"ferrata/config/v1" ‖ bedrockGenesisId ‖ channelId ‖ bridgeHeader ‖ bridgeImageId ‖ evmChainId:u256 ‖ vault:20 ‖ attestationRootId ‖ salt` |
//! | deposit leaf | SHA-256 | 145 | `0x00 ‖ LEAF_TAG ‖ leafIndex:u64 ‖ depositor:20 ‖ token:20 ‖ amount:u256 ‖ C` |
//! | tree node | SHA-256 | 65 | `0x01 ‖ left ‖ right` |
//! | `s` | HMAC(kAuth) | 92 | `"ferrata/s/v1" ‖ evmChainId:u256 ‖ vault:20 ‖ depositor:20 ‖ j:u64` |
//! | `C` | SHA-256 | 77 | `"ferrata/C/v1" ‖ s ‖ kind:u8 ‖ dest` |
//! | `fid` | SHA-256 | 148 | `"ferrata/fact/acc-leaf/v1" ‖ evmChainId:u256 ‖ vault:20 ‖ slot:u256 (0) ‖ leafIndex:u64 ‖ leaf` |
//! | `N` | SHA-256 | 77 | `"ferrata/nf/v1" ‖ s ‖ fid` |
//! | `r_i` | HMAC(kView) | 59 | `"ferrata/burn-tag/v1" ‖ configId ‖ i:u64` |
//! | `sOut` | HMAC(kAuth) | 80 | `"ferrata/s-out/v1" ‖ configId ‖ r` |
//! | `COut` | SHA-256 | 119 | `"ferrata/Cout/v1" ‖ sOut ‖ evmChainId:u256 ‖ vault:20 ‖ recipient:20` |
//! | memo pad | HMAC(kView)[0..20] | 47 | `"ferrata/memo/v1" ‖ r` |
//! | `mac` | HMAC(kView)[0..16] | 155 | `"ferrata/burn-mac/v2" ‖ r ‖ memo:20 ‖ token:20 ‖ amount:u256 ‖ COut` (D12; v1 omitted the memo) |
//! | burn leaf | SHA-256 | 189 | `0x02 ‖ BURN_TAG ‖ configId ‖ burnIndex:u64 ‖ token:20 ‖ amount:u256 ‖ COut ‖ r` |
//! | `burnId` | SHA-256 | 87 | `"ferrata/burn/v1" ‖ configId ‖ burnIndex:u64 ‖ burnLeaf` |
//! | `NOut` | SHA-256 | 49 | `"ferrata/nf-out/v2" ‖ sOut` (D13; v1 also hashed `burnId`, so it changed with the burn's index) |
//! | `keySetHash` | SHA-256 | 21 + 32n | `"ferrata/keyset/v1" ‖ n:u32 ‖ key_1 ‖ … ‖ key_n` |
//! | `burnFrontierHash` | SHA-256 | 1048 | `"ferrata/burn-frontier/v1" ‖ f_0 ‖ … ‖ f_31` (**not in SPEC-v2**: see [`burn_frontier_preimage`]) |
//! | `S` | SHA-256 | 173 | `"ferrata/lez-state/v1" ‖ tipId:u64 ‖ tipHash ‖ keySetHash ‖ configTipHash ‖ burnCount:u64 ‖ burnFrontierHash ‖ importedLeafCount:u64 ‖ sunset:u8` |
//! | checkpoint journal | (vault hashes it) | 416 | see [`CheckpointJournal`] |
//! | release journal | (vault hashes it) | 352 | see [`ReleaseJournal`] |
//!
//! Two further layouts are private prover inputs, not protocol encodings: [`ReleaseInput`] (the
//! release guest's input) and [`ClaimInput`] / [`claim_journal`] (a measurement guest that runs
//! the claim statement inside the zkVM).

use crate::inbound::{ClaimWitness, DestKind};
use crate::outbound::{ReleasePublic, ReleaseWitness};
use crate::tree::{MerklePath, TREE_DEPTH};

/// A SHA-256 digest or any other 32-byte value.
pub type Hash = [u8; 32];
/// An Ethereum address.
pub type Address = [u8; 20];
/// A `uint256`, 32 bytes big-endian.
pub type U256 = [u8; 32];
/// A burn-note MAC: the first 16 bytes of an HMAC-SHA256 output.
pub type Mac = [u8; 16];

/// First byte of every deposit-leaf preimage.
pub const LEAF_PREFIX: u8 = 0x00;
/// First byte of every tree-node preimage.
pub const NODE_PREFIX: u8 = 0x01;
/// First byte of every burn-leaf preimage.
pub const BURN_LEAF_PREFIX: u8 = 0x02;
/// The vault's storage slot of `depositRoot`, as it enters `fid` (a `uint256`).
pub const DEPOSIT_ROOT_SLOT: U256 = [0u8; 32];

/// `sha256("ferrata/vault/leaf/v2")`.
pub const LEAF_TAG: Hash = hex32("17b9508a0d0cbf61207c0fcb66f06ed92a6aeec110a54860479f0bf3cd6aff0b");
/// `sha256("ferrata/burn/leaf/v2")`.
pub const BURN_TAG: Hash = hex32("93474c5ad2f57f5fc610ad22e3f6d50e51b98a4c0eb611dde536a7317c0e8cf2");
/// `sha256("ferrata/vault/release/v2")`.
pub const RELEASE_DOMAIN: Hash = hex32("6e04f4b1fa66535e182a1ef2e4f5bfdfe44f7257f1663ef81ffa4f38d7696e51");
/// `sha256("ferrata/vault/checkpoint/v2")`.
pub const CHECKPOINT_DOMAIN: Hash = hex32("e99926553e520475aa7b6407f28efcca58fa91851d8476395f692eb871457ce1");

/// String whose SHA-256 is [`LEAF_TAG`].
pub const LEAF_TAG_STR: &str = "ferrata/vault/leaf/v2";
/// String whose SHA-256 is [`BURN_TAG`].
pub const BURN_TAG_STR: &str = "ferrata/burn/leaf/v2";
/// String whose SHA-256 is [`RELEASE_DOMAIN`].
pub const RELEASE_DOMAIN_STR: &str = "ferrata/vault/release/v2";
/// String whose SHA-256 is [`CHECKPOINT_DOMAIN`].
pub const CHECKPOINT_DOMAIN_STR: &str = "ferrata/vault/checkpoint/v2";

/// HMAC message (key: bridgeSeed) for `kAuth`.
pub const TAG_AUTH: &[u8] = b"ferrata/auth/v1";
/// HMAC message (key: bridgeSeed) for `kView`.
pub const TAG_VIEW: &[u8] = b"ferrata/view/v1";
/// Prefix of the preimage of `configId`.
pub const TAG_CONFIG: &[u8] = b"ferrata/config/v1";
/// Prefix of the HMAC message for the deposit secret `s`.
pub const TAG_S: &[u8] = b"ferrata/s/v1";
/// Prefix of the preimage of the deposit commitment `C`.
pub const TAG_C: &[u8] = b"ferrata/C/v1";
/// Prefix of the preimage of the accumulator-leaf identifier `fid`.
pub const TAG_FACT_ACC_LEAF: &[u8] = b"ferrata/fact/acc-leaf/v1";
/// Prefix of the preimage of the deposit nullifier `N`.
pub const TAG_NF: &[u8] = b"ferrata/nf/v1";
/// Prefix of the HMAC message for the burn tag `r_i`.
pub const TAG_BURN_TAG: &[u8] = b"ferrata/burn-tag/v1";
/// Prefix of the HMAC message for the release secret `sOut`.
pub const TAG_S_OUT: &[u8] = b"ferrata/s-out/v1";
/// Prefix of the preimage of the burn commitment `COut`.
pub const TAG_C_OUT: &[u8] = b"ferrata/Cout/v1";
/// Prefix of the HMAC message for the memo pad.
pub const TAG_MEMO: &[u8] = b"ferrata/memo/v1";
/// Prefix of the HMAC message for the burn-note MAC.
pub const TAG_BURN_MAC: &[u8] = b"ferrata/burn-mac/v2";
/// Prefix of the preimage of `burnId`.
pub const TAG_BURN: &[u8] = b"ferrata/burn/v1";
/// Prefix of the preimage of the burn nullifier `NOut`.
pub const TAG_NF_OUT: &[u8] = b"ferrata/nf-out/v2";
/// Prefix of the preimage of `keySetHash`.
pub const TAG_KEYSET: &[u8] = b"ferrata/keyset/v1";
/// Prefix of the preimage of `burnFrontierHash` (this crate's choice; SPEC-v2 leaves it open).
pub const TAG_BURN_FRONTIER: &[u8] = b"ferrata/burn-frontier/v1";
/// Prefix of the preimage of the LEZ checkpoint state `S`.
pub const TAG_LEZ_STATE: &[u8] = b"ferrata/lez-state/v1";

/// Every ASCII tag that is prefixed (or HMAC'd) as is.
pub const PREFIX_TAGS: [&[u8]; 17] = [
    TAG_AUTH,
    TAG_VIEW,
    TAG_CONFIG,
    TAG_S,
    TAG_C,
    TAG_FACT_ACC_LEAF,
    TAG_NF,
    TAG_BURN_TAG,
    TAG_S_OUT,
    TAG_C_OUT,
    TAG_MEMO,
    TAG_BURN_MAC,
    TAG_BURN,
    TAG_NF_OUT,
    TAG_KEYSET,
    TAG_BURN_FRONTIER,
    TAG_LEZ_STATE,
];

// ------------------------------------------------------------------------------------------------
// Identities (§1)
// ------------------------------------------------------------------------------------------------

/// The fields `configId` commits to (§1): the pairing of one LEZ bridge configuration with one vault.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfigIdentity {
    /// Logos Blockchain (Bedrock) genesis id.
    pub bedrock_genesis_id: Hash,
    /// The zone's Logos Blockchain channel id.
    pub channel_id: Hash,
    /// The bridge program's header account id on LEZ.
    pub bridge_header: Hash,
    /// The bridge program's image id.
    pub bridge_image_id: Hash,
    /// The vault's chain id.
    pub evm_chain_id: U256,
    /// The vault's address.
    pub vault: Address,
    /// Root id of the RFP-021 attestation lineage the configuration reads Ethereum through.
    pub attestation_root_id: Hash,
    /// Deployer-chosen salt.
    pub salt: Hash,
}

/// Preimage of `configId`, 261 bytes: `"ferrata/config/v1"`(17) ‖ bedrockGenesisId(32) ‖ channelId(32)
/// ‖ bridgeHeader(32) ‖ bridgeImageId(32) ‖ evmChainId(32) ‖ vault(20) ‖ attestationRootId(32) ‖
/// salt(32).
pub fn config_id_preimage(c: &ConfigIdentity) -> [u8; 261] {
    Concat::new()
        .put(TAG_CONFIG)
        .put(&c.bedrock_genesis_id)
        .put(&c.channel_id)
        .put(&c.bridge_header)
        .put(&c.bridge_image_id)
        .put(&c.evm_chain_id)
        .put(&c.vault)
        .put(&c.attestation_root_id)
        .put(&c.salt)
        .finish()
}

// ------------------------------------------------------------------------------------------------
// Trees
// ------------------------------------------------------------------------------------------------

/// Deposit-tree leaf preimage (§3.2), 145 bytes, as the vault builds it:
///
/// | offset | length | field |
/// |---|---|---|
/// | 0 | 1 | `0x00` |
/// | 1 | 32 | [`LEAF_TAG`] |
/// | 33 | 8 | `leaf_index`, u64 |
/// | 41 | 20 | `depositor` |
/// | 61 | 20 | `token` (`0x00…00` = native ETH) |
/// | 81 | 32 | `amount`, uint256 |
/// | 113 | 32 | `C` |
pub fn deposit_leaf_preimage(
    leaf_index: u64,
    depositor: &Address,
    token: &Address,
    amount: &U256,
    commitment: &Hash,
) -> [u8; 145] {
    Concat::new()
        .put(&[LEAF_PREFIX])
        .put(&LEAF_TAG)
        .put(&leaf_index.to_be_bytes())
        .put(depositor)
        .put(token)
        .put(amount)
        .put(commitment)
        .finish()
}

/// Burn-tree leaf preimage (§5.1), 189 bytes:
///
/// | offset | length | field |
/// |---|---|---|
/// | 0 | 1 | `0x02` |
/// | 1 | 32 | [`BURN_TAG`] |
/// | 33 | 32 | `configId` |
/// | 65 | 8 | `burn_index`, u64 |
/// | 73 | 20 | `token` |
/// | 93 | 32 | `amount`, uint256 |
/// | 125 | 32 | `COut` |
/// | 157 | 32 | `r` |
pub fn burn_leaf_preimage(
    config_id: &Hash,
    burn_index: u64,
    token: &Address,
    amount: &U256,
    c_out: &Hash,
    r: &Hash,
) -> [u8; 189] {
    Concat::new()
        .put(&[BURN_LEAF_PREFIX])
        .put(&BURN_TAG)
        .put(config_id)
        .put(&burn_index.to_be_bytes())
        .put(token)
        .put(amount)
        .put(c_out)
        .put(r)
        .finish()
}

/// Tree-node preimage, 65 bytes: `0x01 ‖ left ‖ right`.
pub fn node_preimage(left: &Hash, right: &Hash) -> [u8; 65] {
    Concat::new().put(&[NODE_PREFIX]).put(left).put(right).finish()
}

// ------------------------------------------------------------------------------------------------
// Deposit side (§3.3)
// ------------------------------------------------------------------------------------------------

/// HMAC message (key `kAuth`) for the deposit secret `s`, 92 bytes:
/// `"ferrata/s/v1"`(12) ‖ evmChainId(32) ‖ vault(20) ‖ depositor(20) ‖ j:u64(8).
pub fn deposit_secret_message(chain_id: &U256, vault: &Address, depositor: &Address, j: u64) -> [u8; 92] {
    Concat::new().put(TAG_S).put(chain_id).put(vault).put(depositor).put(&j.to_be_bytes()).finish()
}

/// Preimage of the deposit commitment `C`, 77 bytes: `"ferrata/C/v1"`(12) ‖ s(32) ‖ kind:u8(1) ‖
/// dest(32).
pub fn commitment_preimage(s: &Hash, kind: DestKind, dest: &Hash) -> [u8; 77] {
    Concat::new().put(TAG_C).put(s).put(&[kind as u8]).put(dest).finish()
}

/// Preimage of the accumulator-leaf identifier `fid`, 148 bytes: `"ferrata/fact/acc-leaf/v1"`(24) ‖
/// evmChainId(32) ‖ vault(20) ‖ slot:u256(32, always 0) ‖ leafIndex:u64(8) ‖ leaf(32). It names
/// "leaf `leafIndex` of the accumulator at slot 0 of `vault`", so it is the same under every root.
pub fn deposit_id_preimage(chain_id: &U256, vault: &Address, leaf_index: u64, leaf: &Hash) -> [u8; 148] {
    Concat::new()
        .put(TAG_FACT_ACC_LEAF)
        .put(chain_id)
        .put(vault)
        .put(&DEPOSIT_ROOT_SLOT)
        .put(&leaf_index.to_be_bytes())
        .put(leaf)
        .finish()
}

/// Preimage of the deposit nullifier `N`, 77 bytes: `"ferrata/nf/v1"`(13) ‖ s(32) ‖ fid(32).
pub fn nullifier_preimage(s: &Hash, fid: &Hash) -> [u8; 77] {
    Concat::new().put(TAG_NF).put(s).put(fid).finish()
}

// ------------------------------------------------------------------------------------------------
// Burn side (§5.1, §5.2)
// ------------------------------------------------------------------------------------------------

/// HMAC message (key `kView`) for the burn tag `r_i`, 59 bytes: `"ferrata/burn-tag/v1"`(19) ‖
/// configId(32) ‖ i:u64(8).
pub fn burn_tag_message(config_id: &Hash, i: u64) -> [u8; 59] {
    Concat::new().put(TAG_BURN_TAG).put(config_id).put(&i.to_be_bytes()).finish()
}

/// HMAC message (key `kAuth`) for the release secret `sOut`, 80 bytes: `"ferrata/s-out/v1"`(16) ‖
/// configId(32) ‖ r(32).
pub fn burn_secret_message(config_id: &Hash, r: &Hash) -> [u8; 80] {
    Concat::new().put(TAG_S_OUT).put(config_id).put(r).finish()
}

/// Preimage of the burn commitment `COut`, 119 bytes: `"ferrata/Cout/v1"`(15) ‖ sOut(32) ‖
/// evmChainId(32) ‖ vault(20) ‖ recipient(20).
pub fn burn_commitment_preimage(s_out: &Hash, chain_id: &U256, vault: &Address, recipient: &Address) -> [u8; 119] {
    Concat::new().put(TAG_C_OUT).put(s_out).put(chain_id).put(vault).put(recipient).finish()
}

/// HMAC message (key `kView`) for the memo pad, 47 bytes: `"ferrata/memo/v1"`(15) ‖ r(32). The pad
/// is the first 20 bytes of the HMAC output.
pub fn memo_pad_message(r: &Hash) -> [u8; 47] {
    Concat::new().put(TAG_MEMO).put(r).finish()
}

/// HMAC message (key `kView`) for the burn-note MAC (D12), 155 bytes: `"ferrata/burn-mac/v2"`(19) ‖
/// r(32) ‖ memo(20) ‖ token(20) ‖ amount(32) ‖ COut(32). The MAC is the first 16 bytes of the HMAC
/// output. It covers the memo, so a `kView`-only viewer drops a copied note with another memo.
pub fn burn_mac_message(r: &Hash, memo: &Address, token: &Address, amount: &U256, c_out: &Hash) -> [u8; 155] {
    Concat::new().put(TAG_BURN_MAC).put(r).put(memo).put(token).put(amount).put(c_out).finish()
}

/// Preimage of `burnId`, 87 bytes: `"ferrata/burn/v1"`(15) ‖ configId(32) ‖ burnIndex:u64(8) ‖
/// burnLeaf(32). SPEC-v2's; no longer part of `NOut` (D13).
pub fn burn_id_preimage(config_id: &Hash, burn_index: u64, leaf: &Hash) -> [u8; 87] {
    Concat::new().put(TAG_BURN).put(config_id).put(&burn_index.to_be_bytes()).put(leaf).finish()
}

/// Preimage of the burn nullifier `NOut` (D13), 49 bytes: `"ferrata/nf-out/v2"`(17) ‖ sOut(32). It
/// does not depend on the burn's index, so a burn that a revoked checkpoint collected at one index
/// and the canonical branch at another has one nullifier, and is released once.
pub fn burn_nullifier_preimage(s_out: &Hash) -> [u8; 49] {
    Concat::new().put(TAG_NF_OUT).put(s_out).finish()
}

// ------------------------------------------------------------------------------------------------
// LEZ checkpoints (§4.1, §4.2)
// ------------------------------------------------------------------------------------------------

/// `keySetHash = sha256("ferrata/keyset/v1"(17) ‖ n:u32(4) ‖ key_1(32) ‖ … ‖ key_n(32))`, keys in the
/// channel's `accredited_keys` order. Variable length, so it is hashed incrementally here rather
/// than returned as a preimage. Panics if there are more than `u32::MAX` keys.
pub fn key_set_hash(keys: &[Hash]) -> Hash {
    use sha2::{Digest, Sha256};
    let n = u32::try_from(keys.len()).expect("at most u32::MAX keys");
    let mut h = Sha256::new();
    h.update(TAG_KEYSET);
    h.update(n.to_be_bytes());
    for key in keys {
        h.update(key);
    }
    h.finalize().into()
}

/// Preimage of `burnFrontierHash`, 1048 bytes: `"ferrata/burn-frontier/v1"`(24) ‖ f_0 ‖ … ‖ f_31.
///
/// **SPEC-v2 says only "hash of the incremental burn tree's frontier (32 nodes)"; this layout is
/// this crate's choice and must be agreed with the checkpoint program.** `f_h` is the root of the
/// completed subtree of height `h` on the tree's left edge when bit `h` of the leaf count is 1, and
/// 32 zero bytes when it is 0 ([`crate::tree::IncrementalTree::frontier`]). Zeroing the unused
/// heights makes the hash a function of the leaves alone, independent of how an implementation
/// leaves its stale frontier entries.
pub fn burn_frontier_preimage(frontier: &[Hash; TREE_DEPTH]) -> [u8; 1048] {
    let mut c = Concat::new().put(TAG_BURN_FRONTIER);
    for node in frontier {
        c = c.put(node);
    }
    c.finish()
}

/// The LEZ checkpoint state (§4.1); `S` is the SHA-256 of [`lez_state_preimage`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LezState {
    /// LEZ block id up to and including which burns have been collected.
    pub tip_id: u64,
    /// Hash of that block.
    pub tip_hash: Hash,
    /// [`key_set_hash`] of the current accredited key set.
    pub key_set_hash: Hash,
    /// The channel's configuration-chain tip.
    pub config_tip_hash: Hash,
    /// Number of burns in the burn tree.
    pub burn_count: u64,
    /// SHA-256 of [`burn_frontier_preimage`].
    pub burn_frontier_hash: Hash,
    /// Vault leaf count carried by the configuration's last successful `ImportRoot`.
    pub imported_leaf_count: u64,
    /// Whether the configuration's one-way `Sunset` has executed.
    pub sunset: bool,
}

/// Preimage of `S`, 173 bytes: `"ferrata/lez-state/v1"`(20) ‖ tipId:u64(8) ‖ tipHash(32) ‖
/// keySetHash(32) ‖ configTipHash(32) ‖ burnCount:u64(8) ‖ burnFrontierHash(32) ‖
/// importedLeafCount:u64(8) ‖ sunset:u8(1).
pub fn lez_state_preimage(s: &LezState) -> [u8; 173] {
    Concat::new()
        .put(TAG_LEZ_STATE)
        .put(&s.tip_id.to_be_bytes())
        .put(&s.tip_hash)
        .put(&s.key_set_hash)
        .put(&s.config_tip_hash)
        .put(&s.burn_count.to_be_bytes())
        .put(&s.burn_frontier_hash)
        .put(&s.imported_leaf_count.to_be_bytes())
        .put(&[u8::from(s.sunset)])
        .finish()
}

/// Length of the checkpoint journal: thirteen ABI words.
pub const CHECKPOINT_JOURNAL_LEN: usize = 13 * 32;

/// The checkpoint program's journal (§4.2): `abi.encode(CHECKPOINT_DOMAIN, evmChainId, vault,
/// lezConfigId, nMin, tMin, parentState, newState, burnRoot, burnCount, importedLeafCount, sunset,
/// echoNonce)`, thirteen 32-byte words:
///
/// | word | field | word layout |
/// |---|---|---|
/// | 0 | [`CHECKPOINT_DOMAIN`] | as is |
/// | 1 | `evmChainId` | uint256 |
/// | 2 | `vault` | 12 zero bytes ‖ 20 |
/// | 3 | `lezConfigId` | as is |
/// | 4, 5 | `nMin`, `tMin` | 28 zero bytes ‖ u32 |
/// | 6, 7 | `parentState`, `newState` | as is |
/// | 8 | `burnRoot` | as is |
/// | 9, 10 | `burnCount`, `importedLeafCount` | 24 zero bytes ‖ u64 |
/// | 11 | `sunset` | 31 zero bytes ‖ 0x00 or 0x01 |
/// | 12 | `echoNonce` | as is |
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckpointJournal {
    /// The vault's chain id.
    pub chain_id: U256,
    /// The vault.
    pub vault: Address,
    /// The vault's `lezConfigId` (= `configId`).
    pub lez_config_id: Hash,
    /// Minimum accredited key-set size (deployment constant).
    pub n_min: u32,
    /// Minimum threshold (deployment constant).
    pub t_min: u32,
    /// `S` the checkpoint starts from.
    pub parent_state: Hash,
    /// `S` at the new tip `T`.
    pub new_state: Hash,
    /// Root of the burn tree at `T`.
    pub burn_root: Hash,
    /// Burns in the tree at `T`.
    pub burn_count: u64,
    /// `importedLeafCount` at `T`.
    pub imported_leaf_count: u64,
    /// `sunset` at `T`.
    pub sunset: bool,
    /// 0 for a candidate; the beacon nonce for a finalisation.
    pub echo_nonce: Hash,
}

impl CheckpointJournal {
    /// The 416 journal bytes.
    pub fn encode(&self) -> [u8; CHECKPOINT_JOURNAL_LEN] {
        Concat::new()
            .put(&CHECKPOINT_DOMAIN)
            .put(&self.chain_id)
            .put(&address_word(&self.vault))
            .put(&self.lez_config_id)
            .put(&u64_word(u64::from(self.n_min)))
            .put(&u64_word(u64::from(self.t_min)))
            .put(&self.parent_state)
            .put(&self.new_state)
            .put(&self.burn_root)
            .put(&u64_word(self.burn_count))
            .put(&u64_word(self.imported_leaf_count))
            .put(&u64_word(u64::from(self.sunset)))
            .put(&self.echo_nonce)
            .finish()
    }

    /// Parses journal bytes; `None` unless they are exactly what [`Self::encode`] produces.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; CHECKPOINT_JOURNAL_LEN] = bytes.try_into().ok()?;
        let mut r = Reader::new(bytes);
        if r.take::<32>() != CHECKPOINT_DOMAIN {
            return None;
        }
        let j = Self {
            chain_id: r.take(),
            vault: word_address(&r.take::<32>())?,
            lez_config_id: r.take(),
            n_min: u32::try_from(word_u64(&r.take::<32>())?).ok()?,
            t_min: u32::try_from(word_u64(&r.take::<32>())?).ok()?,
            parent_state: r.take(),
            new_state: r.take(),
            burn_root: r.take(),
            burn_count: word_u64(&r.take::<32>())?,
            imported_leaf_count: word_u64(&r.take::<32>())?,
            sunset: match word_u64(&r.take::<32>())? {
                0 => false,
                1 => true,
                _ => return None,
            },
            echo_nonce: r.take(),
        };
        r.finish();
        Some(j)
    }
}

// ------------------------------------------------------------------------------------------------
// The vault's release journal (§5.2)
// ------------------------------------------------------------------------------------------------

/// Length of the release journal: eleven ABI words.
pub const RELEASE_JOURNAL_LEN: usize = 11 * 32;

/// The release proof's journal: `abi.encode(RELEASE_DOMAIN, evmChainId, vault, lezConfigId,
/// burnRoot, NOut, token, amount, recipient, relayer, relayFee)`, eleven 32-byte words:
///
/// | word | offset | field | word layout |
/// |---|---|---|---|
/// | 0 | 0 | [`RELEASE_DOMAIN`] | as is |
/// | 1 | 32 | `evmChainId` | uint256 |
/// | 2 | 64 | `vault` | 12 zero bytes ‖ 20 |
/// | 3 | 96 | `lezConfigId` | as is |
/// | 4 | 128 | `burnRoot` | as is |
/// | 5 | 160 | `NOut` | as is |
/// | 6 | 192 | `token` | 12 zero bytes ‖ 20 |
/// | 7 | 224 | `amount` | uint256 |
/// | 8 | 256 | `recipient` | 12 zero bytes ‖ 20 |
/// | 9 | 288 | `relayer` | 12 zero bytes ‖ 20 |
/// | 10 | 320 | `relayFee` | uint256 |
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReleaseJournal {
    /// The vault's chain id.
    pub chain_id: U256,
    /// The vault.
    pub vault: Address,
    /// The vault's `lezConfigId` (= `configId`).
    pub lez_config_id: Hash,
    /// The accepted burn root the burn is proven under.
    pub burn_root: Hash,
    /// `NOut`.
    pub nullifier: Hash,
    /// Token (`0x00…00` = native ETH).
    pub token: Address,
    /// Denomination released.
    pub amount: U256,
    /// Who receives `amount - relayFee`.
    pub recipient: Address,
    /// Who receives `relayFee`; `0x00…00` means `msg.sender` (front-runnable).
    pub relayer: Address,
    /// 0, or the registered fee for this denomination.
    pub relay_fee: U256,
}

impl ReleaseJournal {
    /// The 352 journal bytes.
    pub fn encode(&self) -> [u8; RELEASE_JOURNAL_LEN] {
        Concat::new()
            .put(&RELEASE_DOMAIN)
            .put(&self.chain_id)
            .put(&address_word(&self.vault))
            .put(&self.lez_config_id)
            .put(&self.burn_root)
            .put(&self.nullifier)
            .put(&address_word(&self.token))
            .put(&self.amount)
            .put(&address_word(&self.recipient))
            .put(&address_word(&self.relayer))
            .put(&self.relay_fee)
            .finish()
    }

    /// Parses journal bytes; `None` unless they are exactly what [`Self::encode`] produces.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; RELEASE_JOURNAL_LEN] = bytes.try_into().ok()?;
        let mut r = Reader::new(bytes);
        if r.take::<32>() != RELEASE_DOMAIN {
            return None;
        }
        let j = Self {
            chain_id: r.take(),
            vault: word_address(&r.take::<32>())?,
            lez_config_id: r.take(),
            burn_root: r.take(),
            nullifier: r.take(),
            token: word_address(&r.take::<32>())?,
            amount: r.take(),
            recipient: word_address(&r.take::<32>())?,
            relayer: word_address(&r.take::<32>())?,
            relay_fee: r.take(),
        };
        r.finish();
        Some(j)
    }
}

/// A u64 as a uint256 (also its ABI word).
pub fn u64_word(value: u64) -> U256 {
    let mut word = [0u8; 32];
    word[24..].copy_from_slice(&value.to_be_bytes());
    word
}

/// A u128 as a uint256.
pub fn u256_from_u128(value: u128) -> U256 {
    let mut word = [0u8; 32];
    word[16..].copy_from_slice(&value.to_be_bytes());
    word
}

/// An address as an ABI word.
pub fn address_word(address: &Address) -> [u8; 32] {
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(address);
    word
}

fn word_u64(word: &[u8; 32]) -> Option<u64> {
    if word[..24].iter().any(|b| *b != 0) {
        return None;
    }
    Some(u64::from_be_bytes(word[24..].try_into().ok()?))
}

fn word_address(word: &[u8; 32]) -> Option<Address> {
    if word[..12].iter().any(|b| *b != 0) {
        return None;
    }
    word[12..].try_into().ok()
}

// ------------------------------------------------------------------------------------------------
// Private prover inputs (not protocol encodings)
// ------------------------------------------------------------------------------------------------

/// Length of [`ReleaseInput`]'s encoding.
pub const RELEASE_INPUT_LEN: usize = 1340;

/// Everything the release guest reads, 1340 bytes. Only the prover sees it.
///
/// | offset | length | field |
/// |---|---|---|
/// | 0 | 32 | `burnRoot` |
/// | 32 | 8 | witness `burnIndex`, u64 |
/// | 40 | 4 | witness path direction bits, u32 |
/// | 44 | 1024 | witness path siblings, leaf level first |
/// | 1068 | 32 | witness `sOut` |
/// | 1100 | 32 | witness `r` |
/// | 1132 | 32 | `evmChainId` |
/// | 1164 | 20 | `vault` |
/// | 1184 | 32 | `configId` (the journal's `lezConfigId`) |
/// | 1216 | 20 | `token` |
/// | 1236 | 32 | `amount` |
/// | 1268 | 20 | `recipient` |
/// | 1288 | 20 | `relayer` |
/// | 1308 | 32 | `relayFee` |
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseInput {
    /// The accepted burn root to prove membership under.
    pub burn_root: Hash,
    /// The private witness.
    pub witness: ReleaseWitness,
    /// The public statement.
    pub public: ReleasePublic,
}

impl ReleaseInput {
    /// The 1340 input bytes.
    pub fn encode(&self) -> [u8; RELEASE_INPUT_LEN] {
        let (w, p) = (&self.witness, &self.public);
        Concat::new()
            .put(&self.burn_root)
            .put(&w.burn_index.to_be_bytes())
            .put(&w.path.directions.to_be_bytes())
            .put(&siblings_bytes(&w.path))
            .put(&w.s_out)
            .put(&w.r)
            .put(&p.chain_id)
            .put(&p.vault)
            .put(&p.config_id)
            .put(&p.token)
            .put(&p.amount)
            .put(&p.recipient)
            .put(&p.relayer)
            .put(&p.relay_fee)
            .finish()
    }

    /// Parses the 1340 input bytes (every byte string of that length is a valid input).
    pub fn decode(bytes: &[u8; RELEASE_INPUT_LEN]) -> Self {
        let mut r = Reader::new(bytes);
        let burn_root = r.take();
        let burn_index = u64::from_be_bytes(r.take());
        let path = r.path();
        let input = Self {
            burn_root,
            witness: ReleaseWitness { burn_index, path, s_out: r.take(), r: r.take() },
            public: ReleasePublic {
                chain_id: r.take(),
                vault: r.take(),
                config_id: r.take(),
                token: r.take(),
                amount: r.take(),
                recipient: r.take(),
                relayer: r.take(),
                relay_fee: r.take(),
            },
        };
        r.finish();
        input
    }
}

/// Length of [`ClaimInput`]'s encoding.
pub const CLAIM_INPUT_LEN: usize = 1260;
/// Length of [`claim_journal`].
pub const CLAIM_JOURNAL_LEN: usize = 168;

/// Input of the claim measurement guest (not a protocol encoding), 1260 bytes:
///
/// | offset | length | field |
/// |---|---|---|
/// | 0 | 32 | `root` |
/// | 32 | 32 | `evmChainId` |
/// | 64 | 20 | `vault` |
/// | 84 | 20 | `depositor` |
/// | 104 | 20 | `token` |
/// | 124 | 32 | `amount` |
/// | 156 | 8 | `leafIndex`, u64 |
/// | 164 | 4 | path direction bits, u32 |
/// | 168 | 1024 | path siblings |
/// | 1192 | 32 | `s` |
/// | 1224 | 1 | `kind` (0x01 or 0x02; anything else panics on decode) |
/// | 1225 | 3 | zero padding (checked) |
/// | 1228 | 32 | `dest` |
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimInput {
    /// The imported deposit root.
    pub root: Hash,
    /// The vault's chain.
    pub chain_id: U256,
    /// The vault.
    pub vault: Address,
    /// The claim witness.
    pub witness: ClaimWitness,
}

impl ClaimInput {
    /// The 1260 input bytes.
    pub fn encode(&self) -> [u8; CLAIM_INPUT_LEN] {
        let w = &self.witness;
        Concat::new()
            .put(&self.root)
            .put(&self.chain_id)
            .put(&self.vault)
            .put(&w.depositor)
            .put(&w.token)
            .put(&w.amount)
            .put(&w.leaf_index.to_be_bytes())
            .put(&w.path.directions.to_be_bytes())
            .put(&siblings_bytes(&w.path))
            .put(&w.s)
            .put(&[w.kind as u8, 0, 0, 0])
            .put(&w.dest)
            .finish()
    }

    /// Parses the 1260 input bytes. Panics on a kind other than 0x01/0x02 or non-zero padding.
    pub fn decode(bytes: &[u8; CLAIM_INPUT_LEN]) -> Self {
        let mut r = Reader::new(bytes);
        let root = r.take();
        let chain_id = r.take();
        let vault = r.take();
        let (depositor, token, amount) = (r.take(), r.take(), r.take());
        let leaf_index = u64::from_be_bytes(r.take());
        let path = r.path();
        let s = r.take();
        let kind_bytes: [u8; 4] = r.take();
        assert!(kind_bytes[1..] == [0, 0, 0], "non-zero padding");
        let kind = DestKind::from_byte(kind_bytes[0]).expect("kind must be 0x01 or 0x02");
        let input = Self {
            root,
            chain_id,
            vault,
            witness: ClaimWitness { depositor, token, amount, leaf_index, path, s, kind, dest: r.take() },
        };
        r.finish();
        input
    }
}

/// What the claim measurement guest commits (not a protocol encoding), 168 bytes:
/// `root`(32) ‖ `evmChainId`(32) ‖ `vault`(20) ‖ `N`(32) ‖ `token`(20) ‖ `amount`(32). A relayer that
/// verifies such a receipt sees `N` and can refuse a second drop for it.
pub fn claim_journal(
    root: &Hash,
    chain_id: &U256,
    vault: &Address,
    nullifier: &Hash,
    token: &Address,
    amount: &U256,
) -> [u8; CLAIM_JOURNAL_LEN] {
    Concat::new().put(root).put(chain_id).put(vault).put(nullifier).put(token).put(amount).finish()
}

fn siblings_bytes(path: &MerklePath) -> [u8; 32 * TREE_DEPTH] {
    let mut c = Concat::new();
    for sibling in &path.siblings {
        c = c.put(sibling);
    }
    c.finish()
}

// ------------------------------------------------------------------------------------------------
// Fixed-size byte builders
// ------------------------------------------------------------------------------------------------

/// Concatenation into a fixed-size array. Writing past `N` panics, and [`Concat::finish`] panics
/// unless exactly `N` bytes were written, so a layout that disagrees with its declared length
/// fails the first time it is used (every encoding above is exercised by the tests).
struct Concat<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> Concat<N> {
    fn new() -> Self {
        Self { buf: [0u8; N], len: 0 }
    }

    fn put(mut self, bytes: &[u8]) -> Self {
        self.buf[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        self
    }

    fn finish(self) -> [u8; N] {
        assert_eq!(self.len, N, "encoding length mismatch");
        self.buf
    }
}

/// Sequential reads from a fixed-size array; [`Reader::finish`] asserts every byte was read.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take<const M: usize>(&mut self) -> [u8; M] {
        let mut out = [0u8; M];
        out.copy_from_slice(&self.bytes[self.pos..self.pos + M]);
        self.pos += M;
        out
    }

    fn path(&mut self) -> MerklePath {
        let directions = u32::from_be_bytes(self.take());
        let mut siblings = [[0u8; 32]; TREE_DEPTH];
        for sibling in siblings.iter_mut() {
            *sibling = self.take();
        }
        MerklePath { directions, siblings }
    }

    fn finish(self) {
        assert_eq!(self.pos, self.bytes.len(), "trailing bytes");
    }
}

/// Parses 64 lowercase hex digits at compile time (for the constants above).
const fn hex32(s: &str) -> Hash {
    let b = s.as_bytes();
    assert!(b.len() == 64, "expected 64 hex digits");
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (nibble(b[2 * i]) << 4) | nibble(b[2 * i + 1]);
        i += 1;
    }
    out
}

const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("expected lowercase hex"),
    }
}
