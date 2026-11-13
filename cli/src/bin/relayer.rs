//! `ferrata-relayer`: the relayer daemon for the Ferrata bridge (prototype, unaudited).
//!
//! The daemon is [`ferrata_cli::relayerd`] (its rounds, arguments and JSON lines are documented there);
//! `ferrata_relayer_module` runs the same code inside Logos Core. This binary prints each JSON line on
//! standard output and ends the daemon after the round in progress on SIGTERM or SIGINT.

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use ferrata_cli::relayerd::{self, Cli};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    // Clean shutdown: SIGTERM or SIGINT ends the daemon after the round in progress (installed before the
    // first round, so a signal during a round waits for it to finish).
    use tokio::signal::unix::{signal, SignalKind};
    let (mut term, mut int) = (signal(SignalKind::terminate())?, signal(SignalKind::interrupt())?);
    let (tx, rx) = tokio::sync::watch::channel(None);
    tokio::spawn(async move {
        let reason = tokio::select! {
            _ = term.recv() => "SIGTERM",
            _ = int.recv() => "SIGINT",
        };
        let _ = tx.send(Some(reason.to_string()));
    });
    relayerd::run(cli, Arc::new(|line| println!("{line}")), rx).await
}
