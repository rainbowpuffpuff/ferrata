//! What the updater knows about a chain: the instance configurations it can follow (presets, as
//! `harness::mainnet_config` and `harness::sepolia::sepolia_config`), the decoding of what beacon
//! nodes serve (as the lab driver's `decode_update` and `decode_finality_update`), and the job
//! planning that runs the program's own step code natively before a transaction is paid for (as
//! `harness::plan_job`).

use anyhow::{anyhow, bail, ensure, Result};
use ferrata_core::{
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    merkle::ZERO,
    steps::{self, Job},
    store::ApplyOutcome,
    types::{LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    witness::{uncompressed_keys, CompactStore, CompactUpdate, SignerWitness},
    Root,
};
use ferrata_lez_wire::Config;

pub fn root(hex_str: &str) -> Result<Root> {
    let s = hex_str.trim().trim_start_matches("0x");
    hex::decode(s)?.try_into().map_err(|_| anyhow!("{s}: not 32 bytes"))
}

/// A named chain configuration (`preset` in the configuration file).
pub fn preset(name: &str, adoption_parts: u8) -> Result<Config> {
    let (spec, chain_id, gvr, genesis_time) = match name {
        // configs/mainnet.yaml at consensus-specs v1.7.0-beta.4 (as harness::mainnet_spec).
        "mainnet" => (
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
            },
            1u64,
            "4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95",
            1_606_824_023u64,
        ),
        // Sepolia with Gloas at epoch 353,024 (as harness::sepolia::sepolia_spec).
        "sepolia" => {
            let v = |b: u8| [0x90, 0x00, 0x00, b];
            (
                ChainSpec {
                    preset: Preset::MAINNET,
                    versions: [v(0x69), v(0x70), v(0x71), v(0x72), v(0x73), v(0x74), v(0x75), v(0x76), [0; 4]],
                    epochs: [0, 50, 100, 56_832, 132_608, 222_464, 272_640, 353_024, FAR_FUTURE_EPOCH],
                },
                11_155_111,
                "d8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078",
                1_655_733_600,
            )
        }
        other => bail!("unknown chain preset {other:?} (mainnet, sepolia)"),
    };
    Ok(Config {
        chain_id,
        sync_committee_size: 512,
        slots_per_epoch: spec.preset.slots_per_epoch,
        epochs_per_sync_committee_period: spec.preset.epochs_per_sync_committee_period,
        min_sync_committee_participants: spec.preset.min_sync_committee_participants,
        fork_versions: spec.versions,
        fork_epochs: spec.epochs,
        genesis_validators_root: root(gvr)?,
        genesis_time,
        seconds_per_slot: 12,
        store_fork: 3,
        adoption_parts,
    })
}

/// The layout a beacon node serves light-client data of `slot` in.
pub fn served_layout(spec: &ChainSpec, slot: u64) -> Option<LcFork> {
    spec.fork_at_epoch(spec.epoch_at_slot(slot)).lc_fork()
}

/// A `LightClientUpdate` as the `updates` endpoint serves it, in the layout of the fork its
/// attested header falls in.
pub fn decode_update(bytes: &[u8], config: &Config) -> Result<(LightClientUpdate, LcFork)> {
    let spec = config.spec();
    let n = config.sync_committee_size as usize;
    for layout in [LcFork::Electra, LcFork::Gloas] {
        if let Ok(update) = LightClientUpdate::decode(bytes, layout, n) {
            if served_layout(&spec, update.attested_header.beacon.slot) == Some(layout) {
                return Ok((update, layout));
            }
        }
    }
    bail!("an update of {} bytes in no layout this chain's schedule allows", bytes.len())
}

/// A `LightClientFinalityUpdate` as the `LightClientUpdate` it stands for (empty committee
/// fields): Electra (offsets) or Gloas (fixed size) layout.
pub fn decode_finality_update(b: &[u8], config: &Config) -> Result<(LightClientUpdate, LcFork)> {
    let spec = config.spec();
    let n = config.sync_committee_size as usize;
    let u64_at = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().expect("8 bytes"));
    const H: usize = 112 + 32 + 11 * 32;
    if b.len() == 2 * H + 9 * 32 + 160 + 8 {
        let update = LightClientUpdate {
            attested_header: LightClientHeader::decode(&b[..H], LcFork::Gloas).map_err(|e| anyhow!("{e:?}"))?,
            next_sync_committee: SyncCommittee::empty(n),
            next_sync_committee_branch: vec![ZERO; LcFork::Gloas.sync_committee_branch_len()],
            finalized_header: LightClientHeader::decode(&b[H..2 * H], LcFork::Gloas).map_err(|e| anyhow!("{e:?}"))?,
            finality_branch: b[2 * H..2 * H + 288].chunks(32).map(|c| c.try_into().expect("32")).collect(),
            sync_aggregate: SyncAggregate::decode(&b[2 * H + 288..2 * H + 448], n).map_err(|e| anyhow!("{e:?}"))?,
            signature_slot: u64_at(2 * H + 448),
        };
        ensure!(
            served_layout(&spec, update.attested_header.beacon.slot) == Some(LcFork::Gloas),
            "a Gloas finality update before Gloas"
        );
        return Ok((update, LcFork::Gloas));
    }
    ensure!(b.len() > 400, "a finality update of {} bytes", b.len());
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().expect("4 bytes")) as usize;
    let (att, fin) = (u32_at(0), u32_at(4));
    ensure!(att == 400 && fin > att && fin < b.len(), "LightClientFinalityUpdate offsets {att}, {fin}");
    let update = LightClientUpdate {
        attested_header: LightClientHeader::decode(&b[att..fin], LcFork::Electra).map_err(|e| anyhow!("{e:?}"))?,
        next_sync_committee: SyncCommittee::empty(n),
        next_sync_committee_branch: vec![ZERO; LcFork::Electra.sync_committee_branch_len()],
        finalized_header: LightClientHeader::decode(&b[fin..], LcFork::Electra).map_err(|e| anyhow!("{e:?}"))?,
        finality_branch: b[8..232].chunks(32).map(|c| c.try_into().expect("32")).collect(),
        sync_aggregate: SyncAggregate::decode(&b[232..392], n).map_err(|e| anyhow!("{e:?}"))?,
        signature_slot: u64_at(392),
    };
    ensure!(
        served_layout(&spec, update.attested_header.beacon.slot) == Some(LcFork::Electra),
        "an Electra finality update under Gloas"
    );
    Ok((update, LcFork::Electra))
}

/// The SSZ stream of the `updates` endpoint: per update a u64 length, a 4-byte fork digest, the
/// SSZ. Accepts a length that counts the digest or one that does not (as `lab/fetch.py`).
pub fn split_chunks(raw: &[u8]) -> Vec<Vec<u8>> {
    'outer: for counts_digest in [true, false] {
        let (mut out, mut at) = (Vec::new(), 0usize);
        while at < raw.len() {
            if at + 12 > raw.len() {
                continue 'outer;
            }
            let n = u64::from_le_bytes(raw[at..at + 8].try_into().expect("8")) as usize;
            let size = if counts_digest { n.wrapping_sub(4) } else { n };
            if size == 0 || size > raw.len() || at + 12 + size > raw.len() {
                continue 'outer;
            }
            out.push(raw[at + 12..at + 12 + size].to_vec());
            at += 12 + size;
        }
        return out;
    }
    Vec::new()
}

/// The witness of `update`'s non-signers, from the committee that signs it (`current` if its
/// signature slot is in the store's period, else `next`).
pub fn signing_committee<'a>(
    store: &CompactStore,
    update: &LightClientUpdate,
    spec: &ChainSpec,
    current: Option<&'a SyncCommittee>,
    next: Option<&'a SyncCommittee>,
) -> Option<&'a SyncCommittee> {
    if spec.period_at_slot(update.signature_slot) == spec.period_at_slot(store.finalized.slot) {
        current
    } else {
        next
    }
}

pub fn signer_witness(committee: &SyncCommittee, update: &LightClientUpdate) -> SignerWitness {
    SignerWitness::build(committee, &update.sync_aggregate.bits).unwrap_or_default()
}

/// The job as the program would run it: the job after step 1, and, if every step passes, what
/// applying it does to `store` (as `harness::plan_job`).
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

/// The layout an update is sent in to a store in `store_fork`: the layout it is served in, or the
/// store's own if that is later.
pub fn layout_for(config: &Config, store_fork: LcFork, update: &LightClientUpdate) -> Option<LcFork> {
    served_layout(&config.spec(), update.attested_header.beacon.slot).map(|l| l.max(store_fork))
}

/// The hash of the full committee's keys as the program summarises it.
pub fn committee_key(c: &SyncCommittee) -> Root {
    ferrata_core::witness::CommitteeSummary::of(c).pubkeys_root
}

/// A committee in a file: the keys, then the aggregate key (48 bytes each).
pub fn encode_committee(c: &SyncCommittee) -> Vec<u8> {
    let mut out = Vec::with_capacity(48 * (c.pubkeys.len() + 1));
    for k in &c.pubkeys {
        out.extend_from_slice(k);
    }
    out.extend_from_slice(&c.aggregate_pubkey);
    out
}

pub fn decode_committee(b: &[u8]) -> Result<SyncCommittee> {
    ensure!(!b.is_empty() && b.len().is_multiple_of(48), "a committee file of {} bytes", b.len());
    let mut keys: Vec<[u8; 48]> = b.chunks(48).map(|c| c.try_into().expect("48")).collect();
    let aggregate_pubkey = keys.pop().expect("non-empty");
    Ok(SyncCommittee { pubkeys: keys, aggregate_pubkey })
}
