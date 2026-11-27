//! `ferrata-relayer`: the relayer daemon for the Ferrata bridge (prototype, unaudited). The binary
//! (`src/bin/relayer.rs`) and `ferrata_relayer_module` run this code: the same arguments, rounds and
//! JSON lines; the binary prints the lines and stops on SIGTERM or SIGINT, the module keeps them and
//! stops when it is asked to.
//!
//! One loop, every `--poll-secs`:
//! 0. **Root imports** (with `--import-roots` and a zone), on a fixed cadence (bridge.md §5.4): in the first
//!    round of each `--import-every` period of Ethereum time, the newest record of the configuration's
//!    light-client instance on LEZ, the vault's slots 0 and 1 at its block (`eth_getProof`), and, when it
//!    covers deposits LEZ has not imported, `ImportRoot`, built and dry-run with `ferrata_wallet::import`
//!    ([`relayer::plan_root_import`]) and sent; the import is confirmed from `root(cid, n)`. A period whose
//!    attempt waits (no newer record yet) or fails is tried again in its next rounds; at most one import is
//!    sent per period, whenever the deposits came (the period done is kept in `<state>/root-import.json`).
//! 1. **Inbox.** Each `<inbox>/*.json` job ([`relayer::Job`]) handed over by a user who opts in, or by
//!    the checkpoint prover: a release (journal and seal), a rule (B) checkpoint (journal and seal) or
//!    a gas drop (receipt). Checked ([`relayer`]), then sent; the file moves to `<state>/done/` or
//!    `<state>/refused/`, and one JSON line says what happened and why.
//! 2. **Candidates on the vault.** Every live candidate (anyone's, rebuilt from its event and checked
//!    against the digest the vault stored) whose wait is over is finalised with `finalizeAttested`.
//! 3. **Fees.** Whatever the vault credited this relayer (checkpoint fees; relay fees whose push
//!    failed) is withdrawn with `claimOwed`.
//! 4. **Burns on LEZ** (with a zone): the configuration's burns past the newest accepted burn
//!    count; if there are any and no live candidate, `<state>/needs-checkpoint.json` tells the
//!    checkpoint prover (a separate process: `e2e/crown`, stage 7) to cut one.
//!
//! **Exactly once across kills and lost connections** (RFP-022 R.06). The daemon sends one Ethereum transaction
//! at a time. Before it is sent, `<state>/inflight.json` names it (what it does, the inbox job it books, its
//! calldata and the nonce it takes), and its hash once the node has it; the record goes only once the effect is
//! booked (the job moved to `done/`, the line written). Every round first resolves a record left by a killed or
//! cut-off daemon: mined, it books what the transaction did (`"recovered": true`); still pending, the round waits;
//! lost (not mined and unknown to the node, its nonce unspent), the same call is sent again **with the same
//! nonce**, so at most one of the two can ever be mined. Through `tx_sender_module` (`--send-via logos`) the record
//! carries the request's id; a killed daemon drives its open request to its end, and what the module cannot account
//! for (a request it no longer holds after a restart, `stuck`, a leg that failed after it began to leave) is settled
//! on the chain the same way, at the request's nonce (the module's own, from `send_status` or its durable history);
//! lost, it is sent again through the module with that nonce pinned. One daemon per state directory (`<state>/lock`). A root
//! import sent to LEZ is kept the same way (`<state>/root-import-inflight.json`) until `root(cid, n)` holds it. A gas
//! drop is journaled in `<state>/drops.json` ([`relayer::drop_step`]): recorded with the private nonce of the account
//! the credits come from before its credit is sent, one drop at a time; its job stays in the inbox until LEZ shows
//! that nonce moved (the credit applied); a credit not applied within `--drop-wait` (its send failed, a kill left it
//! unsent, LEZ dropped it) is sent again, built only on the recorded state of that account (a credit that lands while
//! the next is built is found, and nothing is sent), so LEZ applies at most one of them; after `--drop-max-sends`
//! credits, or once the deposit is claimed without it, the drop is given up (`gas-drop-given-up`).
//!
//! Keys: an Ethereum keystore file (`--keystore`, `--password-file`), or, on a local anvil only,
//! `--unlocked` (the node signs). The LEZ side: a sequencer through LEZ's wallet with the relayer's own
//! seed (`--sequencer`, feature `lez-wallet`), or the mini-app devnet's zone (`--lez-devnet`, a stand-in
//! for tests: the devnet signs and settles). The relayer holds no user secret: see `cli/src/relayer.rs`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use alloy::network::TransactionBuilder as _;
use alloy::primitives::{Address, Bytes, B256};
use alloy::providers::{DynProvider, Provider};
use alloy::rpc::types::{TransactionReceipt, TransactionRequest};
use alloy::sol_types::SolCall;
use anyhow::{anyhow, bail, ensure, Context as _, Result};
use clap::{Parser, Subcommand};
use ferrata_lez_bridge_wire::BridgeState;
use serde_json::{json, Value};
use sha2::Digest as _;

use crate::eth::{self, IVault};
use crate::lez::{self, Context, Ledger as _, Zone};
use crate::relayer::{self, DropBook, DropContext, DropStep, DropTerms, IRelayVault, Job, Relayer, RootImport};
use crate::{account, h32, hx};

#[derive(Parser, Clone, Debug)]
#[command(name = "ferrata-relayer", version, about = "The Ferrata relayer daemon (prototype, unaudited)")]
pub struct Cli {
    /// Any Ethereum JSON-RPC endpoint.
    #[arg(long, env = "FERRATA_ETH_RPC", global = true)]
    pub eth_rpc: Option<String>,
    /// The vault's address.
    #[arg(long, env = "FERRATA_VAULT", global = true)]
    pub vault: Option<Address>,
    /// Refuse to run unless the endpoint serves this chain id.
    #[arg(long, env = "FERRATA_CHAIN_ID", global = true)]
    pub chain_id: Option<u64>,
    /// The relayer's state directory (drop book, candidates, done and refused jobs).
    #[arg(long, env = "FERRATA_RELAYER_STATE", default_value = "relayer-state", global = true)]
    pub state: PathBuf,
    /// An Ethereum keystore file (Web3 Secret Storage) for the relayer's account.
    #[arg(long, env = "FERRATA_KEYSTORE", global = true)]
    pub keystore: Option<PathBuf>,
    /// A file holding the keystore's password.
    #[arg(long, env = "FERRATA_PASSWORD_FILE", global = true)]
    pub password_file: Option<PathBuf>,
    /// Local anvil only: send from this address and let the node sign (no key here).
    #[arg(long, global = true)]
    pub unlocked: Option<Address>,
    /// Submit releases whose proof pays no relay fee.
    #[arg(long, global = true)]
    pub accept_unpaid: bool,
    /// First block of the vault's events (its deployment block).
    #[arg(long, env = "FERRATA_FROM_BLOCK", default_value_t = 0, global = true)]
    pub from_block: u64,
    /// The LEZ sequencer's RPC endpoint (root imports, gas drops and the burn watch).
    #[arg(long, env = "FERRATA_SEQUENCER", global = true)]
    pub sequencer: Option<String>,
    /// The mini-app devnet's zone (`cli/examples/miniapp_devnet.rs`, `http://127.0.0.1:PORT/rpc`) in place of a
    /// sequencer: a stand-in for tests (reads and calls over the devnet's HTTP methods; the devnet signs and
    /// settles them in its LEZ harness). Loopback addresses only.
    #[arg(long, env = "FERRATA_LEZ_DEVNET", global = true)]
    pub lez_devnet: Option<String>,
    /// The relayer's own seed file (its LEZ wallet's mnemonic).
    #[arg(long, env = "FERRATA_SEED", global = true)]
    pub seed: Option<PathBuf>,
    /// The LEZ wallet's home.
    #[arg(long, env = "FERRATA_LEZ_WALLET", global = true)]
    pub lez_wallet: Option<PathBuf>,
    /// A file holding the LEZ wallet's password.
    #[arg(long, env = "FERRATA_LEZ_PASSWORD_FILE", global = true)]
    pub lez_password_file: Option<PathBuf>,
    /// The bridge program's account id on LEZ (32-byte hex).
    #[arg(long, env = "FERRATA_BRIDGE_ACCOUNT", global = true)]
    pub bridge_account: Option<String>,
    /// The relayer's LEZ public account (32-byte hex): pays the fees of its public transactions (root
    /// imports), named in gas-drop receipts and paid the relay fee by the claims.
    #[arg(long, env = "FERRATA_RELAYER_LEZ_ACCOUNT", global = true)]
    pub lez_account: Option<String>,
    /// The private account of the relayer's seed that gas drops are sent from, sealed to each claimant's
    /// one-time account (default: private account /0).
    #[arg(long, default_value_t = 0, global = true)]
    pub drop_from_private: u32,
    /// The gas-drop guest's image id (32-byte hex).
    #[arg(long, env = "FERRATA_GASDROP_IMAGE_ID", global = true)]
    pub gasdrop_image_id: Option<String>,
    /// Native LEZ sent per gas drop.
    #[arg(long, default_value_t = 0, global = true)]
    pub drop_amount: u128,
    /// Seconds a gas drop's credit sent to LEZ may stay unapplied before it is sent again. It is sent again only on the
    /// state of the drop account the relayer's book recorded before the first send (a credit that lands meanwhile is
    /// found after the sync, and nothing is sent), so LEZ applies at most one of the credits of a drop.
    #[arg(long, default_value_t = 120, global = true)]
    pub drop_wait: u64,
    /// Credits sent for one gas drop at most: still not applied `--drop-wait` after the last of them, the drop is given
    /// up (`gas-drop-given-up`) and the next drops go (one drop at a time).
    #[arg(long, default_value_t = 5, global = true)]
    pub drop_max_sends: u32,
    /// Accept RISC Zero dev-mode (fake) gas-drop receipts: a local devnet whose proofs are fake too. Without it, a
    /// daemon started with RISC0_DEV_MODE set refuses to run gas drops (a fake receipt would be paid a real drop).
    #[arg(long, global = true)]
    pub accept_dev_mode_receipts: bool,
    /// Gas drops only (a network's published gas-drop relayer, `deploy/testnet/host/systemd/ferrata-gasdrop.service`):
    /// no Ethereum key and nothing sent to Ethereum (the vault is only read: its deposits, for the imported roots a
    /// receipt is checked against); every other job is refused; no root imports, finalisations or fee collection.
    /// Needs --sequencer, --gasdrop-image-id and --lez-account.
    #[arg(long, global = true)]
    pub gas_drops_only: bool,
    /// Import the vault's deposit roots on LEZ: each round, the bridge program's `ImportRoot` under the newest
    /// record of the configuration's light-client instance, when that record covers deposits LEZ has not
    /// imported (built and dry-run with ferrata-wallet's `import`, the code ferrata_bridge_module's
    /// `import_root` runs). Needs a zone (--sequencer or --lez-devnet) and --bridge-account.
    #[arg(long, global = true)]
    pub import_roots: bool,
    /// The root imports' period in seconds of Ethereum time: at most one import per period, tried from the first
    /// round of each (a fixed cadence, never in response to a deposit). 0: every round (tests).
    #[arg(long, default_value_t = 3600, global = true)]
    pub import_every: u64,
    /// The Groth16 prover, run as `<command> <input.json> <proof.json>` (RISC Zero's prover image: Docker on
    /// x86, or a remote prover); with it the relayer wraps holders' succinct release receipts.
    #[arg(long, env = "FERRATA_WRAP_COMMAND", global = true)]
    pub wrap_command: Option<PathBuf>,
    /// Seconds a job may wait in the inbox for a temporary condition (root not final, delay, freeze, cap).
    #[arg(long, default_value_t = 7 * 24 * 3600, global = true)]
    pub max_job_age: u64,
    /// Seconds to wait for a sent transaction's receipt within a round. Past it the transaction stays in flight
    /// (`<state>/inflight.json`): the next rounds wait for it, or send the same call again with the same nonce
    /// once the node has lost it.
    #[arg(long, default_value_t = 180, global = true)]
    pub receipt_timeout: u64,
    /// Seconds a root import sent to LEZ may stay unapplied before it is planned again.
    #[arg(long, default_value_t = 600, global = true)]
    pub import_wait: u64,
    /// Seconds the daemon waits at start for its Ethereum endpoint to answer (an endpoint down at start is
    /// waited for, not a reason to stop).
    #[arg(long, default_value_t = 600, global = true)]
    pub startup_wait: u64,
    /// Where the relayer's Ethereum transactions go: `plain` (signed here, or by a local anvil with --unlocked, and
    /// sent over --eth-rpc; the default) or `logos` (Logos Core's `tx_sender_module`, signed by `keystore_module`
    /// through `signer_manager_module`; only when the relayer runs as `ferrata_relayer_module`). With `logos` and a
    /// key of its own as well, a transaction `tx_sender_module` did not broadcast goes the plain way (the fallback).
    #[arg(long, default_value = "plain", value_parser = ["plain", "logos"], global = true)]
    pub send_via: String,
    /// With --send-via logos: the keystore account that sends (the relayer's address; its key's address when it
    /// has one here, which must then be the same).
    #[arg(long, global = true)]
    pub from: Option<Address>,
    /// With --send-via logos: seconds a transaction may wait for its approval before it is withdrawn.
    #[arg(long, default_value_t = 30, global = true)]
    pub approval_timeout: u64,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Clone, Debug)]
pub enum Command {
    /// The daemon.
    Run {
        /// The job inbox.
        #[arg(long, default_value = "relayer-inbox")]
        inbox: PathBuf,
        /// Seconds between rounds.
        #[arg(long, default_value_t = 12)]
        poll_secs: u64,
        /// Stop after this many rounds (0: run until stopped).
        #[arg(long, default_value_t = 0)]
        rounds: u64,
    },
    /// The vault's final state, live candidates, this relayer's balance and the drop book.
    Status,
    /// One protocol-fee collection (RFP-022 F.13, ADR 0048): the bridge program's `CollectFees` of `--token` under the
    /// newest record of the configuration's instance, built and dry-run as the root imports are, then sent from the
    /// relayer's LEZ account (anyone may send it: it mints only to the configuration's fee recipient, what the vault
    /// escrowed for it and LEZ has not minted yet). Needs a zone (--sequencer or --lez-devnet) and --bridge-account;
    /// refused on a zone that runs B1's program, which has no `CollectFees`.
    CollectFees {
        /// The token (ETH: the zero address).
        #[arg(long, default_value = "0x0000000000000000000000000000000000000000")]
        token: Address,
        /// Mint at most this many base units (default: all that is due), e.g. to stay within the token's mint bucket; a
        /// whole number of the token's LEZ unit.
        #[arg(long)]
        limit: Option<u128>,
        /// The record to read the vault under (default: the instance's newest): with an inbound depth, a record at
        /// least that many slots behind the newest one the configuration's clock has read.
        #[arg(long)]
        record_slot: Option<u64>,
    },
}

/// Where the daemon's JSON lines go: the binary prints each on standard output, the module keeps them.
pub type Sink = Arc<dyn Fn(Value) + Send + Sync>;

/// One JSON line: the time, the event, its fields.
pub fn line(event: &str, fields: Value) -> Value {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut line = json!({ "t": t, "event": event });
    if let (Some(obj), Some(extra)) = (line.as_object_mut(), fields.as_object()) {
        obj.extend(extra.clone());
    }
    line
}

fn log(sink: &Sink, event: &str, fields: Value) {
    sink(line(event, fields));
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Seconds since `file` was written (an inbox job's age).
fn file_age(file: &Path) -> u64 {
    std::fs::metadata(file).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map_or(0, |d| d.as_secs())
}

/// Writes `bytes` to `path` through a temporary file beside it, synced before the rename: after a crash the
/// path holds the old or the new content, never a part.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write as _;
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)?;
    if let Some(dir) = path.parent() {
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
    }
    Ok(())
}

/// The Ethereum transaction in flight (one at most): written before it is sent, removed once its effect is
/// booked (module documentation).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Inflight {
    /// What it does: `submitCandidate:<newState>`, `finalizeAttested:<newState>`, `release:<NOut>`,
    /// `claimOwed:<token>`.
    pub key: String,
    pub label: String,
    /// The inbox job it books (its file name), if it came from the inbox.
    pub job: Option<String>,
    /// Its calldata to the vault.
    pub input: Bytes,
    /// The nonce it takes; a lost transaction is sent again with it. Through `tx_sender_module`: the number its request
    /// took, once the module said it (`send_status`'s leg, or its history row); before that, the account's next.
    pub nonce: u64,
    /// Its hash, once the node has answered the send.
    pub hash: Option<B256>,
    /// When the record was written (Unix seconds).
    pub at: u64,
    /// The node's head when it was written: the transaction is in a later block.
    #[serde(default)]
    pub from_block: u64,
    /// `logos`: sent through `tx_sender_module` (`--send-via logos`); absent: the plain path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    /// `tx_sender_module`'s request id, once its `send` answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
}

/// [`Inflight::via`] for the route through `tx_sender_module`.
const VIA_LOGOS: &str = "logos";

impl Inflight {
    pub fn path(state: &Path) -> PathBuf {
        state.join("inflight.json")
    }

    pub fn load(state: &Path) -> Result<Option<Self>> {
        match std::fs::read(Self::path(state)) {
            Ok(b) => Ok(Some(serde_json::from_slice(&b).context("inflight.json")?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn save(&self, state: &Path) -> Result<()> {
        write_atomic(&Self::path(state), &serde_json::to_vec_pretty(self)?)
    }

    fn clear(state: &Path) -> Result<()> {
        match std::fs::remove_file(Self::path(state)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

/// The block of the relayer's last mined transaction and the nonce after it (`<state>/fence.json`): a round reads
/// at a block at or above it, and a nonce below it is never used again, whatever a lagging node answers.
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Fence {
    pub block: u64,
    pub next_nonce: u64,
}

impl Fence {
    fn load(state: &Path) -> Self {
        std::fs::read(state.join("fence.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
}

/// A root import sent to LEZ, until `root(cid, n)` holds what its dry run said it writes.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct ImportInflight {
    leaf_count: u64,
    /// `borsh(RootState)` the import writes.
    writes: Bytes,
    tx: String,
    at: u64,
}

struct Daemon {
    p: DynProvider,
    r: Relayer,
    state: PathBuf,
    from_block: u64,
    zone: Option<(Zone, Context)>,
    gasdrop_image_id: Option<[u8; 32]>,
    lez_me: Option<[u8; 32]>,
    drop_amount: u128,
    /// `--drop-wait`, `--drop-max-sends`.
    drop_wait: u64,
    drop_max_sends: u32,
    /// Tokens whose credited fees the relayer collects (ETH, and every token of a release it sent).
    tokens: std::collections::BTreeSet<Address>,
    /// The vault runs rule (B) (`mode() == 1`): candidates are finalised with `finalizeAttested`.
    rule_b: bool,
    /// `--gas-drops-only`.
    gas_drops_only: bool,
    /// A job deferred for longer than this (seconds) is refused.
    max_job_age: u64,
    /// The Groth16 prover command (wrapping), if any.
    wrap_command: Option<PathBuf>,
    /// `--receipt-timeout`, `--import-wait`.
    receipt_timeout: u64,
    import_wait: u64,
    /// See [`Fence`]; and the block this round reads at.
    fence: Fence,
    at: Option<u64>,
    /// `--import-roots`.
    import_roots: bool,
    /// `--import-every` (seconds; 0: every round).
    import_every: u64,
    /// The period whose import is done (sent, or nothing to send).
    import_done: Option<u64>,
    /// `--send-via logos`: the Ethereum transactions go through Logos Core's `tx_sender_module`.
    logos: Option<crate::logos_send::Route>,
    sink: Sink,
}

impl Daemon {
    /// One transaction to the vault: [`Relayer::submit`] (the plain path), or through `tx_sender_module` after the
    /// same `eth_call`, the receipt then read on the relayer's own node; what `tx_sender_module` did not broadcast
    /// goes the plain way when the relayer has a key here too (the fallback).
    async fn send(
        &mut self,
        input: Bytes,
        label: &'static str,
        key: &str,
        job: Option<&str>,
    ) -> Result<(TransactionReceipt, crate::logos_send::Via)> {
        use crate::logos_send::{Outcome, Via};
        let Some(route) = self.logos.clone() else {
            return Ok((self.send_plain(input, label, key, job).await?, Via::Plain));
        };
        let fallback = route.fallback;
        let tx = TransactionRequest::default().with_from(self.r.me).with_to(self.r.vault).with_input(input.clone());
        if let Err(e) = self.p.call(tx).await {
            match eth::refusal_of(&e, "eth_call before sending") {
                Some(r) => bail!(r),
                None if e.as_error_resp().and_then(|p| p.as_revert_data()).is_some() => {
                    bail!("the vault would revert: {e}")
                }
                None => bail!("the endpoint failed (eth_call before sending): {e}"),
            }
        }
        // Journaled as on the plain path (R.06): the record before the request, with the nonce the account takes next;
        // the request's id as soon as `send` answers it; the hash once `send_status` broadcast it. A daemon killed in
        // between finds its request again and drives it to its end (`tx_sender_module` broadcasts only when polled):
        // never a second request while the first is open; one the module cannot account for is settled on the chain at
        // its nonce, and sent again only with that nonce pinned ([`Daemon::resolve_inflight`]).
        let nonce = match Inflight::load(&self.state)? {
            Some(f) if f.key == key => f.nonce,
            Some(f) => bail!("{} is still in flight (nonce {}): nothing else is sent before it resolves", f.key, f.nonce),
            None => eth::retry_rpc(|| self.p.get_transaction_count(self.r.me).pending()).await?.max(self.fence.next_nonce),
        };
        let mut f = Inflight {
            key: key.to_string(),
            label: label.to_string(),
            job: job.map(str::to_string),
            input: input.clone(),
            nonce,
            hash: None,
            at: unix_now(),
            from_block: self.at.unwrap_or(self.fence.block),
            via: Some(VIA_LOGOS.into()),
            request: None,
        };
        f.save(&self.state)?;
        let t0 = std::time::Instant::now();
        let (r2, vault, sent) = (route.clone(), self.r.vault, input.clone());
        let opened = tokio::task::spawn_blocking(move || r2.open(vault, &sent, label, None))
            .await
            .map_err(|e| anyhow!("the request to Logos Core did not finish (kept in flight): {e}"))?;
        let id = match opened {
            Ok(id) => id,
            // No answer: a request may exist; the next round looks for it (`live_sends`).
            Err(Outcome::NotSent(reason)) if reason.contains("did not answer") => {
                bail!("{reason} (kept in flight: the next round looks for the request)")
            }
            Err(Outcome::NotSent(reason)) => return self.logos_not_sent(reason, fallback, input, label, key, job).await,
            Err(o) => bail!("{o:?} (kept in flight, resolved next round)"),
        };
        f.request = Some(id.clone());
        f.save(&self.state)?;
        let outcome = tokio::task::spawn_blocking(move || route.drive(&id, t0))
            .await
            .map_err(|e| anyhow!("the send through Logos Core did not finish (kept in flight): {e}"))?;
        match outcome {
            Outcome::Broadcast { hash, request_id, polls, millis } => {
                f.hash = Some(hash);
                f.save(&self.state)?;
                let rc = self.wait_receipt(hash).await?;
                let nonce = self.nonce_of(hash).await.unwrap_or(f.nonce);
                self.mined(&rc, nonce)?;
                ensure!(rc.status(), "the transaction {} reverted", rc.transaction_hash);
                Ok((rc, Via::Logos { request_id, polls, millis }))
            }
            Outcome::NotSent(reason) => self.logos_not_sent(reason, fallback, input, label, key, job).await,
            Outcome::Unknown { reason, nonce, .. } => {
                if let Some(n) = nonce.filter(|n| *n != f.nonce) {
                    // The module's own number for the request: the one the chain is checked at.
                    f.nonce = n;
                    f.save(&self.state)?;
                }
                bail!("{reason} (kept in flight, resolved next round)")
            }
        }
    }

    /// Nothing was signed or broadcast for the call `key` through `tx_sender_module`: the plain path when this
    /// relayer holds a key too (the fallback: the same record and nonce, rewritten for the plain path), otherwise the
    /// record goes and the job is planned again in a later round.
    async fn logos_not_sent(
        &mut self,
        reason: String,
        fallback: bool,
        input: Bytes,
        label: &str,
        key: &str,
        job: Option<&str>,
    ) -> Result<(TransactionReceipt, crate::logos_send::Via)> {
        if fallback {
            let rc = self.send_plain(input, label, key, job).await?;
            return Ok((rc, crate::logos_send::Via::Fallback { reason }));
        }
        Inflight::clear(&self.state)?;
        bail!("not sent: {reason}")
    }

    /// The nonce of the relayer's transaction `hash`, as the node has it.
    async fn nonce_of(&self, hash: B256) -> Option<u64> {
        use alloy::consensus::Transaction as _;
        self.p.get_transaction_by_hash(hash).await.ok().flatten().map(|tx| tx.nonce())
    }

    /// A mined transaction of the relayer's moves the fence (and keeps it on disk).
    fn mined(&mut self, rc: &TransactionReceipt, nonce: u64) -> Result<()> {
        let block = rc.block_number.unwrap_or(self.fence.block);
        self.fence = Fence { block: self.fence.block.max(block), next_nonce: self.fence.next_nonce.max(nonce + 1) };
        write_atomic(&self.state.join("fence.json"), &serde_json::to_vec(&self.fence)?)
    }

    /// The vault as of the node's head, if the head is at or above the fence; `None` (the round waits) if the node
    /// is behind the relayer's own last transaction.
    async fn pinned_view(&mut self) -> Result<Option<relayer::VaultView>> {
        let head = eth::retry_rpc(|| self.p.get_block_number()).await?;
        if head < self.fence.block {
            log(&self.sink, "node-behind", json!({ "head": head, "fence": self.fence.block }));
            return Ok(None);
        }
        self.at = Some(head);
        Ok(Some(self.r.view_at(&self.p, Some(head)).await?))
    }

    /// The plain path, journaled: the vault's own checks (`eth_call`: a refusal is reported with the vault's
    /// error and nothing is signed), the in-flight record with the nonce, the send, the hash recorded, the
    /// receipt. The record is removed by the caller once the effect is booked ([`Inflight::clear`]).
    async fn send_plain(&mut self, input: Bytes, label: &str, key: &str, job: Option<&str>) -> Result<TransactionReceipt> {
        let tx = TransactionRequest::default().with_from(self.r.me).with_to(self.r.vault).with_input(input.clone());
        let at = self.at.map_or(alloy::eips::BlockId::latest(), alloy::eips::BlockId::number);
        if let Err(e) = self.p.call(tx.clone()).block(at).await {
            match eth::refusal_of(&e, "eth_call before sending") {
                Some(r) => bail!(r),
                // Revert data the vault's errors do not name: a refusal all the same.
                None if e.as_error_resp().and_then(|p| p.as_revert_data()).is_some() => {
                    bail!("the vault would revert: {e}")
                }
                // No revert: the endpoint's failure (a rate limit, a 5xx, a node behind), retried.
                None => bail!("the endpoint failed (eth_call before sending): {e}"),
            }
        }
        let nonce = match Inflight::load(&self.state)? {
            // The same call, lost by the node, sent again: with its nonce, at most one of them can be mined.
            Some(f) if f.key == key => f.nonce,
            Some(f) => bail!("{} is still in flight (nonce {}): nothing else is sent before it resolves", f.key, f.nonce),
            None => eth::retry_rpc(|| self.p.get_transaction_count(self.r.me).pending()).await?.max(self.fence.next_nonce),
        };
        let mut f = Inflight {
            key: key.to_string(),
            label: label.to_string(),
            job: job.map(str::to_string),
            input: input.clone(),
            nonce,
            hash: None,
            at: unix_now(),
            from_block: self.at.unwrap_or(self.fence.block),
            via: None,
            request: None,
        };
        f.save(&self.state)?;
        let tx = self.with_gas(tx, at).await?;
        let pending = self.p.send_transaction(tx.with_nonce(nonce)).await.map_err(|e| {
            match eth::refusal_of(&e, "eth_sendTransaction") {
                Some(r) => anyhow!(r),
                None => anyhow!("sending (kept in flight, resolved next round): {e}"),
            }
        })?;
        f.hash = Some(*pending.tx_hash());
        f.save(&self.state)?;
        let rc = self.wait_receipt(f.hash.expect("set")).await?;
        self.mined(&rc, nonce)?;
        ensure!(rc.status(), "the transaction {} reverted", rc.transaction_hash);
        Ok(rc)
    }

    /// `tx` with a gas limit of the node's estimate plus 30 % and 20,000: an estimate is the least that passed at
    /// the block it was made at, and a transaction mined later must not run out of gas for a few writes.
    async fn with_gas(&self, tx: TransactionRequest, at: alloy::eips::BlockId) -> Result<TransactionRequest> {
        let est = self.p.estimate_gas(tx.clone()).block(at).await.map_err(|e| anyhow!("eth_estimateGas: {e}"))?;
        Ok(tx.with_gas_limit(est.saturating_mul(13) / 10 + 20_000))
    }

    /// The receipt of `hash`, polled for `--receipt-timeout` seconds; transport errors meanwhile are waited out.
    async fn wait_receipt(&self, hash: B256) -> Result<TransactionReceipt> {
        let t0 = std::time::Instant::now();
        loop {
            if let Ok(Some(rc)) = self.p.get_transaction_receipt(hash).await {
                return Ok(rc);
            }
            if t0.elapsed() > Duration::from_secs(self.receipt_timeout) {
                bail!("{hash:#x} not mined within {} s: kept in flight, resolved next round", self.receipt_timeout);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// The relayer's transaction with `f`'s nonce in the blocks since `f` was written (at most 2,000), and its
    /// receipt, if it carries `f`'s calldata to the vault.
    async fn find_mined(&self, f: &Inflight) -> Result<Option<TransactionReceipt>> {
        use alloy::consensus::Transaction as _;
        use alloy::network::TransactionResponse as _;
        let head = eth::retry_rpc(|| self.p.get_block_number()).await?;
        let start = f.from_block.max(head.saturating_sub(2_000));
        for n in start..=head {
            let Some(block) = eth::retry_rpc(|| self.p.get_block_by_number(n.into()).full()).await? else { continue };
            for tx in block.transactions.txns() {
                if tx.from() == self.r.me && tx.nonce() == f.nonce {
                    if tx.to() != Some(self.r.vault) || tx.input() != &f.input {
                        return Ok(None);
                    }
                    return Ok(eth::retry_rpc(|| self.p.get_transaction_receipt(tx.tx_hash())).await?);
                }
            }
        }
        Ok(None)
    }

    /// Whether the effect of `f` is on the vault: for a nonce spent without a receipt for the recorded hash.
    async fn effect_done(&self, f: &Inflight) -> Result<bool> {
        let word = |k: &str| -> Result<B256> {
            k.split_once(':').map(|(_, w)| w).ok_or_else(|| anyhow!("key {k}"))?.parse().map_err(|e| anyhow!("{e}"))
        };
        let (p, vault) = (&self.p, self.r.vault);
        Ok(match f.label.as_str() {
            "submitCandidate" => {
                let new_state = word(&f.key)?;
                !eth::view(p, vault, IRelayVault::candidatesCall { newState: new_state }, None).await?.submittedAt.is_zero()
                    || eth::view(p, vault, IRelayVault::finalStateCall {}, None).await? == new_state
            }
            // Finalised, not merely gone: a watcher's cancellation also empties the candidate's slot.
            "finalizeAttested" => {
                use alloy::sol_types::SolEvent as _;
                let filter = alloy::rpc::types::Filter::new()
                    .address(vault)
                    .event_signature(IVault::CheckpointFinalized::SIGNATURE_HASH)
                    .topic2(word(&f.key)?)
                    .from_block(f.from_block);
                !eth::retry_rpc(|| p.get_logs(&filter)).await?.is_empty()
            }
            "release" => eth::view(p, vault, IVault::nullifierUsedCall { nullifier: word(&f.key)? }, None).await?,
            "claimOwed" => {
                let token: Address = f.key.split_once(':').map(|(_, t)| t).unwrap_or_default().parse()?;
                eth::view(p, vault, IRelayVault::owedCall { payee: self.r.me, token }, None).await?.is_zero()
            }
            _ => false,
        })
    }

    /// Books what a resolved in-flight transaction did: its job moved to `done/` (or `refused/` if it reverted)
    /// when still in the inbox, and the line the uninterrupted daemon would have written, with `"recovered": true`.
    fn book_recovered(&self, inbox: &Path, f: &Inflight, rc: Option<&TransactionReceipt>, done: bool) -> Result<()> {
        let s = &self.sink;
        let tx = rc.map(|r| json!(r.transaction_hash)).unwrap_or(Value::Null);
        let gas = rc.map(|r| json!(r.gas_used)).unwrap_or(Value::Null);
        match &f.job {
            Some(name) => {
                let dir = if done { "done" } else { "refused" };
                let file = inbox.join(name);
                if file.exists() {
                    std::fs::create_dir_all(self.state.join(dir))?;
                    std::fs::rename(&file, self.state.join(dir).join(name))?;
                }
                if done {
                    log(
                        s,
                        "job-done",
                        json!({ "job": name, "recovered": true, "result": { "kind": f.label, "key": f.key, "tx": tx, "gasUsed": gas } }),
                    );
                } else {
                    log(s, "job-refused", json!({ "job": name, "recovered": true, "reason": format!("{} reverted", f.key), "tx": tx }));
                }
            }
            None => {
                let event = match (f.label.as_str(), done) {
                    ("finalizeAttested", true) => "finalized",
                    ("claimOwed", true) => "fees-collected",
                    (_, true) => "sent",
                    (_, false) => "reverted",
                };
                log(s, event, json!({ "key": f.key, "recovered": true, "tx": tx, "gasUsed": gas }));
            }
        }
        Ok(())
    }

    /// Resolves the in-flight record a killed or cut-off daemon left (module documentation). `Ok(false)`: it
    /// is still pending, and nothing is sent this round.
    async fn resolve_inflight(&mut self, inbox: &Path) -> Result<bool> {
        let Some(mut f) = Inflight::load(&self.state)? else { return Ok(true) };
        let s = self.sink.clone();
        // Through tx_sender_module without a hash yet: its request (by the recorded id, or the one it holds open for this
        // very call) driven to its end; it broadcasts only when polled, so an open request is never made twice. What the
        // module cannot account for (the second review's M2: a request it no longer holds after a restart, `stuck`, a
        // leg failed after it began to leave) is not "nothing sent": it is settled on the chain below, at its nonce.
        let logos = f.via.as_deref() == Some(VIA_LOGOS);
        if let (true, None, Some(route)) = (logos, f.hash, self.logos.clone()) {
            use crate::logos_send::Outcome;
            if f.request.is_none() {
                let (r2, label, input) = (route.clone(), f.label.clone(), f.input.clone());
                let open = tokio::task::spawn_blocking(move || r2.live(&label, &input))
                    .await
                    .map_err(|e| anyhow!("live_sends did not finish: {e}"))?
                    .map_err(|e| anyhow!("{e} (in flight: {} kept)", f.key))?;
                if let Some(id) = open.last() {
                    f.request = Some(id.clone());
                    f.save(&self.state)?;
                    log(&s, "inflight-request-found", json!({ "key": f.key, "requestId": id, "open": open.len() }));
                }
            }
            if let Some(id) = f.request.clone() {
                let r3 = route.clone();
                let outcome = tokio::task::spawn_blocking(move || r3.drive(&id, std::time::Instant::now()))
                    .await
                    .map_err(|e| anyhow!("send_status did not finish: {e}"))?;
                match outcome {
                    Outcome::Broadcast { hash, request_id, polls, .. } => {
                        f.hash = Some(hash);
                        f.save(&self.state)?;
                        log(&s, "inflight-request-driven", json!({ "key": f.key, "requestId": request_id, "hash": hash, "statusPolls": polls }));
                    }
                    Outcome::NotSent(reason) => {
                        // Rejected, cancelled, failed before it left, or withdrawn unapproved, on the module's word for
                        // a request it holds: nothing left.
                        log(&s, "inflight-request-ended", json!({ "key": f.key, "requestId": f.request, "reason": reason }));
                        Inflight::clear(&self.state)?;
                        return Ok(true);
                    }
                    Outcome::Unknown { reason, forgotten: false, nonce } => {
                        f.nonce = nonce.unwrap_or(f.nonce);
                        f.save(&self.state)?;
                        log(&s, "inflight-request-unknown", json!({ "key": f.key, "requestId": f.request, "nonce": f.nonce, "reason": reason }));
                    }
                    Outcome::Unknown { reason, forgotten: true, .. } => {
                        // The module's durable history: its row for the request is written before the call leaves.
                        let (r4, id) = (route.clone(), f.request.clone().unwrap_or_default());
                        let row = tokio::task::spawn_blocking(move || r4.history_row(&id))
                            .await
                            .map_err(|e| anyhow!("history did not finish: {e}"))?;
                        match row {
                            Ok(None) => {
                                log(&s, "inflight-request-forgotten", json!({ "key": f.key, "requestId": f.request, "reason": reason, "history": "no row: never broadcast" }));
                                Inflight::clear(&self.state)?;
                                return Ok(true);
                            }
                            Ok(Some((n, hash))) => {
                                (f.nonce, f.hash) = (n, hash);
                                f.save(&self.state)?;
                                log(&s, "inflight-request-forgotten", json!({ "key": f.key, "requestId": f.request, "reason": reason, "history": { "nonce": n, "hash": hash } }));
                            }
                            Err(e) => {
                                log(&s, "inflight-request-forgotten", json!({ "key": f.key, "requestId": f.request, "nonce": f.nonce, "reason": reason, "history": format!("not read: {e}") }));
                            }
                        }
                    }
                }
            }
        }
        if let Some(hash) = f.hash {
            if let Some(rc) = eth::retry_rpc(|| self.p.get_transaction_receipt(hash)).await? {
                self.mined(&rc, f.nonce)?;
                self.book_recovered(inbox, &f, Some(&rc), rc.status())?;
                Inflight::clear(&self.state)?;
                return Ok(true);
            }
        }
        let latest = eth::retry_rpc(|| self.p.get_transaction_count(self.r.me).latest()).await?;
        if latest > f.nonce {
            // The nonce is spent and no receipt names the recorded hash (the daemon was killed before it had the
            // hash, or sent the call again): the transaction mined under the nonce, found in the blocks since the
            // record, is this call when its calldata is; otherwise this call was never mined.
            match self.find_mined(&f).await? {
                Some(rc) => {
                    self.mined(&rc, f.nonce)?;
                    log(&s, "inflight-found", json!({ "key": f.key, "nonce": f.nonce, "tx": rc.transaction_hash }));
                    self.book_recovered(inbox, &f, Some(&rc), rc.status())?;
                }
                None => {
                    let done = self.effect_done(&f).await?;
                    log(&s, "inflight-nonce-spent", json!({ "key": f.key, "nonce": f.nonce, "effectOnVault": done }));
                    if done {
                        self.book_recovered(inbox, &f, None, true)?;
                    }
                }
            }
            self.fence.next_nonce = self.fence.next_nonce.max(f.nonce + 1);
            Inflight::clear(&self.state)?;
            return Ok(true);
        }
        let known = match f.hash {
            Some(h) => eth::retry_rpc(|| self.p.get_transaction_by_hash(h)).await?.is_some(),
            None => false,
        };
        let pending = eth::retry_rpc(|| self.p.get_transaction_count(self.r.me).pending()).await?;
        if known || pending > f.nonce {
            log(&s, "inflight-pending", json!({ "key": f.key, "nonce": f.nonce, "hash": f.hash, "ageSecs": unix_now().saturating_sub(f.at) }));
            return Ok(false);
        }
        // Lost: neither mined nor known to the node, its nonce unspent. The same call again, with the same nonce.
        log(&s, "inflight-lost", json!({ "key": f.key, "nonce": f.nonce, "hash": f.hash }));
        let tx = TransactionRequest::default().with_from(self.r.me).with_to(self.r.vault).with_input(f.input.clone());
        if let Err(e) = self.p.call(tx.clone()).await {
            // Spent meanwhile (the call refused because it is done)? The next round books it at its nonce.
            if eth::retry_rpc(|| self.p.get_transaction_count(self.r.me).latest()).await? > f.nonce {
                return Ok(false);
            }
            // The call no longer passes (another relayer did it, a freeze, ...): dropped; its job is planned again.
            let reason = eth::refusal_of(&e, "eth_call before sending again").map_or_else(|| e.to_string(), |r| r.to_string());
            log(&s, "inflight-dropped", json!({ "key": f.key, "reason": reason }));
            Inflight::clear(&self.state)?;
            return Ok(true);
        }
        if let Some(route) = self.logos.clone().filter(|r| logos && !r.fallback) {
            return self.resend_pinned(inbox, f, route).await;
        }
        let tx = self.with_gas(tx, alloy::eips::BlockId::latest()).await?;
        let sent = self
            .p
            .send_transaction(tx.with_nonce(f.nonce))
            .await
            .map_err(|e| anyhow!("sending {} again (kept in flight): {e}", f.key))?;
        f.hash = Some(*sent.tx_hash());
        f.at = unix_now();
        f.save(&self.state)?;
        log(&s, "inflight-resent", json!({ "key": f.key, "nonce": f.nonce, "hash": f.hash }));
        let rc = self.wait_receipt(f.hash.expect("set")).await?;
        self.mined(&rc, f.nonce)?;
        self.book_recovered(inbox, &f, Some(&rc), rc.status())?;
        Inflight::clear(&self.state)?;
        Ok(true)
    }

    /// A lost call of the route through `tx_sender_module`, with no key here: the same call again through the module,
    /// its nonce pinned to the record's, so that at most one of the two can ever be mined (the module takes a number
    /// nothing holds, or one whose transaction began to leave; it refuses one an unbroadcast request holds, and the
    /// record then waits). `Ok(false)`: not settled this round.
    async fn resend_pinned(&mut self, inbox: &Path, mut f: Inflight, route: crate::logos_send::Route) -> Result<bool> {
        use crate::logos_send::Outcome;
        let s = self.sink.clone();
        let (r2, vault, input, label, nonce) = (route.clone(), self.r.vault, f.input.clone(), f.label.clone(), f.nonce);
        let opened = tokio::task::spawn_blocking(move || r2.open(vault, &input, &label, Some(nonce)))
            .await
            .map_err(|e| anyhow!("the request to Logos Core did not finish (kept in flight): {e}"))?;
        let id = match opened {
            Ok(id) => id,
            Err(o) => {
                log(&s, "inflight-resend-refused", json!({ "key": f.key, "nonce": f.nonce, "reason": format!("{o:?}") }));
                return Ok(false);
            }
        };
        let earlier = f.request.replace(id.clone());
        f.at = unix_now();
        f.save(&self.state)?;
        log(&s, "inflight-resent", json!({ "key": f.key, "nonce": f.nonce, "requestId": id, "pinned": true, "earlierRequest": earlier }));
        let r3 = route.clone();
        let outcome = tokio::task::spawn_blocking(move || r3.drive(&id, std::time::Instant::now()))
            .await
            .map_err(|e| anyhow!("send_status did not finish: {e}"))?;
        match outcome {
            Outcome::Broadcast { hash, .. } => {
                f.hash = Some(hash);
                f.save(&self.state)?;
                let rc = self.wait_receipt(hash).await?;
                self.mined(&rc, f.nonce)?;
                self.book_recovered(inbox, &f, Some(&rc), rc.status())?;
                Inflight::clear(&self.state)?;
                Ok(true)
            }
            Outcome::NotSent(reason) => {
                // This request ended with nothing sent; the earlier one stays what the record follows.
                f.request = earlier;
                f.save(&self.state)?;
                log(&s, "inflight-pending", json!({ "key": f.key, "nonce": f.nonce, "requestId": f.request, "reason": reason }));
                Ok(false)
            }
            Outcome::Unknown { reason, .. } => {
                log(&s, "inflight-pending", json!({ "key": f.key, "nonce": f.nonce, "requestId": f.request, "reason": reason }));
                Ok(false)
            }
        }
    }

    async fn round(&mut self, inbox: &Path) -> Result<()> {
        let s = self.sink.clone();
        // A transaction a killed or cut-off daemon left in flight comes first; while it is pending nothing is sent.
        if !self.resolve_inflight(inbox).await? {
            return Ok(());
        }
        let Some(v) = self.pinned_view().await? else { return Ok(()) };
        // 0. Root imports, once per period: before the inbox, whose gas drops check the imported roots.
        let period = v.now.checked_div(self.import_every);
        if self.import_roots && (period.is_none() || period != self.import_done) {
            match self.import_root().await {
                Ok(true) => {
                    if let Some(p) = period {
                        // Kept in the state directory: a restart does not import twice in one period.
                        std::fs::write(
                            self.state.join("root-import.json"),
                            serde_json::to_vec(&json!({ "every": self.import_every, "period": p }))?,
                        )?;
                    }
                    self.import_done = period;
                }
                Ok(false) => {}
                Err(e) => log(&s, "root-import-failed", json!({ "reason": format!("{e:#}") })),
            }
        }
        // 1. The inbox, in name order.
        let mut files: Vec<PathBuf> = match std::fs::read_dir(inbox) {
            Ok(dir) => dir
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect(),
            Err(_) => Vec::new(),
        };
        files.sort();
        for file in files {
            let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let outcome = self.job(&file, &v).await;
            let age = file_age(&file);
            let dir = match &outcome {
                Ok(_) => "done",
                // A root not final yet, a delay, a freeze, a cap, a transport failure: retried next round,
                // until the job is older than --max-job-age.
                Err(e) if relayer::temporary(&format!("{e:#}")) && age < self.max_job_age => {
                    log(&s, "job-deferred", json!({ "job": name, "reason": format!("{e:#}"), "ageSecs": age }));
                    continue;
                }
                Err(_) => "refused",
            };
            std::fs::create_dir_all(self.state.join(dir))?;
            std::fs::rename(&file, self.state.join(dir).join(&name))?;
            // The line before the record goes: a daemon killed in between books the job again on restart
            // (`"recovered": true`), so a booked job always has its line.
            match outcome {
                Ok(fields) => log(&s, "job-done", json!({ "job": name, "result": fields })),
                Err(e) => log(&s, "job-refused", json!({ "job": name, "reason": format!("{e:#}") })),
            }
            if Inflight::load(&self.state)?.is_some_and(|f| f.job.as_deref() == Some(name.as_str())) {
                Inflight::clear(&self.state)?;
            }
        }
        if self.gas_drops_only {
            return Ok(());
        }
        // 2. Candidates whose wait is over.
        let Some(v) = self.pinned_view().await? else { return Ok(()) };
        for (c, ready_at) in self.r.candidates_at(&self.p, self.from_block, v.block).await? {
            if !self.rule_b {
                // Rule (A) finalises with a second proof echoing a beacon: the checkpoint prover's job.
                log(
                    &s,
                    "candidate-not-finalized",
                    json!({ "newState": c.newState, "reason": "rule (A) vault: finalize needs a beacon proof" }),
                );
                continue;
            }
            if v.now < ready_at {
                log(&s, "candidate-waiting", json!({ "newState": c.newState, "readyAt": ready_at, "now": v.now }));
                continue;
            }
            let input: Bytes = IRelayVault::finalizeAttestedCall { c: c.clone() }.abi_encode().into();
            let key = format!("finalizeAttested:{:#x}", c.newState);
            match self.send(input, "finalizeAttested", &key, None).await {
                Ok((rc, via)) => {
                    log(
                        &s,
                        "finalized",
                        json!({ "newState": c.newState, "burnRoot": c.burnRoot, "burnCount": c.burnCount, "tx": rc.transaction_hash, "gasUsed": rc.gas_used, "via": via.to_json() }),
                    );
                    Inflight::clear(&self.state)?;
                }
                Err(e) => {
                    log(&s, "finalize-refused", json!({ "newState": c.newState, "reason": format!("{e:#}") }));
                    self.after_failed_send(&key).await?;
                    break;
                }
            }
        }
        // 3. Fees credited to this relayer (checkpoint fees always; a relay fee whose push failed).
        for token in self.tokens.clone() {
            let owed =
                eth::view(&self.p, self.r.vault, IRelayVault::owedCall { payee: self.r.me, token }, v.block).await?;
            if owed.is_zero() {
                continue;
            }
            let input: Bytes = IRelayVault::claimOwedCall { token }.abi_encode().into();
            let key = format!("claimOwed:{token:#x}");
            match self.send(input, "claimOwed", &key, None).await {
                Ok((rc, via)) => {
                    log(
                        &s,
                        "fees-collected",
                        json!({ "token": token, "amount": owed.to_string(), "tx": rc.transaction_hash, "gasUsed": rc.gas_used, "via": via.to_json() }),
                    );
                    Inflight::clear(&self.state)?;
                }
                Err(e) => {
                    log(&s, "fees-refused", json!({ "token": token, "reason": format!("{e:#}") }));
                    self.after_failed_send(&key).await?;
                    break;
                }
            }
        }
        // 4. Burns on LEZ past the newest accepted burn count.
        if let Some((zone, ctx)) = &self.zone {
            let history = lez::history_of(zone, ctx).await?;
            let accepted = eth::accepted_burn_roots(&self.p, self.r.vault, self.from_block, 10_000).await?;
            let live = self.r.candidates_at(&self.p, self.from_block, v.block).await?;
            let path = self.state.join("needs-checkpoint.json");
            if let Some((burns, covered)) =
                relayer::checkpoint_needed(history.burns.len() as u64, &accepted, live.len())
            {
                std::fs::write(
                    &path,
                    serde_json::to_vec_pretty(
                        &json!({ "burns": burns, "accepted": covered, "finalState": v.final_state }),
                    )?,
                )?;
                log(&s, "checkpoint-needed", json!({ "burns": burns, "accepted": covered }));
            } else if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
        Ok(())
    }

    /// Step 0: the vault's newest deposit root on LEZ, under the newest record of the configuration's instance.
    /// `Ok(true)` when the period's import is done (sent and applied, or nothing to import, or closed).
    async fn import_root(&mut self) -> Result<bool> {
        let s = self.sink.clone();
        let (state_dir, import_wait) = (self.state.clone(), self.import_wait);
        let Some((zone, ctx)) = self.zone.as_mut() else { return Ok(true) };
        // An import sent before a kill or a lost answer: applied, it is booked; within --import-wait it is waited
        // for; older, it is planned again (a second ImportRoot of the same count is refused by the program).
        let inflight = state_dir.join("root-import-inflight.json");
        if let Ok(bytes) = std::fs::read(&inflight) {
            let f: ImportInflight = serde_json::from_slice(&bytes).context("root-import-inflight.json")?;
            let at = ferrata_lez_bridge_wire::root_actor(ctx.program, &ctx.cid, f.leaf_count);
            let applied = match lez::read(&*zone, at).await? {
                Some(BridgeState::Root(r)) => borsh::to_vec(&r)? == f.writes.to_vec(),
                _ => false,
            };
            let age = unix_now().saturating_sub(f.at);
            if applied {
                log(&s, "root-imported", json!({ "leafCount": f.leaf_count, "tx": f.tx, "applied": true, "recovered": true }));
                std::fs::remove_file(&inflight)?;
                return Ok(true);
            }
            if age < import_wait {
                log(&s, "root-import-pending", json!({ "leafCount": f.leaf_count, "tx": f.tx, "ageSecs": age }));
                return Ok(false);
            }
            log(&s, "root-import-expired", json!({ "leafCount": f.leaf_count, "tx": f.tx, "ageSecs": age }));
            std::fs::remove_file(&inflight)?;
        }
        let history = lez::history_of(&*zone, ctx).await?;
        let vault_count = eth::view(&self.p, self.r.vault, IVault::nextLeafIndexCall {}, None).await?;
        let imported = history.roots.iter().copied().max().unwrap_or(0);
        let done = match relayer::plan_root_import(&*zone, &self.p, ctx, &history, vault_count).await {
            Ok(RootImport::UpToDate { .. }) => true,
            Ok(RootImport::Waiting { reason, vault_count, imported }) => {
                log(
                    &s,
                    "root-import-waiting",
                    json!({ "reason": reason, "vaultCount": vault_count, "imported": imported }),
                );
                false
            }
            Ok(RootImport::Closed { reason }) => {
                log(&s, "root-import-closed", json!({ "reason": reason }));
                true
            }
            Ok(RootImport::Planned(plan)) => {
                let mut fields = json!({
                    "leafCount": plan.leaf_count, "root": hx(&plan.root), "recordSlot": plan.record_slot,
                    "block": plan.block_number, "blockHash": hx(&plan.block_hash), "previous": plan.imported,
                    "vaultCount": vault_count, "accounts": plan.call.mentions.len(),
                });
                // Kept from before the send: a kill or a lost answer after it is resolved by the next rounds.
                let mut record = ImportInflight {
                    leaf_count: plan.leaf_count,
                    writes: borsh::to_vec(&plan.writes)?.into(),
                    tx: "sending".into(),
                    at: unix_now(),
                };
                write_atomic(&inflight, &serde_json::to_vec_pretty(&record)?)?;
                match zone.submit(&plan.call).await {
                    Ok(tx) => {
                        record.tx = tx.clone();
                        write_atomic(&inflight, &serde_json::to_vec_pretty(&record)?)?;
                        // Applied when root(cid, n) holds what the dry run said the import writes.
                        let at = ferrata_lez_bridge_wire::root_actor(ctx.program, &ctx.cid, plan.leaf_count);
                        let applied =
                            matches!(lez::read(&*zone, at).await?, Some(BridgeState::Root(r)) if r == plan.writes);
                        if applied {
                            std::fs::remove_file(&inflight)?;
                        }
                        fields["tx"] = json!(tx);
                        fields["applied"] = json!(applied);
                        log(&s, if applied { "root-imported" } else { "root-import-unconfirmed" }, fields);
                        applied
                    }
                    Err(e) => {
                        fields["reason"] = json!(format!("{e:#}"));
                        fields["keptInFlight"] = json!(true);
                        log(&s, "root-import-failed", fields);
                        false
                    }
                }
            }
            Err(e) => {
                log(
                    &s,
                    "root-import-refused",
                    json!({ "reason": format!("{e:#}"), "vaultCount": vault_count, "imported": imported }),
                );
                false
            }
        };
        Ok(done)
    }

    /// After a send that failed: a reverted transaction is resolved now (its record goes, nothing more comes of
    /// it); one that may still be mined stays in flight for the next round.
    async fn after_failed_send(&self, key: &str) -> Result<()> {
        if let Some(f) = Inflight::load(&self.state)?.filter(|f| f.key == key) {
            if let Some(h) = f.hash {
                if let Ok(Some(rc)) = self.p.get_transaction_receipt(h).await {
                    if !rc.status() {
                        Inflight::clear(&self.state)?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn job(&mut self, file: &Path, v: &relayer::VaultView) -> Result<serde_json::Value> {
        let job: Job = serde_json::from_slice(&std::fs::read(file)?).context("not a relayer job")?;
        if self.gas_drops_only && !matches!(job, Job::GasDrop { .. }) {
            bail!("refused: this relayer runs gas drops only (--gas-drops-only)");
        }
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned());
        let name = name.as_deref();
        match job {
            Job::Release { journal, seal } => {
                let (req, input) =
                    self.r.plan_release(&self.p, v, &relayer::unhex(&journal)?, relayer::unhex(&seal)?.into()).await?;
                let key = format!("release:{:#x}", req.nullifier);
                let (rc, via) = self.send(input, "release", &key, name).await?;
                self.tokens.insert(req.token);
                let paid = relayer::relay_fee_paid(&rc, self.r.me);
                Ok(json!({
                    "kind": "release", "nullifier": req.nullifier, "recipient": req.recipient, "token": req.token,
                    "relayFee": req.relayFee.to_string(), "paidTo": self.r.me, "paid": paid.map(|(_, f)| f.to_string()),
                    "tx": rc.transaction_hash, "gasUsed": rc.gas_used, "via": via.to_json(),
                }))
            }
            Job::ReleaseSuccinct { receipt } => {
                let Some(cmd) = self.wrap_command.clone() else {
                    bail!("refused: this relayer does not wrap receipts (no --wrap-command)");
                };
                let receipt: risc0_zkvm::Receipt = serde_json::from_value(receipt).context("not a receipt")?;
                let journal = receipt.journal.bytes.clone();
                // The relayer's own checks first: nothing is wrapped for a release it would not send.
                self.r.plan_release(&self.p, v, &journal, Bytes::new()).await?;
                let image = eth::view(&self.p, self.r.vault, IRelayVault::releaseImageIdCall {}, None).await?;
                let work = self.state.join("wrap").join(hex::encode(&sha2::Sha256::digest(&journal)[..8]));
                let started = std::time::Instant::now();
                let seal = tokio::task::block_in_place(|| relayer::wrap_release(&receipt, image.0, &cmd, &work))?;
                let wrap_s = started.elapsed().as_secs_f64();
                let (req, input) = self.r.plan_release(&self.p, v, &journal, seal.into()).await?;
                let key = format!("release:{:#x}", req.nullifier);
                let (rc, via) = self.send(input, "release", &key, name).await?;
                self.tokens.insert(req.token);
                let paid = relayer::relay_fee_paid(&rc, self.r.me);
                Ok(json!({
                    "kind": "release", "wrapped": true, "wrapSecs": wrap_s, "nullifier": req.nullifier,
                    "recipient": req.recipient, "token": req.token, "relayFee": req.relayFee.to_string(),
                    "paidTo": self.r.me, "paid": paid.map(|(_, f)| f.to_string()), "tx": rc.transaction_hash,
                    "gasUsed": rc.gas_used, "via": via.to_json(),
                }))
            }
            Job::Checkpoint { journal, seal } => {
                let journal = relayer::unhex(&journal)?;
                let input = self.r.plan_candidate(&self.p, v, &journal, relayer::unhex(&seal)?.into()).await?;
                let c = relayer::checkpoint_from_journal(&journal)?.checkpoint;
                let key = format!("submitCandidate:{:#x}", c.newState);
                let (rc, via) = self.send(input, "submitCandidate", &key, name).await?;
                Ok(
                    json!({ "kind": "candidate", "newState": c.newState, "burnRoot": c.burnRoot, "burnCount": c.burnCount, "tx": rc.transaction_hash, "gasUsed": rc.gas_used, "via": via.to_json() }),
                )
            }
            Job::GasDrop { receipt, one_time_npk, one_time_vpk } => {
                let keys = relayer::OneTimeKeys::from_hex(&one_time_npk, &one_time_vpk).map_err(|e| {
                    anyhow!("refused: the job's one-time account keys (oneTimeNpk, oneTimeVpk) do not decode: {e:#}")
                })?;
                let (Some((zone, ctx)), Some(image_id), Some(me)) =
                    (self.zone.as_mut(), self.gasdrop_image_id, self.lez_me)
                else {
                    bail!(
                        "refused: this relayer runs no gas drops (no --sequencer, --gasdrop-image-id or --lez-account)"
                    );
                };
                let history = lez::history_of(&*zone, ctx).await?;
                let list = eth::deposit_list(&self.p, self.r.vault, self.from_block, 10_000).await?;
                let roots = relayer::imported_roots(&list.deposits, &history.roots)?;
                let mut chain_id = [0u8; 32];
                chain_id[24..].copy_from_slice(&ctx.chain_id.to_be_bytes());
                let dctx = DropContext { image_id, me, chain_id, vault: ctx.vault, imported_roots: &roots };
                let book_path = self.state.join("drops.json");
                let mut book = DropBook::load(&book_path)?;
                let ctx2 = ctx.clone();
                let claimed = |n: &[u8; 32]| lez::is_claimed(&ctx2, &history, n);
                // The journaled drop (R.06): recorded before its credit, resolved from LEZ in the next rounds; the job
                // stays in the inbox ("in flight": deferred) until LEZ shows the credit applied.
                let terms = DropTerms {
                    amount: self.drop_amount,
                    now: unix_now(),
                    drop_wait: self.drop_wait,
                    max_sends: self.drop_max_sends,
                    job_age: file_age(file),
                    max_job_age: self.max_job_age,
                };
                let (j, step) = relayer::gas_drop_step(
                    &serde_json::to_vec(&receipt)?,
                    &keys,
                    &dctx,
                    claimed,
                    &mut book,
                    &book_path,
                    zone,
                    &terms,
                )
                .await?;
                match step {
                    DropStep::Applied { tx, sends, resolved } => Ok(json!({
                        "kind": "gas-drop", "nullifier": hx(&j.nullifier), "oneTimeAddress": hx(&j.one_time_address),
                        "amount": self.drop_amount.to_string(), "sealed": true, "tx": tx, "sends": sends,
                        "applied": true, "resolvedLater": resolved,
                    })),
                    DropStep::InFlight { reason } => bail!("in flight: {reason}"),
                    DropStep::GivenUp { why, sends } => {
                        log(
                            &self.sink,
                            "gas-drop-given-up",
                            json!({ "job": name, "nullifier": hx(&j.nullifier), "sends": sends, "why": why }),
                        );
                        bail!("refused: the gas drop for this N was given up with nothing applied: {why}")
                    }
                }
            }
        }
    }
}

/// The sequencer zone: LEZ's wallet with the relayer's seed.
#[cfg(feature = "lez-wallet")]
fn sequencer(cli: &Cli, rpc: &str) -> Result<Zone> {
    let need = |o: &Option<PathBuf>, f: &str| o.clone().ok_or_else(|| anyhow!("{f} is required with --sequencer"));
    let seed = crate::seed::Seed::from_file(&need(&cli.seed, "--seed")?)?;
    let wallet = lez::open_wallet(
        &need(&cli.lez_wallet, "--lez-wallet")?,
        rpc,
        &need(&cli.lez_password_file, "--lez-password-file")?,
        &seed,
    )?;
    let payer = cli.lez_account.as_deref().map(account).transpose()?;
    let drop_from = seed
        .private_account_ids(cli.drop_from_private + 1)?
        .pop()
        .ok_or_else(|| anyhow!("no private account /{} in the relayer's seed", cli.drop_from_private))?;
    Ok(Zone::Sequencer(lez::Sequencer {
        wallet,
        payer,
        bridge_program: None,
        synced: false,
        private_fee_payer: None,
        drop_from: Some(drop_from),
    }))
}

#[cfg(not(feature = "lez-wallet"))]
fn sequencer(_cli: &Cli, _rpc: &str) -> Result<Zone> {
    bail!("this build has no LEZ wallet (feature `lez-wallet`): --sequencer is not available here, --lez-devnet is")
}

/// The daemon: `ferrata-relayer ARGS` until its rounds are done, or until `stop` holds a reason (the
/// binary's SIGTERM or SIGINT, the module's `stop`), after the round in progress. Every line goes to `sink`.
pub async fn run(cli: Cli, sink: Sink, mut stop: tokio::sync::watch::Receiver<Option<String>>) -> Result<()> {
    let url = cli.eth_rpc.clone().ok_or_else(|| anyhow!("--eth-rpc is required"))?;
    // RISC Zero's dev mode makes fake receipts verify: a relayer paying gas drops under it pays real drops for fake
    // receipts. Refused unless asked for, on a sequencer at a loopback address (a local devnet).
    if cli.gasdrop_image_id.is_some() && risc0_zkvm::VerifierContext::default().dev_mode() {
        let local = cli.sequencer.as_deref().and_then(|u| url::Url::parse(u).ok()).is_some_and(|u| {
            matches!(u.host_str(), Some("127.0.0.1" | "localhost" | "[::1]" | "::1"))
        });
        ensure!(
            cli.accept_dev_mode_receipts && local,
            "RISC0_DEV_MODE is set: gas-drop receipts would verify without a proof; unset it, or on a local devnet add --accept-dev-mode-receipts (with --sequencer at a loopback address)"
        );
    }
    let vault = cli.vault.ok_or_else(|| anyhow!("--vault is required"))?;
    let logos = cli.send_via == "logos";
    // The key: no network yet.
    if cli.gas_drops_only {
        ensure!(
            cli.keystore.is_none() && cli.unlocked.is_none() && !logos,
            "--gas-drops-only sends nothing to Ethereum: give no --keystore, --unlocked or --send-via logos"
        );
        ensure!(!cli.import_roots, "--gas-drops-only makes no root imports (--import-roots)");
        ensure!(
            cli.sequencer.is_some() && cli.gasdrop_image_id.is_some() && cli.lez_account.is_some() && cli.drop_amount > 0,
            "--gas-drops-only needs --sequencer, --gasdrop-image-id, --lez-account and --drop-amount"
        );
    }
    let (p, me, unlocked) = match (&cli.keystore, &cli.password_file, cli.unlocked) {
        (None, None, None) if cli.gas_drops_only => (eth::provider(&url)?, Address::ZERO, false),
        (None, None, None) if logos => {
            let me = cli.from.ok_or_else(|| anyhow!("--send-via logos needs --from (the keystore account)"))?;
            (eth::provider(&url)?, me, false)
        }
        (Some(ks), Some(pw), None) => {
            let signer = eth::keystore_signer(ks, pw)?;
            let me = signer.address();
            (eth::signing_provider(&url, signer)?, me, false)
        }
        (None, None, Some(me)) => (eth::provider(&url)?, me, true),
        _ => bail!("give --keystore and --password-file, or --unlocked on a local anvil, or --send-via logos --from"),
    };
    let r = Relayer { vault, me, accept_unpaid: cli.accept_unpaid };
    // The start-up reads, retried while the endpoint fails (it may be down, rate-limited or behind when the daemon
    // starts: R.06), up to --startup-wait seconds; a wrong chain, a node that is not anvil for --unlocked, or a
    // vault that is not one stop it at once.
    let t0 = std::time::Instant::now();
    let mut delay = 1u64;
    let (v, rule_b) = loop {
        let attempt: Result<std::result::Result<(relayer::VaultView, bool), String>> = async {
            if unlocked {
                // anvil answers anvil_nodeInfo; a public node does not, and must never be asked to sign.
                if let Err(e) = p.raw_request::<_, serde_json::Value>("anvil_nodeInfo".into(), Vec::<u64>::new()).await {
                    if e.as_error_resp().is_some() {
                        let chain = p.get_chain_id().await?;
                        return Ok(Err(format!("--unlocked needs a local anvil (chain {chain} answers no anvil_nodeInfo)")));
                    }
                    return Err(e.into());
                }
            }
            let v = r.view(&p).await?;
            if let Err(e) = relayer::require_chain(&v, cli.chain_id) {
                return Ok(Err(format!("{e:#}")));
            }
            let mode = eth::view(&p, vault, IRelayVault::modeCall {}, None).await;
            match mode {
                Ok(m) => Ok(Ok((v, m == 1))),
                Err(e) if format!("{e:#}").contains("revert") => Ok(Err(format!("{vault} is not a FerrataVault: {e:#}"))),
                Err(e) => Err(e),
            }
        }
        .await;
        match attempt {
            Ok(Ok(x)) => break x,
            Ok(Err(stop_now)) => bail!(stop_now),
            Err(e) if t0.elapsed().as_secs() < cli.startup_wait && stop.borrow().is_none() => {
                log(&sink, "waiting-for-rpc", json!({ "reason": format!("{e:#}"), "waitedSecs": t0.elapsed().as_secs() }));
                tokio::time::sleep(Duration::from_secs(delay)).await;
                delay = (delay * 2).min(30);
            }
            Err(e) => return Err(e.context(format!("the Ethereum endpoint did not answer within {} s", cli.startup_wait))),
        }
    };
    let logos = if logos {
        let call = crate::logos_send::installed()
            .ok_or_else(|| anyhow!("--send-via logos needs Logos Core: run the relayer as ferrata_relayer_module"))?;
        if let Some(f) = cli.from {
            anyhow::ensure!(f == me, "--from {f} is not the relayer key's address {me}");
        }
        Some(crate::logos_send::Route {
            call,
            chain_id: v.chain_id,
            from: me,
            purpose: "Ferrata relayer",
            fallback: cli.keystore.is_some() || cli.unlocked.is_some(),
            approval_timeout: Duration::from_secs(cli.approval_timeout),
        })
    } else {
        None
    };
    std::fs::create_dir_all(&cli.state)?;
    // One daemon per state directory: two would each think the in-flight record theirs.
    let _lock = if matches!(cli.command, Command::Run { .. }) {
        let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(cli.state.join("lock"))?;
        lock.try_lock()
            .map_err(|_| anyhow!("another ferrata-relayer runs on {} (its lock is held)", cli.state.display()))?;
        Some(lock)
    } else {
        None
    };

    let zone = match (&cli.sequencer, &cli.lez_devnet) {
        (None, None) => None,
        (Some(_), Some(_)) => bail!("give --sequencer or --lez-devnet, not both"),
        (seq, devnet) => {
            let zone = match (seq, devnet) {
                (Some(rpc), _) => sequencer(&cli, rpc)?,
                (_, Some(url)) => Zone::Devnet(lez::DevnetZone::new(url)?),
                (None, None) => unreachable!(),
            };
            let program =
                account(cli.bridge_account.as_deref().ok_or_else(|| anyhow!("--bridge-account is required"))?)?;
            let ctx = Context { program, cid: v.cid.0, chain_id: v.chain_id, vault: vault.into_array() };
            Some((zone, ctx))
        }
    };
    if cli.import_roots && zone.is_none() {
        bail!("--import-roots needs a zone: --sequencer, or --lez-devnet for the devnet");
    }
    let mut d = Daemon {
        p,
        r,
        state: cli.state.clone(),
        from_block: cli.from_block,
        zone,
        gasdrop_image_id: cli.gasdrop_image_id.as_deref().map(h32).transpose()?,
        lez_me: cli.lez_account.as_deref().map(h32).transpose()?,
        drop_amount: cli.drop_amount,
        drop_wait: cli.drop_wait,
        drop_max_sends: cli.drop_max_sends,
        tokens: [Address::ZERO].into_iter().collect(),
        rule_b,
        gas_drops_only: cli.gas_drops_only,
        max_job_age: cli.max_job_age,
        wrap_command: cli.wrap_command.clone(),
        receipt_timeout: cli.receipt_timeout,
        import_wait: cli.import_wait,
        fence: Fence::load(&cli.state),
        at: None,
        import_roots: cli.import_roots,
        import_every: cli.import_every,
        import_done: std::fs::read(cli.state.join("root-import.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .filter(|v| v["every"].as_u64() == Some(cli.import_every))
            .and_then(|v| v["period"].as_u64()),
        logos,
        sink: sink.clone(),
    };
    let zone_kind = d.zone.as_ref().map(|(z, _)| z.kind());
    log(
        &sink,
        "start",
        json!({ "vault": vault, "relayer": me, "chainId": v.chain_id, "configId": v.cid, "finalState": v.final_state,
                "gasDrops": zone_kind == Some("sequencer"), "zone": zone_kind, "importRoots": cli.import_roots,
                "importEvery": cli.import_every,
                "sendVia": d.logos.as_ref().map_or_else(|| json!("plain"), |r| json!({ "path": "logos",
                    "module": crate::logos_send::TX_SENDER, "from": r.from, "fallback": r.fallback })) }),
    );

    match cli.command {
        Command::Status => {
            let candidates = d.r.candidates(&d.p, d.from_block).await?;
            let balance = d.p.get_balance(me).await?;
            let owed = eth::view(&d.p, vault, IRelayVault::owedCall { payee: me, token: Address::ZERO }, None).await?;
            let book = DropBook::load(&d.state.join("drops.json"))?;
            let finalized = eth::view(&d.p, vault, IVault::checkpointCountCall {}, None).await?;
            log(
                &sink,
                "status",
                json!({
                    "finalState": v.final_state, "checkpoints": finalized.to_string(),
                    "candidates": candidates.iter().map(|(c, at)| json!({ "newState": c.newState, "readyAt": at })).collect::<Vec<_>>(),
                    "balanceWei": balance.to_string(), "owedWei": owed.to_string(), "drops": book.drops.len(),
                }),
            );
        }
        Command::CollectFees { token, limit, record_slot } => {
            let Some((zone, ctx)) = d.zone.as_mut() else {
                bail!("collect-fees needs a zone: --sequencer, or --lez-devnet for the devnet")
            };
            let token = token.into_array();
            match relayer::plan_fee_collection(&*zone, &d.p, ctx, token, limit.unwrap_or(u128::MAX), record_slot).await? {
                relayer::FeeCollection::Waiting { reason } => {
                    log(&sink, "lez-fee-collection-waiting", json!({ "token": hx(&token), "reason": reason }));
                }
                relayer::FeeCollection::Planned(plan) => {
                    let mut fields = json!({
                        "token": hx(&token), "recordSlot": plan.record_slot, "block": plan.block_number,
                        "blockHash": hx(&plan.block_hash),
                        "vaultTotal": alloy::primitives::U256::from_be_bytes(plan.total).to_string(),
                        "collectedBefore": alloy::primitives::U256::from_be_bytes(plan.collected).to_string(),
                        "amount": plan.amount.to_string(),
                        "accounts": plan.call.mentions.len(),
                    });
                    // The fee recipient's holding of the token under this configuration, at its holder(cid, account).
                    let recipient = ferrata_lez_bridge_wire::holder_actor(
                        ctx.program,
                        &ctx.cid,
                        ferrata_wallet::AccountId::new(plan.fee_recipient),
                    );
                    let held = |state: Option<BridgeState>| match state {
                        Some(BridgeState::Holder(h)) => h.holding(&ctx.cid, &token).map_or(0, |x| x.amount),
                        _ => 0,
                    };
                    let before = held(lez::read(&*zone, recipient).await?);
                    let tx = zone.submit(&plan.call).await?;
                    // Applied when fees(cid, token) holds what the dry run said the collection writes.
                    let at = ferrata_lez_bridge_wire::fees_actor(ctx.program, &ctx.cid, &token);
                    let applied = matches!(lez::read(&*zone, at).await?, Some(BridgeState::Fees(f)) if f == plan.writes);
                    let after = held(lez::read(&*zone, recipient).await?);
                    fields["tx"] = json!(tx);
                    fields["applied"] = json!(applied);
                    fields["feeRecipient"] = json!(hx(&plan.fee_recipient));
                    fields["feeRecipientHoldingBefore"] = json!(before.to_string());
                    fields["feeRecipientHoldingAfter"] = json!(after.to_string());
                    log(&sink, if applied { "lez-fees-collected" } else { "lez-fee-collection-unconfirmed" }, fields);
                    ensure!(
                        applied,
                        "CollectFees sent ({tx}) but not applied: fees(cid, token) does not hold what the dry run wrote (the \
                         program refused it, or the zone moved since the dry run: a freeze, a bucket, another collection)"
                    );
                }
            }
        }
        Command::Run { inbox, poll_secs, rounds } => {
            std::fs::create_dir_all(&inbox)?;
            let stop_reason = |s: &tokio::sync::watch::Receiver<Option<String>>| s.borrow().clone();
            let mut n = 0u64;
            let mut reason = "rounds".to_string();
            loop {
                if let Err(e) = d.round(&inbox).await {
                    log(&sink, "round-failed", json!({ "reason": format!("{e:#}") }));
                }
                n += 1;
                if rounds != 0 && n >= rounds {
                    break;
                }
                // A stop asked for during the round ends the daemon now.
                if let Some(r) = stop_reason(&stop) {
                    reason = r;
                    break;
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(poll_secs)) => {}
                    changed = stop.changed() => {
                        if changed.is_err() {
                            // Nobody can ask for a stop any more: the poll goes on.
                            tokio::time::sleep(Duration::from_secs(poll_secs)).await;
                        }
                    }
                }
                if let Some(r) = stop_reason(&stop) {
                    reason = r;
                    break;
                }
            }
            log(&sink, "stop", json!({ "rounds": n, "reason": reason }));
        }
    }
    Ok(())
}
