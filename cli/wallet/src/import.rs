//! The relayer's `ImportRoot`: a deposit root `(root, n)` of the vault, read at the record of
//! `record_slot`, with the vault's account proof and the proofs of its slots 0 (the root) and 1 (the
//! leaf count), as `eth_getProof` gives them.

use ferrata_bridge_core::Hash;
use ferrata_lez_bridge_wire::{
    self as bw, config_actor, config_seed, root_actor, root_seed, BridgeConfig, BridgeMessage, ImportProof,
    PendingImport, RootState,
};
use ferrata_lez_wire::{attest, header_actor, RecordState};
use lee_core::account::AccountId;

use crate::Mention;

/// The proofs of an import from `eth_getProof`'s `accountProof` and the storage proofs of slots 0
/// and 1, with the block's RLP header in front when the record binds the block's hash only (a Gloas
/// record), as the program reads them.
pub fn import_proof(
    account: Vec<Vec<u8>>,
    root_slot: Vec<Vec<u8>>,
    count_slot: Vec<Vec<u8>>,
    header: Option<Vec<u8>>,
) -> ImportProof {
    let account = match header {
        Some(h) => bw::with_header(h, account),
        None => account,
    };
    ImportProof { account, root_slot, count_slot }
}

/// A call of the bridge program, ready to sign and send.
#[derive(Clone, Debug)]
pub struct ImportCall {
    pub what: String,
    pub mentions: Vec<Mention>,
    pub root_index: usize,
    pub message: BridgeMessage,
}

/// `ImportRoot` of `(root, leaf_count)` for configuration `cid` of the bridge program at `program`,
/// under the record at `record_slot` of the configuration's attestation instance. Sent at
/// `config(cid)`, which asks `header(id, slot)` to attest the vault's slots 0 and 1 and writes
/// `root(cid, n)`.
pub fn import_call(
    program: AccountId,
    cid: &Hash,
    config: &BridgeConfig,
    record_slot: u64,
    root: Hash,
    leaf_count: u64,
    proof: ImportProof,
) -> ImportCall {
    let message = BridgeMessage::ImportRoot { config_id: *cid, record_slot, root, leaf_count, proof };
    let mentions = vec![
        Mention::Pda { actor: config_actor(program, cid), seed: config_seed(cid) },
        Mention::Public(header_actor(config.attestation, &config.instance_id, record_slot)),
        Mention::Pda { actor: root_actor(program, cid, leaf_count), seed: root_seed(cid, leaf_count) },
    ];
    ImportCall {
        what: format!("import of root(cid, {leaf_count}) under the record of slot {record_slot}"),
        mentions,
        root_index: 0,
        message,
    }
}

/// The import as the two programs run it, natively: the attestation program evaluates the bridge's
/// request (`import_request`) against `record`, the `RecordState` at `header(id, slot)`, bound to the
/// instance or its root id as its `Attest` handler binds it (`attest::evaluate`); the bridge program
/// checks the reply (`check_import`). `Ok`: the `root(cid, n)` state the import writes; `Err`: the
/// code the transaction would exit with (the attestation program's or the bridge program's).
pub fn dry_run_import(
    cid: &Hash,
    config: &BridgeConfig,
    record: &RecordState,
    record_slot: u64,
    root: Hash,
    leaf_count: u64,
    proof: &ImportProof,
) -> Result<RootState, u8> {
    let request = bw::import_request(config, proof);
    let recorded = record.header()?;
    let bound = if request.instance_id == record.root_id { &record.root_id } else { &record.instance_id };
    let attestation = attest::evaluate(bound, record.chain_id, &recorded, &request)?;
    bw::check_import(config, cid, &PendingImport { record_slot, root, leaf_count }, &attestation)
}
