//! Stage 5: Sepolia through its Gloas fork, through the program.
//!
//! Sepolia activated Gloas at epoch 353,024, the first slot of sync-committee period 1379. No
//! public node serves a bootstrap for a Gloas-era checkpoint (the fixture's SOURCE.md records
//! the refusals), so an instance starts from the bootstrap of the last period before the fork,
//! in the Electra layout, with Sepolia's fork schedule in its configuration, and follows the
//! chain: the update of period 1378, then the first update attested under Gloas, with which the
//! instance's store upgrades itself to the Gloas layout, then every period's update to the newest
//! one. From the fork on, a record binds only the hash of an execution block (the bid's parent
//! block); `Attest` then takes that block's RLP header as a witness.
//!
//! The data is `crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10` (fetched by
//! `scripts/fetch-sepolia-gloas.py`), the fixture of the core's `sepolia_gloas` and
//! `sepolia_steps` tests.

use std::{fs, path::PathBuf};

use ferrata_core::{
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    merkle::ZERO,
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    Root,
};
use ferrata_lez_wire::Config;
use serde_json::Value;
use sha2::{Digest, Sha256};

use ferrata_core::rlp::encode_u64;
use ferrata_lez_wire::{
    attest::{evaluate, Address, Attestation, Cmp, Field, LogQuery, Request, Word},
    header_actor, Message, ATTESTED_SELECTOR,
};
use ferrata_spectests::execkit::{account_proof, hexb, receipt_rlp, word, AccountProof, Trie};
use lee::Actor;

use crate::{
    attest::{
        account_query, account_statement_id, condition, log_statement_id, storage_statement_id, transfer_conditions,
        Transfer,
    },
    root,
    scenarios::World,
    TxReport,
};

/// Sepolia's execution chain id (EIP-155).
pub const SEPOLIA_CHAIN_ID: u64 = 11_155_111;
/// Sepolia `GENESIS_TIME` (the fixture's block 12,103,422 has slot number 11,547,103 and
/// timestamp 1,794,298,836 = this + 12 × the slot).
pub const SEPOLIA_GENESIS_TIME: u64 = 1_655_733_600;
pub const SEPOLIA_GENESIS_VALIDATORS_ROOT: &str = "d8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078";
/// The checkpoint `Init` trusts: the first block of period 1378, the last period before Gloas.
pub const SEPOLIA_CHECKPOINT: &str = "5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d";
/// The bootstrap's period, the last one in the Electra layout.
pub const FIRST_PERIOD: u64 = 1378;
/// The first period under Gloas: its first slot is the fork's.
pub const GLOAS_PERIOD: u64 = 1379;
/// The newest period of the fixture.
pub const LAST_PERIOD: u64 = 1409;
/// The slot the fixture's finality update finalizes, and the execution block its record binds.
pub const FINALIZED_SLOT: u64 = 11_547_104;
pub const BOUND_BLOCK: u64 = 12_103_422;
pub const BOUND_BLOCK_HASH: &str = "cd9fa3661248bb2411636d3498a1dda97f0ee50d02b8ac894969e391f2978435";

/// The fixture's directory name under `crates/ferrata-spectests/fixtures`.
pub const GLOAS_FIXTURE: &str = "sepolia-gloas-2026-11-10";

/// The fixture's pinned `SHA256SUMS`, compiled in.
const GLOAS_SUMS: &str = include_str!("../../../crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10/SHA256SUMS");

/// Sepolia's fork schedule (eth-clients/sepolia `metadata/config.yaml` at 7b1b9d9): Gloas at
/// epoch 353,024.
pub fn sepolia_spec() -> ChainSpec {
    let v = |b: u8| [0x90, 0x00, 0x00, b];
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [v(0x69), v(0x70), v(0x71), v(0x72), v(0x73), v(0x74), v(0x75), v(0x76), [0; 4]],
        epochs: [0, 50, 100, 56_832, 132_608, 222_464, 272_640, 353_024, FAR_FUTURE_EPOCH],
    }
}

/// An instance config for Sepolia: the store starts in the Electra layout (the bootstrap is of
/// the Fulu era), the schedule has Gloas, adoption in `parts` transactions.
pub fn sepolia_config(parts: u8) -> Config {
    let spec = sepolia_spec();
    Config {
        chain_id: SEPOLIA_CHAIN_ID,
        sync_committee_size: 512,
        slots_per_epoch: spec.preset.slots_per_epoch,
        epochs_per_sync_committee_period: spec.preset.epochs_per_sync_committee_period,
        min_sync_committee_participants: spec.preset.min_sync_committee_participants,
        fork_versions: spec.versions,
        fork_epochs: spec.epochs,
        genesis_validators_root: root(SEPOLIA_GENESIS_VALIDATORS_ROOT),
        genesis_time: SEPOLIA_GENESIS_TIME,
        seconds_per_slot: 12,
        store_fork: 3,
        adoption_parts: parts,
        ws_period_epochs: crate::test_ws_period_epochs(),
    }
}

/// `FERRATA_SEPOLIA_FIXTURE_DIR` if set (to run a built binary elsewhere), else the fixture of
/// this checkout.
pub fn gloas_fixture_dir() -> PathBuf {
    std::env::var_os("FERRATA_SEPOLIA_FIXTURE_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/ferrata-spectests/fixtures").join(GLOAS_FIXTURE)
    })
}

/// A file of `dir`, checked against the pinned sums `sums` (the text of a `SHA256SUMS`).
pub(crate) fn pinned(dir: PathBuf, sums: &str, name: &str) -> Vec<u8> {
    let path = dir.join(name);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let want = sums
        .lines()
        .find(|l| l.ends_with(&format!("  {name}")))
        .unwrap_or_else(|| panic!("{name} is not in SHA256SUMS"))
        .split_whitespace()
        .next()
        .unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{name} does not match SHA256SUMS");
    bytes
}

/// A file of the Gloas fixture, checked against its pinned `SHA256SUMS`.
pub fn gloas_file(name: &str) -> Vec<u8> {
    pinned(gloas_fixture_dir(), GLOAS_SUMS, name)
}

/// The layout a beacon node serves a period's update in: Electra before the fork, Gloas from it.
pub fn served_layout(period: u64) -> LcFork {
    if period < GLOAS_PERIOD {
        LcFork::Electra
    } else {
        LcFork::Gloas
    }
}

pub fn sepolia_bootstrap() -> LightClientBootstrap {
    LightClientBootstrap::decode(&gloas_file("bootstrap.ssz"), LcFork::Electra, 512).unwrap()
}

/// The update of `period`, as served.
pub fn sepolia_update(period: u64) -> LightClientUpdate {
    LightClientUpdate::decode(&gloas_file(&format!("update_{period}.ssz")), served_layout(period), 512).unwrap()
}

/// A Gloas `LightClientFinalityUpdate` (every field is fixed-size) as the `LightClientUpdate` it
/// stands for.
pub fn gloas_finality_update(b: &[u8]) -> LightClientUpdate {
    const H: usize = 112 + 32 + 11 * 32;
    assert_eq!(b.len(), 2 * H + 9 * 32 + 160 + 8, "a Gloas LightClientFinalityUpdate");
    LightClientUpdate {
        attested_header: LightClientHeader::decode(&b[..H], LcFork::Gloas).unwrap(),
        next_sync_committee: SyncCommittee::empty(512),
        next_sync_committee_branch: vec![ZERO; LcFork::Gloas.sync_committee_branch_len()],
        finalized_header: LightClientHeader::decode(&b[H..2 * H], LcFork::Gloas).unwrap(),
        finality_branch: b[2 * H..2 * H + 288].as_chunks::<32>().0.to_vec(),
        sync_aggregate: SyncAggregate::decode(&b[2 * H + 288..2 * H + 448], 512).unwrap(),
        signature_slot: u64::from_le_bytes(b[2 * H + 448..].try_into().unwrap()),
    }
}

/// The fixture's finality update, finalizing slot [`FINALIZED_SLOT`].
pub fn sepolia_finality_update() -> LightClientUpdate {
    gloas_finality_update(&gloas_file("finality_update.ssz"))
}

/// The RLP header of the execution block the fixture's finalized header binds.
pub fn bound_header() -> Vec<u8> {
    gloas_file("header.rlp")
}

/// The canonical block root a second beacon provider reports for `slot`.
pub fn canonical_root(slot: u64) -> Root {
    let v: Value = serde_json::from_slice(&gloas_file(&format!("beacon_header_{slot}_second.json"))).unwrap();
    root(v["data"]["root"].as_str().unwrap().trim_start_matches("0x"))
}

/// A Sepolia instance on a new chain: `Init` from the pre-Gloas bootstrap and the adoption of
/// its committee, in `parts` transactions each.
pub fn sepolia_world(config: Config) -> World {
    World::from_parts(config, root(SEPOLIA_CHECKPOINT), sepolia_bootstrap(), vec![])
}

/// `Init`, then every period's update from 1378 to `last`, each as a public job. After each the
/// instance must hold the compact form of the native store (which [`World::apply`] upgrades by
/// hand at the fork) and the record of the native finalized header, whose root must be the one
/// a second beacon provider reports.
pub fn replay(parts: u8, last: u64) -> World {
    let mut world = sepolia_world(sepolia_config(parts));
    for period in FIRST_PERIOD..=last {
        let update = sepolia_update(period);
        let run = world.apply(&update, &format!("period {period}"));
        assert!(run.reports.iter().all(crate::TxReport::ok), "period {period}");
        let store = world.inst.store(&world.chain);
        assert_eq!(store.fork, served_layout(period), "period {period}: the store's layout");
        assert_eq!(store.finalized.slot, update.finalized_header.beacon.slot, "period {period}");
        assert_eq!(store.finalized.hash_tree_root(), canonical_root(store.finalized.slot), "period {period}");
    }
    world
}

// ---------------------------------------------------------------------------------------------
// Attestation under a Gloas record: a finalized Sepolia block, and facts under it
// ---------------------------------------------------------------------------------------------

/// The fixture of `lez/scripts/fetch-attest-fixture.py --network sepolia`: a finality update of
/// period 1409, the block its finalized header binds, and proofs under that block.
pub const ATTEST_FIXTURE: &str = "sepolia-attest-2026-11-10";
const ATTEST_SUMS: &str =
    include_str!("../../../crates/ferrata-spectests/fixtures/sepolia-attest-2026-11-10/SHA256SUMS");

/// `FERRATA_SEPOLIA_ATTEST_DIR` if set, else the fixture of this checkout.
pub fn attest_fixture_dir() -> PathBuf {
    std::env::var_os("FERRATA_SEPOLIA_ATTEST_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/ferrata-spectests/fixtures").join(ATTEST_FIXTURE)
    })
}

/// A file of the attestation fixture, checked against its pinned `SHA256SUMS`.
pub fn attest_file(name: &str) -> Vec<u8> {
    pinned(attest_fixture_dir(), ATTEST_SUMS, name)
}

fn hex0x(b: &[u8]) -> String {
    format!("0x{}", hex::encode(b))
}

/// The facts of the attestation fixture.
pub struct SepoliaFacts {
    /// The finality update's sync-committee period.
    pub period: u64,
    pub update: LightClientUpdate,
    pub finalized_slot: u64,
    /// The execution block the finalized header binds, its hash, and its RLP header.
    pub block: u64,
    pub block_hash: Root,
    pub header: Vec<u8>,
    /// A storage slot of a live contract (Sepolia USDC's `totalSupply_`) and its value at `block`.
    pub contract: Address,
    pub slot: Word,
    pub value: Word,
    /// An account at the same address and with the same code hash on mainnet (Multicall3).
    pub same_address: Address,
    pub mainnet_code_hash: Root,
    /// The first ERC-20 `Transfer` of the block.
    pub transfer: Transfer,
    pub receipts: Vec<Value>,
    receipts_trie: Trie,
}

impl SepoliaFacts {
    pub fn load() -> Self {
        let m: Value = serde_json::from_slice(&attest_file("fixture.json")).unwrap();
        assert_eq!(m["chain_id"].as_u64(), Some(SEPOLIA_CHAIN_ID));
        let block = m["block"].as_u64().unwrap();
        let served: Value = serde_json::from_slice(&attest_file(&format!("receipts_{block}.json"))).unwrap();
        let receipts = served["result"].as_array().expect("receipts").clone();
        let receipts_trie = Trie::new(receipts.iter().enumerate().map(|(i, r)| {
            let (key, len) = encode_u64(i as u64);
            (key[..len].to_vec(), receipt_rlp(r))
        }));
        let storage = &m["storage"][0];
        let row = &storage["slots"][0];
        let same = &m["same_address"];
        Self {
            period: m["period"].as_u64().unwrap(),
            update: gloas_finality_update(&attest_file("finality_update.ssz")),
            finalized_slot: m["finalized_slot"].as_u64().unwrap(),
            block,
            block_hash: word(m["block_hash"].as_str().unwrap()),
            header: attest_file("header.rlp"),
            contract: hexb(storage["address"].as_str().unwrap()).try_into().unwrap(),
            slot: word(row["slot"].as_str().unwrap()),
            value: word(row["value"].as_str().unwrap()),
            same_address: hexb(same["address"].as_str().unwrap()).try_into().unwrap(),
            mainnet_code_hash: word(same["mainnet_code_hash"].as_str().unwrap()),
            transfer: Transfer::read(&m["transfer"]),
            receipts,
            receipts_trie,
        }
    }

    /// The root of the receipts trie rebuilt from `eth_getBlockReceipts`.
    pub fn receipts_root(&self) -> Root {
        self.receipts_trie.root()
    }

    /// The proof of transaction `tx_index`'s receipt.
    pub fn receipt_proof(&self, tx_index: u64) -> Vec<Vec<u8>> {
        let (key, len) = encode_u64(tx_index);
        self.receipts_trie.proof(&key[..len])
    }

    /// `eth_getProof` for the storage contract and its slot.
    pub fn storage_proof(&self) -> AccountProof {
        account_proof(&attest_file(&format!("proof_{}_{}.json", hex0x(&self.contract), self.block)))
    }

    /// `eth_getProof` for the account at the same address on both chains.
    pub fn same_address_proof(&self) -> AccountProof {
        account_proof(&attest_file(&format!("proof_{}_{}.json", hex0x(&self.same_address), self.block)))
    }

    /// The JSON log at position `log_index` of transaction `tx_index`'s receipt.
    pub fn json_log(&self, tx_index: u64, log_index: u64) -> &Value {
        &self.receipts[tx_index as usize]["logs"][log_index as usize]
    }
}

/// A Sepolia instance that followed the chain through the Gloas fork ([`replay`]) and then took
/// the attestation fixture's finality update: `header(id, finalized slot)` holds a record in the
/// Gloas layout, which binds the block hash only.
pub struct Stage5 {
    pub w: World,
    pub fx: SepoliaFacts,
    /// A block time after the finality update's signature slot.
    pub time: u64,
}

impl Default for Stage5 {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage5 {
    pub fn new() -> Self {
        let fx = SepoliaFacts::load();
        assert!(
            fx.period <= LAST_PERIOD + 1,
            "the replay's store knows the committees up to period {}",
            LAST_PERIOD + 1
        );
        let mut w = replay(2, LAST_PERIOD);
        let update = fx.update.clone();
        let run = w.apply(&update, "finality update (attestation fixture)");
        assert_eq!(run.outcome.map(|o| o.advanced), Some(true), "the update finalizes a new header");
        assert_eq!(w.inst.store(&w.chain).finalized.slot, fx.finalized_slot);
        let time = w.inst.config.slot_start_ms(update.signature_slot) + 12_000;
        Self { w, fx, time }
    }

    /// `header(id, slot)` of the instance.
    pub fn header(&self, slot: u64) -> Actor {
        header_actor(self.w.inst.program, &self.w.inst.id, slot)
    }

    /// A request bound to the instance and to Sepolia, with the bound block's RLP header.
    pub fn request(&self) -> Request {
        Request {
            instance_id: self.w.inst.id,
            chain_id: SEPOLIA_CHAIN_ID,
            header: Some(self.fx.header.clone()),
            ..Request::default()
        }
    }

    /// What the program must answer: its handler's function, run on the record the chain holds.
    pub fn expected(&self, slot: u64, request: &Request) -> Result<Attestation, u8> {
        let state = self.w.inst.record(&self.w.chain, slot).expect("a record at this slot");
        evaluate(&state.instance_id, state.chain_id, &state.header().expect("a record"), request)
    }

    /// `Attest` at `header(id, slot)` as the root of a transaction.
    pub fn attest(&mut self, label: &str, slot: u64, request: &Request) -> TxReport {
        let actor = self.header(slot);
        let message = borsh::to_vec(&Message::Attest { request: request.clone() }).unwrap();
        self.w.chain.submit(label, actor, message, vec![actor], vec![], self.time)
    }

    /// An accepted root attestation whose event names the natively evaluated identifiers.
    pub fn accept(&mut self, label: &str, slot: u64, request: &Request) -> (TxReport, Attestation) {
        let expected = self.expected(slot, request).unwrap_or_else(|code| panic!("{label}: refused natively ({code})"));
        let report = self.attest(label, slot, request);
        assert!(report.ok(), "{label}: {:?} (exit {:?})", report.charge.revert, report.exit_code);
        let event = report
            .events
            .iter()
            .find(|e| e.event.selector == ATTESTED_SELECTOR)
            .unwrap_or_else(|| panic!("{label}: no attestation event"));
        assert_eq!(event.event.data, expected.event_data(), "{label}: the event names the expected identifiers");
        (report, expected)
    }

    /// A refused root attestation: the transaction reverts with `code`, as natively.
    pub fn refuse(&mut self, label: &str, slot: u64, request: &Request, code: u8) -> TxReport {
        assert_eq!(self.expected(slot, request).err(), Some(code), "{label}: the native evaluation");
        let report = self.attest(label, slot, request);
        assert_eq!(report.exit_code, Some(u32::from(code)), "{label}: {:?}", report.charge.revert);
        assert!(report.events.is_empty(), "{label}: a refusal emits nothing");
        report
    }
}

/// Stage 5's attestations under the Gloas record, then its refusals, on one chain. Returns the
/// world and the reports of each.
pub fn attestations() -> (Stage5, Vec<TxReport>, Vec<TxReport>) {
    let mut s = Stage5::new();
    let slot = s.fx.finalized_slot;
    let record = s.w.inst.record(&s.w.chain, slot).unwrap().header().unwrap();
    assert_eq!(record.execution, ferrata_core::witness::ExecutionSummary::BlockHash(s.fx.block_hash));
    // The block header the request carries is the bound block's, and the roots come from it.
    let decoded = ferrata_core::exec::decode_header_with_hash(&s.fx.header, &s.fx.block_hash).unwrap();
    assert_eq!((decoded.number, decoded.receipts_root), (s.fx.block, s.fx.receipts_root()));
    let mut accepted = Vec::new();

    // 1. A storage slot: Sepolia USDC's `totalSupply_`, equal to `totalSupply()` at the block.
    let usdc = s.fx.storage_proof();
    let storage = Request {
        accounts: vec![account_query(
            &usdc,
            vec![],
            vec![(s.fx.slot, vec![condition(Field::Value, Cmp::Eq, s.fx.value)])],
        )],
        ..s.request()
    };
    let (report, a) =
        s.accept("storage slot: Sepolia USDC totalSupply_ (slot 11), with the block header", slot, &storage);
    assert_eq!((a.block_number, a.block_hash, a.chain_id), (s.fx.block, s.fx.block_hash, SEPOLIA_CHAIN_ID));
    assert_eq!(a.statements[1].id, storage_statement_id(SEPOLIA_CHAIN_ID, &s.fx.contract, &s.fx.slot, &s.fx.value));
    accepted.push(report);

    // 2. A log: the block's first ERC-20 `Transfer`, its emitter and recipient checked.
    let t = s.fx.transfer.clone();
    let log = Request {
        logs: vec![LogQuery {
            tx_index: t.tx_index,
            log_index: t.log_index,
            proof: s.fx.receipt_proof(t.tx_index),
            conditions: transfer_conditions(&t.token, &t.to),
        }],
        ..s.request()
    };
    let (report, a) = s.accept("log: an ERC-20 Transfer (emitter, recipient), with the block header", slot, &log);
    let json = s.fx.json_log(t.tx_index, t.log_index).clone();
    assert_eq!(a.statements[0].id, log_statement_id(SEPOLIA_CHAIN_ID, s.fx.block, t.tx_index, t.log_index, &json));
    accepted.push(report);

    // 3. An account at the same address, with the same code hash, on mainnet: Multicall3.
    let multicall = s.fx.same_address_proof();
    assert_eq!(multicall.code_hash, s.fx.mainnet_code_hash, "the same code on both chains");
    let account = Request {
        accounts: vec![account_query(
            &multicall,
            vec![condition(Field::CodeHash, Cmp::Eq, multicall.code_hash)],
            vec![],
        )],
        ..s.request()
    };
    let (report, a) = s.accept("account: Multicall3 (code hash), with the block header", slot, &account);
    assert_eq!(a.statements[0].id, account_statement_id(SEPOLIA_CHAIN_ID, &multicall));
    assert_ne!(a.statements[0].id, account_statement_id(1, &multicall), "a Sepolia statement is not a mainnet one");
    accepted.push(report);

    // 4. All three in one request.
    let batch = Request {
        accounts: [storage.accounts.clone(), account.accounts.clone()].concat(),
        logs: log.logs.clone(),
        ..s.request()
    };
    let (report, a) = s.accept("the three in one request, with the block header", slot, &batch);
    assert_eq!(a.statements.len(), 4, "USDC, its slot, Multicall3, the log");
    accepted.push(report);

    // Refusals, each a transaction that reverts with the code the native evaluation gives.
    let mut refused = Vec::new();
    let header = |h: Option<Vec<u8>>| Request { header: h, ..storage.clone() };
    let rlp = s.fx.header.clone();
    let mut tampered = rlp.clone();
    tampered[rlp.len() / 2] ^= 1;
    let cases: Vec<(&str, Request, u8)> = vec![
        ("no block header for a record that binds a block hash", header(None), ferrata_lez_wire::code::HEADER_REQUIRED),
        ("the block header with one byte changed", header(Some(tampered)), 120),
        ("the header of another block (12,103,422, the replay fixture's)", header(Some(bound_header())), 120),
        ("the block header truncated by one byte", header(Some(rlp[..rlp.len() - 1].to_vec())), 120),
        ("the block header with one byte appended", header(Some([rlp.as_slice(), &[0x80]].concat())), 120),
        (
            "F.5: the same proof asked as a mainnet statement (chain id 1)",
            Request { chain_id: 1, ..account.clone() },
            140,
        ),
        ("F.5: the same storage proof asked as a mainnet statement", Request { chain_id: 1, ..storage.clone() }, 140),
    ];
    for (label, request, code) in cases {
        refused.push(s.refuse(label, slot, &request, code));
    }
    // A record that holds the payload's fields takes no header: the bootstrap's (Electra layout).
    let boot = s.w.bootstrap.header.beacon.slot;
    let at_boot = Request { header: Some(rlp.clone()), ..log.clone() };
    refused.push(s.refuse(
        "a block header sent to a record that holds the payload (the bootstrap's)",
        boot,
        &at_boot,
        ferrata_lez_wire::code::BAD_QUERY,
    ));
    (s, accepted, refused)
}
