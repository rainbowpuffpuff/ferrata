//! `ferrata-updater --config FILE [--rounds N] [--quiet]` writing to a running LEZ sequencer: `daemon.ledger =
//! "sequencer"` with a `[sequencer]` section (`../ferrata-updater.example.toml`). The daemon is
//! `ferrata_updater::daemon` (the same as the harness build's and the Logos module's); the chain backend is this
//! crate's library ([`ferrata_updater_sequencer::open_ledger`]).

fn main() -> anyhow::Result<()> {
    ferrata_updater::daemon::run(ferrata_updater_sequencer::open_ledger)
}
