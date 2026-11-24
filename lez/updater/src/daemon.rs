//! The daemon: its set-up and its rounds, shared by the binaries and the Logos module
//! (`modules/ferrata_updater_module`): one configuration file, one ledger, one updater per configured chain, a round
//! that ticks each of them, and the health and metrics a round leaves ([`Daemon`]); and the binaries' main loop
//! ([`run`]: `ferrata-updater --config FILE [--rounds N] [--quiet]`, `/health` and `/metrics` on `daemon.listen`;
//! SIGTERM or SIGINT: the round in hand finishes, the backend saves what it keeps, then the daemon exits 0, the chain
//! being the state). The chain backend comes from the build: [`open_ledger`] (the in-process harness, feature
//! `harness`: `src/main.rs` and the module) or `sequencer/` (a running LEZ sequencer: a binary and workspace of its own,
//! which passes its opener to [`run`]).

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context as _, Result};
use lee::{AccountId, Actor, PrivateKey, PublicKey};

use crate::{
    chain::{instance_config, root},
    config::{File, SourceKind},
    ledger::{machine_now_ms, Ledger, Tx, TxOutcome},
    serve::{serve, stale_after, Snapshot},
    source::{BeaconApi, BeaconApiModule, Clock, EthRpcModule, ExecutionRpc, HeaderWitness, Recorded, Source},
    updater::{Instance, Plan, Updater},
    utc_now,
};

/// Opens the chain a configuration names (`daemon.ledger`) for a payer: the backend and the attestation program's
/// account on it. The path is the configuration file's directory (relative paths are relative to it).
pub type OpenLedger = fn(&File, &Path, &PrivateKey) -> Result<(Box<dyn Ledger>, AccountId)>;

/// This crate's backend: `harness` (LEZ's state machine in process, feature `harness`). `sequencer` is the build of
/// `sequencer/`, which links LEZ's node-side crates (a workspace of its own) and opens its backend itself.
pub fn open_ledger(file: &File, _base: &Path, payer: &PrivateKey) -> Result<(Box<dyn Ledger>, AccountId)> {
    match file.daemon.ledger.as_str() {
        #[cfg(feature = "harness")]
        "harness" => {
            let l = crate::ledger::HarnessLedger::new(std::slice::from_ref(payer));
            Ok((Box::new(l), crate::ledger::HarnessLedger::program()))
        }
        #[cfg(not(feature = "harness"))]
        "harness" => {
            let _ = payer;
            bail!("daemon.ledger = \"harness\" needs a build with --features harness")
        }
        "sequencer" => bail!(
            "daemon.ledger = \"sequencer\": the LEZ sequencer backend is not in this build (it is the build of \
             lez/updater/sequencer, which links LEZ's node-side crates)"
        ),
        other => bail!("daemon.ledger = {other:?}: harness or sequencer"),
    }
}

/// A chain backend whose blocks carry the time they are made, read from this machine's clock unless the backend
/// knows better: what the updater checks the program's validity windows against (`Ledger::now_ms`). The daemon puts
/// every backend but the harness (whose blocks take each transaction's own time) behind it. A sequencer on this
/// machine stamps its blocks with the same clock; the one remote sequencer the owner may name for the day-one
/// deployment (`FERRATA_SEQUENCER_REMOTE`, sending only with `FERRATA_TESTNET_SEND=yes`: `chain::check_loopback`, the
/// live driver's `check_send`) stamps them with its own, so `updater::WINDOW_MARGIN_MS` covers a small skew and a
/// refusal by a larger one is held like any window refusal (4, 8, ... rounds), within the breaker's bound.
pub struct MachineClock(pub Box<dyn Ledger>);

impl Ledger for MachineClock {
    fn actor_state(&self, actor: Actor) -> Result<Vec<u8>> {
        self.0.actor_state(actor)
    }
    fn submit(&mut self, payer: &PrivateKey, tx: &Tx, gas_limit: u64, max_fee: u128) -> Result<TxOutcome> {
        self.0.submit(payer, tx, gas_limit, max_fee)
    }
    fn flush(&mut self) -> Result<()> {
        self.0.flush()
    }
    fn now_ms(&self) -> Option<u64> {
        self.0.now_ms().or_else(machine_now_ms)
    }
}

/// A path of the configuration, relative to its file's directory `base`.
pub fn at(base: &Path, p: &str) -> PathBuf {
    if Path::new(p).is_absolute() {
        PathBuf::from(p)
    } else {
        base.join(p)
    }
}

/// How often the backend saves what it keeps while the daemon runs (and when it stops: [`Daemon::flush`]).
pub const FLUSH_EVERY: Duration = Duration::from_secs(300);

/// Everything a configuration file sets up: the ledger and one updater per chain.
pub struct Daemon {
    pub file: File,
    pub ledger: Box<dyn Ledger>,
    pub updaters: Vec<Updater>,
    pub round: u64,
    flushed: Instant,
}

impl Daemon {
    /// [`Daemon::open_with`] this crate's backend ([`open_ledger`]): the harness binary's and the module's.
    pub fn open(path: &Path, verbose: bool) -> Result<Self> {
        Self::open_with(path, verbose, open_ledger)
    }

    /// Reads the configuration (relative paths in it are relative to its directory) and the payer key, makes the
    /// state directory, opens the ledger with `open` and each chain's sources (light-client, headers or recorded).
    pub fn open_with(path: &Path, verbose: bool, open: OpenLedger) -> Result<Self> {
        let file = File::load(path)?;
        let base = path.parent().map(PathBuf::from).unwrap_or_default();

        let key_hex = fs::read_to_string(at(&base, &file.keys.payer_key_file)).context("keys.payer_key_file")?;
        let key_bytes: [u8; 32] = hex::decode(key_hex.trim().trim_start_matches("0x"))?
            .try_into()
            .map_err(|_| anyhow!("the payer key is not 32 bytes"))?;
        let payer = PrivateKey::try_new(key_bytes).map_err(|e| anyhow!("payer key: {e:?}"))?;

        let state_dir = at(&base, &file.daemon.state_dir);
        fs::create_dir_all(&state_dir)?;
        let (ledger, program) = open(&file, &base, &payer)?;
        let ledger: Box<dyn Ledger> =
            if file.daemon.ledger == "harness" { ledger } else { Box::new(MachineClock(ledger)) };

        let mut updaters = Vec::new();
        for c in &file.chains {
            let config = instance_config(&c.preset, c.adoption_parts, c.layout.as_deref())?;
            let mut sources: Vec<Box<dyn Source>> = Vec::new();
            for s in &c.sources {
                if s.kind == SourceKind::Execution {
                    match (&s.url, &s.via) {
                        (Some(url), _) => sources.push(Box::new(ExecutionRpc::new(&s.name, url, s.timeout_secs))),
                        (None, Some(_)) => sources.push(Box::new(EthRpcModule {
                            name: s.name.clone(),
                            chain_id: s.chain_id.expect("checked by config"),
                            timeout_secs: s.timeout_secs,
                            routes: Vec::new(),
                        })),
                        _ => unreachable!("checked by config"),
                    }
                    continue;
                }
                if s.via.is_some() {
                    sources.push(Box::new(BeaconApiModule {
                        name: s.name.clone(),
                        chain_id: s.chain_id.expect("checked by config"),
                        provider: s.provider.clone().unwrap_or_default(),
                        timeout_secs: s.timeout_secs,
                        answered: Vec::new(),
                    }));
                    continue;
                }
                match (&s.url, &s.dir, s.kind) {
                    (Some(url), _, SourceKind::Headers) => sources.push(Box::new(HeaderWitness {
                        name: s.name.clone(),
                        base: url.clone(),
                        timeout_secs: s.timeout_secs,
                    })),
                    (Some(url), _, SourceKind::LightClient) => sources.push(Box::new(BeaconApi {
                        name: s.name.clone(),
                        base: url.clone(),
                        timeout_secs: s.timeout_secs,
                    })),
                    (_, Some(dir), _) => {
                        let mut r = Recorded::open(&s.name, &at(&base, dir), &config, Clock::new(usize::MAX))?;
                        r.client = s.client.clone();
                        sources.push(Box::new(r))
                    }
                    _ => unreachable!("checked by config"),
                }
            }
            let inst = Instance::new(&c.name, program, config, root(&c.checkpoint)?);
            if verbose {
                eprintln!(
                    "{} [{} {}] instance 0x{} of program {program} (checkpoint 0x{}); payer {}; ledger {}",
                    utc_now(),
                    file.daemon.name,
                    c.name,
                    hex::encode(inst.id),
                    hex::encode(inst.checkpoint),
                    AccountId::from(&PublicKey::new_from_private_key(&payer)),
                    file.daemon.ledger
                );
            }
            let mut u = Updater::new(
                &file.daemon.name,
                inst,
                payer.clone(),
                file.fees.clone(),
                file.policy.clone(),
                sources,
                Some(state_dir.clone()),
            )?;
            u.verbose = verbose;
            updaters.push(u);
        }
        Ok(Self { file, ledger, updaters, round: 0, flushed: Instant::now() })
    }

    /// One round: each updater's tick, and every [`FLUSH_EVERY`] the backend's save. Returns what the binary prints
    /// for it (waits, stuck jobs, errors), each line `[daemon chain] round N: ...`.
    pub fn round(&mut self) -> Vec<String> {
        self.round += 1;
        let mut notes = Vec::new();
        for u in self.updaters.iter_mut() {
            match u.tick(self.ledger.as_mut()) {
                Ok(Plan::Send(_)) | Ok(Plan::UpToDate) | Ok(Plan::Deferred(_)) => {}
                Ok(Plan::Waiting(w)) | Ok(Plan::Stuck(w)) => {
                    notes.push(format!("[{} {}] round {}: {w}", u.label, u.inst.name, self.round))
                }
                Err(e) => notes.push(format!("[{} {}] round {}: {e:#}", u.label, u.inst.name, self.round)),
            }
        }
        if self.flushed.elapsed() >= FLUSH_EVERY {
            if let Err(e) = self.flush() {
                notes.push(format!("[{}] saving the backend's state: {e:#}", self.file.daemon.name));
            }
        }
        notes
    }

    /// Save what the backend keeps between runs (the sequencer backend: its follower of the chain; the harness keeps
    /// nothing). The rounds call it every [`FLUSH_EVERY`]; whoever stops the daemon calls it once more.
    pub fn flush(&mut self) -> Result<()> {
        self.flushed = Instant::now();
        self.ledger.flush()
    }

    /// `/metrics` (Prometheus text) as a round leaves it.
    pub fn metrics(&self) -> String {
        self.updaters.iter().map(|u| u.render_metrics()).collect()
    }

    /// `/health`: one JSON object per instance; and whether no instance is down.
    pub fn health(&self) -> (Vec<serde_json::Value>, bool) {
        (self.updaters.iter().map(|u| u.health_json()).collect(), self.updaters.iter().all(|u| u.health().0 != "down"))
    }

    /// Whether every updater found its instance up to date in the last round.
    pub fn up_to_date(&self) -> bool {
        self.updaters.iter().all(|u| u.metrics.last_plan == "up-to-date")
    }

    /// Each instance as the chain holds it: finalized slot and header root, period, jobs applied.
    pub fn instances(&self) -> Vec<serde_json::Value> {
        self.updaters
            .iter()
            .map(|u| {
                let store = u.inst.store(self.ledger.as_ref());
                let state = u.inst.state(self.ledger.as_ref());
                match (store, state) {
                    (Ok(Some(s)), Ok(st)) => serde_json::json!({
                        "chain": u.inst.name, "active": true, "finalized_slot": s.finalized.slot,
                        "finalized_root": hex::encode(s.finalized.hash_tree_root()),
                        "period": u.metrics.period, "applied": st.map_or(0, |x| x.applied),
                    }),
                    (Ok(None), _) => serde_json::json!({ "chain": u.inst.name, "active": false }),
                    (Err(e), _) | (_, Err(e)) => serde_json::json!({ "chain": u.inst.name, "error": format!("{e:#}") }),
                }
            })
            .collect()
    }
}

// ---- the binaries' main loop --------------------------------------------------------------------

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

fn usage() -> ! {
    eprintln!("usage: ferrata-updater --config FILE [--rounds N] [--quiet]");
    std::process::exit(2)
}

/// `ferrata-updater --config FILE [--rounds N] [--quiet]` over the backend `open` opens: rounds until SIGTERM, SIGINT
/// or `--rounds`, `/health` and `/metrics` on `daemon.listen`, each line of the log stamped with the time (UTC); at
/// the end the backend saves what it keeps.
pub fn run(open: OpenLedger) -> Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut config, mut rounds, mut verbose) = (None, None, true);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--config" => config = Some(PathBuf::from(args.next().unwrap_or_else(|| usage()))),
            "--rounds" => rounds = Some(args.next().unwrap_or_else(|| usage()).parse::<u64>().context("--rounds")?),
            "--quiet" => verbose = false,
            _ => usage(),
        }
    }
    let path = config.unwrap_or_else(|| usage());
    let mut d = Daemon::open_with(&path, verbose, open)?;

    unsafe {
        libc::signal(libc::SIGTERM, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t);
    }
    // `/health` answers 503 once no round has refreshed it for ten polls' worth of time, and at least STALE_AFTER (a
    // round hung in a settle wait or on a source is not reported healthy).
    let snapshot = Snapshot::default().with_stale_after(stale_after(d.file.daemon.poll_secs));
    if let Some(addr) = &d.file.daemon.listen {
        let bound = serve(addr, snapshot.clone())?;
        eprintln!("{} [{}] /health and /metrics on http://{bound}", utc_now(), d.file.daemon.name);
    }
    while !STOP.load(Ordering::SeqCst) && rounds.is_none_or(|n| d.round < n) {
        for note in d.round() {
            eprintln!("{} {note}", utc_now());
        }
        let (health, healthy) = d.health();
        snapshot.set(d.metrics(), serde_json::to_string_pretty(&health)? + "\n", healthy);
        // Sleep in short steps, so that a signal ends the wait.
        let until = Instant::now() + Duration::from_secs(d.file.daemon.poll_secs);
        while !STOP.load(Ordering::SeqCst) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(200).min(until.saturating_duration_since(Instant::now())));
        }
    }
    d.flush().context("saving the backend's state")?;
    for u in &d.updaters {
        eprintln!("{} [{} {}] stopped at round {}: {}", utc_now(), u.label, u.inst.name, d.round, u.health_json());
    }
    Ok(())
}
