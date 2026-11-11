//! One updater for one instance: each round it reads the instance from the chain, gathers
//! candidates from its sources, runs the program's own checks on each against the on-chain
//! store, picks the one that moves the instance furthest, and sends the next step of its job
//! (or of a job another updater left open). It keeps no store of its own: the chain is the
//! state, so a restart, or another updater's work, needs no repair. It keeps the full sync
//! committees the program only summarises (the signer witness needs them), keyed by the
//! summary's `pubkeys_root`, accepted only when they match the summaries on chain.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    io::Write as _,
    path::PathBuf,
};

use anyhow::{anyhow, bail, Context as _, Result};
use ferrata_core::{
    fork::LcFork,
    steps::{self, Job, Phase},
    store::ApplyOutcome,
    types::{LightClientBootstrap, LightClientUpdate, SyncCommittee},
    witness::{uncompressed_keys, CompactBootstrap, CompactStore, CompactUpdate, CommitteeSummary, SignerWitness},
    Root,
};
use ferrata_lez_wire::{
    header_actor, instance_actor, instance_id, instance_seed, job_actor, job_id, Config, InstanceState, JobState, Message,
};
use lee::{AccountId, Actor, PrivateKey, PublicAccountEvidence, PublicKey};
use sha2::{Digest, Sha256};

use crate::{
    chain::{
        committee_key, decode_committee, decode_finality_update, decode_update, encode_committee, layout_for, plan_job,
        signer_witness, signing_committee,
    },
    config::{Fees, Policy},
    ledger::{Ledger, Tx, TxOutcome},
    source::Source,
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

/// A step of the public job path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Init,
    Open,
    Miller,
    Adopt(u8),
    Finish,
}

impl Step {
    pub fn name(&self) -> String {
        match self {
            Step::Init => "init".into(),
            Step::Open => "open".into(),
            Step::Miller => "miller".into(),
            Step::Adopt(p) => format!("adopt{p}"),
            Step::Finish => "finish".into(),
        }
    }
    fn byte(&self) -> u8 {
        match self {
            Step::Init => 0,
            Step::Open => 1,
            Step::Miller => 2,
            Step::Adopt(p) => 3 + p,
            Step::Finish => 255,
        }
    }
}

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
}

/// What a round decided.
#[derive(Clone, Debug)]
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
    /// Rounds it was found lagging.
    pub lagging_rounds: u64,
    pub lagging: bool,
    /// It answered last round.
    pub available: bool,
    pub demoted_until: u64,
    /// The newest finalized slot among the candidates it served last round.
    pub finalized_slot: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub rounds: u64,
    /// (step, result) -> transactions: result `applied`, `refused`, `rejected`.
    pub tx: BTreeMap<(String, &'static str), u64>,
    pub refused_codes: BTreeMap<u32, u64>,
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
}

/// What the program would do with a candidate on the current store.
enum Verdict {
    Useful(Box<Planned>),
    /// Valid or not, it would not move the store (older than what the instance holds).
    NoChange,
    /// Its signature falls in a period whose committee the store does not hold yet.
    Unverifiable,
    /// The program would refuse it.
    Invalid(String),
}

struct Planned {
    update: LightClientUpdate,
    layout: LcFork,
    compact: CompactUpdate,
    witness: SignerWitness,
    jid: Root,
    outcome: ApplyOutcome,
}

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
    first_due: BTreeMap<(Root, Step), u64>,
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
            eprintln!("[{} {}] {line}", self.label, self.inst.name);
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
            if let Ok(bootstrap) = LightClientBootstrap::decode(&b, fork, self.inst.config.sync_committee_size as usize) {
                if self.bootstrap_verifies(&bootstrap) {
                    self.bootstrap = Some(bootstrap);
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
        Ok(())
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

    fn journal(&self, action: &Action, out: &TxOutcome) {
        let Some(dir) = self.dir() else { return };
        let line = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            self.round,
            action.step.name(),
            hex::encode(action.job_id),
            action.finalized_slot,
            if out.applied { "applied" } else if out.rejected { "rejected" } else { "refused" },
            out.exit_code.map(|c| c.to_string()).unwrap_or_default(),
            out.gas_exec,
            out.gas_stor,
            out.fee,
            out.block
        );
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(dir.join("journal.tsv")) {
            let _ = f.write_all(line.as_bytes());
        }
    }

    // ---- sources --------------------------------------------------------------------------

    fn usable(&self, i: usize) -> bool {
        let name = self.sources[i].name();
        self.metrics.sources.get(name).map_or(true, |s| s.demoted_until <= self.round)
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
        self.log(&format!("source {name} served what the program would refuse ({why}); not asked for {} rounds", self.policy.demote_rounds));
    }

    fn bootstrap_verifies(&self, b: &LightClientBootstrap) -> bool {
        let config = &self.inst.config;
        let Some(fork) = config.fork() else { return false };
        steps::initialize(&self.inst.checkpoint, &CompactBootstrap::of(b), fork, &config.spec(), config.adoption_parts).is_ok()
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
            if !self.usable(i) {
                continue;
            }
            self.stats(i).requests += 1;
            let checkpoint = self.inst.checkpoint;
            match self.sources[i].bootstrap(&checkpoint) {
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
        let mut needed = vec![store.current.clone()];
        needed.extend(store.next.clone());
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
            if !self.usable(i) {
                continue;
            }
            self.stats(i).requests += 1;
            match self.sources[i].updates(period.saturating_sub(1), 2) {
                Ok(list) => {
                    for bytes in list {
                        if let Ok((u, _)) = decode_update(&bytes, &self.inst.config) {
                            if !u.next_sync_committee.is_empty() && needed.contains(&CommitteeSummary::of(&u.next_sync_committee)) {
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
            if !self.usable(i) {
                continue;
            }
            let errors = self.stats(i).errors;
            self.stats(i).requests += 1;
            let mut head = period + 1;
            match self.sources[i].finality_update() {
                Ok(bytes) => match decode_finality_update(&bytes, &self.inst.config) {
                    Ok((u, _)) => {
                        head = head.max(spec.period_at_slot(u.attested_header.beacon.slot));
                        out.push(Candidate { source: i, update: u, period_update: false });
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
                            Ok((u, _)) => out.push(Candidate { source: i, update: u, period_update: true }),
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

    fn evaluate(&self, store: &CompactStore, update: &LightClientUpdate) -> Verdict {
        let config = &self.inst.config;
        let spec = config.spec();
        let Some(layout) = layout_for(config, store.fork, update) else {
            return Verdict::Invalid("no light-client layout at its slot".into());
        };
        let Ok(store_u) = steps::store_for_update(store, layout, update.attested_header.beacon.slot, &spec) else {
            return Verdict::Invalid(format!("the store cannot move to layout {layout:?}"));
        };
        let current = self.committees.get(&store.current.pubkeys_root);
        let next = store.next.as_ref().and_then(|n| self.committees.get(&n.pubkeys_root));
        let sig_period = spec.period_at_slot(update.signature_slot);
        let store_period = spec.period_at_slot(store.finalized.slot);
        if sig_period != store_period && sig_period != store_period + 1 {
            return if sig_period < store_period { Verdict::NoChange } else { Verdict::Unverifiable };
        }
        let Some(committee) = signing_committee(store, update, &spec, current, next) else {
            return Verdict::Unverifiable;
        };
        let compact = CompactUpdate::of(update);
        let witness = signer_witness(committee, update);
        let (opened, outcome) = plan_job(&store_u, &compact, &witness, update, config);
        match (opened, outcome) {
            (Some(_), Some(o)) if o.advanced || o.adopted || o.rotated => {
                let jid = job_id(&self.inst.id, &store_u.digest(), &compact.digest(layout));
                Verdict::Useful(Box::new(Planned { update: update.clone(), layout, compact, witness, jid, outcome: o }))
            }
            (Some(_), Some(_)) => Verdict::NoChange,
            _ if !steps::update_advances(&store_u, &compact, &spec)
                && update.finalized_header.beacon.slot <= store.finalized.slot
                && (update.next_sync_committee.is_empty() || store.next.is_some()) =>
            {
                Verdict::NoChange
            }
            _ => Verdict::Invalid("the program's checks refuse it".into()),
        }
    }

    // ---- a round --------------------------------------------------------------------------

    fn defer(&mut self, jid: &Root, step: Step) -> bool {
        let due = *self.first_due.entry((*jid, step)).or_insert(self.round);
        let mut h = Sha256::new();
        h.update(self.tag);
        h.update(jid);
        h.update([step.byte()]);
        let d: [u8; 32] = h.finalize().into();
        let wait = u64::from_le_bytes(d[..8].try_into().expect("8")) % self.policy.defer_window;
        if self.round - due < wait {
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
            self.first_due.retain(|_, r| *r >= horizon);
        }
        let plan = self.plan_inner(ledger);
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
            if self.defer(&jid, step) {
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
                    evidence: vec![PublicAccountEvidence::Pda { program: self.inst.program, seed: instance_seed(&self.inst.id) }],
                    time_ms,
                },
                Step::Adopt(p) => {
                    let keys = uncompressed_keys(&bootstrap.current_sync_committee).map_err(|e| anyhow!("{e:?}"))?;
                    self.adopt_tx(job_actor, &keys, p, Some(finalized_slot), time_ms, "init")
                }
                _ => unreachable!(),
            };
            let store_seen = self.inst.state(ledger)?.map(|s| (s.active, s.store));
            return Ok(Plan::Send(Action { step, job_id: jid, tx, finalized_slot, store_seen }));
        };

        let store = CompactStore::decode(&state.store).map_err(|e| anyhow!("store: {e:?}"))?;
        self.metrics.finalized_slot = store.finalized.slot;
        self.metrics.period = spec.period_at_slot(store.finalized.slot);
        if let Err(e) = self.ensure_committees(&store) {
            return Ok(Plan::Stuck(format!("{e:#}")));
        }
        let candidates = self.candidates(&store);
        if candidates.is_empty() {
            return Ok(Plan::Stuck("no source answered".into()));
        }
        // Judge every candidate with the program's own checks.
        let mut useful: Vec<(usize, Box<Planned>)> = Vec::new();
        let mut best_seen: BTreeMap<usize, u64> = BTreeMap::new();
        let mut seen_digests: BTreeMap<Root, bool> = BTreeMap::new();
        for c in candidates {
            let digest = CompactUpdate::of(&c.update).digest(LcFork::Gloas);
            let verdict = self.evaluate(&store, &c.update);
            match verdict {
                Verdict::Useful(p) => {
                    let e = best_seen.entry(c.source).or_default();
                    *e = (*e).max(p.update.finalized_header.beacon.slot);
                    seen_digests.insert(digest, true);
                    useful.push((c.source, p));
                }
                Verdict::NoChange => {
                    let e = best_seen.entry(c.source).or_default();
                    *e = (*e).max(store.finalized.slot.min(c.update.finalized_header.beacon.slot));
                }
                Verdict::Unverifiable => {}
                Verdict::Invalid(why) => {
                    let kind = if c.period_update { "period update" } else { "finality update" };
                    self.demote(c.source, &format!("{kind} attested at {}: {why}", c.update.attested_header.beacon.slot));
                }
            }
        }
        // Lag: a source whose newest verified finalized slot is far behind the best.
        let best = useful.iter().map(|(_, p)| p.update.finalized_header.beacon.slot).max().unwrap_or(store.finalized.slot);
        for i in 0..self.sources.len() {
            if !self.usable(i) {
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
        // Adopting the next committee comes first when the store lacks it; then the furthest
        // finalized header; then the most signers.
        let needs_next = store.next.is_none();
        let key = |p: &Planned| {
            (
                needs_next && p.outcome.adopted,
                p.update.finalized_header.beacon.slot,
                p.update.sync_aggregate.bits.iter().filter(|b| **b).count(),
            )
        };
        // The best candidate that enough sources serve (its finalized header root).
        let agreement = |p: &Planned| {
            let root = p.update.finalized_header.beacon.hash_tree_root();
            useful
                .iter()
                .filter(|(_, q)| q.update.finalized_header.beacon.hash_tree_root() == root)
                .map(|(s, _)| *s)
                .collect::<std::collections::BTreeSet<usize>>()
                .len()
        };
        let mut order: Vec<usize> = (0..useful.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(key(&useful[i].1)));
        let Some(pick) = order.iter().copied().find(|&i| agreement(&useful[i].1) >= self.policy.min_agreeing) else {
            let best = &useful[order[0]].1;
            return Ok(Plan::Waiting(format!(
                "{} of {} sources serve finalized slot {}",
                agreement(best),
                self.policy.min_agreeing,
                best.update.finalized_header.beacon.slot
            )));
        };
        let chosen = useful.swap_remove(pick).1;
        // The job's next step, as the chain holds it (another updater may have begun it).
        let step = match self.job_on_chain(ledger, &chosen.jid)? {
            None => Step::Open,
            Some(job) => match &job.phase {
                Phase::Miller { .. } => Step::Miller,
                Phase::FinalExp { .. } | Phase::Verified => match job.adoption.as_ref().filter(|a| !a.complete) {
                    Some(a) => Step::Adopt(a.chunk_roots.len() as u8),
                    None => Step::Finish,
                },
            },
        };
        if self.defer(&chosen.jid, step) {
            return Ok(Plan::Deferred(step));
        }
        let tx = self.job_tx(&chosen, step)?;
        Ok(Plan::Send(Action {
            step,
            job_id: chosen.jid,
            tx,
            finalized_slot: chosen.update.finalized_header.beacon.slot,
            store_seen: Some((state.active, state.store.clone())),
        }))
    }

    fn job_on_chain(&self, ledger: &dyn Ledger, jid: &Root) -> Result<Option<Job>> {
        let s = ledger.actor_state(self.inst.job(jid))?;
        if s.is_empty() {
            return Ok(None);
        }
        let js: JobState = borsh::from_slice(&s).context("job state")?;
        Ok(Some(Job::decode(&js.job).map_err(|e| anyhow!("job: {e:?}"))?))
    }

    fn adopt_tx(&self, job_actor: Actor, keys: &[[u8; 96]], p: u8, record_slot: Option<u64>, time_ms: u64, what: &str) -> Tx {
        let parts = self.inst.config.adoption_parts;
        let chunk = keys.len() / parts as usize;
        let mut actors = vec![job_actor];
        if p + 1 == parts {
            if let Some(slot) = record_slot {
                actors.extend([self.inst.actor(), self.inst.header(slot)]);
            }
        }
        Tx {
            label: format!("{} {what} adopt {}/{parts}", self.inst.name, p + 1),
            root: job_actor,
            message: borsh::to_vec(&Message::Adopt { part: p, keys: keys[p as usize * chunk..(p as usize + 1) * chunk].to_vec() })
                .expect("borsh"),
            actors,
            evidence: vec![],
            time_ms,
        }
    }

    fn job_tx(&self, p: &Planned, step: Step) -> Result<Tx> {
        let job_actor = self.inst.job(&p.jid);
        let time_ms = self.inst.time_ms(p.update.signature_slot);
        let fin = p.update.finalized_header.beacon.slot;
        let what = format!("fin {fin}");
        Ok(match step {
            Step::Open => Tx {
                label: format!("{} {what} Open", self.inst.name),
                root: self.inst.actor(),
                message: borsh::to_vec(&Message::Open { update: p.compact.encode(p.layout), signers: p.witness.encode() })?,
                actors: vec![self.inst.actor(), job_actor],
                evidence: vec![],
                time_ms,
            },
            Step::Miller => Tx {
                label: format!("{} {what} Miller", self.inst.name),
                root: job_actor,
                message: borsh::to_vec(&Message::Miller)?,
                actors: vec![job_actor],
                evidence: vec![],
                time_ms,
            },
            Step::Adopt(part) => {
                let keys = uncompressed_keys(&p.update.next_sync_committee).map_err(|e| anyhow!("{e:?}"))?;
                self.adopt_tx(job_actor, &keys, part, None, time_ms, &what)
            }
            Step::Finish => {
                let mut actors = vec![job_actor, self.inst.actor()];
                if p.outcome.advanced {
                    actors.push(self.inst.header(fin));
                }
                Tx {
                    label: format!("{} {what} Finish", self.inst.name),
                    root: job_actor,
                    message: borsh::to_vec(&Message::Finish)?,
                    actors,
                    evidence: vec![],
                    time_ms,
                }
            }
            Step::Init => bail!("not a job step"),
        })
    }

    /// An `Open` for any update, unchecked (what an adversary sends; the tests use it).
    pub fn open_unchecked(&self, ledger: &dyn Ledger, update: &LightClientUpdate) -> Result<Tx> {
        let config = &self.inst.config;
        let spec = config.spec();
        let store = self.inst.store(ledger)?.ok_or_else(|| anyhow!("no active instance"))?;
        let layout = layout_for(config, store.fork, update).unwrap_or(store.fork);
        let store_u = steps::store_for_update(&store, layout, update.attested_header.beacon.slot, &spec).unwrap_or(store.clone());
        let compact = CompactUpdate::of(update);
        let current = self.committees.get(&store.current.pubkeys_root);
        let next = store.next.as_ref().and_then(|n| self.committees.get(&n.pubkeys_root));
        let witness = signing_committee(&store, update, &spec, current, next).map(|c| signer_witness(c, update)).unwrap_or_default();
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
        if recheck && self.step_done(ledger, action)? {
            self.metrics.skipped_done += 1;
            return Ok(None);
        }
        if self.metrics.spent() + self.fees.max_fee_per_tx > self.fees.budget {
            self.metrics.budget_stops += 1;
            bail!("budget reached: spent {} of {}", self.metrics.spent(), self.fees.budget);
        }
        let mut attempt = 0u32;
        loop {
            match ledger.submit(&self.payer, &action.tx, self.fees.gas_limit, self.fees.max_fee_per_tx) {
                Ok(out) => {
                    self.record(action, &out);
                    return Ok(Some(out));
                }
                Err(e) => {
                    self.metrics.transport_errors += 1;
                    self.metrics.last_error = format!("{e:#}");
                    attempt += 1;
                    if attempt >= self.policy.max_attempts {
                        bail!("{}: gave up after {attempt} attempts: {e:#}", action.tx.label);
                    }
                    self.metrics.retries += 1;
                    let wait = self.policy.retry_base_ms.saturating_mul(1 << (attempt - 1).min(20)).min(self.policy.retry_max_ms);
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
            (Step::Adopt(p), Some(j)) => j.adoption.as_ref().map_or(true, |a| a.complete || a.chunk_roots.len() as u8 > p),
            (Step::Finish, Some(_)) | (Step::Init, Some(_)) => false,
        })
    }

    fn record(&mut self, action: &Action, out: &TxOutcome) {
        let result = if out.applied {
            "applied"
        } else if out.rejected {
            "rejected"
        } else {
            "refused"
        };
        *self.metrics.tx.entry((action.step.name(), result)).or_default() += 1;
        let gas = out.gas_exec + out.gas_stor;
        if out.applied {
            self.metrics.gas_applied += gas;
            self.metrics.fee_applied += out.fee;
            let last_adopt = matches!(action.step, Step::Adopt(p) if p + 1 == self.inst.config.adoption_parts) && action.tx.actors.len() > 1;
            if action.step == Step::Finish || last_adopt {
                self.metrics.jobs_finished += 1;
            }
        } else if !out.rejected {
            self.metrics.gas_refused += gas;
            self.metrics.fee_refused += out.fee;
            if let Some(c) = out.exit_code {
                *self.metrics.refused_codes.entry(c).or_default() += 1;
            }
        }
        self.log(&format!("{} {result} (exit {:?}, gas {}, fee {}, block {})", action.tx.label, out.exit_code, gas, out.fee, out.block));
        self.journal(action, out);
    }

    /// One round: plan, then send what is due.
    pub fn tick(&mut self, ledger: &mut dyn Ledger) -> Result<Plan> {
        let plan = self.plan(ledger)?;
        if let Plan::Send(action) = &plan {
            self.submit(ledger, action, false)?;
        }
        Ok(plan)
    }

    // ---- health and metrics -----------------------------------------------------------------

    /// `ok`, `degraded` (a source lies, lags or fails; the budget is reached; the last round
    /// failed) or `down` (no usable source).
    pub fn health(&self) -> (&'static str, Vec<String>) {
        let mut why = Vec::new();
        let usable = (0..self.sources.len()).filter(|&i| self.usable(i)).count();
        for (name, s) in &self.metrics.sources {
            if s.demoted_until > self.round {
                why.push(format!("source {name} demoted (served what the program refuses)"));
            } else if s.lagging {
                why.push(format!("source {name} lagging (finalized slot {})", s.finalized_slot));
            } else if !s.available && s.requests > 0 {
                why.push(format!("source {name} unavailable"));
            }
        }
        if self.metrics.spent() + self.fees.max_fee_per_tx > self.fees.budget {
            why.push("budget reached".into());
        }
        if self.metrics.last_plan.starts_with("stuck") || self.metrics.last_plan.starts_with("error") {
            why.push(self.metrics.last_plan.clone());
        }
        let status = if usable == 0 {
            "down"
        } else if why.is_empty() {
            "ok"
        } else {
            "degraded"
        };
        (status, why)
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
        let _ = writeln!(o, "ferrata_updater_gas_total{{{l},result=\"applied\"}} {}", m.gas_applied);
        let _ = writeln!(o, "ferrata_updater_gas_total{{{l},result=\"refused\"}} {}", m.gas_refused);
        let _ = writeln!(o, "ferrata_updater_fee_total{{{l},result=\"applied\"}} {}", m.fee_applied);
        let _ = writeln!(o, "ferrata_updater_fee_total{{{l},result=\"refused\"}} {}", m.fee_refused);
        let _ = writeln!(o, "ferrata_updater_budget_remaining{{{l}}} {}", self.fees.budget.saturating_sub(m.spent()));
        let _ = writeln!(o, "ferrata_updater_transport_errors_total{{{l}}} {}", m.transport_errors);
        let _ = writeln!(o, "ferrata_updater_retries_total{{{l}}} {}", m.retries);
        let _ = writeln!(o, "ferrata_updater_deferrals_total{{{l}}} {}", m.deferrals);
        let _ = writeln!(o, "ferrata_updater_steps_done_by_others_total{{{l}}} {}", m.skipped_done);
        let _ = writeln!(o, "ferrata_updater_jobs_finished_total{{{l}}} {}", m.jobs_finished);
        let _ = writeln!(o, "ferrata_updater_finalized_slot{{{l}}} {}", m.finalized_slot);
        let _ = writeln!(o, "ferrata_updater_period{{{l}}} {}", m.period);
        for (name, s) in &m.sources {
            let sl = format!("{l},source=\"{name}\"");
            let _ = writeln!(o, "ferrata_updater_source_requests_total{{{sl}}} {}", s.requests);
            let _ = writeln!(o, "ferrata_updater_source_errors_total{{{sl}}} {}", s.errors);
            let _ = writeln!(o, "ferrata_updater_source_refused_total{{{sl}}} {}", s.refused);
            let _ = writeln!(o, "ferrata_updater_source_lagging{{{sl}}} {}", u8::from(s.lagging));
            let _ = writeln!(o, "ferrata_updater_source_available{{{sl}}} {}", u8::from(s.available));
            let _ = writeln!(o, "ferrata_updater_source_demoted{{{sl}}} {}", u8::from(s.demoted_until > self.round));
            let _ = writeln!(o, "ferrata_updater_source_finalized_slot{{{sl}}} {}", s.finalized_slot);
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

    pub fn health_json(&self) -> serde_json::Value {
        let (status, why) = self.health();
        serde_json::json!({
            "daemon": self.label,
            "chain": self.inst.name,
            "status": status,
            "reasons": why,
            "round": self.round,
            "finalized_slot": self.metrics.finalized_slot,
            "period": self.metrics.period,
            "last_plan": self.metrics.last_plan,
            "last_error": self.metrics.last_error,
        })
    }
}
