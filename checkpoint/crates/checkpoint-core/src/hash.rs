//! SHA-256 (the bridge's hash and LEZ's), Blake2b-256 (Logos Blockchain's), and the threshold rule.

use blake2::digest::typenum::U32;
use sha2::{Digest as _, Sha256};

use crate::Hash;

/// Logos Blockchain's hasher: `blake2::Blake2b<U32>` (`core/src/crypto.rs` at the pinned rev), i.e.
/// BLAKE2b with a 32-byte digest length parameter (not BLAKE2b-512 truncated).
pub type Blake2b256 = blake2::Blake2b<U32>;

pub fn sha256(data: &[u8]) -> Hash {
    Sha256::digest(data).into()
}

/// SHA-256 of the concatenation of `parts`.
pub fn sha256_parts(parts: &[&[u8]]) -> Hash {
    let mut h = Sha256::new();
    for part in parts {
        h.update(part);
    }
    h.finalize().into()
}

/// BLAKE2b-256 of the concatenation of `parts`.
pub fn blake2b256_parts(parts: &[&[u8]]) -> Hash {
    use blake2::Digest as _;
    let mut h = Blake2b256::new();
    for part in parts {
        h.update(part);
    }
    h.finalize().into()
}

/// LEZ's signing threshold for `n` accredited keys, `t(n) = max(1, min(⌈2n/3⌉, n − 1))`
/// (LEZ `lez/sequencer/core/src/committee_discovery.rs` `channel_config_threshold`; SPEC-v2 §4.2).
pub fn threshold(n: u32) -> u32 {
    let two_thirds_up = (2 * u64::from(n)).div_ceil(3);
    let cap = u64::from(n.saturating_sub(1));
    // At most n, so the cast back is lossless.
    two_thirds_up.min(cap).max(1) as u32
}

/// `sha256("ferrata/burn/leaf/v2")` (SPEC-v2 §5.1).
pub const BURN_TAG: Hash = hex32("93474c5ad2f57f5fc610ad22e3f6d50e51b98a4c0eb611dde536a7317c0e8cf2");
/// `sha256("ferrata/vault/checkpoint/v3")` (SPEC-v3 D3, replacing SPEC-v2 §4.2 item 5's v2 domain).
pub const CHECKPOINT_DOMAIN: Hash = hex32("bdb3b52df781536a6bcce7088d156b3951a00a45ec8bcf2805ba8406fbeb5ec7");

/// ASCII tags that prefix preimages as they are (SPEC-v2: "Tags are the ASCII bytes of the quoted
/// string").
pub const TAG_CONFIG: &[u8] = b"ferrata/config/v1";
pub const TAG_LEZ_STATE: &[u8] = b"ferrata/lez-state/v1";
/// SPEC-v3 D3: the key-set hash binds the channel's posting timeframe (v2 of the encoding).
pub const TAG_KEYSET: &[u8] = attestor_core::TAG_KEYSET;
/// Not in SPEC-v2 (which says only "hash of the incremental burn tree's frontier"); the encoding
/// `spike/ferrata-bridge` chose, adopted here so both implementations agree.
pub const TAG_BURN_FRONTIER: &[u8] = b"ferrata/burn-frontier/v1";
/// This crate's own: the journal of a bounded segment (composition, see `rule::run_segment`).
pub const TAG_SEGMENT: &[u8] = b"ferrata/checkpoint-segment/v1";

/// Decodes 64 hex digits at compile time.
pub const fn hex32(s: &str) -> Hash {
    let b = s.as_bytes();
    assert!(b.len() == 64, "hex32 needs 64 hex digits");
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
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("not a hex digit"),
    }
}
