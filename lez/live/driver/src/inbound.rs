//! The inbound run (`../inbound/`): the identity of its bridge configuration, which the vault on
//! Sepolia pins (`configId`, its constructor argument and slot 2) before the zone exists. Every
//! part is fixed in advance: the two programs' header accounts come from the run's derived keys
//! (`program/<name>/header`, independent of the image and of the order of deployment), the
//! bridge's image id from its frozen binary, the attestation instance from the Sepolia config and
//! the bootstrap's block root; the zone's Bedrock genesis and channel, which a scripted session's
//! local devnet only fixes when it starts and which the program does not check, are fixed tags.

use std::path::Path;

use anyhow::{ensure, Result};
use ferrata_lez_bridge_wire::{BridgeConfig, Terms};
use ferrata_lez_wire::{instance_id, Config};
use lee::AccountId;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    plan::{root, sepolia_config, SEPOLIA_CHAIN_ID, SEPOLIA_CHECKPOINT},
    programs::GuestBinary,
    scenario::deployment_accounts,
};

/// The deployment names of the attestation program and the bridge program on the zone.
pub const ATTEST: &str = "attest";
pub const BRIDGE: &str = "bridge";
/// The Sepolia instance adopts a committee in two transactions, as Stage 5 and the live run do.
pub const ADOPTION_PARTS: u8 = 2;
/// The vault's one registered token, native ETH, and its denominations in wei (0.001, 0.01 and
/// 0.1 ETH, `vault/script/RoundTrip.s.sol`).
pub const ETH: [u8; 20] = [0; 20];
pub const DENOMINATIONS: [u128; 3] = [1_000_000_000_000_000, 10_000_000_000_000_000, 100_000_000_000_000_000];
pub const BEDROCK_GENESIS_TAG: &[u8] = b"ferrata-live/v1/inbound/bedrock-genesis-id";
pub const CHANNEL_TAG: &[u8] = b"ferrata-live/v1/inbound/channel-id";

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn image_bytes(id: [u32; 8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, w) in id.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

pub fn sepolia_instance() -> (Config, [u8; 32], [u8; 32]) {
    let config = sepolia_config(ADOPTION_PARTS);
    let checkpoint = root(SEPOLIA_CHECKPOINT);
    let id = instance_id(&config, &checkpoint);
    (config, checkpoint, id)
}

/// The configuration of the vault at `vault`, with the programs at their run accounts.
pub fn bridge_config(attest: AccountId, bridge_image_id: [u8; 32], vault: [u8; 20]) -> BridgeConfig {
    BridgeConfig {
        bedrock_genesis_id: sha256(BEDROCK_GENESIS_TAG),
        channel_id: sha256(CHANNEL_TAG),
        bridge_image_id,
        attestation: attest,
        instance_id: sepolia_instance().2,
        terms: Terms { evm_chain_id: SEPOLIA_CHAIN_ID, vault, token: ETH, denominations: DENOMINATIONS },
    }
}

fn program(name: &str, binary: &GuestBinary) -> Value {
    let (header, segments) = deployment_accounts(name, binary);
    json!({
        "name": name,
        "header_account": hex::encode(header.value()),
        "header_account_display": header.to_string(),
        "key_label": crate::scenario::header_key_label(name),
        "segment_accounts": segments.iter().map(|s| hex::encode(s.value())).collect::<Vec<_>>(),
        "binary_sha256": binary.sha256(),
        "binary_bytes": binary.bytes.len(),
        "user_elf_bytes": binary.user_elf.len(),
        "image_id": binary.image_hex(),
    })
}

pub fn plan_ids(attest_bin: &Path, bridge_bin: &Path, vault: Option<&str>) -> Result<Value> {
    let attest = GuestBinary::load(ATTEST, attest_bin)?;
    let bridge = GuestBinary::load(BRIDGE, bridge_bin)?;
    let attest_account = deployment_accounts(ATTEST, &attest).0;
    let bridge_account = deployment_accounts(BRIDGE, &bridge).0;
    let bridge_image = image_bytes(bridge.image_id);
    let (config, checkpoint, instance) = sepolia_instance();
    let sample = bridge_config(attest_account, bridge_image, [0; 20]);
    let mut denominations = Vec::new();
    for d in DENOMINATIONS {
        denominations.extend_from_slice(&d.to_be_bytes());
    }
    let mut salt_preimage = b"ferrata-lez/v1/bridge/terms".to_vec();
    salt_preimage.extend_from_slice(&ETH);
    salt_preimage.extend_from_slice(&denominations);
    ensure!(sha256(&salt_preimage) == sample.salt(), "the salt's preimage is not the one bridge-wire hashes");
    let mut root_preimage = b"ferrata-lez/v1/bridge/attestation-root".to_vec();
    root_preimage.extend_from_slice(attest_account.value());
    root_preimage.extend_from_slice(&instance);
    ensure!(sha256(&root_preimage) == sample.attestation_root_id(), "the attestation root id's preimage");
    let pubs = testnet_initial_state::initial_pub_accounts_private_keys();
    let privs = testnet_initial_state::initial_priv_accounts_private_keys();
    let mut out = json!({
        "attest": program(ATTEST, &attest),
        "bridge": program(BRIDGE, &bridge),
        "sepolia": {
            "config": format!("{config:?}"),
            "config_borsh": hex::encode(borsh::to_vec(&config)?),
            "checkpoint": hex::encode(checkpoint),
            "instance_id": hex::encode(instance),
        },
        "bedrock_genesis_id": hex::encode(sample.bedrock_genesis_id),
        "bedrock_genesis_id_preimage": String::from_utf8_lossy(BEDROCK_GENESIS_TAG),
        "channel_id": hex::encode(sample.channel_id),
        "channel_id_preimage": String::from_utf8_lossy(CHANNEL_TAG),
        "bridge_header": hex::encode(bridge_account.value()),
        "bridge_image_id": hex::encode(bridge_image),
        "attestation_account": hex::encode(attest_account.value()),
        "attestation_root_id": hex::encode(sample.attestation_root_id()),
        "attestation_root_id_preimage": hex::encode(&root_preimage),
        "salt": hex::encode(sample.salt()),
        "salt_preimage": hex::encode(&salt_preimage),
        "evm_chain_id": SEPOLIA_CHAIN_ID,
        "token": hex::encode(ETH),
        "denominations_wei": DENOMINATIONS.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
        "dest": {
            "public0": hex::encode(pubs[0].account_id.value()),
            "public0_display": pubs[0].account_id.to_string(),
            "private0": hex::encode(privs[0].account_id().value()),
            "private0_display": privs[0].account_id().to_string(),
        },
    });
    if let Some(v) = vault {
        let bytes: [u8; 20] = hex::decode(v.trim_start_matches("0x"))?
            .try_into()
            .map_err(|_| anyhow::anyhow!("--vault is not 20 bytes"))?;
        let config = bridge_config(attest_account, bridge_image, bytes);
        out["config_id"] = json!({ "vault": hex::encode(bytes), "config_id": hex::encode(config.id(bridge_account)) });
    }
    Ok(out)
}
