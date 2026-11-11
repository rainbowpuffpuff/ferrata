//! What a submitter of the Ferrata program computes off chain: the two chains' configurations,
//! an instance's addresses, and each job's plan. The same planning as `../../../harness/src/lib.rs`
//! (`Instance::update_in`, `plan_job`), which settles in process; here the transactions go to a
//! sequencer. The live driver's modules (`../../../live/driver/src/`) take `exit_name` and `Pda`
//! from here, and its `verify.rs` the mainnet constants of its own scenario.

use ferrata_core::{
    bls::{Aggregation, Blst},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    steps::{self, Job},
    store::{ApplyOutcome, StoreCore},
    types::LightClientUpdate,
    witness::{CompactStore, CompactUpdate, SignerWitness},
    Root,
};
use ferrata_lez_wire::{header_seed, instance_id, instance_seed, job_seed, Config};
use lee::{AccountId, Actor};
use lee_core::program::PdaSeed;

pub static BLS: Blst = Blst::new(Aggregation::Auto);

pub fn root(hex_str: &str) -> Root {
    hex::decode(hex_str).expect("hex").try_into().expect("32 bytes")
}

// ---- Mainnet ----------------------------------------------------------------------------------

pub const MAINNET_CHAIN_ID: u64 = 1;
pub const MAINNET_GENESIS_TIME: u64 = 1_606_824_023;
pub const GENESIS_VALIDATORS_ROOT: &str = "4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
/// The live driver's scenario (its `verify.rs` checks its instance A against these; the lab
/// deploys under another name, so that check does not apply to the lab's records).
pub const TRUSTED_BLOCK_ROOT: &str = "64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac";
pub const EXPECTED_FINALIZED: [(u64, &str); 1] =
    [(15_589_599, "5d1200f2c031f01aa2c3385c881ca6cac727c23d3080e0ac53169160c8ddd573")];

/// Mainnet fork schedule, as `harness::mainnet_spec` (consensus-specs v1.7.0-beta.4; no Gloas).
pub fn mainnet_spec() -> ChainSpec {
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [[0, 0, 0, 0], [1, 0, 0, 0], [2, 0, 0, 0], [3, 0, 0, 0], [4, 0, 0, 0], [5, 0, 0, 0], [6, 0, 0, 0], [7, 0, 0, 0], [8, 0, 0, 0]],
        epochs: [0, 74_240, 144_896, 194_048, 269_568, 364_032, 411_392, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    }
}

/// As `harness::mainnet_config`: Electra/Fulu layout, adoption in `parts` transactions.
pub fn mainnet_config(parts: u8) -> Config {
    let spec = mainnet_spec();
    Config {
        chain_id: MAINNET_CHAIN_ID,
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
        // Electra's weak-subjectivity period for 33,554,432 ETH on the mainnet configuration (the
        // specification's table); a stand-in for the period of the checkpoint state.
        ws_period_epochs: 3_532,
    }
}

// ---- Sepolia ----------------------------------------------------------------------------------

pub const SEPOLIA_CHAIN_ID: u64 = 11_155_111;
pub const SEPOLIA_GENESIS_TIME: u64 = 1_655_733_600;
pub const SEPOLIA_GENESIS_VALIDATORS_ROOT: &str = "d8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078";
/// The block root of the period-1378 bootstrap (`fixtures/sepolia-gloas-2026-11-10`): the last
/// period before Gloas, the earliest bootstrap the program can follow Sepolia from (no public
/// node serves a Gloas-era bootstrap; the fixture's SOURCE.md records the refusals).
pub const SEPOLIA_CHECKPOINT: &str = "5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d";

/// As `harness::sepolia::sepolia_spec`: Gloas at epoch 353,024.
pub fn sepolia_spec() -> ChainSpec {
    let v = |b: u8| [0x90, 0x00, 0x00, b];
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [v(0x69), v(0x70), v(0x71), v(0x72), v(0x73), v(0x74), v(0x75), v(0x76), [0; 4]],
        epochs: [0, 50, 100, 56_832, 132_608, 222_464, 272_640, 353_024, FAR_FUTURE_EPOCH],
    }
}

/// As `harness::sepolia::sepolia_config`: the store starts in the Electra layout, the schedule
/// has Gloas.
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
        // Electra's weak-subjectivity period for 33,554,432 ETH on the mainnet configuration (the
        // specification's table); a stand-in for the period of the checkpoint state.
        ws_period_epochs: 3_532,
    }
}

// ---- Addresses --------------------------------------------------------------------------------

/// A public PDA of a program: an actor whose account only that program's code can create.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pda {
    pub program: AccountId,
    pub seed: PdaSeed,
}

impl Pda {
    pub fn account(&self) -> AccountId {
        AccountId::for_public_pda(&self.program, &self.seed)
    }

    pub fn actor(&self) -> Actor {
        Actor::new(self.account(), self.program)
    }
}

/// One instance of the program deployed at `program`.
#[derive(Clone, Debug)]
pub struct Instance {
    pub name: String,
    pub program: AccountId,
    pub config: Config,
    pub checkpoint: Root,
    pub id: Root,
}

impl Instance {
    pub fn new(name: &str, program: AccountId, config: Config, checkpoint: Root) -> Self {
        let id = instance_id(&config, &checkpoint);
        Self { name: name.to_string(), program, config, checkpoint, id }
    }

    pub fn pda(&self) -> Pda {
        Pda { program: self.program, seed: instance_seed(&self.id) }
    }

    pub fn job(&self, job_id: &Root) -> Pda {
        Pda { program: self.program, seed: job_seed(&self.id, job_id) }
    }

    pub fn header(&self, slot: u64) -> Pda {
        Pda { program: self.program, seed: header_seed(&self.id, slot) }
    }

    pub fn fork(&self) -> LcFork {
        self.config.fork().expect("a known store layout")
    }

    /// As `harness::Instance::layout_for`: the layout of the fork the schedule puts at the
    /// update's attested header (the layout a beacon node serves it in), or the store's own if
    /// that is later.
    pub fn layout_for(&self, store_fork: LcFork, update: &LightClientUpdate) -> LcFork {
        served_layout(&self.config.spec(), update.attested_header.beacon.slot).max(store_fork)
    }
}

/// The light-client layout of the fork the schedule puts at `slot`.
pub fn served_layout(spec: &ChainSpec, slot: u64) -> LcFork {
    spec.fork_at_epoch(spec.epoch_at_slot(slot)).lc_fork().expect("a light-client fork")
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
            let keys = ferrata_core::witness::uncompressed_keys(&update.next_sync_committee).ok()?;
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

/// What a refusal's exit code means (`../../RESULTS.md` §1 lists the blocks of codes).
pub fn exit_name(code: u32) -> String {
    use ferrata_lez_wire::code as c;
    let Ok(code) = u8::try_from(code) else { return format!("code {code}") };
    let name = match code {
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
        40 => "SignatureInvalid",
        41 => "SignatureMalformed",
        52 => "NotNewer",
        other => return format!("code {other}"),
    };
    name.to_string()
}
