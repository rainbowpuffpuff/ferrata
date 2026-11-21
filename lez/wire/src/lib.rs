//! Messages, actor state and addresses of the Ferrata LEZ v0.4 program, shared by the guest and
//! the host.
//!
//! The program has three kinds of actor, each a public PDA of the program:
//!
//! | actor | seed | state |
//! |---|---|---|
//! | `instance(id)` | `H("…/instance" ‖ id)` | [`InstanceState`]: config, active flag, the compact store |
//! | `job(id, job_id)` | `H("…/job" ‖ id ‖ job_id)` | [`JobState`]: one update (or the initial adoption) in progress |
//! | `header(id, slot)` | `H("…/header" ‖ id ‖ slot)` | [`RecordState`]: a finalized header, written once |
//!
//! `id = H("…/instance-id" ‖ config ‖ checkpoint)`, so nobody can occupy another chain's or
//! checkpoint's instance, and `job_id = H("…/job-id" ‖ id ‖ store digest ‖ update digest)`, so a
//! job names the store it started from and the update it verifies.
//!
//! Each message is borsh. Core values (store, update, witness, job, record) are carried in their
//! strict core encodings (`ferrata_core::witness`, `ferrata_core::steps`).
//!
//! [`attest`] holds the attestation interface: the request a consumer sends to `header(id, slot)`
//! (`Message::Attest`), the reply, and [`attest::evaluate`], the one function that verifies a
//! request against a record. The program runs it in its `Attest` handler; a private consumer runs
//! the same function inside its own proof on the record it read. [`consumer`] holds the reference
//! consumer's messages and state.

use borsh::{BorshDeserialize, BorshSerialize};
use ferrata_core::{
    fork::{ChainSpec, LcFork, Preset},
    witness::HeaderRecord,
    ws, Root,
};
use lee_core::{
    account::{AccountId, Actor},
    program::PdaSeed,
};
use sha2::{Digest, Sha256};

/// Version tag inside every hash this program derives.
const DOMAIN: &[u8] = b"ferrata-lez/v1/";

fn tagged_hash(tag: &[u8], parts: &[&[u8]]) -> Root {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(tag);
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// The chain an instance follows. Fixed at `Init`; part of the instance id.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Config {
    /// The execution chain id (EIP-155) of the chain. Every record of the instance carries it,
    /// and every statement identifier is derived from it. The program cannot check that it
    /// belongs to `genesis_validators_root`: nothing a light client verifies commits to it. It is
    /// part of the instance id a consumer pins, and the off-chain checkpoint verification
    /// compares the two.
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
    /// The light-client layout the store starts in, which is the bootstrap's (`LcFork` as
    /// 0 = Altair … 4 = Gloas). The store moves to a later layout when the chain reaches a later
    /// fork of `fork_epochs` (`ferrata_core::steps::store_for_update`); its current layout is in
    /// its encoding (`InstanceState::store`).
    pub store_fork: u8,
    /// How many transactions a committee adoption is split into (1, 2, 4, …).
    pub adoption_parts: u8,
    /// The weak-subjectivity period in epochs (`ferrata_core::ws`), computed off chain from the
    /// checkpoint state's total active balance with `ws::compute_weak_subjectivity_period`; not
    /// zero. Every transaction that creates or extends the instance is valid only before the
    /// period of the instance's finalized header ends, by the sequencer's clock
    /// ([`Config::ws_end_ms`]); a successor must keep it unchanged.
    pub ws_period_epochs: u64,
}

impl Config {
    /// The core's chain configuration.
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

    /// Unix milliseconds at which `slot` starts.
    pub fn slot_start_ms(&self, slot: u64) -> u64 {
        self.genesis_time.saturating_add(slot.saturating_mul(self.seconds_per_slot)).saturating_mul(1000)
    }

    /// The first slot outside the weak-subjectivity period of a finalized header at
    /// `finalized_slot` (`ws::weak_subjectivity_end_slot`); `None` if the period never ends.
    pub fn ws_end_slot(&self, finalized_slot: u64) -> Option<u64> {
        ws::weak_subjectivity_end_slot(finalized_slot, self.ws_period_epochs, &self.spec())
    }

    /// Unix milliseconds at which that slot starts: the exclusive end of the timestamp validity
    /// window of every transaction that creates or extends an instance whose finalized header is
    /// at `finalized_slot`.
    pub fn ws_end_ms(&self, finalized_slot: u64) -> Option<u64> {
        self.ws_end_slot(finalized_slot).map(|slot| self.slot_start_ms(slot))
    }
}

/// `H(config ‖ checkpoint)`: an instance's id.
pub fn instance_id(config: &Config, checkpoint: &Root) -> Root {
    tagged_hash(b"instance-id", &[&borsh::to_vec(config).expect("borsh"), checkpoint])
}

/// `H(root id ‖ parent id ‖ config ‖ checkpoint)`: the id of a successor instance of `parent_id`
/// whose checkpoint is the parent's latest record `checkpoint`. A domain of its own, so that no
/// `Init` can take a successor's id.
pub fn successor_id(root_id: &Root, parent_id: &Root, config: &Config, checkpoint: &Root) -> Root {
    tagged_hash(b"successor-id", &[root_id, parent_id, &borsh::to_vec(config).expect("borsh"), checkpoint])
}

/// `H(id ‖ store digest ‖ update digest)`: a job's id.
pub fn job_id(instance_id: &Root, store_digest: &Root, update_digest: &Root) -> Root {
    tagged_hash(b"job-id", &[instance_id, store_digest, update_digest])
}

pub fn instance_seed(instance_id: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"instance", &[instance_id]))
}

pub fn job_seed(instance_id: &Root, job_id: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"job", &[instance_id, job_id]))
}

pub fn header_seed(instance_id: &Root, slot: u64) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"header", &[instance_id, &slot.to_le_bytes()]))
}

/// `instance(id)` of the program deployed at `program`.
pub fn instance_actor(program: AccountId, instance_id: &Root) -> Actor {
    Actor::new(AccountId::for_public_pda(&program, &instance_seed(instance_id)), program)
}

/// `job(id, job_id)` of the program deployed at `program`.
pub fn job_actor(program: AccountId, instance_id: &Root, job_id: &Root) -> Actor {
    Actor::new(AccountId::for_public_pda(&program, &job_seed(instance_id, job_id)), program)
}

/// `header(id, slot)` of the program deployed at `program`.
pub fn header_actor(program: AccountId, instance_id: &Root, slot: u64) -> Actor {
    Actor::new(AccountId::for_public_pda(&program, &header_seed(instance_id, slot)), program)
}

/// An uncompressed public key (96 bytes).
pub type UncompressedKey = [u8; 96];

/// Every message the program handles.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Message {
    // --- Calls anyone may make (the transaction root, or another program) -----------------
    /// Create `instance(id)` from a bootstrap matching `checkpoint`, and open the job that adopts
    /// its committee. `bootstrap`: `CompactBootstrap` encoding.
    Init { config: Config, checkpoint: Root, bootstrap: Vec<u8> },
    /// Step 1 of an update, at `instance(id)`: `update` is a `CompactUpdate` encoding, `signers`
    /// a `SignerWitness` encoding. Opens `job(id, job_id)`. The update is in the store's layout,
    /// or in the layout of a later fork of the instance's schedule that its attested header has
    /// reached: the job then starts from the store upgraded to that layout, and applying it
    /// upgrades the instance's store.
    Open { update: Vec<u8>, signers: Vec<u8> },
    /// Step 2, at the job: the Miller loops.
    Miller,
    /// One part of the job's committee adoption: `n / parts` uncompressed keys.
    Adopt { part: u8, keys: Vec<UncompressedKey> },
    /// Step 3, at the job: the final exponentiation, then `Apply` to the instance.
    Finish,
    /// Reply with this actor's state (`StateReply`), the read idiom of LEZ v0.4.
    Read,
    /// As `Read`, carrying bytes the program ignores: for measuring what a message byte costs
    /// (`lez/RESULTS.md`, "What the LEZ v0.4 shell costs").
    ReadPadded { padding: Vec<u8> },
    /// At `header(id, slot)`: prove the facts of `request` under this record and reply to the
    /// caller with an [`attest::Attestation`] (their statement identifiers and values), or refuse
    /// with a code. Sent as the root of a transaction, with nobody to reply to, the statement
    /// identifiers are emitted as an event ([`ATTESTED_SELECTOR`]). Changes no state.
    Attest { request: attest::Request },
    /// The private placement (Stage 2): the whole verification of one update in one call, at any
    /// actor of this program (meant to run privately). `store` is the instance's `CompactStore`
    /// encoding; `adoption` the committee's keys when the update adopts one. Sends
    /// `ApplyVerified` to `instance(id)`.
    VerifyAll {
        config: Config,
        checkpoint: Root,
        store: Vec<u8>,
        update: Vec<u8>,
        signers: Vec<u8>,
        adoption: Vec<UncompressedKey>,
    },

    // --- Internal: accepted only from the expected actor of this program --------------------
    /// `instance(id)` → `job(id, job_id)`: the job step 1 produced (`Job` encoding).
    JobOpen { instance_id: Root, job_id: Root, committee_size: u32, job: Vec<u8> },
    /// `job(id, job_id)` → `instance(id)`: the verified job.
    Apply { instance_id: Root, job_id: Root, job: Vec<u8> },
    /// Any actor of this program → `instance(id)`: a job verified in one call by `VerifyAll`.
    ApplyVerified { instance_id: Root, job: Vec<u8> },
    /// `instance(id)` → `header(id, slot)`: the record to write once (`HeaderRecord` encoding),
    /// with the instance's chain id and root id.
    Record { instance_id: Root, slot: u64, chain_id: u64, record: Vec<u8>, root_id: Root },

    // --- Successors (`docs/design/attestation.md` §3.5); appended, so earlier tags keep their bytes
    /// At `instance(id)`, anyone: create a successor of this instance with `config`, whose
    /// checkpoint is this instance's latest record `checkpoint` (its finalized header's root). The
    /// instance checks the three invariants: `config` copies every immutable field of its own
    /// configuration except the fork schedule, which must agree on every fork at or before the
    /// checkpoint's epoch; `checkpoint` is its latest record; and that record is inside the
    /// weak-subjectivity period (the transaction is valid only before the period ends). Then it
    /// sends `Spawn` to `instance(successor_id(root id, id, config, checkpoint))` and emits
    /// [`SUCCEEDED_SELECTOR`].
    Succeed { config: Config, checkpoint: Root },
    /// `instance(parent_id)` → `instance(successor_id(..))`: create the successor, active, with the
    /// parent's store and root id.
    Spawn { root_id: Root, parent_id: Root, checkpoint: Root, config: Config, store: Vec<u8> },
}

/// `instance(id)`'s state.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct InstanceState {
    pub id: Root,
    pub config: Config,
    /// False until the initial committee is adopted.
    pub active: bool,
    /// `CompactStore` encoding.
    pub store: Vec<u8>,
    /// Jobs applied so far (including the activation).
    pub applied: u64,
    /// The root instance id consumers bind to: the instance's own id for an instance `Init`
    /// created, the parent's root id for a successor.
    pub root_id: Root,
    /// The instance this one succeeds; all zero for a root instance.
    pub parent_id: Root,
}

/// `job(id, job_id)`'s state.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct JobState {
    pub instance_id: Root,
    pub job_id: Root,
    pub committee_size: u32,
    /// `Job` encoding.
    pub job: Vec<u8>,
}

/// `header(id, slot)`'s state, written once.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RecordState {
    pub instance_id: Root,
    /// The instance's chain id (`Config::chain_id`).
    pub chain_id: u64,
    /// `HeaderRecord` encoding (`ferrata_core::witness::HeaderRecord`).
    pub record: Vec<u8>,
    /// The root id of the instance that wrote the record (`InstanceState::root_id`): a request
    /// bound to it is served as well as one bound to `instance_id`.
    pub root_id: Root,
}

impl RecordState {
    /// The record, decoded; `BAD_STATE` if the bytes are not a record.
    pub fn header(&self) -> Result<HeaderRecord, u8> {
        HeaderRecord::decode(&self.record).map_err(|_| code::BAD_STATE)
    }
}

/// The event an instance emits when its finalized header advances or it activates.
pub const FINALIZED_SELECTOR: [u8; 8] = *b"FRT-FIN1";

/// The event an instance emits when applying an update moves its store to a later fork's layout:
/// the layout it left and the layout it is in now (one byte each, as `Config::store_fork`).
pub const UPGRADED_SELECTOR: [u8; 8] = *b"FRT-UPG1";

/// The event an instance emits when it creates a successor (`Succeed`): the successor's id, the
/// root id, then the checkpoint's slot (little-endian).
pub const SUCCEEDED_SELECTOR: [u8; 8] = *b"FRT-SUC1";

/// The event `header(id, slot)` emits for an `Attest` sent as the root of a transaction: the
/// instance id, the chain id and the block number (little-endian), then each statement
/// identifier ([`attest::Attestation::event_data`]).
pub const ATTESTED_SELECTOR: [u8; 8] = *b"FRT-ATT1";

/// Who exits with which codes. A handler's exit code is one byte, and 0 is success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeOwner {
    /// The core's refusals (`ferrata_core::Error`), which the program passes on.
    Core,
    /// The step API's own refusals (`ferrata_core::steps::StepError` other than `Core`).
    StepApi,
    /// This program's own refusals ([`code`]).
    Program,
    /// The reference consumer's refusals ([`consumer::code`]).
    Consumer,
}

/// The blocks of refusal codes. The program exits with its own codes and with the core's and the
/// step API's, so each code must have one owner: the blocks are disjoint, and every code lies in
/// a block of its owner (`harness/tests/exit_codes.rs` checks both). 62–79 and 230–255 belong to
/// programs of their own (statements 62–72, rebate 73–79, holdings 230–239, the bridge 240–255).
pub const CODE_BLOCKS: [(CodeOwner, core::ops::RangeInclusive<u8>); 5] = [
    (CodeOwner::Core, 1..=61),        // the light client: 1–61, all in use
    (CodeOwner::StepApi, 80..=99),    // 80–84 in use
    (CodeOwner::Core, 100..=199),     // the execution layer: 100–163 in use
    (CodeOwner::Program, 200..=217),  // 200–217 in use
    (CodeOwner::Consumer, 218..=229), // 218–229 in use
];

/// Defines exit codes and `ALL`, every one of them with its name.
macro_rules! exit_codes {
    ($($(#[doc = $doc:literal])* $name:ident = $value:literal;)*) => {
        $($(#[doc = $doc])* pub const $name: u8 = $value;)*
        /// Every code of this module, with its name.
        pub const ALL: &[(&str, u8)] = &[$((stringify!($name), $name)),*];
    };
}

/// Exit codes of the program's own refusals: 200–217 (see [`CODE_BLOCKS`]).
pub mod code {
    exit_codes! {
        /// The message does not decode.
        BAD_MESSAGE = 200;
        /// The receiver is not the actor the message is for.
        WRONG_RECEIVER = 201;
        /// An internal message from an actor other than the expected one.
        WRONG_SENDER = 202;
        /// `Init` on an instance that exists.
        ALREADY_INITIALIZED = 203;
        /// No instance at the receiver.
        NO_INSTANCE = 204;
        /// No job at the receiver.
        NO_JOB = 205;
        /// An update for an instance whose initial committee is not adopted yet.
        INSTANCE_INACTIVE = 206;
        /// A job that is already open.
        JOB_EXISTS = 207;
        /// A record that already holds another header.
        RECORD_CONFLICT = 208;
        /// Stored state that does not decode.
        BAD_STATE = 209;
        /// A read with no one to reply to.
        UNROUTABLE = 210;
        /// A message about another instance than the receiver's: an internal message, or an
        /// attestation asked of a record of another instance than the caller is bound to.
        WRONG_INSTANCE = 211;
        /// An unknown store layout in a config.
        BAD_CONFIG = 212;
        /// An attestation asked of an actor that holds no record.
        NO_RECORD = 213;
        /// An attestation request the program cannot serve: an accumulator leaf whose root is
        /// not a storage fact of the same request, or a block header sent for a record that
        /// holds the payload's fields and needs none.
        BAD_QUERY = 214;
        /// An attestation asked, without the block's header, of a record that binds a block
        /// hash only (a finalized header in the Gloas layout): `Request::header` must carry the
        /// RLP header that hashes to it.
        HEADER_REQUIRED = 215;
        /// A successor whose configuration is not its parent's with another fork schedule: an
        /// immutable field changed (the chain, the preset, the genesis, the weak-subjectivity
        /// period, the adoption parts), a fork at or before the checkpoint's epoch changed, a
        /// store layout other than the parent store's, a schedule that is not in order, or no
        /// change at all.
        BAD_SUCCESSOR = 216;
        /// A successor from a checkpoint that is not the parent's latest record.
        STALE_CHECKPOINT = 217;
    }
}

/// The attestation interface: what a consumer asks of a record, what it gets back, and the one
/// function that verifies a request ([`evaluate`]).
///
/// A request names facts of one execution block and carries their Merkle-Patricia proofs: accounts
/// (with storage slots under them), event logs by their position in a transaction's receipt, and
/// leaves of a SHA-256 accumulator whose root one of the request's storage slots holds. Each fact
/// may carry conditions (`=, ≠, <, ≤, >, ≥` against a constant), and the request may relate two of
/// its facts to each other. The block is the record's own, or an older one reached from it by
/// EIP-2935 hops. Every root comes from the record: the request cannot supply one. A record of a
/// finalized header in the Gloas layout holds only the hash of the execution block it binds (the
/// bid's parent block); the request then carries that block's RLP header, which must hash to the
/// recorded hash, and the roots are read from it.
///
/// The reply gives, for each fact, its statement identifier (`ferrata_core::attest`,
/// `ferrata_core::accumulator`) and its values, with the instance, the chain id and the block they
/// were proven at. A refusal is an exit code: the core's own code for what failed, or one of
/// [`code`](crate::code).
pub mod attest {
    use borsh::{BorshDeserialize, BorshSerialize};
    use ferrata_core::{
        accumulator::{attest_accumulator_leaf, AccumulatorRoot},
        attest::{self as proven, history_hop},
        witness::{ExecutionSummary, HeaderRecord},
        Error, Root,
    };

    use crate::code;

    /// A 20-byte account address.
    pub type Address = [u8; 20];
    /// A 256-bit value, big-endian.
    pub type Word = [u8; 32];

    /// A comparison of two 256-bit unsigned values.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub enum Cmp {
        Eq,
        Ne,
        Lt,
        Le,
        Gt,
        Ge,
    }

    /// A field of a fact, as a 256-bit word (`ferrata_core::attest::Field`).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub enum Field {
        /// Account nonce.
        Nonce,
        /// Account balance.
        Balance,
        /// Account storage root.
        StorageRoot,
        /// Account code hash.
        CodeHash,
        /// Storage value.
        Value,
        /// The log's emitting address, left-padded to 32 bytes.
        Emitter,
        /// A log topic, by position.
        Topic(u8),
        /// The `i`-th 32-byte word of a log's data.
        DataWord(u32),
    }

    /// `field <cmp> rhs`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct Condition {
        pub field: Field,
        pub cmp: Cmp,
        pub rhs: Word,
    }

    /// A field of one of the request's facts, by the fact's position in the reply.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct Operand {
        pub fact: u32,
        pub field: Field,
    }

    /// `left <cmp> right` between two facts of one request.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct Relation {
        pub left: Operand,
        pub cmp: Cmp,
        pub right: Operand,
    }

    /// A storage slot of an [`AccountQuery`]'s account, with its proof (`storageProof[i].proof`
    /// of `eth_getProof`).
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct SlotQuery {
        pub slot: Word,
        pub proof: Vec<Vec<u8>>,
        pub conditions: Vec<Condition>,
    }

    /// An account, with its proof (`accountProof` of `eth_getProof`), and slots of its storage.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct AccountQuery {
        pub address: Address,
        pub proof: Vec<Vec<u8>>,
        pub conditions: Vec<Condition>,
        pub slots: Vec<SlotQuery>,
    }

    /// The log at position `log_index` of the receipt of transaction `tx_index`, with the
    /// receipt's proof in the block's receipts trie. `log_index` counts from zero within the
    /// receipt (`ferrata_core::exec::locate_log` converts JSON-RPC's block-wide `logIndex`).
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct LogQuery {
        pub tx_index: u64,
        pub log_index: u64,
        pub proof: Vec<Vec<u8>>,
        pub conditions: Vec<Condition>,
    }

    /// A leaf of the accumulator whose root is the value of the storage fact at position `root`
    /// of the reply, with its 32 siblings.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct LeafQuery {
        pub root: u32,
        pub leaf_index: u64,
        pub leaf: Root,
        pub path: Vec<Root>,
    }

    /// One EIP-2935 hop to an older block: the history contract's account proof and the proof of
    /// the older block's slot, both in the state of the block hopped from, and the older block's
    /// RLP header.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct Hop {
        pub account_proof: Vec<Vec<u8>>,
        pub storage_proof: Vec<Vec<u8>>,
        pub header: Vec<u8>,
    }

    /// What a consumer asks of `header(id, slot)`.
    #[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct Request {
        /// The instance the caller is bound to: a record of another instance refuses.
        pub instance_id: Root,
        /// The chain id the caller expects: a record of another chain refuses.
        pub chain_id: u64,
        /// The RLP header of the record's execution block, for a record that binds a block hash
        /// only (a finalized header in the Gloas layout). It must hash to the recorded hash; the
        /// block number, the state root and the receipts root are then read from it. `None` for
        /// a record that holds the payload's fields.
        pub header: Option<Vec<u8>>,
        /// Hops from the record's block to the block the facts are proven at, newest first.
        pub hops: Vec<Hop>,
        pub accounts: Vec<AccountQuery>,
        pub logs: Vec<LogQuery>,
        pub leaves: Vec<LeafQuery>,
        pub relations: Vec<Relation>,
    }

    /// A proven fact.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub enum Fact {
        /// An account's fields; all zero for an address without an account.
        Account { address: Address, nonce: u64, balance: Word, storage_root: Root, code_hash: Root },
        /// A storage slot's value; zero for an absent slot.
        Storage { address: Address, slot: Word, value: Word },
        /// A log of a transaction that succeeded.
        Log { tx_index: u64, log_index: u64, address: Address, topics: Vec<Root>, data: Vec<u8> },
        /// A leaf of an accumulator, and the root it was proven under.
        AccumulatorLeaf { address: Address, slot: Word, leaf_index: u64, leaf: Root, root: Root },
    }

    /// A fact with its statement identifier.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct Statement {
        pub id: Root,
        pub fact: Fact,
    }

    /// The reply to a request: its facts, in the order each account, then its slots, then the
    /// logs, then the leaves.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct Attestation {
        pub instance_id: Root,
        pub chain_id: u64,
        /// The beacon slot of the record the statements rest on.
        pub slot: u64,
        /// The execution block the facts were proven at: the record's, or the last hop's.
        pub block_number: u64,
        pub block_hash: Root,
        /// How many EIP-2935 hops lie between the record and that block.
        pub hops: u32,
        pub statements: Vec<Statement>,
    }

    impl Attestation {
        /// The data of the event emitted for a request sent as a transaction's root: the instance
        /// id, the chain id and the block number (little-endian), then each identifier.
        pub fn event_data(&self) -> Vec<u8> {
            let mut data = Vec::with_capacity(48 + 32 * self.statements.len());
            data.extend_from_slice(&self.instance_id);
            data.extend_from_slice(&self.chain_id.to_le_bytes());
            data.extend_from_slice(&self.block_number.to_le_bytes());
            for statement in &self.statements {
                data.extend_from_slice(&statement.id);
            }
            data
        }
    }

    impl From<Cmp> for proven::Cmp {
        fn from(cmp: Cmp) -> Self {
            match cmp {
                Cmp::Eq => proven::Cmp::Eq,
                Cmp::Ne => proven::Cmp::Ne,
                Cmp::Lt => proven::Cmp::Lt,
                Cmp::Le => proven::Cmp::Le,
                Cmp::Gt => proven::Cmp::Gt,
                Cmp::Ge => proven::Cmp::Ge,
            }
        }
    }

    impl From<Field> for proven::Field {
        fn from(field: Field) -> Self {
            match field {
                Field::Nonce => proven::Field::Nonce,
                Field::Balance => proven::Field::Balance,
                Field::StorageRoot => proven::Field::StorageRoot,
                Field::CodeHash => proven::Field::CodeHash,
                Field::Value => proven::Field::Value,
                Field::Emitter => proven::Field::Emitter,
                Field::Topic(i) => proven::Field::Topic(i),
                Field::DataWord(i) => proven::Field::DataWord(i),
            }
        }
    }

    fn conditions(conditions: &[Condition]) -> Vec<proven::Condition> {
        conditions
            .iter()
            .map(|c| proven::Condition {
                field: c.field.into(),
                predicate: proven::Predicate { cmp: c.cmp.into(), rhs: c.rhs },
            })
            .collect()
    }

    fn operand(operand: &Operand) -> proven::Operand {
        proven::Operand { fact: operand.fact as usize, field: operand.field.into() }
    }

    fn fact(fact: &proven::Fact<'_>) -> Fact {
        match fact {
            proven::Fact::Account(a) => Fact::Account {
                address: a.address,
                nonce: a.account.nonce,
                balance: a.account.balance,
                storage_root: a.account.storage_root,
                code_hash: a.account.code_hash,
            },
            proven::Fact::Storage(s) => Fact::Storage { address: s.address, slot: s.slot, value: s.value },
            proven::Fact::Log(l) => Fact::Log {
                tx_index: l.tx_index,
                log_index: l.log_index,
                address: l.log.address,
                topics: l.log.topics().to_vec(),
                data: l.log.data.to_vec(),
            },
        }
    }

    /// The exit code of a core refusal: the core's own code.
    fn refusal(error: Error) -> u8 {
        error.code() as u8
    }

    /// Verify `request` against the record `recorded` of the instance `instance_id` on chain
    /// `chain_id` (the fields of a `RecordState`), and return its statements.
    ///
    /// In order: the request must name this instance (`WRONG_INSTANCE`) and this chain
    /// (`ChainMismatch`); the record must give an execution block: its payload fields, with no
    /// header in the request (`BAD_QUERY` if there is one), or, for a record that binds a block
    /// hash only, the header in the request (`HEADER_REQUIRED` without one, `HeaderHashMismatch`
    /// if it does not hash to the recorded hash, the RLP decoder's code if it is not a header);
    /// a record with no execution block (the Altair layout, or a zero block hash) is
    /// `BindingMismatch`; each hop is verified (`ferrata_core::attest::history_hop`); the
    /// accounts, slots and logs are proven under the resulting block's state and receipts roots
    /// and their conditions and relations checked (`ferrata_core::attest::attest_batch`); each
    /// leaf is proven against the storage fact it names. The first refusal ends the call.
    pub fn evaluate(
        instance_id: &Root,
        chain_id: u64,
        recorded: &HeaderRecord,
        request: &Request,
    ) -> Result<Attestation, u8> {
        if request.instance_id != *instance_id {
            return Err(code::WRONG_INSTANCE);
        }
        // `attest_batch` checks the chain too; here a request for another chain is refused
        // before its header and its hops are looked at.
        if request.chain_id != chain_id {
            return Err(refusal(Error::ChainMismatch));
        }
        let mut record = match (&recorded.execution, &request.header) {
            (ExecutionSummary::BlockHash(_), Some(header)) => {
                proven::HeaderRecord::from_recorded_block_hash(recorded, chain_id, header)
            }
            (ExecutionSummary::BlockHash(_), None) => return Err(code::HEADER_REQUIRED),
            (ExecutionSummary::Payload { .. }, Some(_)) => return Err(code::BAD_QUERY),
            (ExecutionSummary::Payload { .. } | ExecutionSummary::None, _) => {
                proven::HeaderRecord::from_recorded(recorded, chain_id)
            }
        }
        .map_err(refusal)?;
        for hop in &request.hops {
            record = history_hop(&record, hop.account_proof.as_slice(), hop.storage_proof.as_slice(), &hop.header)
                .map_err(refusal)?;
        }

        let account_conditions: Vec<Vec<proven::Condition>> =
            request.accounts.iter().map(|a| conditions(&a.conditions)).collect();
        let slot_conditions: Vec<Vec<Vec<proven::Condition>>> =
            request.accounts.iter().map(|a| a.slots.iter().map(|s| conditions(&s.conditions)).collect()).collect();
        let log_conditions: Vec<Vec<proven::Condition>> =
            request.logs.iter().map(|l| conditions(&l.conditions)).collect();
        let slots: Vec<Vec<proven::SlotQuery<'_, Vec<u8>>>> = request
            .accounts
            .iter()
            .zip(&slot_conditions)
            .map(|(a, conditions)| {
                a.slots
                    .iter()
                    .zip(conditions)
                    .map(|(s, conditions)| proven::SlotQuery {
                        slot: s.slot,
                        proof: s.proof.as_slice(),
                        conditions: conditions.as_slice(),
                    })
                    .collect()
            })
            .collect();
        let accounts: Vec<proven::AccountQuery<'_, Vec<u8>>> = request
            .accounts
            .iter()
            .zip(&account_conditions)
            .zip(&slots)
            .map(|((a, conditions), slots)| proven::AccountQuery {
                address: a.address,
                proof: a.proof.as_slice(),
                conditions: conditions.as_slice(),
                slots: slots.as_slice(),
            })
            .collect();
        let logs: Vec<proven::LogQuery<'_, Vec<u8>>> = request
            .logs
            .iter()
            .zip(&log_conditions)
            .map(|(l, conditions)| proven::LogQuery {
                tx_index: l.tx_index,
                log_index: l.log_index,
                proof: l.proof.as_slice(),
                conditions: conditions.as_slice(),
            })
            .collect();
        let relations: Vec<proven::Relation> = request
            .relations
            .iter()
            .map(|r| proven::Relation { left: operand(&r.left), cmp: r.cmp.into(), right: operand(&r.right) })
            .collect();
        let batch =
            proven::Batch { accounts: accounts.as_slice(), logs: logs.as_slice(), relations: relations.as_slice() };
        let facts = proven::attest_batch(&record, request.chain_id, &batch).map_err(refusal)?;

        let mut statements: Vec<Statement> =
            facts.iter().map(|attested| Statement { id: attested.id, fact: fact(&attested.fact) }).collect();
        for leaf in &request.leaves {
            let Some(proven::Attested { fact: proven::Fact::Storage(held), .. }) = facts.get(leaf.root as usize) else {
                return Err(code::BAD_QUERY);
            };
            let root = AccumulatorRoot::restore(
                record.chain_id(),
                held.address,
                held.slot,
                held.value,
                record.number(),
                record.block_hash(),
            );
            let attested =
                attest_accumulator_leaf(&root, leaf.leaf_index, &leaf.leaf, leaf.path.as_slice()).map_err(refusal)?;
            statements.push(Statement {
                id: attested.id,
                fact: Fact::AccumulatorLeaf {
                    address: attested.fact.address,
                    slot: attested.fact.slot,
                    leaf_index: attested.fact.leaf_index,
                    leaf: attested.fact.leaf,
                    root: attested.fact.root,
                },
            });
        }
        let hops = match record.binding() {
            proven::Binding::History { hops } => hops,
            proven::Binding::PayloadHeader | proven::Binding::BlockHash => 0,
        };
        Ok(Attestation {
            instance_id: *instance_id,
            chain_id: record.chain_id(),
            slot: recorded.beacon.slot,
            block_number: record.number(),
            block_hash: record.block_hash(),
            hops,
            statements,
        })
    }
}

/// The reference consumer (`guests/consumer`), a program of its own that acts on Ethereum facts.
///
/// **A pool** is what the consumer credits against: the attestation program it trusts (by the
/// program's account), the instance and the chain id, and the one kind of fact it accepts as a
/// deposit, an ERC-20 `Transfer` of `token` to `recipient` ([`Pool`]). Nothing stores a pool:
/// every actor of a pool is addressed by the pool's hash, so a claim made under another program,
/// instance, chain or token lands in another pool's actors.
///
/// | actor | seed | state |
/// |---|---|---|
/// | `claim(pool, N)` | `H("…/claim" ‖ pool id ‖ N)` | [`ClaimState`]: the nullifier `N`, spent once |
/// | `balance(pool, owner)` | `H("…/balance" ‖ pool id ‖ owner)` | [`BalanceState`]: what `owner` was credited |
///
/// `N = H("…/nullifier" ‖ pool id ‖ statement identifier)`. A public claim is one transaction:
/// `Claim` at `claim(pool, N)` calls `Attest` at the pool's `header(id, slot)`; the reply, accepted
/// only from that actor, must carry the pool's instance and chain id and exactly one `Transfer`
/// of the pool's token to its recipient, and its identifier must hash to this actor's `N`; then
/// the sender's `balance(pool, from)` is credited, once. A refused attestation exits the whole
/// transaction with the program's code, so nothing is consumed.
///
/// Run at a private account's actor, `AttestPrivately` reads the record (`Read`) and verifies the
/// same fact with the same function ([`attest::evaluate`](crate::attest::evaluate)) inside the
/// consumer's own execution; the query, the proof and the result stay in its private state.
///
/// The module also keeps the minimal reader of Stage 3 (`ReadRecord`, `ReadPadded`, `CallAttest`).
pub mod consumer {
    use borsh::{BorshDeserialize, BorshSerialize};
    use ferrata_core::Root;
    use lee_core::{
        account::{AccountId, Actor},
        program::PdaSeed,
    };

    use crate::{
        attest::{Address, Attestation, Cmp, Condition, Fact, Field, Hop, LogQuery, Request, Statement, Word},
        tagged_hash,
    };
    use sha2::{Digest, Sha256};

    /// `keccak256("Transfer(address,address,uint256)")`, the first topic of an ERC-20 `Transfer`.
    pub const TRANSFER_TOPIC: Root = [
        0xdd, 0xf2, 0x52, 0xad, 0x1b, 0xe2, 0xc8, 0x9b, 0x69, 0xc2, 0xb0, 0x68, 0xfc, 0x37, 0x8d, 0xaa, 0x95, 0x2b,
        0xa7, 0xf1, 0x63, 0xc4, 0xa1, 0x16, 0x28, 0xf5, 0x5a, 0x4d, 0xf5, 0x23, 0xb3, 0xef,
    ];

    /// An address as an event topic or a predicate constant: left-padded to 32 bytes.
    pub fn address_word(address: &Address) -> Word {
        let mut word = [0u8; 32];
        word[12..].copy_from_slice(address);
        word
    }

    /// What a pool credits against. Its hash names every actor of the pool.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct Pool {
        /// The attestation program's account. Pinned: the pool only ever calls, and only ever
        /// accepts replies from, `header(id, slot)` of this program.
        pub ferrata: AccountId,
        /// The instance the pool is bound to.
        pub instance_id: Root,
        /// The chain id every statement must carry.
        pub chain_id: u64,
        /// The ERC-20 contract whose `Transfer` events are deposits.
        pub token: Address,
        /// The Ethereum address the deposits are transfers to.
        pub recipient: Address,
    }

    /// A deposit a pool accepted.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct Deposit {
        /// The log's statement identifier (`ferrata_core::attest::log_id`).
        pub statement_id: Root,
        /// The execution block of the transfer.
        pub block_number: u64,
        /// The Ethereum address the transfer came from: the owner of the credit.
        pub owner: Address,
        pub amount: u128,
    }

    impl Pool {
        /// `H("…/pool" ‖ pool)`.
        pub fn id(&self) -> Root {
            tagged_hash(b"consumer/pool", &[&borsh::to_vec(self).expect("borsh")])
        }

        /// The pool's acceptance rule as the program's predicates: the log is emitted by the
        /// token, is a `Transfer`, and its recipient is the pool's.
        pub fn conditions(&self) -> Vec<Condition> {
            vec![
                Condition { field: Field::Emitter, cmp: Cmp::Eq, rhs: address_word(&self.token) },
                Condition { field: Field::Topic(0), cmp: Cmp::Eq, rhs: TRANSFER_TOPIC },
                Condition { field: Field::Topic(2), cmp: Cmp::Eq, rhs: address_word(&self.recipient) },
            ]
        }

        /// The request for one log as a deposit of this pool; `header` is the block's RLP
        /// header when the record binds a block hash only ([`Request::header`]).
        pub fn request(&self, tx_index: u64, log_index: u64, proof: Vec<Vec<u8>>, header: Option<Vec<u8>>) -> Request {
            Request {
                instance_id: self.instance_id,
                chain_id: self.chain_id,
                header,
                logs: vec![LogQuery { tx_index, log_index, proof, conditions: self.conditions() }],
                ..Request::default()
            }
        }

        /// Check an attestation against the pool and read the deposit from it: the pool's instance
        /// and chain id (`BINDING_MISMATCH`), and exactly one statement, a three-topic `Transfer`
        /// of the token to the recipient with one data word (`UNEXPECTED_FACT`) that fits 128
        /// bits (`AMOUNT_TOO_LARGE`). The consumer checks the fact itself although it sent the
        /// same rule as predicates: it accepts a deposit on what the reply says, not on what was
        /// asked.
        pub fn accept(&self, attestation: &Attestation) -> Result<Deposit, u8> {
            if attestation.instance_id != self.instance_id || attestation.chain_id != self.chain_id {
                return Err(code::BINDING_MISMATCH);
            }
            let [Statement { id, fact: Fact::Log { address, topics, data, .. } }] = attestation.statements.as_slice()
            else {
                return Err(code::UNEXPECTED_FACT);
            };
            let [transfer, from, to] = topics.as_slice() else {
                return Err(code::UNEXPECTED_FACT);
            };
            if *address != self.token
                || *transfer != TRANSFER_TOPIC
                || *to != address_word(&self.recipient)
                || from[..12] != [0u8; 12]
                || data.len() != 32
            {
                return Err(code::UNEXPECTED_FACT);
            }
            let mut owner = [0u8; 20];
            owner.copy_from_slice(&from[12..]);
            if data[..16] != [0u8; 16] {
                return Err(code::AMOUNT_TOO_LARGE);
            }
            let mut amount = [0u8; 16];
            amount.copy_from_slice(&data[16..]);
            Ok(Deposit {
                statement_id: *id,
                block_number: attestation.block_number,
                owner,
                amount: u128::from_be_bytes(amount),
            })
        }
    }

    /// `H("…/nullifier" ‖ pool id ‖ statement identifier)`: spent once per pool and fact.
    ///
    /// It is a function of public data, so it names the deposit to anyone. That is right for a
    /// public claim and wrong for a private one: a private consumer that published it would link
    /// its transaction to the Ethereum log (`docs/design/attestation.md` §7). A plain `Transfer`
    /// carries no commitment to a secret, so a [`Pool`] has no private claim; a [`VaultPool`]
    /// has one, with [`secret_nullifier`].
    pub fn nullifier(pool_id: &Root, statement_id: &Root) -> Root {
        tagged_hash(b"consumer/nullifier", &[pool_id, statement_id])
    }

    pub fn claim_seed(pool_id: &Root, nullifier: &Root) -> PdaSeed {
        PdaSeed::new(tagged_hash(b"consumer/claim", &[pool_id, nullifier]))
    }

    pub fn balance_seed(pool_id: &Root, owner: &Address) -> PdaSeed {
        PdaSeed::new(tagged_hash(b"consumer/balance", &[pool_id, owner]))
    }

    /// `claim(pool, N)` of the consumer deployed at `program`.
    pub fn claim_actor(program: AccountId, pool_id: &Root, nullifier: &Root) -> Actor {
        Actor::new(AccountId::for_public_pda(&program, &claim_seed(pool_id, nullifier)), program)
    }

    /// `balance(pool, owner)` of the consumer deployed at `program`.
    pub fn balance_actor(program: AccountId, pool_id: &Root, owner: &Address) -> Actor {
        Actor::new(AccountId::for_public_pda(&program, &balance_seed(pool_id, owner)), program)
    }

    // ---------------------------------------------------------------------------------------------
    // Deposits of Ferrata's vault, claimed with the depositor's secret (design §7, rules 5 to 7)
    // ---------------------------------------------------------------------------------------------

    /// `keccak256("Deposit(address,uint64,uint64,address,uint256,bytes32)")`, the first topic of
    /// FerrataVault's `Deposit(depositor indexed, index, leafIndex, token indexed, amount, C)`.
    pub const DEPOSIT_TOPIC: Root = [
        0x83, 0xd2, 0xd1, 0xc3, 0x4c, 0x88, 0x25, 0x84, 0x3c, 0x9a, 0xf8, 0x4b, 0xec, 0x80, 0x37, 0x8e, 0x99, 0x70,
        0x84, 0xe8, 0xc1, 0xd6, 0xd5, 0xa3, 0xc2, 0xa6, 0xe7, 0x58, 0xfe, 0x05, 0x53, 0x8e,
    ];
    /// The destination kinds a commitment binds (the bridge's bytes, SPEC-v2 §3.3).
    pub const KIND_PRIVATE: u8 = 0x01;
    pub const KIND_PUBLIC: u8 = 0x02;

    /// Rule 5's commitment, as the vault's depositors make it for the bridge:
    /// `C = sha256("ferrata/C/v1" ‖ s ‖ kind ‖ dest)`, 77 fixed-width bytes, one opening.
    pub fn commitment(s: &Root, kind: u8, dest: &Root) -> Root {
        Sha256::new()
            .chain_update(b"ferrata/C/v1")
            .chain_update(s)
            .chain_update([kind])
            .chain_update(dest)
            .finalize()
            .into()
    }

    /// Rule 6's nullifier: `H("…/consumer/nf" ‖ s ‖ statement identifier)`. Keyed on the
    /// identifier (S.4), unlinkable without the secret, and the same on the public and the private
    /// path, so one deposit is claimed once whichever path is taken.
    pub fn secret_nullifier(s: &Root, statement_id: &Root) -> Root {
        tagged_hash(b"consumer/nf", &[s, statement_id])
    }

    pub fn holder_seed(pool_id: &Root, dest: &Root) -> PdaSeed {
        PdaSeed::new(tagged_hash(b"consumer/holder", &[pool_id, dest]))
    }

    /// `holder(pool, dest)`: the public credit of a public-kind deposit's destination.
    pub fn holder_actor(program: AccountId, pool_id: &Root, dest: &Root) -> Actor {
        Actor::new(AccountId::for_public_pda(&program, &holder_seed(pool_id, dest)), program)
    }

    /// A pool of the vault's deposits of one token: the reference consumer of U.6 with the
    /// Recommended Consumer Pattern. `token = 0x00…00` is the native-ETH consumer (an ETH
    /// transfer to the vault contract), any other the ERC-20 consumer.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct VaultPool {
        /// The attestation program's account, pinned as in [`Pool`].
        pub ferrata: AccountId,
        pub instance_id: Root,
        pub chain_id: u64,
        /// The vault whose `Deposit` events are the deposits (rule 7: the only fact accepted).
        pub vault: Address,
        /// The token deposited; `0x00…00` for native ETH.
        pub token: Address,
    }

    /// Where a deposit's log is: the record block's RLP header (a Gloas record), the EIP-2935
    /// hops from it to the deposit's block, and the log with its receipt's proof there.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct DepositProof {
        pub header: Option<Vec<u8>>,
        pub hops: Vec<Hop>,
        pub tx_index: u64,
        pub log_index: u64,
        pub proof: Vec<Vec<u8>>,
    }

    /// A vault deposit a pool accepted.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct VaultDeposit {
        /// The log's statement identifier: what the nullifier is keyed on.
        pub statement_id: Root,
        pub block_number: u64,
        pub depositor: Address,
        pub leaf_index: u64,
        pub amount: u128,
        /// The commitment `C` the depositor made.
        pub commitment: Root,
    }

    impl VaultPool {
        /// `H("…/vault-pool" ‖ pool)`.
        pub fn id(&self) -> Root {
            tagged_hash(b"consumer/vault-pool", &[&borsh::to_vec(self).expect("borsh")])
        }

        /// The acceptance rule as the program's predicates: emitted by the vault, a `Deposit`, of
        /// the pool's token.
        pub fn conditions(&self) -> Vec<Condition> {
            vec![
                Condition { field: Field::Emitter, cmp: Cmp::Eq, rhs: address_word(&self.vault) },
                Condition { field: Field::Topic(0), cmp: Cmp::Eq, rhs: DEPOSIT_TOPIC },
                Condition { field: Field::Topic(2), cmp: Cmp::Eq, rhs: address_word(&self.token) },
            ]
        }

        /// The request for one deposit log.
        pub fn request(&self, deposit: &DepositProof) -> Request {
            Request {
                instance_id: self.instance_id,
                chain_id: self.chain_id,
                header: deposit.header.clone(),
                hops: deposit.hops.clone(),
                logs: vec![LogQuery {
                    tx_index: deposit.tx_index,
                    log_index: deposit.log_index,
                    proof: deposit.proof.clone(),
                    conditions: self.conditions(),
                }],
                ..Request::default()
            }
        }

        /// Check an attestation against the pool and read the deposit from it: the binding
        /// (`BINDING_MISMATCH`); exactly one statement, a `Deposit` log of the vault for the
        /// token, three topics and four data words (`UNEXPECTED_FACT`, rule 7); an amount that fits
        /// 128 bits (`AMOUNT_TOO_LARGE`).
        pub fn accept(&self, attestation: &Attestation) -> Result<VaultDeposit, u8> {
            if attestation.instance_id != self.instance_id || attestation.chain_id != self.chain_id {
                return Err(code::BINDING_MISMATCH);
            }
            let [Statement { id, fact: Fact::Log { address, topics, data, .. } }] = attestation.statements.as_slice()
            else {
                return Err(code::UNEXPECTED_FACT);
            };
            let [event, depositor, token] = topics.as_slice() else {
                return Err(code::UNEXPECTED_FACT);
            };
            if *address != self.vault
                || *event != DEPOSIT_TOPIC
                || *token != address_word(&self.token)
                || depositor[..12] != [0u8; 12]
                || data.len() != 4 * 32
                || data[32..56] != [0u8; 24]
            {
                return Err(code::UNEXPECTED_FACT);
            }
            if data[64..80] != [0u8; 16] {
                return Err(code::AMOUNT_TOO_LARGE);
            }
            let mut owner = [0u8; 20];
            owner.copy_from_slice(&depositor[12..]);
            let mut leaf_index = [0u8; 8];
            leaf_index.copy_from_slice(&data[56..64]);
            let mut amount = [0u8; 16];
            amount.copy_from_slice(&data[80..96]);
            let mut c = [0u8; 32];
            c.copy_from_slice(&data[96..128]);
            Ok(VaultDeposit {
                statement_id: *id,
                block_number: attestation.block_number,
                depositor: owner,
                leaf_index: u64::from_be_bytes(leaf_index),
                amount: u128::from_be_bytes(amount),
                commitment: c,
            })
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub enum ConsumerMessage {
        // --- The minimal reader of Stage 3 -------------------------------------------------------
        /// Read `header(id, slot)` of the Ferrata program at `ferrata` (`Read` → `StateReply`)
        /// and keep the record, with the consumer's own `query` (which it never sends), in this
        /// actor's state.
        ReadRecord { ferrata: AccountId, instance_id: Root, slot: u64, query: Vec<u8> },
        /// The same read with `padding` sent along (`ReadPadded`), for the shell-cost measurement.
        ReadPadded { ferrata: AccountId, instance_id: Root, slot: u64, padding: Vec<u8> },
        /// Call `Attest` at `header(request.instance_id, slot)` with `request`, instead of
        /// reading, and keep the reply. Run privately, this publishes the request.
        CallAttest { ferrata: AccountId, slot: u64, request: Request },

        // --- The reference consumer --------------------------------------------------------------
        /// At `claim(pool, N)`: claim the log at `(tx_index, log_index)` of the block recorded at
        /// `slot` as a deposit of `pool`, and credit its sender once. `header`: the block's RLP
        /// header, for a record that binds a block hash only ([`Request::header`]).
        Claim { pool: Pool, slot: u64, tx_index: u64, log_index: u64, proof: Vec<Vec<u8>>, header: Option<Vec<u8>> },
        /// At a private account's actor: read the record of `slot`, verify the same fact there,
        /// and keep it in this actor's state. Sends nothing but `Read`.
        AttestPrivately {
            pool: Pool,
            slot: u64,
            tx_index: u64,
            log_index: u64,
            proof: Vec<Vec<u8>>,
            header: Option<Vec<u8>>,
        },

        // --- Internal: accepted only from the expected actor of this program ---------------------
        /// `claim(pool, N)` → `balance(pool, owner)`: credit a claimed deposit.
        Credit { pool_id: Root, nullifier: Root, owner: Address, amount: u128 },

        // --- Vault deposits with the Recommended Consumer Pattern (rules 5 to 7) ----------------
        /// At `claim(pool, N)`, `N = secret_nullifier(s, statement id)`: the public claim of a
        /// public-kind deposit. It opens the commitment (`s`, `kind`, `dest` are published) and
        /// credits `holder(pool, dest)` once.
        ClaimDeposit { pool: VaultPool, slot: u64, deposit: DepositProof, s: Root, kind: u8, dest: Root },
        /// At the claimant's own actor, meant to run privately: read the record of `slot`, verify
        /// the deposit inside this execution, open its commitment, and spend `N` at
        /// `claim(pool, N)`. A private-kind deposit is credited to this account's own state (it
        /// must be `dest`); a public-kind one to `holder(pool, dest)`. Sends `Read` and `Spend`.
        ClaimDepositPrivately { pool: VaultPool, slot: u64, deposit: DepositProof, s: Root, kind: u8, dest: Root },
        /// Internal, from an actor of this program after a verified claim → `claim(pool, N)`:
        /// spend `N` once; then credit `holder(pool, dest)` if `credit` names it.
        Spend { pool_id: Root, nullifier: Root, credit: Option<(Root, u128)> },
        /// Internal, `claim(pool, N)` → `holder(pool, dest)`: credit a claimed deposit.
        CreditHolder { pool_id: Root, nullifier: Root, dest: Root, amount: u128 },
    }

    /// The state of an actor of the consumer, by the actor's role.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub enum ConsumerState {
        Reader(ReaderState),
        Claim(ClaimState),
        Balance(BalanceState),
        Private(PrivateState),
        /// `claim(pool, N)` of a vault pool.
        Nullifier(NullifierState),
        /// `holder(pool, dest)`.
        Holder(HolderState),
        /// A claimant's own actor (a private account's, when run privately).
        Claimant(ClaimantState),
    }

    /// `claim(pool, N)` of a vault pool. `Pending` lives within one transaction and is decoded
    /// once per handler execution, so its size beside `Spent` costs nothing; boxing would only
    /// complicate the handlers' patterns.
    #[allow(clippy::large_enum_variant)]
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub enum NullifierState {
        /// A public claim waits for its attestation (within a transaction only).
        Pending { pool: VaultPool, header: Actor, s: Root, kind: u8, dest: Root },
        /// `N` is spent. The same on both paths: nothing here says which path spent it.
        Spent { pool_id: Root },
    }

    /// `holder(pool, dest)`.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct HolderState {
        pub pool_id: Root,
        pub dest: Root,
        pub amount: u128,
        pub credits: u64,
    }

    /// A claimant's own actor.
    #[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct ClaimantState {
        /// The record actor whose reply this actor expects.
        pub awaiting: Option<Actor>,
        /// The claim waiting for that record; cleared when the reply arrives.
        pub pending: Option<PendingDeposit>,
        /// Private-kind credits: (pool id, amount), one entry per pool.
        pub holdings: Vec<(Root, u128)>,
        /// How many deposits this actor claimed.
        pub claims: u64,
    }

    /// A private claim between its `Read` and the record's reply.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct PendingDeposit {
        pub pool: VaultPool,
        pub deposit: DepositProof,
        pub s: Root,
        pub kind: u8,
        pub dest: Root,
    }

    /// A Stage 3 reader.
    #[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct ReaderState {
        /// The actor whose reply this actor expects.
        pub awaiting: Option<Actor>,
        /// Whether that reply answers a call (`Attest`) rather than a read.
        pub calling: bool,
        /// The last record read (`RecordState` encoding).
        pub record: Option<Vec<u8>>,
        /// The query the consumer answers privately with the record it reads.
        pub query: Option<Vec<u8>>,
        /// The last reply to `CallAttest` (`Attestation` encoding).
        pub attestation: Option<Vec<u8>>,
    }

    /// `claim(pool, N)`.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub enum ClaimState {
        /// The attestation was asked for and its reply is outstanding. Only within a transaction:
        /// the reply arrives, or the transaction reverts.
        Pending { pool: Pool, header: Actor },
        /// The deposit was credited; the nullifier is spent.
        Spent(Deposit),
    }

    /// `balance(pool, owner)`.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct BalanceState {
        pub pool_id: Root,
        pub owner: Address,
        pub amount: u128,
        /// How many deposits were credited.
        pub credits: u64,
    }

    /// A private account's actor.
    #[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct PrivateState {
        /// The record actor whose reply this actor expects.
        pub awaiting: Option<Actor>,
        /// The query waiting for that record; cleared when the reply arrives, so that what the
        /// actor keeps has the same size whatever the proof was.
        pub pending: Option<PendingAttest>,
        /// The last deposit attested, and the pool it belongs to.
        pub last: Option<(Root, Deposit)>,
        /// How many facts this actor attested.
        pub attested: u64,
    }

    /// A private query between its `Read` and the record's reply.
    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct PendingAttest {
        pub pool: Pool,
        pub tx_index: u64,
        pub log_index: u64,
        pub proof: Vec<Vec<u8>>,
        pub header: Option<Vec<u8>>,
    }

    impl ConsumerState {
        /// The actor whose reply this state waits for.
        pub fn awaiting(&self) -> Option<Actor> {
            match self {
                ConsumerState::Reader(reader) => reader.awaiting,
                ConsumerState::Claim(ClaimState::Pending { header, .. }) => Some(*header),
                ConsumerState::Private(private) => private.awaiting,
                ConsumerState::Nullifier(NullifierState::Pending { header, .. }) => Some(*header),
                ConsumerState::Claimant(claimant) => claimant.awaiting,
                ConsumerState::Claim(ClaimState::Spent(_))
                | ConsumerState::Balance(_)
                | ConsumerState::Nullifier(NullifierState::Spent { .. })
                | ConsumerState::Holder(_) => None,
            }
        }
    }

    /// Refusal codes of the consumer: 218–229 (see [`crate::CODE_BLOCKS`]). 230–239 are
    /// `ferrata_holdings`'s and 240–255 the bridge program's (programs of their own).
    pub mod code {
        exit_codes! {
            /// The message does not decode.
            BAD_MESSAGE = 220;
            /// The actor's state does not decode, or is another role's.
            BAD_STATE = 221;
            /// A reply that is not what was asked for: not a `StateReply` holding a record, or
            /// not an attestation.
            BAD_REPLY = 222;
            /// A claim of a deposit that was already credited: its nullifier is spent.
            ALREADY_CLAIMED = 223;
            /// The receiver is not the actor the message is for; also a private claim not run by
            /// its account's owner, or a private-kind deposit claimed by an account other than its
            /// destination.
            WRONG_RECEIVER = 224;
            /// An internal message from an actor other than the expected one.
            WRONG_SENDER = 225;
            /// An attestation of another instance or chain than the pool's.
            BINDING_MISMATCH = 226;
            /// An attestation that is not exactly one deposit of the pool: one ERC-20 `Transfer`
            /// of its token to its recipient.
            UNEXPECTED_FACT = 227;
            /// A transferred amount, or a balance, that does not fit 128 bits.
            AMOUNT_TOO_LARGE = 228;
            /// The claim actor is not the one the attested statement's nullifier names.
            NULLIFIER_MISMATCH = 229;
            /// The opening (`s`, `kind`, `dest`) does not hash to the deposit's commitment.
            COMMITMENT_MISMATCH = 218;
            /// A destination kind this path does not take: a public claim of a private-kind
            /// deposit, or a byte that is no kind.
            WRONG_KIND = 219;
        }
    }
}
