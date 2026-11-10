//! The inbound run (`../inbound/README.md`): the first crossing from Ethereum to LEZ on a live
//! sequencer. Deposits in Ferrata's vault, attested from a finalized beacon chain by the
//! attestation program, imported by the bridge program and claimed, one by the plaintext path and
//! one privately with a real proof, with the refusals.
//!
//! Two chains of the same shape ([`InboundChain`]): Sepolia as `lez/scripts/fetch-attest-fixture.py`
//! captures it after each deposit's finality ([`sepolia`]), and a stand-in ([`stand_in`]), clearly
//! not Ethereum, for the session before the deposits exist: a beacon chain made here with a
//! committee whose keys we hold, in the Gloas layout, whose finalized headers bind execution blocks
//! made from a real Sepolia header with the state root of a synthetic state holding the vault.
//!
//! The configuration's identity, which the vault pins (`configId`) before the zone exists, is
//! fixed in advance: the two programs' header accounts come from the run's derived keys
//! (`program/<name>/header`, independent of the image and of the order of deployment), the
//! bridge's image id from its frozen binary, the attestation instance from the Sepolia config and
//! the bootstrap's block root; the zone's Bedrock genesis and channel, which a scripted session's
//! local devnet only fixes when it starts and which the program does not check, are fixed tags.

use std::{borrow::Cow, fs, path::Path, path::PathBuf, time::Instant};

use anyhow::{anyhow, bail, ensure, Result};
use ferrata_bridge_core::{
    chain_id_u256,
    inbound::{commitment, deposit_id, deposit_leaf, deposit_secret, nullifier as core_nullifier, DestKind},
    tree::Tree,
    BridgeKeys,
};
use ferrata_core::{
    fork::LcFork,
    merkle::{CURRENT_SYNC_COMMITTEE_GINDEX_GLOAS, EXECUTION_BLOCK_HASH_GINDEX_GLOAS, FINALIZED_ROOT_GINDEX_GLOAS, ZERO},
    types::{
        BeaconBlockHeader, Execution, LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate,
        SyncCommittee,
    },
    validate::{sync_committee_signing_root, Context as CoreContext},
    Root,
};
use ferrata_lez_bridge_wire::{
    code, config_seed, nullifier_seed, root_seed, with_header, BridgeConfig, BridgeMessage, BridgeState, ClaimWitness,
    ConfigState, ImportProof, PairingProof, Terms, SLOT_CONFIG_ID, SLOT_COUNT, SLOT_ROOT,
};
use ferrata_lez_wire::{instance_id, Config};
use ferrata_spectests::{
    execkit::{account_proof, account_rlp, enc, header_rlp, keccak, Trie},
    testkit::{fake_committee, filler, sign, sparse_tree},
};
use lee::{privacy_preserving_transaction::circuit::ProgramCatalog, program::Program, AccountId, Actor};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use wallet::{AccountIdentity, AccountMention};

use crate::{
    live::{mention, Live, Outcome, Sent, Tx},
    manifest::Row,
    plan::{root, sepolia_config, Pda, BLS, SEPOLIA_CHAIN_ID, SEPOLIA_CHECKPOINT},
    private::{private_wallet, TapClient},
    programs::GuestBinary,
    scenario::{self, deployment_accounts, Harness, World},
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

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub fn image_bytes(id: [u32; 8]) -> [u8; 32] {
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

/// The configuration of the vault at `vault` reading instance `instance_id`, with the programs at
/// their run accounts.
pub fn bridge_config(attest: AccountId, bridge_image_id: [u8; 32], instance_id: Root, vault: [u8; 20]) -> BridgeConfig {
    BridgeConfig {
        bedrock_genesis_id: sha256(BEDROCK_GENESIS_TAG),
        channel_id: sha256(CHANNEL_TAG),
        bridge_image_id,
        attestation: attest,
        instance_id,
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
    let sample = bridge_config(attest_account, bridge_image, instance, [0; 20]);
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
        let config = bridge_config(attest_account, bridge_image, instance, bytes);
        out["config_id"] = json!({ "vault": hex::encode(bytes), "config_id": hex::encode(config.id(bridge_account)) });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// The chain the session reads
// ---------------------------------------------------------------------------------------------

/// The seed of the depositor's bridge keys (`bridge/e2e`'s test value; `sepolia-plan.json`).
pub const BRIDGE_SEED_LABEL: &[u8] = b"e2e: user bridge seed (test value)";

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

/// One stage: a finality update whose finalized header binds `block`, and the vault there.
pub struct Stage {
    pub label: String,
    pub update: LightClientUpdate,
    pub block: u64,
    pub header_rlp: Vec<u8>,
    /// `eth_getProof`'s `accountProof` of the vault, and `storageProof[i].proof` of slots 0, 1, 2.
    pub account: Vec<Vec<u8>>,
    pub slots: [Vec<Vec<u8>>; 3],
    pub slot_values: [[u8; 32]; 3],
    /// Every deposit up to `block`, in leaf order.
    pub deposits: Vec<DepositLog>,
}

pub struct InboundChain {
    pub name: String,
    pub config: Config,
    pub checkpoint: Root,
    pub bootstrap: LightClientBootstrap,
    pub bootstrap_fork: LcFork,
    /// (period, update) to apply before the stages, in order; a stage is applied once every update of
    /// a period before its own is.
    pub catch_up: Vec<(u64, LightClientUpdate)>,
    pub vault: [u8; 20],
    pub stages: Vec<Stage>,
}

/// What the depositor knows of deposit `j` (`sepolia-plan.json` `deposits`): its kind and LEZ
/// destination. Deposit 0 is public to LEZ's public test account 0, deposit 1 private to its
/// private test account 0.
pub fn destination(j: u64) -> Result<(DestKind, [u8; 32])> {
    match j {
        0 => Ok((DestKind::Public, *testnet_initial_state::initial_pub_accounts_private_keys()[0].account_id.value())),
        1 => Ok((DestKind::Private, *testnet_initial_state::initial_priv_accounts_private_keys()[0].account_id().value())),
        _ => bail!("the plan has two deposits (j 0 and 1), not {j}"),
    }
}

fn bridge_keys() -> BridgeKeys {
    BridgeKeys::from_bridge_seed(&sha256(BRIDGE_SEED_LABEL))
}

/// The claim witness of the deposit at `leaf` among `deposits`, its nullifier, and its kind.
pub fn witness(chain_id: u64, vault: &[u8; 20], deposits: &[DepositLog], leaf: u64) -> Result<(ClaimWitness, Root, DestKind)> {
    let d = deposits.get(leaf as usize).ok_or_else(|| anyhow!("no deposit at leaf {leaf}"))?;
    // j: the depositor's deposits before this one (the vault's depositCount at deposit time).
    let j = deposits[..leaf as usize].iter().filter(|e| e.depositor == d.depositor).count() as u64;
    let (kind, dest) = destination(j)?;
    let s = deposit_secret(&bridge_keys().auth, &chain_id_u256(chain_id), vault, &d.depositor, j);
    ensure!(commitment(&s, kind, &dest) == d.commitment, "leaf {leaf}: C is not the plan's opening (j {j})");
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

// ---------------------------------------------------------------------------------------------
// The stand-in chain
// ---------------------------------------------------------------------------------------------

/// The stand-in vault's address and its depositor: not Ethereum accounts of anyone.
pub const STAND_IN_VAULT: [u8; 20] = [0x5e; 20];
pub const STAND_IN_DEPOSITOR: [u8; 20] = [0xd1; 20];
/// The stand-in's beacon slots: in Sepolia's period 1409 (Gloas), in the past of the session.
const STAND_IN_BOOTSTRAP_SLOT: u64 = 1409 * 8192 + 64;

fn floorlog2(g: u64) -> usize {
    63 - g.leading_zeros() as usize
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

/// A synthetic execution block: a real Sepolia header (`template`, the replay fixture's bound
/// block) renumbered, with the state root of a state holding the vault (slots 0, 1, 2) among 40
/// other accounts. Returns the stage's execution half.
fn vault_block(template: &Value, number: u64, deposits: &[DepositLog], config_id: Root) -> Result<(Vec<u8>, Root, Vec<Vec<u8>>, [Vec<Vec<u8>>; 3], [[u8; 32]; 3])> {
    let tree = Tree::from_leaves(deposits.iter().map(DepositLog::leaf).collect()).map_err(|_| anyhow!("tree full"))?;
    let mut count = [0u8; 32];
    count[24..].copy_from_slice(&(deposits.len() as u64).to_be_bytes());
    let values = [tree.root(), count, config_id];
    let keys = [SLOT_ROOT, SLOT_COUNT, SLOT_CONFIG_ID];
    let storage = Trie::new(keys.iter().zip(&values).map(|(k, v)| (keccak(k).to_vec(), enc::uint(v))));
    let mut accounts = vec![(STAND_IN_VAULT, account_rlp(1, 0, storage.root(), keccak(b"stand-in vault code")))];
    // Forty other accounts (the vault's own address, 0x5e…5e, is in their range: skipped).
    for i in (0x40..0x69u8).filter(|i| [*i; 20] != STAND_IN_VAULT) {
        accounts.push(([i; 20], account_rlp(u64::from(i), 1_000 + u128::from(i), keccak(&[0x80]), keccak(b""))));
    }
    let state = Trie::new(accounts.iter().map(|(a, rlp)| (keccak(a).to_vec(), rlp.clone())));
    let mut block = template.clone();
    block["stateRoot"] = Value::String(format!("0x{}", hex::encode(state.root())));
    block["number"] = Value::String(format!("0x{number:x}"));
    let rlp = header_rlp(&block);
    let hash = keccak(&rlp);
    let slots = [0usize, 1, 2].map(|i| storage.proof(&keccak(&keys[i])));
    Ok((rlp, hash, state.proof(&keccak(&STAND_IN_VAULT)), slots, values))
}

/// The stand-in: the Sepolia config with the store in the Gloas layout from a bootstrap made here
/// (a 512-key committee whose keys we hold; its execution block holds no vault), and two finality
/// updates signed by that committee finalizing blocks whose vault holds the two deposits of the
/// plan (to LEZ's test accounts): the first deposit, then both.
pub fn stand_in(attest: AccountId, bridge: &GuestBinary, repository: &Path) -> Result<InboundChain> {
    let template: Value = serde_json::from_slice(&fs::read(
        repository.join("crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10/block.json"),
    )?)?;
    let template = template["result"].clone();
    let mut config = sepolia_config(ADOPTION_PARTS);
    config.store_fork = 4; // Gloas
    let spec = config.spec();
    let fake = fake_committee(512, 0x5e);
    let committee: &SyncCommittee = &fake.committee;

    // The bootstrap: block 0 holds no vault.
    let (_, hash0, ..) = vault_block(&template, 1, &[], [0; 32])?;
    let (state_root, branches) = sparse_tree(&[(CURRENT_SYNC_COMMITTEE_GINDEX_GLOAS, committee.hash_tree_root())], 9_001);
    let header0 = gloas_header(STAND_IN_BOOTSTRAP_SLOT, hash0, state_root, 9_100);
    let bootstrap = LightClientBootstrap {
        header: header0.clone(),
        current_sync_committee: committee.clone(),
        current_sync_committee_branch: branches[0].clone(),
    };
    let checkpoint = header0.beacon.hash_tree_root();
    let instance = instance_id(&config, &checkpoint);
    let cid = bridge_config(attest, image_bytes(bridge.image_id), instance, STAND_IN_VAULT).id(deployment_accounts(BRIDGE, bridge).0);

    // The two deposits of the plan, made by the stand-in depositor.
    let keys = bridge_keys();
    let deposits: Vec<DepositLog> = (0..2u64)
        .map(|j| {
            let (kind, dest) = destination(j)?;
            let s = deposit_secret(&keys.auth, &chain_id_u256(SEPOLIA_CHAIN_ID), &STAND_IN_VAULT, &STAND_IN_DEPOSITOR, j);
            let mut amount = [0u8; 32];
            amount[16..].copy_from_slice(&DENOMINATIONS[0].to_be_bytes());
            Ok(DepositLog { leaf_index: j, depositor: STAND_IN_DEPOSITOR, token: ETH, amount, commitment: commitment(&s, kind, &dest) })
        })
        .collect::<Result<_>>()?;

    let ctx = CoreContext { spec: &spec, genesis_validators_root: config.genesis_validators_root, bls: &BLS };
    let mut stages = Vec::new();
    for i in 1..=2u64 {
        let number = 1 + i;
        let (header_rlp, hash, account, slots, slot_values) = vault_block(&template, number, &deposits[..i as usize], cid)?;
        let finalized_slot = STAND_IN_BOOTSTRAP_SLOT + 64 * i;
        let finalized = gloas_header(finalized_slot, hash, filler(9_200 + i), 9_300 + 100 * i);
        let (attested_state, branches) =
            sparse_tree(&[(FINALIZED_ROOT_GINDEX_GLOAS, finalized.beacon.hash_tree_root())], 9_400 + i);
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
        ensure!(floorlog2(FINALIZED_ROOT_GINDEX_GLOAS) == update.finality_branch.len(), "finality branch depth");
        stages.push(Stage {
            label: format!("stage {i}"),
            update,
            block: number,
            header_rlp,
            account,
            slots,
            slot_values,
            deposits: deposits[..i as usize].to_vec(),
        });
    }
    Ok(InboundChain {
        name: "stand-in".into(),
        config,
        checkpoint,
        bootstrap,
        bootstrap_fork: LcFork::Gloas,
        catch_up: Vec::new(),
        vault: STAND_IN_VAULT,
        stages,
    })
}

// ---------------------------------------------------------------------------------------------
// Sepolia, as captured
// ---------------------------------------------------------------------------------------------

fn read(dir: &Path, name: &str) -> Result<Vec<u8>> {
    fs::read(dir.join(name)).map_err(|e| anyhow!("{}: {e}", dir.join(name).display()))
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

/// Sepolia from the period-1378 bootstrap (the repository's replay fixture, every update to 1409)
/// to the captures of `lez/scripts/fetch-attest-fixture.py` (`sepolia-plan.json` `capture`), one
/// per stage.
pub fn sepolia(repository: &Path, captures: &[PathBuf], vault: Option<&str>) -> Result<InboundChain> {
    ensure!(!captures.is_empty(), "--capture DIR, once per stage");
    let replay = repository.join("crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10");
    let bootstrap = LightClientBootstrap::decode(&read(&replay, "bootstrap.ssz")?, LcFork::Electra, 512)
        .map_err(|e| anyhow!("bootstrap.ssz: {e:?}"))?;
    let mut catch_up = Vec::new();
    for period in 1378..=1409u64 {
        let layout = if period < 1379 { LcFork::Electra } else { LcFork::Gloas };
        let name = format!("update_{period}.ssz");
        catch_up.push((period, LightClientUpdate::decode(&read(&replay, &name)?, layout, 512).map_err(|e| anyhow!("{name}: {e:?}"))?));
    }
    let mut stages = Vec::new();
    let mut vault_seen: Option<[u8; 20]> = None;
    for (i, dir) in captures.iter().enumerate() {
        let m: Value = serde_json::from_slice(&read(dir, "fixture.json")?)?;
        let block = m["block"].as_u64().ok_or_else(|| anyhow!("fixture.json: no block"))?;
        for p in m["updates"].as_array().into_iter().flatten() {
            let p = p.as_u64().ok_or_else(|| anyhow!("fixture.json: updates"))?;
            if !catch_up.iter().any(|(q, _)| *q == p) {
                let name = format!("update_{p}.ssz");
                catch_up.push((p, LightClientUpdate::decode(&read(dir, &name)?, LcFork::Gloas, 512).map_err(|e| anyhow!("{name}: {e:?}"))?));
            }
        }
        let address = match vault {
            Some(v) => v.trim_start_matches("0x").to_lowercase(),
            None => m["storage"][0]["address"].as_str().ok_or_else(|| anyhow!("fixture.json: storage address"))?.trim_start_matches("0x").to_lowercase(),
        };
        let vault: [u8; 20] = hex::decode(&address)?.try_into().map_err(|_| anyhow!("vault address"))?;
        ensure!(vault_seen.is_none_or(|v| v == vault), "the captures are of different vaults");
        vault_seen = Some(vault);
        let proof = account_proof(&read(dir, &format!("proof_0x{address}_{block}.json"))?);
        let slot = |n: u8| -> Result<(Vec<Vec<u8>>, [u8; 32])> {
            let mut key = [0u8; 32];
            key[31] = n;
            let s = proof.storage.iter().find(|s| s.key == key).ok_or_else(|| anyhow!("capture {}: no proof of slot {n}", i + 1))?;
            Ok((s.proof.clone(), s.value))
        };
        let (s0, s1, s2) = (slot(0)?, slot(1)?, slot(2)?);
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
        stages.push(Stage {
            label: format!("stage {}", i + 1),
            update: gloas_finality_update(&read(dir, "finality_update.ssz")?)?,
            block,
            header_rlp: read(dir, "header.rlp")?,
            account: proof.proof.clone(),
            slots: [s0.0, s1.0, s2.0],
            slot_values: [s0.1, s1.1, s2.1],
            deposits,
        });
    }
    catch_up.sort_by_key(|(p, _)| *p);
    let config = sepolia_config(ADOPTION_PARTS);
    Ok(InboundChain {
        name: "sepolia".into(),
        config,
        checkpoint: root(SEPOLIA_CHECKPOINT),
        bootstrap,
        bootstrap_fork: LcFork::Electra,
        catch_up,
        vault: vault_seen.expect("one capture"),
        stages,
    })
}

// ---------------------------------------------------------------------------------------------
// The session
// ---------------------------------------------------------------------------------------------

fn tx(step: String, message: &BridgeMessage, payer: AccountId) -> Result<Tx> {
    Ok(Tx { step, harness: None, actors: Vec::new(), root: 0, message: borsh::to_vec(message)?, payer, gas_limit: None, note: String::new() })
}

async fn send(live: &mut Live, step: String, mentions: Vec<AccountMention>, message: &BridgeMessage, payer: AccountId) -> Result<Outcome> {
    live.submit_mentions(tx(step, message, payer)?, mentions).await
}

async fn bridge_state(live: &Live, actor: Actor) -> Result<Option<BridgeState>> {
    let state = live.actor_state(actor).await?;
    Ok(if state.is_empty() { None } else { Some(borsh::from_slice(&state)?) })
}

/// The session: the attestation instance (Init, catch-up), then each stage: its finality update,
/// and the bridge's transactions under the record it writes.
pub async fn session(live: &mut Live, attest: AccountId, bridge_bin: &GuestBinary, chain: &InboundChain, payer: AccountId, private: &str) -> Result<()> {
    let bridge = deployment_accounts(BRIDGE, bridge_bin).0;
    let mut world =
        World::with_checkpoint(&chain.name, attest, chain.config.clone(), chain.checkpoint, &chain.bootstrap, chain.bootstrap_fork)?;
    let inst = world.inst.clone();
    let config = bridge_config(attest, image_bytes(bridge_bin.image_id), inst.id, chain.vault);
    let cid = config.id(bridge);
    let terms: Terms = config.terms;
    println!(
        "[inbound] {} chain: instance {} of the attestation program at {attest}; bridge at {bridge} (image {}); vault 0x{}; configId 0x{}",
        chain.name,
        hex::encode(inst.id),
        bridge_bin.image_hex(),
        hex::encode(chain.vault),
        hex::encode(cid)
    );
    let mut summary = json!({
        "chain": chain.name, "instance_id": hex::encode(inst.id), "attestation_program": hex::encode(attest.value()),
        "bridge_program": hex::encode(bridge.value()), "bridge_image_id": bridge_bin.image_hex(), "vault": hex::encode(chain.vault),
        "config_id": hex::encode(cid), "checkpoint": hex::encode(chain.checkpoint), "stages": [],
    });

    // The instance.
    if scenario::instance_state(live, &inst).await?.is_none() {
        scenario::init(live, &mut world, &chain.bootstrap, &format!("{} Init", chain.name), Harness::None, payer).await?;
    } else {
        bail!("the instance exists already: this session starts on a fresh zone");
    }
    let mut applied = 0;
    let config_pda = Pda { program: bridge, seed: config_seed(&cid) };
    for stage in &chain.stages {
        let period = stage.update.signature_slot / 8192;
        while applied < chain.catch_up.len() && chain.catch_up[applied].0 < period {
            let (p, update) = &chain.catch_up[applied];
            scenario::apply(live, &mut world, update, &format!("period {p}"), Harness::None, payer).await?;
            applied += 1;
        }
        let slot = stage.update.finalized_header.beacon.slot;
        let run = scenario::apply(live, &mut world, &stage.update, &format!("{} finality update", stage.label), Harness::None, payer).await?;
        ensure!(run.outcome.is_some_and(|o| o.advanced), "{}: the finality update does not finalize a new header", stage.label);
        ensure!(stage.slot_values[2] == cid, "{}: the vault's slot 2 is 0x{}, not this configuration's id", stage.label, hex::encode(stage.slot_values[2]));
        let n = stage.deposits.len() as u64;
        let header_pda = inst.header(slot);
        let account = with_header(stage.header_rlp.clone(), stage.account.clone());
        let mut rows = Vec::new();

        if stage.label == "stage 1" {
            let init = |account: Vec<Vec<u8>>| BridgeMessage::Init {
                config: config.clone(),
                record_slot: slot,
                proof: PairingProof { account, config_slot: stage.slots[2].clone() },
            };
            let o = send(live, "bridge Init without the block's header (Gloas record)".into(), vec![mention(config_pda), mention(header_pda)], &init(stage.account.clone()), payer).await?;
            rows.push(o.expect_exit(ferrata_lez_wire::code::HEADER_REQUIRED)?.row);
            let o = send(live, "bridge Init (configId in the vault's slot 2)".into(), vec![mention(config_pda), mention(header_pda)], &init(account.clone()), payer).await?;
            rows.push(o.expect_applied()?.row);
            let state = bridge_state(live, config_pda.actor()).await?;
            ensure!(matches!(state, Some(BridgeState::Config(ConfigState::Ready { .. }))), "config({}) is not ready", hex::encode(cid));
        }
        let root_pda = Pda { program: bridge, seed: root_seed(&cid, n) };
        let import = BridgeMessage::ImportRoot {
            config_id: cid,
            record_slot: slot,
            root: stage.slot_values[0],
            leaf_count: n,
            proof: ImportProof { account: account.clone(), root_slot: stage.slots[0].clone(), count_slot: stage.slots[1].clone() },
        };
        let o = send(live, format!("bridge ImportRoot n={n} ({}, block {})", stage.label, stage.block), vec![mention(config_pda), mention(header_pda), mention(root_pda)], &import, payer).await?;
        rows.push(o.expect_applied()?.row);
        let root_state = bridge_state(live, root_pda.actor()).await?;
        println!("[inbound] root(cid, {n}): {root_state:?}");

        if stage.label == "stage 1" {
            let o = claim(live, "bridge Claim: deposit 0 (public) under root(cid, 1)".into(), (bridge, cid, terms, payer), &stage.deposits, 0, n, None).await?;
            rows.push(o.expect_applied()?.row);
            let o = claim(live, "bridge Claim: deposit 0 again (replay)".into(), (bridge, cid, terms, payer), &stage.deposits, 0, n, None).await?;
            rows.push(o.expect_exit(code::ALREADY_CLAIMED)?.row);
        } else {
            let o = claim(live, format!("bridge Claim: deposit 0 under root(cid, {n}) (a later root)"), (bridge, cid, terms, payer), &stage.deposits, 0, n, None).await?;
            rows.push(o.expect_exit(code::ALREADY_CLAIMED)?.row);
            let public0 = destination(0)?.1;
            let o = claim(live, "bridge Claim: deposit 1 redirected to a public destination (a wrong opening)".into(), (bridge, cid, terms, payer), &stage.deposits, 1, n, Some(public0)).await?;
            rows.push(o.expect_exit(code::CLAIM_INVALID)?.row);
            if private != "skip" {
                private_claim(live, bridge_bin, bridge, cid, n, &stage.deposits, terms, private).await?;
            }
        }
        summary["stages"].as_array_mut().unwrap().push(json!({
            "label": stage.label, "finalized_slot": slot, "block": stage.block, "leaves": n,
            "root": hex::encode(stage.slot_values[0]), "rows": rows.iter().map(|r| format!("{} | {} | {}", r.step, r.status, r.exit)).collect::<Vec<_>>(),
        }));
    }
    let holder = bridge_state(live, Actor::new(AccountId::new(destination(0)?.1), bridge)).await?;
    println!("[inbound] public test account 0's holding: {holder:?}");
    summary["public_holder"] = json!(format!("{holder:?}"));
    fs::create_dir_all(&live.opts.out_dir)?;
    fs::write(live.opts.out_dir.join("inbound-summary.json"), serde_json::to_string_pretty(&summary)?)?;
    live.save()
}

/// The plaintext `Claim` of the deposit at `leaf` under `root(cid, n)`; `redirect` names another
/// (public) destination than the deposit's, which its commitment does not open to.
#[allow(clippy::too_many_arguments)]
async fn claim(
    live: &mut Live,
    step: String,
    (bridge, cid, terms, payer): (AccountId, Root, Terms, AccountId),
    deposits: &[DepositLog],
    leaf: u64,
    n: u64,
    redirect: Option<[u8; 32]>,
) -> Result<Outcome> {
    let (mut w, nullifier, _) = witness(terms.evm_chain_id, &terms.vault, deposits, leaf)?;
    if let Some(dest) = redirect {
        w.dest = dest;
        w.kind = DestKind::Public as u8;
    }
    let root_pda = Pda { program: bridge, seed: root_seed(&cid, n) };
    let n_pda = Pda { program: bridge, seed: nullifier_seed(terms.evm_chain_id, &terms.vault, &nullifier) };
    let holder = AccountIdentity::PublicNoSign(AccountId::new(w.dest)).select_program_actor_state(bridge);
    let message = BridgeMessage::Claim { config_id: cid, leaf_count: n, witness: w };
    send(live, step, vec![mention(root_pda), mention(n_pda), holder], &message, payer).await
}

/// `ClaimPrivately` of deposit 1 at LEZ's private test account 0 (its destination), built and
/// proven by LEZ's wallet, sent to the sequencer, which verifies the receipt.
#[allow(clippy::too_many_arguments)]
async fn private_claim(
    live: &mut Live,
    bridge_bin: &GuestBinary,
    bridge: AccountId,
    cid: Root,
    n: u64,
    deposits: &[DepositLog],
    terms: Terms,
    private: &str,
) -> Result<()> {
    let (w, nullifier, kind) = witness(terms.evm_chain_id, &terms.vault, deposits, 1)?;
    ensure!(kind == DestKind::Private, "deposit 1 is the private one");
    let fake = private == "fake";
    ensure!(
        std::env::var_os("RISC0_DEV_MODE").is_some_and(|v| v == "1") == fake,
        "RISC0_DEV_MODE must be 1 for --private fake and unset for --private real"
    );
    let dir = live.opts.run_dir.join(format!("inbound-private-claim-{private}"));
    fs::create_dir_all(&dir)?;
    let transcript = dir.join("rpc-transcript.jsonl");
    let tx_out = dir.join("transaction.borsh");
    let _ = fs::remove_file(&tx_out);
    live.sync().await?;
    let client = TapClient::recording(live.wallet.client_owned(), &transcript, &tx_out)?;
    let (mut wallet, me) = private_wallet(&dir.join("wallet"), &live.opts.rpc, client)?;
    ensure!(*me.value() == w.dest, "the wallet's private account is not the deposit's destination");
    let tip = wallet.start_at_chain_tip().await?;
    let catalog = ProgramCatalog::from([(bridge, Program::new(Cow::Owned(bridge_bin.bytes.clone()))?)]);
    let root_pda = Pda { program: bridge, seed: root_seed(&cid, n) };
    let n_pda = Pda { program: bridge, seed: nullifier_seed(terms.evm_chain_id, &terms.vault, &nullifier) };
    let mentions = vec![AccountIdentity::PrivateOwned(me).select_program_actor_state(bridge), mention(root_pda), mention(n_pda)];
    let message = borsh::to_vec(&BridgeMessage::ClaimPrivately { config_id: cid, leaf_count: n, witness: w })?;
    let step = format!("bridge ClaimPrivately: deposit 1 at LEZ's private test account 0 ({})", if fake { "fake receipt" } else { "real proof" });
    println!("[inbound] {step}: wallet synced at block {tip}");
    live.rows.push(Row { phase: live.phase.clone(), step: step.clone(), status: "unfinished".into(), note: "the proof had not finished when the run was recorded".into(), ..Row::default() });
    live.save()?;
    let pending = live.rows.len() - 1;
    let started = Instant::now();
    let sent = wallet.send_privacy_preserving_tx_with_pre_check(mentions, 0, message, &catalog, None, |_| Ok(())).await;
    let seconds = started.elapsed().as_secs_f64();
    let since_head = live.head().await?;
    live.rows.remove(pending);
    let hash = match sent {
        Ok((hash, _)) => hash,
        Err(error) => {
            let note = format!("after {seconds:.1} s: {error}: {error:?}");
            println!("[inbound] {step}: rejected: {note}");
            live.rows.push(Row { phase: live.phase.clone(), step, status: "rejected".into(), note, ..Row::default() });
            live.save()?;
            bail!("the private claim was not sent");
        }
    };
    let bytes = fs::metadata(&tx_out).map(|m| m.len()).unwrap_or(0);
    println!("[inbound] proved and sent in {seconds:.1} s ({bytes} bytes)");
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
    if fake {
        ensure!(outcome.row.status == "dropped", "a sequencer that verifies receipts must drop a fake one; it {} it", outcome.row.status);
    } else {
        ensure!(outcome.applied(), "{step}: {} {} {}", outcome.row.status, outcome.row.exit, outcome.row.note);
        let spent = bridge_state(live, n_pda.actor()).await?;
        ensure!(matches!(spent, Some(BridgeState::Nullifier(_))), "the nullifier is not spent after the private claim");
        println!("[inbound] nullifier spent: {spent:?}");
        fs::create_dir_all(&live.opts.out_dir)?;
        fs::copy(&tx_out, live.opts.out_dir.join("inbound-private-claim-tx.borsh"))?;
    }
    live.save()
}

/// Without a sequencer: the chain through the native light client, and the bridge's requests
/// under each record through the attestation program's own evaluation (`attest::evaluate`); every
/// deposit's opening. What the session will send, checked before a zone is started.
pub fn check(chain: &InboundChain, attest: AccountId, bridge_bin: &GuestBinary) -> Result<()> {
    use ferrata_core::witness::HeaderRecord;
    use ferrata_lez_bridge_wire::{import_request, pairing_request};
    use ferrata_lez_wire::attest::evaluate;
    let inst_id = instance_id(&chain.config, &chain.checkpoint);
    let config = bridge_config(attest, image_bytes(bridge_bin.image_id), inst_id, chain.vault);
    let mut native = crate::plan::Native::with_fork(&chain.config, &chain.checkpoint, &chain.bootstrap, chain.bootstrap_fork)?;
    let mut applied = 0;
    for stage in &chain.stages {
        let period = stage.update.signature_slot / 8192;
        while applied < chain.catch_up.len() && chain.catch_up[applied].0 < period {
            let (p, update) = &chain.catch_up[applied];
            let spec = config_spec(&chain.config);
            let layout = spec.fork_at_epoch(spec.epoch_at_slot(update.attested_header.beacon.slot)).lc_fork().unwrap_or(native.core().fork);
            if layout > native.core().fork {
                native.store.upgrade(layout, &native.spec).map_err(|e| anyhow!("period {p}: upgrade: {e:?}"))?;
            }
            native.process(update).map_err(|e| anyhow!("period {p}: {e:?}"))?;
            applied += 1;
        }
        let spec = config_spec(&chain.config);
        let layout = spec.fork_at_epoch(spec.epoch_at_slot(stage.update.attested_header.beacon.slot)).lc_fork().unwrap_or(native.core().fork);
        if layout > native.core().fork {
            native.store.upgrade(layout, &native.spec).map_err(|e| anyhow!("{}: upgrade: {e:?}", stage.label))?;
        }
        let outcome = native.process(&stage.update).map_err(|e| anyhow!("{}: the native store refuses the finality update: {e:?}", stage.label))?;
        ensure!(outcome.advanced, "{}: no new finalized header", stage.label);
        let core = native.core();
        let record = HeaderRecord::of(&core.finalized_header, core.fork, &native.spec);
        let account = with_header(stage.header_rlp.clone(), stage.account.clone());
        let pairing = pairing_request(&config, &PairingProof { account: account.clone(), config_slot: stage.slots[2].clone() });
        if let Ok(h) = ferrata_core::exec::decode_header(&stage.header_rlp) {
            println!(
                "[inbound-check] {}: header state root {} number {}, keccak(account node 0) {}, keccak(header) {}, record {:?}",
                stage.label,
                hex::encode(h.state_root),
                h.number,
                hex::encode(keccak(&stage.account[0])),
                hex::encode(keccak(&stage.header_rlp)),
                record.execution
            );
        }
        evaluate(&inst_id, chain.config.chain_id, &record, &pairing).map_err(|c| anyhow!("{}: the pairing request is refused natively ({c})", stage.label))?;
        let import = import_request(&config, &ImportProof { account, root_slot: stage.slots[0].clone(), count_slot: stage.slots[1].clone() });
        evaluate(&inst_id, chain.config.chain_id, &record, &import).map_err(|c| anyhow!("{}: the import request is refused natively ({c})", stage.label))?;
        let tree = Tree::from_leaves(stage.deposits.iter().map(DepositLog::leaf).collect()).map_err(|_| anyhow!("tree"))?;
        ensure!(tree.root() == stage.slot_values[0], "{}: slot 0 is not the root of the deposits", stage.label);
        for leaf in 0..stage.deposits.len() as u64 {
            witness(chain.config.chain_id, &chain.vault, &stage.deposits, leaf)?;
        }
        println!(
            "[inbound-check] {} ({}): finalized slot {} binds block {}; pairing and import requests accepted natively; {} deposit(s) open to the plan",
            stage.label, chain.name, core.finalized_header.beacon.slot, stage.block, stage.deposits.len()
        );
    }
    println!(
        "[inbound-check] {}: instance 0x{}, configId 0x{}",
        chain.name,
        hex::encode(inst_id),
        hex::encode(config.id(deployment_accounts(BRIDGE, bridge_bin).0))
    );
    Ok(())
}

fn config_spec(config: &Config) -> ferrata_core::fork::ChainSpec {
    config.spec()
}
