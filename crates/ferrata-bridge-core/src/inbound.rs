//! Inbound leg (Ethereum deposit -> LEZ mint).
//!
//! The wallet derives a per-deposit secret `s` from the bridge key `k` and the deposit's public
//! coordinates (vault, depositor, the depositor's deposit counter `j`), deposits with the
//! commitment `C = H(s ‖ dest)`, and later claims by opening `C` under an imported deposit root.
//! The nullifier `N = H(s ‖ fid)` is a function of the secret and of the leaf's identity, not of
//! the root or of who submits the claim, so a public claim and a privately proven claim of the
//! same deposit publish the same `N`, and a deposit cannot be claimed twice under two roots.

use crate::encoding::{
    commitment_preimage, deposit_id_preimage, deposit_leaf_preimage, deposit_secret_message, nullifier_preimage,
};
use crate::tree::{verify_path, Path};
use crate::{sha256, Address, BridgeKey, Hash, VerifyError, U256};

/// `s = HMAC-SHA256(k, "ferrata/s/v1" ‖ chain_id ‖ vault ‖ depositor ‖ j)`, where `j` is the
/// vault's `depositCount[depositor]` at the time of the deposit.
pub fn deposit_secret(key: &BridgeKey, chain_id: u64, vault: &Address, depositor: &Address, j: u64) -> Hash {
    key.prf(&deposit_secret_message(chain_id, vault, depositor, j))
}

/// `C = sha256("ferrata/C/v1" ‖ s ‖ dest)`; `dest` is the LEZ account id that receives the mint.
pub fn commitment(s: &Hash, dest: &Hash) -> Hash {
    sha256(&commitment_preimage(s, dest))
}

/// The vault's leaf for a deposit:
/// `sha256(0x00 ‖ LEAF_TAG ‖ leaf_index ‖ token ‖ amount ‖ C)`.
pub fn deposit_leaf(leaf_index: u64, token: &Address, amount: &U256, commitment: &Hash) -> Hash {
    sha256(&deposit_leaf_preimage(leaf_index, token, amount, commitment))
}

/// `fid = sha256("ferrata/deposit/v1" ‖ chain_id ‖ vault ‖ leaf_index ‖ leaf)`.
pub fn deposit_id(chain_id: u64, vault: &Address, leaf_index: u64, leaf: &Hash) -> Hash {
    sha256(&deposit_id_preimage(chain_id, vault, leaf_index, leaf))
}

/// `N = sha256("ferrata/nf/v1" ‖ s ‖ fid)`.
pub fn nullifier(s: &Hash, fid: &Hash) -> Hash {
    sha256(&nullifier_preimage(s, fid))
}

/// What a claimant knows about one deposit. In a public claim the whole witness is public; in a
/// private claim it stays inside the proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimWitness {
    /// Token deposited (`0x00…00` = native ETH).
    pub token: Address,
    /// Amount deposited.
    pub amount: U256,
    /// The deposit's leaf index in the vault tree.
    pub leaf_index: u64,
    /// Merkle path of the leaf under the claimed root.
    pub path: Path,
    /// The deposit secret.
    pub s: Hash,
    /// The LEZ destination committed to in `C`.
    pub dest: Hash,
}

/// The claim statement: recomputes `C`, the leaf, and the path to `root`, and returns the
/// nullifier `N`. On success the caller mints `amount` of `token` to `dest` and records `N`.
pub fn verify_claim(root: &Hash, w: &ClaimWitness, chain_id: u64, vault: &Address) -> Result<Hash, VerifyError> {
    let c = commitment(&w.s, &w.dest);
    let leaf = deposit_leaf(w.leaf_index, &w.token, &w.amount, &c);
    verify_path(root, &leaf, w.leaf_index, &w.path)?;
    Ok(nullifier(&w.s, &deposit_id(chain_id, vault, w.leaf_index, &leaf)))
}

/// One `Deposit` event of the vault: `(depositor, index j, leafIndex, token, amount, commitment)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultDeposit {
    /// `msg.sender` of the deposit.
    pub depositor: Address,
    /// The depositor's deposit counter `j` (`Deposit.index`).
    pub j: u64,
    /// Position in the deposit tree.
    pub leaf_index: u64,
    /// Token (`0x00…00` = native ETH).
    pub token: Address,
    /// Amount.
    pub amount: U256,
    /// `C`.
    pub commitment: Hash,
}

/// A deposit the key recognised as its own, with everything needed to claim it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedDeposit {
    /// The deposit as published.
    pub deposit: VaultDeposit,
    /// The deposit secret.
    pub s: Hash,
    /// The LEZ destination.
    pub dest: Hash,
    /// The vault leaf.
    pub leaf: Hash,
    /// The nullifier the claim will publish.
    pub nullifier: Hash,
}

/// Wallet recovery: one pass over every deposit of the vault, keeping those whose commitment
/// opens with `s` derived from `key` and the destination `dest_for(depositor, j)`. Costs one
/// HMAC, one SHA-256 and one `dest_for` call per deposit.
#[cfg(feature = "std")]
pub fn recover(
    key: &BridgeKey,
    chain_id: u64,
    vault: &Address,
    deposits: &[VaultDeposit],
    dest_for: impl Fn(&Address, u64) -> Hash,
) -> Vec<OwnedDeposit> {
    deposits
        .iter()
        .filter_map(|d| {
            let s = deposit_secret(key, chain_id, vault, &d.depositor, d.j);
            let dest = dest_for(&d.depositor, d.j);
            if commitment(&s, &dest) != d.commitment {
                return None;
            }
            let leaf = deposit_leaf(d.leaf_index, &d.token, &d.amount, &d.commitment);
            let nullifier = nullifier(&s, &deposit_id(chain_id, vault, d.leaf_index, &leaf));
            Some(OwnedDeposit { deposit: d.clone(), s, dest, leaf, nullifier })
        })
        .collect()
}
