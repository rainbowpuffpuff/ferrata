//! `ferrata_updater_module`: `ferrata-updater` (lez/updater) as a Logos Core module (prototype).
//!
//! Rust-first, as the builder's Rust template: the trait below is the module's contract. The builder
//! derives the `.lidl` from it (`codegen.rust.trait` in metadata.json) and generates the scaffold
//! included below. Each method takes and returns one JSON string; the work is done by the
//! `ferrata-updater-calls` crate (`calls/`), which runs and is tested without the scaffold.
//! `start` runs the daemon's rounds in a thread of the module; `status` and `metrics` read it (as
//! `/health` and `/metrics` do); `stop`, or the host unloading the module, ends it.

use std::panic::{catch_unwind, AssertUnwindSafe};

use ferrata_updater_calls::Daemon;

/// The module's methods. The trait's name is the module's name in PascalCase.
pub trait FerrataUpdaterModule: Send + 'static {
    /// The module's version, the updater it was built from and the program it carries, as JSON.
    fn version(&mut self) -> String;

    /// A configuration file as the daemon reads it. JSON in and out (modules/README.md).
    fn check_config(&mut self, request: String) -> String;

    /// The daemon for a number of rounds (or until up to date), then its health and instances. JSON in and out.
    fn run(&mut self, request: String) -> String;

    /// Start the daemon's rounds in the module, until `stop`. JSON in and out.
    fn start(&mut self, request: String) -> String;

    /// The daemon's rounds, each instance's health and its state on the chain, as JSON (`/health`).
    fn status(&mut self) -> String;

    /// The daemon's Prometheus metrics as the last round left them, as JSON (`/metrics`).
    fn metrics(&mut self) -> String;

    /// Stop the daemon after the round in hand; its final status, as JSON.
    fn stop(&mut self) -> String;

    /// Called once the module is wired to the host.
    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}
}

// The builder injects the generated scaffold here: `install`, `logos_install!`, `RustModuleContext`.
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/provider_gen.rs"));

/// Logos Core's `beacon_api_module.lc_request(chain_id, endpoint, params_json, provider, deadline_ms)`, called by name
/// through the SDK (`PluginProxy::call_json_with_timeout`, what a generated client does inside), as
/// `ferrata_attest_module` calls `eth_rpc_module`. Not a declared dependency: the module keeps loading without it. The
/// updater uses it for a source that says `via = "beacon_api_module"` (lez/updater/src/source.rs, `BeaconApiModule`).
fn beacon_api_module_lc_request(
    chain_id: i64,
    endpoint: &str,
    params: &str,
    provider: &str,
    deadline_ms: u64,
) -> Result<String, String> {
    let v = logos_rust_sdk::LogosModuleSDK::new()
        .plugin("beacon_api_module")
        .call_json_with_timeout(
            "lc_request",
            &serde_json::json!([chain_id, endpoint, params, provider, deadline_ms as i64]),
            // The module's deadline bounds its failover over providers; the call itself gets five seconds more.
            std::time::Duration::from_millis(deadline_ms.max(1) + 5_000),
        )
        .map_err(|e| format!("beacon_api_module.lc_request did not answer: {e:?}"))?;
    Ok(match v {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    })
}

/// Logos Core's `eth_rpc_module.raw_rpc(chain_id, method, params_json)`, called by name through the SDK as
/// `ferrata_attest_module` calls it. The updater uses it for an execution source (`kind = "execution"`,
/// `via = "eth_rpc_module"`: lez/updater/src/source.rs, `EthRpcModule`), which confirms each update's finalized
/// header by its execution block before the update is sent.
fn eth_rpc_module_raw_rpc(chain_id: i64, method: &str, params: &str, timeout_ms: u64) -> Result<String, String> {
    let v = logos_rust_sdk::LogosModuleSDK::new()
        .plugin("eth_rpc_module")
        .call_json_with_timeout(
            "raw_rpc",
            &serde_json::json!([chain_id, method, params]),
            std::time::Duration::from_millis(timeout_ms.max(1)),
        )
        .map_err(|e| format!("eth_rpc_module.raw_rpc did not answer: {e:?}"))?;
    Ok(match v {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    })
}

#[derive(Default)]
struct FerrataUpdater {
    daemon: std::sync::Mutex<Option<Daemon>>,
}

/// A panic must not cross into the host: report it as JSON instead.
fn guarded(method: &str, f: impl FnOnce() -> String) -> String {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        serde_json::json!({ "verdict": "error", "error": format!("{method} panicked") }).to_string()
    })
}

fn not_started() -> String {
    serde_json::json!({ "verdict": "ok", "running": false, "note": "no daemon was started" }).to_string()
}

impl FerrataUpdater {
    fn with_daemon(&self, f: impl FnOnce(&mut Option<Daemon>) -> String) -> String {
        let mut slot = self.daemon.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut slot)
    }
}

impl FerrataUpdaterModule for FerrataUpdater {
    fn version(&mut self) -> String {
        guarded("version", ferrata_updater_calls::version)
    }

    fn check_config(&mut self, request: String) -> String {
        guarded("check_config", || ferrata_updater_calls::check_config(&request))
    }

    fn run(&mut self, request: String) -> String {
        guarded("run", || ferrata_updater_calls::run(&request))
    }

    fn start(&mut self, request: String) -> String {
        guarded("start", || {
            self.with_daemon(|slot| {
                if slot.as_ref().is_some_and(Daemon::running) {
                    return serde_json::json!({ "verdict": "refused",
                        "error": "the updater is running in this module: stop it first" })
                    .to_string();
                }
                match Daemon::start(&request) {
                    Ok(d) => {
                        let mut status: serde_json::Value = serde_json::from_str(&d.status()).unwrap_or_default();
                        status["verdict"] = serde_json::json!("started");
                        *slot = Some(d);
                        status.to_string()
                    }
                    Err(reply) => reply,
                }
            })
        })
    }

    fn status(&mut self) -> String {
        guarded("status", || self.with_daemon(|slot| slot.as_ref().map(Daemon::status).unwrap_or_else(not_started)))
    }

    fn metrics(&mut self) -> String {
        guarded("metrics", || self.with_daemon(|slot| slot.as_ref().map(Daemon::metrics).unwrap_or_else(not_started)))
    }

    fn stop(&mut self) -> String {
        guarded("stop", || self.with_daemon(|slot| slot.as_mut().map(Daemon::stop).unwrap_or_else(not_started)))
    }
}

/// Asked before the host unloads the module: the daemon's rounds stop first.
impl logos_rust_sdk::AboutToUnload for FerrataUpdater {
    fn about_to_unload(&self) -> logos_rust_sdk::Shutdown {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            self.with_daemon(|slot| slot.as_mut().map(Daemon::stop).unwrap_or_default())
        }));
        logos_rust_sdk::Shutdown::Synchronous
    }
}

#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    ferrata_updater_calls::set_beacon_api_module(beacon_api_module_lc_request);
    ferrata_updater_calls::set_eth_rpc_module(eth_rpc_module_raw_rpc);
    logos_install!(FerrataUpdater);
}
