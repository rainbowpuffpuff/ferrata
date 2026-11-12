//! The daemon: `ferrata-updater --config FILE [--rounds N] [--quiet]`. One updater per configured chain, round after
//! round, `/health` and `/metrics` on `daemon.listen`. SIGTERM or SIGINT: the round in hand finishes, the backend
//! saves what it keeps, then the daemon exits 0 (the chain is the state). The chain backend comes from the binary:
//! `src/main.rs` (the in-process harness, feature `harness`) or `sequencer/` (a running LEZ sequencer).

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context as _, Result};
use lee::{AccountId, PrivateKey, PublicKey};

use crate::{
    chain::{instance_config, root},
    config::{File, SourceKind},
    ledger::Ledger,
    serve::{serve, Snapshot},
    source::{BeaconApi, Clock, HeaderWitness, Recorded, Source},
    updater::{Instance, Plan, Updater},
    utc_now,
};

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

/// Opens the chain a configuration names (`daemon.ledger`) for a payer: the backend and the attestation program's
/// account on it. `base` is the configuration file's directory (relative paths are relative to it).
pub type OpenLedger = fn(&File, &Path, &PrivateKey) -> Result<(Box<dyn Ledger>, AccountId)>;

/// A path of the configuration, relative to its file.
pub fn at(base: &Path, p: &str) -> PathBuf {
    if Path::new(p).is_absolute() {
        PathBuf::from(p)
    } else {
        base.join(p)
    }
}

fn usage() -> ! {
    eprintln!("usage: ferrata-updater --config FILE [--rounds N] [--quiet]");
    std::process::exit(2)
}

/// How often the backend saves what it keeps while the daemon runs.
const FLUSH_EVERY: Duration = Duration::from_secs(300);

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
    let file = File::load(&path)?;
    let base = path.parent().map(PathBuf::from).unwrap_or_default();

    let key_hex = fs::read_to_string(at(&base, &file.keys.payer_key_file)).context("keys.payer_key_file")?;
    let key_bytes: [u8; 32] = hex::decode(key_hex.trim().trim_start_matches("0x"))?
        .try_into()
        .map_err(|_| anyhow!("the payer key is not 32 bytes"))?;
    let payer = PrivateKey::try_new(key_bytes).map_err(|e| anyhow!("payer key: {e:?}"))?;

    let state_dir = at(&base, &file.daemon.state_dir);
    fs::create_dir_all(&state_dir)?;
    let (mut ledger, program) = open(&file, &base, &payer)?;

    let mut updaters = Vec::new();
    for c in &file.chains {
        let config = instance_config(&c.preset, c.adoption_parts, c.layout.as_deref())?;
        let mut sources: Vec<Box<dyn Source>> = Vec::new();
        for s in &c.sources {
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
                    sources.push(Box::new(Recorded::open(&s.name, &at(&base, dir), &config, Clock::new(usize::MAX))?))
                }
                _ => unreachable!("checked by config"),
            }
        }
        let inst = Instance::new(&c.name, program, config, root(&c.checkpoint)?);
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

    unsafe {
        libc::signal(libc::SIGTERM, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t);
    }
    let snapshot = Snapshot::default();
    if let Some(addr) = &file.daemon.listen {
        let bound = serve(addr, snapshot.clone())?;
        eprintln!("{} [{}] /health and /metrics on http://{bound}", utc_now(), file.daemon.name);
    }
    let mut round = 0u64;
    let mut flushed = Instant::now();
    while !STOP.load(Ordering::SeqCst) && rounds.is_none_or(|n| round < n) {
        round += 1;
        for u in updaters.iter_mut() {
            match u.tick(ledger.as_mut()) {
                Ok(Plan::Send(_)) | Ok(Plan::UpToDate) | Ok(Plan::Deferred(_)) => {}
                Ok(Plan::Waiting(w)) | Ok(Plan::Stuck(w)) => {
                    eprintln!("{} [{} {}] round {round}: {w}", utc_now(), u.label, u.inst.name)
                }
                Err(e) => eprintln!("{} [{} {}] round {round}: {e:#}", utc_now(), u.label, u.inst.name),
            }
        }
        let metrics: String = updaters.iter().map(|u| u.render_metrics()).collect();
        let health: Vec<_> = updaters.iter().map(|u| u.health_json()).collect();
        let healthy = updaters.iter().all(|u| u.health().0 != "down");
        snapshot.set(metrics, serde_json::to_string_pretty(&health)? + "\n", healthy);
        if flushed.elapsed() >= FLUSH_EVERY {
            if let Err(e) = ledger.flush() {
                eprintln!("{} [{}] saving the backend's state: {e:#}", utc_now(), file.daemon.name);
            }
            flushed = Instant::now();
        }
        // Sleep in short steps, so that a signal ends the wait.
        let until = Instant::now() + Duration::from_secs(file.daemon.poll_secs);
        while !STOP.load(Ordering::SeqCst) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(200).min(until.saturating_duration_since(Instant::now())));
        }
    }
    ledger.flush().context("saving the backend's state")?;
    for u in &updaters {
        eprintln!("{} [{} {}] stopped at round {round}: {}", utc_now(), u.label, u.inst.name, u.health_json());
    }
    Ok(())
}
