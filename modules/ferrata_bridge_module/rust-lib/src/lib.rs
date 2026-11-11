//! `ferrata_bridge_module`: Ferrata's bridge module for Logos Core (prototype).
//!
//! Rust-first, as the builder's Rust template: the trait below is the module's contract. The builder
//! derives the `.lidl` from it (`codegen.rust.trait` in metadata.json) and generates the scaffold
//! included below. Each method takes and returns one JSON string; the work is done by the
//! `ferrata-bridge-calls` crate (`calls/`), which runs and is tested without the scaffold.

use std::panic::{catch_unwind, AssertUnwindSafe};

/// The module's methods. The trait's name is the module's name in PascalCase.
pub trait FerrataBridgeModule: Send + 'static {
    /// The module's version and the ferrata-bridge-core it was built from, as JSON.
    fn version(&mut self) -> String;

    /// A deposit's commitment and the vault call that makes it (`deposit` or `depositETH`), and,
    /// with its leaf index, the leaf, deposit id and nullifier. JSON in and out (modules/README.md).
    fn deposit(&mut self, request: String) -> String;

    /// A claim of a deposit: its Merkle path in the deposit tree, the check the bridge program runs
    /// (`verify_claim`), its nullifier, and the program's `Claim` message. JSON in and out.
    fn claim(&mut self, request: String) -> String;

    /// A burn: the note (tag, memo, MAC, commitment) and the bridge program's `Burn` message, and,
    /// with its index in the burn tree, the leaf, burn id and release nullifier. JSON in and out.
    fn burn(&mut self, request: String) -> String;

    /// A release: the burn's path in the checkpointed burn tree, the release guest's check
    /// (`verify_release`), its nullifier and the journal the vault verifies. JSON in and out.
    fn release(&mut self, request: String) -> String;

    /// The wallet's own deposits and burns among public ones, from its seed alone. JSON in and out.
    fn recover(&mut self, request: String) -> String;

    /// Where each deposit and burn stands (awaiting root, claimable, claimed; awaiting checkpoint,
    /// releasable, released), from the chains' public state. JSON in and out.
    fn status(&mut self, request: String) -> String;

    /// Called once the module is wired to the host.
    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}
}

// The builder injects the generated scaffold here: `install`, `RustModuleContext`, `context()`.
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/provider_gen.rs"));

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
}

#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    install::<FerrataBridge>();
}
