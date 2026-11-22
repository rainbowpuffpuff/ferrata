//! Outbound leg (LEZ burn -> Ethereum release), SPEC-v2 §5.
//!
//! A burn publishes the note `(r, memo, mac, token, amount, COut)` on LEZ and appends
//! `burnLeaf(configId, burnIndex, token, amount, COut, r)` to the configuration's burn tree. Tags,
//! memo pads and MACs come from `kView`; the release secret `sOut` from `kAuth`. A wallet finds its
//! burns by regenerating `r_0, r_1, …`, keeps only notes whose MAC it can verify (so a copied tag on
//! someone else's burn, or a copied note with another memo, is ignored: D12's MAC covers the memo),
//! decrypts the recipient and authenticates it against `COut`. The release proof shows, under an
//! accepted burn root, a burn leaf of this configuration with this token and amount whose `COut` the
//! prover opens to the journal's chain, vault and recipient, and publishes D13's `NOut`, which does
//! not depend on the burn's index.

use crate::encoding::{
    burn_commitment_preimage, burn_id_preimage, burn_leaf_preimage, burn_mac_message, burn_nullifier_preimage,
    burn_secret_message, burn_tag_message, memo_pad_message, Mac, ReleaseJournal, RELEASE_JOURNAL_LEN,
};
use crate::tree::{verify_path, MerklePath};
use crate::{sha256, Address, BridgeKey, BridgeKeys, Hash, VerifyError, U256};

/// Burn tag `r_i = HMAC(kView, "ferrata/burn-tag/v1" ‖ configId ‖ i)`. The wallet advances `i`
/// whenever a burn is published, even if it reverts.
pub fn burn_tag(view: &BridgeKey, config_id: &Hash, i: u64) -> Hash {
    view.prf(&burn_tag_message(config_id, i))
}

/// Release secret `sOut = HMAC(kAuth, "ferrata/s-out/v1" ‖ configId ‖ r)`.
pub fn burn_secret(auth: &BridgeKey, config_id: &Hash, r: &Hash) -> Hash {
    auth.prf(&burn_secret_message(config_id, r))
}

/// Memo pad: `HMAC(kView, "ferrata/memo/v1" ‖ r)[0..20]`.
pub fn memo_pad(view: &BridgeKey, r: &Hash) -> Address {
    let full = view.prf(&memo_pad_message(r));
    let mut pad = [0u8; 20];
    pad.copy_from_slice(&full[..20]);
    pad
}

/// XORs `bytes` with the memo pad: recipient -> memo and memo -> recipient.
pub fn xor_memo(view: &BridgeKey, r: &Hash, bytes: &Address) -> Address {
    let pad = memo_pad(view, r);
    let mut out = [0u8; 20];
    for (o, (b, p)) in out.iter_mut().zip(bytes.iter().zip(pad.iter())) {
        *o = b ^ p;
    }
    out
}

/// `COut = sha256("ferrata/Cout/v1" ‖ sOut ‖ evmChainId ‖ vault ‖ recipient)`.
pub fn burn_commitment(s_out: &Hash, chain_id: &U256, vault: &Address, recipient: &Address) -> Hash {
    sha256(&burn_commitment_preimage(s_out, chain_id, vault, recipient))
}

/// D12: `mac = HMAC(kView, "ferrata/burn-mac/v2" ‖ r ‖ memo ‖ token ‖ amount ‖ COut)[0..16]`.
pub fn burn_mac(view: &BridgeKey, r: &Hash, memo: &Address, token: &Address, amount: &U256, c_out: &Hash) -> Mac {
    let full = view.prf(&burn_mac_message(r, memo, token, amount, c_out));
    let mut mac = [0u8; 16];
    mac.copy_from_slice(&full[..16]);
    mac
}

/// Burn leaf `sha256(0x02 ‖ BURN_TAG ‖ configId ‖ burnIndex ‖ token ‖ amount ‖ COut ‖ r)`.
pub fn burn_leaf(config_id: &Hash, burn_index: u64, token: &Address, amount: &U256, c_out: &Hash, r: &Hash) -> Hash {
    sha256(&burn_leaf_preimage(config_id, burn_index, token, amount, c_out, r))
}

/// `burnId = sha256("ferrata/burn/v1" ‖ configId ‖ burnIndex ‖ burnLeaf)` (SPEC-v2; not in `NOut`
/// since D13).
pub fn burn_id(config_id: &Hash, burn_index: u64, leaf: &Hash) -> Hash {
    sha256(&burn_id_preimage(config_id, burn_index, leaf))
}

/// D13: `NOut = sha256("ferrata/nf-out/v2" ‖ sOut)`. `sOut` is unique per burn (it is derived from
/// the burn's tag), and the vault keys replay protection on `NOut` alone: a burn that two checkpoints
/// collected at different indices (one of them revoked) is released once.
pub fn burn_nullifier(s_out: &Hash) -> Hash {
    sha256(&burn_nullifier_preimage(s_out))
}

/// Where a burn can be released: bound into `COut` at burn time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReleaseTarget {
    /// The vault's chain id.
    pub chain_id: U256,
    /// The vault.
    pub vault: Address,
    /// The Ethereum recipient.
    pub recipient: Address,
}

/// The burn note published on LEZ (§5.1): `(r, memo, mac, token, amount, COut)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BurnNote {
    /// Burn tag.
    pub r: Hash,
    /// Recipient XOR memo pad.
    pub memo: Address,
    /// MAC over `(r, memo, token, amount, COut)` under `kView` (D12).
    pub mac: Mac,
    /// Ethereum token to release (`0x00…00` = native ETH).
    pub token: Address,
    /// Amount to release.
    pub amount: U256,
    /// Burn commitment.
    pub c_out: Hash,
}

impl BurnNote {
    /// The note for the wallet's `i`-th burn under `config_id`.
    pub fn create(
        keys: &BridgeKeys,
        config_id: &Hash,
        i: u64,
        target: &ReleaseTarget,
        token: Address,
        amount: U256,
    ) -> Self {
        let r = burn_tag(&keys.view, config_id, i);
        let s_out = burn_secret(&keys.auth, config_id, &r);
        let c_out = burn_commitment(&s_out, &target.chain_id, &target.vault, &target.recipient);
        let memo = xor_memo(&keys.view, &r, &target.recipient);
        Self { r, memo, mac: burn_mac(&keys.view, &r, &memo, &token, &amount, &c_out), token, amount, c_out }
    }

    /// Whether this note's MAC verifies under `view` (constant-time comparison).
    pub fn mac_verifies(&self, view: &BridgeKey) -> bool {
        let want = burn_mac(view, &self.r, &self.memo, &self.token, &self.amount, &self.c_out);
        want.iter().zip(self.mac.iter()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
    }
}

/// The release proof's private witness (§5.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseWitness {
    /// The burn's leaf index in the burn tree.
    pub burn_index: u64,
    /// Merkle path of the burn leaf; its direction bits must equal `burn_index`.
    pub path: MerklePath,
    /// The release secret.
    pub s_out: Hash,
    /// The burn tag (bound in the leaf).
    pub r: Hash,
}

/// The release statement's public fields; with the burn root and `NOut`, the journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleasePublic {
    /// The vault's chain id.
    pub chain_id: U256,
    /// The vault.
    pub vault: Address,
    /// The configuration id (the journal's `lezConfigId`).
    pub config_id: Hash,
    /// Token to release.
    pub token: Address,
    /// Amount to release.
    pub amount: U256,
    /// Recipient bound in `COut`.
    pub recipient: Address,
    /// Relayer chosen at proving time (`0x00…00`: whoever submits).
    pub relayer: Address,
    /// Relay fee chosen at proving time.
    pub relay_fee: U256,
}

/// The release statement (§5.2): recomputes `COut` from the witness secret and the public chain,
/// vault and recipient, the burn leaf from the configuration, index, token, amount, `COut` and `r`,
/// checks the path (direction bits = `burnIndex`) to `burn_root`, and returns D13's `NOut`, which
/// does not depend on the index. `relayer` and `relay_fee` are not checked here: the prover chooses
/// them and the journal binds them.
pub fn verify_release(burn_root: &Hash, w: &ReleaseWitness, p: &ReleasePublic) -> Result<Hash, VerifyError> {
    let c_out = burn_commitment(&w.s_out, &p.chain_id, &p.vault, &p.recipient);
    let leaf = burn_leaf(&p.config_id, w.burn_index, &p.token, &p.amount, &c_out, &w.r);
    verify_path(burn_root, &leaf, w.burn_index, &w.path)?;
    Ok(burn_nullifier(&w.s_out))
}

/// The release journal bytes for a verified release (layout: [`ReleaseJournal`]).
pub fn release_journal(burn_root: &Hash, nullifier: &Hash, p: &ReleasePublic) -> [u8; RELEASE_JOURNAL_LEN] {
    ReleaseJournal {
        chain_id: p.chain_id,
        vault: p.vault,
        lez_config_id: p.config_id,
        burn_root: *burn_root,
        nullifier: *nullifier,
        token: p.token,
        amount: p.amount,
        recipient: p.recipient,
        relayer: p.relayer,
        relay_fee: p.relay_fee,
    }
    .encode()
}

/// A burn note together with its position in the burn tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublishedBurn {
    /// Leaf index of the note's burn leaf.
    pub burn_index: u64,
    /// The note.
    pub note: BurnNote,
}

/// A burn found with `kView` alone: enough to disclose it, not to release it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewedBurn {
    /// The burn counter value that produced `r`.
    pub i: u64,
    /// The published burn.
    pub burn: PublishedBurn,
    /// The recipient decrypted from the memo (the MAC covers the memo; `COut` authenticates the
    /// recipient for the owner).
    pub recipient: Address,
}

/// A burn the wallet recognised as its own, with everything needed to release it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedBurn {
    /// The burn counter value that produced `r`.
    pub i: u64,
    /// The published burn.
    pub burn: PublishedBurn,
    /// The recipient, decrypted and authenticated by `COut`.
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
    /// Own burns, by counter and then leaf index.
    pub owned: Vec<OwnedBurn>,
    /// First counter value after the last one seen in use: the next burn's `i`.
    pub next_i: u64,
}

/// `kView`-only scan: regenerates `r_0, r_1, …` until `gap_limit` consecutive values appear in no
/// note, and returns every note with one of those tags whose MAC verifies (notes it did not author,
/// such as a copied tag on someone else's burn, are ignored), with the decrypted recipient. A tag
/// seen in any note counts as used. Returns the burns and the next unused counter.
#[cfg(feature = "std")]
pub fn view_burns(
    view: &BridgeKey,
    config_id: &Hash,
    notes: &[PublishedBurn],
    gap_limit: u64,
) -> (Vec<ViewedBurn>, u64) {
    let mut by_tag: std::collections::HashMap<Hash, Vec<&PublishedBurn>> = std::collections::HashMap::new();
    for burn in notes {
        by_tag.entry(burn.note.r).or_default().push(burn);
    }
    let mut found = Vec::new();
    let mut next_i = 0u64;
    let mut i = 0u64;
    while i - next_i < gap_limit {
        let r = burn_tag(view, config_id, i);
        if let Some(candidates) = by_tag.get(&r) {
            next_i = i + 1;
            for burn in candidates.iter().filter(|b| b.note.mac_verifies(view)) {
                found.push(ViewedBurn { i, burn: **burn, recipient: xor_memo(view, &r, &burn.note.memo) });
            }
        }
        i += 1;
    }
    found.sort_by_key(|v| (v.i, v.burn.burn_index));
    (found, next_i)
}

/// Full wallet recovery of burns: [`view_burns`], then for each burn `sOut` from `kAuth`, a check
/// that `COut` opens to the decrypted recipient at this chain and vault (a note with a tampered memo
/// is dropped), and the leaf and `NOut` a release needs.
#[cfg(feature = "std")]
pub fn recover_burns(
    keys: &BridgeKeys,
    config_id: &Hash,
    chain_id: &U256,
    vault: &Address,
    notes: &[PublishedBurn],
    gap_limit: u64,
) -> BurnScan {
    let (viewed, next_i) = view_burns(&keys.view, config_id, notes, gap_limit);
    let owned = viewed
        .into_iter()
        .filter_map(|v| {
            let note = &v.burn.note;
            let s_out = burn_secret(&keys.auth, config_id, &note.r);
            if burn_commitment(&s_out, chain_id, vault, &v.recipient) != note.c_out {
                return None;
            }
            let leaf = burn_leaf(config_id, v.burn.burn_index, &note.token, &note.amount, &note.c_out, &note.r);
            let nullifier = burn_nullifier(&s_out);
            Some(OwnedBurn { i: v.i, burn: v.burn, recipient: v.recipient, s_out, leaf, nullifier })
        })
        .collect();
    BurnScan { owned, next_i }
}
