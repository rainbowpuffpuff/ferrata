//! Deposits, their tree, claim witnesses and burn notes, made with the bridge core as a wallet
//! makes them (`cli/`).

use ferrata_bridge_core::{
    chain_id_u256,
    inbound::{commitment, deposit_id, deposit_leaf, deposit_secret, nullifier as core_nullifier, DestKind},
    outbound::{BurnNote, ReleaseTarget},
    sha256,
    tree::Tree,
    BridgeKeys, Hash,
};
use ferrata_lez03_bridge_core::{word_of_u128, ClaimWitness, Note};

/// A wallet's bridge keys from a test seed.
pub fn keys(seed: &str) -> BridgeKeys {
    BridgeKeys::from_bridge_seed(&sha256(seed.as_bytes()))
}

/// One deposit whose opening is known.
#[derive(Clone, Debug)]
pub struct Deposit {
    pub leaf_index: u64,
    pub depositor: [u8; 20],
    pub token: [u8; 20],
    pub amount: [u8; 32],
    pub s: Hash,
    pub kind: DestKind,
    pub dest: Hash,
}

impl Deposit {
    /// The `j`-th deposit of `depositor` into `vault` on `chain`, with the wallet's secret
    /// `s = HMAC(kAuth, "ferrata/s/v1" ‖ chain ‖ vault ‖ depositor ‖ j)` (the cli's derivation).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        keys: &BridgeKeys,
        chain: u64,
        vault: &[u8; 20],
        depositor: [u8; 20],
        j: u64,
        leaf_index: u64,
        token: [u8; 20],
        amount: u128,
        kind: DestKind,
        dest: Hash,
    ) -> Self {
        let s = deposit_secret(&keys.auth, &chain_id_u256(chain), vault, &depositor, j);
        Self { leaf_index, depositor, token, amount: word_of_u128(amount), s, kind, dest }
    }

    /// `C`, what the depositor sends the vault.
    pub fn commitment(&self) -> Hash {
        commitment(&self.s, self.kind, &self.dest)
    }

    pub fn leaf(&self) -> Hash {
        deposit_leaf(self.leaf_index, &self.depositor, &self.token, &self.amount, &self.commitment())
    }

    pub fn nullifier(&self, chain: u64, vault: &[u8; 20]) -> Hash {
        core_nullifier(&self.s, &deposit_id(&chain_id_u256(chain), vault, self.leaf_index, &self.leaf()))
    }

    /// The claim witness under the root of the first `n` of `leaves`.
    pub fn witness(&self, leaves: &[Hash], n: u64) -> ClaimWitness {
        let path = Tree::from_leaves(leaves[..n as usize].to_vec()).unwrap().path(self.leaf_index).expect("in the tree");
        ClaimWitness {
            depositor: self.depositor,
            token: self.token,
            amount: self.amount,
            leaf_index: self.leaf_index,
            path: path.siblings.to_vec(),
            s: self.s,
            kind: self.kind as u8,
            dest: self.dest,
        }
    }
}

/// The root of the first `n` of `leaves`.
pub fn root(leaves: &[Hash], n: u64) -> Hash {
    Tree::from_leaves(leaves[..n as usize].to_vec()).unwrap().root()
}

/// `N` of the deposit a witness opens (whether or not the witness is valid).
pub fn nullifier_of(w: &ClaimWitness, chain: u64, vault: &[u8; 20]) -> Hash {
    let kind = DestKind::from_byte(w.kind).unwrap_or(DestKind::Public);
    let leaf = deposit_leaf(w.leaf_index, &w.depositor, &w.token, &w.amount, &commitment(&w.s, kind, &w.dest));
    core_nullifier(&w.s, &deposit_id(&chain_id_u256(chain), vault, w.leaf_index, &leaf))
}

/// The wallet's `i`-th burn note under `config_id`, releasable to `recipient` from `vault`.
pub fn note(
    keys: &BridgeKeys,
    config_id: &Hash,
    i: u64,
    chain: u64,
    vault: [u8; 20],
    recipient: [u8; 20],
    token: [u8; 20],
    amount: u128,
) -> Note {
    let target = ReleaseTarget { chain_id: chain_id_u256(chain), vault, recipient };
    let n = BurnNote::create(keys, config_id, i, &target, token, word_of_u128(amount));
    Note { r: n.r, memo: n.memo, mac: n.mac, token: n.token, amount: n.amount, c_out: n.c_out }
}
