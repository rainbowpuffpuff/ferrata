//! The backend for a running LEZ sequencer (`daemon.ledger = "sequencer"` with a `[sequencer]` section,
//! `../ferrata-updater.example.toml`): [`open_ledger`] opens it for `ferrata_updater::daemon`, the daemon the binary
//! (`src/main.rs`) and the Logos module (`modules/ferrata_updater_module`, which links this library) both run. The chain
//! backend is [`ledger::SequencerLedger`]. The live driver's modules (`../../live/driver/src`) are compiled in by path,
//! as `../../crown/driver` does: the wallet connection and the follower of the chain that read every transaction's
//! result on blocks final on Bedrock.

#![allow(dead_code)]

#[path = "../../../live/driver/src/blocklog.rs"]
mod blocklog;
#[path = "../../../live/driver/src/follower.rs"]
mod follower;
#[path = "../../../live/driver/src/live.rs"]
mod live;
#[path = "../../../live/driver/src/manifest.rs"]
mod manifest;
#[path = "../../../live/driver/src/plan.rs"]
mod plan;
#[path = "../../../live/driver/src/programs.rs"]
mod programs;

pub mod day1;
pub mod ledger;

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use ferrata_updater::{config::File, daemon::at, ledger::Ledger};
use lee::{AccountId, PrivateKey};

/// The opener for `ferrata_updater::daemon` ([`ferrata_updater::daemon::OpenLedger`]): `sequencer` (the running LEZ
/// sequencer of `[sequencer]`, connected and followed up to its last final block before the daemon's first round);
/// `harness` is refused here (the in-process chain is `ferrata_updater::daemon::open_ledger`, feature `harness`).
pub fn open_ledger(file: &File, base: &Path, payer: &PrivateKey) -> Result<(Box<dyn Ledger>, AccountId)> {
    match file.daemon.ledger.as_str() {
        "sequencer" => {
            let s = file.sequencer.as_ref().ok_or_else(|| anyhow!("daemon.ledger = \"sequencer\" needs [sequencer]"))?;
            let state_dir = at(base, &file.daemon.state_dir);
            let run_dir = s.run_dir.as_deref().map_or_else(|| state_dir.join("ledger"), |d| at(base, d));
            let program = program_account(&s.program)?;
            let opts = live::Options {
                rpc: s.rpc.clone(),
                run_dir: run_dir.clone(),
                out_dir: run_dir,
                // No harness tables beside the manifest: the columns stay empty.
                harness_results: PathBuf::from("/nonexistent"),
                sequencer_log: s.log.as_deref().map(|p| at(base, p)),
                gas_limit: file.fees.gas_limit,
                wait_final: s.wait_final,
            };
            let l = ledger::SequencerLedger::open(opts, payer)?;
            Ok((Box::new(l), program))
        }
        "harness" => bail!(
            "this build writes to a LEZ sequencer (daemon.ledger = \"sequencer\"); the in-process chain is the build of \
             lez/updater with --features harness"
        ),
        other => bail!("daemon.ledger = {other:?}: harness or sequencer"),
    }
}

/// The program's account: base58 (as LEZ prints an account) or 0x-hex (as crown-driver's session-a.json holds it).
fn program_account(s: &str) -> Result<AccountId> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x") {
        let bytes: [u8; 32] = hex::decode(h)?.try_into().map_err(|_| anyhow!("sequencer.program: not 32 bytes"))?;
        return Ok(AccountId::new(bytes));
    }
    s.parse().map_err(|e| anyhow!("sequencer.program {s:?}: {e:?}"))
}

/// The time now, as the updater writes it.
pub(crate) fn utc() -> String {
    ferrata_updater::utc_now()
}

/// The faucet of the rehearsal (`day1 fund`): one of the two keys LEZ's Justfile imports (`wallet-import-test-accounts`),
/// funded by the genesis of LEZ's `lez/sequencer/service/networks/local.json` (`../../live/driver/src/keys.rs`).
pub(crate) fn keys_local_test() -> Result<PrivateKey> {
    let bytes: [u8; 32] = hex::decode("f434f8741720014586ae43356d2aec6257da086222f604ddb75d69733b86fc4c")?
        .try_into()
        .map_err(|_| anyhow!("the test key is not 32 bytes"))?;
    PrivateKey::try_new(bytes).map_err(|e| anyhow!("the test key is not a scalar: {e}"))
}
