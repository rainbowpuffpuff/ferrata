//! `ferrata_attest_module`: Ferrata's attestation module for Logos Core (prototype).
//!
//! Rust-first, as the builder's Rust template: the trait below is the module's contract. The builder
//! derives the `.lidl` from it (`codegen.rust.trait` in metadata.json) and generates the scaffold
//! included below. Each method takes and returns one JSON string; the work is done by the
//! `ferrata-attest-calls` crate (`calls/`), which runs and is tested without the scaffold.

use std::panic::{catch_unwind, AssertUnwindSafe};

/// The module's methods. The trait's name is the module's name in PascalCase.
pub trait FerrataAttestModule: Send + 'static {
    /// The module's version, what was staged beside it (the commit, LEZ's, each crate's sources'
    /// SHA-256) and its BLS backend, as JSON.
    fn version(&mut self) -> String;

    /// Prove one storage slot or one log against a header record as the attestation program
    /// would, and return its verdict and code as JSON. `request` is JSON (modules/README.md).
    fn dry_run_attest(&mut self, request: String) -> String;

    /// Recompute an instance id from a configuration and checkpoint, check the checkpoint's
    /// bootstrap as the program's `Init` does, and compare the checkpoint with the roots given.
    /// `request` and the reply are JSON (modules/README.md).
    fn verify_checkpoint(&mut self, request: String) -> String;

    /// Read a block and its receipts through Logos Core's `eth_rpc_module` (`eth`) or from a plain
    /// JSON-RPC endpoint (`rpc_url`), rebuild the receipts trie, check it against the block's receipts
    /// root, and return one log's receipt proof.
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

    /// A light-client update planned as `ferrata-updater` plans it (its own code, `ferrata-advance`):
    /// the program's checks run natively, the job it opens, and the transactions of the job's steps
    /// (`Open`, `Miller`, `Adopt`, `Finish`) with their accounts. JSON in and out (modules/README.md).
    fn advance(&mut self, request: String) -> String;

    /// Open (or close) the path through Logos Core's own LEZ module, `lez_core`: a LEZ wallet opened inside it.
    fn lez(&mut self, request: String) -> String;
    /// An instance, a record or a job of the attestation program read through `lez_core.get_account_public`.
    fn lez_read(&mut self, request: String) -> String;
    /// Transactions (the steps of `advance`, or `build_attest_tx`'s) sent through `lez_core.send_generic_public_transaction`.
    fn lez_submit(&mut self, request: String) -> String;

    /// Called once the module is wired to the host.
    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}
}

// The builder injects the generated scaffold here: `install`, `RustModuleContext`, `context()`.
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/provider_gen.rs"));

/// Logos Core's `eth_rpc_module.raw_rpc(chain_id, method, params_json)`, called by name through the SDK
/// (`PluginProxy::call_json_with_timeout`, what a generated client does inside). The module is not
/// declared a dependency: with logos-module-builder 5f74cdb a declared optional dependency's contract
/// fails to install (modules/LOGOS-CORE-MODULES.md), and the module must keep working without it.
/// The calls crate uses it when a request says `"eth": {"via": "eth_rpc_module", "chain_id": N}`.
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

/// Logos Core's own LEZ module, `lez_core`, called by name through the SDK as `eth_rpc_module` above: (method, the
/// arguments as a JSON array, a timeout) -> the method's reply as text. Not declared a dependency, for the same reason;
/// the calls crate uses it for `lez`, `lez_read` and `lez_submit` (calls/src/lez.rs).
fn lez_core_call(method: &str, args: &str, timeout_ms: u64) -> Result<String, String> {
    let args: serde_json::Value = serde_json::from_str(args).map_err(|e| format!("lez_core.{method}: arguments: {e}"))?;
    let v = logos_rust_sdk::LogosModuleSDK::new()
        .plugin("lez_core")
        .call_json_with_timeout(method, &args, std::time::Duration::from_millis(timeout_ms.max(1)))
        .map_err(|e| format!("lez_core.{method} did not answer: {e:?}"))?;
    Ok(match v {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    })
}

/// Logos Core's `beacon_api_module` (logos-evm-beacon-api-module), called by name: `verify_checkpoint`'s beacon checks
/// when the request says `"providers": {"via": "beacon_api_module", "chain_id": N}` (`list_providers`, then each
/// provider pinned through `raw_get`). Not declared a dependency, for the same reason as above.
fn beacon_api_module_call(method: &str, args: &str, timeout_ms: u64) -> Result<String, String> {
    let args: serde_json::Value = serde_json::from_str(args).map_err(|e| format!("beacon_api_module.{method}: arguments: {e}"))?;
    let v = logos_rust_sdk::LogosModuleSDK::new()
        .plugin("beacon_api_module")
        .call_json_with_timeout(method, &args, std::time::Duration::from_millis(timeout_ms.max(1)))
        .map_err(|e| format!("beacon_api_module.{method} did not answer: {e:?}"))?;
    Ok(match v {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    })
}

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

    fn advance(&mut self, request: String) -> String {
        guarded("advance", || ferrata_attest_calls::advance(&request))
    }

    fn lez(&mut self, request: String) -> String {
        guarded("lez", || ferrata_attest_calls::lez(&request))
    }

    fn lez_read(&mut self, request: String) -> String {
        guarded("lez_read", || ferrata_attest_calls::lez_read(&request))
    }

    fn lez_submit(&mut self, request: String) -> String {
        guarded("lez_submit", || ferrata_attest_calls::lez_submit(&request))
    }
}

#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    ferrata_attest_calls::set_eth_rpc_module(eth_rpc_module_raw_rpc);
    ferrata_attest_calls::set_lez_core(lez_core_call);
    ferrata_attest_calls::set_beacon_api_module(beacon_api_module_call);
    install::<FerrataAttest>();
}
