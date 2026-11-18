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
    types::{LightClientBootstrap, LightClientUpdate, SyncCommittee},
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
    source::{client_family, Source},
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

// What the program would do with a candidate, and the job of an accepted one: ferrata-advance's.
use ferrata_advance::{Planned, Verdict};

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
    first_due: BTreeMap<(Root, Step), u64>,
    /// Updates of jobs this updater opened, by job id: (period update, the bytes a source served). Until the job is
    /// finished or moot (`policy.finish_own_jobs`).
    carried: BTreeMap<Root, (bool, Vec<u8>)>,
    /// The Open this round chose, kept once it is applied.
    opening: Option<(Root, bool, Vec<u8>)>,
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
            opening: None,
            confirmed: BTreeMap::new(),
            clients: Vec::new(),
            disagreed: BTreeSet::new(),
            served_by: BTreeMap::new(),
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

    fn carry(&mut self, jid: Root, period_update: bool, raw: Vec<u8>) {
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
    /// fee, block, the time it was settled (UTC), its label.
    fn journal(&self, action: &Action, out: &TxOutcome) {
        let Some(dir) = self.dir() else { return };
        let line = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
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
            action.tx.label
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

    fn evaluate(&self, store: &CompactStore, update: &LightClientUpdate) -> Verdict {
        let current = self.committees.get(&store.current.pubkeys_root);
        let next = store.next.as_ref().and_then(|n| self.committees.get(&n.pubkeys_root));
        ferrata_advance::evaluate(&self.inst.config, &self.inst.id, store, update, current, next)
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
            return Ok(Plan::Send(Action { step, job_id: jid, tx, finalized_slot, store_seen }));
        };

        let store = CompactStore::decode(&state.store).map_err(|e| anyhow!("store: {e:?}"))?;
        self.metrics.finalized_slot = store.finalized.slot;
        self.metrics.period = spec.period_at_slot(store.finalized.slot);
        if let Err(e) = self.ensure_committees(&store) {
            return Ok(Plan::Stuck(format!("{e:#}")));
        }
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
            agreeing.extend(self.confirm_header(header.slot, &root, &mut asked));
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
        let plan = self.next_step(ledger, chosen, (period_update, raw), &state)?;
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
    fn next_step(
        &mut self,
        ledger: &dyn Ledger,
        chosen: Planned,
        served: (bool, Vec<u8>),
        state: &InstanceState,
    ) -> Result<Plan> {
        let step = ferrata_advance::next_step(self.job_on_chain(ledger, &chosen.jid)?.as_ref());
        if self.defer(&chosen.jid, step) {
            return Ok(Plan::Deferred(step));
        }
        let tx = self.job_tx(&chosen, step)?;
        if step == Step::Open {
            self.opening = Some((chosen.jid, served.0, served.1));
        }
        Ok(Plan::Send(Action {
            step,
            job_id: chosen.jid,
            tx,
            finalized_slot: chosen.update.finalized_header.beacon.slot,
            store_seen: Some((state.active, state.store.clone())),
        }))
    }

    /// The next step of a job this updater opened whose update still moves the store (`policy.finish_own_jobs`); a
    /// kept job that is finished, gone or moot is forgotten.
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
                Verdict::Useful(p) if p.jid == jid && self.job_on_chain(ledger, &jid)?.is_some() => {
                    let plan = self.next_step(ledger, *p, (period_update, raw), state)?;
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
        asked: &mut BTreeMap<(usize, u64), Option<Root>>,
    ) -> Vec<usize> {
        let mut out = Vec::new();
        for i in 0..self.sources.len() {
            if self.sources[i].light_client() || !self.usable(i) {
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
                    match self.sources[i].canonical_root(slot) {
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
                let theirs = answer.map_or("no block".to_string(), |r| format!("0x{}", hex::encode(r)));
                self.log(&format!(
                    "source {name} holds {theirs} canonical at slot {slot}, not 0x{}",
                    hex::encode(root)
                ));
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
        if recheck && self.step_done(ledger, action)? {
            self.metrics.skipped_done += 1;
            return Ok(None);
        }
        if self.metrics.spent() + u128::from(self.fees.max_fee_per_tx) > u128::from(self.fees.budget) {
            self.metrics.budget_stops += 1;
            bail!("budget reached: spent {} of {}", self.metrics.spent(), self.fees.budget);
        }
        let mut attempt = 0u32;
        loop {
            match ledger.submit(&self.payer, &action.tx, self.fees.gas_limit, u128::from(self.fees.max_fee_per_tx)) {
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
                    let wait = self
                        .policy
                        .retry_base_ms
                        .saturating_mul(1 << (attempt - 1).min(20))
                        .min(self.policy.retry_max_ms);
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
            (Step::Adopt(p), Some(j)) => {
                j.adoption.as_ref().is_none_or(|a| a.complete || a.chunk_roots.len() as u8 > p)
            }
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
            if action.step == Step::Open && self.policy.finish_own_jobs {
                if let Some((jid, period_update, raw)) = self.opening.take().filter(|o| o.0 == action.job_id) {
                    self.carry(jid, period_update, raw);
                }
            }
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
        } else if !out.rejected {
            self.metrics.gas_refused += gas;
            self.metrics.fee_refused += out.fee;
            if let Some(c) = out.exit_code {
                *self.metrics.refused_codes.entry(c).or_default() += 1;
            }
        }
        self.log(&format!(
            "{} {result} (exit {:?}, gas {}, fee {}, block {})",
            action.tx.label, out.exit_code, gas, out.fee, out.block
        ));
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
            } else if s.mismatch {
                why.push(format!("source {name} holds another block canonical at a finalized header's slot"));
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
        if self.metrics.spent() + u128::from(self.fees.max_fee_per_tx) > u128::from(self.fees.budget) {
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
