//! The release rule, and the release guest run through r0vm (feature `prove`).

use ferrata_bridge_core::Hash;

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
