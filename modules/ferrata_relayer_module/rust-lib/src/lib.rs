//! `ferrata_relayer_module`: the relayer daemon `ferrata-relayer` (cli/) as a Logos Core module (prototype).
//!
//! Rust-first, as the builder's Rust template: the trait below is the module's contract. The builder
//! derives the `.lidl` from it (`codegen.rust.trait` in metadata.json) and generates the scaffold
//! included below. Each method takes and returns one JSON string; the work is done by the
//! `ferrata-relayer-calls` crate (`calls/`), which runs and is tested without the scaffold.
//! `start` runs `ferrata-relayer run` as a daemon in a thread of the module; `status` reads it; `stop`, or
//! the host unloading the module, ends it after the round in progress, as SIGTERM ends the binary.

use std::panic::{catch_unwind, AssertUnwindSafe};

use ferrata_relayer_calls::Daemon;

/// The module's methods. The trait's name is the module's name in PascalCase.
pub trait FerrataRelayerModule: Send + 'static {
    /// The module's version and the relayer it was built from, as JSON.
    fn version(&mut self) -> String;

    /// `ferrata-relayer` with the given arguments until its rounds are done (or `status`), with its lines. JSON in and out.
    fn run(&mut self, request: String) -> String;

    /// `ferrata-relayer ... run ...` as a daemon in the module, until `stop`. JSON in and out.
    fn start(&mut self, request: String) -> String;

    /// Whether the daemon runs, how it ended, its lines from `since` on. JSON in and out.
    fn status(&mut self, request: String) -> String;

    /// Stop the daemon after the round in progress (its `stop` line names `reason`); its final status. JSON in and out.
    fn stop(&mut self, request: String) -> String;

    /// Called once the module is wired to the host.
    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}
}

// The builder injects the generated scaffold here: `install`, `logos_install!`, `RustModuleContext`.
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/provider_gen.rs"));

#[derive(Default)]
struct FerrataRelayer {
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

impl FerrataRelayer {
    fn with_daemon(&self, f: impl FnOnce(&mut Option<Daemon>) -> String) -> String {
        let mut slot = self.daemon.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut slot)
    }
}

impl FerrataRelayerModule for FerrataRelayer {
    fn version(&mut self) -> String {
        guarded("version", ferrata_relayer_calls::version)
    }

    fn run(&mut self, request: String) -> String {
        guarded("run", || ferrata_relayer_calls::run(&request))
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
                        let mut status: serde_json::Value = serde_json::from_str(&d.status("")).unwrap_or_default();
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

    fn status(&mut self, request: String) -> String {
        guarded("status", || {
            self.with_daemon(|slot| slot.as_ref().map(|d| d.status(&request)).unwrap_or_else(not_started))
        })
    }

    fn stop(&mut self, request: String) -> String {
        guarded("stop", || {
            self.with_daemon(|slot| slot.as_mut().map(|d| d.stop(&request)).unwrap_or_else(not_started))
        })
    }
}

/// Asked before the host unloads the module: the daemon stops first, after the round in progress.
impl logos_rust_sdk::AboutToUnload for FerrataRelayer {
    fn about_to_unload(&self) -> logos_rust_sdk::Shutdown {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            self.with_daemon(|slot| slot.as_mut().map(|d| d.stop(r#"{"reason":"unload"}"#)).unwrap_or_default())
        }));
        logos_rust_sdk::Shutdown::Synchronous
    }
}

#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    logos_install!(FerrataRelayer);
}
