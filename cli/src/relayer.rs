//! The relayer (bridge.md §6, §12.3, §14; proposal B2 and B3), prototype, unaudited.
//!
//! **What a relayer holds.** Its own keys: an Ethereum account that pays gas and is paid relay and
//! checkpoint fees, and a LEZ public account that funds gas drops. Everything else it is handed is
//! public, or becomes public in the transaction it sends:
//! - a **release job** from a user who opts in: the release journal and its Groth16 seal, which the
//!   vault call publishes anyway. Never `sOut`, `r`, the burn index, the seed or a key: the holder
//!   proves; the relayer (or any wrapper) only wraps and submits;
//! - a **checkpoint job** from the checkpoint prover: the checkpoint journal and its seal;
//! - a **gas-drop request**: a receipt whose journal is `(root, chain, vault, N, oneTimeAddress,
//!   relayer)` ([`ferrata_bridge_core::gasdrop`]). It learns `N` (the claim publishes it), a time and
//!   the one-time address; never which deposit.
//!
//! **Root imports** ([`plan_root_import`]) need nothing handed over: the relayer reads the vault's
//! deposit root and count (slots 0 and 1) with `eth_getProof` at the block of the newest record of the
//! configuration's light-client instance on LEZ, and sends the bridge program's `ImportRoot`, built and
//! dry-run by `ferrata_wallet::import` (the code `ferrata_bridge_module`'s `import_root` runs).
//!
//! **What it refuses before spending gas.** A job for another vault, chain or configuration; a release
//! naming another relayer (only that relayer can be paid for it, and the proof cannot be changed); an
//! unpaid release unless the operator accepts them; a used nullifier; a checkpoint built on a state
//! that is not the vault's final state, or already a candidate; a gas-drop receipt that does not
//! verify, is not a gas-drop journal, names another relayer or configuration, is under a root LEZ has
//! not imported, or whose `N` is claimed or already had its drop. Everything the vault would refuse
//! (fees, delays, caps, freezes, the proof) is caught by an `eth_call` before signing; whatever a
//! relayer sends anyway, the vault refuses (`tests/relayer_fork.sh` sends tampered calls).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use alloy::primitives::{Address, Bytes, B256, U256};
use alloy::providers::{DynProvider, Provider};
use alloy::rpc::types::{BlockId, Filter, TransactionReceipt};
use alloy::sol;
use alloy::sol_types::{SolCall, SolEvent as _};
use anyhow::{anyhow, bail, ensure, Context as _, Result};
use ferrata_bridge_core::encoding::ReleaseJournal;
use ferrata_bridge_core::gasdrop::GasDropJournal;
use ferrata_bridge_core::inbound::{deposit_leaf, VaultDeposit};
use ferrata_bridge_core::tree::IncrementalTree;
use ferrata_bridge_core::{sha256, Hash};
use serde::{Deserialize, Serialize};

use crate::eth::{self, IVault};
use crate::hx;

sol! {
    /// The vault's checkpoint side, as the relayer calls it (vault/src/FerrataVault.sol).
    #[allow(missing_docs)]
    interface IRelayVault {
        #[derive(Debug, PartialEq, Eq)]
        struct Checkpoint {
            uint32 nMin; uint32 tMin; bytes32 parentState; bytes32 newState; bytes32 burnRoot; uint64 burnCount;
            uint64 importedLeafCount; bool sunset; bytes32 echoNonce; uint64 tipId; bytes32 tipHash; uint32 n; uint32 t;
        }
        event CandidateSubmitted(bytes32 indexed newState, bytes32 indexed parentState, bytes32 burnRoot, uint64 burnCount, uint64 importedLeafCount, bool sunset, uint256 deltaAtSubmit, uint64 tipId, bytes32 tipHash, uint32 n, uint32 t);
        function submitCandidate(Checkpoint c, bytes seal) external;
        function finalizeAttested(Checkpoint c) external;
        function finalState() external view returns (bytes32);
        function candidates(bytes32 newState) external view returns (bytes32 journalDigest, bytes32 tipHash, uint64 tipId, uint32 t, uint48 submittedAt, uint32 deltaAtSubmit, uint64 epoch, address prover);
        function nMin() external view returns (uint32);
        function tMin() external view returns (uint32);
        function mode() external view returns (uint8);
        function owed(address payee, address token) external view returns (uint256);
        function claimOwed(address token) external;
        function releaseImageId() external view returns (bytes32);
    }
}

/// `sha256("ferrata/vault/checkpoint/v3")`: the first word of the vault's 17-word checkpoint journal.
pub fn checkpoint_domain() -> Hash {
    sha256(b"ferrata/vault/checkpoint/v3")
}

fn word_u64(w: &[u8]) -> Result<u64> {
    ensure!(w[..24].iter().all(|b| *b == 0), "a journal word is not a u64");
    Ok(u64::from_be_bytes(w[24..32].try_into().expect("8 bytes")))
}

/// The checkpoint journal the vault verifies a candidate's proof against (17 words,
/// `FerrataVault._checkpointDigest`).
pub fn checkpoint_journal(chain_id: u64, vault: Address, cid: B256, c: &IRelayVault::Checkpoint) -> Vec<u8> {
    let word = |n: u64| {
        let mut w = [0u8; 32];
        w[24..].copy_from_slice(&n.to_be_bytes());
        w
    };
    let mut j = Vec::with_capacity(17 * 32);
    j.extend_from_slice(&checkpoint_domain());
    j.extend_from_slice(&word(chain_id));
    j.extend_from_slice(&[[0u8; 12].as_slice(), vault.as_slice()].concat());
    j.extend_from_slice(cid.as_slice());
    j.extend_from_slice(&word(u64::from(c.nMin)));
    j.extend_from_slice(&word(u64::from(c.tMin)));
    for h in [c.parentState, c.newState, c.burnRoot] {
        j.extend_from_slice(h.as_slice());
    }
    j.extend_from_slice(&word(c.burnCount));
    j.extend_from_slice(&word(c.importedLeafCount));
    j.extend_from_slice(&word(u64::from(c.sunset)));
    j.extend_from_slice(c.echoNonce.as_slice());
    j.extend_from_slice(&word(c.tipId));
    j.extend_from_slice(c.tipHash.as_slice());
    j.extend_from_slice(&word(u64::from(c.n)));
    j.extend_from_slice(&word(u64::from(c.t)));
    j
}

/// A checkpoint journal: its chain, vault, configuration and the checkpoint it carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointJob {
    pub chain_id: u64,
    pub vault: Address,
    pub cid: B256,
    pub checkpoint: IRelayVault::Checkpoint,
}

/// Parses a 17-word checkpoint journal (the checkpoint prover's output, `journal.hex`).
pub fn checkpoint_from_journal(j: &[u8]) -> Result<CheckpointJob> {
    ensure!(j.len() == 17 * 32, "a checkpoint journal has 17 words, this one {} bytes", j.len());
    let w = |i: usize| &j[i * 32..(i + 1) * 32];
    ensure!(w(0) == checkpoint_domain(), "not a checkpoint journal (domain)");
    ensure!(w(2)[..12].iter().all(|b| *b == 0), "the vault word is not an address");
    let h = |i: usize| B256::from_slice(w(i));
    let u32w = |i: usize| -> Result<u32> { Ok(u32::try_from(word_u64(w(i))?)?) };
    let sunset = word_u64(w(11))?;
    ensure!(sunset <= 1, "sunset is not a bool");
    Ok(CheckpointJob {
        chain_id: word_u64(w(1))?,
        vault: Address::from_slice(&w(2)[12..]),
        cid: h(3),
        checkpoint: IRelayVault::Checkpoint {
            nMin: u32w(4)?,
            tMin: u32w(5)?,
            parentState: h(6),
            newState: h(7),
            burnRoot: h(8),
            burnCount: word_u64(w(9))?,
            importedLeafCount: word_u64(w(10))?,
            sunset: sunset == 1,
            echoNonce: h(12),
            tipId: word_u64(w(13))?,
            tipHash: h(14),
            n: u32w(15)?,
            t: u32w(16)?,
        },
    })
}

/// Blocks per `eth_getLogs` request.
pub const LOG_CHUNK: u64 = 10_000;

/// Vault errors after which a job may succeed later (a root not final yet, a delay, a freeze, a cap), and
/// transport failures: the daemon keeps such a job in its inbox and retries it next round.
pub const TEMPORARY: [&str; 11] = [
    "BurnRootNotAccepted",
    "ReleaseDelayNotElapsed",
    "ReleasesAreFrozen",
    "TokenReleasesAreFrozen",
    "TokenReleaseCapExceeded",
    "GlobalReleaseCapExceeded",
    "CheckpointsAreFrozen",
    "error sending request",
    "timed out",
    "connection",
    // A transaction of the relayer's that may still be mined (relayerd's in-flight record).
    "in flight",
];

/// What makes a failure final for the job: a check of the relayer's or the vault's refusing it (anything else, a
/// rate limit, a 5xx, a closed connection, a node behind, a body that is not JSON, is the endpoint's and is retried).
pub const FINAL: [&str; 7] =
    ["refused", "skipped:", "reverted", "not a relayer job", "not a release journal", "not a receipt", "would revert"];

/// Whether a job that failed with `reason` should be retried rather than refused for good: a temporary vault error
/// ([`TEMPORARY`]), or any failure that is not a refusal ([`FINAL`]). A job kept by mistake is retried until
/// `--max-job-age`; a job refused by mistake would be lost, so the endpoint's failures never refuse one (R.06).
pub fn temporary(reason: &str) -> bool {
    TEMPORARY.iter().any(|t| reason.contains(t)) || !FINAL.iter().any(|t| reason.contains(t))
}

/// The relayer's own identity and its terms.
#[derive(Clone, Debug)]
pub struct Relayer {
    /// The vault it serves.
    pub vault: Address,
    /// Its Ethereum account (sender of every call; payee of the relay and checkpoint fees).
    pub me: Address,
    /// Submits releases whose proof pays no relay fee (off by default: a relayer is paid by the proof).
    pub accept_unpaid: bool,
}

/// What the vault's configuration says about a job, read once per round.
#[derive(Clone, Debug)]
pub struct VaultView {
    pub chain_id: u64,
    pub cid: B256,
    pub final_state: B256,
    pub now: u64,
    /// The block every read of the round is made at (`None`: the node's latest). The daemon pins a round to one
    /// block at or above its own last mined transaction's, so a node that answers from an older state now and
    /// then (a load balancer's lagging backend) cannot make it plan from before its own effects (R.06).
    pub block: Option<u64>,
}

impl Relayer {
    /// Reads the vault's chain, configuration, final state and the latest block's time.
    pub async fn view(&self, p: &DynProvider) -> Result<VaultView> {
        self.view_at(p, None).await
    }

    /// The same at block `at` (`None`: latest); the block must exist on the node.
    pub async fn view_at(&self, p: &DynProvider, at: Option<u64>) -> Result<VaultView> {
        let chain_id = eth::retry_rpc(|| p.get_chain_id()).await?;
        let cid = eth::view(p, self.vault, IVault::lezConfigIdCall {}, at).await?;
        let final_state = eth::view(p, self.vault, IRelayVault::finalStateCall {}, at).await?;
        let id = at.map_or(BlockId::latest(), BlockId::number);
        let now = eth::retry_rpc(|| p.get_block(id))
            .await?
            .map(|b| b.header.timestamp)
            .ok_or_else(|| anyhow!("the node has no block {at:?} yet (it is behind)"))?;
        Ok(VaultView { chain_id, cid, final_state, now, block: at })
    }

    /// Checks a release job (journal and seal from a holder who opts in) and builds the vault call.
    /// Refuses before sending what does not concern or does not pay this relayer; the vault's own
    /// checks run as an `eth_call` in [`Relayer::submit`].
    pub async fn plan_release(
        &self,
        p: &DynProvider,
        v: &VaultView,
        journal: &[u8],
        seal: Bytes,
    ) -> Result<(IVault::ReleaseRequest, Bytes)> {
        let j = ReleaseJournal::decode(journal).ok_or_else(|| anyhow!("not a release journal"))?;
        let vault = Address::from(j.vault);
        ensure!(vault == self.vault, "refused: the proof is for vault {vault}, this relayer serves {}", self.vault);
        ensure!(U256::from_be_bytes(j.chain_id) == U256::from(v.chain_id), "refused: the proof is for another chain");
        ensure!(B256::from(j.lez_config_id) == v.cid, "refused: the proof is for another LEZ configuration");
        let relayer = Address::from(j.relayer);
        let fee = U256::from_be_bytes(j.relay_fee);
        ensure!(
            relayer == self.me || relayer == Address::ZERO,
            "refused: the proof names relayer {relayer}; only it can be paid for this release (this relayer is {})",
            self.me
        );
        ensure!(!fee.is_zero() || self.accept_unpaid, "refused: the proof pays no relay fee");
        let nullifier = B256::from(j.nullifier);
        ensure!(
            !eth::view(p, self.vault, IVault::nullifierUsedCall { nullifier }, v.block).await?,
            "refused: NOut {nullifier} is already released"
        );
        let token = Address::from(j.token);
        let amount = U256::from_be_bytes(j.amount);
        let cfg = eth::view(p, self.vault, IVault::tokenConfigCall { token }, v.block).await?;
        let denom_index = cfg
            .denominations_
            .iter()
            .position(|d| *d == amount)
            .ok_or_else(|| anyhow!("refused: {amount} is not a registered denomination of {token}"))?;
        let request = IVault::ReleaseRequest {
            token,
            denomIndex: u8::try_from(denom_index)?,
            recipient: Address::from(j.recipient),
            relayer,
            relayFee: fee,
            burnRoot: B256::from(j.burn_root),
            nullifier,
        };
        Ok((request.clone(), eth::release_calldata(request, seal)))
    }

    /// Checks a checkpoint job against the vault: same chain, vault and configuration; built on the
    /// vault's final state; not already a candidate. Returns the `submitCandidate` calldata.
    pub async fn plan_candidate(&self, p: &DynProvider, v: &VaultView, journal: &[u8], seal: Bytes) -> Result<Bytes> {
        let job = checkpoint_from_journal(journal)?;
        ensure!(job.vault == self.vault, "refused: the checkpoint is for vault {}", job.vault);
        ensure!(job.chain_id == v.chain_id, "refused: the checkpoint is for chain {}", job.chain_id);
        ensure!(job.cid == v.cid, "refused: the checkpoint is for another LEZ configuration");
        let c = &job.checkpoint;
        ensure!(
            c.parentState == v.final_state,
            "refused: built on {}, the vault's final state is {} (stale)",
            c.parentState,
            v.final_state
        );
        let cand = eth::view(p, self.vault, IRelayVault::candidatesCall { newState: c.newState }, v.block).await?;
        ensure!(cand.submittedAt.is_zero(), "skipped: {} is already a candidate", c.newState);
        Ok(IRelayVault::submitCandidateCall { c: c.clone(), seal }.abi_encode().into())
    }

    /// Live candidates on the vault (any submitter's), rebuilt from their `CandidateSubmitted` events
    /// and the vault's committee floors, each checked against the journal digest the vault stored.
    pub async fn candidates(&self, p: &DynProvider, from_block: u64) -> Result<Vec<(IRelayVault::Checkpoint, u64)>> {
        self.candidates_at(p, from_block, None).await
    }

    /// The same, read at block `at` (`None`: latest).
    pub async fn candidates_at(
        &self,
        p: &DynProvider,
        from_block: u64,
        at: Option<u64>,
    ) -> Result<Vec<(IRelayVault::Checkpoint, u64)>> {
        // In ranges of LOG_CHUNK blocks: public endpoints limit the range of one eth_getLogs.
        let latest = match at {
            Some(b) => b,
            None => p.get_block_number().await?,
        };
        let mut logs = Vec::new();
        let mut start = from_block;
        while start <= latest {
            let end = (start + LOG_CHUNK - 1).min(latest);
            let filter = Filter::new()
                .address(self.vault)
                .event_signature(IRelayVault::CandidateSubmitted::SIGNATURE_HASH)
                .from_block(start)
                .to_block(end);
            logs.extend(eth::retry_rpc(|| p.get_logs(&filter)).await.with_context(|| format!("eth_getLogs {start}..{end}"))?);
            start = end + 1;
        }
        let n_min = eth::view(p, self.vault, IRelayVault::nMinCall {}, at).await?;
        let t_min = eth::view(p, self.vault, IRelayVault::tMinCall {}, at).await?;
        let chain_id = eth::retry_rpc(|| p.get_chain_id()).await?;
        let cid = eth::view(p, self.vault, IVault::lezConfigIdCall {}, at).await?;
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        for log in logs.iter().rev() {
            let e = log
                .log_decode::<IRelayVault::CandidateSubmitted>()
                .map_err(|e| anyhow!("a CandidateSubmitted log does not decode: {e}"))?
                .inner
                .data;
            if !seen.insert(e.newState) {
                continue;
            }
            let c = IRelayVault::Checkpoint {
                nMin: n_min,
                tMin: t_min,
                parentState: e.parentState,
                newState: e.newState,
                burnRoot: e.burnRoot,
                burnCount: e.burnCount,
                importedLeafCount: e.importedLeafCount,
                sunset: e.sunset,
                echoNonce: B256::ZERO,
                tipId: e.tipId,
                tipHash: e.tipHash,
                n: e.n,
                t: e.t,
            };
            let cand = eth::view(p, self.vault, IRelayVault::candidatesCall { newState: c.newState }, at).await?;
            if cand.submittedAt.is_zero() {
                continue; // finalised, cancelled or replaced
            }
            let digest = B256::from(sha256(&checkpoint_journal(chain_id, self.vault, cid, &c)));
            if digest != cand.journalDigest {
                continue; // a stale candidate's event; the live one is another submission
            }
            let ready_at = cand.submittedAt.to::<u64>() + u64::from(cand.deltaAtSubmit);
            out.push((c, ready_at));
        }
        Ok(out)
    }

    /// Sends `input` to the vault from this relayer, after an `eth_call` (a refusal is reported with
    /// the vault's error, nothing signed).
    pub async fn submit(&self, p: &DynProvider, input: Bytes) -> Result<TransactionReceipt> {
        eth::send(p, self.me, self.vault, input, U256::ZERO, 1).await
    }
}

/// The burn watch: whether the relayer should ask its checkpoint prover for a checkpoint. It should when
/// LEZ has published more burns (`lez_burns`, from the bridge program's `Burn` calls of this configuration)
/// than the newest accepted burn root covers, and no candidate is live. Returns (burns, accepted count).
pub fn checkpoint_needed(lez_burns: u64, accepted: &[(B256, u64)], live_candidates: usize) -> Option<(u64, u64)> {
    let covered = accepted.iter().map(|(_, n)| *n).max().unwrap_or(0);
    (lez_burns > covered && live_candidates == 0).then_some((lez_burns, covered))
}

/// Wraps a holder's succinct release receipt into the seal the vault's verifier takes (bridge.md §12.3: any
/// relayer can wrap, and learns only what the release publishes): identity_p254 through r0vm, then RISC Zero's
/// Groth16 prover image through `wrap_command <input.json> <proof.json>` (Docker on x86, or a remote prover),
/// then the Groth16 receipt as risc0's `succinct_to_groth16` builds it, verified against the vault's release
/// image before it is used. Returns `selector ‖ seal`.
pub fn wrap_release(
    receipt: &risc0_zkvm::Receipt,
    release_image: [u8; 32],
    wrap_command: &Path,
    work: &Path,
) -> Result<Vec<u8>> {
    use risc0_zkvm::sha::{Digest, Digestible as _};
    use risc0_zkvm::{
        ApiClient, Asset, AssetRequest, Groth16Receipt, Groth16ReceiptVerifierParameters, InnerReceipt, ProverOpts,
        Receipt,
    };
    let id = Digest::from_bytes(release_image);
    receipt.verify(id).map_err(|e| anyhow!("refused: the receipt does not verify against the release image: {e}"))?;
    let succinct = receipt.inner.succinct().map_err(|e| anyhow!("refused: not a succinct receipt: {e}"))?;
    let (input, proof) = (work.join("input.json"), work.join("proof.json"));
    let params = Groth16ReceiptVerifierParameters::default().digest();
    let finish = |proof_json: &str| -> Result<Vec<u8>> {
        let pj: risc0_groth16::ProofJson = serde_json::from_str(proof_json)?;
        let seal: risc0_groth16::Seal = pj.try_into().map_err(|e| anyhow!("proof.json: {e:?}"))?;
        let groth16 = Groth16Receipt::new(seal.to_vec(), succinct.claim.clone(), params);
        let wrapped = Receipt::new(InnerReceipt::Groth16(groth16), receipt.journal.bytes.clone());
        wrapped.verify(id).map_err(|e| anyhow!("the wrapped receipt does not verify: {e}"))?;
        Ok([&params.as_bytes()[..4], seal.to_vec().as_slice()].concat())
    };
    // A seal wrapped before a restart (the daemon killed between the wrap and the release) is used again once it
    // verifies: the wrap is not done twice (R.06).
    if let Some(seal) = std::fs::read_to_string(&proof).ok().and_then(|t| finish(&t).ok()) {
        return Ok(seal);
    }
    let asset = Asset::Inline(bincode::serialize(succinct)?.into());
    let p254 = ApiClient::from_env()
        .map_err(|e| anyhow!("r0vm: {e}"))?
        .identity_p254(&ProverOpts::succinct(), asset, AssetRequest::Inline)
        .map_err(|e| anyhow!("identity_p254: {e}"))?;
    let seal_bytes = p254.get_seal_bytes();
    std::fs::create_dir_all(work)?;
    std::fs::write(&input, risc0_groth16::prove::to_json(&seal_bytes).map_err(|e| anyhow!("input.json: {e}"))?)?;
    let status = std::process::Command::new(wrap_command)
        .arg(&input)
        .arg(&proof)
        .status()
        .with_context(|| format!("running {}", wrap_command.display()))?;
    ensure!(status.success(), "the Groth16 prover failed ({status})");
    finish(&std::fs::read_to_string(&proof)?)
}

/// What a `Release` event paid this relayer (the relay fee), if it is the payee.
pub fn relay_fee_paid(receipt: &TransactionReceipt, me: Address) -> Option<(Address, U256)> {
    receipt.inner.logs().iter().find_map(|log| {
        let e = log.log_decode::<IVault::Release>().ok()?.inner.data;
        (e.relayer == me).then_some((e.token, e.relayFee))
    })
}

// ---- the gas drop --------------------------------------------------------------------------------

/// One drop per `N`, kept on disk across restarts (the relayer's own record; the bridge program's
/// nullifier accounts say which `N` are claimed).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DropBook {
    /// `N` (hex) -> the drop.
    pub drops: BTreeMap<String, DropRecord>,
}

/// A drop sent.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DropRecord {
    pub one_time_address: String,
    pub amount: u128,
    pub at: u64,
    pub tx: String,
}

impl DropBook {
    /// Reads the book at `path` (empty if there is none).
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).with_context(|| format!("reading {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Writes the book to `path` (via a temporary file beside it).
    pub fn save(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Whether `n` had its drop.
    pub fn contains(&self, n: &Hash) -> bool {
        self.drops.contains_key(&hx(n))
    }

    /// Records the drop for `n`.
    pub fn record(&mut self, n: &Hash, r: DropRecord) {
        self.drops.insert(hx(n), r);
    }
}

/// Why a gas-drop request is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropRefusal {
    /// The receipt does not verify against the gas-drop image id (fake receipts verify only in
    /// RISC Zero's dev mode).
    BadReceipt(String),
    /// The journal is not a gas-drop journal.
    NotGasDrop,
    /// The receipt is bound to another relayer: replaying it here is refused.
    OtherRelayer,
    /// The receipt is for another chain or vault.
    OtherConfiguration,
    /// The root is not one LEZ imported for this configuration.
    RootNotImported,
    /// The deposit is claimed: no drop for it.
    AlreadyClaimed,
    /// This relayer already sent the drop for this `N`.
    AlreadyDropped,
    /// The one-time account's public keys do not derive the receipt's one-time address: the sealed
    /// credit would go to another account than the one the receipt binds.
    KeysMismatch,
}

impl std::fmt::Display for DropRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            DropRefusal::BadReceipt(e) => return write!(f, "refused: the receipt does not verify ({e})"),
            DropRefusal::NotGasDrop => "refused: not a gas-drop journal",
            DropRefusal::OtherRelayer => "refused: the receipt is bound to another relayer",
            DropRefusal::OtherConfiguration => "refused: the receipt is for another chain or vault",
            DropRefusal::RootNotImported => "refused: the root is not an imported root of this configuration",
            DropRefusal::AlreadyClaimed => "refused: the deposit with this N is already claimed",
            DropRefusal::AlreadyDropped => "refused: this N already had its drop",
            DropRefusal::KeysMismatch => {
                "refused: the one-time account's public keys do not derive the receipt's one-time address"
            }
        };
        f.write_str(text)
    }
}

/// What a relayer checks a gas-drop receipt against.
#[derive(Clone, Copy)]
pub struct DropContext<'a> {
    /// The gas-drop guest's image id.
    pub image_id: [u8; 32],
    /// This relayer's LEZ account id (the one the receipt must name).
    pub me: Hash,
    /// The configuration's chain id (32-byte big-endian) and vault.
    pub chain_id: [u8; 32],
    pub vault: [u8; 20],
    /// Roots LEZ imported for this configuration.
    pub imported_roots: &'a HashSet<Hash>,
}

/// The roots of the vault's deposit tree at each imported leaf count (`counts`, from the bridge
/// program's `ImportRoot` calls), from the verified deposit list.
pub fn imported_roots(deposits: &[VaultDeposit], counts: &[u64]) -> Result<HashSet<Hash>> {
    let wanted: BTreeSet<u64> = counts.iter().copied().collect();
    let mut tree = IncrementalTree::new();
    let mut roots = HashSet::new();
    if wanted.contains(&0) {
        roots.insert(tree.root());
    }
    for d in deposits {
        let leaf = deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment);
        let n = tree.append(leaf).map_err(|_| anyhow!("deposit tree full"))? + 1;
        if wanted.contains(&n) {
            roots.insert(tree.root());
        }
    }
    Ok(roots)
}

/// Checks a gas-drop receipt (JSON, as `risc0_zkvm::Receipt` serialises). Returns its journal if the
/// relayer should send the drop; `claimed` says whether the bridge program recorded `N`.
pub fn check_gas_drop(
    receipt_json: &[u8],
    ctx: &DropContext<'_>,
    claimed: impl Fn(&Hash) -> bool,
    book: &DropBook,
) -> std::result::Result<GasDropJournal, DropRefusal> {
    let receipt: risc0_zkvm::Receipt =
        serde_json::from_slice(receipt_json).map_err(|e| DropRefusal::BadReceipt(format!("not a receipt: {e}")))?;
    receipt
        .verify(risc0_zkvm::sha::Digest::from_bytes(ctx.image_id))
        .map_err(|e| DropRefusal::BadReceipt(e.to_string()))?;
    let j = GasDropJournal::decode(&receipt.journal.bytes).ok_or(DropRefusal::NotGasDrop)?;
    if j.relayer != ctx.me {
        return Err(DropRefusal::OtherRelayer);
    }
    if j.chain_id != ctx.chain_id || j.vault != ctx.vault {
        return Err(DropRefusal::OtherConfiguration);
    }
    if !ctx.imported_roots.contains(&j.root) {
        return Err(DropRefusal::RootNotImported);
    }
    if claimed(&j.nullifier) {
        return Err(DropRefusal::AlreadyClaimed);
    }
    if book.contains(&j.nullifier) {
        return Err(DropRefusal::AlreadyDropped);
    }
    Ok(j)
}

/// The public keys of a gas drop's one-time private account, which the claimant hands over with the
/// receipt: the relayer seals its credit to them, and they must derive the receipt's one-time address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OneTimeKeys {
    /// The nullifier public key.
    pub npk: Hash,
    /// The viewing public key (its bytes as LEZ serialises it).
    pub vpk: Vec<u8>,
}

impl OneTimeKeys {
    /// From the job's hex fields.
    pub fn from_hex(npk: &str, vpk: &str) -> Result<Self> {
        Ok(Self { npk: crate::h32(npk)?, vpk: unhex(vpk)? })
    }

    /// The private account (LEZ's regular kind) these keys derive.
    pub fn account_id(&self) -> Result<Hash> {
        let vpk = lee_core::encryption::ViewingPublicKey::from_bytes(self.vpk.clone())
            .map_err(|e| anyhow!("the one-time account's viewing key: {e:?}"))?;
        let id = lee::AccountId::for_private_account(
            &lee_core::NullifierPublicKey(self.npk),
            &vpk,
            &lee_core::PrivateAccountKind::Regular,
        );
        Ok(*id.value())
    }
}

/// Sends the native credit of a gas drop.
#[allow(async_fn_in_trait)]
pub trait Credit {
    /// Credits `amount` native LEZ to the one-time account `to` (whose public keys are `keys`, when the
    /// request carries them) from the relayer's account; returns the transaction (hex).
    async fn credit(&mut self, to: Hash, keys: Option<&OneTimeKeys>, amount: u128) -> Result<String>;
}

/// A sequencer endpoint through LEZ's wallet: one native transfer from the relayer's private account,
/// sealed to the one-time private account (`seal_destination` with its public keys), left pending for
/// the claim, which sweeps it into its fee payer in the same proof (bridge.md §6; as LEZ v0.4.0-rc3's run,
/// `lez/crown/driver/src/gasdrop.rs`). A public transfer would not do: no private claim can sweep it.
#[cfg(feature = "lez-wallet")]
impl Credit for crate::lez::Sequencer {
    async fn credit(&mut self, to: Hash, keys: Option<&OneTimeKeys>, amount: u128) -> Result<String> {
        use lee::privacy_preserving_transaction::circuit::ProgramCatalog;
        use lee_core::native_token::{self, NATIVE_TOKEN_PROGRAM_ID};
        use wallet::AccountIdentity;
        let keys = keys.ok_or_else(|| {
            anyhow!("refused: no public keys of the one-time account (oneTimeNpk, oneTimeVpk): the credit is sealed to that private account")
        })?;
        ensure!(keys.account_id()? == to, "{}", DropRefusal::KeysMismatch);
        let from = self
            .drop_from
            .ok_or_else(|| anyhow!("no private LEZ account of this relayer to send gas drops from"))?;
        let npk = lee_core::NullifierPublicKey(keys.npk);
        let vpk = lee_core::encryption::ViewingPublicKey::from_bytes(keys.vpk.clone())
            .map_err(|e| anyhow!("the one-time account's viewing key: {e:?}"))?;
        // The relayer's own notes and pending credits, as the zone holds them now.
        self.wallet.sync_to_latest_block().await.map_err(|e| anyhow!("syncing the relayer's LEZ wallet: {e}"))?;
        let recipient = AccountIdentity::PrivateForeign { npk, vpk, kind: lee_core::PrivateAccountKind::Regular };
        let (_, mut delivery) = self
            .wallet
            .seal_destination(recipient.select_program_actor_state(NATIVE_TOKEN_PROGRAM_ID))
            .map_err(|e| anyhow!("sealing the credit to the one-time account: {e}"))?;
        // As LEZ's native transfer facade does for a credit left pending (program_facades/mod.rs).
        delivery.promotions.listed_only = true;
        let message = lee::program::Program::serialize_message(native_token::Message::Transfer {
            to: lee::AccountId::new(to),
            amount,
        })
        .map_err(|e| anyhow!("the transfer's message: {e:?}"))?;
        // Without NativeTokenTransfer's pre-check of the note balance: the send claims the relayer's pending
        // credits first (a freshly funded account holds only those).
        let sent = self
            .wallet
            .send_tx_with_pre_check(
                vec![AccountIdentity::PrivateOwned(from).balance()],
                0,
                message,
                &ProgramCatalog::default(),
                None,
                delivery,
                None,
                |_| Ok(()),
            )
            .await;
        self.synced = false;
        let (hash, _) = sent.map_err(|e| anyhow!("the sealed credit was not sent: {e}: {e:?}"))?;
        Ok(hash.to_string())
    }
}

/// The relayer's zone: a sequencer sends the credit; the devnet's zone (a stand-in) sends none.
impl Credit for crate::lez::Zone {
    async fn credit(&mut self, to: Hash, keys: Option<&OneTimeKeys>, amount: u128) -> Result<String> {
        match self {
            #[cfg(feature = "lez-wallet")]
            crate::lez::Zone::Sequencer(s) => s.credit(to, keys, amount).await,
            crate::lez::Zone::Devnet(_) => {
                let _ = (to, keys, amount);
                bail!("refused: the devnet's zone sends no native credit (gas drops need a sequencer)")
            }
        }
    }
}

/// Checks a gas-drop request and, if it passes, sends the credit and records it (one per `N`). `keys`,
/// the one-time account's public keys, must derive the receipt's one-time address (checked before the
/// drop is recorded).
#[allow(clippy::too_many_arguments)]
pub async fn gas_drop<C: Credit>(
    receipt_json: &[u8],
    keys: Option<&OneTimeKeys>,
    ctx: &DropContext<'_>,
    claimed: impl Fn(&Hash) -> bool,
    book: &mut DropBook,
    book_path: &Path,
    credit: &mut C,
    amount: u128,
    now: u64,
) -> Result<(GasDropJournal, String)> {
    let j = check_gas_drop(receipt_json, ctx, claimed, book).map_err(|r| anyhow!("{r}"))?;
    if let Some(k) = keys {
        if k.account_id()? != j.one_time_address {
            bail!("{}", DropRefusal::KeysMismatch);
        }
    }
    // Recorded before the credit is sent: a crash between the two cannot lead to a second drop for this N
    // after a restart. A credit that fails stays recorded as failed (it may have been sent); the operator
    // removes the entry only knowing it was not.
    let record = |tx: String| DropRecord { one_time_address: hx(&j.one_time_address), amount, at: now, tx };
    book.record(&j.nullifier, record("pending".into()));
    book.save(book_path)?;
    match credit.credit(j.one_time_address, keys, amount).await {
        Ok(tx) => {
            book.record(&j.nullifier, record(tx.clone()));
            book.save(book_path)?;
            Ok((j, tx))
        }
        Err(e) => {
            book.record(&j.nullifier, record(format!("failed: {e:#}")));
            book.save(book_path)?;
            Err(e.context("the credit failed; recorded as failed"))
        }
    }
}

// ---- root imports --------------------------------------------------------------------------------

/// What the relayer's root import finds in a round ([`plan_root_import`]).
#[derive(Clone, Debug)]
pub enum RootImport {
    /// Every deposit of the vault is under an imported root (`imported`: the newest imported count).
    UpToDate { imported: u64 },
    /// The vault holds deposits LEZ has not imported, and there is no record that covers them yet (the
    /// light client is behind, or the zone has no record or instance to read).
    Waiting { reason: String, vault_count: u64, imported: u64 },
    /// The configuration takes no more imports (sunset is one-way).
    Closed { reason: String },
    /// An import, built and dry-run: ready to send.
    Planned(Box<PlannedImport>),
}

/// An `ImportRoot` call and what it imports.
#[derive(Clone, Debug)]
pub struct PlannedImport {
    pub call: crate::lez::LezCall,
    /// The record it reads: `header(id, record_slot)` of the configuration's instance.
    pub record_slot: u64,
    pub block_number: u64,
    pub block_hash: Hash,
    /// The vault's slots 0 and 1 at that block.
    pub root: Hash,
    pub leaf_count: u64,
    /// The newest count imported before it.
    pub imported: u64,
    /// The `root(cid, n)` state the dry run says the import writes.
    pub writes: ferrata_lez_bridge_wire::RootState,
}

/// One storage entry of an `eth_getProof` answer, by slot: its proof nodes and its value as a word.
fn proof_entry(answer: &serde_json::Value, slot: u64) -> Result<(Vec<Vec<u8>>, Hash)> {
    for e in answer["storageProof"].as_array().ok_or_else(|| anyhow!("eth_getProof: no storageProof"))? {
        if quantity(&e["key"])? == U256::from(slot) {
            return Ok((nodes(&e["proof"])?, quantity(&e["value"])?.to_be_bytes::<32>()));
        }
    }
    bail!("eth_getProof: no proof of slot {slot}")
}

fn quantity(v: &serde_json::Value) -> Result<U256> {
    let text = v.as_str().ok_or_else(|| anyhow!("eth_getProof: not a hex quantity: {v}"))?;
    U256::from_str_radix(text.trim_start_matches("0x"), 16).map_err(|e| anyhow!("eth_getProof: {text}: {e}"))
}

fn nodes(v: &serde_json::Value) -> Result<Vec<Vec<u8>>> {
    v.as_array()
        .ok_or_else(|| anyhow!("eth_getProof: proof nodes are not a list"))?
        .iter()
        .map(|n| unhex(n.as_str().ok_or_else(|| anyhow!("eth_getProof: a proof node is not hex"))?))
        .collect()
}

/// The vault's account proof and the proofs of its slots 0 (the deposit root) and 1 (the leaf count)
/// from an `eth_getProof` answer, with the two values.
pub fn import_proofs(answer: &serde_json::Value) -> Result<(Vec<Vec<u8>>, (Vec<Vec<u8>>, Hash), (Vec<Vec<u8>>, Hash))> {
    let account = nodes(&answer["accountProof"])?;
    Ok((account, proof_entry(answer, 0)?, proof_entry(answer, 1)?))
}

/// The relayer's root import (bridge.md §6): the configuration's newest record on the zone (the
/// instance's finalized header, `header(id, slot)`), the vault's slots 0 and 1 at that record's block
/// (`eth_getProof` from the relayer's Ethereum endpoint, the block's hash checked against the record;
/// under a Gloas record, which binds the block's hash only, the block's header rebuilt from the endpoint,
/// checked against that hash and sent in front of the account proof),
/// and, when its count is above the newest imported one, `ImportRoot` built and dry-run with
/// `ferrata_wallet::import` (the attestation program's `evaluate` and the bridge program's
/// `check_import`, natively): an import the programs would refuse is an error here, and nothing is sent.
/// `vault_count` is the vault's `nextLeafIndex` now (what the relayer waits to cover).
pub async fn plan_root_import<L: crate::lez::Ledger>(
    ledger: &L,
    p: &DynProvider,
    ctx: &crate::lez::Context,
    history: &crate::lez::History,
    vault_count: u64,
) -> Result<RootImport> {
    use borsh::BorshDeserialize as _;
    use ferrata_core::witness::ExecutionSummary;
    use ferrata_lez_bridge_wire::{config_actor, BridgeState, ConfigState};
    use ferrata_wallet::import;

    let imported = history.roots.iter().copied().max().unwrap_or(0);
    let waiting = |reason: String| -> Result<RootImport> { Ok(RootImport::Waiting { reason, vault_count, imported }) };
    let config = match crate::lez::read(ledger, config_actor(ctx.program, &ctx.cid)).await? {
        Some(BridgeState::Config(ConfigState::Ready { sunset: true, .. })) => {
            return Ok(RootImport::Closed {
                reason: "the configuration is sunset: no root is imported after it".into(),
            })
        }
        Some(BridgeState::Config(ConfigState::Ready { config, .. })) => config,
        Some(BridgeState::Config(ConfigState::Pairing { .. })) => {
            return waiting("the configuration is pairing (Init is outstanding)".into())
        }
        None => return waiting("the zone holds no config(cid): the configuration is not initialised".into()),
        Some(other) => bail!("config(cid) holds another state: {other:?}"),
    };
    if vault_count <= imported {
        return Ok(RootImport::UpToDate { imported });
    }
    let Some(instance) = crate::lez::instance(ledger, config.attestation, &config.instance_id).await? else {
        return waiting(format!("the zone holds no instance {} of the attestation program", hx(&config.instance_id)));
    };
    if !instance.active {
        return waiting("the configuration's instance is not active yet".into());
    }
    let slot = instance.finalized_slot;
    let bytes =
        ledger.actor_state(ferrata_lez_wire::header_actor(config.attestation, &config.instance_id, slot)).await?;
    if bytes.is_empty() {
        return waiting(format!("no record at header(id, {slot}), the instance's finalized slot"));
    }
    let record = ferrata_lez_wire::RecordState::try_from_slice(&bytes)
        .map_err(|e| anyhow!("header(id, {slot}) holds no record: {e}"))?;
    let header =
        record.header().map_err(|code| anyhow!("header(id, {slot})'s record does not decode (code {code})"))?;
    let (block_number, block_hash, rlp_header) = match header.execution {
        ExecutionSummary::Payload { block_number, block_hash, .. } => (block_number, block_hash, None),
        ExecutionSummary::BlockHash(hash) => {
            // A Gloas record binds the block's hash only: the import carries the block's RLP header in front of the
            // account proof (the programs take the state root from it), rebuilt here from the endpoint's block and
            // checked against the recorded hash.
            let block: serde_json::Value = p
                .raw_request("eth_getBlockByHash".into(), (hx(&hash), false))
                .await
                .with_context(|| format!("eth_getBlockByHash {}", hx(&hash)))?;
            if block.is_null() {
                return waiting(format!(
                    "header(id, {slot}) records block {}, which the Ethereum endpoint does not serve",
                    hx(&hash)
                ));
            }
            let rlp = header_rlp(&block).with_context(|| format!("the header of block {}", hx(&hash)))?;
            let rebuilt = alloy::primitives::keccak256(&rlp);
            if rebuilt.0 != hash {
                bail!(
                    "the Ethereum endpoint's block {} rebuilds to a header hashing to {}: refused",
                    hx(&hash),
                    hx(&rebuilt.0)
                );
            }
            let number = ferrata_lez_bridge_wire::u64_of(&quantity(&block["number"])?.to_be_bytes::<32>())
                .ok_or_else(|| anyhow!("block {}: its number is above 2^64", hx(&hash)))?;
            (number, hash, Some(rlp))
        }
        ExecutionSummary::None => return waiting(format!("header(id, {slot}) is an Altair record: no execution block")),
    };
    // The endpoint's block at that height must be the recorded one (another chain, or a node behind, is refused).
    let tag = format!("0x{block_number:x}");
    let block: serde_json::Value = p
        .raw_request("eth_getBlockByNumber".into(), (tag.clone(), false))
        .await
        .with_context(|| format!("eth_getBlockByNumber {block_number}"))?;
    let served = block["hash"].as_str().map(unhex).transpose()?.unwrap_or_default();
    if served != block_hash {
        bail!(
            "the Ethereum endpoint's block {block_number} is {}, not the recorded {}: refused",
            if served.is_empty() { "unknown".to_string() } else { hx(&served) },
            hx(&block_hash)
        );
    }
    let answer: serde_json::Value = p
        .raw_request("eth_getProof".into(), (hx(&ctx.vault), [hx(&[0u8; 32]), hx(&word_of(1))], tag))
        .await
        .with_context(|| format!("eth_getProof of the vault at block {block_number}"))?;
    let (account, (root_nodes, root), (count_nodes, count_word)) = import_proofs(&answer)?;
    let leaf_count = ferrata_lez_bridge_wire::u64_of(&count_word)
        .ok_or_else(|| anyhow!("slot 1 (the leaf count) at block {block_number} is above 2^64"))?;
    if leaf_count <= imported {
        return waiting(format!(
            "the newest record, header(id, {slot}) of block {block_number}, holds {leaf_count} deposits, {imported} are imported, the vault holds {vault_count}: waiting for a later record"
        ));
    }
    let proof = import::import_proof(account, root_nodes, count_nodes, rlp_header);
    let writes = import::dry_run_import(&ctx.cid, &config, &record, slot, root, leaf_count, &proof).map_err(|code| {
        anyhow!(crate::codes::program_refusal(
            u32::from(code),
            format!(
                "the dry run of ImportRoot n = {leaf_count} under header(id, {slot}) (the attestation and bridge programs' own checks): not sent"
            )
        ))
    })?;
    let c = import::import_call(ctx.program, &ctx.cid, &config, slot, root, leaf_count, proof);
    let call = crate::lez::LezCall {
        what: c.what,
        mentions: c.mentions.into_iter().map(Into::into).collect(),
        root_index: c.root_index,
        message: c.message,
    };
    Ok(RootImport::Planned(Box::new(PlannedImport {
        call,
        record_slot: slot,
        block_number,
        block_hash,
        root,
        leaf_count,
        imported,
        writes,
    })))
}

fn word_of(n: u64) -> Hash {
    U256::from(n).to_be_bytes::<32>()
}

/// The header fields of an `eth_getBlockBy*` answer in the header's order (`true`: an integer); a block
/// carries a prefix of them (each fork appends), as `scripts/fetch-mainnet-e2e.py` rebuilds headers.
const HEADER_FIELDS: [(&str, bool); 23] = [
    ("parentHash", false),
    ("sha3Uncles", false),
    ("miner", false),
    ("stateRoot", false),
    ("transactionsRoot", false),
    ("receiptsRoot", false),
    ("logsBloom", false),
    ("difficulty", true),
    ("number", true),
    ("gasLimit", true),
    ("gasUsed", true),
    ("timestamp", true),
    ("extraData", false),
    ("mixHash", false),
    ("nonce", false),
    ("baseFeePerGas", true),
    ("withdrawalsRoot", false),
    ("blobGasUsed", true),
    ("excessBlobGas", true),
    ("parentBeaconBlockRoot", false),
    ("requestsHash", false),
    ("blockAccessListHash", false),
    ("slotNumber", true),
];

/// A block's RLP header, rebuilt from an `eth_getBlockByHash` (or `ByNumber`) answer; the caller
/// checks its keccak against the hash it expects. Refuses fields that are not a prefix of the known order.
pub fn header_rlp(block: &serde_json::Value) -> Result<Vec<u8>> {
    fn prefix(len: usize, offset: u8) -> Vec<u8> {
        if len < 56 {
            return vec![offset + len as u8];
        }
        let be: Vec<u8> = (len as u64).to_be_bytes().into_iter().skip_while(|b| *b == 0).collect();
        let mut out = vec![offset + 55 + be.len() as u8];
        out.extend(be);
        out
    }
    fn item(b: &[u8]) -> Vec<u8> {
        if b.len() == 1 && b[0] < 0x80 {
            return b.to_vec();
        }
        let mut out = prefix(b.len(), 0x80);
        out.extend_from_slice(b);
        out
    }
    let present = HEADER_FIELDS.iter().take_while(|(name, _)| !block[*name].is_null()).count();
    if let Some((late, _)) = HEADER_FIELDS[present..].iter().find(|(name, _)| !block[*name].is_null()) {
        bail!("the block's header fields are not a prefix of the known order ({late} without the fields before it)");
    }
    ensure!(present >= 15, "the block has {present} of a header's 15 fields at least");
    let mut payload = Vec::new();
    for (name, int) in &HEADER_FIELDS[..present] {
        let bytes = if *int {
            quantity(&block[*name])?.to_be_bytes::<32>().into_iter().skip_while(|b| *b == 0).collect()
        } else {
            unhex(block[*name].as_str().ok_or_else(|| anyhow!("the block's {name} is not hex"))?)?
        };
        payload.extend(item(&bytes));
    }
    let mut out = prefix(payload.len(), 0xc0);
    out.extend(payload);
    Ok(out)
}

/// A job file in the relayer's inbox (`<inbox>/*.json`), handed over by a user who opts in or by
/// the checkpoint prover. Nothing in it is secret.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Job {
    /// A release: the release journal and the Groth16 seal (hex).
    Release { journal: String, seal: String },
    /// A release as the holder's succinct receipt (JSON): the relayer wraps it (`--wrap-command`).
    ReleaseSuccinct { receipt: serde_json::Value },
    /// A rule (B) checkpoint: the checkpoint journal and the Groth16 seal (hex).
    Checkpoint { journal: String, seal: String },
    /// A gas drop: the receipt (JSON) and the one-time private account's public keys (hex), which derive
    /// the receipt's one-time address and which the relayer seals its credit to.
    GasDrop {
        receipt: serde_json::Value,
        #[serde(rename = "oneTimeNpk")]
        one_time_npk: String,
        #[serde(rename = "oneTimeVpk")]
        one_time_vpk: String,
    },
}

/// Bytes from hex (with or without `0x`).
pub fn unhex(text: &str) -> Result<Vec<u8>> {
    hex::decode(text.trim().trim_start_matches("0x")).map_err(|e| anyhow!("not hex: {e}"))
}

/// Refuses to run on a chain other than the one named (the fork test runs on Sepolia's id).
pub fn require_chain(view: &VaultView, chain_id: Option<u64>) -> Result<()> {
    if let Some(c) = chain_id {
        if view.chain_id != c {
            bail!("the endpoint serves chain {}, not {c}", view.chain_id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_rlp_rebuilds_the_block_hash() {
        // A mainnet block of 9 Nov 2026 (Prague fields up to requestsHash), as eth_getBlockByNumber served it.
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../crates/ferrata-spectests/fixtures/mainnet-e2e-2026-11-09/block_26371634.json"
        ))
        .unwrap();
        let block = &fixture["result"];
        let rlp = header_rlp(block).unwrap();
        assert_eq!(hx(&alloy::primitives::keccak256(&rlp).0), block["hash"].as_str().unwrap());
        // A field after a missing one is refused; a changed field changes the hash (the caller refuses it).
        let mut gap = block.clone();
        gap.as_object_mut().unwrap().remove("withdrawalsRoot");
        assert!(header_rlp(&gap).unwrap_err().to_string().contains("not a prefix"));
        let mut other = block.clone();
        other["gasUsed"] = serde_json::json!("0x1");
        assert_ne!(alloy::primitives::keccak256(header_rlp(&other).unwrap()), alloy::primitives::keccak256(&rlp));
    }

    #[test]
    fn import_proofs_read_an_eth_get_proof_answer() {
        let answer = serde_json::json!({
            "accountProof": ["0xf8", "0x01"],
            "storageProof": [
                { "key": "0x0000000000000000000000000000000000000000000000000000000000000001", "value": "0x2", "proof": ["0xaa"] },
                { "key": "0x0", "value": "0x1234", "proof": ["0xbb", "0xcc"] },
            ],
        });
        let (account, (root_nodes, root), (count_nodes, count)) = import_proofs(&answer).unwrap();
        assert_eq!(account, vec![vec![0xf8], vec![0x01]]);
        assert_eq!(root_nodes, vec![vec![0xbb], vec![0xcc]]);
        assert_eq!(root[30..], [0x12, 0x34]);
        assert!(root[..30].iter().all(|b| *b == 0));
        assert_eq!(count_nodes, vec![vec![0xaa]]);
        assert_eq!(count, word_of(2));
        assert!(import_proofs(&serde_json::json!({ "accountProof": [], "storageProof": [] })).is_err());
        assert!(import_proofs(&serde_json::json!({ "storageProof": [] })).is_err());
    }

    fn checkpoint() -> IRelayVault::Checkpoint {
        IRelayVault::Checkpoint {
            nMin: 1,
            tMin: 1,
            parentState: B256::repeat_byte(1),
            newState: B256::repeat_byte(2),
            burnRoot: B256::repeat_byte(3),
            burnCount: 2,
            importedLeafCount: 2,
            sunset: false,
            echoNonce: B256::ZERO,
            tipId: 252,
            tipHash: B256::repeat_byte(4),
            n: 1,
            t: 1,
        }
    }

    #[test]
    fn checkpoint_journal_round_trips_and_refuses_others() {
        let (vault, cid) = (Address::repeat_byte(9), B256::repeat_byte(7));
        let j = checkpoint_journal(11155111, vault, cid, &checkpoint());
        assert_eq!(j.len(), 17 * 32);
        let job = checkpoint_from_journal(&j).unwrap();
        assert_eq!((job.chain_id, job.vault, job.cid, job.checkpoint), (11155111, vault, cid, checkpoint()));
        assert!(checkpoint_from_journal(&j[..16 * 32]).is_err(), "a short journal");
        let mut other = j.clone();
        other[0] ^= 1;
        assert!(checkpoint_from_journal(&other).is_err(), "another domain");
        let mut not_bool = j.clone();
        not_bool[11 * 32 + 31] = 2;
        assert!(checkpoint_from_journal(&not_bool).is_err(), "sunset is not a bool");
    }

    #[test]
    fn the_recorded_crown_checkpoint_decodes() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../e2e/runs/crown-live-1/checkpoint/receipt/journal.hex");
        let j = unhex(&std::fs::read_to_string(path).unwrap()).unwrap();
        let job = checkpoint_from_journal(&j).unwrap();
        assert_eq!(job.chain_id, 11155111);
        assert_eq!((job.checkpoint.burnCount, job.checkpoint.tipId, job.checkpoint.t), (2, 252, 1));
        assert_eq!(
            checkpoint_journal(job.chain_id, job.vault, job.cid, &job.checkpoint),
            j,
            "re-encodes byte for byte"
        );
    }

    #[test]
    fn temporary_failures_are_retried_and_refusals_are_not() {
        assert!(temporary("refused: vault error BurnRootNotAccepted() (eth_call before sending): …"));
        assert!(temporary("refused: vault error ReleaseDelayNotElapsed() …"));
        assert!(temporary("eth_call: error sending request for url (http://…)"));
        assert!(temporary(
            "eth_call tokenConfig(address): HTTP error 429 with body: {\"error\": {\"code\": -32005, \"message\": \"Too Many Requests\"}}"
        ));
        assert!(temporary("the endpoint failed (eth_call before sending): HTTP error 503 with body: service unavailable"));
        assert!(temporary("server returned an error response: error code -32000: header not found"));
        assert!(temporary("0xab… not mined within 180 s: kept in flight, resolved next round"));
        assert!(!temporary("not a relayer job"));
        assert!(!temporary("the transaction 0x… reverted"));
        assert!(!temporary("refused: the proof names relayer 0x…; only it can be paid for this release"));
        assert!(!temporary("refused: NOut 0x… is already released"));
        assert!(!temporary("refused: vault error VerificationFailed() …"));
    }

    #[test]
    fn the_burn_watch_asks_for_a_checkpoint_only_when_needed() {
        let r = B256::repeat_byte(5);
        assert_eq!(checkpoint_needed(11, &[], 0), Some((11, 0)));
        assert_eq!(checkpoint_needed(11, &[(r, 11)], 0), None);
        assert_eq!(checkpoint_needed(12, &[(r, 11)], 1), None, "a live candidate: wait for it");
        assert_eq!(checkpoint_needed(12, &[(r, 4), (r, 11)], 0), Some((12, 11)));
        assert_eq!(checkpoint_needed(0, &[], 0), None);
    }

    #[test]
    fn imported_roots_are_the_tree_roots_at_the_imported_counts() {
        let deposits: Vec<VaultDeposit> = (0..3u64)
            .map(|i| VaultDeposit {
                depositor: [i as u8 + 1; 20],
                j: 0,
                leaf_index: i,
                token: [0; 20],
                amount: [0x11; 32],
                commitment: [i as u8 + 0x40; 32],
            })
            .collect();
        let mut tree = IncrementalTree::new();
        let mut at = Vec::new();
        for d in &deposits {
            tree.append(deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment)).unwrap();
            at.push(tree.root());
        }
        let roots = imported_roots(&deposits, &[1, 3]).unwrap();
        assert_eq!(roots, [at[0], at[2]].into_iter().collect());
        assert!(!roots.contains(&at[1]), "a count LEZ did not import gives no root");
        assert!(imported_roots(&deposits, &[0]).unwrap().contains(&IncrementalTree::new().root()));
    }
}
