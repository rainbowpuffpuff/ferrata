//! `ferrata-updater --config FILE [--rounds N]`: one updater per configured chain, round after
//! round, `/health` and `/metrics` on `daemon.listen`. SIGTERM or SIGINT: the round in hand
//! finishes, then the daemon exits (nothing else to save: the chain is the state).

use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use anyhow::{Context as _, Result};
use ferrata_updater::{
    daemon::Daemon,
    serve::{serve, Snapshot},
};

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

fn usage() -> ! {
    eprintln!("usage: ferrata-updater --config FILE [--rounds N] [--quiet]");
    std::process::exit(2)
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
    let mut d = Daemon::open(&path, verbose)?;

    unsafe {
        libc::signal(libc::SIGTERM, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t);
    }
    let snapshot = Snapshot::default();
    if let Some(addr) = &d.file.daemon.listen {
        let bound = serve(addr, snapshot.clone())?;
        eprintln!("[{}] /health and /metrics on http://{bound}", d.file.daemon.name);
    }
    while !STOP.load(Ordering::SeqCst) && rounds.is_none_or(|n| d.round < n) {
        for note in d.round() {
            eprintln!("{note}");
        }
        let (health, healthy) = d.health();
        snapshot.set(d.metrics(), serde_json::to_string_pretty(&health)? + "\n", healthy);
        if d.file.daemon.poll_secs > 0 && !STOP.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_secs(d.file.daemon.poll_secs));
        }
    }
    for u in &d.updaters {
        eprintln!("[{} {}] stopped at round {}: {}", u.label, u.inst.name, d.round, u.health_json());
    }
    Ok(())
}
