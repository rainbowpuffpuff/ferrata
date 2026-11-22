//! `ferrata_bridge_module`: Ferrata's bridge module for Logos Core (prototype).
//!
//! Rust-first, as the builder's Rust template: the trait below is the module's contract. The builder
//! derives the `.lidl` from it (`codegen.rust.trait` in metadata.json) and generates the scaffold
//! included below. Each method takes and returns one JSON string; the work is done by the
//! `ferrata-bridge-calls` crate (`calls/`), which runs and is tested without the scaffold.

use std::panic::{catch_unwind, AssertUnwindSafe};

/// The module's methods. The trait's name is the module's name in PascalCase.
pub trait FerrataBridgeModule: Send + 'static {
    /// The module's version and what was staged beside it (the commit, LEZ's, each crate's sources'
    /// SHA-256), as JSON.
    fn version(&mut self) -> String;

    /// A deposit's commitment and the vault call that makes it (`deposit` or `depositETH`), and,
    /// with its leaf index, the leaf, deposit id and nullifier. JSON in and out (modules/README.md).
    fn deposit(&mut self, request: String) -> String;

    /// A claim of a deposit: its Merkle path in the deposit tree, the check the bridge program runs
    /// (`verify_claim`), its nullifier, and the program's `Claim` or `ClaimPrivately` message; given
    /// LEZ's state, the `ferrata` tool's claim (the program's checks, fees, accounts). JSON in and out.
    fn claim(&mut self, request: String) -> String;

    /// A burn: the note (tag, memo, MAC, commitment) and the bridge program's `Burn` message, and,
    /// with its index in the burn tree, the leaf, burn id and release nullifier. JSON in and out.
    fn burn(&mut self, request: String) -> String;

    /// A release: the burn's path in the checkpointed burn tree (its root given, or picked by the
    /// wallet's rule: the newest accepted root whose release delay has passed), the release guest's
    /// check (`verify_release`), its nullifier and the journal the vault verifies. JSON in and out.
    fn release(&mut self, request: String) -> String;

    /// The wallet's own deposits and burns among public ones, from its seed alone. JSON in and out.
    fn recover(&mut self, request: String) -> String;

    /// Where each deposit and burn stands (awaiting root, claimable, claimed; awaiting checkpoint,
    /// releasable, released), from the chains' public state. JSON in and out.
    fn status(&mut self, request: String) -> String;

    /// A release (as `release`), then the release guest run on its input through r0vm (`program`: the
    /// guest's ELF), its journal checked against the native one; the tool's `release --prove`. JSON.
    fn prove_release(&mut self, request: String) -> String;

    /// The relayer's `ImportRoot` of a deposit root from the vault's `eth_getProof` proofs (given, or
    /// read at `block` through Logos Core's `eth_rpc_module` or a plain endpoint), dry-run with the
    /// attestation and bridge programs' own functions when the record is given. JSON.
    fn import_root(&mut self, request: String) -> String;

    /// The bridge program's registry as LEZ holds it (`token(cid, token)`, `config(cid)`,
    /// `root(cid, n)`), decoded, with a claim's schedule range and fees. JSON in and out.
    fn registry(&mut self, request: String) -> String;

    /// The mini-app's calls (`{"method", "params"}`: open, info, registry, price, deposit, scan, claim,
    /// burn, release), each run with the methods above over the chains' state, which a chain service
    /// reads and to which it signs and sends (the stand-in for Logos Core's own modules); `lez` routes the
    /// session's LEZ reads and private sends through Logos Core's `lez_core`, `lez_report` lists those
    /// calls; `claim` and `burn` start a job in a thread of the module and answer at once (a private one is
    /// proven for minutes, and a host may end the call sooner), `job` answers a job's state and, once done, its
    /// reply, `jobs` lists them (calls/src/jobs.rs); `{"ok", ...}`.
    fn wallet(&mut self, request: String) -> String;
    /// The admin's and the freeze authority's functions: `status` (the vault's roles, registry, caps and freezes read
    /// through Logos Core's `eth_rpc_module`; LEZ's from the bridge program's states), a vault action built, checked
    /// with `eth_call` from the role's account and, with `send`, handed to Logos Core's `tx_sender_module` (one
    /// approval in the Logos signer), or a LEZ action's message for the role's LEZ wallet. JSON in and out.
    fn admin(&mut self, request: String) -> String;
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

/// Logos Core's own LEZ module, `lez_core` (logos-execution-zone-module: LEZ's wallet as a module), called by
/// name through the SDK as `eth_rpc_module` above: (method, the arguments as a JSON array, a timeout) -> the
/// method's reply as text. Not declared a dependency, for the same reason, and because the module must keep
/// working without it (the plain path). The calls crate uses it when the wallet session's `lez` call names
/// `lez_core` (calls/src/lez.rs): the bridge's actor states, the wallet's private account and the private
/// claim and burn.
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

/// A method of another Logos module, called by name through the SDK as `eth_rpc_module` above: here Logos Core's send
/// stack, `tx_sender_module` (`send`, `send_status`, `cancel_send`), which holds no dependency of ours either. The
/// calls crate sends the user's deposit and release and the admin's vault calls through it (calls/src/send.rs).
fn module_call(module: &str, method: &str, args: &serde_json::Value, timeout_ms: u64) -> Result<String, String> {
    let v = logos_rust_sdk::LogosModuleSDK::new()
        .plugin(module)
        .call_json_with_timeout(method, args, std::time::Duration::from_millis(timeout_ms.max(1)))
        .map_err(|e| format!("{module}.{method} did not answer: {e:?}"))?;
    Ok(match v {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    })
}

#[derive(Default)]
struct FerrataBridge;

/// A panic must not cross into the host: report it as JSON instead.
fn guarded(method: &str, f: impl FnOnce() -> String) -> String {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        serde_json::json!({ "verdict": "error", "error": format!("{method} panicked") }).to_string()
    })
}

impl FerrataBridgeModule for FerrataBridge {
    fn version(&mut self) -> String {
        guarded("version", ferrata_bridge_calls::version)
    }

    fn deposit(&mut self, request: String) -> String {
        guarded("deposit", || ferrata_bridge_calls::deposit(&request))
    }

    fn claim(&mut self, request: String) -> String {
        guarded("claim", || ferrata_bridge_calls::claim(&request))
    }

    fn burn(&mut self, request: String) -> String {
        guarded("burn", || ferrata_bridge_calls::burn(&request))
    }

    fn release(&mut self, request: String) -> String {
        guarded("release", || ferrata_bridge_calls::release(&request))
    }

    fn recover(&mut self, request: String) -> String {
        guarded("recover", || ferrata_bridge_calls::recover(&request))
    }

    fn status(&mut self, request: String) -> String {
        guarded("status", || ferrata_bridge_calls::status(&request))
    }

    fn prove_release(&mut self, request: String) -> String {
        guarded("prove_release", || ferrata_bridge_calls::prove_release(&request))
    }

    fn wallet(&mut self, request: String) -> String {
        guarded("wallet", || ferrata_bridge_calls::wallet(&request))
    }
    fn import_root(&mut self, request: String) -> String {
        guarded("import_root", || ferrata_bridge_calls::import_root(&request))
    }

    fn registry(&mut self, request: String) -> String {
        guarded("registry", || ferrata_bridge_calls::registry(&request))
    }
    fn admin(&mut self, request: String) -> String {
        guarded("admin", || ferrata_bridge_calls::admin(&request))
    }
}

#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    ferrata_bridge_calls::set_eth_rpc_module(eth_rpc_module_raw_rpc);
    ferrata_bridge_calls::set_lez_core(lez_core_call);
    ferrata_bridge_calls::set_module_call(module_call);
    install::<FerrataBridge>();
}
