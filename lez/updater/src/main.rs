//! `ferrata-updater --config FILE [--rounds N] [--quiet]` with the in-process chain (`daemon.ledger = "harness"`,
//! feature `harness`). The build that writes to a running LEZ sequencer (`daemon.ledger = "sequencer"`) is
//! `sequencer/`: it links LEZ's node-side crates, so it is a workspace of its own. The daemon is
//! `ferrata_updater::daemon`.

use std::path::Path;

use anyhow::{bail, Result};
use ferrata_updater::{config::File, ledger::Ledger};
use lee::{AccountId, PrivateKey};

fn open_ledger(file: &File, _base: &Path, payer: &PrivateKey) -> Result<(Box<dyn Ledger>, AccountId)> {
    match file.daemon.ledger.as_str() {
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
            bail!("daemon.ledger = \"sequencer\": this build has no sequencer backend; build lez/updater/sequencer")
        }
        other => bail!("daemon.ledger = {other:?}: harness or sequencer"),
    }
}

fn main() -> Result<()> {
    ferrata_updater::daemon::run(open_ledger)
}
