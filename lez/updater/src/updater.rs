//! One updater for one instance: each round it reads the instance from the chain, gathers
//! candidates from its sources, runs the program's own checks on each against the on-chain
//! store, picks the one that moves the instance furthest, and sends the next step of its job
//! (or of a job another updater left open). It keeps no store of its own: the chain is the
//! state, so a restart, or another updater's work, needs no repair. It keeps the full sync
//! committees the program only summarises (the signer witness needs them), keyed by the
//! summary's `pubkeys_root`, accepted only when they match the summaries on chain.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    io::Write as _,
    path::PathBuf,
};

use anyhow::{anyhow, bail, Context as _, Result};
use ferrata_core::{
    fork::LcFork,
    steps::{self, Job, Phase},
    types::{Execution, LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncCommittee},
    witness::{uncompressed_keys, CommitteeSummary, CompactBootstrap, CompactStore, CompactUpdate},
    Root,
};
use ferrata_lez_wire::{
    header_actor, instance_actor, instance_id, instance_seed, job_actor, job_id, Config, InstanceState, Message,
};
use lee::{AccountId, Actor, PrivateKey, PublicAccountEvidence, PublicKey};
use sha2::{Digest, Sha256};

use crate::{
    chain::{
        committee_key, decode_committee, decode_finality_update, decode_update, encode_committee, layout_for,
        signer_witness, signing_committee,
    },
    config::{ClientMatch, Fees, Policy},
    ledger::{Ledger, Tx, TxOutcome},
    source::{client_family, clip, Source},
};

/// One instance of the program.
#[derive(Clone, Debug)]
pub struct Instance {
    pub name: String,
    pub program: AccountId,
    pub config: Config,
    pub checkpoint: Root,
    pub id: Root,
}

impl Instance {
    pub fn new(name: &str, program: AccountId, config: Config, checkpoint: Root) -> Self {
        let id = instance_id(&config, &checkpoint);
        Self { name: name.to_string(), program, config, checkpoint, id }
    }
    pub fn actor(&self) -> Actor {
        instance_actor(self.program, &self.id)
    }
    pub fn job(&self, jid: &Root) -> Actor {
        job_actor(self.program, &self.id, jid)
    }
    pub fn header(&self, slot: u64) -> Actor {
        header_actor(self.program, &self.id, slot)
    }
    pub fn state(&self, ledger: &dyn Ledger) -> Result<Option<InstanceState>> {
        let s = ledger.actor_state(self.actor())?;
        if s.is_empty() {
            return Ok(None);
        }
        Ok(Some(borsh::from_slice(&s).context("instance state")?))
    }
    pub fn store(&self, ledger: &dyn Ledger) -> Result<Option<CompactStore>> {
        match self.state(ledger)? {
            Some(s) if s.active => Ok(Some(CompactStore::decode(&s.store).map_err(|e| anyhow!("store: {e:?}"))?)),
            _ => Ok(None),
        }
    }
    fn time_ms(&self, slot: u64) -> u64 {
        self.config.slot_start_ms(slot) + 12_000
    }
}

/// A step of the public job path (ferrata-advance's, shared with ferrata_attest_module).
pub use ferrata_advance::Step;

/// The transaction an updater means to send.
#[derive(Clone, Debug)]
pub struct Action {
    pub step: Step,
    pub job_id: Root,
    pub tx: Tx,
    /// The finalized slot the job's update finalizes (the checkpoint's for an initialization).
    pub finalized_slot: u64,
    /// The instance as the round saw it (active, store bytes): a step is moot once it changes.
    pub store_seen: Option<(bool, Vec<u8>)>,
    /// The timestamp validity window the program gives the transaction (`Init`, `Open`; the others: none), checked
    /// against the chain's clock before every attempt.
    pub window: Window,
}

/// A step is sent only if its validity window ([`Window`]) stays open this long past the chain's clock: the time a
/// transaction may take to reach a block (a sequencer's block interval, a retry).
pub const WINDOW_MARGIN_MS: u64 = 60_000;
/// A candidate update whose signature slot starts more than this after the chain's clock (`Ledger::now_ms`, the clock
/// the program checks `Open`'s window against) is signed in the future: no honest node serves one (a sync aggregate
/// exists only once its slot's block does), and the program refuses its `Open` until then. The signature does not cover
/// `signature_slot` (only its fork version), so a source can serve a real update re-dated to a later slot of the same
/// period, whose window has not opened (internal reviews' findings M4 and M1): such a candidate is refused and its
/// source demoted (`policy.demote_rounds`), and nothing else that source served in the round is used. The margin is the
/// skew between the chain's clock and Ethereum's that the updater tolerates, as for its windows ([`WINDOW_MARGIN_MS`]).
/// One re-dated within it is not refused (a few seconds of clock skew make an honest update look the same), but an
/// update whose `Open` window has not opened by the chain's clock never outranks one that can be sent now (the order in
/// `plan_inner`): a source that re-dates so, even again every round, holds back only an update no other source serves,
/// for as long as it alone serves it (an internal review's M1: it used to hold every update back until the period ended
/// when the re-dated update adopted the next committee). One re-dated to a slot that has started can be chosen, but
/// every step of its job that this updater sends before it carries the job, whoever opened it, waits a turn counted by
/// the instance's state, not by the job (a new copy is a new job, and anyone may open it: two internal
/// reviews' findings M1): one turn per kind of step and state, at most `policy.defer_window - 1` rounds. It is a
/// real update: applied, it moves the instance.
pub const FUTURE_MARGIN_MS: u64 = WINDOW_MARGIN_MS;
/// Refusals in a row, each of a step that stayed due after it (not a race another party won), after which this
/// updater sends nothing for [`BREAKER_PAUSE`] rounds. Each time it opens again before one of this updater's
/// transactions is applied, the pause doubles, up to [`BREAKER_MAX_PAUSE`] rounds; after a pause one step is tried.
/// It bounds the fees a refusal no check here foresees can burn when every round brings a new job (a source serving a
/// new finality update each slot).
pub const BREAKER_AFTER: u32 = 3;
/// See [`BREAKER_AFTER`].
pub const BREAKER_PAUSE: u64 = 8;
/// See [`BREAKER_AFTER`].
pub const BREAKER_MAX_PAUSE: u64 = 512;
/// A step refused for a reason that may pass (its window, by a clock that differs from the chain's; an unknown one; a
/// refusal after which the chain could not be read, [`After::Unread`]) is held [`REFUSAL_BACKOFF`] rounds, then twice
/// as long after each further refusal, up to [`MAX_REFUSAL_BACKOFF`] rounds, and sent again after every hold: it is
/// never given up silently (the breaker, [`BREAKER_AFTER`], bounds what the refusals cost). One that the program
/// refuses, or that ran out of gas, while it stayed due is held for good (out of gas: until `fees.gas_limit` is
/// raised): LEZ executes the same transaction on the same state the same way.
pub const REFUSAL_BACKOFF: u64 = 4;
/// See [`REFUSAL_BACKOFF`].
pub const MAX_REFUSAL_BACKOFF: u64 = 64;
/// A job this updater carries (`policy.finish_own_jobs`) is kept through a hold of its next step that will end, until
/// that step has been refused this many times; then this updater leaves it (another party may finish it, and the step
/// is still sent again if its update is chosen again), so that a refusal no check here explains does not pin the
/// updater to one job.
pub const LEAVE_JOB_AFTER: u32 = 3;
/// Sends in a row that did not land (rejected, refused while their step stayed due, or failed at transport after
/// every retry) after which `/health` says `down`; one is enough for `degraded`.
pub const DOWN_AFTER_FAILED_SENDS: u32 = 5;
/// Rounds in a row that failed (the round ended in an error: a read of the chain failed, every transport retry
/// failed, the budget is reached, the committees the store summarises could not be fetched) after which `/health` says
/// `down`; one is enough for `degraded`. A chain that answers neither reads nor sends fails every round before anything
/// is sent, so it never counts as a failing send.
pub const DOWN_AFTER_FAILED_ROUNDS: u32 = 5;
/// A hold that ends ([`Held::retry_at`]) is forgotten this many rounds after its last refusal. A hold for good is kept
/// while the instance holds the state its step was refused on (a job commits to its starting store, so the step is moot
/// once the instance moves), across restarts: `state_dir/<chain>/held.tsv` (9c L-U5: it used to be forgotten after
/// 4,096 rounds, about 13.7 h at 12 s rounds, and at every restart, and the step was refused and charged again). At
/// most [`MAX_HELD`] holds are kept.
pub const HOLD_ROUNDS: u64 = 4096;
/// See [`HOLD_ROUNDS`].
pub const MAX_HELD: usize = 1024;
/// Refusals after which the chain could not be read ([`After::Unread`]) kept to be judged once it can be read again; the
/// oldest beyond this is dropped unjudged.
pub const MAX_UNJUDGED: usize = 64;

/// Why the chain refused a transaction, as its outcome says (LEZ's note, the program's exit code).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Executed outside the timestamp window the program gave it ("Execution outside of the validity window"): LEZ
    /// charges the whole gas limit.
    Window,
    /// It used up the declared gas limit ("Execution exceeded its cycle budget").
    Gas,
    /// The program refused it with this exit code.
    Program(u32),
    /// Anything else.
    Other,
}

impl Refusal {
    pub fn of(out: &TxOutcome) -> Self {
        if out.note.contains("validity window") {
            Refusal::Window
        } else if out.note.contains("cycle budget") {
            Refusal::Gas
        } else if let Some(code) = out.exit_code {
            Refusal::Program(code)
        } else {
            Refusal::Other
        }
    }

    /// The metric label: `window`, `gas`, `program` or `other`.
    pub fn kind(&self) -> &'static str {
        match self {
            Refusal::Window => "window",
            Refusal::Gas => "gas",
            Refusal::Program(_) => "program",
            Refusal::Other => "other",
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Refusal::Window => "outside its validity window".into(),
            Refusal::Gas => "out of gas".into(),
            Refusal::Program(code) => format!("the program's exit {code}"),
            Refusal::Other => "refused".into(),
        }
    }

    /// LEZ executes the same transaction on the same state the same way: a program's refusal and an exhausted gas
    /// limit repeat; a window depends on the clock.
    fn repeats(&self) -> bool {
        matches!(self, Refusal::Gas | Refusal::Program(_))
    }
}

/// What a refused transaction left, read from the chain after it ([`Updater::submit`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After {
    /// The step was done or moot afterwards: another party's transaction came first. Nothing is held.
    Race,
    /// The step stayed due (no other party sent it, the store did not move): sent again, it would meet the same state.
    /// Held ([`REFUSAL_BACKOFF`]), a failing send, a strike towards the breaker.
    Due,
    /// The chain could not be read after the refusal, `policy.max_attempts` times with the transport backoff between.
    /// On the sequencer backend a read fails while another party's transaction touching the instance is in flight,
    /// which is when a race is lost: held as a cause that may pass is ([`REFUSAL_BACKOFF`] rounds the first time),
    /// whatever the cause, and judged at the next round that reads the chain ([`Updater`]'s `judge_unread`): a race
    /// then, its hold dropped; or a step that stayed due, then a failing send and a strike as [`After::Due`] (9c L-U3).
    Unread,
}

impl After {
    /// The metric label: `race`, `due` or `unread`.
    pub fn label(&self) -> &'static str {
        match self {
            After::Race => "race",
            After::Due => "due",
            After::Unread => "unread",
        }
    }
}

/// A step the chain refused that is not sent again for now: it stayed due after the refusal (no other party sent it,
/// the store did not move), so sent again it would meet the same state; or the chain could not be read after it.
#[derive(Clone, Debug)]
pub struct Held {
    pub step: Step,
    pub finalized_slot: u64,
    pub cause: Refusal,
    /// The chain's note, clipped.
    pub note: String,
    pub count: u32,
    /// Whether the chain, read after the last refusal, showed the step still due ([`After::Due`]; otherwise
    /// [`After::Unread`]).
    pub due: bool,
    /// The round of its last refusal.
    pub round: u64,
    /// The first round it may be sent again (`None`: not on this state).
    pub retry_at: Option<u64>,
    /// `fees.gas_limit` at its last refusal ([`Refusal::Gas`]: the hold ends when the limit is raised).
    pub gas_limit: u64,
    /// The instance's state the step was refused on ([`state_digest`]): the hold is forgotten once it changes.
    pub state: Root,
}

/// The SHA-256 of the instance's state as a round saw it ([`Action::store_seen`]: whether it is active, and its store;
/// none before `Init`).
pub fn state_digest(seen: &Option<(bool, Vec<u8>)>) -> Root {
    let mut h = Sha256::new();
    match seen {
        None => h.update([0u8]),
        Some((active, store)) => {
            h.update([1u8, u8::from(*active)]);
            h.update(store);
        }
    }
    h.finalize().into()
}

/// What the turn (`policy.defer_window`) of an `Open`, and of any step of a job this updater does not carry, is counted
/// by: the instance's state ([`state_digest`], as [`Action::store_seen`] holds it), which no source can change, not the
/// job, whose id a source can change (it hashes `signature_slot`, which the signature does not cover) and whose job
/// anyone may open (two internal reviews' findings M1).
fn state_turn(state: &InstanceState) -> Root {
    state_digest(&Some((state.active, state.store.clone())))
}

fn step_of_byte(b: u8) -> Step {
    match b {
        0 => Step::Init,
        1 => Step::Open,
        2 => Step::Miller,
        255 => Step::Finish,
        p => Step::Adopt(p - 3),
    }
}

/// A line of `held.tsv` ([`Updater`]'s `held_text`): a hold for good, as of round 0 of this run.
fn parse_held(line: &str) -> Option<((Root, Step), Held)> {
    let f: Vec<&str> = line.splitn(9, '\t').collect();
    if f.len() != 9 {
        return None;
    }
    let jid = crate::chain::root(f[0]).ok()?;
    let step = step_of_byte(f[1].parse().ok()?);
    let cause = match f[3] {
        "window" => Refusal::Window,
        "gas" => Refusal::Gas,
        "other" => Refusal::Other,
        c => Refusal::Program(c.strip_prefix("program:")?.parse().ok()?),
    };
    let held = Held {
        step,
        finalized_slot: f[2].parse().ok()?,
        cause,
        note: clip(f[8]),
        count: f[4].parse().ok()?,
        due: f[5] == "1",
        round: 0,
        retry_at: None,
        gas_limit: f[6].parse().ok()?,
        state: crate::chain::root(f[7]).ok()?,
    };
    Some(((jid, step), held))
}

/// Refusals in a row of steps that stayed due, and the pause they set ([`BREAKER_AFTER`]).
#[derive(Clone, Debug, Default)]
pub struct Breaker {
    pub strikes: u32,
    /// Times it opened since this updater's last applied transaction.
    pub trips: u32,
    /// Nothing is sent before this round.
    pub paused_until: u64,
    /// The last such refusal, described.
    pub last: String,
}

/// What a round decided.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // one per round; boxing it buys nothing
pub enum Plan {
    /// The instance holds the best verified update every source serves.
    UpToDate,
    /// A step is due; this updater waits its turn (another may send it).
    Deferred(Step),
    /// Fewer sources than `min_agreeing` serve the chosen update's finalized header.
    Waiting(String),
    /// Nothing can be done this round (no source answered, no committee, ...).
    Stuck(String),
    Send(Action),
}

/// Per source.
#[derive(Clone, Debug, Default)]
pub struct SourceStats {
    pub requests: u64,
    pub errors: u64,
    /// Candidates it served that the program would refuse (a forged signature, a wrong
    /// bootstrap, a broken branch).
    pub refused: u64,
    /// The last thing it served that the program would refuse (or signed in the future), described.
    pub last_refused: String,
    /// Rounds it was found lagging.
    pub lagging_rounds: u64,
    pub lagging: bool,
    /// It answered last round.
    pub available: bool,
    pub demoted_until: u64,
    /// The newest finalized slot among the candidates it served last round (a headers source: the newest it
    /// confirmed).
    pub finalized_slot: u64,
    /// A headers source: the times it reported another block (or none) canonical at a finalized header's slot.
    pub mismatches: u64,
    /// ... and whether its last answer was such.
    pub mismatch: bool,
    /// A headers source: the finalized headers it confirmed.
    pub confirmed: u64,
    /// `policy.min_distinct_clients`: the client it last reported at `/eth/v1/node/version` (empty: none yet).
    pub client: String,
    /// ... the times that report changed (a provider behind a load balancer may answer from nodes of several clients).
    pub client_changes: u64,
    /// ... the updates for which it served other bytes than another source (the same finality update, by attested
    /// header and signature slot, or the same finished period), each counted once.
    pub disagreements: u64,
    /// ... and this round's, described.
    pub disagree: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub rounds: u64,
    /// (step, result) -> transactions: result `applied`, `refused`, `rejected`.
    pub tx: BTreeMap<(String, &'static str), u64>,
    pub refused_codes: BTreeMap<u32, u64>,
    /// Refused transactions by cause ([`Refusal::kind`]) and by what followed ([`After::label`]): `race` (the step was
    /// done or moot afterwards: another party's transaction came first), `due` (it stayed due: held,
    /// [`Updater::held`]) or `unread` (the chain could not be read after it: held for a while, and judged later).
    pub refusals: BTreeMap<(&'static str, &'static str), u64>,
    /// The `unread` refusals as judged once the chain could be read again: `race` or `due`.
    pub unread_judged: BTreeMap<&'static str, u64>,
    pub gas_applied: u64,
    pub gas_refused: u64,
    pub fee_applied: u128,
    pub fee_refused: u128,
    pub transport_errors: u64,
    pub retries: u64,
    pub deferrals: u64,
    /// Steps it planned that another updater sent first (seen on chain before sending).
    pub skipped_done: u64,
    /// Jobs whose last step (it applied the update) it sent.
    pub jobs_finished: u64,
    pub budget_stops: u64,
    pub finalized_slot: u64,
    pub period: u64,
    pub last_plan: String,
    pub last_error: String,
    /// Sends since this updater's last applied transaction that did not land: rejected, refused while the step stayed
    /// due, or failed at transport after every retry (a refusal that lost a race to another party is none of these).
    pub failing_sends: u32,
    /// The last of them, described.
    pub last_failure: String,
    /// Rounds in a row that ended in an error ([`Updater::tick`] returned one: a read of the chain failed, every
    /// transport retry failed, the budget is reached); a round without an error resets it.
    pub failed_rounds: u32,
    /// This updater's last applied transaction: its round and time (UTC).
    pub last_applied_round: u64,
    pub last_applied_utc: String,
    /// The furthest finalized slot the sources serve in an update the program would not refuse (verified, or signed by
    /// a committee the store does not hold yet), as of the last round that asked them.
    pub sources_head: u64,
    /// The round in which the instance's finalized slot last moved (or the instance was first seen active; 0: not yet).
    pub progress_round: u64,
    /// `policy.min_distinct_clients`: this round waits for clients, and the furthest update the sources serve that the
    /// program would not refuse finalizes this many slots past the instance (`None`: not waiting for clients). Beyond
    /// `max_lag_slots`, `/health` says so.
    pub client_wait: Option<u64>,
    pub sources: BTreeMap<String, SourceStats>,
}

impl Metrics {
    pub fn spent(&self) -> u128 {
        self.fee_applied + self.fee_refused
    }
    pub fn count(&self, result: &str) -> u64 {
        self.tx.iter().filter(|((_, r), _)| *r == result).map(|(_, n)| n).sum()
    }
}

/// A candidate update, as one source served it.
struct Candidate {
    source: usize,
    update: LightClientUpdate,
    period_update: bool,
    /// As the source served it (kept for a job this updater opens: `finish_own_jobs`).
    raw: Vec<u8>,
}

// What the program would do with a candidate, and the job of an accepted one; a step's validity window:
// ferrata-advance's.
use ferrata_advance::{Planned, Verdict, Window};

/// What two sources must serve byte for byte when both serve it (`policy.min_distinct_clients`): a finality update,
/// by its attested header's root and its signature slot (one block's sync aggregate over its parent: honest nodes
/// derive the same bytes), or the update of a finished sync-committee period (`true`, zero, period: the best update of
/// a period no longer changes once the period is over).
type UpdateKey = (bool, Root, u64);

pub struct Updater {
    /// The daemon's name (logs, metrics).
    pub label: String,
    pub inst: Instance,
    payer: PrivateKey,
    pub payer_id: AccountId,
    tag: [u8; 32],
    pub fees: Fees,
    pub policy: Policy,
    pub sources: Vec<Box<dyn Source>>,
    committees: BTreeMap<Root, SyncCommittee>,
    bootstrap: Option<LightClientBootstrap>,
    state_dir: Option<PathBuf>,
    pub metrics: Metrics,
    round: u64,
    /// Per (what the turn is counted by, step): the first round of this updater's turn to send that step, and the wait
    /// fixed for the turn (`None`: drawn from the job, [`Updater::defer`]).
    first_due: BTreeMap<(Root, Step), (u64, Option<u64>)>,
    /// Updates of jobs this updater sent a step for (its own `Open`, or a later step of a job another party opened),
    /// by job id: (period update, the bytes a source served). Until the job is finished or moot
    /// (`policy.finish_own_jobs`): a job is never left after this updater has paid for a step of it.
    carried: BTreeMap<Root, (bool, Vec<u8>)>,
    /// The job of the step this round chose and its update, kept once the step lands (or another party's step of the
    /// same job came first).
    sending: Option<(Root, bool, Vec<u8>)>,
    /// (headers source, slot) -> the root it confirmed canonical there (a finalized block stays canonical).
    confirmed: BTreeMap<(usize, u64), Root>,
    /// Per source, the client family it reported this round (`policy.min_distinct_clients`; `None`: none).
    clients: Vec<Option<String>>,
    /// Disagreements already counted: (what was served, source).
    disagreed: BTreeSet<(UpdateKey, usize)>,
    /// `client_match = "bytes"`: per update (the SHA-256 of the bytes as served), the (client family, source) pairs that
    /// served exactly those bytes, this round or an earlier one: implementations differ in which finality update they
    /// keep serving (measured 18 November 2026: Lodestar serves the first update of a newly finalized checkpoint for
    /// the whole epoch, Nimbus the newest, so the two serve the same bytes during one slot), so the match need not
    /// fall in one round. At most 4096 updates are kept.
    served_by: BTreeMap<Root, BTreeSet<(String, usize)>>,
    /// Steps refused while they stayed due (or with the chain unreadable after the refusal), by (job, step): not sent
    /// again on the same state, or not yet ([`Held::retry_at`]).
    pub held: BTreeMap<(Root, Step), Held>,
    /// What `held.tsv` holds (the holds for good as last written), so it is written only when they change.
    held_saved: String,
    /// Refusals after which the chain could not be read ([`After::Unread`]): the action and its cause, judged at the next
    /// round that reads the chain. At most [`MAX_UNJUDGED`].
    unjudged: Vec<(Action, Refusal, String)>,
    pub breaker: Breaker,
    /// The instance can no longer advance by the chain's clock: its finalized header (before `Init`: the checkpoint)
    /// is past its weak-subjectivity period, so the program refuses every `Open` (`Init`) from now on. (The finalized
    /// slot it was found at, 0 before `Init`; why, with what to do.) Cleared when a step's window admits it again (a
    /// clock put right) or the instance moves (a job already open finished). `/health` says `down` while it is set.
    pub expired: Option<(u64, String)>,
    /// How a backoff waits (milliseconds); tests pass one that does not sleep.
    pub sleep: fn(u64),
    /// Log lines to stderr.
    pub verbose: bool,
}

fn real_sleep(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms))
}

impl Updater {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        label: &str,
        inst: Instance,
        payer: PrivateKey,
        fees: Fees,
        policy: Policy,
        sources: Vec<Box<dyn Source>>,
        state_dir: Option<PathBuf>,
    ) -> Result<Self> {
        let pk = PublicKey::new_from_private_key(&payer);
        let payer_id = AccountId::from(&pk);
        let tag: [u8; 32] = Sha256::digest(format!("{label}/{payer_id:?}").as_bytes()).into();
        let mut me = Self {
            label: label.to_string(),
            inst,
            payer,
            payer_id,
            tag,
            fees,
            policy,
            sources,
            committees: BTreeMap::new(),
            bootstrap: None,
            state_dir,
            metrics: Metrics::default(),
            round: 0,
            first_due: BTreeMap::new(),
            carried: BTreeMap::new(),
            sending: None,
            confirmed: BTreeMap::new(),
            clients: Vec::new(),
            disagreed: BTreeSet::new(),
            served_by: BTreeMap::new(),
            held: BTreeMap::new(),
            held_saved: String::new(),
            unjudged: Vec::new(),
            breaker: Breaker::default(),
            expired: None,
            sleep: real_sleep,
            verbose: false,
        };
        for s in &me.sources {
            me.metrics.sources.insert(s.name().to_string(), SourceStats::default());
        }
        me.load_cache()?;
        Ok(me)
    }

    fn log(&self, line: &str) {
        if self.verbose {
            eprintln!("{} [{} {}] {line}", crate::utc_now(), self.label, self.inst.name);
        }
    }

    // ---- the cache ------------------------------------------------------------------------

    fn dir(&self) -> Option<PathBuf> {
        self.state_dir.as_ref().map(|d| d.join(&self.inst.name))
    }

    fn load_cache(&mut self) -> Result<()> {
        let Some(dir) = self.dir() else { return Ok(()) };
        fs::create_dir_all(dir.join("committees"))?;
        if let Ok(b) = fs::read(dir.join("bootstrap.ssz")) {
            let fork = self.inst.config.fork().ok_or_else(|| anyhow!("config: no layout"))?;
            if let Ok(bootstrap) = LightClientBootstrap::decode(&b, fork, self.inst.config.sync_committee_size as usize)
            {
                if self.bootstrap_verifies(&bootstrap) {
                    self.bootstrap = Some(bootstrap);
                }
            }
        }
        fs::create_dir_all(dir.join("open"))?;
        for e in fs::read_dir(dir.join("open"))? {
            let path = e?.path();
            let jid = path.file_stem().and_then(|s| s.to_str()).and_then(|s| crate::chain::root(s).ok());
            if let (Some(jid), Ok(b)) = (jid, fs::read(&path)) {
                if let Some((&kind, raw)) = b.split_first() {
                    self.carried.insert(jid, (kind == 1, raw.to_vec()));
                }
            }
        }
        for e in fs::read_dir(dir.join("committees"))? {
            let path = e?.path();
            if let Ok(c) = fs::read(&path).map_err(anyhow::Error::from).and_then(|b| decode_committee(&b)) {
                // Kept under the name of its own summary, or not kept at all.
                if path.file_stem().and_then(|s| s.to_str()) == Some(hex::encode(committee_key(&c)).as_str()) {
                    self.committees.insert(committee_key(&c), c);
                }
            }
        }
        if let Ok(text) = fs::read_to_string(dir.join("held.tsv")) {
            self.held.extend(text.lines().filter_map(parse_held).take(MAX_HELD));
            self.held_saved = self.held_text();
        }
        Ok(())
    }

    /// The holds for good, a line each (tab-separated: job, step, finalized slot, cause, refusals, due, gas limit, the
    /// instance's state, the chain's note), as `held.tsv` keeps them.
    fn held_text(&self) -> String {
        let mut o = String::new();
        for ((jid, step), h) in self.held.iter().filter(|(_, h)| h.retry_at.is_none()) {
            let cause = match h.cause {
                Refusal::Program(code) => format!("program:{code}"),
                c => c.kind().to_string(),
            };
            let _ = writeln!(
                o,
                "{}\t{}\t{}\t{cause}\t{}\t{}\t{}\t{}\t{}",
                hex::encode(jid),
                step.byte(),
                h.finalized_slot,
                h.count,
                u8::from(h.due),
                h.gas_limit,
                hex::encode(h.state),
                h.note
            );
        }
        o
    }

    /// Write `state_dir/<chain>/held.tsv` if the holds for good changed (9c L-U5).
    fn save_held(&mut self) {
        let text = self.held_text();
        if text == self.held_saved {
            return;
        }
        if let Some(dir) = self.dir() {
            if let Err(e) = fs::write(dir.join("held.tsv"), &text) {
                self.log(&format!("held.tsv: {e}"));
                return;
            }
        }
        self.held_saved = text;
    }

    fn keep_committee(&mut self, c: &SyncCommittee) -> Result<()> {
        let key = committee_key(c);
        if self.committees.contains_key(&key) {
            return Ok(());
        }
        if let Some(dir) = self.dir() {
            fs::write(dir.join("committees").join(format!("{}.bin", hex::encode(key))), encode_committee(c))?;
        }
        self.committees.insert(key, c.clone());
        Ok(())
    }

    fn carry(&mut self, jid: Root, period_update: bool, raw: Vec<u8>) {
        if self.carried.contains_key(&jid) {
            return;
        }
        if let Some(dir) = self.dir() {
            let mut b = vec![u8::from(period_update)];
            b.extend_from_slice(&raw);
            let _ = fs::write(dir.join("open").join(format!("{}.bin", hex::encode(jid))), b);
        }
        self.carried.insert(jid, (period_update, raw));
    }

    fn drop_carried(&mut self, jid: &Root) {
        if self.carried.remove(jid).is_some() {
            if let Some(dir) = self.dir() {
                let _ = fs::remove_file(dir.join("open").join(format!("{}.bin", hex::encode(jid))));
            }
        }
    }

    /// One line per transaction: round, step, job, finalized slot, result, exit code, execution and storage gas,
    /// fee, block, the time it was settled (UTC), its label, the chain's note (a refusal's reason; clipped).
    fn journal(&self, action: &Action, out: &TxOutcome) {
        let Some(dir) = self.dir() else { return };
        let line = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            self.round,
            action.step.name(),
            hex::encode(action.job_id),
            action.finalized_slot,
            if out.applied {
                "applied"
            } else if out.rejected {
                "rejected"
            } else {
                "refused"
            },
            out.exit_code.map(|c| c.to_string()).unwrap_or_default(),
            out.gas_exec,
            out.gas_stor,
            out.fee,
            out.block,
            crate::utc_now(),
            action.tx.label,
            clip(&out.note)
        );
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(dir.join("journal.tsv")) {
            let _ = f.write_all(line.as_bytes());
        }
    }

    // ---- sources --------------------------------------------------------------------------

    fn usable(&self, i: usize) -> bool {
        let name = self.sources[i].name();
        self.metrics.sources.get(name).is_none_or(|s| s.demoted_until <= self.round)
    }

    fn stats(&mut self, i: usize) -> &mut SourceStats {
        let name = self.sources[i].name().to_string();
        self.metrics.sources.entry(name).or_default()
    }

    fn demote(&mut self, i: usize, why: &str) {
        let until = self.round + self.policy.demote_rounds;
        let name = self.sources[i].name().to_string();
        let s = self.stats(i);
        s.refused += 1;
        s.demoted_until = until;
        s.last_refused = clip(why);
        self.log(&format!(
            "source {name} served what the program would refuse ({why}); not asked for {} rounds",
            self.policy.demote_rounds
        ));
    }

    fn bootstrap_verifies(&self, b: &LightClientBootstrap) -> bool {
        let config = &self.inst.config;
        let Some(fork) = config.fork() else { return false };
        steps::initialize(&self.inst.checkpoint, &CompactBootstrap::of(b), fork, &config.spec(), config.adoption_parts)
            .is_ok()
    }

    /// The bootstrap of the instance's checkpoint: the first source's that the program's `Init`
    /// checks accept (a source serving another is demoted).
    fn ensure_bootstrap(&mut self) -> Result<()> {
        if self.bootstrap.is_some() {
            return Ok(());
        }
        let fork = self.inst.config.fork().ok_or_else(|| anyhow!("config: no layout"))?;
        let n = self.inst.config.sync_committee_size as usize;
        for i in 0..self.sources.len() {
            if !self.usable(i) || !self.sources[i].light_client() {
                continue;
            }
            self.stats(i).requests += 1;
            let checkpoint = self.inst.checkpoint;
            let answer = self.sources[i].bootstrap(&checkpoint);
            self.stats(i).available = answer.is_ok();
            match answer {
                Ok(bytes) => match LightClientBootstrap::decode(&bytes, fork, n) {
                    Ok(b) if self.bootstrap_verifies(&b) => {
                        if let Some(dir) = self.dir() {
                            fs::write(dir.join("bootstrap.ssz"), &bytes)?;
                        }
                        self.keep_committee(&b.current_sync_committee)?;
                        self.bootstrap = Some(b);
                        return Ok(());
                    }
                    Ok(_) => self.demote(i, "a bootstrap that does not match the checkpoint"),
                    Err(e) => self.demote(i, &format!("a bootstrap that does not decode: {e:?}")),
                },
                Err(e) => {
                    self.stats(i).errors += 1;
                    self.log(&format!("bootstrap: {e:#}"));
                }
            }
        }
        bail!("no source served the checkpoint's bootstrap")
    }

    /// The full committees the store summarises, from the cache, the bootstrap, or the sources'
    /// period updates (accepted only when they match the summaries on chain).
    fn ensure_committees(&mut self, store: &CompactStore) -> Result<()> {
        let mut needed = vec![store.current];
        needed.extend(store.next);
        let missing = |me: &Self| needed.iter().filter(|s| !me.committees.contains_key(&s.pubkeys_root)).count();
        if missing(self) == 0 {
            return Ok(());
        }
        if let Ok(()) = self.ensure_bootstrap() {
            let c = self.bootstrap.as_ref().expect("set").current_sync_committee.clone();
            if needed.contains(&CommitteeSummary::of(&c)) {
                self.keep_committee(&c)?;
            }
        }
        let spec = self.inst.config.spec();
        let period = spec.period_at_slot(store.finalized.slot);
        for i in 0..self.sources.len() {
            if missing(self) == 0 {
                break;
            }
            if !self.usable(i) || !self.sources[i].light_client() {
                continue;
            }
            self.stats(i).requests += 1;
            match self.sources[i].updates(period.saturating_sub(1), 2) {
                Ok(list) => {
                    for bytes in list {
                        if let Ok((u, _)) = decode_update(&bytes, &self.inst.config) {
                            if !u.next_sync_committee.is_empty()
                                && needed.contains(&CommitteeSummary::of(&u.next_sync_committee))
                            {
                                self.keep_committee(&u.next_sync_committee)?;
                            }
                        }
                    }
                }
                Err(_) => self.stats(i).errors += 1,
            }
        }
        if missing(self) > 0 {
            bail!("the store's committees are not served by any source (period {period})");
        }
        Ok(())
    }

    fn candidates(&mut self, store: &CompactStore) -> Vec<Candidate> {
        let spec = self.inst.config.spec();
        let period = spec.period_at_slot(store.finalized.slot);
        let mut out = Vec::new();
        for i in 0..self.sources.len() {
            if !self.usable(i) || !self.sources[i].light_client() {
                continue;
            }
            let errors = self.stats(i).errors;
            self.stats(i).requests += 1;
            let mut head = period + 1;
            match self.sources[i].finality_update() {
                Ok(bytes) => match decode_finality_update(&bytes, &self.inst.config) {
                    Ok((u, _)) => {
                        head = head.max(spec.period_at_slot(u.attested_header.beacon.slot));
                        out.push(Candidate { source: i, update: u, period_update: false, raw: bytes });
                    }
                    Err(e) => self.demote(i, &format!("a finality update that does not decode: {e:#}")),
                },
                Err(_) => self.stats(i).errors += 1,
            }
            let count = (head - period + 1).min(self.policy.max_periods_per_request);
            self.stats(i).requests += 1;
            match self.sources[i].updates(period, count) {
                Ok(list) => {
                    for bytes in list {
                        match decode_update(&bytes, &self.inst.config) {
                            Ok((u, _)) => out.push(Candidate { source: i, update: u, period_update: true, raw: bytes }),
                            Err(e) => self.demote(i, &format!("an update that does not decode: {e:#}")),
                        }
                    }
                }
                Err(_) => self.stats(i).errors += 1,
            }
            let answered = self.stats(i).errors < errors + 2;
            self.stats(i).available = answered;
        }
        out
    }

    /// `policy.min_distinct_clients`: ask every usable source which client it runs (once a round: a provider behind a
    /// load balancer can answer from several nodes, and a change is counted).
    fn refresh_clients(&mut self) {
        self.clients = vec![None; self.sources.len()];
        for i in 0..self.sources.len() {
            if !self.usable(i) {
                continue;
            }
            self.stats(i).requests += 1;
            match self.sources[i].client() {
                Ok(version) => {
                    let name = self.sources[i].name().to_string();
                    let s = self.stats(i);
                    let before = std::mem::replace(&mut s.client, version.clone());
                    if !before.is_empty() && before != version {
                        s.client_changes += 1;
                        self.log(&format!("source {name} now reports {version} (was {before})"));
                    }
                    self.clients[i] = client_family(&version);
                }
                Err(e) => {
                    self.stats(i).errors += 1;
                    self.log(&format!("client: {e:#}"));
                }
            }
        }
    }

    /// The client family source `i` reported this round.
    fn family(&self, i: usize) -> Option<String> {
        self.clients.get(i).cloned().flatten()
    }

    /// Sources that served different bytes for the same update (`UpdateKey`): logged, counted once per update and
    /// source, and named in `/health` for the round. Who is right is not decided here; an update the program would
    /// refuse demotes its source as always.
    fn note_disagreements(&mut self, candidates: &[Candidate]) {
        let spec = self.inst.config.spec();
        for i in 0..self.sources.len() {
            self.stats(i).disagree = None;
        }
        let head = candidates.iter().map(|c| spec.period_at_slot(c.update.attested_header.beacon.slot)).max();
        let mut served: BTreeMap<UpdateKey, Vec<(usize, Root, u64, usize)>> = BTreeMap::new();
        for c in candidates {
            let key = if c.period_update {
                let period = spec.period_at_slot(c.update.attested_header.beacon.slot);
                if Some(period) >= head {
                    continue; // the period is not over: its best update can still change
                }
                (true, [0u8; 32], period)
            } else {
                (false, c.update.attested_header.beacon.hash_tree_root(), c.update.signature_slot)
            };
            let digest: Root = Sha256::digest(&c.raw).into();
            let signers = c.update.sync_aggregate.bits.iter().filter(|b| **b).count();
            served.entry(key).or_default().push((c.source, digest, c.update.finalized_header.beacon.slot, signers));
        }
        if self.disagreed.len() > 4096 {
            self.disagreed.clear();
        }
        for (key, list) in served {
            if list.iter().map(|x| x.1).collect::<BTreeSet<_>>().len() < 2 {
                continue;
            }
            let what = if key.0 {
                format!("the period-{} update", key.2)
            } else {
                format!("the finality update attested at 0x{}, signed at slot {}", &hex::encode(key.1)[..16], key.2)
            };
            let line: Vec<String> = list
                .iter()
                .map(|&(i, digest, finalized, signers)| {
                    format!(
                        "{} ({}): sha256 {}…, finalized slot {finalized}, {signers} signers",
                        self.sources[i].name(),
                        self.family(i).unwrap_or_else(|| "no client".into()),
                        &hex::encode(digest)[..16]
                    )
                })
                .collect();
            self.log(&format!("sources disagree on {what}: {}", line.join("; ")));
            for &(i, digest, ..) in &list {
                let others: Vec<&str> =
                    list.iter().filter(|x| x.1 != digest).map(|x| self.sources[x.0].name()).collect();
                let note = format!("serves other bytes than {} for {what}", others.join(", "));
                let new = self.disagreed.insert((key, i));
                let s = self.stats(i);
                if new {
                    s.disagreements += 1;
                }
                s.disagree = Some(note);
            }
        }
    }

    /// [`FUTURE_MARGIN_MS`]: the candidates of the sources that served none signed in the future by the chain's clock
    /// (`now_ms`; `None`: each transaction carries its own time, so nothing is in the future), and the first refusal
    /// described (empty: none). A source that served one is demoted, and none of its candidates is kept this round.
    fn refuse_future(&mut self, candidates: Vec<Candidate>, now_ms: Option<u64>) -> (Vec<Candidate>, String) {
        let Some(now) = now_ms else { return (candidates, String::new()) };
        let mut liars: BTreeMap<usize, String> = BTreeMap::new();
        for c in &candidates {
            let start = self.inst.config.slot_start_ms(c.update.signature_slot);
            if start <= now.saturating_add(FUTURE_MARGIN_MS) || liars.contains_key(&c.source) {
                continue;
            }
            let kind = if c.period_update { "period update" } else { "finality update" };
            let why = format!(
                "a {kind} finalizing slot {} signed at slot {}, which starts {} s after the chain's clock ({}): signed in \
                 the future",
                c.update.finalized_header.beacon.slot,
                c.update.signature_slot,
                (start - now) / 1000,
                crate::utc(now / 1000)
            );
            liars.insert(c.source, why);
        }
        let first = liars.values().next().cloned().unwrap_or_default();
        for (&i, why) in &liars {
            self.demote(i, why);
        }
        (candidates.into_iter().filter(|c| !liars.contains_key(&c.source)).collect(), first)
    }

    fn evaluate(&self, store: &CompactStore, update: &LightClientUpdate) -> Verdict {
        let current = self.committees.get(&store.current.pubkeys_root);
        let next = store.next.as_ref().and_then(|n| self.committees.get(&n.pubkeys_root));
        ferrata_advance::evaluate(&self.inst.config, &self.inst.id, store, update, current, next)
    }

    // ---- a round --------------------------------------------------------------------------

    /// Whether this updater waits its turn to send `step` of job `jid` (`policy.defer_window`): `H(tag, jid, step) mod
    /// defer_window` rounds from the first round the step was due under `since` (an `Open`, or a step of a job this
    /// updater does not carry: the instance's state, as [`state_turn`] gives it; a step of a job it carries: the job),
    /// or the wait fixed for the turn (after this updater sent such a step there: a whole turn, [`Updater::submit`]).
    fn defer(&mut self, jid: &Root, since: &Root, step: Step) -> bool {
        let (due, fixed) = *self.first_due.entry((*since, step)).or_insert((self.round, None));
        let wait = fixed.unwrap_or_else(|| {
            let mut h = Sha256::new();
            h.update(self.tag);
            h.update(jid);
            h.update([step.byte()]);
            let d: [u8; 32] = h.finalize().into();
            u64::from_le_bytes(d[..8].try_into().expect("8")) % self.policy.defer_window
        });
        if self.round.saturating_sub(due) < wait {
            self.metrics.deferrals += 1;
            return true;
        }
        false
    }

    /// Read the chain and the sources; decide this round's transaction.
    pub fn plan(&mut self, ledger: &dyn Ledger) -> Result<Plan> {
        self.round += 1;
        self.metrics.rounds += 1;
        if self.first_due.len() > 4096 {
            let horizon = self.round.saturating_sub(1024);
            self.first_due.retain(|_, (r, _)| *r >= horizon);
        }
        let round = self.round;
        self.held.retain(|_, h| h.retry_at.is_none() || h.round + HOLD_ROUNDS > round);
        self.judge_unread(ledger);
        let mut plan = self.plan_inner(ledger);
        self.save_held();
        if matches!(plan, Ok(Plan::Send(_))) && self.round < self.breaker.paused_until {
            plan = Ok(Plan::Stuck(self.paused()));
        }
        if !matches!(plan, Ok(Plan::Waiting(_))) {
            self.metrics.client_wait = None;
        }
        self.metrics.last_plan = match &plan {
            Ok(Plan::UpToDate) => "up-to-date".into(),
            Ok(Plan::Deferred(s)) => format!("deferred {}", s.name()),
            Ok(Plan::Waiting(w)) => format!("waiting: {w}"),
            Ok(Plan::Stuck(w)) => format!("stuck: {w}"),
            Ok(Plan::Send(a)) => format!("send {}", a.tx.label),
            Err(e) => format!("error: {e:#}"),
        };
        if let Err(e) = &plan {
            self.metrics.last_error = format!("{e:#}");
        }
        plan
    }

    fn plan_inner(&mut self, ledger: &dyn Ledger) -> Result<Plan> {
        let config = self.inst.config.clone();
        let spec = config.spec();
        let fork = config.fork().ok_or_else(|| anyhow!("config: no layout"))?;
        let state = self.inst.state(ledger)?;
        // A step held on another state of the instance is moot (a job commits to the store it starts from): forgotten.
        let seen = state_digest(&state.as_ref().map(|s| (s.active, s.store.clone())));
        self.held.retain(|_, h| h.state == seen);
        let Some(state) = state.filter(|s| s.active) else {
            // Not initialized, or its committee not adopted yet.
            if let Err(e) = self.ensure_bootstrap() {
                return Ok(Plan::Stuck(format!("{e:#}")));
            }
            let bootstrap = self.bootstrap.clone().expect("set");
            let compact = CompactBootstrap::of(&bootstrap);
            let (store, job) = steps::initialize(&self.inst.checkpoint, &compact, fork, &spec, config.adoption_parts)
                .map_err(|e| anyhow!("init: {e:?}"))?;
            let jid = job_id(&self.inst.id, &job.store_digest, &job.update_digest);
            let job_actor = self.inst.job(&jid);
            let time_ms = self.inst.time_ms(bootstrap.header.beacon.slot);
            let finalized_slot = store.finalized.slot;
            let step = if self.inst.state(ledger)?.is_none() {
                Step::Init
            } else {
                match self.job_on_chain(ledger, &jid)? {
                    Some(job) => match job.adoption.as_ref().filter(|a| !a.complete) {
                        Some(a) => Step::Adopt(a.chunk_roots.len() as u8),
                        None => return Ok(Plan::Stuck("an initialization job without a pending adoption".into())),
                    },
                    None => return Ok(Plan::Stuck("an inactive instance without its job".into())),
                }
            };
            let window = match step {
                Step::Init => ferrata_advance::init_window(&config, bootstrap.header.beacon.slot),
                _ => Window::ANY,
            };
            if let Some((late, why)) = self.outside(ledger, &window, time_ms) {
                return Ok(if late {
                    let why = format!(
                        "Init not sent: {why}: the checkpoint's weak-subjectivity period ends there, and no instance can \
                         be created from it after that: initialise from a newer checkpoint"
                    );
                    self.expired = Some((0, why.clone()));
                    Plan::Stuck(why)
                } else {
                    Plan::Waiting(format!("Init not sent: {why}"))
                });
            }
            self.expired = None;
            if let Some(why) = self.held_back(&jid, step) {
                return Ok(Plan::Stuck(why));
            }
            if self.defer(&jid, &jid, step) {
                return Ok(Plan::Deferred(step));
            }
            let tx = match step {
                Step::Init => Tx {
                    label: format!("{} Init", self.inst.name),
                    root: self.inst.actor(),
                    message: borsh::to_vec(&Message::Init {
                        config: config.clone(),
                        checkpoint: self.inst.checkpoint,
                        bootstrap: compact.encode(fork),
                    })?,
                    actors: vec![self.inst.actor(), job_actor],
                    evidence: vec![PublicAccountEvidence::Pda {
                        program: self.inst.program,
                        seed: instance_seed(&self.inst.id),
                    }],
                    time_ms,
                },
                Step::Adopt(p) => {
                    let keys = uncompressed_keys(&bootstrap.current_sync_committee).map_err(|e| anyhow!("{e:?}"))?;
                    self.adopt_tx(job_actor, &keys, p, Some(finalized_slot), time_ms, "init")
                }
                _ => unreachable!(),
            };
            let store_seen = self.inst.state(ledger)?.map(|s| (s.active, s.store));
            return Ok(Plan::Send(Action { step, job_id: jid, tx, finalized_slot, store_seen, window }));
        };

        let store = CompactStore::decode(&state.store).map_err(|e| anyhow!("store: {e:?}"))?;
        if self.metrics.progress_round == 0 || store.finalized.slot != self.metrics.finalized_slot {
            self.metrics.progress_round = self.round;
        }
        if self.expired.as_ref().is_some_and(|(slot, _)| *slot != store.finalized.slot) {
            self.expired = None;
        }
        self.metrics.finalized_slot = store.finalized.slot;
        self.metrics.period = spec.period_at_slot(store.finalized.slot);
        // Without the committees the store summarises no update can be judged or sent: the round ends in an error, so
        // that rounds in a row of it count towards `down` (DOWN_AFTER_FAILED_ROUNDS) as an unreadable chain's do (9c
        // L-U2: it used to end `Stuck`, before the sources' head was read, and `/health` stayed `degraded` for good).
        self.ensure_committees(&store)?;
        if self.policy.finish_own_jobs {
            if let Some(plan) = self.continue_carried(ledger, &store, &state)? {
                return Ok(plan);
            }
        }
        let k = self.policy.min_distinct_clients;
        if k >= 2 {
            self.refresh_clients();
        }
        let candidates = self.candidates(&store);
        if candidates.is_empty() {
            return Ok(Plan::Stuck("no source answered".into()));
        }
        let (candidates, future) = self.refuse_future(candidates, ledger.now_ms());
        if candidates.is_empty() {
            return Ok(Plan::Stuck(format!(
                "every source that answered served an update signed in the future ({future}) and is demoted; when every \
                 source is demoted so, the chain's clock may be behind Ethereum's"
            )));
        }
        if k >= 2 {
            self.note_disagreements(&candidates);
            if self.policy.client_match == ClientMatch::Bytes {
                if self.served_by.len() > 4096 {
                    self.served_by.clear();
                }
                for c in &candidates {
                    if let Some(family) = self.family(c.source) {
                        let digest: Root = Sha256::digest(&c.raw).into();
                        self.served_by.entry(digest).or_default().insert((family, c.source));
                    }
                }
            }
        }
        // Judge every candidate with the program's own checks.
        // (source, the job ferrata-advance plans, period update, the bytes as served)
        let mut useful: Vec<(usize, Box<Planned>, bool, Vec<u8>)> = Vec::new();
        let mut best_seen: BTreeMap<usize, u64> = BTreeMap::new();
        let mut seen_digests: BTreeMap<Root, bool> = BTreeMap::new();
        // The furthest finalized slot a candidate the program would not refuse claims (verified, or signed by a
        // committee the store does not hold yet): how far the sources are past the instance.
        let mut seen_head = store.finalized.slot;
        for c in candidates {
            let digest = CompactUpdate::of(&c.update).digest(LcFork::Gloas);
            let verdict = self.evaluate(&store, &c.update);
            if matches!(verdict, Verdict::Useful(_) | Verdict::Unverifiable) {
                seen_head = seen_head.max(c.update.finalized_header.beacon.slot);
            }
            match verdict {
                Verdict::Useful(p) => {
                    let e = best_seen.entry(c.source).or_default();
                    *e = (*e).max(p.update.finalized_header.beacon.slot);
                    seen_digests.insert(digest, true);
                    useful.push((c.source, p, c.period_update, c.raw));
                }
                Verdict::NoChange => {
                    let e = best_seen.entry(c.source).or_default();
                    *e = (*e).max(store.finalized.slot.min(c.update.finalized_header.beacon.slot));
                }
                Verdict::Unverifiable => {}
                Verdict::Invalid(why) => {
                    let kind = if c.period_update { "period update" } else { "finality update" };
                    self.demote(
                        c.source,
                        &format!("{kind} attested at {}: {why}", c.update.attested_header.beacon.slot),
                    );
                }
            }
        }
        self.metrics.sources_head = seen_head;
        // Lag: a source whose newest verified finalized slot is far behind the best.
        let best =
            useful.iter().map(|(_, p, ..)| p.update.finalized_header.beacon.slot).max().unwrap_or(store.finalized.slot);
        for i in 0..self.sources.len() {
            if !self.usable(i) || !self.sources[i].light_client() {
                continue;
            }
            let mine = best_seen.get(&i).copied().unwrap_or(0);
            let max_lag = self.policy.max_lag_slots;
            let s = self.stats(i);
            // Lagging is said of a source that answers; one that does not is unavailable.
            let lagging = s.available && mine + max_lag < best;
            s.finalized_slot = mine;
            s.lagging = lagging;
            if lagging {
                s.lagging_rounds += 1;
            }
        }
        if useful.is_empty() {
            return Ok(Plan::UpToDate);
        }
        // An update that can be sent now comes first: its `Open` window is open by the chain's clock (its signature slot
        // has started; without a clock each transaction carries its own time, so every update can). Then adopting the
        // next committee when the store lacks it; then the furthest finalized header; then the earliest signature slot;
        // then the most signers. The signature does not cover `signature_slot` (only its fork version), so a source can
        // serve a real update, or another real update of the same finalized header with more signers, re-dated to a
        // later slot of the same period, whose `Open` window has not opened yet (internal reviews' findings M4 and M1).
        // One signed in the future by more than FUTURE_MARGIN_MS was refused above. One re-dated within it, again every
        // round, used to rank first when it adopted the next committee, and every round waited for its window until the
        // period ended (a review's M1); now it ranks after every update that can be sent, so it holds back only
        // itself, while no other source serves its finalized header. One re-dated to a slot that has started can rank
        // first; the steps of its job then wait their turns by the instance's state (`next_step`), not by the job, so a
        // new copy each round, opened by the source itself or not, does not postpone them (two internal
        // reviews' findings M1). The comparison is the program's own (`Open`'s
        // window starts at the signature slot's start), exact: a tolerance would let a source re-date to a slot about to
        // start. The signer count does not change the store an update leaves (every update applied has two thirds of the
        // committee); it only breaks the remaining ties.
        let needs_next = store.next.is_none();
        let now = ledger.now_ms();
        let key = |p: &Planned| {
            (
                now.is_none_or(|t| config.slot_start_ms(p.update.signature_slot) <= t),
                needs_next && p.outcome.adopted,
                p.update.finalized_header.beacon.slot,
                std::cmp::Reverse(p.update.signature_slot),
                p.update.sync_aggregate.bits.iter().filter(|b| **b).count(),
            )
        };
        // The best candidate that enough sources serve: light-client sources serving an update with its finalized
        // header, and headers sources holding that block canonical at its slot.
        let mut order: Vec<usize> = (0..useful.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(key(&useful[i].1)));
        let mut asked: BTreeMap<(usize, u64), Option<Root>> = BTreeMap::new();
        let (mut pick, mut first) = (None, None);
        for &i in &order {
            let header = useful[i].1.update.finalized_header.beacon.clone();
            let root = header.hash_tree_root();
            let mut agreeing: BTreeSet<usize> = useful
                .iter()
                .filter(|(_, q, ..)| q.update.finalized_header.beacon.hash_tree_root() == root)
                .map(|(s, ..)| *s)
                .collect();
            let exec = execution_block_hash(&useful[i].1.update.finalized_header);
            agreeing.extend(self.confirm_header(header.slot, &root, exec, &mut asked));
            // Client diversity: the clients of the sources that served this update byte for byte, or that serve or
            // confirm its finalized header.
            let pairs: BTreeSet<(String, usize)> = match (k >= 2, self.policy.client_match) {
                (false, _) => BTreeSet::new(),
                (true, ClientMatch::Bytes) => {
                    let digest: Root = Sha256::digest(&useful[i].3).into();
                    self.served_by.get(&digest).cloned().unwrap_or_default()
                }
                (true, ClientMatch::FinalizedHeader) => {
                    agreeing.iter().filter_map(|&s| Some((self.family(s)?, s))).collect()
                }
            };
            let clients: BTreeSet<String> = pairs.iter().map(|(f, _)| f.clone()).collect();
            let diverse = k < 2 || distinct_pairs(&pairs.iter().cloned().collect::<Vec<_>>(), k);
            if first.is_none() && !diverse {
                self.log(&format!(
                    "update finalizing slot {} (attested slot {}): {} only, {} of the {k} clients \
                     policy.min_distinct_clients asks; not sent",
                    header.slot,
                    useful[i].1.update.attested_header.beacon.slot,
                    if clients.is_empty() {
                        "no client".to_string()
                    } else {
                        clients.iter().cloned().collect::<Vec<_>>().join(", ")
                    },
                    clients.len()
                ));
            }
            first.get_or_insert((agreeing.len(), header.slot, clients.clone(), diverse));
            if agreeing.len() >= self.policy.min_agreeing && diverse {
                pick = Some((i, agreeing, clients));
                break;
            }
        }
        let Some((pick, agreeing, clients)) = pick else {
            let (n, slot, clients, first_diverse) = first.expect("useful is not empty");
            // Waiting for clients (not for `min_agreeing`): how far the furthest verified update is past the instance.
            self.metrics.client_wait = (!first_diverse).then(|| seen_head.saturating_sub(store.finalized.slot));
            let diversity = if k >= 2 {
                let how = match self.policy.client_match {
                    ClientMatch::Bytes => "byte for byte",
                    ClientMatch::FinalizedHeader => "its finalized header",
                };
                format!(
                    "; {} of {k} clients agree {how} ({})",
                    clients.len(),
                    clients.iter().cloned().collect::<Vec<_>>().join(", ")
                )
            } else {
                String::new()
            };
            let served = if n >= self.policy.min_agreeing {
                format!("finalized slot {slot}: {n} sources serve it")
            } else {
                format!("{n} of {} sources serve finalized slot {slot}", self.policy.min_agreeing)
            };
            return Ok(Plan::Waiting(format!("{served}{diversity}")));
        };
        let (_, chosen, period_update, raw) = useful.swap_remove(pick);
        let chosen = *chosen;
        let names: Vec<&str> = agreeing.iter().map(|&s| self.sources[s].name()).collect();
        let what = format!(
            "finalized slot {} root 0x{} (attested slot {}, signature slot {}, {} of {} signers{}{}{}), served or \
             confirmed by {}",
            chosen.update.finalized_header.beacon.slot,
            hex::encode(chosen.update.finalized_header.beacon.hash_tree_root()),
            chosen.update.attested_header.beacon.slot,
            chosen.update.signature_slot,
            chosen.update.sync_aggregate.bits.iter().filter(|b| **b).count(),
            chosen.update.sync_aggregate.bits.len(),
            if chosen.outcome.advanced { "; advances" } else { "" },
            if chosen.outcome.rotated { "; rotates the committees" } else { "" },
            if chosen.outcome.adopted { "; adopts the next committee" } else { "" },
            names.join(", ")
        ) + &if k >= 2 {
            format!("; clients {}", clients.iter().cloned().collect::<Vec<_>>().join(", "))
        } else {
            String::new()
        };
        let plan = self.next_step(ledger, chosen, (period_update, raw), &state, &store)?;
        if let Plan::Send(a) = &plan {
            if a.step == Step::Open {
                self.log(&format!("job 0x{}: {what}", &hex::encode(a.job_id)[..16]));
            }
        }
        Ok(plan)
    }

    /// The job's next step, as the chain holds it (another updater, or this one before a restart, may have begun it).
    /// `served`: whether its update came from the `updates` endpoint, and its bytes as the source served them (kept
    /// once its `Open` is applied: `policy.finish_own_jobs`).
    /// `Open` is sent only inside its validity window, by the chain's clock; a step held after a refusal is not sent.
    fn next_step(
        &mut self,
        ledger: &dyn Ledger,
        chosen: Planned,
        served: (bool, Vec<u8>),
        state: &InstanceState,
        store: &CompactStore,
    ) -> Result<Plan> {
        let step = ferrata_advance::next_step(self.job_on_chain(ledger, &chosen.jid)?.as_ref());
        let fin = store.finalized.slot;
        let signature_slot = chosen.update.signature_slot;
        let window = match step {
            Step::Open => match ferrata_advance::open_window(&self.inst.config, fin, signature_slot) {
                Some(w) => w,
                None => {
                    let why = format!(
                        "Open not sent: the update is signed at slot {signature_slot}, at or after the end of the \
                         weak-subjectivity period of the instance's finalized header (slot {fin}), which the program \
                         refuses: the instance can no longer advance; initialise a new one"
                    );
                    self.expired = Some((fin, why.clone()));
                    return Ok(Plan::Stuck(why));
                }
            },
            _ => Window::ANY,
        };
        if let Some((late, why)) = self.outside(ledger, &window, self.inst.time_ms(signature_slot)) {
            return Ok(if late {
                let why = format!(
                    "Open not sent: {why}: the weak-subjectivity period of the instance's finalized header (slot {fin}) \
                     ends there, and no update can be applied to it after that: initialise a new instance"
                );
                self.expired = Some((fin, why.clone()));
                Plan::Stuck(why)
            } else {
                Plan::Waiting(format!("Open not sent: {why}"))
            });
        }
        self.expired = None;
        if let Some(why) = self.held_back(&chosen.jid, step) {
            return Ok(Plan::Stuck(why));
        }
        // Whose turn it is (`policy.defer_window`). A step of a job this updater carries (it sent a step of it) counts
        // from the first round it was due, by its job: nobody else can change that job, and what others send of it only
        // moves it on. An `Open`, and a step of any other job, counts from the first round a step of that kind was due on
        // the instance's state, whatever job it is for. The job id hashes the whole update, `signature_slot` included,
        // which the signature does not cover; a beacon node can build another real update of the period from each of
        // its blocks; and anyone may open a job and send its steps. Counted by the job, a source serving a new copy each
        // round, or a new copy it had opened (and taken on a step) itself, had this updater count again each round, and
        // one that knew its tag (its name and payer account, both public) served only copies it deferred: an update
        // adopting the next committee held every update back for good (two internal reviews' findings M1). Now each
        // kind of step has one turn per state, at most `defer_window - 1` rounds, whatever the sources
        // serve and others send; a source can only present jobs at another step, each kind's turn running once.
        let since = match step {
            Step::Open => state_turn(state),
            _ if self.carried.contains_key(&chosen.jid) => chosen.jid,
            _ => state_turn(state),
        };
        if self.defer(&chosen.jid, &since, step) {
            return Ok(Plan::Deferred(step));
        }
        let tx = self.job_tx(&chosen, step)?;
        self.sending = Some((chosen.jid, served.0, served.1));
        Ok(Plan::Send(Action {
            step,
            job_id: chosen.jid,
            tx,
            finalized_slot: chosen.update.finalized_header.beacon.slot,
            store_seen: Some((state.active, state.store.clone())),
            window,
        }))
    }

    /// Whether `window` refuses a transaction settled now ([`WINDOW_MARGIN_MS`] before its end), by the chain's clock
    /// (where the chain takes each transaction's own time, `tx_time_ms`): (past its end, why).
    fn outside(&self, ledger: &dyn Ledger, window: &Window, tx_time_ms: u64) -> Option<(bool, String)> {
        let now = ledger.now_ms().unwrap_or(tx_time_ms);
        if window.admits(now, WINDOW_MARGIN_MS) {
            return None;
        }
        let t = |ms: u64| crate::utc(ms / 1000);
        Some(match (window.start_ms, window.end_ms) {
            (Some(start), _) if now < start => {
                (false, format!("its validity window opens at {} (the chain's clock: {})", t(start), t(now)))
            }
            (_, end) => (
                true,
                format!(
                    "its validity window ends at {} (the chain's clock: {}; a transaction is sent at least {} s before \
                     the end)",
                    end.map_or("?".into(), t),
                    t(now),
                    WINDOW_MARGIN_MS / 1000
                ),
            ),
        })
    }

    /// Why `(jid, step)` is not sent this round, if it is held after a refusal.
    fn held_back(&self, jid: &Root, step: Step) -> Option<String> {
        let h = self.held.get(&(*jid, step))?;
        if !self.holding(h) {
            return None;
        }
        let hint = match (h.cause, h.retry_at) {
            (_, Some(r)) => format!("tried again at round {r}"),
            (Refusal::Gas, None) => format!("fees.gas_limit {} is below what it needs: raise it", h.gas_limit),
            (Refusal::Program(_), None) => "the program refuses it on this state".to_string(),
            (_, None) => "not sent again on this state".to_string(),
        };
        Some(format!(
            "{} of job 0x{} (finalized slot {}) refused {} time(s), the last {} ({}: {}): {hint}",
            step.name(),
            &hex::encode(jid)[..16],
            h.finalized_slot,
            h.count,
            if h.due { "while it stayed due" } else { "with the chain unreadable after it" },
            h.cause.describe(),
            h.note
        ))
    }

    /// Whether a hold still holds this round.
    fn holding(&self, h: &Held) -> bool {
        let gas_raised = h.cause == Refusal::Gas && self.fees.gas_limit > h.gas_limit;
        !gas_raised && h.retry_at.is_none_or(|r| self.round < r)
    }

    /// Hold `(job, step)` after a refusal: `due`, the chain read after it showed the step still due ([`After::Due`]);
    /// otherwise it could not be read ([`After::Unread`]: held for a while whatever the cause). What follows, described.
    fn hold(&mut self, action: &Action, cause: Refusal, note: &str, due: bool) -> String {
        let (round, gas_limit, state) = (self.round, self.fees.gas_limit, state_digest(&action.store_seen));
        let h = self.held.entry((action.job_id, action.step)).or_insert_with(|| Held {
            step: action.step,
            finalized_slot: action.finalized_slot,
            cause,
            note: String::new(),
            count: 0,
            due,
            round,
            retry_at: None,
            gas_limit,
            state,
        });
        h.count = h.count.saturating_add(1);
        h.cause = cause;
        h.note = clip(note);
        h.due = due;
        h.round = round;
        h.gas_limit = gas_limit;
        h.state = state;
        // A cause that repeats on the same state, seen to leave the step due: for good. Any other: a hold that doubles
        // up to MAX_REFUSAL_BACKOFF rounds, then the step is sent again (review of the fixes, L3: it used to stop
        // after three).
        h.retry_at = if due && cause.repeats() {
            None
        } else {
            Some(round + (REFUSAL_BACKOFF << (h.count - 1).min(16)).min(MAX_REFUSAL_BACKOFF))
        };
        let what = match h.retry_at {
            Some(r) => format!("held until round {r}"),
            None if cause == Refusal::Gas => format!("held until fees.gas_limit is raised above {gas_limit}"),
            None => "not sent again on this state".to_string(),
        };
        if self.held.len() > MAX_HELD {
            if let Some(oldest) = self.held.iter().min_by_key(|(_, h)| h.round).map(|(k, _)| *k) {
                self.held.remove(&oldest);
            }
        }
        what
    }

    /// Count a refusal of a step that stayed due; the breaker opens at [`BREAKER_AFTER`] in a row.
    fn strike(&mut self, what: String) -> Option<String> {
        let b = &mut self.breaker;
        b.strikes += 1;
        b.last = what;
        if b.strikes < BREAKER_AFTER {
            return None;
        }
        b.trips += 1;
        let pause = BREAKER_PAUSE.saturating_mul(1 << (b.trips - 1).min(16)).min(BREAKER_MAX_PAUSE);
        b.paused_until = self.round + 1 + pause;
        Some(format!("{} refusals in a row: nothing sent before round {}", b.strikes, b.paused_until))
    }

    fn paused(&self) -> String {
        format!(
            "sending paused until round {}: {} refusals in a row of steps that stayed due, the last: {}",
            self.breaker.paused_until, self.breaker.strikes, self.breaker.last
        )
    }

    /// The next step of a job this updater sent a step for whose update still moves the store
    /// (`policy.finish_own_jobs`); a kept job that is finished, gone or moot is forgotten. A job whose next step is held
    /// is kept while the hold will end and the step has been refused fewer than [`LEAVE_JOB_AFTER`] times (review of
    /// the fixes, L2: it used to be left at the first hold, and the steps paid for it were lost once the sources moved
    /// on); otherwise this updater leaves it to another party.
    fn continue_carried(
        &mut self,
        ledger: &dyn Ledger,
        store: &CompactStore,
        state: &InstanceState,
    ) -> Result<Option<Plan>> {
        let carried: Vec<(Root, (bool, Vec<u8>))> = self.carried.iter().map(|(k, v)| (*k, v.clone())).collect();
        for (jid, (period_update, raw)) in carried {
            let decoded = if period_update {
                decode_update(&raw, &self.inst.config)
            } else {
                decode_finality_update(&raw, &self.inst.config)
            };
            let Ok((update, _)) = decoded else {
                self.drop_carried(&jid);
                continue;
            };
            match self.evaluate(store, &update) {
                Verdict::Useful(p) if p.jid == jid => {
                    let Some(job) = self.job_on_chain(ledger, &jid)? else {
                        self.drop_carried(&jid);
                        continue;
                    };
                    let step = ferrata_advance::next_step(Some(&job));
                    let plan = self.next_step(ledger, *p, (period_update, raw), state, store)?;
                    if let Plan::Stuck(why) = &plan {
                        // A held step. Kept through a hold that will end; left after LEAVE_JOB_AFTER refusals, or a
                        // hold for good (another party may finish it).
                        let passes = |h: &Held| h.retry_at.is_some() && h.count < LEAVE_JOB_AFTER;
                        if self.held.get(&(jid, step)).is_some_and(passes) {
                            return Ok(Some(plan));
                        }
                        self.log(&format!("kept job 0x{} left: {why}", &hex::encode(jid)[..16]));
                        self.drop_carried(&jid);
                        continue;
                    }
                    if let Plan::Send(a) = &plan {
                        self.log(&format!(
                            "job 0x{} (finalized slot {}) is open: its {} first",
                            &hex::encode(jid)[..16],
                            a.finalized_slot,
                            a.step.name()
                        ));
                    }
                    return Ok(Some(plan));
                }
                _ => self.drop_carried(&jid),
            }
        }
        Ok(None)
    }

    /// The headers sources that hold `root` canonical at `slot`: each asked at most once a round, a confirmation kept
    /// (a finalized block stays canonical).
    fn confirm_header(
        &mut self,
        slot: u64,
        root: &Root,
        exec: Option<Root>,
        asked: &mut BTreeMap<(usize, u64), Option<Root>>,
    ) -> Vec<usize> {
        let mut out = Vec::new();
        for i in 0..self.sources.len() {
            if self.sources[i].light_client() || !self.usable(i) {
                continue;
            }
            // An execution endpoint confirms the header by its execution block (an Altair header binds none).
            let execution = self.sources[i].execution();
            if execution && exec.is_none() {
                continue;
            }
            if self.confirmed.get(&(i, slot)) == Some(root) {
                out.push(i);
                continue;
            }
            let answer = match asked.get(&(i, slot)) {
                Some(a) => *a,
                None => {
                    self.stats(i).requests += 1;
                    let answer = match (execution, exec) {
                        (true, Some(h)) => self.sources[i].execution_block(&h).map(|n| n.map(|_| *root)),
                        _ => self.sources[i].canonical_root(slot),
                    };
                    match answer {
                        Ok(a) => {
                            self.stats(i).available = true;
                            asked.insert((i, slot), a);
                            a
                        }
                        Err(e) => {
                            let s = self.stats(i);
                            s.errors += 1;
                            s.available = false;
                            self.log(&format!("header at slot {slot}: {e:#}"));
                            continue;
                        }
                    }
                }
            };
            let name = self.sources[i].name().to_string();
            let s = self.stats(i);
            if answer.as_ref() == Some(root) {
                s.mismatch = false;
                s.confirmed += 1;
                s.finalized_slot = s.finalized_slot.max(slot);
                if self.confirmed.len() > 4096 {
                    self.confirmed.clear();
                }
                self.confirmed.insert((i, slot), *root);
                out.push(i);
            } else {
                s.mismatch = true;
                s.mismatches += 1;
                if let (true, Some(h)) = (execution, exec) {
                    self.log(&format!(
                        "source {name} does not hold the execution block 0x{} canonical (the finalized header at slot \
                         {slot}, 0x{})",
                        hex::encode(h),
                        hex::encode(root)
                    ));
                } else {
                    let theirs = answer.map_or("no block".to_string(), |r| format!("0x{}", hex::encode(r)));
                    self.log(&format!(
                        "source {name} holds {theirs} canonical at slot {slot}, not 0x{}",
                        hex::encode(root)
                    ));
                }
            }
        }
        out
    }

    fn job_on_chain(&self, ledger: &dyn Ledger, jid: &Root) -> Result<Option<Job>> {
        ferrata_advance::job_of_state(&ledger.actor_state(self.inst.job(jid))?)
    }

    fn adopt_tx(
        &self,
        job_actor: Actor,
        keys: &[[u8; 96]],
        p: u8,
        record_slot: Option<u64>,
        time_ms: u64,
        what: &str,
    ) -> Tx {
        let parts = self.inst.config.adoption_parts;
        let t = ferrata_advance::adopt_step(self.inst.program, &self.inst.id, parts, job_actor, keys, p, record_slot);
        Tx {
            label: format!("{} {what} adopt {}/{parts}", self.inst.name, p + 1),
            root: t.root,
            message: t.message,
            actors: t.actors,
            evidence: vec![],
            time_ms,
        }
    }

    fn job_tx(&self, p: &Planned, step: Step) -> Result<Tx> {
        let time_ms = self.inst.time_ms(p.update.signature_slot);
        let what = format!("fin {}", p.update.finalized_header.beacon.slot);
        let label = match step {
            Step::Open => format!("{} {what} Open", self.inst.name),
            Step::Miller => format!("{} {what} Miller", self.inst.name),
            Step::Adopt(part) => {
                format!("{} {what} adopt {}/{}", self.inst.name, part + 1, self.inst.config.adoption_parts)
            }
            Step::Finish => format!("{} {what} Finish", self.inst.name),
            Step::Init => bail!("not a job step"),
        };
        let t = ferrata_advance::job_step(self.inst.program, &self.inst.id, &self.inst.config, p, step)?;
        Ok(Tx { label, root: t.root, message: t.message, actors: t.actors, evidence: vec![], time_ms })
    }

    /// An `Open` for any update, unchecked (what an adversary sends; the tests use it).
    pub fn open_unchecked(&self, ledger: &dyn Ledger, update: &LightClientUpdate) -> Result<Tx> {
        let config = &self.inst.config;
        let spec = config.spec();
        let store = self.inst.store(ledger)?.ok_or_else(|| anyhow!("no active instance"))?;
        let layout = layout_for(config, store.fork, update).unwrap_or(store.fork);
        let store_u =
            steps::store_for_update(&store, layout, update.attested_header.beacon.slot, &spec).unwrap_or(store.clone());
        let compact = CompactUpdate::of(update);
        let current = self.committees.get(&store.current.pubkeys_root);
        let next = store.next.as_ref().and_then(|n| self.committees.get(&n.pubkeys_root));
        let witness = signing_committee(&store, update, &spec, current, next)
            .map(|c| signer_witness(c, update))
            .unwrap_or_default();
        let jid = job_id(&self.inst.id, &store_u.digest(), &compact.digest(layout));
        Ok(Tx {
            label: format!("{} unchecked Open", self.inst.name),
            root: self.inst.actor(),
            message: borsh::to_vec(&Message::Open { update: compact.encode(layout), signers: witness.encode() })?,
            actors: vec![self.inst.actor(), self.inst.job(&jid)],
            evidence: vec![],
            time_ms: self.inst.time_ms(update.signature_slot),
        })
    }

    /// Send `action`, retrying transport failures with exponential backoff; the result as the
    /// chain settled it. Before sending, the step is checked against the chain again: if another
    /// updater sent it meanwhile, nothing is sent.
    pub fn submit(&mut self, ledger: &mut dyn Ledger, action: &Action, recheck: bool) -> Result<Option<TxOutcome>> {
        if recheck && self.read_step_done(ledger, action)? {
            self.metrics.skipped_done += 1;
            return Ok(None);
        }
        if self.metrics.spent() + u128::from(self.fees.max_fee_per_tx) > u128::from(self.fees.budget) {
            self.metrics.budget_stops += 1;
            bail!("budget reached: spent {} of {}", self.metrics.spent(), self.fees.budget);
        }
        let mut attempt = 0u32;
        loop {
            if let Some((_, why)) = self.outside(ledger, &action.window, action.tx.time_ms) {
                bail!("{}: not sent: {why}", action.tx.label);
            }
            match ledger.submit(&self.payer, &action.tx, self.fees.gas_limit, u128::from(self.fees.max_fee_per_tx)) {
                Ok(out) => {
                    // A step sent on a state took this updater's turn there: the next step of that kind on it (another
                    // job's, or this one's after a refusal) waits a whole turn, `defer_window - 1` rounds. A fresh
                    // turn, its wait drawn from each round's job, would end at the first round whose draw had run
                    // out, so with `finish_own_jobs = false` and sources whose best update changes every round (beacon
                    // nodes serving their newest finality update) an `Open` went out about every third round
                    // (an internal review's L2); without a new turn, every round.
                    let whole = self.policy.defer_window.saturating_sub(1);
                    self.first_due
                        .insert((state_digest(&action.store_seen), action.step), (self.round + 1, Some(whole)));
                    // A refused step: the chain is read again. Done or moot: a race. Still due (no other party's
                    // transaction came first, the store did not move): sent again it would meet the same state.
                    // Unreadable after every attempt: unknown, held for a while (review of the fixes, L4: a failed read
                    // used to count as still due, so lost races tripped the breaker).
                    let after = if out.applied || out.rejected {
                        None
                    } else {
                        Some(match self.read_step_done(ledger, action) {
                            Ok(true) => After::Race,
                            Ok(false) => After::Due,
                            Err(_) => After::Unread,
                        })
                    };
                    self.record(action, &out, after);
                    return Ok(Some(out));
                }
                Err(e) => {
                    self.metrics.transport_errors += 1;
                    self.metrics.last_error = format!("{e:#}");
                    attempt += 1;
                    if attempt >= self.policy.max_attempts {
                        self.metrics.failing_sends += 1;
                        self.metrics.last_failure =
                            format!("{}: transport: gave up after {attempt} attempts: {e:#}", action.tx.label);
                        bail!("{}: gave up after {attempt} attempts: {e:#}", action.tx.label);
                    }
                    self.metrics.retries += 1;
                    let wait = self.backoff_ms(attempt);
                    self.log(&format!("{}: {e:#}; retry {attempt} in {wait} ms", action.tx.label));
                    (self.sleep)(wait);
                    if self.step_done(ledger, action)? {
                        self.metrics.skipped_done += 1;
                        return Ok(None);
                    }
                }
            }
        }
    }

    /// The transport backoff before attempt `attempt + 1` (`attempt` from 1): `retry_base_ms * 2^(attempt-1)`, at most
    /// `retry_max_ms`.
    fn backoff_ms(&self, attempt: u32) -> u64 {
        self.policy.retry_base_ms.saturating_mul(1 << attempt.saturating_sub(1).min(20)).min(self.policy.retry_max_ms)
    }

    /// [`Updater::step_done`], read up to `policy.max_attempts` times with the transport backoff between: on the
    /// sequencer backend a read fails while another party's transaction touching the instance is in flight.
    fn read_step_done(&mut self, ledger: &dyn Ledger, action: &Action) -> Result<bool> {
        let mut attempt = 0u32;
        loop {
            match self.step_done(ledger, action) {
                Ok(done) => return Ok(done),
                Err(e) => {
                    attempt += 1;
                    if attempt >= self.policy.max_attempts {
                        return Err(e.context(format!(
                            "{}: the chain could not be read ({attempt} attempts)",
                            action.tx.label
                        )));
                    }
                    let wait = self.backoff_ms(attempt);
                    self.log(&format!("{}: reading the chain: {e:#}; again in {wait} ms", action.tx.label));
                    (self.sleep)(wait);
                }
            }
        }
    }

    /// True if the chain no longer needs `action`: another updater sent the step, or a job was
    /// applied since the round began (the job's store is gone, so are its remaining steps).
    fn step_done(&self, ledger: &dyn Ledger, action: &Action) -> Result<bool> {
        let state = self.inst.state(ledger)?;
        if action.step == Step::Init {
            return Ok(state.is_some());
        }
        let now = state.as_ref().map(|s| (s.active, s.store.clone()));
        if now != action.store_seen {
            return Ok(true);
        }
        let job = self.job_on_chain(ledger, &action.job_id)?;
        Ok(match (action.step, job) {
            (Step::Open, j) => j.is_some(),
            (_, None) => true,
            (Step::Miller, Some(j)) => !matches!(j.phase, Phase::Miller { .. }),
            (Step::Adopt(p), Some(j)) => {
                j.adoption.as_ref().is_none_or(|a| a.complete || a.chunk_roots.len() as u8 > p)
            }
            (Step::Finish, Some(_)) | (Step::Init, Some(_)) => false,
        })
    }

    /// `after`: for a refusal, what the chain showed after it (`None` for an applied or rejected transaction).
    fn record(&mut self, action: &Action, out: &TxOutcome, after: Option<After>) {
        let result = if out.applied {
            "applied"
        } else if out.rejected {
            "rejected"
        } else {
            "refused"
        };
        *self.metrics.tx.entry((action.step.name(), result)).or_default() += 1;
        let gas = out.gas_exec + out.gas_stor;
        let mut why = String::new();
        // Keep the job of a step that landed, or that another party's transaction for the same job may have overtaken
        // (the job goes on): it comes first until it is finished or moot (`policy.finish_own_jobs`). A kept job that
        // is not on chain is forgotten the next round.
        let job_goes_on = out.applied || (!out.rejected && after != Some(After::Due));
        if self.policy.finish_own_jobs && job_goes_on && action.step != Step::Finish {
            if let Some((jid, period_update, raw)) = self.sending.take().filter(|o| o.0 == action.job_id) {
                self.carry(jid, period_update, raw);
            }
        }
        if out.applied {
            self.held.remove(&(action.job_id, action.step));
            self.breaker = Breaker::default();
            self.metrics.failing_sends = 0;
            self.metrics.last_applied_round = self.round;
            self.metrics.last_applied_utc = crate::utc_now();
            if action.step == Step::Finish {
                self.drop_carried(&action.job_id);
            }
            self.metrics.gas_applied += gas;
            self.metrics.fee_applied += out.fee;
            let last_adopt = matches!(action.step, Step::Adopt(p) if p + 1 == self.inst.config.adoption_parts)
                && action.tx.actors.len() > 1;
            if action.step == Step::Finish || last_adopt {
                self.metrics.jobs_finished += 1;
            }
        } else if out.rejected {
            why = format!(": {}", clip(&out.note));
            self.metrics.failing_sends += 1;
            self.metrics.last_failure = format!("{} rejected{why}", action.tx.label);
        } else {
            self.metrics.gas_refused += gas;
            self.metrics.fee_refused += out.fee;
            if let Some(c) = out.exit_code {
                *self.metrics.refused_codes.entry(c).or_default() += 1;
            }
            let cause = Refusal::of(out);
            let after = after.unwrap_or(After::Due);
            *self.metrics.refusals.entry((cause.kind(), after.label())).or_default() += 1;
            let reason = format!("{}: {}", cause.describe(), clip(&out.note));
            match after {
                After::Due => {
                    self.metrics.failing_sends += 1;
                    self.metrics.last_failure = format!("{} refused: {reason}", action.tx.label);
                    let held = self.hold(action, cause, &out.note, true);
                    let what = format!("{} {reason}", action.tx.label);
                    why = format!(": {reason}; the step stayed due: {held}");
                    if let Some(paused) = self.strike(what) {
                        why += &format!("; {paused}");
                    }
                }
                After::Unread => {
                    let held = self.hold(action, cause, &out.note, false);
                    if self.unjudged.len() >= MAX_UNJUDGED {
                        self.unjudged.remove(0);
                    }
                    self.unjudged.push((action.clone(), cause, reason.clone()));
                    why = format!(
                        ": {reason}; the chain could not be read after it ({} attempts), so whether another party's \
                         transaction came first is not known: {held}, judged once the chain is read again",
                        self.policy.max_attempts
                    );
                }
                After::Race => {
                    why = format!(
                        ": {reason}; the step was done or moot afterwards (another party's transaction came first)"
                    );
                }
            }
        }
        self.log(&format!(
            "{} {result} (exit {:?}, gas {}, fee {}, block {}){why}",
            action.tx.label, out.exit_code, gas, out.fee, out.block
        ));
        self.journal(action, out);
        self.save_held();
    }

    /// The refusals after which the chain could not be read ([`After::Unread`]), judged now that it may be read again:
    /// a step done or moot was a race another party won (its hold is dropped); a step that stayed due counts as any
    /// refusal of a step that stayed due does: a send that did not land and a strike towards the breaker, its hold for
    /// good if the cause repeats on the same state (9c L-U3: such a refusal used to be neither, whatever followed). One
    /// whose chain still cannot be read waits for a later round.
    fn judge_unread(&mut self, ledger: &dyn Ledger) {
        for (action, cause, reason) in std::mem::take(&mut self.unjudged) {
            match self.step_done(ledger, &action) {
                Err(_) => self.unjudged.push((action, cause, reason)),
                Ok(true) => {
                    *self.metrics.unread_judged.entry("race").or_default() += 1;
                    self.held.remove(&(action.job_id, action.step));
                    self.log(&format!(
                        "{} (refused with the chain unreadable after it): read again, the step was done or moot \
                         (another party's transaction came first); not held",
                        action.tx.label
                    ));
                }
                Ok(false) => {
                    *self.metrics.unread_judged.entry("due").or_default() += 1;
                    self.metrics.failing_sends += 1;
                    self.metrics.last_failure =
                        format!("{} refused: {reason}; the chain read again: the step stayed due", action.tx.label);
                    if let Some(h) = self.held.get_mut(&(action.job_id, action.step)) {
                        h.due = true;
                        if cause.repeats() {
                            h.retry_at = None;
                        }
                    }
                    let mut why = format!(
                        "{} (refused with the chain unreadable after it: {reason}): read again, the step stayed due",
                        action.tx.label
                    );
                    if let Some(paused) = self.strike(format!("{} {reason}", action.tx.label)) {
                        why += &format!("; {paused}");
                    }
                    self.log(&why);
                }
            }
        }
    }

    /// One round: plan, then send what is due, after reading the chain once more: the round read the instance before
    /// it asked its sources, and another party may have sent the step, or moved the store, since (nothing is sent
    /// then; `_steps_done_by_others_total`). A round that ends in an error counts towards
    /// [`DOWN_AFTER_FAILED_ROUNDS`]; one that does not resets the count.
    pub fn tick(&mut self, ledger: &mut dyn Ledger) -> Result<Plan> {
        let result = match self.plan(ledger) {
            Ok(Plan::Send(action)) => self.submit(ledger, &action, true).map(|_| Plan::Send(action)),
            other => other,
        };
        match &result {
            Ok(_) => self.metrics.failed_rounds = 0,
            Err(e) => {
                self.metrics.failed_rounds = self.metrics.failed_rounds.saturating_add(1);
                self.metrics.last_error = format!("{e:#}");
            }
        }
        result
    }

    // ---- health and metrics -----------------------------------------------------------------

    /// `ok`, `degraded` or `down`, and why. Degraded: a source lies, lags, fails or disagrees; a transaction it sent
    /// did not land (rejected, refused while its step stayed due, failed at transport); a round ended in an error; the
    /// last round was stuck; sending is paused; the instance has not moved for [`Updater::stall_rounds`] rounds while
    /// the sources serve updates more than `max_lag_slots` past it. Down: this updater moves nothing: no light-client
    /// source usable and answering; [`DOWN_AFTER_FAILED_SENDS`] sends in a row that did not land;
    /// [`DOWN_AFTER_FAILED_ROUNDS`] rounds in a row that ended in an error (a chain that answers neither reads nor
    /// sends); the instance (or, before `Init`, its checkpoint) past its weak-subjectivity period by the chain's clock
    /// ([`Updater::expired`]: a person must initialise a new one); no progress for [`Updater::stall_down_rounds`]
    /// rounds while the sources serve updates more than `max_lag_slots` past it (a step held, sending paused, too few
    /// sources or clients agreeing, round after round); or the budget reached.
    pub fn health(&self) -> (&'static str, Vec<String>) {
        let mut why = Vec::new();
        // Light-client sources not demoted that answered when last asked (or were not asked yet).
        let answering = (0..self.sources.len())
            .filter(|&i| self.sources[i].light_client() && self.usable(i))
            .filter(|&i| {
                self.metrics.sources.get(self.sources[i].name()).is_none_or(|s| s.available || s.requests == 0)
            })
            .count();
        for (name, s) in &self.metrics.sources {
            if s.demoted_until > self.round {
                why.push(format!(
                    "source {name} demoted until round {} (served what the program refuses: {})",
                    s.demoted_until, s.last_refused
                ));
            } else if s.lagging {
                why.push(format!("source {name} lagging (finalized slot {})", s.finalized_slot));
            } else if s.mismatch {
                let execution = self.sources.iter().any(|x| x.name() == name && x.execution());
                why.push(if execution {
                    format!("source {name} does not hold a finalized header's execution block canonical")
                } else {
                    format!("source {name} holds another block canonical at a finalized header's slot")
                });
            } else if let Some(d) = &s.disagree {
                why.push(format!("source {name} {d}"));
            } else if !s.available && s.requests > 0 {
                why.push(format!("source {name} unavailable"));
            }
        }
        if let Some(lag) = self.metrics.client_wait.filter(|lag| *lag > self.policy.max_lag_slots) {
            why.push(format!(
                "waiting for clients: the sources serve updates {lag} slots past the instance, none sent: fewer than \
                 {} clients agree (policy.min_distinct_clients)",
                self.policy.min_distinct_clients
            ));
        }
        let budget_reached = self.metrics.spent() + u128::from(self.fees.max_fee_per_tx) > u128::from(self.fees.budget);
        if budget_reached {
            why.push("budget reached".into());
        }
        if answering == 0 {
            why.push("no light-client source answers".into());
        }
        if self.metrics.failing_sends > 0 {
            why.push(format!(
                "the last {} transaction(s) sent did not land; the last: {}",
                self.metrics.failing_sends, self.metrics.last_failure
            ));
        }
        if self.metrics.failed_rounds > 0 {
            why.push(format!(
                "the last {} round(s) ended in an error; the last: {}",
                self.metrics.failed_rounds, self.metrics.last_error
            ));
        }
        let behind = self.metrics.sources_head.saturating_sub(self.metrics.finalized_slot);
        let still = self.round.saturating_sub(self.metrics.progress_round);
        let left_behind = self.metrics.progress_round > 0 && behind > self.policy.max_lag_slots;
        if left_behind && still >= self.stall_rounds() {
            why.push(format!(
                "no progress: the instance's finalized slot {} has not moved for {still} rounds while the sources serve \
                 updates {behind} slots past it{}",
                self.metrics.finalized_slot,
                if still >= self.stall_down_rounds() { "" } else { " (down after twice as many)" }
            ));
        }
        let plan = &self.metrics.last_plan;
        if plan.starts_with("stuck") || (plan.starts_with("error") && self.metrics.failed_rounds == 0) {
            why.push(plan.clone());
        }
        if let Some((_, w)) = self.expired.as_ref().filter(|(_, w)| !plan.ends_with(w.as_str())) {
            why.push(w.clone());
        }
        if self.round < self.breaker.paused_until && !plan.starts_with("stuck: sending paused") {
            why.push(self.paused());
        }
        let down = answering == 0
            || budget_reached
            || self.metrics.failing_sends >= DOWN_AFTER_FAILED_SENDS
            || self.metrics.failed_rounds >= DOWN_AFTER_FAILED_ROUNDS
            || self.expired.is_some()
            || (left_behind && still >= self.stall_down_rounds());
        let status = if down {
            "down"
        } else if why.is_empty() {
            "ok"
        } else {
            "degraded"
        };
        (status, why)
    }

    /// Rounds without progress after which `/health` says so: room for every step of a job (`Open`, `Miller`, two
    /// `Adopt` parts, `Finish`) each waiting its full deferral, and ten more.
    pub fn stall_rounds(&self) -> u64 {
        self.policy.defer_window.saturating_mul(6).saturating_add(10)
    }

    /// Rounds without progress (while the sources serve updates past the instance) after which `/health` says `down`:
    /// twice [`Updater::stall_rounds`].
    pub fn stall_down_rounds(&self) -> u64 {
        self.stall_rounds().saturating_mul(2)
    }

    /// How many jobs it keeps to finish (`policy.finish_own_jobs`).
    pub fn carried_jobs(&self) -> usize {
        self.carried.len()
    }

    /// The metrics in Prometheus' text format.
    pub fn render_metrics(&self) -> String {
        let l = format!("daemon=\"{}\",chain=\"{}\"", self.label, self.inst.name);
        let m = &self.metrics;
        let mut o = String::new();
        let _ = writeln!(o, "ferrata_updater_rounds_total{{{l}}} {}", m.rounds);
        for ((step, result), n) in &m.tx {
            let _ = writeln!(o, "ferrata_updater_tx_total{{{l},step=\"{step}\",result=\"{result}\"}} {n}");
        }
        for (code, n) in &m.refused_codes {
            let _ = writeln!(o, "ferrata_updater_refused_by_code_total{{{l},code=\"{code}\"}} {n}");
        }
        for ((cause, after), n) in &m.refusals {
            let _ = writeln!(o, "ferrata_updater_refusals_total{{{l},cause=\"{cause}\",after=\"{after}\"}} {n}");
        }
        for (judged, n) in &m.unread_judged {
            let _ = writeln!(o, "ferrata_updater_unread_refusals_judged_total{{{l},as=\"{judged}\"}} {n}");
        }
        let held = self.held.values().filter(|h| self.holding(h)).count();
        let _ = writeln!(o, "ferrata_updater_held_steps{{{l}}} {held}");
        let _ =
            writeln!(o, "ferrata_updater_sending_paused{{{l}}} {}", u8::from(self.round < self.breaker.paused_until));
        let _ = writeln!(o, "ferrata_updater_gas_total{{{l},result=\"applied\"}} {}", m.gas_applied);
        let _ = writeln!(o, "ferrata_updater_gas_total{{{l},result=\"refused\"}} {}", m.gas_refused);
        let _ = writeln!(o, "ferrata_updater_fee_total{{{l},result=\"applied\"}} {}", m.fee_applied);
        let _ = writeln!(o, "ferrata_updater_fee_total{{{l},result=\"refused\"}} {}", m.fee_refused);
        let _ = writeln!(
            o,
            "ferrata_updater_budget_remaining{{{l}}} {}",
            u128::from(self.fees.budget).saturating_sub(m.spent())
        );
        let _ = writeln!(o, "ferrata_updater_transport_errors_total{{{l}}} {}", m.transport_errors);
        let _ = writeln!(o, "ferrata_updater_retries_total{{{l}}} {}", m.retries);
        let _ = writeln!(o, "ferrata_updater_deferrals_total{{{l}}} {}", m.deferrals);
        let _ = writeln!(o, "ferrata_updater_steps_done_by_others_total{{{l}}} {}", m.skipped_done);
        let _ = writeln!(o, "ferrata_updater_jobs_finished_total{{{l}}} {}", m.jobs_finished);
        let _ = writeln!(o, "ferrata_updater_finalized_slot{{{l}}} {}", m.finalized_slot);
        let _ = writeln!(o, "ferrata_updater_sources_head_slot{{{l}}} {}", m.sources_head);
        let _ = writeln!(o, "ferrata_updater_failing_sends{{{l}}} {}", m.failing_sends);
        let _ = writeln!(o, "ferrata_updater_failed_rounds{{{l}}} {}", m.failed_rounds);
        let _ = writeln!(o, "ferrata_updater_past_weak_subjectivity{{{l}}} {}", u8::from(self.expired.is_some()));
        let _ = writeln!(o, "ferrata_updater_rounds_without_progress{{{l}}} {}", self.rounds_without_progress());
        let _ = writeln!(o, "ferrata_updater_period{{{l}}} {}", m.period);
        if self.policy.min_distinct_clients >= 2 {
            let _ = writeln!(o, "ferrata_updater_client_wait_slots{{{l}}} {}", m.client_wait.unwrap_or(0));
        }
        for (name, s) in &m.sources {
            let sl = format!("{l},source=\"{name}\"");
            let _ = writeln!(o, "ferrata_updater_source_requests_total{{{sl}}} {}", s.requests);
            let _ = writeln!(o, "ferrata_updater_source_errors_total{{{sl}}} {}", s.errors);
            let _ = writeln!(o, "ferrata_updater_source_refused_total{{{sl}}} {}", s.refused);
            let _ = writeln!(o, "ferrata_updater_source_lagging{{{sl}}} {}", u8::from(s.lagging));
            let _ = writeln!(o, "ferrata_updater_source_available{{{sl}}} {}", u8::from(s.available));
            let _ = writeln!(o, "ferrata_updater_source_demoted{{{sl}}} {}", u8::from(s.demoted_until > self.round));
            let _ = writeln!(o, "ferrata_updater_source_finalized_slot{{{sl}}} {}", s.finalized_slot);
            let _ = writeln!(o, "ferrata_updater_source_confirmed_total{{{sl}}} {}", s.confirmed);
            let _ = writeln!(o, "ferrata_updater_source_mismatches_total{{{sl}}} {}", s.mismatches);
            if self.policy.min_distinct_clients >= 2 {
                let _ = writeln!(o, "ferrata_updater_source_disagreements_total{{{sl}}} {}", s.disagreements);
                let _ = writeln!(o, "ferrata_updater_source_client_changes_total{{{sl}}} {}", s.client_changes);
                if let Some(family) = client_family(&s.client) {
                    let _ = writeln!(
                        o,
                        "ferrata_updater_source_client_info{{{sl},client=\"{family}\",version=\"{}\"}} 1",
                        s.client
                    );
                }
            }
        }
        let (status, _) = self.health();
        let code = match status {
            "ok" => 0,
            "degraded" => 1,
            _ => 2,
        };
        let _ = writeln!(o, "ferrata_updater_health{{{l}}} {code}");
        o
    }

    /// Rounds since the instance's finalized slot last moved (0 before it is active).
    pub fn rounds_without_progress(&self) -> u64 {
        if self.metrics.progress_round == 0 {
            0
        } else {
            self.round.saturating_sub(self.metrics.progress_round)
        }
    }

    pub fn health_json(&self) -> serde_json::Value {
        let (status, why) = self.health();
        let mut v = serde_json::json!({
            "daemon": self.label,
            "chain": self.inst.name,
            "status": status,
            "reasons": why,
            "round": self.round,
            "finalized_slot": self.metrics.finalized_slot,
            "period": self.metrics.period,
            "last_plan": self.metrics.last_plan,
            "last_error": self.metrics.last_error,
            "held": self
                .held
                .iter()
                .filter(|(_, h)| self.holding(h))
                .filter_map(|((jid, step), _)| self.held_back(jid, *step))
                .collect::<Vec<_>>(),
            "paused_until_round": self.breaker.paused_until,
            "failing_sends": self.metrics.failing_sends,
            "last_failure": self.metrics.last_failure,
            "failed_rounds": self.metrics.failed_rounds,
            "last_applied_utc": self.metrics.last_applied_utc,
            "sources_head_slot": self.metrics.sources_head,
            "rounds_without_progress": self.rounds_without_progress(),
            "updated_utc": crate::utc_now(),
        });
        if self.policy.min_distinct_clients >= 2 {
            // What each source reported at /eth/v1/node/version.
            let clients: BTreeMap<&String, &String> =
                self.metrics.sources.iter().map(|(name, s)| (name, &s.client)).collect();
            v["clients"] = serde_json::json!(clients);
        }
        v
    }
}

/// Whether `k` of `pairs` (client family, source) have pairwise different families and pairwise different sources: `k`
/// clients, each from a source of its own.
fn distinct_pairs(pairs: &[(String, usize)], k: usize) -> bool {
    fn pick(pairs: &[(String, usize)], k: usize, families: &mut Vec<String>, sources: &mut Vec<usize>) -> bool {
        if families.len() == k {
            return true;
        }
        for (f, s) in pairs {
            if families.contains(f) || sources.contains(s) {
                continue;
            }
            families.push(f.clone());
            sources.push(*s);
            if pick(pairs, k, families, sources) {
                return true;
            }
            families.pop();
            sources.pop();
        }
        false
    }
    pick(pairs, k, &mut Vec::new(), &mut Vec::new())
}

#[cfg(test)]
mod tests {
    use super::distinct_pairs;

    #[test]
    fn distinct_clients_from_distinct_sources() {
        let p = |v: &[(&str, usize)]| v.iter().map(|(f, s)| (f.to_string(), *s)).collect::<Vec<_>>();
        assert!(!distinct_pairs(&p(&[("nimbus", 0), ("nimbus", 1)]), 2), "one client from two sources");
        assert!(!distinct_pairs(&p(&[("nimbus", 0), ("lodestar", 0)]), 2), "two clients from one source");
        assert!(distinct_pairs(&p(&[("nimbus", 0), ("lodestar", 0), ("lodestar", 1)]), 2));
        assert!(!distinct_pairs(&p(&[("nimbus", 0), ("lodestar", 1)]), 3));
        assert!(distinct_pairs(&p(&[("nimbus", 0), ("lodestar", 1), ("lighthouse", 2)]), 3));
        assert!(!distinct_pairs(&p(&[]), 2));
    }
}

/// The execution block a light-client header binds: the payload header's block hash (Capella to Electra layouts), or
/// the block hash itself (Gloas); none in the Altair layout.
pub fn execution_block_hash(h: &LightClientHeader) -> Option<Root> {
    match &h.execution {
        Execution::Payload { header, .. } => Some(header.block_hash),
        Execution::BlockHash { block_hash, .. } => Some(*block_hash),
        Execution::None => None,
    }
}
