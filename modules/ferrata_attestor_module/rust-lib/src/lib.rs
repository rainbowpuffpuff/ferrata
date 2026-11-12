//! `ferrata_attestor_module`: `ferrata_attestor` (checkpoint/attestor) as a Logos Core module (prototype).
//!
//! Rust-first, as the builder's Rust template: the trait below is the module's contract. The builder
//! derives the `.lidl` from it (`codegen.rust.trait` in metadata.json) and generates the scaffold
//! included below. Each method takes and returns one JSON string; the work is done by the
//! `ferrata-attestor-calls` crate (`calls/`), which runs and is tested without the scaffold.
//! `start` runs the attestor's loop in a thread of the module (the daemon a sequencer runs beside its
//! node); `status` reads it; `stop`, or the host unloading the module, ends it.

use std::panic::{catch_unwind, AssertUnwindSafe};

use ferrata_attestor_calls::Daemon;

/// The module's methods. The trait's name is the module's name in PascalCase.
pub trait FerrataAttestorModule: Send + 'static {
    /// The module's version and the attestor it was built from, as JSON.
    fn version(&mut self) -> String;

    /// A 32-byte attestation key written to a new file (`out`; `seed_label` only for devnet keys); its public key.
    fn keygen(&mut self, request: String) -> String;

    /// The public key of a 32-byte secret key file. JSON in and out (modules/README.md).
    fn pubkey(&mut self, request: String) -> String;

    /// The deployment (chain, vault, configuration, channel) a checkpoint_b input serves. JSON in and out.
    fn deployment(&mut self, request: String) -> String;

    /// The channel key's one certificate of an attestation key for a deployment. JSON in and out.
    fn certify(&mut self, request: String) -> String;

    /// The node's final tier as the attestor checks it: tip, key set, blocks per signer. JSON in and out.
    fn tier(&mut self, request: String) -> String;

    /// Read the node's final tier, check it and sign its tip (final-tier message v2). JSON in and out.
    fn attest(&mut self, request: String) -> String;

    /// Check published attestations: certificate and signature over the live key set. JSON in and out.
    fn verify(&mut self, request: String) -> String;

    /// The collector: t of n valid attestations into a checkpoint_b input, the rule run natively. JSON in and out.
    fn collect(&mut self, request: String) -> String;

    /// An input with exactly the given attestations, unchecked (one the program must refuse). JSON in and out.
    fn assemble(&mut self, request: String) -> String;

    /// Devnet set-up: a checkpoint_b input over final blocks from the node's answers. JSON in and out.
    fn base(&mut self, request: String) -> String;

    /// Start the attestor's loop: ask the node, check, sign, publish, every poll. JSON in and out.
    fn start(&mut self, request: String) -> String;

    /// The loop's rounds, what each did, and the last attestation, as JSON.
    fn status(&mut self) -> String;

    /// Stop the loop after the round in hand; its final status, as JSON.
    fn stop(&mut self) -> String;

    /// Called once the module is wired to the host.
    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}
}

// The builder injects the generated scaffold here: `install`, `logos_install!`, `RustModuleContext`.
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/provider_gen.rs"));

#[derive(Default)]
struct FerrataAttestor {
    daemon: std::sync::Mutex<Option<Daemon>>,
}

/// A panic must not cross into the host: report it as JSON instead.
fn guarded(method: &str, f: impl FnOnce() -> String) -> String {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        serde_json::json!({ "verdict": "error", "error": format!("{method} panicked") }).to_string()
    })
}

impl FerrataAttestor {
    fn with_daemon(&self, f: impl FnOnce(&mut Option<Daemon>) -> String) -> String {
        let mut slot = self.daemon.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut slot)
    }
}

impl FerrataAttestorModule for FerrataAttestor {
    fn version(&mut self) -> String {
        guarded("version", ferrata_attestor_calls::version)
    }

    fn keygen(&mut self, request: String) -> String {
        guarded("keygen", || ferrata_attestor_calls::keygen(&request))
    }

    fn pubkey(&mut self, request: String) -> String {
        guarded("pubkey", || ferrata_attestor_calls::pubkey(&request))
    }

    fn deployment(&mut self, request: String) -> String {
        guarded("deployment", || ferrata_attestor_calls::deployment(&request))
    }

    fn certify(&mut self, request: String) -> String {
        guarded("certify", || ferrata_attestor_calls::certify(&request))
    }

    fn tier(&mut self, request: String) -> String {
        guarded("tier", || ferrata_attestor_calls::tier(&request))
    }

    fn attest(&mut self, request: String) -> String {
        guarded("attest", || ferrata_attestor_calls::attest(&request))
    }

    fn verify(&mut self, request: String) -> String {
        guarded("verify", || ferrata_attestor_calls::verify(&request))
    }

    fn collect(&mut self, request: String) -> String {
        guarded("collect", || ferrata_attestor_calls::collect(&request))
    }

    fn assemble(&mut self, request: String) -> String {
        guarded("assemble", || ferrata_attestor_calls::assemble(&request))
    }

    fn base(&mut self, request: String) -> String {
        guarded("base", || ferrata_attestor_calls::base(&request))
    }

    fn start(&mut self, request: String) -> String {
        guarded("start", || {
            self.with_daemon(|slot| {
                if slot.as_ref().is_some_and(Daemon::running) {
                    return serde_json::json!({ "verdict": "refused", "error": "the attestor's loop is running: stop it first" })
                        .to_string();
                }
                match Daemon::start(&request) {
                    Ok(d) => {
                        let status = d.status();
                        *slot = Some(d);
                        serde_json::json!({ "verdict": "started",
                            "status": serde_json::from_str::<serde_json::Value>(&status).unwrap_or_default() })
                        .to_string()
                    }
                    Err(reply) => reply,
                }
            })
        })
    }

    fn status(&mut self) -> String {
        guarded("status", || {
            self.with_daemon(|slot| match slot {
                Some(d) => d.status(),
                None => serde_json::json!({ "verdict": "ok", "running": false, "rounds": 0 }).to_string(),
            })
        })
    }

    fn stop(&mut self) -> String {
        guarded("stop", || {
            self.with_daemon(|slot| match slot {
                Some(d) => d.stop(),
                None => serde_json::json!({ "verdict": "ok", "running": false, "rounds": 0 }).to_string(),
            })
        })
    }
}

/// Asked before the host unloads the module: the loop stops first.
impl logos_rust_sdk::AboutToUnload for FerrataAttestor {
    fn about_to_unload(&self) -> logos_rust_sdk::Shutdown {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            self.with_daemon(|slot| slot.as_mut().map(Daemon::stop).unwrap_or_default())
        }));
        logos_rust_sdk::Shutdown::Synchronous
    }
}

#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    logos_install!(FerrataAttestor);
}
