//! What a submitter of the Ferrata program computes off chain: the mainnet data, an instance's
//! addresses, and each job's messages and actors. The same planning as `../../harness/src/lib.rs`
//! (which settles in process); here the transactions go to a sequencer.

use std::{fs, path::Path};

use anyhow::{anyhow, ensure, Context, Result};
use ferrata_core::{
    bls::{Aggregation, Blst},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    policy::{Adoption, FinalizedStore},
    steps::{self, Job},
    store::{ApplyOutcome, StoreCore},
    types::{LightClientBootstrap, LightClientUpdate, SyncCommittee},
    validate::Context as CoreContext,
    witness::{CompactStore, CompactUpdate, SignerWitness},
    Root,
};
use ferrata_lez_wire::{header_seed, instance_id, instance_seed, job_seed, Config};
use lee::{AccountId, Actor};
use lee_core::program::PdaSeed;
use sha2::{Digest, Sha256};

pub static BLS: Blst = Blst::new(Aggregation::Auto);

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
    hex::decode(hex_str).expect("hex").try_into().expect("32 bytes")
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

/// The fixtures' pinned `SHA256SUMS`, compiled in.
const FIXTURE_SUMS: &str = include_str!("../../../../crates/ferrata-spectests/fixtures/mainnet-p1899/SHA256SUMS");

pub struct Fixtures {
    pub bootstrap: LightClientBootstrap,
    pub updates: Vec<LightClientUpdate>,
}

fn fixture(dir: &Path, name: &str) -> Result<Vec<u8>> {
    let path = dir.join(name);
    let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let want = FIXTURE_SUMS
        .lines()
        .find(|l| l.ends_with(name))
        .and_then(|l| l.split_whitespace().next())
        .ok_or_else(|| anyhow!("{name} is not in SHA256SUMS"))?;
    ensure!(hex::encode(Sha256::digest(&bytes)) == want, "{name} does not match SHA256SUMS");
    Ok(bytes)
}

/// The period-1899 bootstrap and the best updates of periods 1899-1903 (Fulu, 512-member
/// committees), each checked against the pinned sums.
pub fn load_fixtures(dir: &Path) -> Result<Fixtures> {
    let bootstrap = LightClientBootstrap::decode(&fixture(dir, "bootstrap.ssz")?, LcFork::Electra, 512)
        .map_err(|e| anyhow!("bootstrap.ssz: {e:?}"))?;
    let mut updates = Vec::new();
    for i in 0..5 {
        let name = format!("update_{i}.ssz");
        updates.push(
            LightClientUpdate::decode(&fixture(dir, &name)?, LcFork::Electra, 512)
                .map_err(|e| anyhow!("{name}: {e:?}"))?,
        );
    }
    Ok(Fixtures { bootstrap, updates })
}

/// `update` without its next committee: a finality update with the same signature.
pub fn as_finality_update(update: &LightClientUpdate) -> LightClientUpdate {
    let mut u = update.clone();
    u.next_sync_committee = SyncCommittee::empty(512);
    u.next_sync_committee_branch = vec![[0; 32]; u.next_sync_committee_branch.len()];
    u
}

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
}

/// The native light client next to an instance: it supplies the full committees a submitter
/// needs for the signer witness, and what the instance's store must equal after each job.
pub struct Native {
    pub store: FinalizedStore,
    pub spec: ChainSpec,
    pub genesis_validators_root: Root,
}

impl Native {
    pub fn new(config: &Config, checkpoint: &Root, bootstrap: &LightClientBootstrap) -> Result<Self> {
        let spec = config.spec();
        let store =
            FinalizedStore::initialize_with(checkpoint, bootstrap, LcFork::Electra, &spec, &BLS, Adoption::OnChain)
                .map_err(|e| anyhow!("the native store refuses the bootstrap: {e:?}"))?;
        Ok(Self { store, spec, genesis_validators_root: config.genesis_validators_root })
    }

    pub fn core(&self) -> &StoreCore {
        self.store.core()
    }

    pub fn process(&mut self, update: &LightClientUpdate) -> Result<ApplyOutcome, ferrata_core::Error> {
        let ctx = CoreContext { spec: &self.spec, genesis_validators_root: self.genesis_validators_root, bls: &BLS };
        self.store.process_update(update, update.signature_slot + 1, &ctx)
    }
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
