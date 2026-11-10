//! Messages, actor state and addresses of the Ferrata LEZ v0.4 program, shared by the guest and
//! the host.
//!
//! The program has four kinds of actor, each a public PDA of the program:
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

use borsh::{BorshDeserialize, BorshSerialize};
use ferrata_core::{
    fork::{ChainSpec, LcFork, Preset},
    Root,
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
    /// The store's light-client layout (`LcFork` as 0 = Altair … 4 = Gloas).
    pub store_fork: u8,
    /// How many transactions a committee adoption is split into (1, 2, 4, …).
    pub adoption_parts: u8,
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

    /// The store's layout, or `None` for an unknown byte.
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
}

/// `H(config ‖ checkpoint)`: an instance's id.
pub fn instance_id(config: &Config, checkpoint: &Root) -> Root {
    tagged_hash(b"instance-id", &[&borsh::to_vec(config).expect("borsh"), checkpoint])
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
    /// a `SignerWitness` encoding. Opens `job(id, job_id)`.
    Open { update: Vec<u8>, signers: Vec<u8> },
    /// Step 2, at the job: the Miller loops.
    Miller,
    /// One part of the job's committee adoption: `n / parts` uncompressed keys.
    Adopt { part: u8, keys: Vec<UncompressedKey> },
    /// Step 3, at the job: the final exponentiation, then `Apply` to the instance.
    Finish,
    /// Reply with this actor's state (`StateReply`), the read idiom of LEZ v0.4.
    Read,
    /// As `Read`, carrying a consumer's query. Only for showing what a call exposes (Stage 3):
    /// the program ignores the query.
    Attest { query: Vec<u8> },
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
    /// `instance(id)` → `header(id, slot)`: the record to write once (`HeaderRecord` encoding).
    Record { instance_id: Root, slot: u64, record: Vec<u8> },
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
    /// `HeaderRecord` encoding.
    pub record: Vec<u8>,
}

/// The event an instance emits when its finalized header advances or it activates.
pub const FINALIZED_SELECTOR: [u8; 8] = *b"FRT-FIN1";

/// Who exits with which codes. A handler's exit code is one byte, and 0 is success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeOwner {
    /// The core's refusals (`ferrata_core::Error`), which the program passes on.
    Core,
    /// The step API's own refusals (`ferrata_core::steps::StepError` other than `Core`).
    StepApi,
    /// This program's own refusals ([`code`]).
    Program,
    /// The Stage 3 consumer's refusals ([`consumer::code`]).
    Consumer,
}

/// The blocks of refusal codes. The program exits with its own codes and with the core's and the
/// step API's, so each code must have one owner: the blocks are disjoint, and every code lies in
/// a block of its owner (`harness/tests/exit_codes.rs` checks both). 240–255 are unassigned.
pub const CODE_BLOCKS: [(CodeOwner, core::ops::RangeInclusive<u8>); 5] = [
    (CodeOwner::Core, 1..=79),        // the light client: 1–61 in use
    (CodeOwner::StepApi, 80..=99),    // 80–84 in use
    (CodeOwner::Core, 100..=199),     // the execution layer: 100–163 in use
    (CodeOwner::Program, 200..=219),  // 200–212 in use
    (CodeOwner::Consumer, 220..=239), // 220–222 in use
];

/// Defines exit codes and `ALL`, every one of them with its name.
macro_rules! exit_codes {
    ($($(#[doc = $doc:literal])* $name:ident = $value:literal;)*) => {
        $($(#[doc = $doc])* pub const $name: u8 = $value;)*
        /// Every code of this module, with its name.
        pub const ALL: &[(&str, u8)] = &[$((stringify!($name), $name)),*];
    };
}

/// Exit codes of the program's own refusals: 200–212 (see [`CODE_BLOCKS`]).
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
        /// An internal message about another instance.
        WRONG_INSTANCE = 211;
        /// An unknown store layout in a config.
        BAD_CONFIG = 212;
    }
}

/// The minimal consumer of Stage 3 (`guests/consumer`): an actor of another program that reads a
/// `header(id, slot)` record, or calls the program with a query instead.
pub mod consumer {
    use borsh::{BorshDeserialize, BorshSerialize};
    use ferrata_core::Root;
    use lee_core::account::{AccountId, Actor};

    #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub enum ConsumerMessage {
        /// Read `header(id, slot)` of the Ferrata program at `ferrata` (`Read` → `StateReply`)
        /// and keep the record, with the consumer's own `query` (which it never sends), in this
        /// actor's state.
        ReadRecord { ferrata: AccountId, instance_id: Root, slot: u64, query: Vec<u8> },
        /// The same, but by calling the program with a query (`Attest`).
        CallAttest { ferrata: AccountId, instance_id: Root, slot: u64, query: Vec<u8> },
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    pub struct ConsumerState {
        /// The actor whose `StateReply` this actor expects.
        pub awaiting: Option<Actor>,
        /// The last record read (`RecordState` encoding).
        pub record: Option<Vec<u8>>,
        /// The query the consumer answers privately with the record it reads.
        pub query: Option<Vec<u8>>,
    }

    /// Refusal codes of the consumer: 220–222 (see [`crate::CODE_BLOCKS`]).
    pub mod code {
        exit_codes! {
            /// The message does not decode.
            BAD_MESSAGE = 220;
            /// The consumer's state does not decode.
            BAD_STATE = 221;
            /// The reply to a read is not a `StateReply` holding a record.
            BAD_REPLY = 222;
        }
    }
}
