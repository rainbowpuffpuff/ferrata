//! Outbound leg (LEZ burn -> Ethereum release).
//!
//! A burn publishes the note `(r, memo, token, amount, C_out)` on LEZ and appends
//! `burn_leaf(leafIndex, token, amount, C_out)` to the LEZ burn tree. Everything secret derives
//! from the bridge key `k` and the burn tag `r`, so the wallet can rediscover its burns (by
//! regenerating `r_0, r_1, …`), decrypt the recipient from the memo and authenticate it against
//! `C_out`. The release proof shows, under a burn root the vault has accepted, knowledge of
//! `s_out` opening a leaf's `C_out` to the journal's chain, vault and recipient, and commits the
//! vault's release journal with `N_out`.

use crate::encoding::{
    burn_commitment_preimage, burn_id_preimage, burn_leaf_preimage, burn_nullifier_preimage, burn_secret_message,
    burn_tag_message, memo_pad_message, ReleaseJournal, RELEASE_JOURNAL_LEN,
};
use crate::tree::{verify_path, Path};
use crate::{sha256, Address, BridgeKey, Hash, VerifyError, U256};

/// Burn tag `r_i = HMAC-SHA256(k, "ferrata/burn-tag/v1" ‖ lez_config_id ‖ i)` for the wallet's
/// `i`-th burn under this configuration (`i = 0, 1, 2, …`).
pub fn burn_tag(key: &BridgeKey, lez_config_id: &Hash, i: u64) -> Hash {
    key.prf(&burn_tag_message(lez_config_id, i))
}

/// Release secret `s_out = HMAC-SHA256(k, "ferrata/s-out/v1" ‖ r)`.
pub fn burn_secret(key: &BridgeKey, r: &Hash) -> Hash {
    key.prf(&burn_secret_message(r))
}

/// Memo pad: `HMAC-SHA256(k, "ferrata/memo/v1" ‖ r)[0..20]`.
pub fn memo_pad(key: &BridgeKey, r: &Hash) -> Address {
    let full = key.prf(&memo_pad_message(r));
    let mut pad = [0u8; 20];
    pad.copy_from_slice(&full[..20]);
    pad
}

/// XORs `bytes` with the memo pad: turns a recipient into the memo and the memo back into the
/// recipient. The memo has no integrity of its own; [`recover_burns`] authenticates the result
/// against `C_out`.
pub fn xor_memo(key: &BridgeKey, r: &Hash, bytes: &Address) -> Address {
    let pad = memo_pad(key, r);
    let mut out = [0u8; 20];
    for (o, (b, p)) in out.iter_mut().zip(bytes.iter().zip(pad.iter())) {
        *o = b ^ p;
    }
    out
}

/// `C_out = sha256("ferrata/Cout/v1" ‖ s_out ‖ chain_id ‖ vault ‖ recipient)`.
pub fn burn_commitment(s_out: &Hash, chain_id: u64, vault: &Address, recipient: &Address) -> Hash {
    sha256(&burn_commitment_preimage(s_out, chain_id, vault, recipient))
}

/// The burn-tree leaf: `sha256(0x00 ‖ BURN_LEAF_TAG ‖ leaf_index ‖ token ‖ amount ‖ C_out)`.
pub fn burn_leaf(leaf_index: u64, token: &Address, amount: &U256, c_out: &Hash) -> Hash {
    sha256(&burn_leaf_preimage(leaf_index, token, amount, c_out))
}

/// Burn id `sha256("ferrata/burn/v1" ‖ lez_config_id ‖ leaf_index ‖ leaf)`.
pub fn burn_id(lez_config_id: &Hash, leaf_index: u64, leaf: &Hash) -> Hash {
    sha256(&burn_id_preimage(lez_config_id, leaf_index, leaf))
}

/// `N_out = sha256("ferrata/nf-out/v1" ‖ s_out ‖ burn_id)`.
pub fn burn_nullifier(s_out: &Hash, burn_id: &Hash) -> Hash {
    sha256(&burn_nullifier_preimage(s_out, burn_id))
}

/// Where a burn can be released: bound into `C_out` at burn time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReleaseTarget {
    /// The vault's chain.
    pub chain_id: u64,
    /// The vault.
    pub vault: Address,
    /// The Ethereum recipient.
    pub recipient: Address,
}

/// The burn note published on LEZ: `(r, memo, token, amount, C_out)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BurnNote {
    /// Burn tag.
    pub r: Hash,
    /// Recipient XOR memo pad.
    pub memo: Address,
    /// Ethereum token to release (`0x00…00` = native ETH).
    pub token: Address,
    /// Amount to release.
    pub amount: U256,
    /// Burn commitment.
    pub c_out: Hash,
}

impl BurnNote {
    /// The note for the wallet's `i`-th burn under `lez_config_id`.
    pub fn create(
        key: &BridgeKey,
        lez_config_id: &Hash,
        i: u64,
        target: &ReleaseTarget,
        token: Address,
        amount: U256,
    ) -> Self {
        let r = burn_tag(key, lez_config_id, i);
        let s_out = burn_secret(key, &r);
        Self {
            r,
            memo: xor_memo(key, &r, &target.recipient),
            token,
            amount,
            c_out: burn_commitment(&s_out, target.chain_id, &target.vault, &target.recipient),
        }
    }
}

/// The release proof's private witness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseWitness {
    /// The burn's leaf index in the burn tree.
    pub leaf_index: u64,
    /// Merkle path of the burn leaf under the burn root.
    pub path: Path,
    /// The release secret.
    pub s_out: Hash,
}

/// The release statement's public fields (with the burn root and `N_out`, the journal).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleasePublic {
    /// `block.chainid` of the vault's chain.
    pub chain_id: u64,
    /// The vault.
    pub vault: Address,
    /// The vault's `lezConfigId`.
    pub lez_config_id: Hash,
    /// Token to release.
    pub token: Address,
    /// Amount to release.
    pub amount: U256,
    /// Recipient bound in `C_out`.
    pub recipient: Address,
    /// Relay-fee recipient chosen at proving time (`0x00…00`: the transaction sender).
    pub fee_recipient: Address,
    /// Relay fee chosen at proving time.
    pub relay_fee: U256,
}

/// The release statement: recomputes `C_out` from the witness secret and the public chain,
/// vault and recipient, the burn leaf from the public token and amount, and its path to
/// `burn_root`, and returns `N_out`.
///
/// `fee_recipient` and `relay_fee` are not checked here: the prover chooses them, and the journal
/// binds them, so whoever submits the proof cannot change them.
pub fn verify_release(burn_root: &Hash, w: &ReleaseWitness, p: &ReleasePublic) -> Result<Hash, VerifyError> {
    let c_out = burn_commitment(&w.s_out, p.chain_id, &p.vault, &p.recipient);
    let leaf = burn_leaf(w.leaf_index, &p.token, &p.amount, &c_out);
    verify_path(burn_root, &leaf, w.leaf_index, &w.path)?;
    Ok(burn_nullifier(&w.s_out, &burn_id(&p.lez_config_id, w.leaf_index, &leaf)))
}

/// The release journal bytes for a verified release (layout: [`ReleaseJournal`]).
pub fn release_journal(burn_root: &Hash, nullifier: &Hash, p: &ReleasePublic) -> [u8; RELEASE_JOURNAL_LEN] {
    ReleaseJournal {
        chain_id: p.chain_id,
        vault: p.vault,
        lez_config_id: p.lez_config_id,
        burn_root: *burn_root,
        nullifier: *nullifier,
        token: p.token,
        amount: p.amount,
        recipient: p.recipient,
        fee_recipient: p.fee_recipient,
        relay_fee: p.relay_fee,
    }
    .encode()
}

/// A burn note together with its position in the burn tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublishedBurn {
    /// Leaf index of the note's leaf.
    pub leaf_index: u64,
    /// The note.
    pub note: BurnNote,
}

/// A burn the key recognised as its own, with everything needed to release it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedBurn {
    /// The wallet's burn counter value that produced `r`.
    pub i: u64,
    /// The published burn.
    pub burn: PublishedBurn,
    /// The recipient decrypted from the memo and authenticated by `C_out`.
    pub recipient: Address,
    /// The release secret.
    pub s_out: Hash,
    /// The burn leaf.
    pub leaf: Hash,
    /// The nullifier the release will publish.
    pub nullifier: Hash,
}

/// Result of a burn recovery scan.
#[cfg(feature = "std")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BurnScan {
    /// Own burns, by burn counter and then leaf index.
    pub owned: Vec<OwnedBurn>,
    /// First burn counter value after the last one seen in use: the next burn's `i`.
    pub next_i: u64,
}

/// Wallet recovery of burns: regenerates `r_0, r_1, …` until `gap_limit` consecutive values
/// appear in no note, decrypts the memo of every note carrying one of them, and keeps the notes
/// whose recomputed `C_out` matches (a copied `r` with a forged memo or `C_out` is skipped, but
/// still counts as a used `i`). One hash-map pass over the notes plus three HMACs and one SHA-256
/// per tag checked.
#[cfg(feature = "std")]
pub fn recover_burns(
    key: &BridgeKey,
    lez_config_id: &Hash,
    chain_id: u64,
    vault: &Address,
    notes: &[PublishedBurn],
    gap_limit: u64,
) -> BurnScan {
    let mut by_tag: std::collections::HashMap<Hash, Vec<&PublishedBurn>> = std::collections::HashMap::new();
    for burn in notes {
        by_tag.entry(burn.note.r).or_default().push(burn);
    }
    let mut owned = Vec::new();
    let mut next_i = 0u64;
    let mut i = 0u64;
    while i - next_i < gap_limit {
        let r = burn_tag(key, lez_config_id, i);
        if let Some(candidates) = by_tag.get(&r) {
            next_i = i + 1;
            let s_out = burn_secret(key, &r);
            for burn in candidates {
                let recipient = xor_memo(key, &r, &burn.note.memo);
                if burn_commitment(&s_out, chain_id, vault, &recipient) != burn.note.c_out {
                    continue;
                }
                let leaf = burn_leaf(burn.leaf_index, &burn.note.token, &burn.note.amount, &burn.note.c_out);
                let nullifier = burn_nullifier(&s_out, &burn_id(lez_config_id, burn.leaf_index, &leaf));
                owned.push(OwnedBurn { i, burn: **burn, recipient, s_out, leaf, nullifier });
            }
        }
        i += 1;
    }
    owned.sort_by_key(|o| (o.i, o.burn.leaf_index));
    BurnScan { owned, next_i }
}
