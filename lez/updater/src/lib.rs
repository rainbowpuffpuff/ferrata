//! `ferrata-updater`: keeps Ferrata light-client instances on LEZ following Ethereum.
//!
//! - [`source`]: beacon API endpoints, Logos Core's `beacon_api_module` and recorded data; faults for the tests.
//! - [`lc_json`]: the beacon API's JSON form of light-client data turned into SSZ (what `beacon_api_module` serves).
//! - [`chain`]: chain presets, decoding of what beacon nodes serve, the program's checks run natively.
//! - [`updater`]: one instance's updater: candidates, cross-check, the job's next step, cooperation
//!   with other updaters, retries, health and metrics.
//! - [`ledger`]: the chain it writes to (the in-process LEZ harness with feature `harness`).
//! - [`config`]: the one configuration file.
//! - [`daemon`]: the daemon: the set-up from that file and the rounds (the binaries' and the Logos module's), over a
//!   chain backend (this crate's harness, or `sequencer/`'s running LEZ sequencer), and the binaries' main loop.
//! - [`serve`]: `/health` and `/metrics` over HTTP.

pub mod chain;
pub mod config;
pub mod daemon;
pub mod lc_json;
pub mod ledger;
pub mod serve;
pub mod source;
pub mod updater;

/// Now, as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn utc_now() -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    utc(t)
}

/// Unix seconds as `YYYY-MM-DDTHH:MM:SSZ` (the proleptic Gregorian calendar, days from the epoch).
pub fn utc(t: u64) -> String {
    let (days, rem) = ((t / 86_400) as i64, t % 86_400);
    let z = days + 719_468;
    let (era, doe) = (z.div_euclid(146_097), z.rem_euclid(146_097));
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}
