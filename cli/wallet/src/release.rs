//! The release: the rule that picks the burn root, the witness and journal under it, and the release
//! guest run through r0vm (feature `prove`).

use ferrata_bridge_core::{
    encoding::ReleaseInput,
    outbound::{burn_leaf, release_journal, verify_release, OwnedBurn, PublishedBurn, ReleasePublic, ReleaseWitness},
    tree::Tree,
    Address, Hash, U256,
};
use ferrata_lez_bridge_wire::word_of_u128;

use crate::Refusal;

/// A burn root the vault accepts (finalised, not revoked), with what the release rule needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcceptedRoot {
    pub root: Hash,
    /// Burns it covers (burn indices below it).
    pub count: u64,
    /// Its checkpoint index.
    pub index: u64,
    /// When its checkpoint was finalised (Ethereum time, seconds).
    pub finalized_at: u64,
}

/// Why no root is picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootRefusal {
    /// No accepted root covers the burn (the vault's `BurnRootNotAccepted`).
    NotAccepted,
    /// Every root that covers it is still in its release delay; the first opens at `first_open`
    /// (the vault's `ReleaseDelayNotElapsed`).
    DelayNotElapsed { first_open: u64 },
}

impl RootRefusal {
    /// The vault's error name.
    pub fn name(&self) -> &'static str {
        match self {
            RootRefusal::NotAccepted => "BurnRootNotAccepted",
            RootRefusal::DelayNotElapsed { .. } => "ReleaseDelayNotElapsed",
        }
    }
}

/// The roots the release rule looks at for `burn_index`, in the order it looks at them: those that
/// cover the burn, newest first (`roots` is oldest first, as the vault's events give them).
pub fn covering(roots: &[AcceptedRoot], burn_index: u64) -> impl Iterator<Item = &AcceptedRoot> {
    roots.iter().rev().filter(move |r| r.count > burn_index)
}

/// The wallet's release rule: among the accepted burn roots that cover a burn, the newest whose
/// release delay has passed at `now` (the vault opens a root at `finalizedAt + delay` and accepts a
/// release strictly after). The caller offers the roots [`covering`] gives, in that order, each with
/// its delay (the vault's `releaseDelayFor(token, checkpointIndex)`), until one is taken.
#[derive(Clone, Copy, Debug)]
pub struct RootRule {
    now: u64,
    first_open: Option<u64>,
}

impl RootRule {
    pub fn new(now: u64) -> Self {
        RootRule { now, first_open: None }
    }

    /// When `root` opens.
    pub fn opens(root: &AcceptedRoot, delay: u64) -> u64 {
        root.finalized_at.saturating_add(delay)
    }

    /// Offers the next root with its delay: `true` if the rule takes it.
    pub fn offer(&mut self, root: &AcceptedRoot, delay: u64) -> bool {
        let opens = Self::opens(root, delay);
        if self.now > opens {
            return true;
        }
        self.first_open = Some(self.first_open.map_or(opens, |o| o.min(opens)));
        false
    }

    /// The refusal once every covering root has been offered and none taken.
    pub fn refusal(&self) -> RootRefusal {
        match self.first_open {
            Some(first_open) => RootRefusal::DelayNotElapsed { first_open },
            None => RootRefusal::NotAccepted,
        }
    }
}

/// The rule over roots whose delays are all known: `roots` oldest first, each with its delay.
pub fn pick_root(roots: &[(AcceptedRoot, u64)], burn_index: u64, now: u64) -> Result<(AcceptedRoot, u64), RootRefusal> {
    let mut rule = RootRule::new(now);
    for (root, delay) in roots.iter().rev().filter(|(r, _)| r.count > burn_index) {
        if rule.offer(root, *delay) {
            return Ok((*root, *delay));
        }
    }
    Err(rule.refusal())
}

/// A release, built and checked natively: the release guest's input and the journal the vault
/// rebuilds from its call.
#[derive(Clone, Debug)]
pub struct ReleasePlan {
    pub input: ReleaseInput,
    pub journal: Vec<u8>,
    pub nullifier: Hash,
}

/// The release of `owned` under the accepted burn root `burn_root` of the first `burn_count` burns
/// of `burns` (the configuration's published burns on the zone, in order of application): the burn
/// tree rebuilt and checked against the vault's root, the path, the public inputs, the release
/// statement (`verify_release`), the journal.
#[allow(clippy::too_many_arguments)]
pub fn plan(
    cid: &Hash,
    chain_id: &U256,
    vault: &Address,
    burns: &[PublishedBurn],
    owned: &OwnedBurn,
    burn_root: Hash,
    burn_count: u64,
    relayer: Address,
    relay_fee: u128,
) -> Result<ReleasePlan, Refusal> {
    if owned.burn.burn_index >= burn_count {
        return Err(Refusal::wallet(format!(
            "burn {} is not under the accepted burn root ({burn_count} burns): wait for the next checkpoint",
            owned.burn.burn_index
        )));
    }
    if burn_count as usize > burns.len() {
        return Err(Refusal::wallet(format!("the zone shows {} burns, the burn root {burn_count}", burns.len())));
    }
    let leaves: Vec<Hash> = burns[..burn_count as usize]
        .iter()
        .map(|b| burn_leaf(cid, b.burn_index, &b.note.token, &b.note.amount, &b.note.c_out, &b.note.r))
        .collect();
    let tree = Tree::from_leaves(leaves).map_err(|_| Refusal::wallet("too many burns"))?;
    if tree.root() != burn_root {
        return Err(Refusal::wallet(format!(
            "the zone's burns do not give the vault's accepted burn root at {burn_count} burns: the zone or the RPC serves another chain"
        )));
    }
    let path = tree
        .path(owned.burn.burn_index)
        .ok_or_else(|| Refusal::wallet(format!("burn {} is not in the tree", owned.burn.burn_index)))?;
    let witness = ReleaseWitness { burn_index: owned.burn.burn_index, path, s_out: owned.s_out, r: owned.burn.note.r };
    let public = ReleasePublic {
        chain_id: *chain_id,
        vault: *vault,
        config_id: *cid,
        token: owned.burn.note.token,
        amount: owned.burn.note.amount,
        recipient: owned.recipient,
        relayer,
        relay_fee: word_of_u128(relay_fee),
    };
    let nullifier = verify_release(&burn_root, &witness, &public)
        .map_err(|e| Refusal::wallet(format!("the release statement does not hold: {e:?}")))?;
    if nullifier != owned.nullifier {
        return Err(Refusal::wallet("the release nullifier is not the recovered one"));
    }
    let journal = release_journal(&burn_root, &nullifier, &public).to_vec();
    Ok(ReleasePlan { input: ReleaseInput { burn_root, witness, public }, journal, nullifier })
}

/// What r0vm made of the release guest.
#[cfg(feature = "prove")]
pub struct Proven {
    pub image_id: [u8; 32],
    /// The receipt (JSON), succinct; a real one only without `RISC0_DEV_MODE`.
    pub receipt: Vec<u8>,
    pub dev_mode: bool,
}

/// Runs the release guest `elf` on `input` (the encoded `ReleaseInput`) through r0vm
/// (`RISC0_DEV_MODE=1`: executed only, a fake receipt; unset: a succinct proof on this machine's CPU,
/// verified against the image id), and checks that its journal is `journal`, the native one. The
/// vault takes a Groth16 seal: a relayer wraps the receipt.
#[cfg(feature = "prove")]
pub fn prove(elf: &[u8], input: &[u8], journal: &[u8]) -> Result<Proven, String> {
    use risc0_zkvm::{compute_image_id, default_prover, ExecutorEnv, ProverOpts};
    let image_id = compute_image_id(elf).map_err(|e| format!("not a guest program: {e}"))?;
    let env = ExecutorEnv::builder().write_slice(input).build().map_err(|e| format!("{e}"))?;
    let info = default_prover().prove_with_opts(env, elf, &ProverOpts::succinct()).map_err(|e| format!("r0vm: {e}"))?;
    if info.receipt.journal.bytes != journal {
        return Err("the guest's journal is not the native release journal".into());
    }
    let dev_mode = std::env::var("RISC0_DEV_MODE").map(|v| v == "1" || v == "true").unwrap_or(false);
    if !dev_mode {
        info.receipt.verify(image_id).map_err(|e| format!("the receipt does not verify: {e}"))?;
    }
    let receipt = serde_json::to_vec(&info.receipt).map_err(|e| format!("{e}"))?;
    let mut id = [0u8; 32];
    id.copy_from_slice(image_id.as_bytes());
    Ok(Proven { image_id: id, receipt, dev_mode })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(count: u64, index: u64, finalized_at: u64) -> AcceptedRoot {
        AcceptedRoot { root: [index as u8; 32], count, index, finalized_at }
    }

    #[test]
    fn newest_open_root_wins() {
        // Roots of 4, 8, 12 burns; the newest still in its delay at t = 1000.
        let roots = [(root(4, 0, 100), 120), (root(8, 1, 500), 120), (root(12, 2, 950), 120)];
        assert_eq!(pick_root(&roots, 3, 1000), Ok((roots[1].0, 120)));
        assert_eq!(pick_root(&roots, 5, 1000), Ok((roots[1].0, 120)));
        // Burn 9 is under the newest root only, which opens at 1070.
        assert_eq!(pick_root(&roots, 9, 1000), Err(RootRefusal::DelayNotElapsed { first_open: 1070 }));
        assert_eq!(pick_root(&roots, 9, 1070), Err(RootRefusal::DelayNotElapsed { first_open: 1070 }));
        assert_eq!(pick_root(&roots, 9, 1071), Ok((roots[2].0, 120)));
        // No root covers burn 12.
        assert_eq!(pick_root(&roots, 12, 5000), Err(RootRefusal::NotAccepted));
        assert_eq!(RootRefusal::NotAccepted.name(), "BurnRootNotAccepted");
    }

    #[test]
    fn per_root_delays() {
        // A newer root with a longer delay (partial participation) opens after an older one.
        let roots = [(root(4, 0, 100), 3600), (root(8, 1, 200), 10_800)];
        assert_eq!(pick_root(&roots, 1, 3701), Ok((roots[0].0, 3600)));
        assert_eq!(pick_root(&roots, 1, 3700), Err(RootRefusal::DelayNotElapsed { first_open: 3700 }));
        assert_eq!(pick_root(&roots, 1, 11_001), Ok((roots[1].0, 10_800)));
        let order: Vec<u64> = covering(&roots.map(|(r, _)| r), 1).map(|r| r.index).collect();
        assert_eq!(order, vec![1, 0]);
    }
}
