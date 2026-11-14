//! `ferrata_monitor_module`: the watchers' `monitor` (monitor/) as a Logos Core module (prototype).
//!
//! Rust-first, as the builder's Rust template: the trait below is the module's contract. The builder
//! derives the `.lidl` from it (`codegen.rust.trait` in metadata.json) and generates the scaffold
//! included below. Each method takes and returns one JSON string; the work is done by the
//! `ferrata-monitor-calls` crate (`calls/`), which runs and is tested without the scaffold.
//! `start` runs `monitor run` as a daemon in a thread of the module; `status` reads it; `stop`, or the
//! host unloading the module, ends it the way SIGTERM ends the binary.

use std::panic::{catch_unwind, AssertUnwindSafe};

use ferrata_monitor_calls::Daemon;

/// The module's methods. The trait's name is the module's name in PascalCase.
pub trait FerrataMonitorModule: Send + 'static {
    /// The module's version and the monitor it was built from, as JSON.
    fn version(&mut self) -> String;

    /// One offline verdict on a candidate checkpoint against node views (`monitor check`). JSON in and out.
    fn check(&mut self, request: String) -> String;

    /// A node view from a LEZ sequencer's block record (`monitor node-view`). JSON in and out.
    fn node_view(&mut self, request: String) -> String;

    /// `monitor run` with the given arguments for its `--max-polls` polls, then its state. JSON in and out.
    fn run(&mut self, request: String) -> String;

    /// `monitor run` with the given arguments as a daemon in the module, until `stop`. JSON in and out.
    fn start(&mut self, request: String) -> String;

    /// Whether the daemon runs, how it ended, its state file, as JSON.
    fn status(&mut self) -> String;

    /// Stop the daemon after the poll in hand (a `stopped` alert, the state saved); its status, as JSON.
    fn stop(&mut self) -> String;

    /// The watcher key into Logos Core's `keystore_module` and unlocked for `tx_sender_module` (this module named a
    /// custodian of `keystore_module` and `signer_manager_module`), for `--send-via logos`. JSON in and out.
    fn custody(&mut self, request: String) -> String;

    /// Called once the module is wired to the host.
    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}
}

// The builder injects the generated scaffold here: `install`, `logos_install!`, `RustModuleContext`.
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/provider_gen.rs"));

/// Another Logos Core module's method, called by name through the SDK (`PluginProxy::call_json_with_timeout`, what a
/// generated client does inside), as `ferrata_bridge_module` calls `eth_rpc_module`: the monitor's sends go to
/// `tx_sender_module` and `custody` to `keystore_module` and `signer_manager_module`, none of them a declared
/// dependency (the monitor runs without them on the plain path).
fn module_call(
    module: &str,
    method: &str,
    args: &serde_json::Value,
    timeout: std::time::Duration,
) -> Result<String, String> {
    let v = logos_rust_sdk::LogosModuleSDK::new()
        .plugin(module)
        .call_json_with_timeout(method, args, timeout)
        .map_err(|e| format!("{module}.{method} did not answer: {e:?}"))?;
    Ok(match v {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    })
}

#[derive(Default)]
struct FerrataMonitor {
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

impl FerrataMonitor {
    fn with_daemon(&self, f: impl FnOnce(&mut Option<Daemon>) -> String) -> String {
        let mut slot = self.daemon.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut slot)
    }
}

impl FerrataMonitorModule for FerrataMonitor {
    fn version(&mut self) -> String {
        guarded("version", ferrata_monitor_calls::version)
    }

    fn check(&mut self, request: String) -> String {
        guarded("check", || ferrata_monitor_calls::check(&request))
    }

    fn node_view(&mut self, request: String) -> String {
        guarded("node_view", || ferrata_monitor_calls::node_view(&request))
    }

    fn run(&mut self, request: String) -> String {
        guarded("run", || ferrata_monitor_calls::run(&request))
    }

    fn start(&mut self, request: String) -> String {
        guarded("start", || {
            self.with_daemon(|slot| {
                if slot.as_ref().is_some_and(Daemon::running) {
                    return serde_json::json!({ "verdict": "refused",
                        "error": "a daemon is running in this module: stop it first" })
                    .to_string();
                }
                match Daemon::start(&request) {
                    Ok(d) => {
                        let running = d.running();
                        let mut status: serde_json::Value = serde_json::from_str(&d.status()).unwrap_or_default();
                        if running {
                            status["verdict"] = serde_json::json!("started");
                        }
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

    fn stop(&mut self) -> String {
        guarded("stop", || self.with_daemon(|slot| slot.as_mut().map(Daemon::stop).unwrap_or_else(not_started)))
    }

    fn custody(&mut self, request: String) -> String {
        guarded("custody", || ferrata_monitor_calls::custody(&request))
    }
}

/// Asked before the host unloads the module: the daemon stops first (state saved, lock released).
impl logos_rust_sdk::AboutToUnload for FerrataMonitor {
    fn about_to_unload(&self) -> logos_rust_sdk::Shutdown {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            self.with_daemon(|slot| slot.as_mut().map(Daemon::stop).unwrap_or_default())
        }));
        logos_rust_sdk::Shutdown::Synchronous
    }
}

#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    ferrata_monitor_calls::set_module_call(module_call);
    logos_install!(FerrataMonitor);
}
