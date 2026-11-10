//! Inbound leg (Ethereum deposit -> LEZ mint), SPEC-v2 §3.
//!
//! The wallet derives a per-deposit secret `s` from `kAuth` and the deposit's public coordinates
//! (chain, vault, depositor, the depositor's counter `j`), deposits with `C = H(s ‖ kind ‖ dest)`,
//! and later claims by opening `C` under an imported deposit root. The nullifier
//! `N = H(s ‖ fid)` depends on the secret and on `fid`, the identifier of "leaf `leafIndex` of the
//! accumulator at slot 0 of `vault`", never on the root: a deposit has one nullifier under every
//! later root, so it cannot be claimed once per root.

use crate::encoding::{
    commitment_preimage, deposit_id_preimage, deposit_leaf_preimage, deposit_secret_message, nullifier_preimage,
};
use crate::tree::{verify_path, MerklePath};
use crate::{sha256, Address, BridgeKey, Hash, VerifyError, WriteOnceSet, U256};

/// The destination kind committed in `C` (§3.3). A private-kind claim must execute as the
/// destination account itself; the kind byte stops a commitment made for one kind from being
/// opened as the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum DestKind {
    /// A private LEZ account (`0x01`).
    Private = 0x01,
    /// A public LEZ account (`0x02`).
    Public = 0x02,
}

impl DestKind {
    /// Both kinds.
    pub const ALL: [DestKind; 2] = [DestKind::Private, DestKind::Public];

    /// The kind with this byte, if any.
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(DestKind::Private),
            0x02 => Some(DestKind::Public),
            _ => None,
        }
    }
}

/// `s = HMAC(kAuth, "ferrata/s/v1" ‖ evmChainId ‖ vault ‖ depositor ‖ j)`, `j` being the vault's
/// `depositCount[depositor]` at deposit time.
pub fn deposit_secret(auth: &BridgeKey, chain_id: &U256, vault: &Address, depositor: &Address, j: u64) -> Hash {
    auth.prf(&deposit_secret_message(chain_id, vault, depositor, j))
}

/// `C = sha256("ferrata/C/v1" ‖ s ‖ kind ‖ dest)`.
pub fn commitment(s: &Hash, kind: DestKind, dest: &Hash) -> Hash {
    sha256(&commitment_preimage(s, kind, dest))
}

/// The vault's leaf: `sha256(0x00 ‖ LEAF_TAG ‖ leafIndex ‖ depositor ‖ token ‖ amount ‖ C)`.
pub fn deposit_leaf(leaf_index: u64, depositor: &Address, token: &Address, amount: &U256, c: &Hash) -> Hash {
    sha256(&deposit_leaf_preimage(leaf_index, depositor, token, amount, c))
}

/// `fid = sha256("ferrata/fact/acc-leaf/v1" ‖ evmChainId ‖ vault ‖ slot 0 ‖ leafIndex ‖ leaf)`.
pub fn deposit_id(chain_id: &U256, vault: &Address, leaf_index: u64, leaf: &Hash) -> Hash {
    sha256(&deposit_id_preimage(chain_id, vault, leaf_index, leaf))
}

/// `N = sha256("ferrata/nf/v1" ‖ s ‖ fid)`.
pub fn nullifier(s: &Hash, fid: &Hash) -> Hash {
    sha256(&nullifier_preimage(s, fid))
}

/// Everything a claimant knows about one deposit. In a plaintext public claim the whole witness is
/// public; in a privacy-preserving claim it stays inside the proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimWitness {
    /// `msg.sender` of the deposit (bound in the leaf).
    pub depositor: Address,
    /// Token deposited (`0x00…00` = native ETH).
    pub token: Address,
    /// Amount deposited.
    pub amount: U256,
    /// The deposit's leaf index.
    pub leaf_index: u64,
    /// Merkle path of the leaf under the claimed root; its direction bits must equal `leaf_index`.
    pub path: MerklePath,
    /// The deposit secret.
    pub s: Hash,
    /// Destination kind.
    pub kind: DestKind,
    /// The LEZ destination account id.
    pub dest: Hash,
}

/// The claim statement (§3.3): recomputes `C` and the leaf, checks the path to `root` with direction
/// bits equal to `leafIndex`, and returns `N`. The caller then publishes `N`, `token` and `amount`
/// and mints to `dest`. `chain_id` and `vault` enter only `N`: they must come from the program's
/// configuration (the vault whose root was imported), never from the claimant.
pub fn verify_claim(root: &Hash, w: &ClaimWitness, chain_id: &U256, vault: &Address) -> Result<Hash, VerifyError> {
    let c = commitment(&w.s, w.kind, &w.dest);
    let leaf = deposit_leaf(w.leaf_index, &w.depositor, &w.token, &w.amount, &c);
    verify_path(root, &leaf, w.leaf_index, &w.path)?;
    Ok(nullifier(&w.s, &deposit_id(chain_id, vault, w.leaf_index, &leaf)))
}

/// Key of an inbound nullifier account (§1): one namespace for the whole bridge program, keyed by
/// `(evmChainId, vault, N)`, so two configurations cannot both mint against one vault deposit.
pub type NullifierKey = (U256, Address, Hash);

/// A claim whose nullifier is already recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlreadyClaimed;

/// What the LEZ bridge program does after [`verify_claim`]: record `(evmChainId, vault, N)` in its
/// write-once nullifier set, refusing a second claim of the same deposit.
pub fn register_claim(
    nullifiers: &mut impl WriteOnceSet<NullifierKey>,
    chain_id: &U256,
    vault: &Address,
    n: &Hash,
) -> Result<(), AlreadyClaimed> {
    if nullifiers.insert_new((*chain_id, *vault, *n)) {
        Ok(())
    } else {
        Err(AlreadyClaimed)
    }
}

/// One `Deposit` event of the vault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultDeposit {
    /// `msg.sender` of the deposit.
    pub depositor: Address,
    /// The depositor's counter `j`.
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
    /// Destination kind.
    pub kind: DestKind,
    /// The wallet's account index of the destination.
    pub account_index: u32,
    /// The destination account id.
    pub dest: Hash,
    /// The vault leaf.
    pub leaf: Hash,
    /// The nullifier a claim will publish.
    pub nullifier: Hash,
}

/// Wallet recovery (§3.3): one pass over every deposit of the vault. For each deposit,
/// `s` is derived from `kAuth` and the deposit's `(depositor, j)`, and `C` is recomputed for the
/// `2·gap` candidate destinations `dest_of(kind, a)`, `a = 0..gap`, both kinds (computed once, up
/// front). Destinations outside that window, or not derived from the seed, are not found.
/// Cost per deposit: one HMAC and up to `2·gap` SHA-256 of 77 bytes.
#[cfg(feature = "std")]
pub fn recover(
    auth: &BridgeKey,
    chain_id: &U256,
    vault: &Address,
    deposits: &[VaultDeposit],
    gap: u32,
    dest_of: impl Fn(DestKind, u32) -> Hash,
) -> Vec<OwnedDeposit> {
    let candidates: Vec<(DestKind, u32, Hash)> =
        (0..gap).flat_map(|a| DestKind::ALL.map(|kind| (kind, a, dest_of(kind, a)))).collect();
    let mut owned = Vec::new();
    for d in deposits {
        let s = deposit_secret(auth, chain_id, vault, &d.depositor, d.j);
        if let Some((kind, account_index, dest)) =
            candidates.iter().find(|(kind, _, dest)| commitment(&s, *kind, dest) == d.commitment)
        {
            let leaf = deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment);
            let nullifier = nullifier(&s, &deposit_id(chain_id, vault, d.leaf_index, &leaf));
            owned.push(OwnedDeposit {
                deposit: d.clone(),
                s,
                kind: *kind,
                account_index: *account_index,
                dest: *dest,
                leaf,
                nullifier,
            });
        }
    }
    owned
}
