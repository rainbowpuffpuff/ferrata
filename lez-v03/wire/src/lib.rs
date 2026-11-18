//! Instructions, effects, state and addresses of Ferrata's light-client program on LEZ **v0.3.1**,
//! shared by the guest and the host. EXPERIMENTAL and UNAUDITED (see `../README.md`).
//!
//! LEZ v0.3.1 runs a program in two phases (`lee_core::program`, tag v0.3.1): a **plan**, which sees
//! the instruction and the accounts' ids but no state, and returns *effects* on its own shards of
//! those accounts; then one **apply** per effect, which sees one shard's data and the effect's
//! data, and returns the shard's new data. A transaction is atomic: if any apply refuses, nothing
//! of it is kept. So the plan here only routes, and every check runs in an apply:
//!
//! | account (public PDA of the program) | seed | shard |
//! |---|---|---|
//! | `instance(id)` | `H("…/instance" ‖ id)` | [`InstanceState`]: config, store, the job in progress |
//! | `header(id, slot)` | `H("…/header" ‖ id ‖ slot)` | [`RecordState`]: a finalized header, written once |
//! | `statement(id, s)` | `H("…/statement" ‖ id ‖ s)` | [`Statement`]: an attested storage slot, written once |
//!
//! `id = H("…/instance-id" ‖ config ‖ checkpoint)` as on LEZ v0.4 (the same [`Config`] and the
//! same trusted checkpoint), under this program's own domain tag.
//!
//! An apply that writes `header(id, slot)` or `statement(id, s)` cannot see the instance's store:
//! the transaction's instruction *claims* the record (or the statement), the apply at the
//! instance (or at the record) checks the claim against what it verified, and refuses otherwise,
//! which reverts the whole transaction. The same holds for the timestamp validity window, which the
//! plan must set without seeing the store: the instruction claims it, and the instance's apply
//! checks it is exactly the window the LEZ v0.4 program sets (from the start of the signature slot
//! to the end of the weak-subjectivity period of the stored finalized header).

use borsh::{BorshDeserialize, BorshSerialize};
use ferrata_core::{
    fork::{ChainSpec, LcFork, Preset},
    ws, Root,
};
use lee_core::{account::AccountId, program::PdaSeed};
use sha2::{Digest, Sha256};

/// Version tag inside every hash this program derives (not the v0.4 program's tag).
const DOMAIN: &[u8] = b"ferrata-lez-v03/v1/";

fn tagged_hash(tag: &[u8], parts: &[&[u8]]) -> Root {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(tag);
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// The chain an instance follows; fixed at `Init`, part of the instance id. The same structure,
/// borsh layout and meaning as the LEZ v0.4 program's `ferrata_lez_wire::Config`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Config {
    /// The execution chain id (EIP-155); every record and statement carries it.
    pub chain_id: u64,
    pub sync_committee_size: u32,
    pub slots_per_epoch: u64,
    pub epochs_per_sync_committee_period: u64,
    pub min_sync_committee_participants: u64,
    /// Fork versions, indexed like `ChainSpec::versions`.
    pub fork_versions: [[u8; 4]; 9],
    /// Fork activation epochs, indexed like `ChainSpec::epochs`.
    pub fork_epochs: [u64; 9],
    pub genesis_validators_root: Root,
    /// Unix seconds of slot 0.
    pub genesis_time: u64,
    pub seconds_per_slot: u64,
    /// The light-client layout the store starts in (0 = Altair … 4 = Gloas).
    pub store_fork: u8,
    /// How many parts a committee adoption is split into (1, 2, 4, …).
    pub adoption_parts: u8,
    /// The weak-subjectivity period in epochs (`ferrata_core::ws`), not zero.
    pub ws_period_epochs: u64,
}

impl Config {
    pub fn spec(&self) -> ChainSpec {
        ChainSpec {
            preset: Preset {
                sync_committee_size: self.sync_committee_size as usize,
                slots_per_epoch: self.slots_per_epoch,
                epochs_per_sync_committee_period: self.epochs_per_sync_committee_period,
                min_sync_committee_participants: self.min_sync_committee_participants,
            },
            versions: self.fork_versions,
            epochs: self.fork_epochs,
        }
    }

    /// The layout the store starts in, or `None` for an unknown byte.
    pub fn fork(&self) -> Option<LcFork> {
        Some(match self.store_fork {
            0 => LcFork::Altair,
            1 => LcFork::Capella,
            2 => LcFork::Deneb,
            3 => LcFork::Electra,
            4 => LcFork::Gloas,
            _ => return None,
        })
    }

    /// Unix milliseconds at which `slot` starts (LEZ v0.3.1 block timestamps are milliseconds).
    pub fn slot_start_ms(&self, slot: u64) -> u64 {
        self.genesis_time.saturating_add(slot.saturating_mul(self.seconds_per_slot)).saturating_mul(1000)
    }

    /// The first slot outside the weak-subjectivity period of a finalized header at `finalized_slot`.
    pub fn ws_end_slot(&self, finalized_slot: u64) -> Option<u64> {
        ws::weak_subjectivity_end_slot(finalized_slot, self.ws_period_epochs, &self.spec())
    }

    /// Unix milliseconds at which that slot starts.
    pub fn ws_end_ms(&self, finalized_slot: u64) -> Option<u64> {
        self.ws_end_slot(finalized_slot).map(|slot| self.slot_start_ms(slot))
    }
}

/// `H(config ‖ checkpoint)`: an instance's id.
pub fn instance_id(config: &Config, checkpoint: &Root) -> Root {
    tagged_hash(b"instance-id", &[&borsh::to_vec(config).expect("borsh"), checkpoint])
}

pub fn instance_seed(instance_id: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"instance", &[instance_id]))
}

pub fn header_seed(instance_id: &Root, slot: u64) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"header", &[instance_id, &slot.to_le_bytes()]))
}

pub fn statement_seed(instance_id: &Root, statement_id: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"statement", &[instance_id, statement_id]))
}

/// `instance(id)` of the program deployed at `program`.
pub fn instance_account(program: &AccountId, instance_id: &Root) -> AccountId {
    AccountId::for_public_pda(program, &instance_seed(instance_id))
}

/// `header(id, slot)` of the program deployed at `program`.
pub fn header_account(program: &AccountId, instance_id: &Root, slot: u64) -> AccountId {
    AccountId::for_public_pda(program, &header_seed(instance_id, slot))
}

/// `statement(id, s)` of the program deployed at `program`, `s` the block-bound statement id.
pub fn statement_account(program: &AccountId, instance_id: &Root, statement_id: &Root) -> AccountId {
    AccountId::for_public_pda(program, &statement_seed(instance_id, statement_id))
}

/// An uncompressed BLS public key (96 bytes).
pub type UncompressedKey = [u8; 96];
/// A 20-byte account address.
pub type Address = [u8; 20];
/// A 256-bit value, big-endian.
pub type Word = [u8; 32];

/// The timestamp validity window (Unix ms, `[from, to)`) the instruction claims and the plan sets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Window {
    pub from_ms: Option<u64>,
    pub to_ms: Option<u64>,
}

/// One part of a committee adoption: the uncompressed keys of part `index`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Part {
    pub index: u8,
    pub keys: Vec<UncompressedKey>,
}

/// How far an `Update` verifies the signature in its own transaction.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Verify {
    /// Step 1 only (the core's `steps::step1`): the job waits for its Miller loops.
    Open,
    /// Steps 1 and 2: the job waits for its final exponentiation.
    Miller,
    /// Steps 1, 2 and 3: the core's whole check, final exponentiation included.
    Core,
    /// EXPERIMENTAL, UNAUDITED: steps 1 and 2, then the residue-witness check (`bench/residue`,
    /// `bench/RESIDUE.md`) instead of the final exponentiation; `c` and `w` are 576 bytes each.
    Residue { c: Vec<u8>, w: Vec<u8> },
}

/// One step of the instance's job in progress (`Continue`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Step {
    /// The Miller loops (`Job::step2`).
    Miller,
    /// The final exponentiation (`Job::step3`).
    FinalExp,
    /// EXPERIMENTAL, UNAUDITED: the residue-witness check in place of `FinalExp`.
    Residue { c: Vec<u8>, w: Vec<u8> },
    /// One part of the committee adoption (`Job::adopt`).
    Adopt(Part),
}

/// The record a transaction claims it writes at `header(id, slot)`: the encoding of the
/// `ferrata_core::witness::HeaderRecord` of the finalized header its job applies, and the chain id.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RecordClaim {
    pub chain_id: u64,
    pub record: Vec<u8>,
}

/// One storage slot and its storage proof (RLP nodes).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SlotProof {
    pub slot: Word,
    pub proof: Vec<Vec<u8>>,
}

/// Storage slots of one account under a record: the account proof, the slots' proofs, and, for a
/// record that binds a block hash only (a Gloas-layout header), the block's RLP header.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StorageQuery {
    pub chain_id: u64,
    pub address: Address,
    pub account_proof: Vec<Vec<u8>>,
    pub slots: Vec<SlotProof>,
    pub header: Option<Vec<u8>>,
}

/// An attested storage slot, as `statement(id, s)` holds it. `id` is the core's statement id
/// (`ferrata_core::attest::storage_id`, the same for the same value at any block); `id_at_block`
/// the block-bound one (`storage_id_at_block`), which names the account.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Statement {
    pub instance_id: Root,
    pub chain_id: u64,
    /// The beacon slot of the record the slot was proven under.
    pub record_slot: u64,
    pub block_number: u64,
    pub block_hash: Root,
    pub address: Address,
    pub slot: Word,
    pub value: Word,
    pub id: Root,
    pub id_at_block: Root,
}

/// A transaction's instruction.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    /// Create `instance(id)` from a trusted checkpoint (the block root `checkpoint` and the
    /// bootstrap in the config's store layout), with any adoption parts of its committee; once
    /// every part is adopted the instance is active and `header(id, checkpoint slot)` written.
    /// Accounts: `instance(id)`, then `header(id, slot)` if `record` is claimed.
    Init {
        config: Config,
        checkpoint: Root,
        bootstrap: Vec<u8>,
        adopt: Vec<Part>,
        record: Option<RecordClaim>,
        window: Window,
    },
    /// Verify an update against the store (`verify` says how far), adopt any parts, and apply it
    /// if complete. `signers` is the core's `SignerWitness` encoding, or (EXPERIMENTAL) `0xffff`/
    /// `0xfffe` followed by a `residue_agg::NonSigners` witness. Accounts: `instance(id)`, then
    /// `header(id, slot)` if `record` is claimed.
    Update {
        instance_id: Root,
        update: Vec<u8>,
        signers: Vec<u8>,
        verify: Verify,
        adopt: Vec<Part>,
        record: Option<RecordClaim>,
        window: Window,
    },
    /// Run steps of the instance's job in progress, and apply it if complete. Accounts:
    /// `instance(id)`, then `header(id, slot)` if `record` is claimed.
    Continue { instance_id: Root, steps: Vec<Step>, record: Option<RecordClaim> },
    /// Prove storage slots under `header(id, slot)` and write each as `statement(id, s)`.
    /// Accounts: `header(id, slot)`, then one `statement(id, s)` per claimed statement, in order.
    Attest { instance_id: Root, slot: u64, query: StorageQuery, statements: Vec<Statement> },
}

/// What happens at `instance(id)`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum InstanceOp {
    Init { config: Config, checkpoint: Root, bootstrap: Vec<u8>, adopt: Vec<Part> },
    Update { instance_id: Root, update: Vec<u8>, signers: Vec<u8>, verify: Verify, adopt: Vec<Part> },
    Continue { instance_id: Root, steps: Vec<Step> },
}

/// The data of one effect (the plan's output, one apply's input).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Effect {
    /// At `instance(id)`: verify and advance; the claims are checked against what was verified.
    Instance { op: InstanceOp, record: Option<RecordClaim>, window: Window },
    /// At `header(id, slot)`: write the claimed record (once).
    Record { instance_id: Root, slot: u64, claim: RecordClaim },
    /// At `header(id, slot)`: verify the query against the record and that it proves exactly the
    /// claimed statements; changes nothing.
    Check { instance_id: Root, query: StorageQuery, statements: Vec<Statement> },
    /// At `statement(id, s)`: write the claimed statement (once).
    Statement(Statement),
}

/// The shard of `instance(id)`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct InstanceState {
    pub id: Root,
    pub config: Config,
    pub checkpoint: Root,
    /// False until the bootstrap committee is adopted.
    pub active: bool,
    /// `ferrata_core::witness::CompactStore` encoding.
    pub store: Vec<u8>,
    /// Jobs applied (the activation included).
    pub applied: u64,
    /// `ferrata_core::steps::Job` encoding of the job in progress, if any.
    pub job: Option<Vec<u8>>,
}

/// The shard of `header(id, slot)`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RecordState {
    pub instance_id: Root,
    pub chain_id: u64,
    /// `ferrata_core::witness::HeaderRecord` encoding.
    pub record: Vec<u8>,
}

/// Defines exit codes and `ALL`, every one of them with its name.
macro_rules! exit_codes {
    ($($(#[doc = $doc:literal])* $name:ident = $value:literal;)*) => {
        $($(#[doc = $doc])* pub const $name: u8 = $value;)*
        /// Every code of this module, with its name.
        pub const ALL: &[(&str, u8)] = &[$((stringify!($name), $name)),*];
    };
}

/// Exit codes. The core's refusals keep their codes (1–79 light client, 80–84 the step API,
/// 100–199 the execution layer); this program's own are 200–229.
pub mod code {
    exit_codes! {
        /// The instruction or an effect does not decode.
        BAD_INSTRUCTION = 200;
        /// An account is not the one the instruction implies, or the count is wrong.
        WRONG_ACCOUNT = 201;
        /// `Init` on an instance that exists.
        ALREADY_INITIALIZED = 203;
        /// No instance at the account.
        NO_INSTANCE = 204;
        /// `Continue` with no job in progress.
        NO_JOB = 205;
        /// An update of an instance whose bootstrap committee is not adopted yet.
        INSTANCE_INACTIVE = 206;
        /// An update while a job whose signature verified waits for its adoption (`Continue` it).
        JOB_PENDING = 207;
        /// A record that already holds another header, or a statement another value.
        RECORD_CONFLICT = 208;
        /// Stored state that does not decode.
        BAD_STATE = 209;
        /// The instruction names another instance than the account's.
        WRONG_INSTANCE = 211;
        /// An invalid config.
        BAD_CONFIG = 212;
        /// An attestation asked of an account that holds no record.
        NO_RECORD = 213;
        /// A query the program cannot serve (a header sent for a record that needs none).
        BAD_QUERY = 214;
        /// A query without the block's RLP header for a record that binds a block hash only.
        HEADER_REQUIRED = 215;
        /// The claimed timestamp window is not the one the program sets.
        WRONG_WINDOW = 218;
        /// The claimed record is not the one the transaction writes (or one is claimed where none
        /// is written, or none where one is).
        WRONG_RECORD = 219;
        /// The claimed statements are not exactly the ones the query proves.
        WRONG_STATEMENT = 220;
        /// A residue witness of the wrong length.
        BAD_WITNESS = 221;
    }
}
