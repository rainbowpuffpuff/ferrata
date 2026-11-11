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
}

impl Relayer {
    /// Reads the vault's chain, configuration, final state and the latest block's time.
    pub async fn view(&self, p: &DynProvider) -> Result<VaultView> {
        let chain_id = p.get_chain_id().await?;
        let cid = eth::view(p, self.vault, IVault::lezConfigIdCall {}, None).await?;
        let final_state = eth::view(p, self.vault, IRelayVault::finalStateCall {}, None).await?;
        let now = p.get_block(BlockId::latest()).await?.map(|b| b.header.timestamp).unwrap_or(0);
        Ok(VaultView { chain_id, cid, final_state, now })
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
            !eth::view(p, self.vault, IVault::nullifierUsedCall { nullifier }, None).await?,
            "refused: NOut {nullifier} is already released"
        );
        let token = Address::from(j.token);
        let amount = U256::from_be_bytes(j.amount);
        let cfg = eth::view(p, self.vault, IVault::tokenConfigCall { token }, None).await?;
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
        let cand = eth::view(p, self.vault, IRelayVault::candidatesCall { newState: c.newState }, None).await?;
        ensure!(cand.submittedAt.is_zero(), "skipped: {} is already a candidate", c.newState);
        Ok(IRelayVault::submitCandidateCall { c: c.clone(), seal }.abi_encode().into())
    }

    /// Live candidates on the vault (any submitter's), rebuilt from their `CandidateSubmitted` events
    /// and the vault's committee floors, each checked against the journal digest the vault stored.
    pub async fn candidates(&self, p: &DynProvider, from_block: u64) -> Result<Vec<(IRelayVault::Checkpoint, u64)>> {
        let filter = Filter::new()
            .address(self.vault)
            .event_signature(IRelayVault::CandidateSubmitted::SIGNATURE_HASH)
            .from_block(from_block);
        let logs = p.get_logs(&filter).await?;
        let n_min = eth::view(p, self.vault, IRelayVault::nMinCall {}, None).await?;
        let t_min = eth::view(p, self.vault, IRelayVault::tMinCall {}, None).await?;
        let chain_id = p.get_chain_id().await?;
        let cid = eth::view(p, self.vault, IVault::lezConfigIdCall {}, None).await?;
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
            let cand = eth::view(p, self.vault, IRelayVault::candidatesCall { newState: c.newState }, None).await?;
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

/// Sends the native credit of a gas drop.
#[allow(async_fn_in_trait)]
pub trait Credit {
    /// Credits `amount` native LEZ to `to` from the relayer's account; returns the transaction (hex).
    async fn credit(&mut self, to: Hash, amount: u128) -> Result<String>;
}

/// A sequencer endpoint through LEZ's wallet: a native transfer from the relayer's public account to
/// the one-time address (an asynchronous credit the claim sweeps).
impl Credit for crate::lez::Sequencer {
    async fn credit(&mut self, to: Hash, amount: u128) -> Result<String> {
        use wallet::program_facades::{native_token_transfer::NativeTokenTransfer, CreditDelivery};
        let payer = self.payer.ok_or_else(|| anyhow!("no LEZ payer account for gas drops"))?;
        let (hash, _) = NativeTokenTransfer(&self.wallet)
            .transfer(
                wallet::AccountIdentity::Public(payer),
                wallet::AccountIdentity::PublicNoSign(lee::AccountId::new(to)),
                amount,
                CreditDelivery::Automatic,
            )
            .await
            .map_err(|e| anyhow!("the native transfer failed: {e:?}"))?;
        Ok(format!("{hash:?}"))
    }
}

/// Checks a gas-drop request and, if it passes, sends the credit and records it (one per `N`).
pub async fn gas_drop<C: Credit>(
    receipt_json: &[u8],
    ctx: &DropContext<'_>,
    claimed: impl Fn(&Hash) -> bool,
    book: &mut DropBook,
    book_path: &Path,
    credit: &mut C,
    amount: u128,
    now: u64,
) -> Result<(GasDropJournal, String)> {
    let j = check_gas_drop(receipt_json, ctx, claimed, book).map_err(|r| anyhow!("{r}"))?;
    let tx = credit.credit(j.one_time_address, amount).await?;
    book.record(
        &j.nullifier,
        DropRecord { one_time_address: hx(&j.one_time_address), amount, at: now, tx: tx.clone() },
    );
    book.save(book_path)?;
    Ok((j, tx))
}

/// A job file in the relayer's inbox (`<inbox>/*.json`), handed over by a user who opts in or by
/// the checkpoint prover. Nothing in it is secret.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Job {
    /// A release: the release journal and the Groth16 seal (hex).
    Release { journal: String, seal: String },
    /// A rule (B) checkpoint: the checkpoint journal and the Groth16 seal (hex).
    Checkpoint { journal: String, seal: String },
    /// A gas drop: the receipt (JSON).
    GasDrop { receipt: serde_json::Value },
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
