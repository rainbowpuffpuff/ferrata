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

    /// Called once the module is wired to the host.
    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}
}

// The builder injects the generated scaffold here: `install`, `RustModuleContext`, `context()`, and the
// typed client of each dependency (`eth_rpc_module::EthRpcModuleClient`, from deps/eth_rpc_module.lidl).
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/provider_gen.rs"));

/// Logos Core's `eth_rpc_module.raw_rpc(chain_id, method, params_json)`, through the client the builder
/// generates from that module's contract (metadata.json: an optional dependency, its contract in
/// `deps/eth_rpc_module.lidl`). `import_root` reads the vault's `eth_getProof` through it when the
/// request says `"eth": {"via": "eth_rpc_module", "chain_id": N}`.
fn eth_rpc_module_raw_rpc(chain_id: i64, method: &str, params: &str, timeout_ms: u64) -> Result<String, String> {
    eth_rpc_module::EthRpcModuleClient::new()
        .raw_rpc_with_timeout(chain_id, method, params, std::time::Duration::from_millis(timeout_ms.max(1)))
        .map_err(|e| format!("eth_rpc_module.raw_rpc did not answer: {e:?}"))
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

    fn import_root(&mut self, request: String) -> String {
        guarded("import_root", || ferrata_bridge_calls::import_root(&request))
    }

    fn registry(&mut self, request: String) -> String {
        guarded("registry", || ferrata_bridge_calls::registry(&request))
    }
}

#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    ferrata_bridge_calls::set_eth_rpc_module(eth_rpc_module_raw_rpc);
    install::<FerrataBridge>();
}
