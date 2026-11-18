//! Drives Ferrata's LEZ v0.3.1 light-client program (`../guest`) through LEZ v0.3.1's own block
//! application, with real Sepolia and mainnet light-client data. EXPERIMENTAL, UNAUDITED
//! (`../README.md`).
//!
//! **The chain.** Every transaction is a public transaction with a fee declaration, settled alone in
//! a block of its own as LEZ's sequencer settles it: `chain_state::apply::derive_block_summary`
//! derives the block's fee summary and payout (the builder's path), the block gets its fee and clock
//! transactions, and `chain_state::apply::apply_block` applies it (the path of the sequencer and of
//! every follower: the fee reserve, the action metered at the declared gas limit, the refund, the
//! per-block caps `MAX_GAS_EXEC` and `MAX_GAS_STOR`). The state is LEZ v0.3.1's testnet genesis
//! (`testnet_initial_state::initial_state`) and one funded payer.
//!
//! **The relayer.** [`Instance`] keeps the native light client (`ferrata_core::policy`, full
//! committees) beside the program's instance, and builds each instruction as a relayer would: the
//! signer witness from the committee that signed, the residue witness from the Miller-loop product
//! computed natively, the adoption parts from the committee's keys, the record and the timestamp
//! window the transaction claims.

use std::path::PathBuf;

use chain_state::apply::{apply_block, derive_block_summary, Tip};
use common::{
    block::HashableBlockData,
    transaction::{clock_invocation, fee_invocation, LeeTransaction},
};
use ferrata_core::{
    attest as proven,
    bls::{Aggregation, Blst},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    merkle::ZERO,
    policy::{Adoption, FinalizedStore},
    steps::{self, Job, Phase},
    store::{ApplyOutcome, StoreCore},
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    validate::Context,
    witness::{uncompressed_keys, CompactBootstrap, CompactStore, CompactUpdate, HeaderRecord, SignerWitness},
    Root,
};
use ferrata_v03_wire::{
    header_account, instance_account, instance_id, statement_account, Config, InstanceState, Instruction, Part,
    RecordClaim, RecordState, SlotProof, Statement, Step, StorageQuery, Verify, Window,
};
use lee::{
    error::LeeError,
    public_transaction::{Message, WitnessSet},
    AccountId, FeeDeclaration, PrivateKey, PublicKey, PublicTransaction, V03State,
};
use lee_core::{account::ProgramShardSelector, program::PROGRAM_LOADER_ACCOUNT_ID};
use sha2::{Digest, Sha256};

pub use methods_v03::{FERRATA_V03_ELF, FERRATA_V03_ID};

/// The BLS backend of the native light client (as the core's tests and the LEZ v0.4 harness).
pub static BLS: Blst = Blst::new(Aggregation::Auto);

/// LEZ v0.3.1's per-transaction (and per-block) execution cap (`fee_core::market::MAX_GAS_EXEC`).
pub const MAX_GAS_EXEC: u64 = fee_core::market::MAX_GAS_EXEC;
/// The gas limit declared by every program transaction of the tests: the cap.
pub const GAS_LIMIT: u64 = MAX_GAS_EXEC;
/// The gas limit of the program loader's transactions (native: it charges no cycles).
pub const LOADER_GAS_LIMIT: u64 = 100_000;
/// The harness payer's genesis balance (native units).
pub const PAYER_BALANCE: u128 = 1_000_000_000_000_000;

pub fn sha256(parts: &[&[u8]]) -> Root {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

pub fn root(hex_str: &str) -> Root {
    hex::decode(hex_str.trim_start_matches("0x")).unwrap().try_into().unwrap()
}

/// The image id as hex (little-endian words, as risc0 prints a `Digest`).
pub fn image_id_hex(id: [u32; 8]) -> String {
    hex::encode(id.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>())
}

/// A key derived from a public label: throwaway by design.
pub fn derived_key(label: &str) -> PrivateKey {
    PrivateKey::try_new(sha256(&[b"ferrata-v03/key/", label.as_bytes()])).expect("a valid secp256k1 scalar")
}

pub fn account_of(key: &PrivateKey) -> AccountId {
    AccountId::from(&PublicKey::new_from_private_key(key))
}

// ---------------------------------------------------------------------------------------------
// Chain configurations and recorded data
// ---------------------------------------------------------------------------------------------

pub const MAINNET_CHAIN_ID: u64 = 1;
pub const MAINNET_GENESIS_TIME: u64 = 1_606_824_023;
pub const MAINNET_GENESIS_VALIDATORS_ROOT: &str = "4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
/// The block root the mainnet fixture's bootstrap is for (`crates/ferrata-spectests/fixtures/mainnet-p1899`).
pub const MAINNET_CHECKPOINT: &str = "64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac";

pub const SEPOLIA_CHAIN_ID: u64 = 11_155_111;
pub const SEPOLIA_GENESIS_TIME: u64 = 1_655_733_600;
pub const SEPOLIA_GENESIS_VALIDATORS_ROOT: &str = "d8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078";
/// The period-1378 bootstrap's block root (`sepolia-gloas-2026-11-10`).
pub const SEPOLIA_CHECKPOINT: &str = "5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d";
pub const SEPOLIA_FIRST_PERIOD: u64 = 1378;
pub const SEPOLIA_GLOAS_PERIOD: u64 = 1379;
pub const SEPOLIA_LAST_PERIOD: u64 = 1409;

/// Mainnet's fork schedule (consensus-specs `configs/mainnet.yaml`, as the LEZ v0.4 harness).
pub fn mainnet_spec() -> ChainSpec {
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [[0, 0, 0, 0], [1, 0, 0, 0], [2, 0, 0, 0], [3, 0, 0, 0], [4, 0, 0, 0], [5, 0, 0, 0], [6, 0, 0, 0], [7, 0, 0, 0], [8, 0, 0, 0]],
        epochs: [0, 74_240, 144_896, 194_048, 269_568, 364_032, 411_392, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    }
}

/// Sepolia's fork schedule (eth-clients/sepolia at 7b1b9d9): Gloas at epoch 353,024.
pub fn sepolia_spec() -> ChainSpec {
    let v = |b: u8| [0x90, 0x00, 0x00, b];
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [v(0x69), v(0x70), v(0x71), v(0x72), v(0x73), v(0x74), v(0x75), v(0x76), [0; 4]],
        epochs: [0, 50, 100, 56_832, 132_608, 222_464, 272_640, 353_024, FAR_FUTURE_EPOCH],
    }
}

/// The weak-subjectivity period of the LEZ v0.4 tests (`ws::compute_weak_subjectivity_period` at
/// 33,554,432 ETH of active balance, mainnet's WS config): 3,532 epochs.
pub fn test_ws_period_epochs() -> u64 {
    use ferrata_core::ws::{compute_weak_subjectivity_period, WsConfig, ETH_TO_GWEI};
    compute_weak_subjectivity_period(33_554_432 * ETH_TO_GWEI, &WsConfig::MAINNET).expect("in range")
}

fn config_of(spec: &ChainSpec, chain_id: u64, gvr: &str, genesis_time: u64, store_fork: u8, parts: u8) -> Config {
    Config {
        chain_id,
        sync_committee_size: 512,
        slots_per_epoch: spec.preset.slots_per_epoch,
        epochs_per_sync_committee_period: spec.preset.epochs_per_sync_committee_period,
        min_sync_committee_participants: spec.preset.min_sync_committee_participants,
        fork_versions: spec.versions,
        fork_epochs: spec.epochs,
        genesis_validators_root: root(gvr),
        genesis_time,
        seconds_per_slot: 12,
        store_fork,
        adoption_parts: parts,
        ws_period_epochs: test_ws_period_epochs(),
    }
}

/// Mainnet, the store in the Electra layout (the bootstrap is of the Fulu era), adoption in `parts`.
pub fn mainnet_config(parts: u8) -> Config {
    config_of(&mainnet_spec(), MAINNET_CHAIN_ID, MAINNET_GENESIS_VALIDATORS_ROOT, MAINNET_GENESIS_TIME, 3, parts)
}

/// Sepolia, the store in the Electra layout at the start, Gloas in the schedule.
pub fn sepolia_config(parts: u8) -> Config {
    config_of(&sepolia_spec(), SEPOLIA_CHAIN_ID, SEPOLIA_GENESIS_VALIDATORS_ROOT, SEPOLIA_GENESIS_TIME, 3, parts)
}

/// `crates/ferrata-spectests/fixtures/<name>` of this checkout (or under `FERRATA_FIXTURES`).
pub fn fixture_dir(name: &str) -> PathBuf {
    std::env::var_os("FERRATA_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/ferrata-spectests/fixtures"))
        .join(name)
}

/// A fixture file, checked against the fixture's pinned `SHA256SUMS`.
pub fn fixture(dir: &str, name: &str) -> Vec<u8> {
    let d = fixture_dir(dir);
    let bytes = std::fs::read(d.join(name)).unwrap_or_else(|e| panic!("{dir}/{name}: {e}"));
    let sums = std::fs::read_to_string(d.join("SHA256SUMS")).expect("SHA256SUMS");
    let want = sums
        .lines()
        .find(|l| l.ends_with(&format!("  {name}")))
        .unwrap_or_else(|| panic!("{name} is not in {dir}/SHA256SUMS"))
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{dir}/{name} does not match SHA256SUMS");
    bytes
}

pub fn mainnet_bootstrap() -> LightClientBootstrap {
    LightClientBootstrap::decode(&fixture("mainnet-p1899", "bootstrap.ssz"), LcFork::Electra, 512).unwrap()
}

pub fn mainnet_updates() -> Vec<LightClientUpdate> {
    (0..5)
        .map(|i| {
            LightClientUpdate::decode(&fixture("mainnet-p1899", &format!("update_{i}.ssz")), LcFork::Electra, 512)
                .unwrap()
        })
        .collect()
}

pub fn sepolia_bootstrap() -> LightClientBootstrap {
    LightClientBootstrap::decode(&fixture("sepolia-gloas-2026-11-10", "bootstrap.ssz"), LcFork::Electra, 512).unwrap()
}

/// The layout a beacon node serves Sepolia's period update in.
pub fn sepolia_served_layout(period: u64) -> LcFork {
    if period < SEPOLIA_GLOAS_PERIOD {
        LcFork::Electra
    } else {
        LcFork::Gloas
    }
}

pub fn sepolia_update(period: u64) -> LightClientUpdate {
    let b = fixture("sepolia-gloas-2026-11-10", &format!("update_{period}.ssz"));
    LightClientUpdate::decode(&b, sepolia_served_layout(period), 512).unwrap()
}

/// A Gloas `LightClientFinalityUpdate` (every field fixed-size) as the `LightClientUpdate` it
/// stands for (as the LEZ v0.4 harness reads it).
pub fn gloas_finality_update(b: &[u8]) -> LightClientUpdate {
    const H: usize = 112 + 32 + 11 * 32;
    assert_eq!(b.len(), 2 * H + 9 * 32 + 160 + 8, "a Gloas LightClientFinalityUpdate");
    LightClientUpdate {
        attested_header: LightClientHeader::decode(&b[..H], LcFork::Gloas).unwrap(),
        next_sync_committee: SyncCommittee::empty(512),
        next_sync_committee_branch: vec![ZERO; LcFork::Gloas.sync_committee_branch_len()],
        finalized_header: LightClientHeader::decode(&b[H..2 * H], LcFork::Gloas).unwrap(),
        finality_branch: b[2 * H..2 * H + 288].chunks(32).map(|c| c.try_into().unwrap()).collect(),
        sync_aggregate: SyncAggregate::decode(&b[2 * H + 288..2 * H + 448], 512).unwrap(),
        signature_slot: u64::from_le_bytes(b[2 * H + 448..].try_into().unwrap()),
    }
}

/// `update` without its next committee: a finality update with the same signature.
pub fn as_finality_update(update: &LightClientUpdate) -> LightClientUpdate {
    let mut u = update.clone();
    u.next_sync_committee = SyncCommittee::empty(512);
    u.next_sync_committee_branch = vec![ZERO; u.next_sync_committee_branch.len()];
    u
}

/// An `eth_getProof` response (the JSON-RPC envelope or its `result`) as a [`StorageQuery`] of
/// `slots` (all of the response's if empty), with the block's RLP header.
pub fn storage_query(proof_json: &[u8], chain_id: u64, header: Option<Vec<u8>>, slots: &[usize]) -> StorageQuery {
    let v: serde_json::Value = serde_json::from_slice(proof_json).expect("eth_getProof JSON");
    let r = v.get("result").unwrap_or(&v);
    let hexv = |s: &serde_json::Value| hex::decode(s.as_str().unwrap().trim_start_matches("0x")).unwrap();
    let address: [u8; 20] = hexv(&r["address"]).try_into().unwrap();
    let account_proof = r["accountProof"].as_array().unwrap().iter().map(hexv).collect();
    let all = r["storageProof"].as_array().unwrap();
    let pick: Vec<usize> = if slots.is_empty() { (0..all.len()).collect() } else { slots.to_vec() };
    let slots = pick
        .iter()
        .map(|&i| {
            let s = &all[i];
            let key = hexv(&s["key"]);
            let mut slot = [0u8; 32];
            slot[32 - key.len()..].copy_from_slice(&key);
            SlotProof { slot, proof: s["proof"].as_array().unwrap().iter().map(hexv).collect() }
        })
        .collect();
    StorageQuery { chain_id, address, account_proof, slots, header }
}

// ---------------------------------------------------------------------------------------------
// The chain
// ---------------------------------------------------------------------------------------------

/// One settled transaction.
#[derive(Clone, Debug)]
pub struct TxReport {
    pub label: String,
    pub block: u64,
    /// `applied`, `refused` (the program exited with a code: charged, reverted, in the block),
    /// `reverted: …` (another chargeable failure), or `dropped: …` (the builder would not include
    /// it: no block).
    pub status: String,
    pub exit: Option<u32>,
    /// Execution gas the block charged for it (zkVM cycles, clamped to the gas limit).
    pub exec_gas: u64,
    /// Storage gas: the transaction's wire size in bytes.
    pub stor_gas: u64,
    /// The runtime's cycle count at the gas limit: the whole transaction's if it applied, the
    /// refusing call's alone if it was refused (`exec_gas` is what the block charged).
    pub cycles: u64,
    pub gas_limit: u64,
    /// What the payer paid (native units).
    pub fee: u128,
    pub base_fee_exec: u64,
    pub base_fee_stor: u64,
    pub timestamp_ms: u64,
}

impl TxReport {
    pub fn ok(&self) -> bool {
        self.status == "applied"
    }

    pub fn refused_with(&self, code: u8) -> bool {
        self.status == "refused" && self.exit == Some(u32::from(code))
    }

    pub fn tsv_header() -> &'static str {
        "label\tblock\tstatus\texit\texec_gas\tof_max_gas_exec\tstor_gas\tcycles\tgas_limit\tfee\tbase_fee_exec\tbase_fee_stor\ttimestamp_ms"
    }

    pub fn tsv(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{:.1}%\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.label,
            self.block,
            self.status,
            self.exit.map_or("-".into(), |c| c.to_string()),
            self.exec_gas,
            self.exec_gas as f64 * 100.0 / MAX_GAS_EXEC as f64,
            self.stor_gas,
            self.cycles,
            self.gas_limit,
            self.fee,
            self.base_fee_exec,
            self.base_fee_stor,
            self.timestamp_ms
        )
    }
}

/// LEZ v0.3.1's state machine and block application, one user transaction per block.
pub struct Chain {
    pub state: V03State,
    tip: Tip,
    pub next_block: u64,
    pub last_timestamp: u64,
    pub payer_key: PrivateKey,
    pub payer: AccountId,
    pub reports: Vec<TxReport>,
}

impl Default for Chain {
    fn default() -> Self {
        Self::new()
    }
}

impl Chain {
    /// LEZ v0.3.1's testnet genesis (no cross-zone programs), the test block producer, one payer.
    pub fn new() -> Self {
        Self::with_accounts(vec![])
    }

    /// [`Chain::new`] with `extra` public accounts at genesis (to replay another chain's payer at its nonce).
    pub fn with_accounts(extra: Vec<(AccountId, lee_core::account::Account)>) -> Self {
        let payer_key = derived_key("harness/payer");
        let payer = account_of(&payer_key);
        let mut state = testnet_initial_state::initial_state(false)
            .with_public_accounts([common::test_utils::producer_seed()])
            .with_public_account_balances([(payer, PAYER_BALANCE)])
            .with_public_accounts(extra);
        let genesis = common::test_utils::produce_dummy_block(1, None, vec![]);
        apply_block(None, &genesis, &mut state).expect("genesis applies");
        Self { state, tip: Tip::from(&genesis), next_block: 2, last_timestamp: 100, payer_key, payer, reports: vec![] }
    }

    pub fn balance(&self, account: AccountId) -> u128 {
        self.state.get_account_by_id(account).data.native_balance().unwrap_or(0)
    }

    /// The shard `program` keeps on `account` (empty if none).
    pub fn shard(&self, account: AccountId, program: AccountId) -> Vec<u8> {
        self.state.get_account_by_id(account).data.shard(program).to_vec()
    }

    /// A public transaction to `program` over `accounts` (each `program`'s shard), signed by
    /// `signers` and the payer (who pays), declaring `gas_limit`.
    pub fn transaction(
        &self,
        program: AccountId,
        accounts: &[AccountId],
        instruction: Vec<u8>,
        signers: &[&PrivateKey],
        gas_limit: u64,
    ) -> PublicTransaction {
        let mut keys: Vec<&PrivateKey> = signers.to_vec();
        keys.push(&self.payer_key);
        let nonces = keys.iter().map(|k| self.state.get_account_by_id(account_of(k)).nonce).collect();
        let selectors = accounts.iter().map(|a| ProgramShardSelector::new(*a, program)).collect();
        let fee = FeeDeclaration::new(self.payer, gas_limit, 0, u128::MAX >> 1);
        let message = Message::new_preserialized(program, selectors, nonces, instruction, Some(fee));
        let witness = WitnessSet::for_message(&message, &keys);
        PublicTransaction::new(message, witness)
    }

    /// Settle `tx` in a block of its own at `timestamp_ms` (moved forward if the chain is past it).
    pub fn settle(&mut self, label: &str, tx: PublicTransaction, gas_limit: u64, timestamp_ms: u64) -> TxReport {
        let timestamp = timestamp_ms.max(self.last_timestamp + 1);
        let block_id = self.next_block;
        let opening = chain_state::apply::opening_fee_state(&self.state);
        // The outcome as LEZ's runtime reports it, at the declared gas limit (pure).
        let pure = lee::ValidatedStateDiff::from_public_transaction_with_cycle_budget(
            &tx,
            &self.state,
            block_id,
            timestamp,
            gas_limit,
        );
        let (mut status, exit, cycles) = match &pure {
            Ok((_, charge)) => ("applied".to_owned(), None, charge.cycles),
            Err(LeeError::ProgramExitedWithCode { code, cycles }) => ("refused".to_owned(), Some(*code), *cycles),
            Err(e) => (format!("reverted: {e}"), None, 0),
        };
        drop(pure);
        let before = self.balance(self.payer);
        let txs = vec![LeeTransaction::Public(tx)];
        let mut report = TxReport {
            label: label.to_owned(),
            block: 0,
            status: String::new(),
            exit,
            exec_gas: 0,
            stor_gas: 0,
            cycles,
            gas_limit,
            fee: 0,
            base_fee_exec: opening.base_fee_exec,
            base_fee_stor: opening.base_fee_stor,
            timestamp_ms: timestamp,
        };
        match derive_block_summary(&self.state, &txs, block_id, timestamp) {
            Ok((summary, payout)) => {
                let mut transactions = txs;
                transactions.push(LeeTransaction::Public(fee_invocation(
                    summary.clone(),
                    payout,
                    common::test_utils::producer_account_for_testing(),
                )));
                transactions.push(LeeTransaction::Public(clock_invocation(block_id, timestamp)));
                let block = HashableBlockData {
                    block_id,
                    prev_block_hash: self.tip.hash,
                    timestamp,
                    transactions,
                }
                .into_pending_block();
                apply_block(Some(&self.tip), &block, &mut self.state).expect("a derived block applies");
                self.tip = Tip::from(&block);
                self.next_block += 1;
                self.last_timestamp = timestamp;
                report.block = block_id;
                report.exec_gas = summary.gas_used_exec;
                report.stor_gas = summary.gas_used_stor;
                report.fee = before - self.balance(self.payer);
            }
            Err(e) => status = format!("dropped: {e}"),
        }
        report.status = status;
        self.reports.push(report.clone());
        report
    }

    /// Settle `txs` (label, signed transaction, declared gas limit) as LEZ v0.3.1's block builder
    /// groups queued transactions (`lez/sequencer/core/src/lib.rs` 1783-1840): in order, a transaction
    /// joins the current block only if the execution gas charged so far in the block plus its
    /// *declared* limit is at most `MAX_GAS_EXEC` and the bytes fit `MAX_GAS_STOR`; otherwise it, and
    /// everything behind it, waits for the next block. A transaction the settlement rejects (a nonce
    /// mismatch, a fee it cannot reserve) is dropped, as the builder drops it. Every block is then
    /// derived and applied as in [`Chain::settle`].
    pub fn settle_queued(&mut self, txs: &[(String, PublicTransaction, u64)], timestamp_ms: u64) -> Vec<TxReport> {
        use chain_state::apply::{opening_fee_state, settle_transaction};
        let mut out = Vec::with_capacity(txs.len());
        let mut i = 0;
        while i < txs.len() {
            let block_id = self.next_block;
            let timestamp = timestamp_ms.max(self.last_timestamp + 1);
            let opening = opening_fee_state(&self.state);
            let mut scratch = self.state.clone();
            let mut summary = fee_core::BlockFeeSummary::default();
            let mut included: Vec<LeeTransaction> = vec![];
            let mut reports: Vec<TxReport> = vec![];
            while i < txs.len() {
                let (label, tx, gas_limit) = &txs[i];
                let wrapped = LeeTransaction::Public(tx.clone());
                let bytes = borsh::to_vec(&wrapped).unwrap().len() as u64;
                if summary.gas_used_exec + gas_limit > MAX_GAS_EXEC || summary.gas_used_stor + bytes > fee_core::market::MAX_GAS_STOR {
                    if included.is_empty() {
                        // Fits no block, not even an empty one: the builder drops it.
                        out.push(TxReport { label: label.clone(), block: 0, status: "dropped: over the block caps".into(), exit: None, exec_gas: 0, stor_gas: bytes, cycles: 0, gas_limit: *gas_limit, fee: 0, base_fee_exec: opening.base_fee_exec, base_fee_stor: opening.base_fee_stor, timestamp_ms: timestamp });
                        i += 1;
                        continue;
                    }
                    break;
                }
                let pure = lee::ValidatedStateDiff::from_public_transaction_with_cycle_budget(tx, &scratch, block_id, timestamp, *gas_limit);
                let (status, exit, cycles) = match &pure {
                    Ok((_, charge)) => ("applied".to_owned(), None, charge.cycles),
                    Err(LeeError::ProgramExitedWithCode { code, cycles }) => ("refused".to_owned(), Some(*code), *cycles),
                    Err(e) => (format!("reverted: {e}"), None, 0),
                };
                drop(pure);
                let payer = tx.message().fee.as_ref().map(|f| f.payer).unwrap_or(self.payer);
                let paid_before = scratch.get_account_by_id(payer).data.native_balance().unwrap_or(0);
                let (exec_before, stor_before) = (summary.gas_used_exec, summary.gas_used_stor);
                let mut r = TxReport { label: label.clone(), block: block_id, status, exit, exec_gas: 0, stor_gas: bytes, cycles, gas_limit: *gas_limit, fee: 0, base_fee_exec: opening.base_fee_exec, base_fee_stor: opening.base_fee_stor, timestamp_ms: timestamp };
                match settle_transaction(&wrapped, &mut scratch, &opening, block_id, timestamp, included.len() as u64, &mut summary) {
                    Ok(_) => {
                        r.exec_gas = summary.gas_used_exec - exec_before;
                        r.stor_gas = summary.gas_used_stor - stor_before;
                        r.fee = paid_before - scratch.get_account_by_id(payer).data.native_balance().unwrap_or(0);
                        included.push(wrapped);
                    }
                    Err(e) => {
                        r.block = 0;
                        r.status = format!("dropped: {e}");
                    }
                }
                reports.push(r);
                i += 1;
            }
            if included.is_empty() {
                out.extend(reports);
                continue;
            }
            let (summary, payout) = derive_block_summary(&self.state, &included, block_id, timestamp).expect("the block's transactions settle");
            let mut transactions = included;
            transactions.push(LeeTransaction::Public(fee_invocation(summary, payout, common::test_utils::producer_account_for_testing())));
            transactions.push(LeeTransaction::Public(clock_invocation(block_id, timestamp)));
            let block = HashableBlockData { block_id, prev_block_hash: self.tip.hash, timestamp, transactions }.into_pending_block();
            apply_block(Some(&self.tip), &block, &mut self.state).expect("a derived block applies");
            self.tip = Tip::from(&block);
            self.next_block += 1;
            self.last_timestamp = timestamp;
            self.reports.extend(reports.iter().cloned());
            out.extend(reports);
        }
        out
    }

    pub fn submit(
        &mut self,
        label: &str,
        program: AccountId,
        accounts: &[AccountId],
        instruction: Vec<u8>,
        signers: &[&PrivateKey],
        gas_limit: u64,
        timestamp_ms: u64,
    ) -> TxReport {
        let tx = self.transaction(program, accounts, instruction, signers, gas_limit);
        self.settle(label, tx, gas_limit, timestamp_ms)
    }

    /// Deploy `elf` through LEZ's `program_loader` as a deployer does (`wallet program-loader
    /// deploy`): one `WriteSegment` per 96 KiB chunk of the user ELF, tail first, then
    /// `CreateHeader` (immutable) at an account whose key signs. Returns the program's account.
    pub fn deploy(&mut self, label: &str, elf: &[u8]) -> AccountId {
        let user_elf = risc0_binfmt::ProgramBinary::decode(elf).expect("a RISC Zero program binary").user_elf.to_vec();
        let chunks: Vec<&[u8]> = user_elf.chunks(program_loader_core::MAX_SEGMENT_DATA_LEN).collect();
        let header_key = derived_key(&format!("deploy/{label}/header"));
        let header = account_of(&header_key);
        let segments: Vec<AccountId> = (0..chunks.len())
            .map(|i| account_of(&derived_key(&format!("deploy/{label}/segment/{i}"))))
            .collect();
        let t = self.last_timestamp + 1000;
        for i in (0..chunks.len()).rev() {
            let next = segments.get(i + 1).copied();
            let mut accounts = vec![segments[i]];
            accounts.extend(next);
            let instruction = borsh::to_vec(&program_loader_core::Instruction::WriteSegment {
                bytecode: chunks[i].to_vec(),
                next_segment: next,
            })
            .unwrap();
            let r = self.submit(
                &format!("deploy {label}: WriteSegment {} of {} ({} bytes)", i + 1, chunks.len(), chunks[i].len()),
                PROGRAM_LOADER_ACCOUNT_ID,
                &accounts,
                instruction,
                &[],
                LOADER_GAS_LIMIT,
                t,
            );
            assert!(r.ok(), "{r:?}");
        }
        let mut accounts = vec![header];
        accounts.extend(&segments);
        let instruction = borsh::to_vec(&program_loader_core::Instruction::CreateHeader {
            first_segment: segments[0],
            immutable: true,
        })
        .unwrap();
        let r = self.submit(
            &format!("deploy {label}: CreateHeader (immutable)"),
            PROGRAM_LOADER_ACCOUNT_ID,
            &accounts,
            instruction,
            &[&header_key],
            LOADER_GAS_LIMIT,
            t,
        );
        assert!(r.ok(), "{r:?}");
        assert_eq!(self.state.get_program_image_id(header), Some(FERRATA_V03_ID), "the loader's image id");
        header
    }
}

// ---------------------------------------------------------------------------------------------
// The relayer's side
// ---------------------------------------------------------------------------------------------

/// How a transaction composes the signing key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signers {
    /// The core's `SignerWitness`: a key and a branch per non-signer.
    Core,
    /// EXPERIMENTAL: `residue_agg::NonSigners` (one multiproof), summed by blst's batch addition.
    Multiproof,
}

/// One transaction a relayer sends: its instruction, its accounts and the time it is valid from.
#[derive(Clone, Debug)]
pub struct Planned {
    pub label: String,
    pub instruction: Instruction,
    pub accounts: Vec<AccountId>,
    pub timestamp_ms: u64,
}

/// An instance of the program at `program` and the native light client beside it.
#[derive(Clone)]
pub struct Instance {
    pub program: AccountId,
    pub config: Config,
    pub checkpoint: Root,
    pub id: Root,
    pub spec: ChainSpec,
    pub native: FinalizedStore,
}

impl Instance {
    pub fn new(program: AccountId, config: Config, checkpoint: Root, bootstrap: &LightClientBootstrap) -> Self {
        let spec = config.spec();
        let fork = config.fork().expect("a known layout");
        let native =
            FinalizedStore::initialize_with(&checkpoint, bootstrap, fork, &spec, &BLS, Adoption::OnChain).unwrap();
        let id = instance_id(&config, &checkpoint);
        Self { program, config, checkpoint, id, spec, native }
    }

    pub fn account(&self) -> AccountId {
        instance_account(&self.program, &self.id)
    }

    pub fn header_account(&self, slot: u64) -> AccountId {
        header_account(&self.program, &self.id, slot)
    }

    pub fn ctx(&self) -> Context<'_> {
        Context { spec: &self.spec, genesis_validators_root: self.config.genesis_validators_root, bls: &BLS }
    }

    /// The instance's state on `chain`, if any.
    pub fn state(&self, chain: &Chain) -> Option<InstanceState> {
        let shard = chain.shard(self.account(), self.program);
        (!shard.is_empty()).then(|| borsh::from_slice(&shard).expect("instance state decodes"))
    }

    pub fn store(&self, chain: &Chain) -> CompactStore {
        CompactStore::decode(&self.state(chain).expect("an instance").store).unwrap()
    }

    pub fn record(&self, chain: &Chain, slot: u64) -> Option<RecordState> {
        let shard = chain.shard(self.header_account(slot), self.program);
        (!shard.is_empty()).then(|| borsh::from_slice(&shard).expect("record decodes"))
    }

    /// The adoption parts of `committee` (uncompressed keys, `adoption_parts` parts).
    pub fn parts(&self, committee: &SyncCommittee) -> Vec<Part> {
        let keys = uncompressed_keys(committee).expect("valid compressed keys");
        let per = keys.len() / self.config.adoption_parts as usize;
        keys.chunks(per).enumerate().map(|(i, k)| Part { index: i as u8, keys: k.to_vec() }).collect()
    }

    fn claim(&self, record: &HeaderRecord) -> RecordClaim {
        RecordClaim { chain_id: self.config.chain_id, record: record.encode() }
    }

    /// `Init` with the first `parts_now` adoption parts (all of them: one transaction).
    pub fn init(&self, bootstrap: &LightClientBootstrap, parts_now: usize) -> (Planned, Vec<Part>) {
        let fork = self.config.fork().unwrap();
        let compact = CompactBootstrap::of(bootstrap);
        let (_, job) = steps::initialize(&self.checkpoint, &compact, fork, &self.spec, self.config.adoption_parts)
            .expect("the bootstrap verifies");
        let mut parts = self.parts(&bootstrap.current_sync_committee);
        let rest = parts.split_off(parts_now.min(parts.len()));
        let slot = bootstrap.header.beacon.slot;
        let record = rest.is_empty().then(|| self.claim(job.pending.record()));
        let mut accounts = vec![self.account()];
        if record.is_some() {
            accounts.push(self.header_account(slot));
        }
        let instruction = Instruction::Init {
            config: self.config.clone(),
            checkpoint: self.checkpoint,
            bootstrap: compact.encode(fork),
            adopt: parts,
            record,
            window: Window { from_ms: None, to_ms: self.config.ws_end_ms(slot) },
        };
        // Valid from genesis to the end of the checkpoint's weak-subjectivity period: sent a
        // slot after the checkpoint.
        let timestamp_ms = self.config.slot_start_ms(slot + 1);
        (Planned { label: "Init".into(), instruction, accounts, timestamp_ms }, rest)
    }

    /// `Continue` with `steps`, claiming `record` if the job completes and writes one.
    pub fn continue_with(&self, label: &str, steps: Vec<Step>, record: Option<&HeaderRecord>, after_ms: u64) -> Planned {
        let mut accounts = vec![self.account()];
        if let Some(r) = record {
            accounts.push(self.header_account(r.beacon.slot));
        }
        Planned {
            label: label.into(),
            instruction: Instruction::Continue {
                instance_id: self.id,
                steps,
                record: record.map(|r| self.claim(r)),
            },
            accounts,
            timestamp_ms: after_ms,
        }
    }

    /// The layout a relayer sends `update` in: the fork the schedule puts at its attested header
    /// (as a beacon node serves it), or the store's own if later.
    pub fn layout_for(&self, store_fork: LcFork, update: &LightClientUpdate) -> LcFork {
        let slot = update.attested_header.beacon.slot;
        let served = self.spec.fork_at_epoch(self.spec.epoch_at_slot(slot)).lc_fork().expect("a light-client fork");
        served.max(store_fork)
    }

    /// The committee that signed `update`, in the native store `full`.
    pub fn signing_committee<'a>(&self, full: &'a StoreCore, update: &LightClientUpdate) -> &'a SyncCommittee {
        let store_period = self.spec.period_at_slot(full.finalized_header.beacon.slot);
        if self.spec.period_at_slot(update.signature_slot) == store_period {
            &full.current_sync_committee
        } else {
            &full.next_sync_committee
        }
    }

    /// The signer witness bytes of `update` against the native store.
    pub fn signer_bytes(&self, update: &LightClientUpdate, signers: Signers) -> Vec<u8> {
        let committee = self.signing_committee(self.native.core(), update);
        let bits = &update.sync_aggregate.bits;
        match signers {
            Signers::Core => SignerWitness::build(committee, bits).unwrap_or_default().encode(),
            Signers::Multiproof => {
                let mut s = vec![0xff, 0xff];
                s.extend_from_slice(&residue_agg::NonSigners::build(committee, bits).unwrap().encode());
                s
            }
        }
    }

    /// The job `update` opens on the instance's current store (natively, as the program runs
    /// step 1), and the store it starts from.
    pub fn native_job(&self, update: &LightClientUpdate, layout: LcFork) -> Option<(Job, CompactStore)> {
        let stored = CompactStore::of(self.native.core());
        let store = steps::store_for_update(&stored, layout, update.attested_header.beacon.slot, &self.spec).ok()?;
        let compact = CompactUpdate::of(update);
        let committee = self.signing_committee(self.native.core(), update);
        let witness = SignerWitness::build(committee, &update.sync_aggregate.bits).ok()?;
        let job = steps::step1(
            &store,
            &compact,
            &witness,
            update.signature_slot,
            &self.spec,
            &self.config.genesis_validators_root,
            self.config.adoption_parts,
        )
        .ok()?;
        Some((job, store))
    }

    /// The honest residue witness (c, w) of `job` after its Miller loops (EXPERIMENTAL).
    pub fn residue_witness(job: &Job) -> (Vec<u8>, Vec<u8>) {
        let mut j = job.clone();
        if matches!(j.phase, Phase::Miller { .. }) {
            j.step2().unwrap();
        }
        let Phase::FinalExp { product } = &j.phase else { panic!("a job after its Miller loops") };
        let f = residue::decode_fp12(product).unwrap();
        let (c, w) = residue::witness(&f);
        assert_eq!(residue::check(&f, &c, &w), Ok(()));
        (residue::encode_fp12(&c).to_vec(), residue::encode_fp12(&w).to_vec())
    }

    /// The record the job of `update` writes when it completes, if it advances the store.
    fn expected_record(&self, job: &Job, store: &CompactStore) -> (Option<HeaderRecord>, ApplyOutcome) {
        let mut done = job.clone();
        done.phase = Phase::Verified;
        if let Some(a) = done.adoption.as_mut() {
            a.complete = true;
        }
        let (_, outcome) = steps::apply(store, &done, &self.spec).expect("the job applies");
        (outcome.advanced.then(|| done.pending.record().clone()), outcome)
    }

    /// An `Update` of `update`: `verify` as given (`Residue` gets the honest witness), `adopt_now`
    /// adoption parts in the same transaction. Returns the transaction, the parts left (a
    /// `Continue` adopts them) and the record the job writes when it completes.
    pub fn update(
        &self,
        label: &str,
        update: &LightClientUpdate,
        verify: Verify,
        signers: Signers,
        adopt_now: bool,
    ) -> (Planned, Vec<Part>, Option<HeaderRecord>) {
        let layout = self.layout_for(self.native.core().fork, update);
        let (job, store) = self.native_job(update, layout).expect("the update opens a job natively");
        let summary = matches!(verify, Verify::ResidueSummary { .. });
        let verify = match verify {
            Verify::Residue { .. } => {
                let (c, w) = Self::residue_witness(&job);
                Verify::Residue { c, w }
            }
            // EXPERIMENTAL (bench/RESIDUE.md §10): a handoff's committee adopted by its summary in the
            // same transaction, so no adoption parts.
            Verify::ResidueSummary { .. } => {
                let (c, w) = Self::residue_witness(&job);
                Verify::ResidueSummary { c, w }
            }
            v => v,
        };
        let parts = match (&job.adoption, update.next_sync_committee.pubkeys.iter().any(|k| *k != [0u8; 48])) {
            (Some(_), true) if !summary => self.parts(&update.next_sync_committee),
            _ => vec![],
        };
        let (adopt, rest) = if adopt_now { (parts, vec![]) } else { (vec![], parts) };
        let (record, _) = self.expected_record(&job, &store);
        let completes =
            matches!(verify, Verify::Core | Verify::Residue { .. } | Verify::ResidueSummary { .. }) && rest.is_empty();
        let claim = if completes { record.as_ref().map(|r| self.claim(r)) } else { None };
        let mut accounts = vec![self.account()];
        if let Some(c) = &claim {
            accounts.push(self.header_account(HeaderRecord::decode(&c.record).unwrap().beacon.slot));
        }
        let stored = CompactStore::of(self.native.core());
        let from_ms = Some(self.config.slot_start_ms(update.signature_slot));
        let to_ms = self.config.ws_end_slot(stored.finalized.slot).map(|end| self.config.slot_start_ms(end));
        let instruction = Instruction::Update {
            instance_id: self.id,
            update: CompactUpdate::of(update).encode(layout),
            signers: self.signer_bytes(update, signers),
            verify,
            adopt,
            record: claim,
            window: Window { from_ms, to_ms },
        };
        let timestamp_ms = self.config.slot_start_ms(update.signature_slot) + 12_000;
        (Planned { label: label.into(), instruction, accounts, timestamp_ms }, rest, record)
    }

    /// Bring the native light client to `update` (as the program did), upgrading its layout first.
    pub fn follow(&mut self, update: &LightClientUpdate) -> ApplyOutcome {
        let layout = self.layout_for(self.native.core().fork, update);
        if layout > self.native.core().fork {
            self.native.upgrade(layout, &self.spec).expect("a store upgrades forwards");
        }
        let ctx = Context { spec: &self.spec, genesis_validators_root: self.config.genesis_validators_root, bls: &BLS };
        self.native.process_update(update, update.signature_slot + 1, &ctx).expect("the native store accepts")
    }

    /// The statements a query proves under the record at `slot` (natively), as `Attest` claims them.
    pub fn statements(&self, record: &RecordState, query: &StorageQuery) -> Vec<Statement> {
        let recorded = HeaderRecord::decode(&record.record).unwrap();
        let rec = match &query.header {
            Some(h) => proven::HeaderRecord::from_recorded_block_hash(&recorded, record.chain_id, h),
            None => proven::HeaderRecord::from_recorded(&recorded, record.chain_id),
        }
        .expect("a record of this chain");
        query
            .slots
            .iter()
            .map(|s| {
                let a = proven::attest_storage(
                    &rec,
                    record.chain_id,
                    &query.address,
                    query.account_proof.as_slice(),
                    &s.slot,
                    s.proof.as_slice(),
                )
                .expect("the proofs verify natively");
                Statement {
                    instance_id: record.instance_id,
                    chain_id: a.chain_id,
                    record_slot: recorded.beacon.slot,
                    block_number: a.block_number,
                    block_hash: a.block_hash,
                    address: a.fact.address,
                    slot: a.fact.slot,
                    value: a.fact.value,
                    id: a.id,
                    id_at_block: a.id_at_block(),
                }
            })
            .collect()
    }

    /// `Attest` of `query` under `header(id, slot)`, claiming `statements`.
    pub fn attest(&self, label: &str, slot: u64, query: StorageQuery, statements: Vec<Statement>, at_ms: u64) -> Planned {
        let mut accounts = vec![self.header_account(slot)];
        accounts.extend(statements.iter().map(|s| statement_account(&self.program, &self.id, &s.id_at_block)));
        Planned {
            label: label.into(),
            instruction: Instruction::Attest { instance_id: self.id, slot, query, statements },
            accounts,
            timestamp_ms: at_ms,
        }
    }

    pub fn statement(&self, chain: &Chain, s: &Statement) -> Option<Statement> {
        let shard = chain.shard(statement_account(&self.program, &self.id, &s.id_at_block), self.program);
        (!shard.is_empty()).then(|| borsh::from_slice(&shard).expect("statement decodes"))
    }

    /// Send `p` on `chain`, paid by the chain's payer, at the gas limit `gas_limit`.
    pub fn send(&self, chain: &mut Chain, p: &Planned, gas_limit: u64) -> TxReport {
        let data = borsh::to_vec(&p.instruction).unwrap();
        chain.submit(&p.label, self.program, &p.accounts, data, &[], gas_limit, p.timestamp_ms)
    }

    /// The instance on `chain` holds the compact form of the native store, and no job.
    pub fn assert_in_step(&self, chain: &Chain) {
        let state = self.state(chain).expect("an instance");
        assert!(state.active, "the instance is active");
        assert_eq!(state.job, None, "no job in progress");
        assert_eq!(CompactStore::decode(&state.store).unwrap(), CompactStore::of(self.native.core()), "the store");
        let slot = self.native.core().finalized_header.beacon.slot;
        let record = self.record(chain, slot).expect("the finalized header's record");
        let expected = HeaderRecord::of(&self.native.core().finalized_header, self.native.core().fork, &self.spec);
        assert_eq!(record.record, expected.encode(), "the record of the finalized header");
        assert_eq!(record.instance_id, self.id);
        assert_eq!(record.chain_id, self.config.chain_id);
    }
}

/// Write a table of `reports` (TSV) to `lez-v03/results/<name>.tsv`.
pub fn write_results(name: &str, reports: &[TxReport]) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../results");
    std::fs::create_dir_all(&dir).unwrap();
    let mut out = String::from(TxReport::tsv_header());
    out.push('\n');
    for r in reports {
        out.push_str(&r.tsv());
        out.push('\n');
    }
    std::fs::write(dir.join(format!("{name}.tsv")), out).unwrap();
}

// ---------------------------------------------------------------------------------------------
// Driven by state: what a run sends next, whatever third parties did before it
// ---------------------------------------------------------------------------------------------
//
// Anyone may send the program's transactions (`Init`, `Update`, `Continue`, `Attest`) and
// anyone may write an empty account's loader shard (`WriteSegment` needs no signature). None of it
// can make the program accept what the core refuses (soundness is the program's); it can only make
// a pre-signed sequence fail (liveness). So a run reads the chain before it signs, and signs what
// is missing from the state it finds:
// - the program: deployed only if the header and segment accounts are free or hold exactly ours;
//   otherwise the run chooses fresh accounts ([`deployment`]);
// - an instance created by someone else without its committee: adopt it (`Continue`);
// - a verified handoff left without its adoption (it blocks every update, 207): adopt it;
// - updates someone else already applied: skipped; the native light client is rebuilt from the
//   store the chain holds ([`Instance::sync_to_state`]), so later updates are built on it.

/// Read access to the accounts' shards (the in-process chain, or a snapshot of a sequencer's).
pub trait StateView {
    fn shard_of(&self, account: AccountId, program: AccountId) -> Vec<u8>;
}

impl StateView for Chain {
    fn shard_of(&self, account: AccountId, program: AccountId) -> Vec<u8> {
        self.shard(account, program)
    }
}

/// A snapshot of shards read over RPC.
#[derive(Clone, Debug, Default)]
pub struct MapView(pub std::collections::HashMap<(AccountId, AccountId), Vec<u8>>);

impl StateView for MapView {
    fn shard_of(&self, account: AccountId, program: AccountId) -> Vec<u8> {
        self.0.get(&(account, program)).cloned().unwrap_or_default()
    }
}

/// The deployment's accounts as the chain holds them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Deployment {
    /// The header holds this program (`FERRATA_V03_ID`): nothing to deploy.
    Ours,
    /// The header is free; `written[i]` if segment `i` already holds exactly our chunk.
    Free { written: Vec<bool> },
    /// Someone else wrote the header or a segment: choose fresh accounts.
    Taken(String),
}

/// The segments' expected loader shards for `header`'s chain of `segments`.
pub fn segment_shards(segments: &[AccountId]) -> Vec<Vec<u8>> {
    let user_elf = risc0_binfmt::ProgramBinary::decode(FERRATA_V03_ELF).expect("a RISC Zero program binary").user_elf.to_vec();
    user_elf
        .chunks(program_loader_core::MAX_SEGMENT_DATA_LEN)
        .enumerate()
        .map(|(i, c)| {
            lee_core::program::ProgramSegment { bytecode: c.to_vec(), next_segment: segments.get(i + 1).copied() }.to_bytes()
        })
        .collect()
}

pub fn deployment(view: &dyn StateView, header: AccountId, segments: &[AccountId]) -> Deployment {
    let h = view.shard_of(header, PROGRAM_LOADER_ACCOUNT_ID);
    if !h.is_empty() {
        return match lee_core::program::ProgramHeader::from_bytes(&h) {
            Some(p) if p.image_id == FERRATA_V03_ID => Deployment::Ours,
            Some(p) => Deployment::Taken(format!("the header account {header} holds image {}", image_id_hex(p.image_id))),
            None => Deployment::Taken(format!("the header account {header} holds other loader data")),
        };
    }
    let mut written = vec![];
    for (i, (seg, want)) in segments.iter().zip(segment_shards(segments)).enumerate() {
        let got = view.shard_of(*seg, PROGRAM_LOADER_ACCOUNT_ID);
        if got.is_empty() {
            written.push(false);
        } else if got == want {
            written.push(true);
        } else {
            return Deployment::Taken(format!("segment account {i} ({seg}) was written by someone else"));
        }
    }
    Deployment::Free { written }
}

/// One step a run sends, with the kind its verdict is checked by (`init`, `adopt-init`,
/// `adopt-pending`, `update:<name>`, `continue:<name>`).
#[derive(Clone, Debug)]
pub struct Planned2 {
    pub kind: String,
    pub planned: Planned,
}

impl Instance {
    /// Make the native light client hold exactly `store` (the chain's), its committees taken
    /// from `known` by their summaries. The finalized header is the beacon header alone: records
    /// are always built from the update that finalizes a header, never from the store.
    pub fn sync_to_state(&mut self, store: &CompactStore, known: &[SyncCommittee]) -> Result<(), String> {
        let find = |s: &ferrata_core::witness::CommitteeSummary| {
            known.iter().find(|c| ferrata_core::witness::CommitteeSummary::of(c) == *s).cloned()
        };
        let current = find(&store.current).ok_or("the store's current committee is not among the known ones")?;
        let next = match &store.next {
            Some(n) => find(n).ok_or("the store's next committee is not among the known ones")?,
            None => SyncCommittee::empty(self.config.sync_committee_size as usize),
        };
        let mut header = LightClientHeader::empty(store.fork);
        header.beacon = store.finalized.clone();
        let core = StoreCore { fork: store.fork, finalized_header: header, current_sync_committee: current, next_sync_committee: next };
        self.native = FinalizedStore::from_core(core, &BLS).map_err(|e| format!("{e:?}"))?;
        if CompactStore::of(self.native.core()) != *store {
            return Err("the rebuilt store differs from the chain's".into());
        }
        Ok(())
    }

    /// The parts of `committee` from part `from` on.
    fn parts_from(&self, committee: &SyncCommittee, from: u8) -> Vec<Part> {
        self.parts(committee).into_iter().filter(|p| p.index >= from).collect()
    }

    /// Everything to send next, read from `view`: `Init` (or the adoption a third party's
    /// `Init` left undone), the adoption of a verified handoff left undone, then every update of
    /// `updates` newer than the store (in order; one or two transactions each). The native light
    /// client ends where the chain will be once these apply. `notes` says what was found.
    pub fn plan_from_state(
        &mut self,
        view: &dyn StateView,
        bootstrap: &LightClientBootstrap,
        updates: &[(String, LightClientUpdate)],
        known: &[SyncCommittee],
        notes: &mut Vec<String>,
    ) -> Result<Vec<Planned2>, String> {
        let mut out = vec![];
        let shard = view.shard_of(self.account(), self.program);
        let after = |t: u64| t + 12_000;
        if shard.is_empty() {
            let (p, rest) = self.init(bootstrap, usize::MAX);
            assert!(rest.is_empty());
            out.push(Planned2 { kind: "init".into(), planned: p });
        } else {
            let state: InstanceState = borsh::from_slice(&shard).map_err(|e| format!("instance state: {e}"))?;
            let job = state.job.as_ref().map(|j| Job::decode(j)).transpose().map_err(|e| format!("job: {e:?}"))?;
            if !state.active {
                // Someone created the instance without adopting its committee: adopt it.
                let job = job.ok_or("an inactive instance without its job")?;
                let from = job.adoption.as_ref().map(|a| a.next_part()).unwrap_or(0);
                let parts = self.parts_from(&bootstrap.current_sync_committee, from);
                notes.push(format!("the instance exists without its committee adopted: Continue with parts {from}.. of it"));
                let steps = parts.into_iter().map(Step::Adopt).collect();
                let c = self.continue_with("adopt the bootstrap committee (Continue)", steps, Some(job.pending.record()), after(self.config.slot_start_ms(bootstrap.header.beacon.slot)));
                out.push(Planned2 { kind: "adopt-init".into(), planned: c });
            } else {
                let store = CompactStore::decode(&state.store).map_err(|e| format!("store: {e:?}"))?;
                self.sync_to_state(&store, known)?;
                if let Some(job) = job.filter(|j| j.phase == Phase::Verified && j.adoption_pending()) {
                    // A verified handoff someone left without its adoption blocks every update (207).
                    let a = job.adoption.as_ref().unwrap();
                    let committee = known
                        .iter()
                        .find(|c| ferrata_core::witness::CommitteeSummary::of(c) == a.committee)
                        .ok_or("the pending job's committee is not among the known ones: fetch its period's update")?
                        .clone();
                    let mut done = job.clone();
                    done.adoption.as_mut().unwrap().complete = true;
                    let (next, outcome) = steps::apply(&store, &done, &self.spec).map_err(|e| format!("{e:?}"))?;
                    let record = outcome.advanced.then(|| job.pending.record().clone());
                    notes.push(format!("a verified handoff waits for its adoption: Continue with parts {}..", a.next_part()));
                    let steps = self.parts_from(&committee, a.next_part()).into_iter().map(Step::Adopt).collect();
                    let c = self.continue_with("adopt the pending handoff's committee (Continue)", steps, record.as_ref(), 0);
                    out.push(Planned2 { kind: "adopt-pending".into(), planned: c });
                    self.sync_to_state(&next, known)?;
                }
            }
        }
        for (name, u) in updates {
            let finalized = self.native.core().finalized_header.beacon.slot;
            if u.finalized_header.beacon.slot <= finalized && !(u.next_sync_committee.pubkeys.iter().any(|k| *k != [0u8; 48]) && !self.native.core().is_next_sync_committee_known()) {
                notes.push(format!("{name}: the instance is already at slot {finalized} or later: skipped"));
                continue;
            }
            let layout = self.layout_for(self.native.core().fork, u);
            if self.native_job(u, layout).is_none() {
                notes.push(format!("{name}: refused by the native light client on the chain's store: skipped"));
                continue;
            }
            let (p, rest, record) = self.update(&format!("update {name} (residue witness{})", ""), u, Verify::Residue { c: vec![], w: vec![] }, Signers::Core, false);
            let one = rest.is_empty();
            let mut p = p;
            if one {
                p.label = format!("update {name} (residue witness, one transaction)");
            }
            let t = p.timestamp_ms;
            out.push(Planned2 { kind: format!("update:{name}"), planned: p });
            if !one {
                let c = self.continue_with(&format!("update {name}: Continue (adopt 512 keys, apply)"), rest.into_iter().map(Step::Adopt).collect(), record.as_ref(), t + 12_000);
                out.push(Planned2 { kind: format!("continue:{name}"), planned: c });
            }
            self.follow(u);
        }
        Ok(out)
    }
}
