//! The crown run's LEZ side (`../../README.md`, `e2e/crown/README.md`): the attestation program and
//! the merged bridge program on a LEZ v0.4 devnet, in two sessions with the devnet saved between
//! them.
//!
//! Session A deploys both programs and the Sepolia instance (Init from the checkpoint fixture's bootstrap and its
//! period updates: `sepolia_base`), then writes what the vault's constructor needs: the
//! channel's key, the last final block T0 (the checkpoint's parent tip), the zone's genesis id and
//! the configuration's identity. Session B, after the deposits and their captures exist, applies
//! the captures' updates and finality updates and runs the bridge: Init (the reciprocal pairing),
//! Register ETH (the vault's registry entry and fee schedule 0), ImportFees (schedule 1 absent at
//! the second record), ImportRoot n=1 and n=2, Claim of deposit 0 (public), ClaimPrivately of
//! deposit 1 (real proof), Burn public and Burn private (real proof), and writes the burn notes.
//!
//! The stand-in chain (`--chain stand-in`, rehearsals only, labelled so in every record): a
//! committee whose keys we hold signs finality updates whose finalized headers bind the captured
//! execution blocks (an anvil fork's), in the Gloas layout of Sepolia's config.

use std::{borrow::Cow, fs, path::{Path, PathBuf}, time::{Instant, SystemTime, UNIX_EPOCH}};

use anyhow::{anyhow, bail, ensure, Context, Result};
use ferrata_bridge_core::{
    chain_id_u256,
    inbound::{commitment, deposit_id, deposit_leaf, deposit_secret, nullifier as core_nullifier, DestKind},
    outbound::{burn_commitment, burn_secret, burn_tag, xor_memo},
    tree::Tree,
    BridgeKeys,
};
use ferrata_core::{
    fork::LcFork,
    merkle::{CURRENT_SYNC_COMMITTEE_GINDEX_GLOAS, EXECUTION_BLOCK_HASH_GINDEX_GLOAS, FINALIZED_ROOT_GINDEX_GLOAS, ZERO},
    store::ApplyOutcome,
    types::{
        BeaconBlockHeader, Execution, LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate,
        SyncCommittee,
    },
    validate::{sync_committee_signing_root, Context as CoreContext},
    Root,
};
use ferrata_lez_bridge_wire::{
    burn_mac_v2, burn_seed, config_seed, fee_slot, fees_slots, holder_seed, nf_out_v2, nullifier_seed, register_slots,
    registry_slot,
    root_seed, token_seed, with_header, word_of, word_of_u128, BridgeConfig, BridgeMessage, BridgeState, CapParams,
    ClaimWitness, ConfigState, Controls, FeeSchedule, HolderState, ImportProof, Note, PairingProof, ScheduleRange,
    TokenEntry, VaultProof, SLOT_CONFIG_ID, SLOT_COUNT, SLOT_ROOT, UNLIMITED,
};
use ferrata_lez_wire::{instance_id, Config};
use ferrata_spectests::{
    execkit::{account_proof, keccak},
    testkit::{fake_committee, filler, sign, sparse_tree},
};
use lee::{privacy_preserving_transaction::circuit::ProgramCatalog, program::Program, AccountId};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use wallet::{AccountIdentity, AccountMention};

use crate::{
    keys::{account_of, derived_key, lez_test_payers},
    live::{mention, Live, Outcome, Sent, Tx},
    manifest::Row,
    plan::{sepolia_config, Native, Pda, BLS, SEPOLIA_CHAIN_ID},
    private::{private_wallet, TapClient},
    programs::GuestBinary,
    scenario::{self, deployment_accounts, Harness, World},
};

/// The deployment names of the two programs (their header accounts come from the derived keys of
/// `program/<name>/header`: independent of the image and of the order of deployment).
pub const ATTEST: &str = "attest";
pub const BRIDGE: &str = "bridge";
/// The Sepolia instance adopts a committee in two transactions.
pub const ADOPTION_PARTS: u8 = 2;
/// The zone's channel (LEZ local.json `bedrock_config.channel_id`).
pub const CHANNEL_ID: Root = [1; 32];
/// Native ETH as FerrataVault v5 registers it (`vault/script/DeployVault.s.sol` `_ethParams`).
pub const ETH: [u8; 20] = [0; 20];
pub const ETH_DENOMINATIONS: [u128; 3] = [1_000_000_000_000_000, 10_000_000_000_000_000, 100_000_000_000_000_000];
pub const ETH_RELAY_FEES: [u128; 3] = [2_000_000_000_000, 20_000_000_000_000, 200_000_000_000_000];
pub const ETH_CHECKPOINT_FEES: [u128; 3] = [1_000_000_000_000, 10_000_000_000_000, 100_000_000_000_000];
pub const ETH_LEZ_UNIT: u128 = 1_000_000_000_000;
pub const ETH_UNIT_WEIGHT: u128 = 3000;
/// The depositor's bridge seed (`bridge/e2e`'s test value; the inbound plan; `e2e/crown/identity.py`).
pub const BRIDGE_SEED_LABEL: &[u8] = b"e2e: user bridge seed (test value)";
/// The release recipient written into the burn notes: the run's deployer.
pub const DEFAULT_RECIPIENT: &str = "0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07";
/// The stand-in's beacon slots: in Sepolia's period 1409 (Gloas).
const STAND_IN_BOOTSTRAP_SLOT: u64 = 1409 * 8192 + 64;
/// How far back from a capture's block the ETH schedule 0's time is searched (registration
/// happens in the deploy, hours before the deposits).
const FEES_AT_SEARCH_SECONDS: u64 = 30 * 24 * 3600;

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn h(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn image_bytes(id: [u32; 8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, w) in id.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

fn hex_n<const N: usize>(text: &str) -> Result<[u8; N]> {
    hex::decode(text.trim().trim_start_matches("0x"))?.try_into().map_err(|_| anyhow!("{text}: not {N} bytes"))
}

pub fn address(text: &str) -> Result<[u8; 20]> {
    hex_n::<20>(text)
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------
// The configuration
// ---------------------------------------------------------------------------------------------

/// The roles of the configuration and its caps: no floor, unlimited caps (e2e/crown/README.md
/// "Identity of the configuration"). The admin and the freeze authority are throwaway accounts of
/// derived keys (they act by signing, which admits their accounts); the fee recipient only
/// receives, and a public account must exist to be admitted, so it is the second of the funded
/// test accounts of LEZ's local genesis (`keys::lez_test_payers()[3]`, a published throwaway key).
pub fn controls() -> Controls {
    let unlimited = CapParams { capacity: UNLIMITED, refill_per_second: 0 };
    let fee_recipient = account_of(&lez_test_payers().expect("LEZ's test keys")[3]);
    Controls {
        admin: *account_of(&derived_key("crown/admin")).value(),
        freeze_authority: *account_of(&derived_key("crown/freeze-authority")).value(),
        fee_recipient: *fee_recipient.value(),
        floor: CapParams { capacity: 0, refill_per_second: 0 },
        token_cap: unlimited,
        global_cap: unlimited,
    }
}

pub fn fee_recipient() -> AccountId {
    AccountId::new(controls().fee_recipient)
}

/// ETH's registry entry as the vault publishes it.
pub fn eth_entry() -> TokenEntry {
    TokenEntry {
        token: ETH,
        lez_definition: [0; 32],
        decimals: 18,
        lez_unit: ETH_LEZ_UNIT,
        unit_weight: ETH_UNIT_WEIGHT,
        denominations: ETH_DENOMINATIONS.to_vec(),
        relay_fees: ETH_RELAY_FEES.to_vec(),
    }
}

/// ETH's schedule 0 as `registerToken` publishes it at time `t` (FerrataRegistry: from leaf 0,
/// announced and effective at the registration block's timestamp, no mint or burn fee).
pub fn eth_schedule_zero(t: u64) -> FeeSchedule {
    FeeSchedule {
        index: 0,
        from_leaf: 0,
        announced_at: t,
        effective_at: t,
        mint_fees: vec![0; 3],
        burn_fees: vec![0; 3],
        checkpoint_fees: ETH_CHECKPOINT_FEES.to_vec(),
    }
}

/// What session A fixes: the zone (genesis, channel, tip), the programs and the instance.
#[derive(Clone, Debug)]
pub struct Identity {
    pub label: String,
    pub chain: String,
    pub tip_id: u64,
    pub tip_hash: Root,
    pub bedrock_genesis_id: Root,
    pub channel_id: Root,
    pub channel_keys: Vec<Root>,
    pub posting_timeframe: u64,
    pub config_tip_hash: Root,
    pub attestation: AccountId,
    pub attestation_image_id: Root,
    pub bridge_header: AccountId,
    pub bridge_image_id: Root,
    pub instance_id: Root,
    pub checkpoint: Root,
}

impl Identity {
    pub fn bridge_config(&self, vault: [u8; 20]) -> BridgeConfig {
        BridgeConfig {
            bedrock_genesis_id: self.bedrock_genesis_id,
            channel_id: self.channel_id,
            bridge_image_id: self.bridge_image_id,
            attestation: self.attestation,
            instance_id: self.instance_id,
            evm_chain_id: SEPOLIA_CHAIN_ID,
            vault,
            controls: controls(),
        }
    }

    /// The 261-byte preimage of `configId`, written out by hand (e2e/crown/README.md), and checked
    /// against the bridge core's.
    pub fn config_preimage(&self, vault: [u8; 20]) -> Result<Vec<u8>> {
        let config = self.bridge_config(vault);
        let mut p = b"ferrata/config/v1".to_vec();
        p.extend_from_slice(&self.bedrock_genesis_id);
        p.extend_from_slice(&self.channel_id);
        p.extend_from_slice(self.bridge_header.value());
        p.extend_from_slice(&self.bridge_image_id);
        p.extend_from_slice(&chain_id_u256(SEPOLIA_CHAIN_ID));
        p.extend_from_slice(&vault);
        p.extend_from_slice(&config.attestation_root_id());
        p.extend_from_slice(&config.salt());
        ensure!(p.len() == 261, "the configId preimage is {} bytes", p.len());
        ensure!(sha256(&p) == config.id(self.bridge_header), "the configId preimage is not the bridge core's");
        Ok(p)
    }

    pub fn config_json(&self, vault: [u8; 20]) -> Value {
        let config = self.bridge_config(vault);
        let preimage = self.config_preimage(vault).map(|p| h(&p)).unwrap_or_else(|e| format!("ERROR: {e}"));
        json!({ "vault": h(&vault), "configId": h(&config.id(self.bridge_header)), "preimage": preimage })
    }

    pub fn to_json(&self, vault: Option<[u8; 20]>) -> Value {
        let sample = self.bridge_config([0; 20]);
        let c = controls();
        let cap = |p: &CapParams| json!({ "capacity": p.capacity.to_string(), "refillPerSecond": p.refill_per_second.to_string() });
        let mut out = json!({
            "label": self.label,
            "chain": self.chain,
            "tip": { "id": self.tip_id, "hash": h(&self.tip_hash) },
            "channel": {
                "id": h(&self.channel_id),
                "keys": self.channel_keys.iter().map(|k| h(k)).collect::<Vec<_>>(),
                "postingTimeframe": self.posting_timeframe,
                "configTipHash": h(&self.config_tip_hash),
            },
            "bedrockGenesisId": h(&self.bedrock_genesis_id),
            "evmChainId": SEPOLIA_CHAIN_ID,
            "attestation": { "account": h(self.attestation.value()), "imageId": h(&self.attestation_image_id) },
            "bridge": { "header": h(self.bridge_header.value()), "imageId": h(&self.bridge_image_id) },
            "instance": { "id": h(&self.instance_id), "checkpoint": h(&self.checkpoint) },
            "attestationRootId": h(&sample.attestation_root_id()),
            "controls": {
                "admin": h(&c.admin), "freezeAuthority": h(&c.freeze_authority), "feeRecipient": h(&c.fee_recipient),
                "floor": cap(&c.floor), "tokenCap": cap(&c.token_cap), "globalCap": cap(&c.global_cap),
                "borsh": h(&borsh::to_vec(&c).expect("borsh")),
            },
            "salt": h(&sample.salt()),
            "eth": {
                "registrySlot": h(&registry_slot(&ETH)), "feeSlot0": h(&fee_slot(&ETH, 0)), "feeSlot1": h(&fee_slot(&ETH, 1)),
                "entryHash": h(&eth_entry().hash()),
            },
        });
        if let Some(v) = vault {
            out["vault"] = json!(h(&v));
            out["configId"] = json!(h(&self.bridge_config(v).id(self.bridge_header)));
        }
        out
    }

    pub fn load(path: &Path) -> Result<Self> {
        let v: Value = serde_json::from_slice(&fs::read(path).with_context(|| format!("{}", path.display()))?)?;
        let s = |p: &str| -> Result<String> {
            v.pointer(p).and_then(Value::as_str).map(str::to_string).ok_or_else(|| anyhow!("{}: no {p}", path.display()))
        };
        let r = |p: &str| -> Result<Root> { hex_n::<32>(&s(p)?) };
        let keys = v
            .pointer("/channel/keys")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("no /channel/keys"))?
            .iter()
            .map(|k| hex_n::<32>(k.as_str().unwrap_or("")))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            label: s("/label")?,
            chain: s("/chain")?,
            tip_id: v.pointer("/tip/id").and_then(Value::as_u64).ok_or_else(|| anyhow!("no /tip/id"))?,
            tip_hash: r("/tip/hash")?,
            bedrock_genesis_id: r("/bedrockGenesisId")?,
            channel_id: r("/channel/id")?,
            channel_keys: keys,
            posting_timeframe: v.pointer("/channel/postingTimeframe").and_then(Value::as_u64).unwrap_or(0),
            config_tip_hash: r("/channel/configTipHash")?,
            attestation: AccountId::new(r("/attestation/account")?),
            attestation_image_id: r("/attestation/imageId")?,
            bridge_header: AccountId::new(r("/bridge/header")?),
            bridge_image_id: r("/bridge/imageId")?,
            instance_id: r("/instance/id")?,
            checkpoint: r("/instance/checkpoint")?,
        })
    }
}

/// The vault slots a capture must prove (every one the session attests).
pub fn capture_slots() -> Value {
    let mut slots = vec![
        json!({ "slot": h(&SLOT_ROOT), "what": "depositRoot (slot 0)" }),
        json!({ "slot": h(&SLOT_COUNT), "what": "nextLeafIndex (slot 1)" }),
        json!({ "slot": h(&SLOT_CONFIG_ID), "what": "storedLezConfigId (slot 2)" }),
    ];
    let names = ["registry entry of ETH", "fee schedule 0 of ETH", "fee schedule 1 of ETH"];
    for (s, what) in register_slots(&ETH)[..3].iter().zip(names) {
        slots.push(json!({ "slot": h(s), "what": what }));
    }
    json!({ "slots": slots, "fetch_attest_fixture_storage": format!(
        "{{vault}}:0=depositRoot(),1=nextLeafIndex(),2=storedLezConfigId(),{},{},{}",
        h(&registry_slot(&ETH)), h(&fee_slot(&ETH, 0)), h(&fee_slot(&ETH, 1))) })
}

// ---------------------------------------------------------------------------------------------
// The chain the sessions read
// ---------------------------------------------------------------------------------------------

/// One `Deposit` of the vault.
#[derive(Clone, Debug)]
pub struct DepositLog {
    pub leaf_index: u64,
    pub depositor: [u8; 20],
    pub token: [u8; 20],
    pub amount: [u8; 32],
    pub commitment: [u8; 32],
}

impl DepositLog {
    pub fn leaf(&self) -> Root {
        deposit_leaf(self.leaf_index, &self.depositor, &self.token, &self.amount, &self.commitment)
    }
}

/// One capture: a finality update whose finalized header binds `block`, and the vault there.
pub struct Stage {
    pub label: String,
    pub update: LightClientUpdate,
    pub block: u64,
    pub timestamp: u64,
    pub header_rlp: Vec<u8>,
    pub vault: [u8; 20],
    /// `eth_getProof`'s `accountProof` of the vault.
    pub account: Vec<Vec<u8>>,
    /// Every proven storage slot: (key, proof, value).
    pub slots: Vec<([u8; 32], Vec<Vec<u8>>, [u8; 32])>,
    /// Every deposit up to `block`, in leaf order.
    pub deposits: Vec<DepositLog>,
}

impl Stage {
    pub fn slot(&self, key: &[u8; 32]) -> Result<(Vec<Vec<u8>>, [u8; 32])> {
        self.slots
            .iter()
            .find(|(k, ..)| k == key)
            .map(|(_, p, v)| (p.clone(), *v))
            .ok_or_else(|| anyhow!("{}: the capture has no proof of slot {} (crown-driver capture-slots)", self.label, h(key)))
    }

    pub fn vault_proof(&self, slots: &[[u8; 32]]) -> Result<VaultProof> {
        Ok(VaultProof {
            account: with_header(self.header_rlp.clone(), self.account.clone()),
            slots: slots.iter().map(|s| self.slot(s).map(|(p, _)| p)).collect::<Result<_>>()?,
        })
    }
}

pub struct Chain {
    pub name: String,
    pub stand_in: bool,
    pub config: Config,
    pub checkpoint: Root,
    pub bootstrap: LightClientBootstrap,
    pub bootstrap_fork: LcFork,
    /// (period, update), in order; the first `base_updates` are session A's.
    pub catch_up: Vec<(u64, LightClientUpdate)>,
    pub base_updates: usize,
    pub stages: Vec<Stage>,
}

fn read(dir: &Path, name: &str) -> Result<Vec<u8>> {
    fs::read(dir.join(name)).map_err(|e| anyhow!("{}: {e}", dir.join(name).display()))
}

/// The light-client layouts a fixture's objects may be in.
const LAYOUTS: [LcFork; 5] = [LcFork::Altair, LcFork::Capella, LcFork::Deneb, LcFork::Electra, LcFork::Gloas];

/// The light-client layout of the fork that `config`'s schedule puts at `slot`.
fn layout_at(config: &Config, slot: u64) -> Option<LcFork> {
    let spec = config.spec();
    spec.fork_at_epoch(spec.epoch_at_slot(slot)).lc_fork()
}

/// `Config::store_fork`'s byte of a layout (`Config::fork` read backwards).
fn store_fork_of(layout: LcFork) -> u8 {
    match layout {
        LcFork::Altair => 0,
        LcFork::Capella => 1,
        LcFork::Deneb => 2,
        LcFork::Electra => 3,
        LcFork::Gloas => 4,
    }
}

/// `bootstrap.ssz` in the layout of the fork at its own slot (the encoding does not name its layout).
fn decode_bootstrap(config: &Config, bytes: &[u8]) -> Result<(LightClientBootstrap, LcFork)> {
    let n = config.sync_committee_size as usize;
    LAYOUTS
        .into_iter()
        .find_map(|layout| {
            let b = LightClientBootstrap::decode(bytes, layout, n).ok()?;
            (layout_at(config, b.header.beacon.slot) == Some(layout)).then_some((b, layout))
        })
        .ok_or_else(|| anyhow!("bootstrap.ssz decodes in no layout of the fork at its slot"))
}

/// An `update_<p>.ssz` in the layout of the fork at its attested slot (as `native_apply` upgrades the store).
fn decode_update(config: &Config, bytes: &[u8], name: &str) -> Result<LightClientUpdate> {
    let n = config.sync_committee_size as usize;
    LAYOUTS
        .into_iter()
        .find_map(|layout| {
            let u = LightClientUpdate::decode(bytes, layout, n).ok()?;
            (layout_at(config, u.attested_header.beacon.slot) == Some(layout)).then_some(u)
        })
        .ok_or_else(|| anyhow!("{name} decodes in no layout of the fork at its attested slot"))
}

/// The block root the second beacon provider reported as canonical at `slot`
/// (`beacon_header_<slot>_second.json`, its answer kept byte for byte by the fetch scripts).
fn second_provider_root(fixture: &Path, slot: u64) -> Result<Root> {
    let name = format!("beacon_header_{slot}_second.json");
    let v: Value = serde_json::from_slice(&read(fixture, &name)?).with_context(|| name.clone())?;
    let data = &v["data"];
    ensure!(data["header"]["message"]["slot"].as_str() == Some(slot.to_string().as_str()), "{name}: not the header at slot {slot}");
    ensure!(data["canonical"].as_bool() != Some(false), "{name}: not canonical");
    hex_n::<32>(data["root"].as_str().ok_or_else(|| anyhow!("{name}: no data.root"))?)
}

/// Sepolia from a checkpoint fixture (`--fixture DIR`; `build.sh pack` copies what this reads into the bundle):
/// `bootstrap.ssz` in the layout of the fork at its slot, which sets the store's layout; the checkpoint = the root the
/// second beacon provider reported at that slot (`beacon_header_<slot>_second.json`), which `Init` and the native store
/// check the bootstrap against; and every `update_<p>.ssz` present, for the periods from the bootstrap's on without a
/// gap, each signed in its period: session A's catch-up. The crown run's fixture is
/// `crates/ferrata-spectests/fixtures/sepolia-2026-11-12` (period 1411); `sepolia-gloas-2026-11-10` (period 1378,
/// 32 updates) reads the same way, but its checkpoint left the weak-subjectivity period on 21 Oct 2026.
pub fn sepolia_base(fixture: &Path) -> Result<Chain> {
    let mut config = sepolia_config(ADOPTION_PARTS);
    let spec = config.spec();
    let (bootstrap, bootstrap_fork) = decode_bootstrap(&config, &read(fixture, "bootstrap.ssz")?)?;
    config.store_fork = store_fork_of(bootstrap_fork);
    let slot = bootstrap.header.beacon.slot;
    let checkpoint = second_provider_root(fixture, slot)?;
    ensure!(
        bootstrap.header.beacon.hash_tree_root() == checkpoint,
        "bootstrap.ssz (slot {slot}) does not hash to the checkpoint 0x{} of beacon_header_{slot}_second.json",
        hex::encode(checkpoint)
    );
    let mut periods = Vec::new();
    for entry in fs::read_dir(fixture).with_context(|| fixture.display().to_string())? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if let Some(p) = name.strip_prefix("update_").and_then(|r| r.strip_suffix(".ssz")) {
            periods.push(p.parse::<u64>().map_err(|_| anyhow!("{name}: not update_<period>.ssz"))?);
        }
    }
    periods.sort_unstable();
    let first = spec.period_at_slot(slot);
    ensure!(!periods.is_empty(), "{}: no update_<period>.ssz: the store would not learn the next committee", fixture.display());
    ensure!(
        periods.iter().enumerate().all(|(i, p)| *p == first + i as u64),
        "{}: the updates' periods {periods:?} are not {first}, {}, ... without a gap",
        fixture.display(),
        first + 1
    );
    let mut catch_up = Vec::new();
    for p in periods {
        let name = format!("update_{p}.ssz");
        let update = decode_update(&config, &read(fixture, &name)?, &name)?;
        ensure!(spec.period_at_slot(update.signature_slot) == p, "{name} is not signed in period {p}");
        catch_up.push((p, update));
    }
    let base_updates = catch_up.len();
    Ok(Chain {
        name: "crown sepolia".into(),
        stand_in: false,
        config,
        checkpoint,
        bootstrap,
        bootstrap_fork,
        catch_up,
        base_updates,
        stages: Vec::new(),
    })
}

/// The first slot outside the weak-subjectivity period of a store whose finalized header is at `slot`, and the time
/// it starts (Unix ms): the exclusive end of every transaction's validity window on that store.
pub fn ws_end(config: &Config, slot: u64) -> Result<(u64, u64)> {
    let end = config.ws_end_slot(slot).ok_or_else(|| anyhow!("the configuration has no weak-subjectivity period"))?;
    Ok((end, config.slot_start_ms(end)))
}

/// Unix milliseconds as `YYYY-MM-DDTHH:MM:SSZ`.
fn utc(ms: u64) -> String {
    let s = ms / 1000;
    let (days, rem) = (s / 86_400, s % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// A Gloas-layout light-client header at `slot` binding `block_hash`, whose execution branch
/// verifies against its own body root.
fn gloas_header(slot: u64, block_hash: Root, state_root: Root, tag: u64) -> LightClientHeader {
    let (body_root, branches) = sparse_tree(&[(EXECUTION_BLOCK_HASH_GINDEX_GLOAS, block_hash)], tag);
    LightClientHeader {
        beacon: BeaconBlockHeader { slot, proposer_index: 7, parent_root: filler(tag + 20), state_root, body_root },
        execution: Execution::BlockHash { block_hash, branch: branches[0].clone() },
    }
}

/// The stand-in (rehearsals only): Sepolia's config in the Gloas layout, from a bootstrap made
/// here (a 512-key committee whose keys we hold).
pub fn stand_in_base() -> Result<Chain> {
    let mut config = sepolia_config(ADOPTION_PARTS);
    config.store_fork = 4; // Gloas
    let fake = fake_committee(512, 0x5e);
    let block0 = sha256(b"ferrata-crown/stand-in/bootstrap execution block (not a block)");
    let (state_root, branches) = sparse_tree(&[(CURRENT_SYNC_COMMITTEE_GINDEX_GLOAS, fake.committee.hash_tree_root())], 9_001);
    let header0 = gloas_header(STAND_IN_BOOTSTRAP_SLOT, block0, state_root, 9_100);
    let bootstrap = LightClientBootstrap {
        header: header0.clone(),
        current_sync_committee: fake.committee.clone(),
        current_sync_committee_branch: branches[0].clone(),
    };
    Ok(Chain {
        name: "crown STAND-IN".into(),
        stand_in: true,
        checkpoint: header0.beacon.hash_tree_root(),
        config,
        bootstrap,
        bootstrap_fork: LcFork::Gloas,
        catch_up: Vec::new(),
        base_updates: 0,
        stages: Vec::new(),
    })
}

/// The stand-in committee's finality update for stage `i` (1-based), finalizing `block_hash`.
fn stand_in_update(config: &Config, i: u64, block_hash: Root) -> Result<LightClientUpdate> {
    let fake = fake_committee(512, 0x5e);
    let spec = config.spec();
    let ctx = CoreContext { spec: &spec, genesis_validators_root: config.genesis_validators_root, bls: &BLS };
    let finalized_slot = STAND_IN_BOOTSTRAP_SLOT + 64 * i;
    let finalized = gloas_header(finalized_slot, block_hash, filler(9_200 + i), 9_300 + 100 * i);
    let (attested_state, branches) = sparse_tree(&[(FINALIZED_ROOT_GINDEX_GLOAS, finalized.beacon.hash_tree_root())], 9_400 + i);
    let attested = gloas_header(finalized_slot + 32, filler(9_500 + i), attested_state, 9_600 + 100 * i);
    let bits = vec![true; 512];
    let mut update = LightClientUpdate {
        attested_header: attested,
        next_sync_committee: SyncCommittee::empty(512),
        next_sync_committee_branch: vec![ZERO; LcFork::Gloas.sync_committee_branch_len()],
        finalized_header: finalized,
        finality_branch: branches[0].clone(),
        sync_aggregate: SyncAggregate { bits: bits.clone(), signature: [0; 96] },
        signature_slot: finalized_slot + 33,
    };
    update.sync_aggregate.signature = sign(&fake, &bits, &sync_committee_signing_root(&update, &ctx));
    Ok(update)
}

/// A Gloas `LightClientFinalityUpdate` (every field fixed-size) as the update it stands for.
pub fn gloas_finality_update(b: &[u8]) -> Result<LightClientUpdate> {
    const H: usize = 112 + 32 + 11 * 32;
    ensure!(b.len() == 2 * H + 9 * 32 + 160 + 8, "not a Gloas LightClientFinalityUpdate ({} bytes)", b.len());
    let header = |r: &[u8]| LightClientHeader::decode(r, LcFork::Gloas).map_err(|e| anyhow!("header: {e:?}"));
    Ok(LightClientUpdate {
        attested_header: header(&b[..H])?,
        next_sync_committee: SyncCommittee::empty(512),
        next_sync_committee_branch: vec![ZERO; LcFork::Gloas.sync_committee_branch_len()],
        finalized_header: header(&b[H..2 * H])?,
        finality_branch: b[2 * H..2 * H + 288].chunks_exact(32).map(|c| c.try_into().unwrap()).collect(),
        sync_aggregate: SyncAggregate::decode(&b[2 * H + 288..2 * H + 448], 512).map_err(|e| anyhow!("aggregate: {e:?}"))?,
        signature_slot: u64::from_le_bytes(b[2 * H + 448..].try_into()?),
    })
}

/// The `proof_<vault>_<block>.json` of a capture, whatever the case of the address.
fn proof_file(dir: &Path, vault: &[u8; 20], block: u64) -> Result<Vec<u8>> {
    let want = format!("proof_0x{}_{block}.json", hex::encode(vault));
    for entry in fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().to_string();
        if name.to_lowercase() == want {
            return read(dir, &name);
        }
    }
    bail!("{}: no {want}", dir.display())
}

/// Add the captures (`lez/scripts/fetch-attest-fixture.py --network sepolia`, or
/// `lez/crown/capture-anvil.py` for the stand-in), one stage each, in order.
pub fn add_captures(chain: &mut Chain, captures: &[PathBuf]) -> Result<()> {
    for (i, dir) in captures.iter().enumerate() {
        let m: Value = serde_json::from_slice(&read(dir, "fixture.json")?)?;
        let block = m["block"].as_u64().ok_or_else(|| anyhow!("{}: fixture.json has no block", dir.display()))?;
        if !chain.stand_in {
            // A capture fetches every period's update from `--updates-from` (drive.py: the bundle fixture's last period
            // + 1); one of a period the base covers (an older capture) is not needed: the store is past it.
            let base_last = chain.catch_up[..chain.base_updates].iter().map(|(p, _)| *p).max();
            for p in m["updates"].as_array().into_iter().flatten() {
                let p = p.as_u64().ok_or_else(|| anyhow!("fixture.json: updates"))?;
                if base_last.is_some_and(|last| p <= last) {
                    continue;
                }
                if !chain.catch_up.iter().any(|(q, _)| *q == p) {
                    let name = format!("update_{p}.ssz");
                    chain.catch_up.push((p, decode_update(&chain.config, &read(dir, &name)?, &name)?));
                }
            }
        }
        let vault = address(m["storage"][0]["address"].as_str().ok_or_else(|| anyhow!("fixture.json: storage[0].address"))?)?;
        let proof = account_proof(&proof_file(dir, &vault, block)?);
        ensure!(proof.address == vault, "capture {}: the proof is of another account", i + 1);
        let logs: Vec<Value> = serde_json::from_slice(&read(dir, &format!("deposits_{block}.json"))?)?;
        let mut deposits = Vec::new();
        for v in &logs {
            let topics = v["topics"].as_array().ok_or_else(|| anyhow!("a Deposit log without topics"))?;
            let data = hex::decode(v["data"].as_str().unwrap_or("").trim_start_matches("0x"))?;
            ensure!(topics.len() == 3 && data.len() == 128, "not a Deposit log");
            let w = |k: usize| -> [u8; 32] { data[32 * k..32 * (k + 1)].try_into().unwrap() };
            let topic = |k: usize| -> Result<[u8; 20]> {
                Ok(hex::decode(topics[k].as_str().unwrap_or("").trim_start_matches("0x"))?[12..].try_into()?)
            };
            deposits.push(DepositLog {
                depositor: topic(1)?,
                leaf_index: u64::from_be_bytes(w(1)[24..].try_into()?),
                token: topic(2)?,
                amount: w(2),
                commitment: w(3),
            });
        }
        deposits.sort_by_key(|d| d.leaf_index);
        for (k, d) in deposits.iter().enumerate() {
            ensure!(d.leaf_index == k as u64, "capture {}: the deposits are not leaves 0..{}", i + 1, deposits.len());
        }
        let header_rlp = read(dir, "header.rlp")?;
        let decoded = ferrata_core::exec::decode_header(&header_rlp).map_err(|e| anyhow!("capture {}: header.rlp: {e:?}", i + 1))?;
        ensure!(decoded.number == block, "capture {}: header.rlp is block {}, not {block}", i + 1, decoded.number);
        let update = if chain.stand_in {
            stand_in_update(&chain.config, i as u64 + 1, keccak(&header_rlp))?
        } else {
            gloas_finality_update(&read(dir, "finality_update.ssz")?)?
        };
        if let Execution::BlockHash { block_hash, .. } = &update.finalized_header.execution {
            ensure!(*block_hash == keccak(&header_rlp), "capture {}: the finality update does not bind header.rlp", i + 1);
        }
        chain.stages.push(Stage {
            label: format!("stage {}", i + 1),
            update,
            block,
            timestamp: decoded.timestamp,
            header_rlp,
            vault,
            account: proof.proof.clone(),
            slots: proof.storage.iter().map(|s| (s.key, s.proof.clone(), s.value)).collect(),
            deposits,
        });
    }
    let base = chain.base_updates;
    chain.catch_up[base..].sort_by_key(|(p, _)| *p);
    if let Some(v) = chain.stages.first().map(|s| s.vault) {
        ensure!(chain.stages.iter().all(|s| s.vault == v), "the captures are of different vaults");
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Deposits and burns
// ---------------------------------------------------------------------------------------------

fn bridge_keys() -> BridgeKeys {
    BridgeKeys::from_bridge_seed(&sha256(BRIDGE_SEED_LABEL))
}

/// Deposit `j` of the depositor (the vault's `depositCount` at deposit time): 0 public to LEZ's
/// public test account 0, 1 private to its private test account 0 (e2e/crown/README.md stages 4, 5).
pub fn destination(j: u64) -> Result<(DestKind, [u8; 32])> {
    match j {
        0 => Ok((DestKind::Public, *testnet_initial_state::initial_pub_accounts_private_keys()[0].account_id.value())),
        1 => Ok((DestKind::Private, *testnet_initial_state::initial_priv_accounts_private_keys()[0].account_id().value())),
        _ => bail!("the run has two deposits (j 0 and 1), not {j}"),
    }
}

/// The claim witness of the deposit at `leaf` among `deposits`, its nullifier and its kind.
pub fn witness(vault: &[u8; 20], deposits: &[DepositLog], leaf: u64) -> Result<(ClaimWitness, Root, DestKind)> {
    let chain_id = SEPOLIA_CHAIN_ID;
    let d = deposits.get(leaf as usize).ok_or_else(|| anyhow!("no deposit at leaf {leaf}"))?;
    let j = deposits[..leaf as usize].iter().filter(|e| e.depositor == d.depositor).count() as u64;
    let (kind, dest) = destination(j)?;
    let s = deposit_secret(&bridge_keys().auth, &chain_id_u256(chain_id), vault, &d.depositor, j);
    ensure!(commitment(&s, kind, &dest) == d.commitment, "leaf {leaf}: C is not the run's opening (j {j})");
    let tree = Tree::from_leaves(deposits.iter().map(DepositLog::leaf).collect()).map_err(|_| anyhow!("tree full"))?;
    let path = tree.path(leaf).ok_or_else(|| anyhow!("leaf {leaf} is not in the tree"))?;
    let n = core_nullifier(&s, &deposit_id(&chain_id_u256(chain_id), vault, leaf, &d.leaf()));
    let w = ClaimWitness {
        depositor: d.depositor,
        token: d.token,
        amount: d.amount,
        leaf_index: leaf,
        path: path.siblings.to_vec(),
        s,
        kind: kind as u8,
        dest,
    };
    Ok((w, n, kind))
}

/// The wallet's `i`-th burn note under `cid` for `recipient` on Ethereum, and its `sOut`.
pub fn burn_note(cid: &Root, vault: &[u8; 20], i: u64, recipient: &[u8; 20], amount: u128) -> (Note, Root) {
    let keys = bridge_keys();
    let r = burn_tag(&keys.view, cid, i);
    let s_out = burn_secret(&keys.auth, cid, &r);
    let c_out = burn_commitment(&s_out, &chain_id_u256(SEPOLIA_CHAIN_ID), vault, recipient);
    let memo = xor_memo(&keys.view, &r, recipient);
    let amount = word_of_u128(amount);
    let mac = burn_mac_v2(&keys.view, &r, &memo, &ETH, &amount, &c_out);
    (Note { r, memo, mac, token: ETH, amount, c_out }, s_out)
}

/// The time of ETH's schedule 0: the one that makes its hash the value the vault holds at fee
/// slot 0 (searched in the 30 days before the capture's block).
pub fn find_fees_at(stage: &Stage) -> Result<u64> {
    let (_, want) = stage.slot(&fee_slot(&ETH, 0))?;
    ensure!(want != [0; 32], "{}: the vault's fee slot 0 of ETH is empty (ETH not registered?)", stage.label);
    let to = stage.timestamp;
    for t in (to.saturating_sub(FEES_AT_SEARCH_SECONDS)..=to).rev() {
        if eth_schedule_zero(t).hash(&ETH) == want {
            return Ok(t);
        }
    }
    bail!("{}: no time in the 30 days before block {} gives ETH's schedule 0 hash {} (other fees?)", stage.label, stage.block, h(&want))
}

// ---------------------------------------------------------------------------------------------
// Native checks
// ---------------------------------------------------------------------------------------------

fn native_apply(native: &mut Native, config: &Config, update: &LightClientUpdate, label: &str) -> Result<ApplyOutcome> {
    let spec = config.spec();
    let layout = spec.fork_at_epoch(spec.epoch_at_slot(update.attested_header.beacon.slot)).lc_fork().unwrap_or(native.core().fork);
    if layout > native.core().fork {
        native.store.upgrade(layout, &native.spec).map_err(|e| anyhow!("{label}: upgrade: {e:?}"))?;
    }
    native.process(update).map_err(|e| anyhow!("{label}: the native store refuses it: {e:?}"))
}

/// Without a sequencer: the chain through the native light client (session A's catch-up first, as the sessions
/// apply it), the bridge's attestation requests under each record through the attestation program's own evaluation,
/// the registry entry and schedule against the vault's slots, every deposit's opening; and, by this machine's clock,
/// whether the next session's transactions fall inside the weak-subjectivity period the program bounds them by
/// (without `identity`: session A's, bounded by the bootstrap's slot; with it: session B's, by the finalized header
/// session A left).
pub fn check(chain: &Chain, program: &GuestBinary, bridge: &GuestBinary, identity: Option<&Identity>) -> Result<()> {
    use ferrata_core::witness::HeaderRecord;
    use ferrata_lez_bridge_wire::{import_request, pairing_request, vault_request};
    use ferrata_lez_wire::attest::evaluate;
    let attest = deployment_accounts(ATTEST, program).0;
    let bridge_header = deployment_accounts(BRIDGE, bridge).0;
    let inst_id = instance_id(&chain.config, &chain.checkpoint);
    println!("[check] {}: instance 0x{} of the attestation program at {attest}; bridge at {bridge_header}", chain.name, hex::encode(inst_id));
    let mut native = Native::with_fork(&chain.config, &chain.checkpoint, &chain.bootstrap, chain.bootstrap_fork)?;
    let boot_slot = chain.bootstrap.header.beacon.slot;
    println!(
        "[check] {}: bootstrap slot {boot_slot} (period {}, {:?} layout, store_fork {}), checkpoint 0x{}: accepted by the native store",
        chain.name,
        boot_slot / 8192,
        chain.bootstrap_fork,
        chain.config.store_fork,
        hex::encode(chain.checkpoint)
    );
    for (p, update) in &chain.catch_up[..chain.base_updates] {
        let o = native_apply(&mut native, &chain.config, update, &format!("period {p}"))?;
        println!(
            "[check] period {p} update (attested {}, finalized {}): advanced {}, adopted {}, rotated {}",
            update.attested_header.beacon.slot,
            update.finalized_header.beacon.slot,
            o.advanced,
            o.adopted,
            o.rotated
        );
    }
    let after_a = native.core().finalized_header.beacon.slot;
    println!(
        "[check] after session A's {} update(s): finalized slot {after_a}, next sync committee {}",
        chain.base_updates,
        if native.core().is_next_sync_committee_known() { "known" } else { "UNKNOWN" }
    );
    let (bound_slot, what) = if identity.is_none() { (boot_slot, "session A (Init and its catch-up)") } else { (after_a, "session B") };
    let (end_slot, end_ms) = ws_end(&chain.config, bound_slot)?;
    let now = now_ms();
    println!(
        "[check] {what}: valid until slot {end_slot} ({}), the weak-subjectivity end of slot {bound_slot}; now {}: {}",
        utc(end_ms),
        utc(now),
        if now < end_ms { "inside" } else { "OUTSIDE" }
    );
    ensure!(
        now < end_ms,
        "{what} would be refused: the program's validity window ended at {} (\"Execution outside of the validity window\"); a newer checkpoint is needed",
        utc(end_ms)
    );
    let mut applied = chain.base_updates;
    for stage in &chain.stages {
        let period = stage.update.signature_slot / 8192;
        while applied < chain.catch_up.len() && chain.catch_up[applied].0 < period {
            let (p, update) = &chain.catch_up[applied];
            native_apply(&mut native, &chain.config, update, &format!("period {p}"))?;
            applied += 1;
        }
        let outcome = native_apply(&mut native, &chain.config, &stage.update, &stage.label)?;
        ensure!(outcome.advanced, "{}: no new finalized header", stage.label);
        let core = native.core();
        let record = HeaderRecord::of(&core.finalized_header, core.fork, &native.spec);
        let tree = Tree::from_leaves(stage.deposits.iter().map(DepositLog::leaf).collect()).map_err(|_| anyhow!("tree"))?;
        ensure!(tree.root() == stage.slot(&SLOT_ROOT)?.1, "{}: slot 0 is not the root of the deposits", stage.label);
        ensure!(stage.slot(&SLOT_COUNT)?.1 == word_of(stage.deposits.len() as u64), "{}: slot 1 is not the deposit count", stage.label);
        for leaf in 0..stage.deposits.len() as u64 {
            witness(&stage.vault, &stage.deposits, leaf)?;
        }
        let entry_ok = stage.slot(&registry_slot(&ETH)).map(|(_, v)| v == eth_entry().hash()).unwrap_or(false);
        let fees_at = find_fees_at(stage).ok();
        println!(
            "[check] {}: finalized slot {} binds block {} (timestamp {}); {} deposit(s) open to the run; ETH entry {}; schedule 0 time {:?}",
            stage.label,
            core.finalized_header.beacon.slot,
            stage.block,
            stage.timestamp,
            stage.deposits.len(),
            if entry_ok { "matches the vault" } else { "DOES NOT match the vault" },
            fees_at
        );
        if identity.is_none() {
            // Without session A's identity: the import request (the vault's account and slots 0 and 1 under
            // this record) does not depend on the configuration's identity, only on the vault.
            let placeholder = Identity {
                label: String::new(),
                chain: String::new(),
                tip_id: 0,
                tip_hash: [0; 32],
                bedrock_genesis_id: [0; 32],
                channel_id: CHANNEL_ID,
                channel_keys: Vec::new(),
                posting_timeframe: 0,
                config_tip_hash: [0; 32],
                attestation: attest,
                attestation_image_id: image_bytes(program.image_id),
                bridge_header,
                bridge_image_id: image_bytes(bridge.image_id),
                instance_id: inst_id,
                checkpoint: chain.checkpoint,
            };
            let config = placeholder.bridge_config(stage.vault);
            let account = with_header(stage.header_rlp.clone(), stage.account.clone());
            let import = import_request(&config, &ImportProof { account, root_slot: stage.slot(&SLOT_ROOT)?.0, count_slot: stage.slot(&SLOT_COUNT)?.0 });
            evaluate(&inst_id, chain.config.chain_id, &record, &import).map_err(|c| anyhow!("{}: import refused natively ({c})", stage.label))?;
            println!("[check] {}: the import request (vault account, slots 0 and 1, header in front) accepted natively under this record", stage.label);
        }
        if let Some(identity) = identity {
            ensure!(identity.instance_id == inst_id, "session A's instance is not this chain's");
            ensure!(identity.attestation == attest && identity.bridge_header == bridge_header, "session A's programs are not these");
            let config = identity.bridge_config(stage.vault);
            let cid = config.id(bridge_header);
            ensure!(stage.slot(&SLOT_CONFIG_ID)?.1 == cid, "{}: the vault's slot 2 is not configId 0x{}", stage.label, hex::encode(cid));
            let account = with_header(stage.header_rlp.clone(), stage.account.clone());
            let pairing = pairing_request(&config, &PairingProof { account: account.clone(), config_slot: stage.slot(&SLOT_CONFIG_ID)?.0 });
            evaluate(&inst_id, chain.config.chain_id, &record, &pairing).map_err(|c| anyhow!("{}: pairing refused natively ({c})", stage.label))?;
            let import = import_request(&config, &ImportProof { account, root_slot: stage.slot(&SLOT_ROOT)?.0, count_slot: stage.slot(&SLOT_COUNT)?.0 });
            evaluate(&inst_id, chain.config.chain_id, &record, &import).map_err(|c| anyhow!("{}: import refused natively ({c})", stage.label))?;
            let reg = register_slots(&ETH);
            let request = vault_request(&config, &reg, &stage.vault_proof(&reg)?).map_err(|c| anyhow!("register request: {c}"))?;
            evaluate(&inst_id, chain.config.chain_id, &record, &request).map_err(|c| anyhow!("{}: register request refused natively ({c})", stage.label))?;
            println!("[check] {}: configId 0x{} in slot 2; pairing, import and register requests accepted natively", stage.label, hex::encode(cid));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The sessions
// ---------------------------------------------------------------------------------------------

fn payers() -> Result<Vec<AccountId>> {
    Ok(lez_test_payers()?.iter().map(account_of).collect())
}

fn tx(step: &str, message: &BridgeMessage, payer: AccountId) -> Result<Tx> {
    Ok(Tx { step: step.to_string(), harness: None, actors: Vec::new(), root: 0, message: borsh::to_vec(message)?, payer, gas_limit: None, note: String::new() })
}

async fn send(live: &mut Live, step: &str, mentions: Vec<AccountMention>, message: &BridgeMessage, payer: AccountId) -> Result<Outcome> {
    live.submit_mentions(tx(step, message, payer)?, mentions).await
}

async fn bridge_state(live: &Live, pda: Pda) -> Result<Option<BridgeState>> {
    let state = live.actor_state(pda.actor()).await?;
    Ok(if state.is_empty() { None } else { Some(borsh::from_slice(&state)?) })
}

async fn holder_state(live: &Live, bridge: AccountId, account: AccountId) -> Result<Option<BridgeState>> {
    let state = live.actor_state(holder_pda(bridge, account).actor()).await?;
    Ok(if state.is_empty() { None } else { Some(borsh::from_slice(&state)?) })
}

/// `holder(account)`: the PDA holding `account`'s public wrapped balances (security fix C-1); a claim
/// mints there, a public burn forwards from there.
fn holder_pda(bridge: AccountId, account: AccountId) -> Pda {
    Pda { program: bridge, seed: holder_seed(account.value()) }
}

/// The fee schedule range a wallet names for a claim of `leaf`: the schedule it falls under and
/// the widest range the token's state allows (as the harness's `range`).
async fn range_for(live: &Live, token: Pda, leaf: u64) -> Result<ScheduleRange> {
    let Some(BridgeState::Token(t)) = bridge_state(live, token).await? else { bail!("token(cid, ETH) is not registered") };
    let j = t.schedules.iter().rposition(|s| s.from_leaf <= leaf).unwrap_or(0);
    let until = t.schedules.get(j + 1).map_or(t.known_until, |s| s.from_leaf);
    ensure!(leaf < until, "leaf {leaf} is beyond the token's known schedules (known until {})", t.known_until);
    Ok(ScheduleRange { index: j as u64, from_leaf: t.schedules[j].from_leaf, until_leaf: until })
}

fn row_json(o: &Outcome) -> Value {
    json!({ "step": o.row.step, "status": o.row.status, "exit": o.row.exit, "block": o.row.block, "tx": o.row.tx_hash, "execGas": o.row.exec_gas })
}

pub struct SessionA {
    pub channel: PathBuf,
    pub genesis: PathBuf,
    pub identity_out: PathBuf,
    pub vault: Option<[u8; 20]>,
}

/// Session A: deploy, the instance and its catch-up, then the identity at the last final block.
pub async fn session_a(live: &mut Live, program: &GuestBinary, bridge_bin: &GuestBinary, chain: &Chain, a: &SessionA) -> Result<()> {
    let payer = payers()?[1];
    let attest = scenario::deploy(live, ATTEST, program, payer).await?;
    let bridge = scenario::deploy(live, BRIDGE, bridge_bin, payer).await?;
    let mut world = World::with_checkpoint(&chain.name, attest, chain.config.clone(), chain.checkpoint, &chain.bootstrap, chain.bootstrap_fork)?;
    println!("[crown-a] {}: attestation program {attest}, bridge {bridge}, instance 0x{}", chain.name, hex::encode(world.inst.id));
    ensure!(scenario::instance_state(live, &world.inst).await?.is_none(), "the instance exists already: session A starts on a fresh zone");
    scenario::init(live, &mut world, &chain.bootstrap, &format!("{} Init", chain.name), Harness::None, payer).await?;
    for (p, update) in &chain.catch_up[..chain.base_updates] {
        let run = scenario::apply(live, &mut world, update, &format!("period {p} update"), Harness::None, payer).await?;
        let o = run.outcome.ok_or_else(|| anyhow!("period {p} update: not applied (native store and sequencer refuse it)"))?;
        println!("[crown-a] period {p} update applied: advanced {}, adopted {}, rotated {}", o.advanced, o.adopted, o.rotated);
    }
    ensure!(
        chain.stand_in || world.native.core().is_next_sync_committee_known(),
        "after the catch-up the instance does not know the next sync committee"
    );
    live.sync().await?;
    let tip = live.follower.blocks.last().ok_or_else(|| anyhow!("no block followed"))?;
    let (tip_id, tip_hash) = (tip.header.block_id, tip.header.hash.0);

    let channel: Value = serde_json::from_slice(&fs::read(&a.channel)?).context("the node's /channel answer")?;
    let keys = channel["accredited_keys"]
        .as_array()
        .ok_or_else(|| anyhow!("/channel: no accredited_keys"))?
        .iter()
        .map(|k| hex_n::<32>(k.as_str().unwrap_or("")))
        .collect::<Result<Vec<_>>>()?;
    ensure!(keys.len() == 1, "the channel holds {} keys, the run expects one", keys.len());
    let identity = Identity {
        label: if chain.stand_in { "STAND-IN rehearsal (beacon committee held by the driver; not Sepolia)".into() } else { "crown".into() },
        chain: if chain.stand_in { "stand-in".into() } else { "sepolia".into() },
        tip_id,
        tip_hash,
        bedrock_genesis_id: sha256(&fs::read(&a.genesis)?),
        channel_id: CHANNEL_ID,
        channel_keys: keys,
        posting_timeframe: channel["posting_timeframe"].as_u64().ok_or_else(|| anyhow!("/channel: posting_timeframe"))?,
        config_tip_hash: hex_n::<32>(channel["config_tip_hash"].as_str().unwrap_or(""))?,
        attestation: attest,
        attestation_image_id: image_bytes(program.image_id),
        bridge_header: bridge,
        bridge_image_id: image_bytes(bridge_bin.image_id),
        instance_id: world.inst.id,
        checkpoint: chain.checkpoint,
    };
    if let Some(v) = a.vault {
        identity.config_preimage(v)?;
    }
    let out = identity.to_json(a.vault);
    fs::write(&a.identity_out, serde_json::to_string_pretty(&out)? + "\n")?;
    println!("[crown-a] T0 = block {tip_id} 0x{}; identity written to {}", hex::encode(tip_hash), a.identity_out.display());
    live.save()
}

pub struct SessionB {
    pub identity: Identity,
    /// A claim's time is now + this (seconds; default -30): the including block's timestamp must lie in
    /// [t, t + 10 min), so a slow prover needs a positive offset (about its proof time minus a few minutes).
    pub claim_time_offset_s: i64,
    pub eth_fees_at: Option<u64>,
    pub recipient: [u8; 20],
    pub private: String,
    pub session_out: PathBuf,
    pub notes_out: PathBuf,
    /// Only the operator's and the relayer's steps (the finality updates, `Init`, `Register`, `ImportFees`,
    /// `ImportRoot`): no claim and no burn, which a user's own wallet sends (e2e/seqnet: the `ferrata` tool).
    pub operator_only: bool,
}

/// Session B: the captures, the bridge's pairing, registration and imports, the claims and the
/// burns; then the session's record and the burn notes.
pub async fn session_b(live: &mut Live, program: &GuestBinary, bridge_bin: &GuestBinary, chain: &Chain, b: &SessionB) -> Result<()> {
    let payers = payers()?;
    let (payer, holder0) = (payers[1], payers[0]);
    ensure!(*holder0.value() == destination(0)?.1, "LEZ's public test account 0 is deposit 0's destination");
    let attest = deployment_accounts(ATTEST, program).0;
    let bridge = deployment_accounts(BRIDGE, bridge_bin).0;
    let id = &b.identity;
    ensure!(id.attestation == attest && id.bridge_header == bridge, "session A deployed other programs");
    ensure!(id.bridge_image_id == image_bytes(bridge_bin.image_id), "session A's bridge image is not --bridge-bin's");
    ensure!(!chain.stages.is_empty(), "session B needs the captures (--capture DIR, in order)");

    // The instance as session A left it: the native store replays A's updates without sending.
    let mut world = World::with_checkpoint(&chain.name, attest, chain.config.clone(), chain.checkpoint, &chain.bootstrap, chain.bootstrap_fork)?;
    ensure!(world.inst.id == id.instance_id, "session A's instance is not this chain's");
    ensure!(scenario::instance_state(live, &world.inst).await?.is_some(), "the instance does not exist: session A has not run on this zone");
    for (p, update) in &chain.catch_up[..chain.base_updates] {
        native_apply(&mut world.native, &chain.config, update, &format!("period {p} (session A's)"))?;
    }
    scenario::check_store(live, &world).await.context("the instance is not where session A left it")?;

    let vault = chain.stages[0].vault;
    let config = id.bridge_config(vault);
    let cid = config.id(bridge);
    id.config_preimage(vault)?;
    println!("[crown-b] {}: vault 0x{}, configId 0x{}, instance 0x{}", chain.name, hex::encode(vault), hex::encode(cid), hex::encode(world.inst.id));
    let chain_id = SEPOLIA_CHAIN_ID;
    let config_pda = Pda { program: bridge, seed: config_seed(&cid) };
    let token_pda = Pda { program: bridge, seed: token_seed(&cid, &ETH) };
    let mut steps: Vec<Value> = Vec::new();
    let mut claims: Vec<Value> = Vec::new();
    let mut notes: Vec<Value> = Vec::new();
    let mut applied = chain.base_updates;
    for (k, stage) in chain.stages.iter().enumerate() {
        let period = stage.update.signature_slot / 8192;
        while applied < chain.catch_up.len() && chain.catch_up[applied].0 < period {
            let (p, update) = &chain.catch_up[applied];
            scenario::apply(live, &mut world, update, &format!("period {p} update"), Harness::None, payer).await?;
            applied += 1;
        }
        let run = scenario::apply(live, &mut world, &stage.update, &format!("{} finality update (block {})", stage.label, stage.block), Harness::None, payer).await?;
        ensure!(run.outcome.is_some_and(|o| o.advanced), "{}: the finality update does not finalize a new header", stage.label);
        let slot = stage.update.finalized_header.beacon.slot;
        let (slot2_proof, slot2) = stage.slot(&SLOT_CONFIG_ID)?;
        ensure!(slot2 == cid, "{}: the vault's slot 2 is {}, not this configuration's id", stage.label, h(&slot2));
        let header_pda = world.inst.header(slot);
        let account = with_header(stage.header_rlp.clone(), stage.account.clone());
        let n = stage.deposits.len() as u64;

        if k == 0 {
            let init = BridgeMessage::Init { config: config.clone(), record_slot: slot, proof: PairingProof { account: account.clone(), config_slot: slot2_proof } };
            let o = send(live, "bridge Init (pairing: configId in the vault's slot 2)", vec![mention(config_pda), mention(header_pda)], &init, payer).await?.expect_applied()?;
            steps.push(row_json(&o));
            ensure!(matches!(bridge_state(live, config_pda).await?, Some(BridgeState::Config(ConfigState::Ready { .. }))), "config(cid) is not ready");

            let entry = eth_entry();
            ensure!(stage.slot(&registry_slot(&ETH))?.1 == entry.hash(), "{}: the vault's ETH registry entry is not DeployVault v5's", stage.label);
            let t = match b.eth_fees_at { Some(t) => t, None => find_fees_at(stage)? };
            let schedule = eth_schedule_zero(t);
            ensure!(stage.slot(&fee_slot(&ETH, 0))?.1 == schedule.hash(&ETH), "{}: ETH's schedule 0 at {t} is not the vault's", stage.label);
            println!("[crown-b] ETH schedule 0 announced and effective at {t}");
            let reg = register_slots(&ETH);
            let message = BridgeMessage::Register { config_id: cid, entry, schedule, record_slot: slot, proof: stage.vault_proof(&reg)? };
            let o = send(live, "bridge Register ETH (registry entry, schedules 0 and 1, leaf count)", vec![mention(config_pda), mention(header_pda), mention(token_pda)], &message, payer).await?.expect_applied()?;
            steps.push(row_json(&o));
        } else {
            let (_, fee1) = stage.slot(&fee_slot(&ETH, 1))?;
            ensure!(fee1 == [0; 32], "{}: ETH has a schedule 1; the run expects none", stage.label);
            let message = BridgeMessage::ImportFees { config_id: cid, token: ETH, index: 1, schedule: None, record_slot: slot, proof: stage.vault_proof(&fees_slots(&ETH, 1))? };
            let o = send(live, &format!("bridge ImportFees: ETH schedule 1 absent at n={n}"), vec![mention(config_pda), mention(header_pda), mention(token_pda)], &message, payer).await?.expect_applied()?;
            steps.push(row_json(&o));
        }
        if let Some(BridgeState::Token(t)) = bridge_state(live, token_pda).await? {
            println!("[crown-b] token(cid, ETH): {} schedule(s), known until leaf {}", t.schedules.len(), t.known_until);
        }

        let root_pda = Pda { program: bridge, seed: root_seed(&cid, n) };
        let (root_proof, root_value) = stage.slot(&SLOT_ROOT)?;
        let (count_proof, count_value) = stage.slot(&SLOT_COUNT)?;
        ensure!(count_value == word_of(n), "{}: slot 1 is not the number of deposits", stage.label);
        let import = BridgeMessage::ImportRoot {
            config_id: cid,
            record_slot: slot,
            root: root_value,
            leaf_count: n,
            proof: ImportProof { account: account.clone(), root_slot: root_proof, count_slot: count_proof },
        };
        let o = send(live, &format!("bridge ImportRoot n={n} ({}, block {})", stage.label, stage.block), vec![mention(config_pda), mention(header_pda), mention(root_pda)], &import, payer).await?.expect_applied()?;
        steps.push(row_json(&o));

        if b.operator_only {
            continue;
        }
        if k == 0 {
            // Claim of deposit 0 (public), relay fee to the fee recipient (no relayer).
            let (w, nullifier, kind) = witness(&vault, &stage.deposits, 0)?;
            ensure!(kind == DestKind::Public, "deposit 0 is the public one");
            let range = range_for(live, token_pda, 0).await?;
            let n_pda = Pda { program: bridge, seed: nullifier_seed(chain_id, &vault, &nullifier) };
            let dest = AccountId::new(w.dest);
            let message = BridgeMessage::Claim { config_id: cid, leaf_count: n, witness: w, range, relayer: None };
            let mentions = vec![mention(root_pda), mention(token_pda), mention(config_pda), mention(n_pda), mention(holder_pda(bridge, dest)), mention(holder_pda(bridge, fee_recipient()))];
            let o = send(live, &format!("bridge Claim: deposit 0 (public) under root(cid, {n})"), mentions, &message, payer).await?.expect_applied()?;
            steps.push(row_json(&o));
            claims.push(json!({ "leaf": 0, "kind": "public", "nullifier": h(&nullifier), "block": o.row.block, "tx": o.row.tx_hash, "dest": h(dest.value()) }));
            println!("[crown-b] public test account 0's holding: {:?}", holder_state(live, bridge, dest).await?);
        } else {
            // The private claim of deposit 1, then the two burns.
            let mut session = PrivateSession::open(live, bridge_bin, &b.private).await?;
            let (w, nullifier, kind) = witness(&vault, &stage.deposits, 1)?;
            ensure!(kind == DestKind::Private && w.dest == *session.me.value(), "deposit 1 is the private one, to the wallet's private account");
            let range = range_for(live, token_pda, 1).await?;
            let n_pda = Pda { program: bridge, seed: nullifier_seed(chain_id, &vault, &nullifier) };
            let message = BridgeMessage::ClaimPrivately { config_id: cid, leaf_count: n, witness: w, range, relayer: None };
            let publics = vec![mention(root_pda), mention(token_pda), mention(config_pda), mention(n_pda), mention(holder_pda(bridge, fee_recipient()))];
            let o = session.send(live, "bridge ClaimPrivately: deposit 1 at LEZ's private test account 0", publics, &message, "claim").await?;
            steps.push(row_json(&o));
            if session.fake {
                println!("[crown-b] fake receipt dropped as expected; no burns without a real private claim");
                break;
            }
            ensure!(matches!(bridge_state(live, n_pda).await?, Some(BridgeState::Nullifier(_))), "the nullifier is not spent after the private claim");
            claims.push(json!({ "leaf": 1, "kind": "private", "nullifier": h(&nullifier), "block": o.row.block, "tx": o.row.tx_hash }));

            // Burn public: one denomination of public test account 0, note 0.
            let amount = ETH_DENOMINATIONS[0];
            let (note, s_out) = burn_note(&cid, &vault, 0, &b.recipient, amount);
            let burn_pda = Pda { program: bridge, seed: burn_seed(&cid, &note.hash()) };
            let mentions = vec![
                AccountIdentity::Public(holder0).select_program_actor_state(bridge),
                mention(holder_pda(bridge, holder0)),
                mention(token_pda),
                mention(config_pda),
                mention(burn_pda),
            ];
            let o = send(live, "bridge Burn (public): 0.001 ETH of public test account 0, note 0", mentions, &BridgeMessage::burn(cid, &note), payer).await?.expect_applied()?;
            steps.push(row_json(&o));
            ensure!(matches!(bridge_state(live, burn_pda).await?, Some(BridgeState::Burn(_))), "the public burn is not recorded");
            notes.push(note_json(0, "public", &cid, &vault, &note, &s_out, &b.recipient, &burn_pda, &o));

            // Burn private: one denomination of private test account 0, note 1.
            session.refresh(live, bridge).await?;
            let (note, s_out) = burn_note(&cid, &vault, 1, &b.recipient, amount);
            let burn_pda = Pda { program: bridge, seed: burn_seed(&cid, &note.hash()) };
            let publics = vec![mention(token_pda), mention(config_pda), mention(burn_pda)];
            let o = session.send(live, "bridge Burn (private): 0.001 ETH of private test account 0, note 1", publics, &BridgeMessage::burn(cid, &note), "burn").await?;
            steps.push(row_json(&o));
            ensure!(matches!(bridge_state(live, burn_pda).await?, Some(BridgeState::Burn(_))), "the private burn is not recorded");
            notes.push(note_json(1, "private", &cid, &vault, &note, &s_out, &b.recipient, &burn_pda, &o));
        }
    }

    // T1: every block up to the last one followed is final on Bedrock.
    live.sync().await?;
    let tip = live.follower.blocks.last().ok_or_else(|| anyhow!("no block followed"))?;
    let (t1, t1_hash) = (tip.header.block_id, tip.header.hash.0);
    let session = json!({
        "label": id.label,
        "chain": id.chain,
        "tipT0": { "id": id.tip_id, "hash": h(&id.tip_hash) },
        "tipT1": { "id": t1, "hash": h(&t1_hash) },
        "blocks": { "from": id.tip_id + 1, "to": t1 },
        "configId": h(&cid),
        "vault": h(&vault),
        "instance": h(&world.inst.id),
        "steps": steps,
        "claims": claims,
    });
    fs::write(&b.session_out, serde_json::to_string_pretty(&session)? + "\n")?;
    fs::write(&b.notes_out, serde_json::to_string_pretty(&notes)? + "\n")?;
    println!("[crown-b] blocks {}..={t1} (T1 0x{}); {} burn note(s) written to {}", id.tip_id + 1, hex::encode(t1_hash), notes.len(), b.notes_out.display());
    live.save()
}

#[allow(clippy::too_many_arguments)]
fn note_json(i: u64, kind: &str, cid: &Root, vault: &[u8; 20], note: &Note, s_out: &Root, recipient: &[u8; 20], burn: &Pda, o: &Outcome) -> Value {
    json!({
        "i": i, "kind": kind, "configId": h(cid), "evmChainId": SEPOLIA_CHAIN_ID, "vault": h(vault),
        "token": h(&note.token), "denomIndex": 0, "amount": ETH_DENOMINATIONS[0].to_string(),
        "r": h(&note.r), "memo": h(&note.memo), "mac": h(&note.mac), "cOut": h(&note.c_out), "noteHash": h(&note.hash()),
        "burnAccount": h(burn.account().value()), "sOut": h(s_out), "nOut": h(&nf_out_v2(s_out)), "recipient": h(recipient),
        "block": o.row.block, "tx": o.row.tx_hash,
    })
}

/// LEZ's wallet holding the private test account 0, for the session's private transactions:
/// each is executed and proven by the wallet (r0vm as LEZ's prover), sent, and settled.
struct PrivateSession {
    wallet: wallet::WalletCore<TapClient>,
    me: AccountId,
    bridge: AccountId,
    catalog: ProgramCatalog,
    dir: PathBuf,
    fake: bool,
}

impl PrivateSession {
    async fn open(live: &mut Live, bridge_bin: &GuestBinary, private: &str) -> Result<Self> {
        let fake = match private {
            "real" => false,
            "fake" => true,
            other => bail!("--private {other}: real or fake"),
        };
        ensure!(
            std::env::var_os("RISC0_DEV_MODE").is_some_and(|v| v == "1") == fake,
            "RISC0_DEV_MODE must be 1 for --private fake and unset for --private real"
        );
        let dir = live.opts.run_dir.join(format!("crown-private-{private}"));
        fs::create_dir_all(&dir)?;
        live.sync().await?;
        let client = TapClient::recording(live.wallet.client_owned(), &dir.join("rpc-transcript.jsonl"), &dir.join("transaction.borsh"))?;
        let (mut wallet, me) = private_wallet(&dir.join("wallet"), &live.opts.rpc, client)?;
        let tip = wallet.start_at_chain_tip().await?;
        println!("[crown-b] private wallet at block {tip}: account {me}");
        let bridge = deployment_accounts(BRIDGE, bridge_bin).0;
        let catalog = ProgramCatalog::from([(bridge, Program::new(Cow::Owned(bridge_bin.bytes.clone()))?)]);
        Ok(Self { wallet, me, bridge, catalog, dir, fake })
    }

    /// Prove and send `message` at this account's actor with `publics`, and settle it.
    async fn send(&mut self, live: &mut Live, step: &str, publics: Vec<AccountMention>, message: &BridgeMessage, name: &str) -> Result<Outcome> {
        let mut mentions = vec![AccountIdentity::PrivateOwned(self.me).select_program_actor_state(self.bridge)];
        mentions.extend(publics);
        let step = format!("{step} ({})", if self.fake { "fake receipt" } else { "real proof" });
        let tx_out = self.dir.join("transaction.borsh");
        let _ = fs::remove_file(&tx_out);
        live.rows.push(Row { phase: live.phase.clone(), step: step.clone(), status: "unfinished".into(), note: "the proof had not finished when the run was recorded".into(), ..Row::default() });
        live.save()?;
        let pending = live.rows.len() - 1;
        let started = Instant::now();
        let sent = self.wallet.send_privacy_preserving_tx_with_pre_check(mentions, 0, borsh::to_vec(message)?, &self.catalog, None, |_| Ok(())).await;
        let seconds = started.elapsed().as_secs_f64();
        let since_head = live.head().await?;
        live.rows.remove(pending);
        let hash = match sent {
            Ok((hash, _)) => hash,
            Err(error) => {
                let note = format!("after {seconds:.1} s: {error}: {error:?}");
                println!("[crown-b] {step}: rejected: {note}");
                live.rows.push(Row { phase: live.phase.clone(), step, status: "rejected".into(), note, ..Row::default() });
                live.save()?;
                bail!("the private transaction was not sent");
            }
        };
        let bytes = fs::metadata(&tx_out).map(|m| m.len()).unwrap_or(0);
        println!("[crown-b] {step}: executed, proven and sent in {seconds:.1} s ({bytes} bytes)");
        let outcome = live
            .settle(Sent {
                step: step.clone(),
                hash,
                sent_at: Instant::now(),
                since_head,
                payer: None,
                balance_before: 0,
                harness: None,
                note: format!("execute_and_prove and submission {seconds:.1} s; transaction {bytes} bytes"),
            })
            .await?;
        if self.fake {
            ensure!(outcome.row.status == "dropped", "a sequencer that verifies receipts must drop a fake one; it {} it", outcome.row.status);
        } else {
            ensure!(outcome.applied(), "{step}: {} {} {}", outcome.row.status, outcome.row.exit, outcome.row.note);
            fs::create_dir_all(&live.opts.out_dir)?;
            fs::copy(&tx_out, live.opts.out_dir.join(format!("crown-private-{name}-tx.borsh")))?;
        }
        live.save()?;
        Ok(outcome)
    }

    /// Follow the chain to the head so that the wallet holds its account's state after the
    /// last private transaction (it follows its account's nullifiers).
    async fn refresh(&mut self, live: &mut Live, bridge: AccountId) -> Result<()> {
        live.sync().await?;
        let height = live.follower.height();
        self.wallet.sync_to_block(height).await?;
        let account = self.wallet.get_account_private(self.me).ok_or_else(|| anyhow!("the wallet lost its private account"))?;
        let state = account.data.actor_state(bridge).to_vec();
        let holding: Option<HolderState> = if state.is_empty() {
            None
        } else {
            match borsh::from_slice::<BridgeState>(&state)? {
                BridgeState::Holder(h) => Some(h),
                other => bail!("the private account's bridge state is not a holder: {other:?}"),
            }
        };
        println!("[crown-b] private test account 0 after the claim (wallet's view at block {height}): {holding:?}");
        ensure!(
            holding.is_some_and(|h| h.holdings.iter().any(|x| x.token == ETH && x.amount >= ETH_DENOMINATIONS[0])),
            "the wallet does not see the claimed 0.001 ETH on its private account"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../crates/ferrata-spectests/fixtures").join(name)
    }

    /// Session A's catch-up through the native store; the outcomes in order.
    fn replay(chain: &Chain) -> (Native, Vec<ApplyOutcome>) {
        let mut native = Native::with_fork(&chain.config, &chain.checkpoint, &chain.bootstrap, chain.bootstrap_fork).expect("bootstrap");
        let outcomes = chain.catch_up[..chain.base_updates]
            .iter()
            .map(|(p, u)| native_apply(&mut native, &chain.config, u, &format!("period {p}")).expect("update"))
            .collect();
        (native, outcomes)
    }

    #[test]
    fn utc_formats_unix_ms() {
        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        let config = sepolia_config(ADOPTION_PARTS);
        assert_eq!(utc(config.slot_start_ms(11_288_576)), "2026-10-05T10:35:12Z");
        assert_eq!(utc(config.slot_start_ms(11_562_112)), "2026-11-12T10:22:24Z");
    }

    /// The crown run's chain: the Gloas bootstrap of period 1411 and that period's update, which finalizes an older
    /// header but carries the next committee the fresh store lacks; inside its weak-subjectivity period until
    /// 28 Nov 2026 03:13:36 UTC.
    #[test]
    fn crown_fixture_reads_and_replays() {
        let chain = sepolia_base(&fixture("sepolia-2026-11-12")).expect("sepolia_base");
        assert_eq!(chain.bootstrap_fork, LcFork::Gloas);
        assert_eq!(chain.config.store_fork, 4);
        assert_eq!(hex::encode(chain.checkpoint), "3b901f5e6853b5cf5eb04d7ccfe1272e66c38291cb3a11dd159cacd1117ccf7c");
        assert_eq!(chain.bootstrap.header.beacon.slot, 11_562_112);
        assert_eq!(chain.catch_up.iter().map(|(p, _)| *p).collect::<Vec<_>>(), vec![1411]);
        assert_eq!(chain.base_updates, 1);
        let (native, outcomes) = replay(&chain);
        assert_eq!(outcomes, vec![ApplyOutcome { rotated: false, advanced: false, adopted: true }]);
        assert!(native.core().is_next_sync_committee_known());
        assert_eq!(native.core().finalized_header.beacon.slot, 11_562_112);
        let (end, end_ms) = ws_end(&chain.config, 11_562_112).unwrap();
        assert_eq!((end, utc(end_ms).as_str()), (11_675_168, "2026-11-28T03:13:36Z"));
    }

    /// The fixture crown-live-2 ran: read the same way (Electra bootstrap, 32 updates into Gloas), and its checkpoint's
    /// weak-subjectivity period ended on 21 Oct 2026, three weeks before the run's Init.
    #[test]
    fn replay_fixture_reads_and_is_outside_its_period() {
        let chain = sepolia_base(&fixture("sepolia-gloas-2026-11-10")).expect("sepolia_base");
        assert_eq!(chain.bootstrap_fork, LcFork::Electra);
        assert_eq!(chain.config.store_fork, 3);
        assert_eq!(hex::encode(chain.checkpoint), crate::plan::SEPOLIA_CHECKPOINT);
        assert_eq!(chain.catch_up.iter().map(|(p, _)| *p).collect::<Vec<_>>(), (1378..=1409).collect::<Vec<_>>());
        let (native, outcomes) = replay(&chain);
        assert!(outcomes.iter().all(|o| o.advanced));
        assert_eq!(native.core().finalized_header.beacon.slot, 11_542_528);
        let (end, end_ms) = ws_end(&chain.config, chain.bootstrap.header.beacon.slot).unwrap();
        assert_eq!((end, utc(end_ms).as_str()), (11_401_632, "2026-10-21T03:26:24Z"));
        assert!(end_ms < chain.config.slot_start_ms(11_562_112));
    }

    /// The stand-in's fixed bootstrap slot (rehearsals) is bounded the same way.
    #[test]
    fn stand_in_period_end() {
        let chain = stand_in_base().unwrap();
        let (_, end_ms) = ws_end(&chain.config, chain.bootstrap.header.beacon.slot).unwrap();
        assert_eq!(utc(end_ms), "2026-11-25T10:09:36Z");
    }
}
