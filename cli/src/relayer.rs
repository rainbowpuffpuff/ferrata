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
    #[allow(missing_docs, clippy::too_many_arguments)]
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
    parse_checkpoint_journal(j).map_err(|e| anyhow!("not a checkpoint journal: {e:#}"))
}

fn parse_checkpoint_journal(j: &[u8]) -> Result<CheckpointJob> {
    ensure!(j.len() == 17 * 32, "a checkpoint journal has 17 words, this one {} bytes", j.len());
    let w = |i: usize| &j[i * 32..(i + 1) * 32];
    ensure!(w(0) == checkpoint_domain(), "another domain");
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
pub const FINAL: [&str; 9] = [
    "refused",
    "skipped:",
    "reverted",
    "not a relayer job",
    "not a release journal",
    "not a checkpoint journal",
    "not hex",
    "not a receipt",
    "would revert",
];

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

fn settled_by_default() -> bool {
    true
}

/// A drop: recorded before its credit is sent, settled once LEZ shows the credit applied ([`drop_step`]).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DropRecord {
    pub one_time_address: String,
    pub amount: u128,
    pub at: u64,
    /// The credit's last transaction (hex); `pending` while LEZ's wallet has not answered its send, `send failed: …`
    /// when it failed (sent again after `--drop-wait`), `failed: …` in books written before the journal.
    pub tx: String,
    /// The private nonce (decimal) of the relayer's drop account before this drop's first credit. The drop account
    /// spends only for drops, one drop at a time, and every credit of this drop is built on exactly that state
    /// ([`Credit::credit_on`]: after its own sync, a credit finds the account moved and sends nothing): the credit is
    /// applied once the account's nonce has moved, and of two credits built on the same state LEZ applies at most one
    /// (the second spends a nullifier the first spent). `None`: a book written before the journal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_nonce: Option<String>,
    /// Applied on LEZ, or given up (the deposit claimed without it, `--drop-max-sends` credits sent, or past
    /// `--max-job-age`, with nothing applied): nothing more is sent for this `N`. Books written before the journal hold settled drops only.
    #[serde(default = "settled_by_default")]
    pub settled: bool,
    /// Credits sent for this `N` (the first, and each one sent again after `--drop-wait`).
    #[serde(default)]
    pub sends: u32,
    /// When the last credit was sent (Unix seconds).
    #[serde(default)]
    pub sent_at: u64,
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
        use std::io::Write as _;
        let tmp = path.with_extension("tmp");
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&serde_json::to_vec_pretty(self)?)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Whether `n` had its drop.
    pub fn contains(&self, n: &Hash) -> bool {
        self.drops.contains_key(&hx(n))
    }

    /// The drop in flight (recorded, not settled), if any: at most one at a time.
    pub fn in_flight(&self) -> Option<(&String, &DropRecord)> {
        self.drops.iter().find(|(_, r)| !r.settled)
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

/// The receipt's own checks (JSON, as `risc0_zkvm::Receipt` serialises): it verifies against the gas-drop image id,
/// names this relayer and this configuration, under a root LEZ imported. Not the book or the claims ([`check_gas_drop`]).
pub fn check_gas_drop_receipt(
    receipt_json: &[u8],
    ctx: &DropContext<'_>,
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
    Ok(j)
}

/// Checks a gas-drop receipt (JSON, as `risc0_zkvm::Receipt` serialises). Returns its journal if the
/// relayer should send the drop; `claimed` says whether the bridge program recorded `N`.
pub fn check_gas_drop(
    receipt_json: &[u8],
    ctx: &DropContext<'_>,
    claimed: impl Fn(&Hash) -> bool,
    book: &DropBook,
) -> std::result::Result<GasDropJournal, DropRefusal> {
    let j = check_gas_drop_receipt(receipt_json, ctx)?;
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

    /// The private account (LEZ's regular kind) these keys derive. A viewing key that does not decode is the job's
    /// fault: refused, not retried.
    pub fn account_id(&self) -> Result<Hash> {
        let vpk = lee_core::encryption::ViewingPublicKey::from_bytes(self.vpk.clone())
            .map_err(|e| anyhow!("refused: the one-time account's viewing key does not decode: {e:?}"))?;
        let id = lee::AccountId::for_private_account(
            &lee_core::NullifierPublicKey(self.npk),
            &vpk,
            &lee_core::PrivateAccountKind::Regular,
        );
        Ok(*id.value())
    }
}

/// What [`Credit::credit_on`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Built {
    /// Built on the state asked for and sent: the transaction (hex).
    Sent(String),
    /// Nothing built or sent: after the zone's own sync the drop account is no longer at the state asked for (its
    /// nonce now): a credit built on that state was applied meanwhile.
    Moved(u128),
}

/// Sends the native credit of a gas drop.
#[allow(async_fn_in_trait)]
pub trait Credit {
    /// Credits `amount` native LEZ to the one-time account `to` (whose public keys are `keys`, when the
    /// request carries them) from the relayer's account; returns the transaction (hex).
    async fn credit(&mut self, to: Hash, keys: Option<&OneTimeKeys>, amount: u128) -> Result<String>;
    /// The private nonce of the account the credits come from, as the zone holds it now (after a sync): it moves
    /// when a credit of the relayer's is applied ([`DropRecord::from_nonce`]). `None`: no such account here (the
    /// journaled drop, [`drop_step`], is not available).
    async fn drop_nonce(&mut self) -> Result<Option<u128>> {
        Ok(None)
    }
    /// The credit of a journaled drop ([`drop_step`]), built only on the drop account's state `on` (its nonce as the
    /// drop's record holds it): the zone syncs, and a drop account no longer at `on` sends nothing ([`Built::Moved`]);
    /// otherwise the credit is built on exactly that synced state, so every credit of the drop spends the same
    /// nullifier and LEZ applies at most one of them, whenever each lands. Zones without a drop account refuse.
    async fn credit_on(&mut self, to: Hash, keys: &OneTimeKeys, amount: u128, on: u128) -> Result<Built> {
        let _ = (to, keys, amount, on);
        bail!("refused: this zone builds no journaled gas drop (no drop account)")
    }
}

/// A sequencer endpoint through LEZ's wallet: one native transfer from the relayer's private account,
/// sealed to the one-time private account (`seal_destination` with its public keys), left pending for
/// the claim, which sweeps it into its fee payer in the same proof (bridge.md §6; as LEZ v0.4.0-rc3's run,
/// `lez/crown/driver/src/gasdrop.rs`). A public transfer would not do: no private claim can sweep it.
#[cfg(feature = "lez-wallet")]
impl Credit for crate::lez::Sequencer {
    async fn credit(&mut self, to: Hash, keys: Option<&OneTimeKeys>, amount: u128) -> Result<String> {
        match self.sealed_credit(to, keys, amount, None).await? {
            Built::Sent(tx) => Ok(tx),
            Built::Moved(n) => bail!("the sealed credit was not sent: the drop account moved (nonce {n})"),
        }
    }

    async fn drop_nonce(&mut self) -> Result<Option<u128>> {
        let from = self
            .drop_from
            .ok_or_else(|| anyhow!("no private LEZ account of this relayer to send gas drops from"))?;
        // LEZ's wallet changes its private accounts only from what a sync finds applied, never when it sends.
        self.wallet.sync_to_latest_block().await.map_err(|e| anyhow!("syncing the relayer's LEZ wallet: {e}"))?;
        self.synced = true;
        let account = self
            .wallet
            .get_account_private(from)
            .ok_or_else(|| anyhow!("LEZ's wallet does not hold the relayer's drop account {from}"))?;
        Ok(Some(account.nonce.0))
    }

    async fn credit_on(&mut self, to: Hash, keys: &OneTimeKeys, amount: u128, on: u128) -> Result<Built> {
        self.sealed_credit(to, Some(keys), amount, Some(on)).await
    }
}

#[cfg(feature = "lez-wallet")]
impl crate::lez::Sequencer {
    /// The sealed credit ([`Credit`] for a sequencer); with `on`, built only on that state of the drop account.
    async fn sealed_credit(
        &mut self,
        to: Hash,
        keys: Option<&OneTimeKeys>,
        amount: u128,
        on: Option<u128>,
    ) -> Result<Built> {
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
            .map_err(|e| anyhow!("refused: the one-time account's viewing key does not decode: {e:?}"))?;
        // The relayer's own notes and pending credits, as the zone holds them now. LEZ's wallet builds from its
        // storage as of this sync (`send_tx_with_pre_check` takes `&self`): the credit below is built on exactly the
        // state checked here, whatever lands at the sequencer meanwhile.
        self.wallet.sync_to_latest_block().await.map_err(|e| anyhow!("syncing the relayer's LEZ wallet: {e}"))?;
        if let Some(on) = on {
            let now = self
                .wallet
                .get_account_private(from)
                .ok_or_else(|| anyhow!("LEZ's wallet does not hold the relayer's drop account {from}"))?
                .nonce
                .0;
            if now != on {
                self.synced = true;
                return Ok(Built::Moved(now));
            }
        }
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
        Ok(Built::Sent(hash.to_string()))
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

    async fn drop_nonce(&mut self) -> Result<Option<u128>> {
        match self {
            #[cfg(feature = "lez-wallet")]
            crate::lez::Zone::Sequencer(s) => s.drop_nonce().await,
            crate::lez::Zone::Devnet(_) => Ok(None),
        }
    }

    async fn credit_on(&mut self, to: Hash, keys: &OneTimeKeys, amount: u128, on: u128) -> Result<Built> {
        match self {
            #[cfg(feature = "lez-wallet")]
            crate::lez::Zone::Sequencer(s) => s.credit_on(to, keys, amount, on).await,
            crate::lez::Zone::Devnet(_) => {
                let _ = (to, keys, amount, on);
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
    // removes the entry only knowing it was not. (The relayer daemon resumes such drops instead: [`drop_step`].)
    let record = |tx: String| DropRecord {
        one_time_address: hx(&j.one_time_address),
        amount,
        at: now,
        tx,
        from_nonce: None,
        settled: true,
        sends: 1,
        sent_at: now,
    };
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

/// The daemon's terms for a journaled drop ([`drop_step`]).
#[derive(Clone, Copy, Debug)]
pub struct DropTerms {
    /// Native LEZ per drop.
    pub amount: u128,
    /// Now (Unix seconds).
    pub now: u64,
    /// Seconds a credit sent may stay unapplied before it is sent again (`--drop-wait`).
    pub drop_wait: u64,
    /// Credits sent for one drop at most (`--drop-max-sends`): a drop still not applied `--drop-wait` after the last
    /// of them is given up, so that a drop whose credit keeps failing does not hold every later drop back.
    pub max_sends: u32,
    /// How long the job has waited in the inbox, and how long it may (`--max-job-age`): a drop with nothing applied
    /// by then is given up.
    pub job_age: u64,
    pub max_job_age: u64,
}

/// Where a journaled drop stands after one [`drop_step`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropStep {
    /// The credit is applied on LEZ (the drop account's nonce moved past the record): the job is done. `resolved`:
    /// found applied in a later step than the one that sent it (after a lost answer, a restart, a kill, or while a
    /// credit was being sent again).
    Applied { tx: String, sends: u32, resolved: bool },
    /// Recorded and not applied yet (sent and waited for, its send failed, a kill left it unsent, or another drop
    /// is in flight): the job stays in the inbox ("in flight") and the next round takes the next step.
    InFlight { reason: String },
    /// Given up with nothing applied (the deposit claimed without it, `--drop-max-sends` credits sent, or the job
    /// past `--max-job-age`): settled, nothing more is sent for this `N`, and the job is refused.
    GivenUp { why: String, sends: u32 },
}

/// One step of a gas drop with its journal (RFP-022 R.06: resumed after a failed or interrupted credit, never sent
/// twice to LEZ's effect). The receipt's checks first ([`check_gas_drop_receipt`]), then [`drop_step`].
#[allow(clippy::too_many_arguments)]
pub async fn gas_drop_step<C: Credit>(
    receipt_json: &[u8],
    keys: &OneTimeKeys,
    ctx: &DropContext<'_>,
    claimed: impl Fn(&Hash) -> bool,
    book: &mut DropBook,
    book_path: &Path,
    credit: &mut C,
    terms: &DropTerms,
) -> Result<(GasDropJournal, DropStep)> {
    let j = check_gas_drop_receipt(receipt_json, ctx).map_err(|r| anyhow!("{r}"))?;
    let step = drop_step(&j, keys, claimed, book, book_path, credit, terms).await?;
    Ok((j, step))
}

/// The state of the drop account a recorded drop's credits are built on ([`DropRecord::from_nonce`]).
fn recorded_state(r: &DropRecord, short: &str) -> Result<u128> {
    r.from_nonce.as_deref().and_then(|n| n.parse().ok()).ok_or_else(|| {
        anyhow!("refused: the gas drop for N {short}… has no recorded state of the drop account (a book written before the journal)")
    })
}

/// A drop settled with nothing applied.
fn give_up_drop(n: &Hash, mut r: DropRecord, why: String, book: &mut DropBook, book_path: &Path) -> Result<DropStep> {
    r.settled = true;
    r.tx = format!("given up, nothing applied ({why}); last: {}", r.tx);
    let sends = r.sends;
    book.record(n, r);
    book.save(book_path)?;
    Ok(DropStep::GivenUp { why, sends })
}

/// The drop for `j` with its journal (the book, saved before every send):
/// - a new `N`: refused if claimed; waits while another drop is in flight (one at a time: the drop account's nonce
///   then speaks for this drop alone; a drop left in flight longer than `--max-job-age` and `--drop-wait` after its
///   record, its job gone, is settled first: applied if the account moved, else given up); else the drop account's
///   nonce is recorded, then the credit is sent;
/// - a recorded `N` not settled: the drop account's nonce moved since the record → applied (settled, done); not
///   moved → waited for until `--drop-wait` after the last send, then sent again, built only on the recorded state
///   ([`Credit::credit_on`]: a credit that lands while the next is being built is found, and nothing is sent), so of
///   all the credits sent LEZ applies at most one; the deposit claimed without it or `--drop-max-sends` credits sent
///   (each `--drop-wait` after the last send), or the job past `--max-job-age`, with nothing applied → given up;
/// - a settled `N`: refused (`AlreadyDropped`).
///
/// Not told apart: a credit of a drop given up that a sequencer still holds and applies after the give-up moves the
/// drop account as the next drop's own credit would (which LEZ then refuses: the same state), so that next drop is
/// booked applied, its `N` settled: its claimant gets nothing more from this relayer and must name another or pay its
/// fee itself. A give-up waits `--drop-wait` after the last send for that reason (past `--max-job-age`, not).
///
/// Errors are refusals (final for the job) or the zone's failures (the job is retried; nothing is lost: the record
/// stays and the next step resolves it).
pub async fn drop_step<C: Credit>(
    j: &GasDropJournal,
    keys: &OneTimeKeys,
    claimed: impl Fn(&Hash) -> bool,
    book: &mut DropBook,
    book_path: &Path,
    credit: &mut C,
    t: &DropTerms,
) -> Result<DropStep> {
    if keys.account_id()? != j.one_time_address {
        bail!("{}", DropRefusal::KeysMismatch);
    }
    let key = hx(&j.nullifier);
    let short = &key[..18.min(key.len())];
    // The drop's last transaction and its number of sends before this step (none for a new N).
    let (mut r, before) = match book.drops.get(&key).cloned() {
        Some(r) if r.settled => bail!("{}", DropRefusal::AlreadyDropped),
        Some(mut r) => {
            let from = recorded_state(&r, short)?;
            let nonce = credit.drop_nonce().await?.ok_or_else(|| anyhow!("refused: this zone sends no gas drops"))?;
            if nonce != from {
                r.settled = true;
                book.record(&j.nullifier, r.clone());
                book.save(book_path)?;
                return Ok(DropStep::Applied { tx: r.tx, sends: r.sends, resolved: true });
            }
            let since = t.now.saturating_sub(r.sent_at);
            let give_up = if t.job_age >= t.max_job_age {
                Some(format!("not applied within --max-job-age ({} s)", t.max_job_age))
            } else if since < t.drop_wait {
                None
            } else if claimed(&j.nullifier) {
                Some("the deposit was claimed without it".to_string())
            } else if r.sends >= t.max_sends {
                Some(format!("not applied {} s after the last of --drop-max-sends ({}) credits", t.drop_wait, t.max_sends))
            } else {
                None
            };
            if let Some(why) = give_up {
                return give_up_drop(&j.nullifier, r, why, book, book_path);
            }
            if since < t.drop_wait {
                return Ok(DropStep::InFlight {
                    reason: format!(
                        "the gas drop for N {short}… (send {}, {}) is not applied yet; sent again {} s after its last send unless LEZ shows it applied",
                        r.sends,
                        r.tx,
                        t.drop_wait
                    ),
                });
            }
            let before = (r.tx.clone(), r.sends);
            r.sends += 1;
            r.sent_at = t.now;
            r.tx = "pending".into();
            (r, before)
        }
        None => {
            if let Some((other, rec)) = book.in_flight().map(|(k, r)| (k.clone(), r.clone())) {
                let o = &other[..18.min(other.len())];
                if t.now.saturating_sub(rec.at) < t.max_job_age.saturating_add(t.drop_wait) {
                    return Ok(DropStep::InFlight {
                        reason: format!("the gas drop for N {o}… is in flight: one drop at a time"),
                    });
                }
                // Its job is gone (refused at --max-job-age while the zone failed): it would hold every later drop
                // back. Settled by the rule its own step would have applied.
                let from = recorded_state(&rec, o)?;
                let nonce =
                    credit.drop_nonce().await?.ok_or_else(|| anyhow!("refused: this zone sends no gas drops"))?;
                let mut done = rec;
                done.settled = true;
                if nonce == from {
                    done.tx = format!("given up, nothing applied (in flight past --max-job-age); last: {}", done.tx);
                }
                book.drops.insert(other, done);
                book.save(book_path)?;
            }
            if claimed(&j.nullifier) {
                bail!("{}", DropRefusal::AlreadyClaimed);
            }
            let nonce = credit.drop_nonce().await?.ok_or_else(|| anyhow!("refused: this zone sends no gas drops"))?;
            let r = DropRecord {
                one_time_address: hx(&j.one_time_address),
                amount: t.amount,
                at: t.now,
                tx: "pending".into(),
                from_nonce: Some(nonce.to_string()),
                settled: false,
                sends: 1,
                sent_at: t.now,
            };
            (r, (String::new(), 0))
        }
    };
    let on = recorded_state(&r, short)?;
    book.record(&j.nullifier, r.clone());
    book.save(book_path)?;
    match credit.credit_on(j.one_time_address, keys, t.amount, on).await {
        Ok(Built::Sent(tx)) => {
            r.tx = tx.clone();
            book.record(&j.nullifier, r.clone());
            book.save(book_path)?;
            // Applied already (a block may have come meanwhile)? Otherwise the next rounds see it.
            if let Ok(Some(n)) = credit.drop_nonce().await {
                if n != on {
                    r.settled = true;
                    book.record(&j.nullifier, r.clone());
                    book.save(book_path)?;
                    return Ok(DropStep::Applied { tx, sends: r.sends, resolved: false });
                }
            }
            Ok(DropStep::InFlight {
                reason: format!("the gas drop for N {short}… sent ({tx}, send {}); waiting for LEZ to apply it", r.sends),
            })
        }
        // A credit of this drop sent before (built on the same state) was applied while this one was being built:
        // nothing was sent now (the second review's M1).
        Ok(Built::Moved(_)) if before.1 > 0 => {
            (r.tx, r.sends, r.settled) = (before.0, before.1, true);
            book.record(&j.nullifier, r.clone());
            book.save(book_path)?;
            Ok(DropStep::Applied { tx: r.tx, sends: r.sends, resolved: true })
        }
        // Nothing of this drop was ever sent, and the drop account moved before its first credit: recorded again on
        // the new state, sent in the next round.
        Ok(Built::Moved(n)) => {
            r.from_nonce = Some(n.to_string());
            (r.sends, r.sent_at) = (0, 0);
            r.tx = format!("not sent: the drop account moved to nonce {n} before this drop's first credit");
            book.record(&j.nullifier, r.clone());
            book.save(book_path)?;
            Ok(DropStep::InFlight {
                reason: format!(
                    "the gas drop for N {short}… was not sent: the drop account moved before its first credit (now nonce {n}); sent on that state next round"
                ),
            })
        }
        Err(e) => {
            r.tx = format!("send failed: {e:#}");
            book.record(&j.nullifier, r.clone());
            book.save(book_path)?;
            Ok(DropStep::InFlight {
                reason: format!(
                    "the gas drop's credit for N {short}… was not sent ({e:#}); sent again {} s after this attempt unless LEZ shows it applied",
                    t.drop_wait
                ),
            })
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

/// A storage proof's nodes and the slot's value.
pub type SlotProof = (Vec<Vec<u8>>, Hash);

/// The vault's account proof and the proofs of its slots 0 (the deposit root) and 1 (the leaf count)
/// from an `eth_getProof` answer, with the two values.
pub fn import_proofs(answer: &serde_json::Value) -> Result<(Vec<Vec<u8>>, SlotProof, SlotProof)> {
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
        // A malformed job is refused, not retried until --max-job-age.
        let short = checkpoint_from_journal(&[0u8; 3]).unwrap_err();
        assert!(!temporary(&format!("{short:#}")), "{short:#}");
        let mut other = checkpoint_journal(1, Address::ZERO, B256::ZERO, &checkpoint());
        other[0] ^= 1;
        assert!(!temporary(&format!("{:#}", checkpoint_from_journal(&other).unwrap_err())));
        assert!(!temporary(&format!("{:#}", unhex("0xzz").unwrap_err())));
        assert!(!temporary("refused: the job's one-time account keys (oneTimeNpk, oneTimeVpk) do not decode: …"));
        assert!(temporary("in flight: the gas drop for N 0x… sent (0x…, send 1); waiting for LEZ to apply it"));
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

    /// A zone for the journaled drop: the drop account's private nonce, and the credits sent, each built on the
    /// account's state at its send; applying one spends that state (a second credit built on it is refused, as LEZ
    /// refuses a nullifier already seen).
    #[derive(Default)]
    struct DropZone {
        nonce: u128,
        built_on: Vec<(u128, Hash)>,
        applied: Vec<Hash>,
        fail_sends: u32,
        down: bool,
    }

    impl DropZone {
        fn apply(&mut self, i: usize) -> bool {
            let (state, to) = self.built_on[i];
            if state != self.nonce {
                return false;
            }
            self.nonce = state.wrapping_mul(31).wrapping_add(7);
            self.applied.push(to);
            true
        }
    }

    impl Credit for DropZone {
        async fn credit(&mut self, to: Hash, keys: Option<&OneTimeKeys>, _amount: u128) -> Result<String> {
            assert!(keys.is_some(), "the credit is sealed to the one-time account's keys");
            if self.down || self.fail_sends > 0 {
                self.fail_sends = self.fail_sends.saturating_sub(1);
                bail!("the sealed credit was not sent: error sending request: connection closed");
            }
            self.built_on.push((self.nonce, to));
            Ok(format!("tx{}", self.built_on.len()))
        }
        async fn drop_nonce(&mut self) -> Result<Option<u128>> {
            ensure!(!self.down, "syncing the relayer's LEZ wallet: connection refused");
            Ok(Some(self.nonce))
        }
        /// As `lez::Sequencer`'s: the zone's state now (its sync), checked against `on`, the credit built on it.
        async fn credit_on(&mut self, to: Hash, keys: &OneTimeKeys, amount: u128, on: u128) -> Result<Built> {
            ensure!(!self.down, "syncing the relayer's LEZ wallet: connection refused");
            if self.nonce != on {
                return Ok(Built::Moved(self.nonce));
            }
            self.credit(to, Some(keys), amount).await.map(Built::Sent)
        }
    }

    /// A zone in which a credit sent before lands at the start of the next `credit_on`, while its sync runs: the
    /// block that comes between the step's own check of the drop account and the credit's build (the second
    /// review's M1).
    #[derive(Default)]
    struct RacyZone {
        z: DropZone,
        apply_at_next_credit: Option<usize>,
    }

    impl Credit for RacyZone {
        async fn credit(&mut self, to: Hash, keys: Option<&OneTimeKeys>, amount: u128) -> Result<String> {
            self.z.credit(to, keys, amount).await
        }
        async fn drop_nonce(&mut self) -> Result<Option<u128>> {
            self.z.drop_nonce().await
        }
        async fn credit_on(&mut self, to: Hash, keys: &OneTimeKeys, amount: u128, on: u128) -> Result<Built> {
            if let Some(i) = self.apply_at_next_credit.take() {
                assert!(self.z.apply(i), "the earlier credit applies");
            }
            self.z.credit_on(to, keys, amount, on).await
        }
    }

    fn one_time(seed: u8) -> (OneTimeKeys, GasDropJournal) {
        let vpk = lee_core::encryption::ViewingPublicKey::from_seed(&[seed; 32], &[seed ^ 0x5a; 32]);
        let keys = OneTimeKeys { npk: [seed; 32], vpk: vpk.to_bytes().to_vec() };
        let j = GasDropJournal {
            root: [1; 32],
            chain_id: [2; 32],
            vault: [3; 20],
            nullifier: sha256(&[seed]),
            one_time_address: keys.account_id().unwrap(),
            relayer: [4; 32],
        };
        (keys, j)
    }

    fn terms(now: u64) -> DropTerms {
        DropTerms { amount: 50_000_000, now, drop_wait: 60, max_sends: 5, job_age: now - 100, max_job_age: 3_600 }
    }

    fn book_path(name: &str) -> std::path::PathBuf {
        // Inside cli/target/ (ignored by git); unit tests have no CARGO_TARGET_TMPDIR.
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/unit-tmp/drop-journal").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("drops.json")
    }

    fn in_flight(s: &DropStep) -> bool {
        matches!(s, DropStep::InFlight { reason } if temporary(&format!("in flight: {reason}")))
    }

    #[tokio::test]
    async fn a_failed_credit_is_sent_again_on_the_same_state_and_applied_once() {
        let (keys, j) = one_time(7);
        let path = book_path("failed");
        let (mut book, mut zone) = (DropBook::default(), DropZone { fail_sends: 1, ..DropZone::default() });
        let none = |_: &Hash| false;
        let s = drop_step(&j, &keys, none, &mut book, &path, &mut zone, &terms(100)).await.unwrap();
        assert!(in_flight(&s), "{s:?}");
        let r = &DropBook::load(&path).unwrap().drops[&hx(&j.nullifier)];
        assert!(!r.settled && r.tx.starts_with("send failed") && r.sends == 1 && r.from_nonce == Some("0".into()));
        // Within --drop-wait nothing is sent; then it is sent again.
        let s = drop_step(&j, &keys, none, &mut book, &path, &mut zone, &terms(130)).await.unwrap();
        assert!(in_flight(&s) && zone.built_on.is_empty(), "{s:?}");
        let s = drop_step(&j, &keys, none, &mut book, &path, &mut zone, &terms(161)).await.unwrap();
        assert!(in_flight(&s) && zone.built_on.len() == 1, "{s:?}");
        assert!(zone.apply(0));
        let s = drop_step(&j, &keys, none, &mut book, &path, &mut zone, &terms(170)).await.unwrap();
        assert_eq!(s, DropStep::Applied { tx: "tx1".into(), sends: 2, resolved: true });
        assert_eq!(zone.applied, vec![j.one_time_address]);
        let again = drop_step(&j, &keys, none, &mut book, &path, &mut zone, &terms(400)).await;
        assert!(again.unwrap_err().to_string().contains("already had its drop"));
        assert_eq!(zone.built_on.len(), 1, "one credit sent after the failure, none after it applied");
    }

    #[tokio::test]
    async fn a_kill_between_the_record_and_the_send_resends_after_the_wait_and_applies_once() {
        let (keys, j) = one_time(8);
        let path = book_path("killed-unsent");
        let mut zone = DropZone { nonce: 41, ..DropZone::default() };
        // What a daemon killed inside the credit's send leaves: the record, nothing sent.
        let mut left = DropBook::default();
        left.record(
            &j.nullifier,
            DropRecord {
                one_time_address: hx(&j.one_time_address),
                amount: 50_000_000,
                at: 100,
                tx: "pending".into(),
                from_nonce: Some("41".into()),
                settled: false,
                sends: 1,
                sent_at: 100,
            },
        );
        left.save(&path).unwrap();
        let mut book = DropBook::load(&path).unwrap();
        let none = |_: &Hash| false;
        let s = drop_step(&j, &keys, none, &mut book, &path, &mut zone, &terms(120)).await.unwrap();
        assert!(in_flight(&s) && zone.built_on.is_empty(), "{s:?}");
        let s = drop_step(&j, &keys, none, &mut book, &path, &mut zone, &terms(161)).await.unwrap();
        assert!(in_flight(&s) && zone.built_on == vec![(41, j.one_time_address)], "{s:?}");
        assert!(zone.apply(0));
        let s = drop_step(&j, &keys, none, &mut book, &path, &mut zone, &terms(162)).await.unwrap();
        assert_eq!(s, DropStep::Applied { tx: "tx1".into(), sends: 2, resolved: true });
        assert_eq!(zone.applied.len(), 1);
    }

    #[tokio::test]
    async fn a_kill_after_the_send_books_the_credit_and_two_sends_apply_once() {
        let (keys, j) = one_time(9);
        let path = book_path("killed-sent");
        let mut zone = DropZone::default();
        let none = |_: &Hash| false;
        let mut book = DropBook::default();
        let s = drop_step(&j, &keys, none, &mut book, &path, &mut zone, &terms(100)).await.unwrap();
        assert!(in_flight(&s) && zone.built_on.len() == 1, "{s:?}");
        // Killed; the credit applied meanwhile; restarted from the book on disk: booked, nothing sent again.
        assert!(zone.apply(0));
        let mut book = DropBook::load(&path).unwrap();
        let s = drop_step(&j, &keys, none, &mut book, &path, &mut zone, &terms(500)).await.unwrap();
        assert_eq!(s, DropStep::Applied { tx: "tx1".into(), sends: 1, resolved: true });
        assert_eq!(zone.built_on.len(), 1);
        // A credit sent, not applied in time, sent again: both built on the same state, LEZ applies one.
        let (keys2, j2) = one_time(10);
        let s = drop_step(&j2, &keys2, none, &mut book, &path, &mut zone, &terms(600)).await.unwrap();
        assert!(in_flight(&s));
        let s = drop_step(&j2, &keys2, none, &mut book, &path, &mut zone, &terms(661)).await.unwrap();
        assert!(in_flight(&s) && zone.built_on.len() == 3);
        assert_eq!(zone.built_on[1].0, zone.built_on[2].0, "the second send is built on the first one's state");
        assert!(zone.apply(1));
        assert!(!zone.apply(2), "the second credit spends a state the first spent: refused");
        let s = drop_step(&j2, &keys2, none, &mut book, &path, &mut zone, &terms(662)).await.unwrap();
        assert_eq!(s, DropStep::Applied { tx: "tx3".into(), sends: 2, resolved: true });
        assert_eq!(zone.applied, vec![j.one_time_address, j2.one_time_address], "one credit per N");
    }

    /// The second review's race (its model test, turned into the regression): the first credit lands
    /// while the credit sent again syncs, after the step's own check found the drop account unmoved. Before the fix
    /// the second credit was built on the state the first had made, and both applied.
    #[tokio::test]
    async fn a_credit_landing_while_it_is_sent_again_is_found_and_nothing_more_is_built() {
        let (keys, j) = one_time(21);
        let path = book_path("race");
        let (mut book, mut zone) = (DropBook::default(), RacyZone::default());
        let none = |_: &Hash| false;
        let s = drop_step(&j, &keys, none, &mut book, &path, &mut zone, &terms(100)).await.unwrap();
        assert!(in_flight(&s), "{s:?}");
        // --drop-wait has passed with the first credit not applied; it lands while the resend's credit syncs.
        zone.apply_at_next_credit = Some(0);
        let s = drop_step(&j, &keys, none, &mut book, &path, &mut zone, &terms(161)).await.unwrap();
        assert_eq!(s, DropStep::Applied { tx: "tx1".into(), sends: 1, resolved: true });
        let states: Vec<u128> = zone.z.built_on.iter().map(|(n, _)| *n).collect();
        assert_eq!(states, vec![0], "one credit built, on the recorded state; none on the state the first made");
        assert_eq!(zone.z.applied, vec![j.one_time_address], "one credit applied for this N");
        let r = &DropBook::load(&path).unwrap().drops[&hx(&j.nullifier)];
        assert!(r.settled && r.sends == 1 && r.tx == "tx1", "{r:?}");
    }

    /// Before its first credit the drop account moved (a credit a sequencer held for a drop given up earlier lands):
    /// nothing is built on the old state; the drop is recorded on the new one and sent next round.
    #[tokio::test]
    async fn a_drop_account_moved_before_the_first_credit_is_recorded_again_and_sent_on_the_new_state() {
        let (k1, j1) = one_time(22);
        let (k2, j2) = one_time(23);
        let path = book_path("moved-first");
        let (mut book, mut zone) = (DropBook::default(), RacyZone::default());
        let none = |_: &Hash| false;
        // A drop for N1 sent, then given up (its deposit claimed without it) with its credit still held somewhere.
        assert!(in_flight(&drop_step(&j1, &k1, none, &mut book, &path, &mut zone, &terms(100)).await.unwrap()));
        let n1 = j1.nullifier;
        let claimed = move |n: &Hash| *n == n1;
        let s = drop_step(&j1, &k1, claimed, &mut book, &path, &mut zone, &terms(161)).await.unwrap();
        assert!(matches!(&s, DropStep::GivenUp { why, sends: 1 } if why.contains("claimed without it")), "{s:?}");
        // N2's first credit: the held credit of N1 lands during its sync.
        zone.apply_at_next_credit = Some(0);
        let s = drop_step(&j2, &k2, none, &mut book, &path, &mut zone, &terms(200)).await.unwrap();
        assert!(matches!(&s, DropStep::InFlight { reason } if reason.contains("moved before its first credit")), "{s:?}");
        assert_eq!(zone.z.built_on.len(), 1, "nothing built for N2 on the state N1's credit spent");
        let r = DropBook::load(&path).unwrap().drops[&hx(&j2.nullifier)].clone();
        assert!(!r.settled && r.sends == 0 && r.from_nonce == Some(zone.z.nonce.to_string()), "{r:?}");
        // Next round: built on the new state, applied once.
        let s = drop_step(&j2, &k2, none, &mut book, &path, &mut zone, &terms(201)).await.unwrap();
        assert!(in_flight(&s) && zone.z.built_on.len() == 2 && zone.z.built_on[1].0 == zone.z.nonce, "{s:?}");
        assert!(zone.z.apply(1));
        let s = drop_step(&j2, &k2, none, &mut book, &path, &mut zone, &terms(202)).await.unwrap();
        assert_eq!(s, DropStep::Applied { tx: "tx2".into(), sends: 1, resolved: true });
        assert_eq!(zone.z.applied, vec![j1.one_time_address, j2.one_time_address]);
    }

    #[tokio::test]
    async fn one_drop_in_flight_at_a_time_and_a_drop_not_needed_is_given_up() {
        let (k1, j1) = one_time(11);
        let (k2, j2) = one_time(12);
        let path = book_path("one-at-a-time");
        let (mut book, mut zone) = (DropBook::default(), DropZone::default());
        let none = |_: &Hash| false;
        assert!(in_flight(&drop_step(&j1, &k1, none, &mut book, &path, &mut zone, &terms(100)).await.unwrap()));
        let s = drop_step(&j2, &k2, none, &mut book, &path, &mut zone, &terms(101)).await.unwrap();
        assert!(matches!(&s, DropStep::InFlight { reason } if reason.contains("one drop at a time")), "{s:?}");
        assert_eq!(zone.built_on.len(), 1, "nothing sent for the second N while the first is in flight");
        // N1 claimed without this relayer's credit (which never applied): given up once --drop-wait has passed.
        let n1 = j1.nullifier;
        let claimed = move |n: &Hash| *n == n1;
        assert!(in_flight(&drop_step(&j1, &k1, claimed, &mut book, &path, &mut zone, &terms(130)).await.unwrap()));
        let s = drop_step(&j1, &k1, claimed, &mut book, &path, &mut zone, &terms(161)).await.unwrap();
        assert!(matches!(&s, DropStep::GivenUp { why, sends: 1 } if why.contains("claimed without it")), "{s:?}");
        assert!(book.in_flight().is_none());
        // N2 now goes; past --max-job-age with nothing applied it is given up too.
        assert!(in_flight(&drop_step(&j2, &k2, none, &mut book, &path, &mut zone, &terms(170)).await.unwrap()));
        let late = DropTerms { job_age: 3_600, ..terms(240) };
        let s = drop_step(&j2, &k2, none, &mut book, &path, &mut zone, &late).await.unwrap();
        assert!(matches!(&s, DropStep::GivenUp { why, .. } if why.contains("--max-job-age")), "{s:?}");
        assert!(zone.applied.is_empty() && book.in_flight().is_none());
        let again = drop_step(&j2, &k2, none, &mut book, &path, &mut zone, &terms(300)).await;
        assert!(again.unwrap_err().to_string().contains("already had its drop"));
    }

    /// The second review's L4: a drop whose credit keeps failing is given up `--drop-wait` after its last allowed
    /// send, and the next drop goes.
    #[tokio::test]
    async fn a_drop_failing_every_send_is_given_up_after_its_last_send_and_the_next_drop_goes() {
        let (k1, j1) = one_time(24);
        let (k2, j2) = one_time(25);
        let path = book_path("cap");
        let (mut book, mut zone) = (DropBook::default(), DropZone { fail_sends: 100, ..DropZone::default() });
        let none = |_: &Hash| false;
        let mut now = 100;
        for _ in 0..5 {
            assert!(in_flight(&drop_step(&j1, &k1, none, &mut book, &path, &mut zone, &terms(now)).await.unwrap()));
            now += 61;
        }
        assert_eq!(zone.fail_sends, 95, "five sends of N1");
        let s = drop_step(&j1, &k1, none, &mut book, &path, &mut zone, &terms(now)).await.unwrap();
        assert!(matches!(&s, DropStep::GivenUp { why, sends: 5 } if why.contains("--drop-max-sends")), "{s:?}");
        zone.fail_sends = 0;
        assert!(in_flight(&drop_step(&j2, &k2, none, &mut book, &path, &mut zone, &terms(now)).await.unwrap()));
        assert!(zone.apply(0));
        let s = drop_step(&j2, &k2, none, &mut book, &path, &mut zone, &terms(now + 1)).await.unwrap();
        assert_eq!(s, DropStep::Applied { tx: "tx1".into(), sends: 1, resolved: true });
        assert_eq!(zone.applied, vec![j2.one_time_address]);
    }

    /// A drop left in flight with its job gone (refused at --max-job-age while the zone failed) is settled by the next
    /// drop once it is older than --max-job-age and --drop-wait, instead of holding every later drop back.
    #[tokio::test]
    async fn a_drop_left_in_flight_past_the_job_age_does_not_hold_later_drops_back() {
        let (k1, j1) = one_time(26);
        let (k2, j2) = one_time(27);
        let path = book_path("orphan");
        let (mut book, mut zone) = (DropBook::default(), DropZone { fail_sends: 1, ..DropZone::default() });
        let none = |_: &Hash| false;
        assert!(in_flight(&drop_step(&j1, &k1, none, &mut book, &path, &mut zone, &terms(100)).await.unwrap()));
        let s = drop_step(&j2, &k2, none, &mut book, &path, &mut zone, &terms(3_000)).await.unwrap();
        assert!(matches!(&s, DropStep::InFlight { reason } if reason.contains("one drop at a time")), "{s:?}");
        assert!(in_flight(&drop_step(&j2, &k2, none, &mut book, &path, &mut zone, &terms(3_761)).await.unwrap()));
        let r1 = &book.drops[&hx(&j1.nullifier)];
        assert!(r1.settled && r1.tx.starts_with("given up, nothing applied (in flight past"), "{r1:?}");
        assert_eq!(zone.built_on, vec![(0, j2.one_time_address)]);
    }

    #[tokio::test]
    async fn a_zone_that_fails_while_a_drop_is_resolved_loses_nothing() {
        let (keys, j) = one_time(13);
        let path = book_path("zone-down");
        let (mut book, mut zone) = (DropBook::default(), DropZone::default());
        let none = |_: &Hash| false;
        assert!(in_flight(&drop_step(&j, &keys, none, &mut book, &path, &mut zone, &terms(100)).await.unwrap()));
        zone.down = true;
        let e = drop_step(&j, &keys, none, &mut book, &path, &mut zone, &terms(200)).await.unwrap_err();
        assert!(temporary(&format!("{e:#}")), "the zone's failure is retried: {e:#}");
        assert_eq!(DropBook::load(&path).unwrap().drops[&hx(&j.nullifier)].sends, 1, "the record unchanged");
        zone.down = false;
        assert!(zone.apply(0));
        let s = drop_step(&j, &keys, none, &mut book, &path, &mut zone, &terms(201)).await.unwrap();
        assert_eq!(s, DropStep::Applied { tx: "tx1".into(), sends: 1, resolved: true });
        // A book written before the journal: its drops are settled (one per N, as before).
        let old: DropBook = serde_json::from_str(
            r#"{"drops":{"0xaa":{"one_time_address":"0xbb","amount":5,"at":1,"tx":"failed: x"}}}"#,
        )
        .unwrap();
        assert!(old.drops["0xaa"].settled && old.in_flight().is_none());
    }

    /// The second review's L6: a viewing key in hex that is not a key is the job's fault: refused, not retried.
    #[tokio::test]
    async fn a_viewing_key_that_does_not_decode_is_refused() {
        let (mut keys, j) = one_time(28);
        keys.vpk = vec![0xff; 7];
        let path = book_path("bad-vpk");
        let (mut book, mut zone) = (DropBook::default(), DropZone::default());
        let e = drop_step(&j, &keys, |_: &Hash| false, &mut book, &path, &mut zone, &terms(100)).await.unwrap_err();
        assert!(!temporary(&format!("{e:#}")) && format!("{e:#}").contains("viewing key"), "{e:#}");
        assert!(book.drops.is_empty() && zone.built_on.is_empty());
    }
}
