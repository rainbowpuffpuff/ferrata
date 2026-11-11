//! Shared code of the program fuzz targets: the bridge guest's source compiled on the host, and
//! the actors a delivery can name.

use ferrata_lez_bridge_wire as w;
use lee_core::account::{AccountId, Actor};

/// The bridge program's guest source (`lez/guests/bridge/src/main.rs`), compiled on the host by
/// `build.rs` (only its leading `//!` lines become `//`).
#[allow(dead_code, unused_imports, clippy::all)]
pub mod bridge_guest {
    include!(concat!(env!("OUT_DIR"), "/bridge_guest.rs"));

    /// The guest's `handle`: one delivery, `Ok(response)` or a refusal code.
    pub fn run(input: &ReceiveInput) -> Result<Response, u8> {
        handle(input)
    }
}

/// The program account the fuzzed deliveries run under (any fixed id serves).
pub const PROGRAM: [u8; 32] = [0xb7; 32];

pub fn program() -> AccountId {
    AccountId::new(PROGRAM)
}

/// The fields of a message that name actors (zero where a message has none).
#[derive(Clone, Copy, Default, Debug)]
pub struct Names {
    pub config_id: w::Root,
    pub token: w::Address,
    pub leaf_count: u64,
    pub evm_chain_id: u64,
    pub vault: w::Address,
    pub nullifier: w::Root,
    pub account: w::Root,
    pub note: w::Root,
}

pub fn names(m: &w::BridgeMessage) -> Names {
    use w::BridgeMessage as M;
    let mut n = Names::default();
    match m {
        M::Burn { config_id, r, memo, mac, token, amount, c_out } => {
            n.config_id = *config_id;
            n.token = *token;
            n.note = w::Note { r: *r, memo: *memo, mac: *mac, token: *token, amount: *amount, c_out: *c_out }.hash();
        }
        M::ImportRoot { config_id, leaf_count, .. } => {
            n.config_id = *config_id;
            n.leaf_count = *leaf_count;
        }
        M::Sunset { config_id } => n.config_id = *config_id,
        M::Echo { nonce } => n.account = *nonce,
        M::Init { config, .. } => {
            n.config_id = config.id(program());
            n.evm_chain_id = config.evm_chain_id;
            n.vault = config.vault;
        }
        M::Claim { config_id, leaf_count, .. } | M::ClaimPrivately { config_id, leaf_count, .. } => {
            n.config_id = *config_id;
            n.leaf_count = *leaf_count;
        }
        M::Read => {}
        M::WriteRoot { root } => {
            n.config_id = root.config_id;
            n.leaf_count = root.leaf_count;
            n.evm_chain_id = root.evm_chain_id;
            n.vault = root.vault;
        }
        M::Spend { nullifier, config_id, evm_chain_id, vault, token, leaf_count, mint_to, .. } => {
            n.nullifier = *nullifier;
            n.config_id = *config_id;
            n.evm_chain_id = *evm_chain_id;
            n.vault = *vault;
            n.token = *token;
            n.leaf_count = *leaf_count;
            n.account = mint_to.unwrap_or_default();
        }
        M::Mint { nullifier, config_id, evm_chain_id, vault, token, holder, .. } => {
            n.nullifier = *nullifier;
            n.config_id = *config_id;
            n.evm_chain_id = *evm_chain_id;
            n.vault = *vault;
            n.token = *token;
            n.account = *holder;
        }
        M::Register { config_id, entry, .. } => {
            n.config_id = *config_id;
            n.token = entry.token;
        }
        M::ImportFees { config_id, token, .. } | M::ExtendFees { config_id, token, .. } => {
            n.config_id = *config_id;
            n.token = *token;
        }
        M::WriteToken { state } => {
            n.config_id = state.config_id;
            n.token = state.entry.token;
            n.evm_chain_id = state.evm_chain_id;
            n.vault = state.vault;
        }
        M::Issue { config_id, token, nullifier, leaf_count, mint_to, .. } => {
            n.config_id = *config_id;
            n.token = *token;
            n.nullifier = *nullifier;
            n.leaf_count = *leaf_count;
            n.account = mint_to.unwrap_or_default();
        }
        M::Admit { config_id, token, .. } => {
            n.config_id = *config_id;
            n.token = *token;
        }
        M::Freeze { config_id, token, .. } | M::SetCap { config_id, token, .. } | M::Tick { config_id, token, .. } => {
            n.config_id = *config_id;
            n.token = token.unwrap_or_default();
        }
    }
    n
}

/// One of the program's actors named by `n` (mode 0..=7).
pub fn actor(mode: u8, n: &Names) -> Actor {
    let p = program();
    match mode % 8 {
        0 => w::config_actor(p, &n.config_id),
        1 => w::token_actor(p, &n.config_id, &n.token),
        2 => w::root_actor(p, &n.config_id, n.leaf_count),
        3 => w::nullifier_actor(p, n.evm_chain_id, &n.vault, &n.nullifier),
        4 => w::holder_actor(p, AccountId::new(n.account)),
        5 => w::burn_actor(p, &n.config_id, &n.note),
        6 => w::own_actor(p, AccountId::new(n.account)),
        _ => Actor::new(AccountId::new(n.account), p),
    }
}

/// The actor a state waits for a reply from, as the guest's `handle` checks it.
pub fn awaited(state: &w::BridgeState) -> Option<Actor> {
    match state {
        w::BridgeState::Config(c) => c.awaiting(),
        w::BridgeState::Token(t) => t.awaiting(),
        w::BridgeState::Holder(w::HolderState { pending: Some(p), .. }) => {
            Some(w::root_actor(program(), &p.config_id, p.leaf_count))
        }
        _ => None,
    }
}
