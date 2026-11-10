//! `ferrata_attest_module`: Ferrata's attestation module for Logos Core (prototype).
//!
//! Rust-first, as the builder's Rust template: the trait below is the module's contract. The builder
//! derives the `.lidl` from it (`codegen.rust.trait` in metadata.json) and generates the scaffold
//! included below. Each method takes and returns one JSON string; the work is done by the
//! `ferrata-attest-calls` crate (`calls/`), which runs and is tested without the scaffold.

use std::panic::{catch_unwind, AssertUnwindSafe};

/// The module's methods. The trait's name is the module's name in PascalCase.
pub trait FerrataAttestModule: Send + 'static {
    /// The module's version, the ferrata-core it was built from and its BLS backend, as JSON.
    fn version(&mut self) -> String;

    /// Prove one storage slot or one log against a header record as the attestation program
    /// would, and return its verdict and code as JSON. `request` is JSON (modules/README.md).
    fn dry_run_attest(&mut self, request: String) -> String;

    /// Recompute an instance id from a configuration and checkpoint, check the checkpoint's
    /// bootstrap as the program's `Init` does, and compare the checkpoint with the roots given.
    /// `request` and the reply are JSON (modules/README.md).
    fn verify_checkpoint(&mut self, request: String) -> String;

    /// Called once the module is wired to the host.
    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}
}

// The builder injects the generated scaffold here: `install`, `RustModuleContext`, `context()`.
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/provider_gen.rs"));

#[derive(Default)]
struct FerrataAttest;

/// A panic must not cross into the host: report it as JSON instead.
fn guarded(method: &str, f: impl FnOnce() -> String) -> String {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        serde_json::json!({ "verdict": "error", "error": format!("{method} panicked") }).to_string()
    })
}

impl FerrataAttestModule for FerrataAttest {
    fn version(&mut self) -> String {
        guarded("version", ferrata_attest_calls::version)
    }

    fn dry_run_attest(&mut self, request: String) -> String {
        guarded("dry_run_attest", || ferrata_attest_calls::dry_run_attest(&request))
    }

    fn verify_checkpoint(&mut self, request: String) -> String {
        guarded("verify_checkpoint", || ferrata_attest_calls::verify_checkpoint(&request))
    }
}

#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    install::<FerrataAttest>();
}
