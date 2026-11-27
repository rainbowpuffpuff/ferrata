//! The borsh vectors of the IDL check (`idl/`, milestones A3 and B3).
//!
//! For every message of the attestation program and of the bridge program, every actor state
//! and every variant of every enum they contain, this test builds a value with distinct field
//! values, encodes it with borsh, and records its bytes beside its Rust `Debug` text in
//! `idl/vectors/*.vectors.json`. `idl/check.py` then decodes the bytes with nothing but the IDL
//! (`idl/*.idl.json`), renders what it decoded in `Debug` syntax and requires the same text: a
//! field missing, renamed, reordered or of another type in the IDL, or a variant at another
//! index, changes the text or the length.
//!
//! The matches below have no wildcard arm, so adding a variant to a message or a state stops
//! this test from compiling until a vector (and with it an IDL entry) is added.
//!
//! `FERRATA_IDL_WRITE=1 cargo test -p ferrata-lez-bridge-wire --test idl_vectors` rewrites the
//! files; without it the test fails if the committed files differ from what the wire encodes now.

use ferrata_lez_bridge_wire as bw;
use ferrata_lez_wire::{self as aw, attest};
use lee_core::account::AccountId;

type Root = [u8; 32];

fn r(n: u8) -> Root {
    core::array::from_fn(|i| n.wrapping_mul(7).wrapping_add(i as u8))
}
fn a(n: u8) -> [u8; 20] {
    core::array::from_fn(|i| n.wrapping_mul(11).wrapping_add(i as u8))
}
fn key(n: u8) -> [u8; 96] {
    core::array::from_fn(|i| n.wrapping_add(i as u8))
}

struct Vector {
    name: String,
    ty: &'static str,
    hex: String,
    debug: String,
}

fn vector<T: borsh::BorshSerialize + core::fmt::Debug>(name: &str, ty: &'static str, value: &T) -> Vector {
    let bytes = borsh::to_vec(value).expect("borsh");
    let debug = format!("{value:?}");
    assert!(!debug.contains('"') && !debug.contains('\\'), "{name}: Debug text needs JSON escaping");
    Vector { name: name.to_string(), ty, hex: hex(&bytes), debug }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ------------------------------------------------------------------------------------------
// The attestation program
// ------------------------------------------------------------------------------------------

fn message_label(m: &aw::Message) -> &'static str {
    use aw::Message::*;
    match m {
        Init { .. } => "Init",
        Open { .. } => "Open",
        Miller => "Miller",
        Adopt { .. } => "Adopt",
        Finish => "Finish",
        Read => "Read",
        ReadPadded { .. } => "ReadPadded",
        Attest { .. } => "Attest",
        VerifyAll { .. } => "VerifyAll",
        JobOpen { .. } => "JobOpen",
        Apply { .. } => "Apply",
        ApplyVerified { .. } => "ApplyVerified",
        Record { .. } => "Record",
        Succeed { .. } => "Succeed",
        Spawn { .. } => "Spawn",
    }
}

fn field_label(f: &attest::Field) -> &'static str {
    use attest::Field::*;
    match f {
        Nonce => "Nonce",
        Balance => "Balance",
        StorageRoot => "StorageRoot",
        CodeHash => "CodeHash",
        Value => "Value",
        Emitter => "Emitter",
        Topic(_) => "Topic",
        DataWord(_) => "DataWord",
    }
}

fn cmp_label(c: &attest::Cmp) -> &'static str {
    use attest::Cmp::*;
    match c {
        Eq => "Eq",
        Ne => "Ne",
        Lt => "Lt",
        Le => "Le",
        Gt => "Gt",
        Ge => "Ge",
    }
}

fn fact_label(f: &attest::Fact) -> &'static str {
    use attest::Fact::*;
    match f {
        Account { .. } => "Account",
        Storage { .. } => "Storage",
        Log { .. } => "Log",
        AccumulatorLeaf { .. } => "AccumulatorLeaf",
    }
}

fn config() -> aw::Config {
    aw::Config {
        chain_id: 11_155_111,
        sync_committee_size: 512,
        slots_per_epoch: 32,
        epochs_per_sync_committee_period: 256,
        min_sync_committee_participants: 1,
        fork_versions: core::array::from_fn(|i| [0x90, 0x00, 0x00, 0x69 + i as u8]),
        fork_epochs: core::array::from_fn(|i| 50 + 1000 * i as u64),
        genesis_validators_root: r(0xd8),
        genesis_time: 1_655_733_600,
        seconds_per_slot: 12,
        store_fork: 4,
        adoption_parts: 2,
        ws_period_epochs: 3_532,
    }
}

fn request() -> attest::Request {
    use attest::{Cmp, Condition, Field, Operand, Relation};
    let fields = [
        Field::Nonce,
        Field::Balance,
        Field::StorageRoot,
        Field::CodeHash,
        Field::Value,
        Field::Emitter,
        Field::Topic(2),
        Field::DataWord(3),
    ];
    let cmps = [Cmp::Eq, Cmp::Ne, Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge];
    let conditions: Vec<Condition> = fields
        .iter()
        .enumerate()
        .map(|(i, f)| Condition { field: *f, cmp: cmps[i % cmps.len()], rhs: r(40 + i as u8) })
        .collect();
    for f in &fields {
        let _ = field_label(f);
    }
    for c in &cmps {
        let _ = cmp_label(c);
    }
    attest::Request {
        instance_id: r(1),
        chain_id: 11_155_111,
        header: Some(vec![0xf9, 0x02, 0x10]),
        hops: vec![attest::Hop {
            account_proof: vec![vec![1, 2], vec![3]],
            storage_proof: vec![vec![4]],
            header: vec![5, 6],
        }],
        accounts: vec![attest::AccountQuery {
            address: a(2),
            proof: vec![vec![7, 8, 9]],
            conditions: conditions[..4].to_vec(),
            slots: vec![attest::SlotQuery {
                slot: r(3),
                proof: vec![vec![10], vec![11, 12]],
                conditions: conditions[4..5].to_vec(),
            }],
        }],
        logs: vec![attest::LogQuery {
            tx_index: 17,
            log_index: 2,
            proof: vec![vec![13, 14]],
            conditions: conditions[5..].to_vec(),
        }],
        leaves: vec![attest::LeafQuery { root: 1, leaf_index: 5, leaf: r(4), path: vec![r(5), r(6)] }],
        relations: vec![Relation {
            left: Operand { fact: 1, field: Field::Value },
            cmp: Cmp::Ge,
            right: Operand { fact: 2, field: Field::DataWord(0) },
        }],
    }
}

fn attestation() -> attest::Attestation {
    use attest::{Fact, Statement};
    let facts = vec![
        Fact::Account { address: a(3), nonce: 9, balance: r(7), storage_root: r(8), code_hash: r(9) },
        Fact::Storage { address: a(3), slot: r(10), value: r(11) },
        Fact::Log { tx_index: 17, log_index: 2, address: a(4), topics: vec![r(12), r(13)], data: vec![0, 1, 2, 3] },
        Fact::AccumulatorLeaf { address: a(5), slot: r(14), leaf_index: 6, leaf: r(15), root: r(16) },
    ];
    for f in &facts {
        let _ = fact_label(f);
    }
    attest::Attestation {
        instance_id: r(1),
        chain_id: 11_155_111,
        slot: 11_547_103,
        block_number: 12_103_422,
        block_hash: r(17),
        hops: 1,
        statements: facts.into_iter().enumerate().map(|(i, fact)| Statement { id: r(20 + i as u8), fact }).collect(),
    }
}

fn attestation_vectors() -> Vec<Vector> {
    use aw::Message::*;
    let messages = vec![
        Init { config: config(), checkpoint: r(30), bootstrap: vec![1, 2, 3] },
        Open { update: vec![4, 5], signers: vec![6] },
        Miller,
        Adopt { part: 1, keys: vec![key(7), key(8)] },
        Finish,
        Read,
        ReadPadded { padding: vec![0; 5] },
        Attest { request: request() },
        Attest { request: attest::Request::default() },
        VerifyAll {
            config: config(),
            checkpoint: r(31),
            store: vec![9, 9],
            update: vec![10],
            signers: vec![11, 12, 13],
            adoption: vec![key(9)],
        },
        JobOpen { instance_id: r(1), job_id: r(32), committee_size: 512, job: vec![14] },
        Apply { instance_id: r(1), job_id: r(33), job: vec![15, 16] },
        ApplyVerified { instance_id: r(1), job: vec![17] },
        Record { instance_id: r(1), slot: 11_547_104, chain_id: 11_155_111, record: vec![18, 19, 20], root_id: r(1) },
        Succeed { config: config(), checkpoint: r(35) },
        Spawn { root_id: r(1), parent_id: r(1), checkpoint: r(35), config: config(), store: vec![27, 28] },
    ];
    let mut out: Vec<Vector> = Vec::new();
    let mut seen = std::collections::BTreeMap::<&str, u32>::new();
    for m in &messages {
        let label = message_label(m);
        let n = seen.entry(label).or_default();
        *n += 1;
        out.push(vector(&format!("Message::{label}#{n}"), "Message", m));
    }
    out.push(vector(
        "InstanceState",
        "InstanceState",
        &aw::InstanceState {
            id: r(1),
            config: config(),
            active: true,
            store: vec![21, 22],
            applied: 1909,
            root_id: r(1),
            parent_id: r(0),
        },
    ));
    out.push(vector(
        "JobState",
        "JobState",
        &aw::JobState { instance_id: r(1), job_id: r(34), committee_size: 512, job: vec![23] },
    ));
    out.push(vector(
        "RecordState",
        "RecordState",
        &aw::RecordState { instance_id: r(1), chain_id: 11_155_111, record: vec![24, 25, 26], root_id: r(1) },
    ));
    out.push(vector("Attestation", "Attestation", &attestation()));
    out.push(vector("Request", "Request", &request()));
    out
}

// ------------------------------------------------------------------------------------------
// The bridge program
// ------------------------------------------------------------------------------------------

fn bridge_label(m: &bw::BridgeMessage) -> &'static str {
    use bw::BridgeMessage::*;
    match m {
        Burn { .. } => "Burn",
        ImportRoot { .. } => "ImportRoot",
        Sunset { .. } => "Sunset",
        Echo { .. } => "Echo",
        Init { .. } => "Init",
        Claim { .. } => "Claim",
        ClaimPrivately { .. } => "ClaimPrivately",
        Read => "Read",
        WriteRoot { .. } => "WriteRoot",
        Spend { .. } => "Spend",
        Mint { .. } => "Mint",
        Register { .. } => "Register",
        ImportFees { .. } => "ImportFees",
        WriteToken { .. } => "WriteToken",
        ExtendFees { .. } => "ExtendFees",
        Issue { .. } => "Issue",
        Admit { .. } => "Admit",
        Freeze { .. } => "Freeze",
        SetCap { .. } => "SetCap",
        Tick { .. } => "Tick",
        AttestRetired { .. } => "AttestRetired",
        ProposeRoles { .. } => "ProposeRoles",
        ApplyRoles { .. } => "ApplyRoles",
        CancelRoles { .. } => "CancelRoles",
        Pause { .. } => "Pause",
        Cosigned { .. } => "Cosigned",
        Transfer { .. } => "Transfer",
        Credit { .. } => "Credit",
        SetDepth { .. } => "SetDepth",
        AttestListing { .. } => "AttestListing",
        SetListing { .. } => "SetListing",
        CollectFees { .. } => "CollectFees",
        AccrueFees { .. } => "AccrueFees",
        MintFees { .. } => "MintFees",
    }
}

fn state_label(s: &bw::BridgeState) -> &'static str {
    match s {
        bw::BridgeState::Config(bw::ConfigState::Pairing { .. }) => "Config.Pairing",
        bw::BridgeState::Config(bw::ConfigState::Ready { pending: None, .. }) => "Config.Ready",
        bw::BridgeState::Config(bw::ConfigState::Ready { pending: Some(p), .. }) => match p {
            bw::Pending::Import(_) => "Config.Ready.Import",
            bw::Pending::Register { .. } => "Config.Ready.Register",
            bw::Pending::Fees { .. } => "Config.Ready.Fees",
            bw::Pending::Time(t) => match t.then {
                bw::Timed::Tick => "Config.Ready.Time.Tick",
                bw::Timed::Freeze { .. } => "Config.Ready.Time.Freeze",
                bw::Timed::Pause { .. } => "Config.Ready.Time.Pause",
                bw::Timed::Propose { .. } => "Config.Ready.Time.Propose",
                bw::Timed::Apply => "Config.Ready.Time.Apply",
            },
            bw::Pending::Retired { .. } => "Config.Ready.Retired",
            bw::Pending::Listing { .. } => "Config.Ready.Listing",
            bw::Pending::Collect { .. } => "Config.Ready.Collect",
        },
        bw::BridgeState::Root(_) => "Root",
        bw::BridgeState::Nullifier(_) => "Nullifier",
        bw::BridgeState::Holder(_) => "Holder",
        bw::BridgeState::Burn(_) => "Burn",
        bw::BridgeState::Token(_) => "Token",
        bw::BridgeState::Fees(_) => "Fees",
    }
}

fn entry() -> bw::TokenEntry {
    bw::TokenEntry {
        token: a(40),
        lez_definition: r(41),
        decimals: 6,
        lez_unit: 1_000_000,
        unit_weight: 3,
        denominations: vec![1, 10, 100],
        relay_fees: vec![0, 1, 2],
    }
}

fn schedule(index: u64) -> bw::FeeSchedule {
    bw::FeeSchedule {
        index,
        from_leaf: 10 + index,
        announced_at: 1_700_000_000 + index,
        effective_at: 1_700_604_800 + index,
        mint_fees: vec![5, 6, 7],
        burn_fees: vec![8, 9, 10],
        checkpoint_fees: vec![11, 12, 13],
    }
}

fn controls() -> bw::Controls {
    bw::Controls {
        admin: r(42),
        freeze_authority: r(43),
        fee_recipient: r(44),
        floor: bw::CapParams { capacity: 100, refill_per_second: 1 },
        token_cap: bw::CapParams { capacity: 10_000, refill_per_second: 7 },
        global_cap: bw::CapParams { capacity: u128::MAX, refill_per_second: 9 },
    }
}

fn bridge_config() -> bw::BridgeConfig {
    bw::BridgeConfig {
        bedrock_genesis_id: r(45),
        channel_id: r(46),
        bridge_image_id: r(47),
        attestation: AccountId::new(r(48)),
        instance_id: r(1),
        evm_chain_id: 11_155_111,
        vault: a(49),
        controls: controls(),
    }
}

fn gate() -> bw::Gate {
    bw::Gate {
        now_ms: 1_762_800_000_000,
        mints_until: 0,
        burns_until: 1_762_900_000_000,
        mint: bw::Bucket { capacity: 1000, refill_per_second: 2, level: 900, last_ms: 1_762_800_000_001 },
        burn: bw::Bucket { capacity: 2000, refill_per_second: 3, level: 1500, last_ms: 1_762_800_000_002 },
    }
}

fn witness() -> bw::ClaimWitness {
    bw::ClaimWitness {
        depositor: a(50),
        token: a(40),
        amount: r(51),
        leaf_index: 15,
        path: vec![r(52), r(53)],
        s: r(54),
        kind: 2,
        dest: r(55),
    }
}

fn freeze_authority(n: u8) -> bw::FreezeAuthority {
    bw::FreezeAuthority { members: vec![r(n), r(n + 1)], threshold: 2 }
}

fn roles(proposal: bool) -> bw::Roles {
    bw::Roles {
        freeze_authority: freeze_authority(70),
        watchers: vec![r(72)],
        proposal: proposal.then(|| bw::RoleProposal {
            freeze_authority: freeze_authority(73),
            watchers: vec![],
            effective_ms: 1_764_000_000_000,
        }),
        vault_retired: false,
        sunset_approved: proposal,
        inbound_depth_slots: if proposal { 64 } else { 0 },
        tip_slot: 11_547_300,
    }
}

fn time_record(header: bool) -> bw::TimeRecord {
    bw::TimeRecord { record_slot: 11_547_200, header: header.then(|| vec![0xf9, 1, 2]) }
}

fn token_state(pending: Option<bw::PendingTime>) -> bw::TokenState {
    bw::TokenState {
        config_id: r(60),
        evm_chain_id: 11_155_111,
        vault: a(49),
        attestation: AccountId::new(r(48)),
        instance_id: r(1),
        entry: entry(),
        schedules: vec![schedule(0), schedule(1)],
        known_until: 42,
        controls: controls(),
        gate: gate(),
        pending,
        delisted: false,
        listing_slot: 11_547_160,
    }
}

fn root_state() -> bw::RootState {
    bw::RootState {
        config_id: r(60),
        evm_chain_id: 11_155_111,
        vault: a(49),
        leaf_count: 16,
        root: r(61),
        record_slot: 11_547_150,
        block_number: 9_600_123,
    }
}

fn bridge_vectors() -> Vec<Vector> {
    use bw::BridgeMessage::*;
    let messages = vec![
        Burn {
            config_id: r(60),
            r: r(62),
            memo: a(63),
            mac: core::array::from_fn(|i| 0xa0 + i as u8),
            token: a(40),
            amount: r(64),
            c_out: r(65),
        },
        ImportRoot {
            config_id: r(60),
            record_slot: 11_547_150,
            root: r(61),
            leaf_count: 16,
            proof: bw::ImportProof {
                account: vec![vec![1, 2]],
                root_slot: vec![vec![3]],
                count_slot: vec![vec![4], vec![5]],
            },
        },
        Sunset { config_id: r(60) },
        Echo { nonce: r(66) },
        Init {
            config: bridge_config(),
            record_slot: 11_547_100,
            proof: bw::PairingProof { account: vec![vec![6]], config_slot: vec![vec![7, 8]] },
        },
        Claim { config_id: r(60), leaf_count: 16, witness: witness(), relayer: Some(r(67)) },
        Claim { config_id: r(60), leaf_count: 16, witness: witness(), relayer: None },
        ClaimPrivately { config_id: r(60), leaf_count: 16, witness: witness(), relayer: None },
        Read,
        WriteRoot { root: root_state() },
        Spend {
            nullifier: r(68),
            config_id: r(60),
            evm_chain_id: 11_155_111,
            vault: a(49),
            token: a(40),
            leaf_count: 16,
            amount: 100_000_000_000_000_000,
            mint_to: Some(r(69)),
            fees: vec![(r(70), 3), (r(71), 4)],
        },
        Mint {
            nullifier: r(68),
            config_id: r(60),
            evm_chain_id: 11_155_111,
            vault: a(49),
            token: a(40),
            holder: r(72),
            amount: 99,
        },
        Register {
            config_id: r(60),
            entry: entry(),
            schedule: schedule(0),
            record_slot: 11_547_160,
            proof: bw::VaultProof { account: vec![vec![9]], slots: vec![vec![vec![10], vec![11]], vec![]] },
        },
        ImportFees {
            config_id: r(60),
            token: a(40),
            index: 1,
            schedule: Some(schedule(1)),
            record_slot: 11_547_170,
            proof: bw::VaultProof { account: vec![], slots: vec![vec![vec![12]]] },
        },
        ImportFees {
            config_id: r(60),
            token: a(40),
            index: 2,
            schedule: None,
            record_slot: 11_547_171,
            proof: bw::VaultProof::default(),
        },
        WriteToken { state: token_state(None) },
        ExtendFees { config_id: r(60), token: a(40), index: 1, schedule: Some(schedule(1)), count: 30 },
        Issue {
            config_id: r(60),
            token: a(40),
            nullifier: r(68),
            amount: 1_000_000,
            leaf_count: 16,
            mint_to: None,
            relayer: Some(r(67)),
        },
        Admit { config_id: r(60), token: a(40), mints: true, value: 77 },
        Freeze {
            config_id: r(60),
            token: Some(a(40)),
            mints: true,
            burns: false,
            until_ms: 1_763_000_000_000,
            record: time_record(true),
        },
        Freeze { config_id: r(60), token: None, mints: false, burns: true, until_ms: 0, record: time_record(false) },
        SetCap {
            config_id: r(60),
            token: Some(a(40)),
            mints: false,
            cap: bw::CapParams { capacity: 5, refill_per_second: 6 },
        },
        Tick { config_id: r(60), token: None, record: time_record(false) },
        AttestRetired {
            config_id: r(60),
            record_slot: 11_547_300,
            proof: bw::VaultProof { account: vec![vec![13]], slots: vec![vec![vec![14]]] },
        },
        ProposeRoles {
            config_id: r(60),
            freeze_authority: freeze_authority(70),
            watchers: vec![r(72), r(73)],
            record: time_record(true),
        },
        ApplyRoles { config_id: r(60), record: time_record(false) },
        CancelRoles { config_id: r(60) },
        Pause { config_id: r(60), token: Some(a(40)), mints: true, burns: false, record: time_record(false) },
        Cosigned { config_id: r(60), signers: vec![r(70), r(71)], action: bw::CosignedAction::Sunset },
        Cosigned {
            config_id: r(60),
            signers: vec![r(70), r(71)],
            action: bw::CosignedAction::Freeze {
                token: None,
                mints: true,
                burns: true,
                until_ms: 1_763_000_000_002,
                record: time_record(false),
            },
        },
        Transfer { config_id: r(60), token: a(40), amount: 900_000_000_000_000_000, to: r(74) },
        Credit { config_id: r(60), token: a(40), amount: 900_000_000_000_000_000, to: r(74) },
        SetDepth { config_id: r(60), slots: 64 },
        AttestListing {
            config_id: r(60),
            token: a(40),
            record_slot: 11_547_320,
            proof: bw::VaultProof { account: vec![vec![0xf8, 0x51]], slots: vec![vec![vec![0xe3, 0x01]]] },
        },
        SetListing { config_id: r(60), token: a(40), delisted: true, record_slot: 11_547_320 },
        CollectFees {
            config_id: r(60),
            token: a(40),
            record_slot: 11_547_330,
            limit: 5_000_000_000_000,
            proof: bw::VaultProof { account: vec![vec![0xf8, 0x52]], slots: vec![vec![vec![0xe3, 0x02]]] },
        },
        AccrueFees { config_id: r(60), token: a(40), total: 7_000_000_000_000, limit: u128::MAX },
        MintFees { config_id: r(60), token: a(40), amount: 7_000_000_000_000 },
    ];
    let mut out: Vec<Vector> = Vec::new();
    let mut seen = std::collections::BTreeMap::<&str, u32>::new();
    for m in &messages {
        let label = bridge_label(m);
        let n = seen.entry(label).or_default();
        *n += 1;
        out.push(vector(&format!("BridgeMessage::{label}#{n}"), "BridgeMessage", m));
    }
    let ready = |pending| bw::ConfigState::Ready {
        config: bridge_config(),
        pending,
        gate: gate(),
        sunset: false,
        roles: roles(false),
    };
    let states = vec![
        bw::BridgeState::Config(bw::ConfigState::Pairing { config: bridge_config(), record_slot: 11_547_100 }),
        bw::BridgeState::Config(ready(None)),
        bw::BridgeState::Config(ready(Some(bw::Pending::Import(bw::PendingImport {
            record_slot: 11_547_150,
            root: r(61),
            leaf_count: 16,
        })))),
        bw::BridgeState::Config(ready(Some(bw::Pending::Register {
            entry: entry(),
            schedule: schedule(0),
            record_slot: 11_547_160,
        }))),
        bw::BridgeState::Config(ready(Some(bw::Pending::Fees {
            token: a(40),
            index: 3,
            schedule: None,
            record_slot: 11_547_180,
        }))),
        bw::BridgeState::Config(ready(Some(bw::Pending::Time(bw::PendingTime {
            record: time_record(true),
            then: bw::Timed::Tick,
        })))),
        bw::BridgeState::Config(bw::ConfigState::Ready {
            config: bridge_config(),
            pending: Some(bw::Pending::Time(bw::PendingTime {
                record: time_record(false),
                then: bw::Timed::Freeze { mints: true, burns: true, until_ms: 1_763_000_000_001 },
            })),
            gate: gate(),
            sunset: true,
            roles: roles(true),
        }),
        bw::BridgeState::Config(ready(Some(bw::Pending::Time(bw::PendingTime {
            record: time_record(false),
            then: bw::Timed::Pause { mints: true, burns: true },
        })))),
        bw::BridgeState::Config(ready(Some(bw::Pending::Time(bw::PendingTime {
            record: time_record(false),
            then: bw::Timed::Propose { freeze_authority: freeze_authority(73), watchers: vec![r(74)] },
        })))),
        bw::BridgeState::Config(ready(Some(bw::Pending::Time(bw::PendingTime {
            record: time_record(true),
            then: bw::Timed::Apply,
        })))),
        bw::BridgeState::Config(ready(Some(bw::Pending::Retired { record_slot: 11_547_310 }))),
        bw::BridgeState::Config(ready(Some(bw::Pending::Listing { token: a(40), record_slot: 11_547_320 }))),
        bw::BridgeState::Config(ready(Some(bw::Pending::Collect {
            token: a(40),
            record_slot: 11_547_330,
            limit: 5_000_000_000_000,
        }))),
        bw::BridgeState::Root(root_state()),
        bw::BridgeState::Nullifier(bw::NullifierState {
            config_id: r(60),
            token: a(40),
            leaf_count: 16,
            amount: 1_000_000,
        }),
        bw::BridgeState::Holder(bw::HolderState {
            holdings: vec![
                bw::Holding { config_id: r(60), token: a(40), amount: 5 },
                bw::Holding { config_id: r(60), token: a(73), amount: 6 },
            ],
            pending: Some(bw::PendingClaim { config_id: r(60), leaf_count: 16, witness: witness(), relayer: None }),
        }),
        bw::BridgeState::Holder(bw::HolderState { holdings: vec![], pending: None }),
        bw::BridgeState::Burn(bw::BurnRecord {
            config_id: r(60),
            note: bw::Note { r: r(62), memo: a(63), mac: [0xee; 16], token: a(40), amount: r(64), c_out: r(65) },
        }),
        bw::BridgeState::Token(token_state(None)),
        bw::BridgeState::Token(token_state(Some(bw::PendingTime {
            record: time_record(true),
            then: bw::Timed::Freeze { mints: false, burns: true, until_ms: 9 },
        }))),
        bw::BridgeState::Token(bw::TokenState { delisted: true, listing_slot: 11_547_320, ..token_state(None) }),
        bw::BridgeState::Fees(bw::FeesState { config_id: r(60), token: a(40), collected: 7_000_000_000_000 }),
    ];
    let mut seen = std::collections::BTreeMap::<&str, u32>::new();
    for s in &states {
        let label = state_label(s);
        let n = seen.entry(label).or_default();
        *n += 1;
        out.push(vector(&format!("BridgeState::{label}#{n}"), "BridgeState", s));
    }
    out
}

// ------------------------------------------------------------------------------------------
// Files
// ------------------------------------------------------------------------------------------

fn render(program: &str, vectors: &[Vector], codes: &[(&str, u8)]) -> String {
    let mut s = String::new();
    s.push_str("{\n");
    s.push_str(&format!("  \"program\": \"{program}\",\n"));
    s.push_str("  \"generator\": \"lez/bridge-wire/tests/idl_vectors.rs (FERRATA_IDL_WRITE=1 to rewrite)\",\n");
    s.push_str("  \"encoding\": \"borsh 1.x, as the program decodes its message and its actor state\",\n");
    s.push_str("  \"program_codes\": [\n");
    for (i, (name, code)) in codes.iter().enumerate() {
        let comma = if i + 1 < codes.len() { "," } else { "" };
        s.push_str(&format!("    [\"{name}\", {code}]{comma}\n"));
    }
    s.push_str("  ],\n  \"vectors\": [\n");
    for (i, v) in vectors.iter().enumerate() {
        let comma = if i + 1 < vectors.len() { "," } else { "" };
        s.push_str(&format!(
            "    {{\"name\": \"{}\", \"type\": \"{}\", \"hex\": \"{}\",\n     \"debug\": \"{}\"}}{comma}\n",
            v.name, v.ty, v.hex, v.debug
        ));
    }
    s.push_str("  ]\n}\n");
    s
}

fn check_or_write(file: &str, text: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../idl/vectors").join(file);
    if std::env::var("FERRATA_IDL_WRITE").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        eprintln!("wrote {}", path.display());
    } else {
        let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            committed == text,
            "{} differs from what the wire encodes now: rerun with FERRATA_IDL_WRITE=1, then update idl/*.idl.json until idl/check.py passes",
            path.display()
        );
    }
}

#[test]
fn attestation_program_vectors() {
    let vectors = attestation_vectors();
    assert!(vectors.len() >= 18);
    check_or_write("ferrata_attestation.vectors.json", &render("ferrata_attestation", &vectors, aw::code::ALL));
}

#[test]
fn bridge_program_vectors() {
    let vectors = bridge_vectors();
    assert!(vectors.len() >= 30);
    check_or_write("ferrata_bridge.vectors.json", &render("ferrata_bridge", &vectors, bw::code::ALL));
}

// ------------------------------------------------------------------------------------------
// PDA vectors: every actor kind's account id from the wire's own functions, for `check.py` to
// recompute from the seeds the IDL documents (domain, tag, inputs and their encodings).
// ------------------------------------------------------------------------------------------

fn pda_entry(
    program: &str,
    actor: &str,
    program_id: &AccountId,
    inputs: &[(&str, &str, String)],
    account: &lee_core::account::Actor,
) -> String {
    let inputs: Vec<String> = inputs
        .iter()
        .map(|(name, enc, value)| format!("{{\"name\": \"{name}\", \"encoding\": \"{enc}\", \"value\": \"{value}\"}}"))
        .collect();
    format!(
        "    {{\"program\": \"{program}\", \"actor\": \"{actor}\", \"program_id\": \"{}\", \"inputs\": [{}], \"account\": \"{}\"}}",
        hex(program_id.value()),
        inputs.join(", "),
        hex(account.account_id.value())
    )
}

#[test]
fn pda_vectors() {
    let program = AccountId::new(r(90));
    let (id, job, cid, token, vault, nullifier, note) = (r(1), r(91), r(60), a(40), a(49), r(68), r(92));
    let holder = AccountId::new(r(93));
    let entries = [
        pda_entry(
            "ferrata_attestation",
            "instance",
            &program,
            &[("instance_id", "bytes", hex(&id))],
            &aw::instance_actor(program, &id),
        ),
        pda_entry(
            "ferrata_attestation",
            "job",
            &program,
            &[("instance_id", "bytes", hex(&id)), ("job_id", "bytes", hex(&job))],
            &aw::job_actor(program, &id, &job),
        ),
        pda_entry(
            "ferrata_attestation",
            "header",
            &program,
            &[("instance_id", "bytes", hex(&id)), ("slot", "u64le", 11_549_664u64.to_string())],
            &aw::header_actor(program, &id, 11_549_664),
        ),
        pda_entry(
            "ferrata_bridge",
            "config",
            &program,
            &[("config_id", "bytes", hex(&cid))],
            &bw::config_actor(program, &cid),
        ),
        pda_entry(
            "ferrata_bridge",
            "token_actor",
            &program,
            &[("config_id", "bytes", hex(&cid)), ("token", "bytes", hex(&token))],
            &bw::token_actor(program, &cid, &token),
        ),
        pda_entry(
            "ferrata_bridge",
            "root",
            &program,
            &[("config_id", "bytes", hex(&cid)), ("leaf_count", "u64le", 16u64.to_string())],
            &bw::root_actor(program, &cid, 16),
        ),
        pda_entry(
            "ferrata_bridge",
            "nullifier",
            &program,
            &[
                ("config_id", "bytes", hex(&cid)),
                ("evm_chain_id", "u256be", 11_155_111u64.to_string()),
                ("vault", "bytes", hex(&vault)),
                ("nullifier", "bytes", hex(&nullifier)),
            ],
            &bw::nullifier_actor(program, &cid, 11_155_111, &vault, &nullifier),
        ),
        pda_entry(
            "ferrata_bridge",
            "burn",
            &program,
            &[("config_id", "bytes", hex(&cid)), ("r", "bytes", hex(&note))],
            &bw::burn_actor(program, &cid, &note),
        ),
        pda_entry(
            "ferrata_bridge",
            "fees",
            &program,
            &[("config_id", "bytes", hex(&cid)), ("token", "bytes", hex(&token))],
            &bw::fees_actor(program, &cid, &token),
        ),
        pda_entry(
            "ferrata_bridge",
            "holder",
            &program,
            &[("config_id", "bytes", hex(&cid)), ("holder", "bytes", hex(holder.value()))],
            &bw::holder_actor(program, &cid, holder),
        ),
    ];
    let text = format!(
        "{{\n  \"generator\": \"lez/bridge-wire/tests/idl_vectors.rs (FERRATA_IDL_WRITE=1 to rewrite)\",\n  \"pdas\": [\n{}\n  ]\n}}\n",
        entries.join(",\n")
    );
    check_or_write("pda.vectors.json", &text);
}
