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

    /// Read a block and its receipts from a JSON-RPC endpoint (`rpc_url`), rebuild the receipts
    /// trie, check it against the block's receipts root, and return one log's receipt proof.
    /// `request` and the reply are JSON (modules/README.md).
    fn fetch_log_proof(&mut self, request: String) -> String;

    /// Build the attestation program's `Attest` message for a log or a storage slot at
    /// `header(id, slot)`, and name the account it is sent to. JSON in and out (modules/README.md).
    fn build_attest_tx(&mut self, request: String) -> String;

    /// Decode the program's `Attested` event and compare its statement ids with a dry run's.
    /// JSON in and out (modules/README.md).
    fn read_attestation(&mut self, request: String) -> String;

    /// Decode an instance's state as LEZ holds it at `instance(id)`: id, configuration, activation,
    /// store (finalized header, committees) and jobs applied. JSON in and out (modules/README.md).
    fn read_instance(&mut self, request: String) -> String;

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

    fn fetch_log_proof(&mut self, request: String) -> String {
        guarded("fetch_log_proof", || ferrata_attest_calls::fetch_log_proof(&request))
    }

    fn build_attest_tx(&mut self, request: String) -> String {
        guarded("build_attest_tx", || ferrata_attest_calls::build_attest_tx(&request))
    }

    fn read_attestation(&mut self, request: String) -> String {
        guarded("read_attestation", || ferrata_attest_calls::read_attestation(&request))
    }

    fn read_instance(&mut self, request: String) -> String {
        guarded("read_instance", || ferrata_attest_calls::read_instance(&request))
    }
}

#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    install::<FerrataAttest>();
}
