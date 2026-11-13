//! `ferrata-updater --config FILE [--rounds N] [--quiet]` with the in-process chain (`daemon.ledger = "harness"`,
//! feature `harness`). The build that writes to a running LEZ sequencer (`daemon.ledger = "sequencer"`) is
//! `sequencer/`: it links LEZ's node-side crates, so it is a workspace of its own. The daemon, its set-up and its
//! rounds are `ferrata_updater::daemon`, which the Logos module (`modules/ferrata_updater_module`) drives too.

fn main() -> anyhow::Result<()> {
    ferrata_updater::daemon::run(ferrata_updater::daemon::open_ledger)
}
