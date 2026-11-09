//! Every byte layout of the bridge, defined once.
//!
//! Conventions:
//! * integers are fixed-width big-endian: `u64` is 8 bytes, `uint256` is 32 bytes;
//! * an Ethereum address is its 20 bytes, except inside the ABI-encoded release journal, where
//!   it is a 32-byte word with 12 leading zero bytes (as `abi.encode` writes it);
//! * `‖` is plain concatenation, with no length prefixes;
//! * the two leaf tags and the vault's release domain are the SHA-256 of their ASCII string
//!   (32 bytes); every other tag is its ASCII string itself, prefixed byte for byte.
//!
//! Domain separation: tree leaves start with `0x00`, tree nodes with `0x01`, every other SHA-256
//! preimage and every HMAC message with one of the ASCII tags below (first byte `f`). No tag is a
//! prefix of another, and each kind of preimage has one fixed length, so preimages of different
//! kinds never coincide.
//!
//! | value | hash | length | preimage / message |
//! |---|---|---|---|
//! | deposit leaf | SHA-256 | 125 | `0x00 ‖ LEAF_TAG ‖ leafIndex:u64 ‖ token:20 ‖ amount:u256 ‖ C:32` |
//! | burn leaf | SHA-256 | 125 | `0x00 ‖ BURN_LEAF_TAG ‖ leafIndex:u64 ‖ token:20 ‖ amount:u256 ‖ C_out:32` |
//! | tree node | SHA-256 | 65 | `0x01 ‖ left:32 ‖ right:32` |
//! | `s` | HMAC(k) | 68 | `"ferrata/s/v1" ‖ chainId:u64 ‖ vault:20 ‖ depositor:20 ‖ j:u64` |
//! | `C` | SHA-256 | 76 | `"ferrata/C/v1" ‖ s:32 ‖ dest:32` |
//! | `fid` | SHA-256 | 86 | `"ferrata/deposit/v1" ‖ chainId:u64 ‖ vault:20 ‖ leafIndex:u64 ‖ leaf:32` |
//! | `N` | SHA-256 | 77 | `"ferrata/nf/v1" ‖ s:32 ‖ fid:32` |
//! | `r_i` | HMAC(k) | 59 | `"ferrata/burn-tag/v1" ‖ lezConfigId:32 ‖ i:u64` |
//! | `s_out` | HMAC(k) | 48 | `"ferrata/s-out/v1" ‖ r:32` |
//! | memo pad | HMAC(k), first 20 bytes | 47 | `"ferrata/memo/v1" ‖ r:32` |
//! | `C_out` | SHA-256 | 95 | `"ferrata/Cout/v1" ‖ s_out:32 ‖ chainId:u64 ‖ vault:20 ‖ recipient:20` |
//! | burn id | SHA-256 | 87 | `"ferrata/burn/v1" ‖ lezConfigId:32 ‖ leafIndex:u64 ‖ leaf:32` |
//! | `N_out` | SHA-256 | 81 | `"ferrata/nf-out/v1" ‖ s_out:32 ‖ burnId:32` |
//! | release journal | (SHA-256 by the vault) | 352 | see [`ReleaseJournal`] |
//!
//! Two further layouts are private prover inputs, not protocol encodings: [`ReleaseInput`] (the
//! release guest's input) and [`ClaimInput`] / [`claim_journal`] (a test vehicle that runs the
//! claim statement inside the zkVM).

use crate::outbound::{ReleasePublic, ReleaseWitness};
use crate::tree::{Path, TREE_DEPTH};

/// A SHA-256 digest or any other 32-byte value.
pub type Hash = [u8; 32];
/// An Ethereum address.
pub type Address = [u8; 20];
/// A `uint256`, 32 bytes big-endian.
pub type U256 = [u8; 32];

/// First byte of every tree-leaf preimage.
pub const LEAF_PREFIX: u8 = 0x00;
/// First byte of every tree-node preimage.
pub const NODE_PREFIX: u8 = 0x01;

/// `sha256("ferrata/vault/leaf/v1")`, as `FerrataVault.LEAF_TAG`.
pub const LEAF_TAG: Hash = hex32("084d5599858dde3bdadaec9a70979121276922b791de25071d145ac6b13c4286");
/// `sha256("ferrata/burn/leaf/v1")`.
pub const BURN_LEAF_TAG: Hash = hex32("b9a876f3673cc2c3546df046edc81938bcdbc115e53e6b0614b94bf16ae7f53b");
/// `sha256("ferrata/vault/release/v1")`, as `FerrataVault.RELEASE_DOMAIN`.
pub const RELEASE_DOMAIN: Hash = hex32("38d947b280ef2710d905d714dc6c55d9fdaa844e4769df08152f6bed4bc9f444");

/// String whose SHA-256 is [`LEAF_TAG`].
pub const LEAF_TAG_STR: &str = "ferrata/vault/leaf/v1";
/// String whose SHA-256 is [`BURN_LEAF_TAG`].
pub const BURN_LEAF_TAG_STR: &str = "ferrata/burn/leaf/v1";
/// String whose SHA-256 is [`RELEASE_DOMAIN`].
pub const RELEASE_DOMAIN_STR: &str = "ferrata/vault/release/v1";

/// Prefix of the HMAC message for the deposit secret `s`.
pub const TAG_S: &[u8] = b"ferrata/s/v1";
/// Prefix of the preimage of the deposit commitment `C`.
pub const TAG_C: &[u8] = b"ferrata/C/v1";
/// Prefix of the preimage of the deposit id `fid`.
pub const TAG_DEPOSIT: &[u8] = b"ferrata/deposit/v1";
/// Prefix of the preimage of the deposit nullifier `N`.
pub const TAG_NF: &[u8] = b"ferrata/nf/v1";
/// Prefix of the HMAC message for the burn tag `r_i`.
pub const TAG_BURN_TAG: &[u8] = b"ferrata/burn-tag/v1";
/// Prefix of the HMAC message for the release secret `s_out`.
pub const TAG_S_OUT: &[u8] = b"ferrata/s-out/v1";
/// Prefix of the HMAC message for the memo pad.
pub const TAG_MEMO: &[u8] = b"ferrata/memo/v1";
/// Prefix of the preimage of the burn commitment `C_out`.
pub const TAG_C_OUT: &[u8] = b"ferrata/Cout/v1";
/// Prefix of the preimage of the burn id.
pub const TAG_BURN: &[u8] = b"ferrata/burn/v1";
/// Prefix of the preimage of the burn nullifier `N_out`.
pub const TAG_NF_OUT: &[u8] = b"ferrata/nf-out/v1";

/// Every ASCII tag that is prefixed as is (HMAC messages and tagged SHA-256 preimages).
pub const PREFIX_TAGS: [&[u8]; 10] =
    [TAG_S, TAG_C, TAG_DEPOSIT, TAG_NF, TAG_BURN_TAG, TAG_S_OUT, TAG_MEMO, TAG_C_OUT, TAG_BURN, TAG_NF_OUT];

// ------------------------------------------------------------------------------------------------
// Tree
// ------------------------------------------------------------------------------------------------

/// Deposit-tree leaf preimage, 125 bytes:
///
/// | offset | length | field |
/// |---|---|---|
/// | 0 | 1 | `0x00` |
/// | 1 | 32 | [`LEAF_TAG`] |
/// | 33 | 8 | `leaf_index`, u64 |
/// | 41 | 20 | `token` (`0x00…00` = native ETH) |
/// | 61 | 32 | `amount`, uint256 |
/// | 93 | 32 | `commitment` (`C`) |
///
/// Identical to `FerrataVault.leafHash`'s `abi.encodePacked(LEAF_PREFIX, LEAF_TAG, uint64
/// leafIndex, address token, uint256 amount, bytes32 commitment)`.
pub fn deposit_leaf_preimage(leaf_index: u64, token: &Address, amount: &U256, commitment: &Hash) -> [u8; 125] {
    leaf_preimage(&LEAF_TAG, leaf_index, token, amount, commitment)
}

/// Burn-tree leaf preimage, 125 bytes: the deposit-leaf layout with [`BURN_LEAF_TAG`] at offset 1
/// and `C_out` at offset 93.
pub fn burn_leaf_preimage(leaf_index: u64, token: &Address, amount: &U256, c_out: &Hash) -> [u8; 125] {
    leaf_preimage(&BURN_LEAF_TAG, leaf_index, token, amount, c_out)
}

fn leaf_preimage(tag: &Hash, leaf_index: u64, token: &Address, amount: &U256, commitment: &Hash) -> [u8; 125] {
    Concat::new()
        .put(&[LEAF_PREFIX])
        .put(tag)
        .put(&leaf_index.to_be_bytes())
        .put(token)
        .put(amount)
        .put(commitment)
        .finish()
}

/// Tree-node preimage, 65 bytes: `0x01 ‖ left ‖ right`.
pub fn node_preimage(left: &Hash, right: &Hash) -> [u8; 65] {
    Concat::new().put(&[NODE_PREFIX]).put(left).put(right).finish()
}

// ------------------------------------------------------------------------------------------------
// Inbound: deposit -> mint
// ------------------------------------------------------------------------------------------------

/// HMAC message for the deposit secret `s`, 68 bytes:
/// `"ferrata/s/v1"`(12) ‖ `chain_id`:u64(8) ‖ `vault`(20) ‖ `depositor`(20) ‖ `j`:u64(8).
pub fn deposit_secret_message(chain_id: u64, vault: &Address, depositor: &Address, j: u64) -> [u8; 68] {
    Concat::new().put(TAG_S).put(&chain_id.to_be_bytes()).put(vault).put(depositor).put(&j.to_be_bytes()).finish()
}

/// Preimage of the deposit commitment `C`, 76 bytes: `"ferrata/C/v1"`(12) ‖ `s`(32) ‖ `dest`(32).
pub fn commitment_preimage(s: &Hash, dest: &Hash) -> [u8; 76] {
    Concat::new().put(TAG_C).put(s).put(dest).finish()
}

/// Preimage of the deposit id `fid`, 86 bytes:
/// `"ferrata/deposit/v1"`(18) ‖ `chain_id`:u64(8) ‖ `vault`(20) ‖ `leaf_index`:u64(8) ‖ `leaf`(32).
pub fn deposit_id_preimage(chain_id: u64, vault: &Address, leaf_index: u64, leaf: &Hash) -> [u8; 86] {
    Concat::new()
        .put(TAG_DEPOSIT)
        .put(&chain_id.to_be_bytes())
        .put(vault)
        .put(&leaf_index.to_be_bytes())
        .put(leaf)
        .finish()
}

/// Preimage of the deposit nullifier `N`, 77 bytes: `"ferrata/nf/v1"`(13) ‖ `s`(32) ‖ `fid`(32).
pub fn nullifier_preimage(s: &Hash, fid: &Hash) -> [u8; 77] {
    Concat::new().put(TAG_NF).put(s).put(fid).finish()
}

// ------------------------------------------------------------------------------------------------
// Outbound: burn -> release
// ------------------------------------------------------------------------------------------------

/// HMAC message for the burn tag `r_i`, 59 bytes:
/// `"ferrata/burn-tag/v1"`(19) ‖ `lez_config_id`(32) ‖ `i`:u64(8).
pub fn burn_tag_message(lez_config_id: &Hash, i: u64) -> [u8; 59] {
    Concat::new().put(TAG_BURN_TAG).put(lez_config_id).put(&i.to_be_bytes()).finish()
}

/// HMAC message for the release secret `s_out`, 48 bytes: `"ferrata/s-out/v1"`(16) ‖ `r`(32).
pub fn burn_secret_message(r: &Hash) -> [u8; 48] {
    Concat::new().put(TAG_S_OUT).put(r).finish()
}

/// HMAC message for the memo pad, 47 bytes: `"ferrata/memo/v1"`(15) ‖ `r`(32). The pad is the
/// first 20 bytes of the HMAC output.
pub fn memo_pad_message(r: &Hash) -> [u8; 47] {
    Concat::new().put(TAG_MEMO).put(r).finish()
}

/// Preimage of the burn commitment `C_out`, 95 bytes:
/// `"ferrata/Cout/v1"`(15) ‖ `s_out`(32) ‖ `chain_id`:u64(8) ‖ `vault`(20) ‖ `recipient`(20).
pub fn burn_commitment_preimage(s_out: &Hash, chain_id: u64, vault: &Address, recipient: &Address) -> [u8; 95] {
    Concat::new().put(TAG_C_OUT).put(s_out).put(&chain_id.to_be_bytes()).put(vault).put(recipient).finish()
}

/// Preimage of the burn id, 87 bytes:
/// `"ferrata/burn/v1"`(15) ‖ `lez_config_id`(32) ‖ `leaf_index`:u64(8) ‖ `leaf`(32).
pub fn burn_id_preimage(lez_config_id: &Hash, leaf_index: u64, leaf: &Hash) -> [u8; 87] {
    Concat::new().put(TAG_BURN).put(lez_config_id).put(&leaf_index.to_be_bytes()).put(leaf).finish()
}

/// Preimage of the burn nullifier `N_out`, 81 bytes: `"ferrata/nf-out/v1"`(17) ‖ `s_out`(32) ‖
/// `burn_id`(32).
pub fn burn_nullifier_preimage(s_out: &Hash, burn_id: &Hash) -> [u8; 81] {
    Concat::new().put(TAG_NF_OUT).put(s_out).put(burn_id).finish()
}

// ------------------------------------------------------------------------------------------------
// The vault's release journal
// ------------------------------------------------------------------------------------------------

/// Length of the release journal: eleven ABI words.
pub const RELEASE_JOURNAL_LEN: usize = 11 * 32;

/// The public journal of a release proof: exactly the bytes `FerrataVault._releaseJournalDigest`
/// hashes, `abi.encode(RELEASE_DOMAIN, block.chainid, address(this), lezConfigId, burnRoot,
/// nullifier, token, amount, recipient, feeRecipient, relayFee)`. Every field is a static ABI
/// type, so `abi.encode` writes eleven 32-byte words and nothing else:
///
/// | word | offset | field | Solidity type | word layout |
/// |---|---|---|---|---|
/// | 0 | 0 | [`RELEASE_DOMAIN`] | bytes32 | as is |
/// | 1 | 32 | `chain_id` | uint256 | 24 zero bytes ‖ u64 |
/// | 2 | 64 | `vault` | address | 12 zero bytes ‖ 20 |
/// | 3 | 96 | `lez_config_id` | bytes32 | as is |
/// | 4 | 128 | `burn_root` | bytes32 | as is |
/// | 5 | 160 | `nullifier` (`N_out`) | bytes32 | as is |
/// | 6 | 192 | `token` | address | 12 zero bytes ‖ 20 |
/// | 7 | 224 | `amount` | uint256 | as is |
/// | 8 | 256 | `recipient` | address | 12 zero bytes ‖ 20 |
/// | 9 | 288 | `fee_recipient` | address | 12 zero bytes ‖ 20 |
/// | 10 | 320 | `relay_fee` | uint256 | as is |
///
/// The vault's chain id is a uint256; this encoding (like `s`, `fid` and `C_out`) carries a u64,
/// so a chain whose id does not fit in 64 bits cannot be served.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReleaseJournal {
    /// `block.chainid` of the vault's chain.
    pub chain_id: u64,
    /// The vault's address.
    pub vault: Address,
    /// The LEZ configuration the vault is paired with.
    pub lez_config_id: Hash,
    /// The accepted burn root the burn is proven under.
    pub burn_root: Hash,
    /// `N_out`.
    pub nullifier: Hash,
    /// Token (`0x00…00` = native ETH).
    pub token: Address,
    /// Denomination released, in the token's base units.
    pub amount: U256,
    /// Who receives `amount - relay_fee`.
    pub recipient: Address,
    /// Who receives `relay_fee`; `0x00…00` means the vault pays it to `msg.sender`.
    pub fee_recipient: Address,
    /// Relay fee (0, or the vault's standard fee for this denomination).
    pub relay_fee: U256,
}

impl ReleaseJournal {
    /// The 352 journal bytes.
    pub fn encode(&self) -> [u8; RELEASE_JOURNAL_LEN] {
        Concat::new()
            .put(&RELEASE_DOMAIN)
            .put(&u64_word(self.chain_id))
            .put(&address_word(&self.vault))
            .put(&self.lez_config_id)
            .put(&self.burn_root)
            .put(&self.nullifier)
            .put(&address_word(&self.token))
            .put(&self.amount)
            .put(&address_word(&self.recipient))
            .put(&address_word(&self.fee_recipient))
            .put(&self.relay_fee)
            .finish()
    }

    /// Parses journal bytes. `None` unless `bytes` is exactly a journal [`Self::encode`] could
    /// have produced: 352 bytes, the release domain, zero padding in every address word and in
    /// the high 24 bytes of the chain-id word.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; RELEASE_JOURNAL_LEN] = bytes.try_into().ok()?;
        let mut r = Reader::new(bytes);
        if r.take::<32>() != RELEASE_DOMAIN {
            return None;
        }
        let journal = Self {
            chain_id: word_u64(&r.take::<32>())?,
            vault: word_address(&r.take::<32>())?,
            lez_config_id: r.take(),
            burn_root: r.take(),
            nullifier: r.take(),
            token: word_address(&r.take::<32>())?,
            amount: r.take(),
            recipient: word_address(&r.take::<32>())?,
            fee_recipient: word_address(&r.take::<32>())?,
            relay_fee: r.take(),
        };
        r.finish();
        Some(journal)
    }
}

/// A u64 as a uint256 ABI word.
pub fn u64_word(value: u64) -> [u8; 32] {
    let mut word = [0u8; 32];
    word[24..].copy_from_slice(&value.to_be_bytes());
    word
}

/// An address as an ABI word.
pub fn address_word(address: &Address) -> [u8; 32] {
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(address);
    word
}

/// A u128 as a uint256 (big-endian, high 16 bytes zero).
pub fn u256_from_u128(value: u128) -> U256 {
    let mut word = [0u8; 32];
    word[16..].copy_from_slice(&value.to_be_bytes());
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
pub const RELEASE_INPUT_LEN: usize = 1280;

/// Everything the release guest reads, 1280 bytes. Only the prover sees it; the guest commits
/// the [`ReleaseJournal`].
///
/// | offset | length | field |
/// |---|---|---|
/// | 0 | 32 | `burn_root` |
/// | 32 | 8 | witness `leaf_index`, u64 |
/// | 40 | 1024 | witness `path`: 32 siblings, leaf level first |
/// | 1064 | 32 | witness `s_out` |
/// | 1096 | 8 | `chain_id`, u64 |
/// | 1104 | 20 | `vault` |
/// | 1124 | 32 | `lez_config_id` |
/// | 1156 | 20 | `token` |
/// | 1176 | 32 | `amount`, uint256 |
/// | 1208 | 20 | `recipient` |
/// | 1228 | 20 | `fee_recipient` |
/// | 1248 | 32 | `relay_fee`, uint256 |
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
    /// The 1280 input bytes.
    pub fn encode(&self) -> [u8; RELEASE_INPUT_LEN] {
        let (w, p) = (&self.witness, &self.public);
        Concat::new()
            .put(&self.burn_root)
            .put(&w.leaf_index.to_be_bytes())
            .put(&path_bytes(&w.path))
            .put(&w.s_out)
            .put(&p.chain_id.to_be_bytes())
            .put(&p.vault)
            .put(&p.lez_config_id)
            .put(&p.token)
            .put(&p.amount)
            .put(&p.recipient)
            .put(&p.fee_recipient)
            .put(&p.relay_fee)
            .finish()
    }

    /// Parses the 1280 input bytes (every byte string of that length is a valid input).
    pub fn decode(bytes: &[u8; RELEASE_INPUT_LEN]) -> Self {
        let mut r = Reader::new(bytes);
        let input = Self {
            burn_root: r.take(),
            witness: ReleaseWitness { leaf_index: u64::from_be_bytes(r.take()), path: r.path(), s_out: r.take() },
            public: ReleasePublic {
                chain_id: u64::from_be_bytes(r.take()),
                vault: r.take(),
                lez_config_id: r.take(),
                token: r.take(),
                amount: r.take(),
                recipient: r.take(),
                fee_recipient: r.take(),
                relay_fee: r.take(),
            },
        };
        r.finish();
        input
    }
}

/// Length of [`ClaimInput`]'s encoding.
pub const CLAIM_INPUT_LEN: usize = 1208;
/// Length of [`claim_journal`].
pub const CLAIM_JOURNAL_LEN: usize = 116;

/// Test vehicle, not a protocol encoding: the input of a zkVM guest that runs
/// [`crate::inbound::verify_claim`], so tests can check that a claim proven privately yields the
/// same nullifier as the same claim made in public. 1208 bytes:
///
/// | offset | length | field |
/// |---|---|---|
/// | 0 | 32 | `root` |
/// | 32 | 8 | `chain_id`, u64 |
/// | 40 | 20 | `vault` |
/// | 60 | 20 | `token` |
/// | 80 | 32 | `amount`, uint256 |
/// | 112 | 8 | `leaf_index`, u64 |
/// | 120 | 1024 | `path` |
/// | 1144 | 32 | `s` |
/// | 1176 | 32 | `dest` |
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimInput {
    /// The imported deposit root.
    pub root: Hash,
    /// The vault's chain.
    pub chain_id: u64,
    /// The vault.
    pub vault: Address,
    /// The claim witness.
    pub witness: crate::inbound::ClaimWitness,
}

impl ClaimInput {
    /// The 1208 input bytes.
    pub fn encode(&self) -> [u8; CLAIM_INPUT_LEN] {
        let w = &self.witness;
        Concat::new()
            .put(&self.root)
            .put(&self.chain_id.to_be_bytes())
            .put(&self.vault)
            .put(&w.token)
            .put(&w.amount)
            .put(&w.leaf_index.to_be_bytes())
            .put(&path_bytes(&w.path))
            .put(&w.s)
            .put(&w.dest)
            .finish()
    }

    /// Parses the 1208 input bytes.
    pub fn decode(bytes: &[u8; CLAIM_INPUT_LEN]) -> Self {
        let mut r = Reader::new(bytes);
        let input = Self {
            root: r.take(),
            chain_id: u64::from_be_bytes(r.take()),
            vault: r.take(),
            witness: crate::inbound::ClaimWitness {
                token: r.take(),
                amount: r.take(),
                leaf_index: u64::from_be_bytes(r.take()),
                path: r.path(),
                s: r.take(),
                dest: r.take(),
            },
        };
        r.finish();
        input
    }
}

/// Test vehicle, not a protocol encoding: what the claim guest commits, 116 bytes:
/// `root`(32) ‖ `nullifier`(32) ‖ `token`(20) ‖ `amount`(32).
pub fn claim_journal(root: &Hash, nullifier: &Hash, token: &Address, amount: &U256) -> [u8; CLAIM_JOURNAL_LEN] {
    Concat::new().put(root).put(nullifier).put(token).put(amount).finish()
}

fn path_bytes(path: &Path) -> [u8; 32 * TREE_DEPTH] {
    let mut c = Concat::new();
    for sibling in path {
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

    fn path(&mut self) -> Path {
        let mut path = [[0u8; 32]; TREE_DEPTH];
        for sibling in path.iter_mut() {
            *sibling = self.take();
        }
        path
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
