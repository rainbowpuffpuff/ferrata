//! Forks, presets and the chain configuration an instance is initialised with.

/// Consensus forks, in activation order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Fork {
    Phase0 = 0,
    Altair,
    Bellatrix,
    Capella,
    Deneb,
    Electra,
    Fulu,
    Gloas,
    Heze,
}

impl Fork {
    /// Every fork, in activation order.
    pub const ALL: [Fork; 9] = [
        Fork::Phase0,
        Fork::Altair,
        Fork::Bellatrix,
        Fork::Capella,
        Fork::Deneb,
        Fork::Electra,
        Fork::Fulu,
        Fork::Gloas,
        Fork::Heze,
    ];

    /// Lower-case name, as used in the consensus-spec-tests directory layout.
    pub const fn name(self) -> &'static str {
        match self {
            Fork::Phase0 => "phase0",
            Fork::Altair => "altair",
            Fork::Bellatrix => "bellatrix",
            Fork::Capella => "capella",
            Fork::Deneb => "deneb",
            Fork::Electra => "electra",
            Fork::Fulu => "fulu",
            Fork::Gloas => "gloas",
            Fork::Heze => "heze",
        }
    }

    /// Inverse of [`Fork::name`].
    pub fn from_name(name: &str) -> Option<Fork> {
        Fork::ALL.into_iter().find(|f| f.name() == name)
    }

    /// The light-client data layout this fork uses. Phase 0 has no light client.
    pub const fn lc_fork(self) -> Option<LcFork> {
        match self {
            Fork::Phase0 => None,
            Fork::Altair | Fork::Bellatrix => Some(LcFork::Altair),
            Fork::Capella => Some(LcFork::Capella),
            Fork::Deneb => Some(LcFork::Deneb),
            Fork::Electra | Fork::Fulu => Some(LcFork::Electra),
            Fork::Gloas | Fork::Heze => Some(LcFork::Gloas),
        }
    }
}

/// Light-client data layouts. Forks that left the light-client containers unchanged share one
/// (Bellatrix uses Altair's, Fulu uses Electra's).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LcFork {
    /// `LightClientHeader { beacon }`.
    Altair,
    /// Adds `execution: capella.ExecutionPayloadHeader` and `execution_branch`.
    Capella,
    /// `execution` becomes `deneb.ExecutionPayloadHeader` (blob gas fields).
    Deneb,
    /// Same header as Deneb; state branches one level deeper (EIP-7251 grew the state).
    Electra,
    /// `execution` replaced by `execution_block_hash` (EIP-7732); EIP-7688 state gindices.
    Gloas,
}

impl LcFork {
    /// The next layout, if any.
    pub const fn next(self) -> Option<LcFork> {
        match self {
            LcFork::Altair => Some(LcFork::Capella),
            LcFork::Capella => Some(LcFork::Deneb),
            LcFork::Deneb => Some(LcFork::Electra),
            LcFork::Electra => Some(LcFork::Gloas),
            LcFork::Gloas => None,
        }
    }
}

/// Preset values the light client depends on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preset {
    pub sync_committee_size: usize,
    pub slots_per_epoch: u64,
    pub epochs_per_sync_committee_period: u64,
    pub min_sync_committee_participants: u64,
}

impl Preset {
    /// The `mainnet` preset (also used by Sepolia and Hoodi).
    pub const MAINNET: Preset = Preset {
        sync_committee_size: 512,
        slots_per_epoch: 32,
        epochs_per_sync_committee_period: 256,
        min_sync_committee_participants: 1,
    };

    /// The `minimal` preset (spec tests and fast local devnets).
    pub const MINIMAL: Preset = Preset {
        sync_committee_size: 32,
        slots_per_epoch: 8,
        epochs_per_sync_committee_period: 8,
        min_sync_committee_participants: 1,
    };

    /// `UPDATE_TIMEOUT`, in slots.
    pub const fn update_timeout(&self) -> u64 {
        self.slots_per_epoch * self.epochs_per_sync_committee_period
    }
}

/// `FAR_FUTURE_EPOCH`: a fork that is not scheduled.
pub const FAR_FUTURE_EPOCH: u64 = u64::MAX;

/// Chain configuration: preset plus fork schedule. Written once when an instance is created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainSpec {
    pub preset: Preset,
    /// Fork versions, indexed by `Fork as usize` (`versions[0]` is `GENESIS_FORK_VERSION`).
    pub versions: [[u8; 4]; 9],
    /// Activation epochs, indexed by `Fork as usize` (`epochs[0]` is always 0).
    pub epochs: [u64; 9],
}

impl ChainSpec {
    /// Activation epoch of `fork`.
    pub const fn fork_epoch(&self, fork: Fork) -> u64 {
        self.epochs[fork as usize]
    }

    /// Version of `fork`.
    pub const fn fork_version(&self, fork: Fork) -> [u8; 4] {
        self.versions[fork as usize]
    }

    /// The fork active at `epoch`: the latest one whose activation epoch has passed.
    pub fn fork_at_epoch(&self, epoch: u64) -> Fork {
        Fork::ALL.into_iter().rev().find(|f| epoch >= self.fork_epoch(*f)).unwrap_or(Fork::Phase0)
    }

    /// `compute_fork_version(epoch)`.
    pub fn compute_fork_version(&self, epoch: u64) -> [u8; 4] {
        self.fork_version(self.fork_at_epoch(epoch))
    }

    /// `compute_epoch_at_slot`.
    pub const fn epoch_at_slot(&self, slot: u64) -> u64 {
        slot / self.preset.slots_per_epoch
    }

    /// `compute_sync_committee_period_at_slot`.
    pub const fn period_at_slot(&self, slot: u64) -> u64 {
        self.epoch_at_slot(slot) / self.preset.epochs_per_sync_committee_period
    }

    /// True if `slot` is at or after the activation of `fork`.
    pub fn is_at_or_after(&self, slot: u64, fork: Fork) -> bool {
        self.epoch_at_slot(slot) >= self.fork_epoch(fork)
    }
}
