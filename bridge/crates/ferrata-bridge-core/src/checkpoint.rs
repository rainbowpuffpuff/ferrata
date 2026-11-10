//! LEZ -> Ethereum checkpoint encodings (SPEC-v2 §4.1, §4.2): the state hash `S`, `keySetHash`,
//! `burnFrontierHash`, the signing threshold `t(n)`, and the journal ([`CheckpointJournal`]). Only
//! the encodings: the checkpoint program itself is built elsewhere.

use crate::encoding::{burn_frontier_preimage, lez_state_preimage};
pub use crate::encoding::{key_set_hash, CheckpointJournal, LezState};
use crate::tree::IncrementalTree;
use crate::{sha256, Hash};

impl LezState {
    /// `S = sha256(lez_state_preimage(self))`.
    pub fn hash(&self) -> Hash {
        sha256(&lez_state_preimage(self))
    }
}

/// `burnFrontierHash` of a burn tree: SHA-256 of its canonical frontier (layout and the choice of
/// canonical form: [`crate::encoding::burn_frontier_preimage`]).
pub fn burn_frontier_hash(tree: &IncrementalTree) -> Hash {
    sha256(&burn_frontier_preimage(&tree.frontier()))
}

/// LEZ's signing threshold for `n` accredited keys: `t(n) = max(1, min(⌈2n/3⌉, n − 1))` (§4.2).
pub fn threshold(n: u32) -> u32 {
    let two_thirds_up = (2 * u64::from(n)).div_ceil(3) as u32;
    two_thirds_up.min(n.saturating_sub(1)).max(1)
}
