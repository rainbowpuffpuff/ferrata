//! What the updater knows about a chain: the instance configurations it can follow (presets, as
//! `harness::mainnet_config` and `harness::sepolia::sepolia_config`), the decoding of what beacon
//! nodes serve (as the lab driver's `decode_update` and `decode_finality_update`), and the job
//! planning that runs the program's own step code natively before a transaction is paid for (as
//! `harness::plan_job`).

use anyhow::{anyhow, bail, ensure, Result};
use ferrata_core::{
    fork::{ChainSpec, Preset, FAR_FUTURE_EPOCH},
    types::SyncCommittee,
    Root,
};
use ferrata_lez_wire::Config;

// The decoding of served updates and the job planning are ferrata-advance's, shared with
// ferrata_attest_module (`advance`).
pub use ferrata_advance::{
    decode_committee, decode_finality_update, decode_update, layout_for, plan_job, served_layout, signer_witness,
    signing_committee,
};

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
        // Electra's weak-subjectivity period for 33,554,432 ETH on the mainnet configuration (the specification's
        // table), as the harness and the live driver configure their instances (lc-a1); a stand-in for the period
        // of the checkpoint state.
        ws_period_epochs: 3_532,
    })
}

/// An instance's configuration: the preset with its store in `layout` (the checkpoint bootstrap's light-client
/// layout, `Config::store_fork`; `None`: the preset's, Electra). The instance id hashes the whole configuration, so
/// an updater finds an instance another submitter initialized (crown-driver's `sepolia_base` sets the store's layout
/// from its bootstrap the same way) only with the same layout.
pub fn instance_config(preset_name: &str, adoption_parts: u8, layout: Option<&str>) -> Result<Config> {
    let mut config = preset(preset_name, adoption_parts)?;
    if let Some(name) = layout {
        config.store_fork = match name {
            "altair" => 0,
            "capella" => 1,
            "deneb" => 2,
            "electra" => 3,
            "gloas" => 4,
            other => bail!("layout {other:?}: altair, capella, deneb, electra or gloas"),
        };
    }
    ensure!(config.fork().is_some(), "layout {layout:?}: not a light-client layout of this build");
    Ok(config)
}

/// The URL guards' parser, shared with the live driver (`../../live/driver/src/rpcurl.rs`, standard library only):
/// the host a URL names, read strictly, a userinfo refused.
#[path = "../../live/driver/src/rpcurl.rs"]
pub mod rpcurl;

/// Only a sequencer on this machine (as the live driver's `check_loopback`): a LEZ wallet's default is a public
/// network, and nothing here is meant for one. The host is read strictly ([`rpcurl`]): `http://127.0.0.1:1@HOST` is
/// refused, not taken for loopback.
pub fn check_loopback(rpc: &str) -> Result<()> {
    // The day-one deployment's exception (as the live driver's `check_loopback`): another sequencer when
    // `FERRATA_SEQUENCER_REMOTE` names exactly this URL (and it reads strictly); the sequencer backend then sends to it
    // only with the owner's `FERRATA_TESTNET_SEND=yes` (`live.rs`, `check_send`).
    if std::env::var("FERRATA_SEQUENCER_REMOTE").is_ok_and(|named| !named.is_empty() && named == rpc)
        && rpcurl::parse(rpc).is_ok()
    {
        return Ok(());
    }
    let u = rpcurl::parse(rpc).map_err(|why| anyhow!("refusing {why}"))?;
    ensure!(u.is_loopback(), "refusing to talk to {}: only a sequencer on this machine", u.host);
    Ok(())
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
