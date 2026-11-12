//! `ferrata-updater`: keeps Ferrata light-client instances on LEZ following Ethereum.
//!
//! - [`source`]: beacon API endpoints and recorded data; faults for the tests.
//! - [`chain`]: chain presets, decoding of what beacon nodes serve, the program's checks run natively.
//! - [`updater`]: one instance's updater: candidates, cross-check, the job's next step, cooperation
//!   with other updaters, retries, health and metrics.
//! - [`ledger`]: the chain it writes to (the in-process LEZ harness with feature `harness`).
//! - [`config`]: the one configuration file.
//! - [`daemon`]: the set-up from that file and the rounds (the binary's and the Logos module's).
//! - [`serve`]: `/health` and `/metrics` over HTTP.

pub mod chain;
pub mod config;
pub mod daemon;
pub mod ledger;
pub mod serve;
pub mod source;
pub mod updater;
