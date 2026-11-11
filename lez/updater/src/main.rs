//! `ferrata-updater --config FILE [--rounds N]`: one updater per configured chain, round after
//! round, `/health` and `/metrics` on `daemon.listen`. SIGTERM or SIGINT: the round in hand
//! finishes, then the daemon exits (nothing else to save: the chain is the state).

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use anyhow::{anyhow, bail, Context as _, Result};
use ferrata_updater::{
    chain::{preset, root},
    config::File,
    ledger::Ledger,
    serve::{serve, Snapshot},
    source::{BeaconApi, Clock, Recorded, Source},
    updater::{Instance, Plan, Updater},
};
use lee::{AccountId, PrivateKey};

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

fn usage() -> ! {
    eprintln!("usage: ferrata-updater --config FILE [--rounds N] [--quiet]");
    std::process::exit(2)
}

/// The chain to write to, and the Ferrata program's account on it.
fn open_ledger(kind: &str, payer: &PrivateKey) -> Result<(Box<dyn Ledger>, AccountId)> {
    match kind {
        #[cfg(feature = "harness")]
        "harness" => {
            let l = ferrata_updater::ledger::HarnessLedger::new(std::slice::from_ref(payer));
            Ok((Box::new(l), ferrata_updater::ledger::HarnessLedger::program()))
        }
        #[cfg(not(feature = "harness"))]
        "harness" => {
            let _ = payer;
            bail!("daemon.ledger = \"harness\" needs a build with --features harness")
        }
        "sequencer" => {
            bail!(
                "daemon.ledger = \"sequencer\": the LEZ sequencer backend is not in this build (lez/updater/README.md)"
            )
        }
        other => bail!("daemon.ledger = {other:?}: harness or sequencer"),
    }
}

fn main() -> Result<()> {
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
    let at = |p: &str| if PathBuf::from(p).is_absolute() { PathBuf::from(p) } else { base.join(p) };

    let key_hex = fs::read_to_string(at(&file.keys.payer_key_file)).context("keys.payer_key_file")?;
    let key_bytes: [u8; 32] = hex::decode(key_hex.trim().trim_start_matches("0x"))?
        .try_into()
        .map_err(|_| anyhow!("the payer key is not 32 bytes"))?;
    let payer = PrivateKey::try_new(key_bytes).map_err(|e| anyhow!("payer key: {e:?}"))?;

    let (mut ledger, program) = open_ledger(&file.daemon.ledger, &payer)?;

    let state_dir = at(&file.daemon.state_dir);
    fs::create_dir_all(&state_dir)?;
    let mut updaters = Vec::new();
    for c in &file.chains {
        let config = preset(&c.preset, c.adoption_parts)?;
        let mut sources: Vec<Box<dyn Source>> = Vec::new();
        for s in &c.sources {
            match (&s.url, &s.dir) {
                (Some(url), _) => sources.push(Box::new(BeaconApi {
                    name: s.name.clone(),
                    base: url.clone(),
                    timeout_secs: s.timeout_secs,
                })),
                (_, Some(dir)) => {
                    sources.push(Box::new(Recorded::open(&s.name, &at(dir), &config, Clock::new(usize::MAX))?))
                }
                _ => unreachable!("checked by config"),
            }
        }
        let inst = Instance::new(&c.name, program, config, root(&c.checkpoint)?);
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
        eprintln!("[{}] /health and /metrics on http://{bound}", file.daemon.name);
    }
    let mut round = 0u64;
    while !STOP.load(Ordering::SeqCst) && rounds.is_none_or(|n| round < n) {
        round += 1;
        for u in updaters.iter_mut() {
            match u.tick(ledger.as_mut()) {
                Ok(Plan::Send(_)) | Ok(Plan::UpToDate) | Ok(Plan::Deferred(_)) => {}
                Ok(Plan::Waiting(w)) | Ok(Plan::Stuck(w)) => eprintln!("[{} {}] {w}", u.label, u.inst.name),
                Err(e) => eprintln!("[{} {}] round {round}: {e:#}", u.label, u.inst.name),
            }
        }
        let metrics: String = updaters.iter().map(|u| u.render_metrics()).collect();
        let health: Vec<_> = updaters.iter().map(|u| u.health_json()).collect();
        let healthy = updaters.iter().all(|u| u.health().0 != "down");
        snapshot.set(metrics, serde_json::to_string_pretty(&health)? + "\n", healthy);
        if file.daemon.poll_secs > 0 && !STOP.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_secs(file.daemon.poll_secs));
        }
    }
    for u in &updaters {
        eprintln!("[{} {}] stopped at round {round}: {}", u.label, u.inst.name, u.health_json());
    }
    Ok(())
}
