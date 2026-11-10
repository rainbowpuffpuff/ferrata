//! Drives the Ferrata LEZ program through LEZ v0.4.0-rc1's state machine.
//!
//! Every transaction is a signed, fee-paying public transaction settled by [`settle`], LEZ's
//! sequencer settlement (reserve, metered action, refund) over `lee`'s state transition
//! functions, on a state holding LEZ's fee program and system accounts. [`trace`] replays each
//! action to split its cycles by transition. The program is deployed at genesis, immutable, as
//! LEZ's own tests deploy programs (`V03State::with_genesis_programs`).

pub mod private;
pub mod scenarios;
pub mod settle;
pub mod trace;

use std::{borrow::Cow, collections::BTreeMap, fs, path::PathBuf};

use fee_core::{market, BlockFeeSummary};
use ferrata_core::{
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    steps::{self, Job},
    store::{ApplyOutcome, StoreCore},
    types::{LightClientBootstrap, LightClientUpdate, SyncCommittee},
    witness::{uncompressed_keys, CompactBootstrap, CompactStore, CompactUpdate, SignerWitness},
    Root,
};
use ferrata_lez_wire::{
    header_actor, instance_actor, instance_id, instance_seed, job_actor, job_id, Config, InstanceState, JobState,
    Message, RecordState,
};
use lee::{
    program::Program,
    public_transaction::{Message as PublicMessage, WitnessSet},
    Account, AccountId, Actor, FeeDeclaration, PrivateKey, PublicAccountEvidence, PublicKey, PublicTransaction,
    V03State,
};
use lee_core::{
    account::{ActorState, Nonce},
    program::TransactionEvent,
};
use settle::{classify, opening_fee_state, settle_charged_transaction, Charge, LeeTransaction};
use sha2::{Digest, Sha256};
use trace::{trace_public, TransitionTrace};

/// A second deployment of the program's image, at another address: a distinct program for
/// dispatch and PDA derivation (as `TWIN` in LEZ's own state tests).
pub const TWIN: AccountId = AccountId::new([0x5c; 32]);

pub fn ferrata_program() -> Program {
    Program::new_unchecked(methods::FERRATA_LEZ_ID, Cow::Borrowed(methods::FERRATA_LEZ_ELF))
}

/// Where the genesis deployment of the program lives.
pub fn ferrata_account() -> AccountId {
    AccountId::from_builtin_program(methods::FERRATA_LEZ_ID)
}

pub fn consumer_program() -> Program {
    Program::new_unchecked(methods::FERRATA_CONSUMER_ID, Cow::Borrowed(methods::FERRATA_CONSUMER_ELF))
}

pub fn consumer_account() -> AccountId {
    AccountId::from_builtin_program(methods::FERRATA_CONSUMER_ID)
}

pub fn scripted_program() -> Program {
    Program::new_unchecked(methods::SCRIPTED_ID, Cow::Borrowed(methods::SCRIPTED_ELF))
}

pub fn scripted_account() -> AccountId {
    AccountId::from_builtin_program(methods::SCRIPTED_ID)
}

/// The image id as hex (little-endian words, as risc0 prints a `Digest`).
pub fn image_id_hex(id: [u32; 8]) -> String {
    hex::encode(id.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>())
}

// ---------------------------------------------------------------------------------------------
// Mainnet data
// ---------------------------------------------------------------------------------------------

/// Mainnet `GENESIS_TIME`.
pub const MAINNET_GENESIS_TIME: u64 = 1_606_824_023;
pub const GENESIS_VALIDATORS_ROOT: &str = "4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
pub const TRUSTED_BLOCK_ROOT: &str = "64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac";
/// Finalized (slot, block root) after each update, as a second provider reports them.
pub const EXPECTED_FINALIZED: [(u64, &str); 5] = [
    (15_556_608, "64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac"),
    (15_564_800, "96949350abfe8a04e6f1ae22b3064fdcee177dec32133348a2a69d200151e09a"),
    (15_572_992, "7e2771b1939afa205540003849bca0b0aa6ee71dd5a767cca2e35e084a008a4c"),
    (15_581_184, "e7c0ae95dd6a82d9eca7d9dc4e1b42a4c8197fbb0e4cd880bc5f9631de2b992b"),
    (15_589_599, "5d1200f2c031f01aa2c3385c881ca6cac727c23d3080e0ac53169160c8ddd573"),
];

pub fn root(hex_str: &str) -> Root {
    hex::decode(hex_str).unwrap().try_into().unwrap()
}

/// Mainnet fork schedule, from `configs/mainnet.yaml` at consensus-specs v1.7.0-beta.4.
pub fn mainnet_spec() -> ChainSpec {
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [
            [0, 0, 0, 0],
            [1, 0, 0, 0],
            [2, 0, 0, 0],
            [3, 0, 0, 0],
            [4, 0, 0, 0],
            [5, 0, 0, 0],
            [6, 0, 0, 0],
            [7, 0, 0, 0],
            [8, 0, 0, 0],
        ],
        epochs: [0, 74_240, 144_896, 194_048, 269_568, 364_032, 411_392, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    }
}

/// An instance config for mainnet, Electra/Fulu layout, adoption in `parts` transactions.
pub fn mainnet_config(parts: u8) -> Config {
    let spec = mainnet_spec();
    Config {
        sync_committee_size: 512,
        slots_per_epoch: spec.preset.slots_per_epoch,
        epochs_per_sync_committee_period: spec.preset.epochs_per_sync_committee_period,
        min_sync_committee_participants: spec.preset.min_sync_committee_participants,
        fork_versions: spec.versions,
        fork_epochs: spec.epochs,
        genesis_validators_root: root(GENESIS_VALIDATORS_ROOT),
        genesis_time: MAINNET_GENESIS_TIME,
        seconds_per_slot: 12,
        store_fork: 3,
        adoption_parts: parts,
    }
}

/// The mainnet fixtures: `FERRATA_FIXTURE_DIR` if set (to run a built binary elsewhere), else
/// `crates/ferrata-spectests/fixtures/mainnet-p1899` of this checkout.
pub fn fixture_dir() -> PathBuf {
    std::env::var_os("FERRATA_FIXTURE_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/ferrata-spectests/fixtures/mainnet-p1899")
    })
}

/// The fixtures' pinned `SHA256SUMS`, compiled in, so that another fixture directory cannot
/// change what the fixtures must hash to.
const FIXTURE_SUMS: &str = include_str!("../../../crates/ferrata-spectests/fixtures/mainnet-p1899/SHA256SUMS");

/// Read a fixture and check it against the pinned `SHA256SUMS`.
pub fn fixture(name: &str) -> Vec<u8> {
    let path = fixture_dir().join(name);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let want = FIXTURE_SUMS.lines().find(|l| l.ends_with(name)).unwrap().split_whitespace().next().unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{name} does not match SHA256SUMS");
    bytes
}

pub fn mainnet_bootstrap() -> LightClientBootstrap {
    LightClientBootstrap::decode(&fixture("bootstrap.ssz"), LcFork::Electra, 512).unwrap()
}

pub fn mainnet_updates() -> Vec<LightClientUpdate> {
    (0..5)
        .map(|i| LightClientUpdate::decode(&fixture(&format!("update_{i}.ssz")), LcFork::Electra, 512).unwrap())
        .collect()
}

/// `update` without its next committee: a finality update with the same signature.
pub fn as_finality_update(update: &LightClientUpdate) -> LightClientUpdate {
    let mut u = update.clone();
    u.next_sync_committee = SyncCommittee::empty(512);
    u.next_sync_committee_branch = vec![[0; 32]; u.next_sync_committee_branch.len()];
    u
}

// ---------------------------------------------------------------------------------------------
// The chain
// ---------------------------------------------------------------------------------------------

/// One settled transaction.
#[derive(Clone, Debug)]
pub struct TxReport {
    pub label: String,
    pub root: Actor,
    /// The transaction's borsh size as `LeeTransaction` (its storage gas).
    pub tx_bytes: usize,
    /// The root message's size.
    pub message_bytes: usize,
    pub charge: Charge,
    pub transitions: Vec<TransitionTrace>,
    pub events: Vec<TransactionEvent>,
    /// The program's exit code if the action was refused with one.
    pub exit_code: Option<u32>,
}

impl TxReport {
    pub fn ok(&self) -> bool {
        self.charge.revert.is_none()
    }
}

/// A LEZ state with the fee program, a funded fee payer, the Ferrata program (and its twin), the
/// consumer and LEZ's `scripted` test program. Each transaction is settled in a block of its own.
pub struct Chain {
    pub state: V03State,
    payer_key: PrivateKey,
    pub payer: AccountId,
    nonce: u128,
    pub block_id: u64,
    /// Every report, in order.
    pub reports: Vec<TxReport>,
}

impl Default for Chain {
    fn default() -> Self {
        Self::new()
    }
}

impl Chain {
    pub fn new() -> Self {
        let payer_key = PrivateKey::try_new([0x11; 32]).unwrap();
        let payer = AccountId::from(&PublicKey::new_from_private_key(&payer_key));
        let state = V03State::new()
            .with_public_accounts([
                (system_accounts::fee_state_account_id(), system_accounts::fee_state_account()),
                (system_accounts::fee_escrow_account_id(), Account::default()),
                (system_accounts::fee_inbox_account_id(), Account::default()),
                (payer, Account::funded(1 << 100)),
            ])
            .with_named_programs([(programs::fee_account_id(), programs::fee()), (TWIN, ferrata_program())])
            .with_genesis_programs([(ferrata_program(), true), (consumer_program(), true), (scripted_program(), true)]);
        Self { state, payer_key, payer, nonce: 0, block_id: 2, reports: Vec::new() }
    }

    /// The state of `actor` (empty if none).
    pub fn actor_state(&self, actor: Actor) -> ActorState {
        self.state.get_account_by_id(actor.account_id).data.actor_state(actor.program_account_id).clone()
    }

    /// Sign, settle and record one public transaction whose root message is `message`.
    pub fn submit(
        &mut self,
        label: &str,
        root: Actor,
        message: Vec<u8>,
        actors: Vec<Actor>,
        evidence: Vec<PublicAccountEvidence>,
        timestamp_ms: u64,
    ) -> TxReport {
        let gas_limit = market::MAX_GAS_EXEC;
        let fee = FeeDeclaration::new(self.payer, gas_limit, 0, u128::MAX >> 8);
        let message_bytes = message.len();
        let msg = PublicMessage::new(
            root,
            message,
            actors,
            BTreeMap::from([(self.payer, Nonce(self.nonce))]),
            Some(fee),
            evidence,
        );
        let witness_set = WitnessSet::for_message(&msg, &[&self.payer_key]);
        let tx = PublicTransaction::new(msg, witness_set);
        let (transitions, _) = trace_public(&tx, &self.state, gas_limit);
        let lee_tx = LeeTransaction::Public(tx);
        let view = classify(&lee_tx).expect("classifies");
        let opening = opening_fee_state(&self.state);
        let mut summary = BlockFeeSummary::default();
        let (events, charge) = settle_charged_transaction(
            &lee_tx,
            &view,
            &mut self.state,
            &opening,
            self.block_id,
            timestamp_ms,
            &mut summary,
        )
        .unwrap_or_else(|e| panic!("{label}: settlement rejected the transaction: {e}"));
        self.nonce += 1;
        self.block_id += 1;
        let traced: u64 = transitions.iter().map(|t| t.cycles).sum();
        let exit_code = transitions.iter().map(|t| t.exit_code).find(|c| *c != 0);
        // LEZ charges what it metered on success and on a coded exit; any other failure (a panic,
        // an unadmitted actor, a validity window) is charged the whole gas limit.
        if charge.revert.is_none() || exit_code.is_some() {
            assert_eq!(traced, charge.cycles, "{label}: the transitions must sum to the cycles LEZ metered");
        } else {
            assert_eq!(charge.cycles, gas_limit, "{label}: an uncoded failure is charged the gas limit");
        }
        let report = TxReport {
            label: label.to_string(),
            root,
            tx_bytes: view.gas_stor() as usize,
            message_bytes,
            charge,
            transitions,
            events,
            exit_code,
        };
        self.reports.push(report.clone());
        report
    }
}

// ---------------------------------------------------------------------------------------------
// An instance and its jobs, as a submitter daemon would drive them
// ---------------------------------------------------------------------------------------------

/// One instance of the program at `program`.
#[derive(Clone, Debug)]
pub struct Instance {
    pub program: AccountId,
    pub config: Config,
    pub checkpoint: Root,
    pub id: Root,
}

/// The transactions of one job, and what it was expected to do.
#[derive(Clone, Debug)]
pub struct JobRun {
    pub job_id: Root,
    pub reports: Vec<TxReport>,
    /// What the job does when applied (computed natively before submitting).
    pub outcome: Option<ApplyOutcome>,
}

impl Instance {
    pub fn new(program: AccountId, config: Config, checkpoint: Root) -> Self {
        let id = instance_id(&config, &checkpoint);
        Self { program, config, checkpoint, id }
    }

    pub fn actor(&self) -> Actor {
        instance_actor(self.program, &self.id)
    }

    pub fn fork(&self) -> LcFork {
        self.config.fork().unwrap()
    }

    pub fn state(&self, chain: &Chain) -> Option<InstanceState> {
        let s = chain.actor_state(self.actor());
        (!s.is_empty()).then(|| borsh::from_slice(&s).unwrap())
    }

    pub fn store(&self, chain: &Chain) -> CompactStore {
        CompactStore::decode(&self.state(chain).expect("instance exists").store).unwrap()
    }

    pub fn record(&self, chain: &Chain, slot: u64) -> Option<RecordState> {
        let s = chain.actor_state(header_actor(self.program, &self.id, slot));
        (!s.is_empty()).then(|| borsh::from_slice(&s).unwrap())
    }

    pub fn job(&self, chain: &Chain, job_id: &Root) -> Option<JobState> {
        let s = chain.actor_state(job_actor(self.program, &self.id, job_id));
        (!s.is_empty()).then(|| borsh::from_slice(&s).unwrap())
    }

    fn slot_time(&self, slot: u64) -> u64 {
        self.config.slot_start_ms(slot) + 12_000
    }

    /// `Init`, then the adoption of the bootstrap committee in `adoption_parts` transactions.
    pub fn init(&self, chain: &mut Chain, bootstrap: &LightClientBootstrap, label: &str) -> JobRun {
        let fork = self.fork();
        let spec = self.config.spec();
        let compact = CompactBootstrap::of(bootstrap);
        let parts = self.config.adoption_parts;
        let (store, job) =
            steps::initialize(&self.checkpoint, &compact, fork, &spec, parts).expect("bootstrap verifies");
        let jid = job_id(&self.id, &job.store_digest, &job.update_digest);
        let job_actor = job_actor(self.program, &self.id, &jid);
        let time = self.slot_time(bootstrap.header.beacon.slot);
        let mut reports = vec![chain.submit(
            &format!("{label} Init"),
            self.actor(),
            borsh::to_vec(&Message::Init {
                config: self.config.clone(),
                checkpoint: self.checkpoint,
                bootstrap: compact.encode(fork),
            })
            .unwrap(),
            vec![self.actor(), job_actor],
            vec![PublicAccountEvidence::Pda { program: self.program, seed: instance_seed(&self.id) }],
            time,
        )];
        let keys = uncompressed_keys(&bootstrap.current_sync_committee).unwrap();
        reports.extend(self.adopt_all(
            chain,
            &format!("{label} adopt"),
            job_actor,
            &keys,
            Some(store.finalized.slot),
            time,
        ));
        JobRun { job_id: jid, reports, outcome: Some(ApplyOutcome::default()) }
    }

    /// The adoption transactions of a job. The last one also reaches the instance and, if it
    /// applies the job (an initialization), the header record at `record_slot`.
    fn adopt_all(
        &self,
        chain: &mut Chain,
        label: &str,
        job_actor: Actor,
        keys: &[[u8; 96]],
        record_slot: Option<u64>,
        time: u64,
    ) -> Vec<TxReport> {
        let parts = self.config.adoption_parts;
        let chunk = keys.len() / parts as usize;
        (0..parts)
            .map(|p| {
                let mut actors = vec![job_actor];
                if p + 1 == parts {
                    if let Some(slot) = record_slot {
                        actors.extend([self.actor(), header_actor(self.program, &self.id, slot)]);
                    }
                }
                let part = keys[p as usize * chunk..(p as usize + 1) * chunk].to_vec();
                chain.submit(
                    &format!("{label} {}/{parts}", p + 1),
                    job_actor,
                    borsh::to_vec(&Message::Adopt { part: p, keys: part }).unwrap(),
                    actors,
                    vec![],
                    time,
                )
            })
            .collect()
    }

    /// The witness for `update`'s non-signers from the full committees of `full`.
    pub fn signer_witness(full: &StoreCore, update: &LightClientUpdate, spec: &ChainSpec) -> SignerWitness {
        let store_period = spec.period_at_slot(full.finalized_header.beacon.slot);
        let committee = if spec.period_at_slot(update.signature_slot) == store_period {
            &full.current_sync_committee
        } else {
            &full.next_sync_committee
        };
        SignerWitness::build(committee, &update.sync_aggregate.bits).unwrap_or_default()
    }

    /// The whole job for one update: `Open`, `Miller`, the adoption parts if it adopts a
    /// committee, `Finish`. The job is first run natively to name its actors and to know whether
    /// it adopts and advances (as a submitter would). Stops at the first refused transaction.
    pub fn update(&self, chain: &mut Chain, update: &LightClientUpdate, full: &StoreCore, label: &str) -> JobRun {
        let fork = self.fork();
        let spec = self.config.spec();
        let store = self.store(chain);
        let compact = CompactUpdate::of(update);
        let witness = Self::signer_witness(full, update, &spec);
        let jid = job_id(&self.id, &store.digest(), &compact.digest(fork));
        let job_actor = job_actor(self.program, &self.id, &jid);
        let (planned, outcome) = plan_job(&store, &compact, &witness, update, &self.config);
        let time = self.slot_time(update.signature_slot);
        let mut reports = Vec::new();
        let open = chain.submit(
            &format!("{label} Open (step 1)"),
            self.actor(),
            borsh::to_vec(&Message::Open { update: compact.encode(fork), signers: witness.encode() }).unwrap(),
            vec![self.actor(), job_actor],
            vec![],
            time,
        );
        let ok = open.ok();
        reports.push(open);
        if !ok {
            return JobRun { job_id: jid, reports, outcome: None };
        }
        let miller = chain.submit(
            &format!("{label} Miller (step 2)"),
            job_actor,
            borsh::to_vec(&Message::Miller).unwrap(),
            vec![job_actor],
            vec![],
            time,
        );
        reports.push(miller);
        if planned.as_ref().is_some_and(|job| job.adoption.is_some()) {
            let keys = uncompressed_keys(&update.next_sync_committee).unwrap();
            reports.extend(self.adopt_all(chain, &format!("{label} adopt"), job_actor, &keys, None, time));
        }
        let mut actors = vec![job_actor, self.actor()];
        if outcome.is_some_and(|o| o.advanced) {
            actors.push(header_actor(self.program, &self.id, update.finalized_header.beacon.slot));
        }
        reports.push(chain.submit(
            &format!("{label} Finish (step 3)"),
            job_actor,
            borsh::to_vec(&Message::Finish).unwrap(),
            actors,
            vec![],
            time,
        ));
        JobRun { job_id: jid, reports, outcome }
    }
}

/// The job as a submitter plans it: the job after step 1 (it names the job and says whether it
/// adopts a committee), and, if every step passes natively, what applying it does.
pub fn plan_job(
    store: &CompactStore,
    compact: &CompactUpdate,
    witness: &SignerWitness,
    update: &LightClientUpdate,
    config: &Config,
) -> (Option<Job>, Option<ApplyOutcome>) {
    let spec = config.spec();
    let n = config.sync_committee_size as usize;
    let Ok(opened) = steps::step1(
        store,
        compact,
        witness,
        update.signature_slot,
        &spec,
        &config.genesis_validators_root,
        config.adoption_parts,
    ) else {
        return (None, None);
    };
    let finish = || -> Option<ApplyOutcome> {
        let mut job = opened.clone();
        job.step2().ok()?;
        job.step3().ok()?;
        if job.adoption_pending() {
            let keys = uncompressed_keys(&update.next_sync_committee).ok()?;
            let parts = config.adoption_parts;
            let chunk = n / parts as usize;
            for p in 0..parts {
                job.adopt(p, &keys[p as usize * chunk..(p as usize + 1) * chunk], n).ok()?;
            }
        }
        steps::apply(store, &job, &spec).ok().map(|(_, outcome)| outcome)
    };
    let outcome = finish();
    (Some(opened), outcome)
}

// ---------------------------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------------------------

/// `1234567` as `1,234,567`.
pub fn grouped(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// What a refusal's exit code means.
pub fn exit_name(code: u32) -> String {
    use ferrata_lez_wire::code as c;
    let name = match code as u8 {
        c::BAD_MESSAGE => "BadMessage",
        c::WRONG_RECEIVER => "WrongReceiver",
        c::WRONG_SENDER => "WrongSender",
        c::ALREADY_INITIALIZED => "AlreadyInitialized",
        c::NO_INSTANCE => "NoInstance",
        c::NO_JOB => "NoJob",
        c::INSTANCE_INACTIVE => "InstanceInactive",
        c::JOB_EXISTS => "JobExists",
        c::RECORD_CONFLICT => "RecordConflict",
        c::BAD_STATE => "BadState",
        c::UNROUTABLE => "Unroutable",
        c::WRONG_INSTANCE => "WrongInstance",
        c::BAD_CONFIG => "BadConfig",
        80 => "StepError::Witness",
        81 => "StepError::Phase",
        82 => "StepError::StoreChanged",
        83 => "StepError::AdoptionIncomplete",
        84 => "StepError::NoAdoption",
        other => {
            return match CORE_ERRORS.into_iter().find(|e| e.code() == u16::from(other)) {
                Some(e) => format!("{e:?}"),
                None => format!("code {other}"),
            }
        }
    };
    name.to_string()
}

/// Every `ferrata_core::Error`, in the order of its declaration. The enum is `#[non_exhaustive]`,
/// so `tests/exit_codes.rs` checks this list against the core's source.
pub const CORE_ERRORS: [ferrata_core::Error; 47] = {
    use ferrata_core::Error as E;
    [
        E::Malformed,
        E::CheckpointMismatch,
        E::BootstrapHeaderInvalid,
        E::CurrentCommitteeBranchInvalid,
        E::ParticipationLow,
        E::AttestedHeaderInvalid,
        E::SlotOrderInvalid,
        E::PeriodSkipped,
        E::UpdateIrrelevant,
        E::FinalizedHeaderInvalid,
        E::FinalityBranchInvalid,
        E::NextCommitteeBranchInvalid,
        E::NextCommitteeMismatch,
        E::NextCommitteeUnexpected,
        E::SignatureInvalid,
        E::SignatureMalformed,
        E::PubkeyInvalid,
        E::SigningKeyIdentity,
        E::CommitteeInvalid,
        E::NotFinalized,
        E::BelowSupermajority,
        E::NotNewer,
        E::CommitteeUnknown,
        E::ForkMismatch,
        E::RlpNonCanonical,
        E::RlpMalformed,
        E::ProofNodeMismatch,
        E::ProofIncomplete,
        E::ProofTrailingNodes,
        E::ProofNodeInvalid,
        E::HeaderHashMismatch,
        E::HeaderUnsupported,
        E::ReceiptNotIncluded,
        E::ReceiptUnsupported,
        E::ReceiptFailed,
        E::LogIndexOutOfRange,
        E::PredicateFailed,
        E::QueryInvalid,
        E::ChainMismatch,
        E::BindingMismatch,
        E::HistoryOutOfWindow,
        E::HistoryHashMismatch,
        E::HistoryContractMissing,
        E::AccumulatorPathInvalid,
        E::AccumulatorIndexOutOfRange,
        E::AccumulatorLeafEmpty,
        E::AccumulatorRootMismatch,
    ]
};

/// A markdown table of settled transactions: per transaction, the handlers it ran with their
/// cycles, the execution gas LEZ charged, its share of `TARGET_GAS_EXEC`, the storage gas, the
/// base fee at genesis base fees, and the result.
pub fn render(reports: &[TxReport]) -> String {
    let mut out = String::from(
        "| # | transaction | handlers run (cycles each) | execution gas | of 5M target | storage gas (tx bytes) | base fee | result |\n\
         |---|---|---|---:|---:|---:|---:|---|\n",
    );
    for (i, r) in reports.iter().enumerate() {
        let handlers: Vec<String> =
            r.transitions.iter().map(|t| format!("{} {}", t.handler, grouped(t.cycles))).collect();
        let result = match (&r.exit_code, &r.charge.revert) {
            (Some(code), _) => format!("refused: exit {code} ({})", exit_name(*code)),
            (None, Some(revert)) => format!("reverted: {revert}"),
            (None, None) => "applied".to_string(),
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {:.0}% | {} | {} | {} |\n",
            i + 1,
            r.label,
            handlers.join(" → "),
            grouped(r.charge.gas_exec),
            r.charge.gas_exec as f64 / market::TARGET_GAS_EXEC as f64 * 100.0,
            grouped(r.charge.gas_stor),
            grouped(r.charge.fee_base as u64),
            result,
        ));
    }
    out
}

/// Every handler execution of `reports`: cycles, segments, and the bytes it read and committed.
pub fn render_transitions(reports: &[TxReport]) -> String {
    let mut out = String::from(
        "| transaction | handler | cycles | segments | input bytes (ReceiveInput) | of which message | of which pre-state | journal bytes | post-state bytes | exit |\n\
         |---|---|---:|---:|---:|---:|---:|---:|---:|---:|\n",
    );
    for r in reports {
        for t in &r.transitions {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                r.label,
                t.handler,
                grouped(t.cycles),
                t.segments,
                grouped(t.input_bytes as u64),
                grouped(t.message_bytes as u64),
                grouped(t.pre_state_bytes as u64),
                grouped(t.journal_bytes as u64),
                t.post_state_bytes.map_or("–".to_string(), |b| grouped(b as u64)),
                t.exit_code,
            ));
        }
    }
    out
}
