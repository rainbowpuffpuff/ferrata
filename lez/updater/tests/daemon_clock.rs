//! The clock the daemon checks the program's validity windows against (UP-H1): a backend other than the harness gets
//! this machine's clock (the sequencer backend's sequencer is on loopback and stamps its blocks with it); the harness,
//! and a `Ledger` that wraps it without saying otherwise, keep each transaction's own time, so a test whose recorded
//! data's windows end in wall-clock time keeps passing after that day.
#![cfg(feature = "harness")]

use std::{fs, path::Path, path::PathBuf};

use anyhow::Result;
use ferrata_updater::{
    config::File,
    daemon::Daemon,
    ledger::{machine_now_ms, HarnessLedger, Ledger, Tx, TxOutcome},
    source::lab_record,
};
use lee::{AccountId, Actor, PrivateKey};

/// Stands in for the sequencer backend: the in-process chain, opened for any `daemon.ledger`.
fn open_in_process(_: &File, _: &Path, payer: &PrivateKey) -> Result<(Box<dyn Ledger>, AccountId)> {
    Ok((Box::new(HarnessLedger::new(std::slice::from_ref(payer))), HarnessLedger::program()))
}

/// A configuration in a directory of its own, with absolute paths to the dev key and the lab's recorded mainnet data.
fn config(ledger: &str) -> PathBuf {
    let base = std::env::var_os("CARGO_TARGET_TMPDIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!("daemon-clock-{ledger}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let record = lab_record("mainnet");
    let checkpoint = fs::read_to_string(record.join("checkpoint")).unwrap();
    let key = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dev/payer.key");
    let sequencer = if ledger == "sequencer" {
        "[sequencer]\nrpc = \"http://127.0.0.1:3040\"\nprogram = \"A5E62aMHKFUPwtuMZRUTSPsGwkpzeGBWmbVzD8F5tKcX\"\n"
    } else {
        ""
    };
    let text = format!(
        "[daemon]\nname = \"clock\"\nstate_dir = \"state\"\nledger = \"{ledger}\"\n{sequencer}[keys]\npayer_key_file = \
         {key:?}\n[[chain]]\nname = \"mainnet\"\npreset = \"mainnet\"\ncheckpoint = \"{}\"\n  [[chain.source]]\n  name = \
         \"record\"\n  dir = {record:?}\n",
        checkpoint.trim()
    );
    let path = dir.join("updater.toml");
    fs::write(&path, text).unwrap();
    path
}

/// A wrapper of the in-process chain that says nothing about the clock (as a test's censoring or delaying stand-in).
struct Wrapper(HarnessLedger);

impl Ledger for Wrapper {
    fn actor_state(&self, actor: Actor) -> Result<Vec<u8>> {
        self.0.actor_state(actor)
    }
    fn submit(&mut self, payer: &PrivateKey, tx: &Tx, gas_limit: u64, max_fee: u128) -> Result<TxOutcome> {
        self.0.submit(payer, tx, gas_limit, max_fee)
    }
}

#[test]
fn a_sequencer_backend_gets_the_machine_clock_and_the_harness_its_own_time() {
    let sequencer = Daemon::open_with(&config("sequencer"), false, open_in_process).unwrap();
    let harness = Daemon::open_with(&config("harness"), false, open_in_process).unwrap();
    let (seen, now) = (sequencer.ledger.now_ms(), machine_now_ms().unwrap());
    println!(
        "daemon.ledger = \"sequencer\": now_ms {seen:?} (this machine: {now}); \"harness\": {:?}",
        harness.ledger.now_ms()
    );
    assert!(seen.is_some_and(|t| t.abs_diff(now) < 5_000), "{seen:?} {now}");
    assert_eq!(harness.ledger.now_ms(), None);
    assert_eq!(Wrapper(HarnessLedger::new(&[])).now_ms(), None, "a wrapper keeps each transaction's own time");
}
