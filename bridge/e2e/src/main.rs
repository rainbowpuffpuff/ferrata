//! Wallet side and receipt handling of the end-to-end return-leg fixture (`e2e/` at the repository
//! root). The LEZ side is `checkpoint/fidelity/src/bin/e2e.rs`; the two meet only through files,
//! as a wallet, a zone, a prover and a vault would, and every value both sides can compute is
//! compared here: `configId`, the burn root, the checkpoint states and the 17-word journal.
//!
//! ```text
//! ferrata-e2e image-id <program.bin>…
//!     image id, size and sha256 of guest program binaries
//! ferrata-e2e program-binary <user elf> <out.bin>
//!     the program binary risc0-build 3.0.5 writes beside a guest's user ELF (the ELF with the v1compat
//!     kernel), for a build whose `.bin` files were not kept; prints its image id
//! ferrata-e2e notes --deployment <deployment.json> --params <params.json> --out <wallet.json>
//!     the wallet's deposits (commitments, leaves, the deposit root the zone imports) and burn
//!     notes, between other users' notes, in the order they are published; a deposit of the
//!     params file may name its LEZ destination ("kind": "private" | "public", "dest": 0x + 32 bytes)
//! ferrata-e2e release --deployment <d> --wallet <w> --lez <lez.json> --out-dir <dir>
//!     reads the zone as an observer, recovers the wallet's burns from its seed, rebuilds the burn
//!     tree, recomputes the checkpoint journals, and writes release-<name>.{input,json}
//! ferrata-e2e exec  --program <bin> --input <file> --meta <file.json>
//!     executor only: the guest's journal must equal the native one; prints cycles and segments
//! ferrata-e2e prove --program <bin> --input <file> --meta <file.json> --out-dir <dir>
//!     succinct receipt on this machine's CPU (meant for the release guest)
//! ferrata-e2e verify --receipt <bin> --image-id <0x…> --meta <file.json>
//! ferrata-e2e p254 --receipt <succinct.bin> --image-id <0x…> --out-dir <dir>
//!     first step of the Groth16 wrap: identity_p254 in r0vm, then <dir>/input.json for RISC Zero's
//!     Groth16 prover image (`docker run --rm -v <dir>:/mnt risczero/risc0-groth16-prover:v2025-04-03.1`
//!     writes <dir>/proof.json)
//! ferrata-e2e groth16 --receipt <succinct.bin> --image-id <0x…> --proof <proof.json> --out-dir <dir>
//!     second step: the Groth16 receipt from proof.json, verified; writes receipt-groth16.bin,
//!     seal.hex (selector ‖ proof, as the on-chain verifier takes it) and journal.hex
//! ferrata-e2e wrap --receipt <succinct.bin> --image-id <0x…> --out-dir <dir>
//!     both steps in one, through r0vm (which runs Docker itself)
//! ferrata-e2e fixture --run-dir <dir> --checkpoint-program <bin> --release-program <bin> --out <fixture.json>
//!     everything the vault's tests and scripts read, from the files of a run
//! ferrata-e2e crown-release --identity <identity.json> --burns <burns.json> --notes <notes.json> --out-dir <dir>
//!     the crown run (e2e/crown/README.md, stage 9): the burn tree from the burns the checkpoint counted
//!     (checkpoint/realblocks `realblocks-crown`'s burns.json; every leaf recomputed by the bridge core),
//!     one release statement per own burn of notes.json (r, s_out, recipient), release-<name>.{input,json}
//! ferrata-e2e crown-synthetic --identity <identity.json> --out-dir <dir>
//!     STAND-IN burns.json + notes.json (two burns of a test wallet between two of another user) for a
//!     rehearsal of crown-release and the release proofs before the LEZ session has burned
//! ```
//!
//! Guest programs are read from files (the `.bin` risc0-build writes), never built here, and their
//! image ids are computed from those bytes: the fixture names the image ids of exactly the
//! programs that were proven. Meta files are `{"kind": "checkpoint" | "release", "journal": hex}`;
//! a checkpoint input is framed as its guest reads it (a little-endian u32 length, then the
//! bytes), a release input is written as is.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, ensure, Context as _, Result};
use ferrata_bridge_core::checkpoint::{burn_frontier_hash, threshold, LezState};
use ferrata_bridge_core::encoding::{config_id_preimage, u256_from_u128, ConfigIdentity, ReleaseInput};
use ferrata_bridge_core::inbound::{commitment, deposit_leaf, deposit_secret, DestKind};
use ferrata_bridge_core::outbound::{
    burn_commitment, burn_leaf, burn_secret, recover_burns, release_journal, verify_release, BurnNote, PublishedBurn,
    ReleasePublic, ReleaseTarget, ReleaseWitness,
};
use ferrata_bridge_core::tree::{IncrementalTree, Tree};
use ferrata_bridge_core::{chain_id_u256, sha256, Address, BridgeKeys, Hash, U256};
use risc0_zkvm::sha::Digestible as _;
use risc0_zkvm::{
    compute_image_id, default_executor, default_prover, ApiClient, Asset, AssetRequest, Bytes, Digest, ExecutorEnv,
    Groth16Receipt, Groth16ReceiptVerifierParameters, InnerReceipt, ProverOpts, Receipt,
};
use serde_json::{json, Value};

mod checkpoint_v3;
use checkpoint_v3::{key_set_hash_v2, CheckpointJournalV3};

/// The wallet's bridge seed is `sha256` of this label: a test value, written into `wallet.json`.
const SEED_LABEL: &str = "e2e: user bridge seed (test value)";
/// How many unused burn counters end the wallet's scan of the published notes.
const GAP_LIMIT: u64 = 20;

// ---- small helpers ------------------------------------------------------------------------------------

fn hx(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn unhex_vec(text: &str) -> Result<Vec<u8>> {
    hex::decode(text.trim().trim_start_matches("0x")).with_context(|| format!("hex: {text}"))
}

fn unhex<const N: usize>(text: &str) -> Result<[u8; N]> {
    let bytes = unhex_vec(text)?;
    bytes.try_into().map_err(|b: Vec<u8>| anyhow::anyhow!("expected {N} bytes, got {}: {text}", b.len()))
}

fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key].as_str().with_context(|| format!("missing string \"{key}\""))
}

fn field<const N: usize>(v: &Value, key: &str) -> Result<[u8; N]> {
    unhex(text(v, key)?)
}

fn number(v: &Value, key: &str) -> Result<u64> {
    v[key].as_u64().with_context(|| format!("missing number \"{key}\""))
}

fn list<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    v[key].as_array().with_context(|| format!("missing array \"{key}\""))
}

/// A decimal amount of base units as a `uint256`.
fn amount(v: &Value, key: &str) -> Result<U256> {
    let digits = text(v, key)?;
    Ok(u256_from_u128(digits.parse().with_context(|| format!("\"{key}\": {digits} is not a decimal amount"))?))
}

fn label(text: &str) -> Hash {
    sha256(text.as_bytes())
}

fn label_addr(text: &str) -> Address {
    label(text)[..20].try_into().expect("20 bytes")
}

fn read_json(path: &str) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| path.to_string())?).with_context(|| path.to_string())
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(value)? + "\n").with_context(|| path.display().to_string())
}

fn loadavg() -> String {
    fs::read_to_string("/proc/loadavg").unwrap_or_default().trim().to_string()
}

/// `--name value` pairs after the subcommand.
fn options(args: &[String]) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let mut it = args.iter();
    while let Some(name) = it.next() {
        let name = name.strip_prefix("--").with_context(|| format!("unexpected argument {name}"))?;
        out.insert(name.to_string(), it.next().with_context(|| format!("--{name} needs a value"))?.clone());
    }
    Ok(out)
}

fn required<'a>(opts: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str> {
    opts.get(name).map(String::as_str).with_context(|| format!("--{name} is required"))
}

fn refuse_dev_mode() -> Result<()> {
    let on = std::env::var("RISC0_DEV_MODE").map(|v| !v.is_empty() && v != "0" && v != "false").unwrap_or(false);
    ensure!(!on, "RISC0_DEV_MODE is set: receipts would be fake");
    Ok(())
}

// ---- the deployment, as both sides read it ----------------------------------------------------------

/// The preimage of `configId` from `deployment.json`, and `configId` as this crate computes it,
/// which must be the value the checkpoint side wrote.
fn identity(d: &Value) -> Result<(ConfigIdentity, Hash)> {
    let i = &d["config_identity"];
    let identity = ConfigIdentity {
        bedrock_genesis_id: field(i, "bedrock_genesis_id")?,
        channel_id: field(i, "channel_id")?,
        bridge_header: field(i, "bridge_header")?,
        bridge_image_id: field(i, "bridge_image_id")?,
        evm_chain_id: field(i, "evm_chain_id")?,
        vault: field(i, "vault")?,
        attestation_root_id: field(i, "attestation_root_id")?,
        salt: field(i, "salt")?,
    };
    let config_id = sha256(&config_id_preimage(&identity));
    ensure!(
        config_id == field::<32>(d, "config_id")?,
        "configId: the bridge core computes {} from the deployment's preimage, the checkpoint core wrote {}",
        hx(&config_id),
        text(d, "config_id")?
    );
    ensure!(identity.evm_chain_id == chain_id_u256(number(d, "evm_chain_id")?), "chain id word");
    ensure!(identity.vault == field::<20>(d, "vault")?, "vault");
    Ok((identity, config_id))
}

// ---- notes: the wallet's deposits and burns --------------------------------------------------------

fn note_json(note: &BurnNote) -> Value {
    json!({
        "r": hx(&note.r), "memo": hx(&note.memo), "mac": hx(&note.mac), "token": hx(&note.token),
        "amount": hx(&note.amount), "c_out": hx(&note.c_out),
    })
}

fn note_from(v: &Value) -> Result<BurnNote> {
    Ok(BurnNote {
        r: field(v, "r")?,
        memo: field(v, "memo")?,
        mac: field(v, "mac")?,
        token: field(v, "token")?,
        amount: field(v, "amount")?,
        c_out: field(v, "c_out")?,
    })
}

fn cmd_notes(opts: &BTreeMap<String, String>) -> Result<()> {
    let d = read_json(required(opts, "deployment")?)?;
    let p = read_json(required(opts, "params")?)?;
    let (id, config_id) = identity(&d)?;
    let (chain_id, vault) = (id.evm_chain_id, id.vault);
    let wallet = BridgeKeys::from_bridge_seed(&label(SEED_LABEL));

    // Deposits: the wallet is the depositor's, the vault is fresh, so deposit j is leaf j. A deposit
    // goes to the LEZ account its "kind" and "dest" name; without them, to a made-up account of
    // kind private for even j, public for odd j.
    let depositor: Address = field(&p, "depositor")?;
    let mut tree = IncrementalTree::new();
    let mut deposits = Vec::new();
    for (j, spec) in list(&p, "deposits")?.iter().enumerate() {
        let j = j as u64;
        let (token, value): (Address, U256) = (field(spec, "token")?, amount(spec, "amount")?);
        let kind = match spec.get("kind").map(Value::as_str) {
            None if j.is_multiple_of(2) => DestKind::Private,
            None => DestKind::Public,
            Some(Some("private")) => DestKind::Private,
            Some(Some("public")) => DestKind::Public,
            Some(_) => bail!("deposit {j}: \"kind\" is \"private\" or \"public\", not {}", spec["kind"]),
        };
        let dest = match spec.get("dest") {
            None => label(&format!("e2e: LEZ destination account {j}")),
            Some(_) => field(spec, "dest")?,
        };
        let s = deposit_secret(&wallet.auth, &chain_id, &vault, &depositor, j);
        let c = commitment(&s, kind, &dest);
        let leaf = deposit_leaf(j, &depositor, &token, &value, &c);
        let leaf_index = tree.append(leaf).map_err(|_| anyhow::anyhow!("deposit tree full"))?;
        ensure!(leaf_index == j, "leaf index");
        deposits.push(json!({
            "j": j, "leaf_index": leaf_index, "token": hx(&token), "amount": hx(&value),
            "kind": if kind == DestKind::Private { "private" } else { "public" }, "dest": hx(&dest),
            "secret_s": hx(&s), "commitment": hx(&c), "leaf": hx(&leaf),
        }));
    }
    ensure!(!deposits.is_empty(), "at least one deposit");

    // Burns: before each of the wallet's notes one of another user, and one more after the last.
    let other = |k: usize, token: Address, value: U256| {
        let keys = BridgeKeys::from_bridge_seed(&label(&format!("e2e: other user {k} (test value)")));
        let recipient = label_addr(&format!("e2e: other recipient {k}"));
        BurnNote::create(&keys, &config_id, 0, &ReleaseTarget { chain_id, vault, recipient }, token, value)
    };
    let mut notes: Vec<BurnNote> = Vec::new();
    let mut own = Vec::new();
    let mut last = None;
    for (i, spec) in list(&p, "burns")?.iter().enumerate() {
        let (token, value): (Address, U256) = (field(spec, "token")?, amount(spec, "amount")?);
        let recipient: Address = field(spec, "recipient")?;
        notes.push(other(notes.len(), token, value));
        own.push(json!({
            "name": text(spec, "name")?, "i": i, "position": notes.len(), "token": hx(&token), "amount": hx(&value),
            "recipient": hx(&recipient), "relayer": hx(&field::<20>(spec, "relayer")?),
            "relay_fee": hx(&amount(spec, "relay_fee")?),
        }));
        notes.push(BurnNote::create(
            &wallet,
            &config_id,
            i as u64,
            &ReleaseTarget { chain_id, vault, recipient },
            token,
            value,
        ));
        last = Some((token, value));
    }
    let (token, value) = last.context("at least one burn")?;
    notes.push(other(notes.len(), token, value));

    let value = json!({
        "what": "The wallet side of the end-to-end return-leg fixture. Test values throughout: the bridge seed is \
                 sha256 of bridge_seed_label, and secret_s of each deposit is printed. burns are the notes \
                 published on the zone, in order; own says which are this wallet's.",
        "bridge_seed_label": SEED_LABEL,
        "config_id": hx(&config_id),
        "evm_chain_id": number(&d, "evm_chain_id")?,
        "vault": hx(&vault),
        "depositor": hx(&depositor),
        "deposits": deposits,
        "import_root": {"root": hx(&tree.root()), "leaf_count": tree.len()},
        "burns": notes.iter().map(note_json).collect::<Vec<_>>(),
        "own": own,
    });
    let out = PathBuf::from(required(opts, "out")?);
    write_json(&out, &value)?;
    println!(
        "wrote {}: {} deposits (root {}), {} notes of which {} are the wallet's",
        out.display(),
        tree.len(),
        hx(&tree.root()),
        notes.len(),
        own.len()
    );
    Ok(())
}

// ---- release: the wallet reads the zone and builds its release statements ------------------------------

/// What an observer extracts from the zone's blocks up to `T`, by SPEC-v2 §4.2 item 2: the burns
/// of non-reverted transactions for this configuration, in block and transaction order, the
/// largest imported leaf count, and whether the configuration was sunset.
struct Observed {
    burns: Vec<PublishedBurn>,
    imported_leaf_count: u64,
    sunset: bool,
    tip_hash: Hash,
}

fn observe(lez: &Value, config_id: &Hash, t_block: u64) -> Result<Observed> {
    let mut out = Observed { burns: Vec::new(), imported_leaf_count: 0, sunset: false, tip_hash: [0u8; 32] };
    let mut seen_tip = false;
    for block in list(lez, "blocks")? {
        let id = number(block, "id")?;
        if id > t_block {
            continue;
        }
        if id == t_block {
            out.tip_hash = field(block, "hash")?;
            seen_tip = true;
        }
        for message in list(block, "bridge_messages")? {
            let ours = message["config_id"].as_str().map(unhex::<32>).transpose()? == Some(*config_id);
            if message["reverted"].as_bool().context("reverted")? || !ours {
                continue;
            }
            match text(message, "kind")? {
                "Burn" => {
                    let burn_index = out.burns.len() as u64;
                    out.burns.push(PublishedBurn { burn_index, note: note_from(message)? });
                }
                "ImportRoot" if !out.sunset => {
                    out.imported_leaf_count = out.imported_leaf_count.max(number(message, "leaf_count")?);
                }
                "Sunset" => out.sunset = true,
                _ => {}
            }
        }
    }
    ensure!(seen_tip, "block {t_block} (T) is not among the observed blocks");
    Ok(out)
}

fn cmd_release(opts: &BTreeMap<String, String>) -> Result<()> {
    let d = read_json(required(opts, "deployment")?)?;
    let w = read_json(required(opts, "wallet")?)?;
    let lez = read_json(required(opts, "lez")?)?;
    let dir = PathBuf::from(required(opts, "out-dir")?);
    fs::create_dir_all(&dir)?;
    let (id, config_id) = identity(&d)?;
    let (chain_id, vault) = (id.evm_chain_id, id.vault);
    ensure!(config_id == field::<32>(&lez, "config_id")?, "lez.json is for another configuration");
    let wallet = BridgeKeys::from_bridge_seed(&label(text(&w, "bridge_seed_label")?));

    // 1. The zone as observed, and the wallet's own burns found from the seed alone.
    let candidate = &lez["checkpoints"]["candidate"];
    let t_block = number(candidate, "t_block")?;
    let seen = observe(&lez, &config_id, t_block)?;
    let scan = recover_burns(&wallet, &config_id, &chain_id, &vault, &seen.burns, GAP_LIMIT);
    let own = list(&w, "own")?;
    ensure!(scan.owned.len() == own.len(), "the wallet found {} of its {} burns", scan.owned.len(), own.len());

    // 2. The burn tree, by this crate's two tree implementations.
    let leaves: Vec<Hash> = seen
        .burns
        .iter()
        .map(|b| burn_leaf(&config_id, b.burn_index, &b.note.token, &b.note.amount, &b.note.c_out, &b.note.r))
        .collect();
    let mut frontier = IncrementalTree::new();
    for leaf in &leaves {
        frontier.append(*leaf).map_err(|_| anyhow::anyhow!("burn tree full"))?;
    }
    let tree = Tree::from_leaves(leaves).map_err(|_| anyhow::anyhow!("burn tree full"))?;
    let burn_root = tree.root();
    ensure!(burn_root == frontier.root(), "full and incremental burn tree disagree");

    // 3. The checkpoint states and journals, recomputed from the deployment and the observed
    //    blocks with this crate's SPEC-v3 encodings, against the bytes the checkpoint program's
    //    rule produced. No key rotation happens in the fixture, so the key set is the genesis one.
    let genesis = &d["genesis"];
    let keys: Vec<Hash> =
        list(&genesis["state"], "keys")?.iter().map(|k| unhex(k.as_str().context("key")?)).collect::<Result<_>>()?;
    let n = u32::try_from(keys.len())?;
    let key_set = key_set_hash_v2(&keys, u32::try_from(number(&d, "posting_timeframe")?)?);
    let config_tip_hash: Hash = field(&genesis["state"], "config_tip_hash")?;
    let parent = LezState {
        tip_id: number(genesis, "block_id")?,
        tip_hash: field(genesis, "block_hash")?,
        key_set_hash: key_set,
        config_tip_hash,
        burn_count: 0,
        burn_frontier_hash: burn_frontier_hash(&IncrementalTree::new()),
        imported_leaf_count: 0,
        sunset: false,
    };
    ensure!(parent.hash() == field::<32>(&genesis["state"], "S")?, "genesis state S: bridge core and checkpoint core");
    let at_t = LezState {
        tip_id: t_block,
        tip_hash: seen.tip_hash,
        key_set_hash: key_set,
        config_tip_hash,
        burn_count: frontier.len(),
        burn_frontier_hash: burn_frontier_hash(&frontier),
        imported_leaf_count: seen.imported_leaf_count,
        sunset: seen.sunset,
    };
    let (n_min, t_min) = (u32::try_from(number(&d, "n_min")?)?, u32::try_from(number(&d, "t_min")?)?);
    let expected = |echo_nonce: Hash| CheckpointJournalV3 {
        chain_id,
        vault,
        lez_config_id: config_id,
        n_min,
        t_min,
        parent_state: parent.hash(),
        new_state: at_t.hash(),
        burn_root,
        burn_count: frontier.len(),
        imported_leaf_count: seen.imported_leaf_count,
        sunset: seen.sunset,
        echo_nonce,
        tip_id: t_block,
        tip_hash: seen.tip_hash,
        n,
        t: threshold(n),
    };
    let mut compared = Vec::new();
    for (name, echo_nonce) in [("candidate", Some([0u8; 32])), ("finalize", None)] {
        let cp = &lez["checkpoints"][name];
        if cp.is_null() {
            continue;
        }
        let theirs = unhex_vec(text(cp, "journal")?)?;
        let decoded = CheckpointJournalV3::decode(&theirs)
            .with_context(|| format!("the {name} journal is not a canonical SPEC-v3 D3 journal"))?;
        let echo_nonce = echo_nonce.map_or_else(|| field::<32>(cp, "echo_nonce"), Ok)?;
        let ours = expected(echo_nonce);
        ensure!(
            ours.encode()[..] == theirs[..],
            "{name} journal: the checkpoint program committed {decoded:?}, the bridge core recomputes {ours:?}"
        );
        compared.push(name);
    }
    ensure!(compared.contains(&"candidate"), "lez.json has no candidate checkpoint");

    // 4. One release statement per own burn, against the root the checkpoint carries.
    let mut cases = Vec::new();
    for (owned, spec) in scan.owned.iter().zip(own) {
        let name = text(spec, "name")?;
        ensure!(
            owned.i == number(spec, "i")? && owned.burn.burn_index == number(spec, "position")?,
            "{name}: position"
        );
        ensure!(owned.recipient == field::<20>(spec, "recipient")?, "{name}: recipient recovered from the memo");
        let public = ReleasePublic {
            chain_id,
            vault,
            config_id,
            token: owned.burn.note.token,
            amount: owned.burn.note.amount,
            recipient: owned.recipient,
            relayer: field(spec, "relayer")?,
            relay_fee: field(spec, "relay_fee")?,
        };
        let witness = ReleaseWitness {
            burn_index: owned.burn.burn_index,
            path: tree.path(owned.burn.burn_index).context("path")?,
            s_out: owned.s_out,
            r: owned.burn.note.r,
        };
        let nullifier =
            verify_release(&burn_root, &witness, &public).map_err(|e| anyhow::anyhow!("{name}: release {e:?}"))?;
        ensure!(nullifier == owned.nullifier, "{name}: nullifier");
        let journal = release_journal(&burn_root, &nullifier, &public);
        let input = ReleaseInput { burn_root, witness, public: public.clone() };
        fs::write(dir.join(format!("release-{name}.input")), input.encode())?;
        let meta = json!({
            "kind": "release",
            "description": format!("end-to-end return leg, release of the wallet's burn {} (burn index {})", owned.i, owned.burn.burn_index),
            "name": name, "burn_index": owned.burn.burn_index, "burn_root": hx(&burn_root), "nullifier": hx(&nullifier),
            "token": hx(&public.token), "amount": hx(&public.amount), "recipient": hx(&public.recipient),
            "relayer": hx(&public.relayer), "relay_fee": hx(&public.relay_fee),
            "journal": hex::encode(journal),
        });
        write_json(&dir.join(format!("release-{name}.json")), &meta)?;
        cases.push(name.to_string());
    }
    println!(
        "configId, genesis state, burn root {} over {} burns, new state {} and the {} journal(s) agree between the \
         bridge core and the checkpoint core; wrote release inputs for {:?}",
        hx(&burn_root),
        frontier.len(),
        hx(&at_t.hash()),
        compared.join(" and "),
        cases
    );
    Ok(())
}

// ---- guest programs given as files: execute, prove, verify --------------------------------------------

struct Job {
    program: Vec<u8>,
    image_id: Digest,
    input: Vec<u8>,
    /// The checkpoint guests read a length first; the release guest reads its fixed-size input.
    framed: bool,
    journal: Vec<u8>,
}

fn expected_journal(meta_path: &str) -> Result<(String, Vec<u8>)> {
    let meta = read_json(meta_path)?;
    Ok((text(&meta, "kind")?.to_string(), unhex_vec(text(&meta, "journal")?)?))
}

fn job(opts: &BTreeMap<String, String>) -> Result<Job> {
    let program_path = required(opts, "program")?;
    let program = fs::read(program_path).with_context(|| program_path.to_string())?;
    let input_path = required(opts, "input")?;
    let (kind, journal) = expected_journal(required(opts, "meta")?)?;
    Ok(Job {
        image_id: compute_image_id(&program)?,
        program,
        input: fs::read(input_path).with_context(|| input_path.to_string())?,
        framed: kind != "release",
        journal,
    })
}

impl Job {
    fn env(&self) -> Result<ExecutorEnv<'static>> {
        let mut builder = ExecutorEnv::builder();
        if self.framed {
            builder.write_slice(&u32::try_from(self.input.len())?.to_le_bytes());
        }
        builder.write_slice(self.input.as_slice());
        builder.build()
    }
}

fn cmd_program_binary(paths: &[String]) -> Result<()> {
    ensure!(paths.len() == 2, "usage: ferrata-e2e program-binary <user elf> <out.bin>");
    let user = fs::read(&paths[0]).with_context(|| paths[0].clone())?;
    ensure!(user.starts_with(b"\x7fELF"), "{}: not an ELF", paths[0]);
    let binary = risc0_binfmt::ProgramBinary::new(&user, risc0_zkos_v1compat::V1COMPAT_ELF).encode();
    fs::write(&paths[1], &binary).with_context(|| paths[1].clone())?;
    cmd_image_id(&paths[1..])
}

fn cmd_image_id(paths: &[String]) -> Result<()> {
    ensure!(!paths.is_empty(), "usage: ferrata-e2e image-id <program.bin>…");
    for path in paths {
        let program = fs::read(path).with_context(|| path.clone())?;
        println!(
            "{}",
            json!({"program": path, "image_id": hx(compute_image_id(&program)?.as_bytes()), "bytes": program.len(),
                   "sha256": hex::encode(sha256(&program))})
        );
    }
    Ok(())
}

fn cmd_exec(opts: &BTreeMap<String, String>) -> Result<()> {
    let job = job(opts)?;
    let start = Instant::now();
    let info = default_executor().execute(job.env()?, &job.program)?;
    let secs = start.elapsed().as_secs_f64();
    ensure!(info.journal.bytes == job.journal, "the guest's journal differs from the native one");
    let po2s: Vec<u32> = info.segments.iter().map(|s| s.po2).collect();
    println!(
        "{}",
        json!({"image_id": hx(job.image_id.as_bytes()), "input_bytes": job.input.len(), "user_cycles": info.cycles(),
               "segments": po2s.len(), "po2": po2s, "padded_cycles": po2s.iter().map(|p| 1u64 << p).sum::<u64>(),
               "journal_bytes": info.journal.bytes.len(), "journal_equals_native": true, "executor_wall_s": secs})
    );
    Ok(())
}

fn cmd_prove(opts: &BTreeMap<String, String>) -> Result<()> {
    refuse_dev_mode()?;
    let job = job(opts)?;
    let dir = PathBuf::from(required(opts, "out-dir")?);
    fs::create_dir_all(&dir)?;
    let load_before = loadavg();
    let start = Instant::now();
    let info = default_prover().prove_with_opts(job.env()?, &job.program, &ProverOpts::succinct())?;
    let secs = start.elapsed().as_secs_f64();
    let receipt = info.receipt;
    ensure!(matches!(receipt.inner, InnerReceipt::Succinct(_)), "expected a succinct receipt");
    ensure!(receipt.journal.bytes == job.journal, "the receipt's journal differs from the native one");
    receipt.verify(job.image_id).context("the succinct receipt does not verify against the program's image id")?;
    let bytes = bincode::serialize(&receipt)?;
    fs::write(dir.join("receipt-succinct.bin"), &bytes)?;
    let s = &info.stats;
    let record = json!({
        "image_id": hx(job.image_id.as_bytes()), "program_sha256": hex::encode(sha256(&job.program)),
        "input_sha256": hex::encode(sha256(&job.input)), "risc0_zkvm": risc0_zkvm::VERSION, "prover": "r0vm, CPU",
        "threads": std::thread::available_parallelism().map(usize::from).unwrap_or(0),
        "rayon_num_threads": std::env::var("RAYON_NUM_THREADS").ok(),
        "wall_s": secs, "loadavg_before": load_before, "loadavg_after": loadavg(),
        "segments": s.segments, "user_cycles": s.user_cycles, "total_cycles": s.total_cycles,
        "paging_cycles": s.paging_cycles, "reserved_cycles": s.reserved_cycles,
        "receipt_kind": "succinct", "receipt_bytes": bytes.len(), "receipt_sha256": hex::encode(sha256(&bytes)),
        "verifies": true, "journal_equals_native": true,
    });
    write_json(&dir.join("prove.json"), &record)?;
    println!("{record}");
    Ok(())
}

fn load_receipt(path: &str) -> Result<Receipt> {
    bincode::deserialize(&fs::read(path).with_context(|| path.to_string())?).with_context(|| path.to_string())
}

fn image_id(opts: &BTreeMap<String, String>) -> Result<Digest> {
    Ok(Digest::from_bytes(unhex::<32>(required(opts, "image-id")?)?))
}

fn receipt_kind(receipt: &Receipt) -> &'static str {
    match &receipt.inner {
        InnerReceipt::Succinct(_) => "succinct",
        InnerReceipt::Groth16(_) => "groth16",
        InnerReceipt::Composite(_) => "composite",
        InnerReceipt::Fake(_) => "fake",
        _ => "other",
    }
}

fn cmd_verify(opts: &BTreeMap<String, String>) -> Result<()> {
    refuse_dev_mode()?;
    let path = required(opts, "receipt")?;
    let receipt = load_receipt(path)?;
    let kind = receipt_kind(&receipt);
    ensure!(kind == "succinct" || kind == "groth16", "{path}: a {kind} receipt is not a real proof here");
    let id = image_id(opts)?;
    receipt.verify(id).with_context(|| format!("{path} does not verify against {}", hx(id.as_bytes())))?;
    let (_, journal) = expected_journal(required(opts, "meta")?)?;
    ensure!(receipt.journal.bytes == journal, "{path}: the journal differs from the native one");
    println!(
        "{}",
        json!({"receipt": path, "kind": kind, "image_id": hx(id.as_bytes()), "verifies": true,
               "journal_equals_native": true, "journal_sha256": hex::encode(sha256(&receipt.journal.bytes))})
    );
    Ok(())
}

// ---- the Groth16 wrap, in RISC Zero's own two steps ---------------------------------------------------

/// The seal as `RiscZeroGroth16Verifier.verify` takes it: the first four bytes of the
/// verifier-parameters digest, then the 256-byte proof (risc0-ethereum's `encode_seal`).
fn eth_seal(receipt: &Receipt) -> Result<Vec<u8>> {
    let g = receipt.inner.groth16().context("not a Groth16 receipt")?;
    Ok([&g.verifier_parameters.as_bytes()[..4], g.seal.as_slice()].concat())
}

fn write_groth16(dir: &Path, receipt: &Receipt, id: Digest, record: Value) -> Result<()> {
    receipt.verify(id).context("the Groth16 receipt does not verify")?;
    let seal = eth_seal(receipt)?;
    ensure!(seal.len() == 260, "seal of {} bytes", seal.len());
    let bytes = bincode::serialize(receipt)?;
    fs::create_dir_all(dir)?;
    fs::write(dir.join("receipt-groth16.bin"), &bytes)?;
    fs::write(dir.join("seal.hex"), hx(&seal) + "\n")?;
    fs::write(dir.join("journal.hex"), hx(&receipt.journal.bytes) + "\n")?;
    let mut record = record;
    record["image_id"] = json!(hx(id.as_bytes()));
    record["receipt_bytes"] = json!(bytes.len());
    record["seal_bytes"] = json!(seal.len());
    record["selector"] = json!(hx(&seal[..4]));
    record["journal_sha256"] = json!(hex::encode(sha256(&receipt.journal.bytes)));
    record["verifies"] = json!(true);
    write_json(&dir.join("groth16.json"), &record)?;
    println!("{record}");
    Ok(())
}

/// Step 1 (`succinct_to_groth16` in risc0-zkvm 3.0.5 calls it first): the recursion program that
/// re-proves the succinct receipt with Poseidon over BN254, in r0vm, and the JSON input of the
/// Groth16 prover.
fn cmd_p254(opts: &BTreeMap<String, String>) -> Result<()> {
    refuse_dev_mode()?;
    ensure!(cfg!(feature = "wrap"), "p254 needs the feature `wrap` (default): rebuild without --no-default-features");
    let receipt = load_receipt(required(opts, "receipt")?)?;
    let id = image_id(opts)?;
    receipt.verify(id).context("the succinct receipt does not verify")?;
    let succinct = receipt.inner.succinct().context("not a succinct receipt")?;
    let dir = PathBuf::from(required(opts, "out-dir")?);
    fs::create_dir_all(&dir)?;
    let asset = Asset::Inline(Bytes::from(bincode::serialize(succinct)?));
    let load_before = loadavg();
    let start = Instant::now();
    let p254 = ApiClient::from_env()?.identity_p254(&ProverOpts::succinct(), asset, AssetRequest::Inline)?;
    let secs = start.elapsed().as_secs_f64();
    let seal_bytes = p254.get_seal_bytes();
    fs::write(dir.join("seal.r0"), &seal_bytes)?;
    fs::write(dir.join("input.json"), groth16_prover_input(&seal_bytes)?)?;
    let record = json!({
        "step": "identity_p254", "image_id": hx(id.as_bytes()), "wall_s": secs, "loadavg_before": load_before,
        "loadavg_after": loadavg(), "p254_seal_bytes": seal_bytes.len(),
        "threads": std::thread::available_parallelism().map(usize::from).unwrap_or(0),
    });
    write_json(&dir.join("p254.json"), &record)?;
    println!("{record}");
    Ok(())
}

/// The JSON input of RISC Zero's Groth16 prover image (risc0-groth16's `prove::to_json`, feature `wrap`).
#[cfg(feature = "wrap")]
fn groth16_prover_input(seal_bytes: &[u8]) -> Result<String> {
    risc0_groth16::prove::to_json(seal_bytes)
}

#[cfg(not(feature = "wrap"))]
fn groth16_prover_input(_seal_bytes: &[u8]) -> Result<String> {
    bail!("built without the feature `wrap`")
}

/// Step 2, after the prover image wrote `proof.json`: the Groth16 receipt exactly as
/// `succinct_to_groth16` builds it (the proof, the succinct receipt's claim, the default verifier
/// parameters), verified here before anything is written.
fn cmd_groth16(opts: &BTreeMap<String, String>) -> Result<()> {
    refuse_dev_mode()?;
    let succinct_receipt = load_receipt(required(opts, "receipt")?)?;
    let id = image_id(opts)?;
    let succinct = succinct_receipt.inner.succinct().context("not a succinct receipt")?;
    let proof_path = required(opts, "proof")?;
    let proof: risc0_groth16::ProofJson =
        serde_json::from_str(&fs::read_to_string(proof_path).with_context(|| proof_path.to_string())?)?;
    let seal: risc0_groth16::Seal = proof.try_into()?;
    let groth16 = Groth16Receipt::new(
        seal.to_vec(),
        succinct.claim.clone(),
        Groth16ReceiptVerifierParameters::default().digest(),
    );
    let receipt = Receipt::new(InnerReceipt::Groth16(groth16), succinct_receipt.journal.bytes.clone());
    write_groth16(
        Path::new(required(opts, "out-dir")?),
        &receipt,
        id,
        json!({"step": "groth16 receipt from proof.json (prover image run outside r0vm)"}),
    )
}

/// Both steps through r0vm, which shells out to Docker for the second.
fn cmd_wrap(opts: &BTreeMap<String, String>) -> Result<()> {
    refuse_dev_mode()?;
    let succinct = load_receipt(required(opts, "receipt")?)?;
    let id = image_id(opts)?;
    succinct.verify(id).context("the succinct receipt does not verify")?;
    let load_before = loadavg();
    let start = Instant::now();
    let receipt = default_prover().compress(&ProverOpts::groth16(), &succinct)?;
    let secs = start.elapsed().as_secs_f64();
    ensure!(receipt.journal.bytes == succinct.journal.bytes, "journal changed in the wrap");
    write_groth16(
        Path::new(required(opts, "out-dir")?),
        &receipt,
        id,
        json!({"step": "identity_p254 + Groth16 through r0vm (Docker)", "wall_s": secs, "loadavg_before": load_before,
               "loadavg_after": loadavg()}),
    )
}

// ---- the fixture the vault's tests and scripts read ---------------------------------------------------

fn trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// `{journal, seal?}` of one proof of a run: the journal from the native run's meta file, and the
/// seal from `receipts/<name>/seal.hex` if that receipt proves this journal. A receipt of another
/// journal (a finalisation proven for an earlier beacon) is left out and named on stderr.
fn proof_json(run: &Path, name: &str) -> Result<Option<Value>> {
    let meta_path = run.join(format!("{name}.json"));
    if !meta_path.exists() {
        return Ok(None);
    }
    let (_, journal) = expected_journal(&meta_path.to_string_lossy())?;
    let mut out = json!({"journal": hx(&journal), "journalDigest": hx(&sha256(&journal))});
    let receipts = run.join("receipts").join(name);
    if let Some(seal) = trimmed(&receipts.join("seal.hex")) {
        let proven = trimmed(&receipts.join("journal.hex")).with_context(|| format!("{name}: journal.hex"))?;
        ensure!(unhex_vec(&seal)?.len() == 260, "{name}: seal length");
        if proven == hx(&journal) {
            out["seal"] = json!(seal);
        } else {
            eprintln!("{name}: the receipt in receipts/{name}/ proves another journal; no seal in the fixture");
        }
    }
    Ok(Some(out))
}

fn cmd_fixture(opts: &BTreeMap<String, String>) -> Result<()> {
    let run = PathBuf::from(required(opts, "run-dir")?);
    let file = |name: &str| read_json(&run.join(name).to_string_lossy());
    let (p, d, w, lez) = (file("params.json")?, file("deployment.json")?, file("wallet.json")?, file("lez.json")?);
    let (_, config_id) = identity(&d)?;
    let program_id = |name: &str| -> Result<String> {
        let path = required(opts, name)?;
        Ok(hx(compute_image_id(&fs::read(path).with_context(|| path.to_string())?)?.as_bytes()))
    };

    let deposits: Vec<Value> = list(&w, "deposits")?
        .iter()
        .map(|dep| json!({"token": dep["token"], "amount": dep["amount"], "commitment": dep["commitment"]}))
        .collect();
    let mut releases = Vec::new();
    for spec in list(&w, "own")? {
        let name = text(spec, "name")?;
        let meta = file(&format!("release-{name}.json"))?;
        let mut release = proof_json(&run, &format!("release-{name}"))?.context("release meta")?;
        for key in ["token", "amount", "recipient", "relayer"] {
            release[key] = meta[key].clone();
        }
        release["name"] = json!(name);
        release["relayFee"] = meta["relay_fee"].clone();
        release["burnRoot"] = meta["burn_root"].clone();
        release["nullifier"] = meta["nullifier"].clone();
        releases.push(release);
    }
    let candidate = proof_json(&run, "cp-e2e-candidate")?.context("cp-e2e-candidate.json")?;
    let mut value = json!({
        "what": "End-to-end return-leg fixture (e2e/README.md): what vault/test/real and vault/script read. Journals \
                 are the native runs' and the proven receipts'; seals are selector ‖ Groth16 proof. Test keys and \
                 a stand-in zone; the proofs are real.",
        "chainId": number(&d, "evm_chain_id")?,
        "deployer": p["deployer"], "groth16Verifier": p["groth16_verifier"], "emergencyStop": p["emergency_stop"],
        "token": p["token"], "vault": d["vault"],
        "checkpointImageId": program_id("checkpoint-program")?,
        "releaseImageId": program_id("release-program")?,
        "lezConfigId": hx(&config_id),
        "genesisState": d["genesis"]["state"]["S"],
        "nMin": d["n_min"], "tMin": d["t_min"], "n": d["n"], "t": d["t"],
        "postingTimeframe": d["posting_timeframe"],
        "depositor": w["depositor"], "depositRoot": w["import_root"]["root"],
        "depositCount": w["import_root"]["leaf_count"], "deposits": deposits,
        "burnRoot": lez["state_at_t"]["burn_root"], "burnCount": lez["state_at_t"]["burn_count"],
        "newState": lez["state_at_t"]["S"],
        "candidate": candidate,
        "releaseCount": releases.len(), "releases": releases,
    });
    if let Some(finalize) = proof_json(&run, "cp-e2e-finalize")? {
        value["finalize"] = finalize;
        value["echoNonce"] = lez["checkpoints"]["finalize"]["echo_nonce"].clone();
    }
    // The Ethereum block that recorded the beacon the finalisation echoes (written by the scripts).
    if let Ok(beacon) = file("beacon.json") {
        value["beacon"] = beacon;
    }
    let complete = value["candidate"]["seal"].is_string()
        && value["finalize"]["seal"].is_string()
        && value["releases"].as_array().is_some_and(|r| r.iter().all(|x| x["seal"].is_string()));
    value["complete"] = json!(complete);
    let out = PathBuf::from(required(opts, "out")?);
    write_json(&out, &value)?;
    println!("wrote {} ({})", out.display(), if complete { "all seals present" } else { "seals still missing" });
    Ok(())
}

// ---- the crown run's releases (e2e/crown/README.md, stage 9) ---------------------------------------------

/// `field` when present, else `default`.
fn field_or<const N: usize>(v: &Value, key: &str, default: [u8; N]) -> Result<[u8; N]> {
    if v[key].is_null() {
        Ok(default)
    } else {
        field(v, key)
    }
}

fn cmd_crown_release(opts: &BTreeMap<String, String>) -> Result<()> {
    let id = read_json(required(opts, "identity")?)?;
    let burns = read_json(required(opts, "burns")?)?;
    let notes = read_json(required(opts, "notes")?)?;
    let dir = PathBuf::from(required(opts, "out-dir")?);
    fs::create_dir_all(&dir)?;
    let chain_id = chain_id_u256(number(&id, "chainId")?);
    let vault: Address = field(&id, "vault")?;
    let config_id: Hash = field(&burns, "config_id")?;
    if !id["configId"].is_null() {
        ensure!(
            config_id == field::<32>(&id, "configId")?,
            "burns.json is for another configuration than identity.json"
        );
    }

    // 1. The burn tree: every leaf of the checkpoint's burns recomputed by the bridge core, both trees.
    let all = list(&burns, "burns")?;
    let (mut leaves, mut frontier) = (Vec::with_capacity(all.len()), IncrementalTree::new());
    for (k, b) in all.iter().enumerate() {
        ensure!(number(b, "burn_index")? == k as u64, "burn {k}: burn_index");
        let leaf = burn_leaf(
            &config_id,
            k as u64,
            &field(b, "token")?,
            &field(b, "amount")?,
            &field(b, "c_out")?,
            &field(b, "r")?,
        );
        ensure!(leaf == field::<32>(b, "leaf")?, "burn {k}: the bridge core's leaf is not the checkpoint core's");
        frontier.append(leaf).map_err(|_| anyhow::anyhow!("burn tree full"))?;
        leaves.push(leaf);
    }
    let tree = Tree::from_leaves(leaves).map_err(|_| anyhow::anyhow!("burn tree full"))?;
    let burn_root = tree.root();
    ensure!(burn_root == frontier.root(), "full and incremental burn tree disagree");
    ensure!(burn_root == field::<32>(&burns, "burn_root")?, "the bridge core's burn root is not the checkpoint's");

    // 2. One release statement per own burn.
    let mut written = Vec::new();
    let mut fragments = Vec::new();
    // notes.json: {"burns": [{name, r, s_out, recipient, ...}]}, or the LEZ side's burn-notes.json: a list of
    // {i, kind, r, sOut, recipient, token, amount (decimal), cOut, denomIndex, ...} (spike interface lez.md).
    let entries: Vec<Value> = match notes.as_array() {
        Some(l) => l
            .iter()
            .map(|n| {
                let mut m = n.clone();
                // `i` and `denomIndex` may be numbers or decimal strings.
                let num = |v: &Value| v.as_u64().or_else(|| v.as_str().and_then(|t| t.parse().ok()));
                let i = num(&n["i"]).map_or_else(|| "x".to_string(), |v| v.to_string());
                m["name"] = json!(format!("{}-{i}", n["kind"].as_str().unwrap_or("burn")));
                m["denomIndex"] = num(&n["denomIndex"]).map_or(Value::Null, |v| json!(v));
                m["s_out"] = n["sOut"].clone();
                m["c_out"] = n["cOut"].clone();
                m["amount"] = match n["amount"].as_str() {
                    Some(d) if !d.starts_with("0x") => json!(hx(&u256_from_u128(d.parse().unwrap_or(u128::MAX)))),
                    _ => n["amount"].clone(),
                };
                m
            })
            .collect(),
        None => list(&notes, "burns")?.clone(),
    };
    for note in &entries {
        let name = text(note, "name")?;
        let r: Hash = field(note, "r")?;
        let (k, b) = all
            .iter()
            .enumerate()
            .find(|(_, b)| field::<32>(b, "r").ok() == Some(r))
            .with_context(|| format!("{name}: no burn with this r among the checkpoint's"))?;
        let (token, amount): (Address, U256) = (field(b, "token")?, field(b, "amount")?);
        ensure!(
            field_or(note, "token", token)? == token && field_or(note, "amount", amount)? == amount,
            "{name}: token or amount"
        );
        ensure!(field_or(note, "c_out", field::<32>(b, "c_out")?)? == field::<32>(b, "c_out")?, "{name}: cOut");
        let s_out: Hash = field(note, "s_out")?;
        let recipient: Address = field(note, "recipient")?;
        ensure!(
            burn_commitment(&s_out, &chain_id, &vault, &recipient) == field::<32>(b, "c_out")?,
            "{name}: COut from s_out, the chain id, the vault and the recipient is not the burn's"
        );
        let public = ReleasePublic {
            chain_id,
            vault,
            config_id,
            token,
            amount,
            recipient,
            relayer: field_or(note, "relayer", recipient)?,
            relay_fee: field_or(note, "relay_fee", [0u8; 32])?,
        };
        let witness = ReleaseWitness { burn_index: k as u64, path: tree.path(k as u64).context("path")?, s_out, r };
        let nullifier =
            verify_release(&burn_root, &witness, &public).map_err(|e| anyhow::anyhow!("{name}: release {e:?}"))?;
        let journal = release_journal(&burn_root, &nullifier, &public);
        let input = ReleaseInput { burn_root, witness, public: public.clone() };
        fs::write(dir.join(format!("release-{name}.input")), input.encode())?;
        let meta = json!({
            "kind": "release",
            "description": format!("crown run: release of the burn {name} (burn index {k}, LEZ block {})", b["block"]),
            "name": name, "burn_index": k, "block": b["block"], "origin": b["origin"],
            "burn_root": hx(&burn_root), "nullifier": hx(&nullifier), "token": hx(&public.token),
            "amount": hx(&public.amount), "recipient": hx(&public.recipient), "relayer": hx(&public.relayer),
            "relay_fee": hx(&public.relay_fee), "journal": hex::encode(journal),
        });
        write_json(&dir.join(format!("release-{name}.json")), &meta)?;
        // The crown.json `releases[i]` entry (vault/script/Crown.s.sol) but its seal, added after the wrap.
        fragments.push(json!({"name": name, "token": hx(&public.token), "denomIndex": note.get("denomIndex"),
            "recipient": hx(&public.recipient), "relayer": hx(&public.relayer), "relayFee": hx(&public.relay_fee),
            "burnRoot": hx(&burn_root), "nullifier": hx(&nullifier), "seal": null}));
        written.push(name.to_string());
    }
    write_json(&dir.join("releases.json"), &json!(fragments))?;
    println!(
        "burn root {} over {} burns (bridge core = checkpoint core, leaf by leaf); release inputs for {:?}",
        hx(&burn_root),
        all.len(),
        written
    );
    Ok(())
}

fn cmd_crown_synthetic(opts: &BTreeMap<String, String>) -> Result<()> {
    let id = read_json(required(opts, "identity")?)?;
    let dir = PathBuf::from(required(opts, "out-dir")?);
    fs::create_dir_all(&dir)?;
    let chain_id = chain_id_u256(number(&id, "chainId")?);
    let vault: Address = field(&id, "vault")?;
    let config_id: Hash = field(&id, "configId")?;
    let wallet = BridgeKeys::from_bridge_seed(&label("crown-proofs: synthetic wallet (test value)"));
    let other = BridgeKeys::from_bridge_seed(&label("crown-proofs: synthetic other user (test value)"));
    let (token, amount) = ([0u8; 20], u256_from_u128(1_000_000_000_000_000));
    let (mut burns, mut notes, mut tree) = (Vec::new(), Vec::new(), IncrementalTree::new());
    let mut push = |note: &BurnNote, burns: &mut Vec<Value>| -> Result<()> {
        let k = burns.len() as u64;
        let leaf = burn_leaf(&config_id, k, &note.token, &note.amount, &note.c_out, &note.r);
        tree.append(leaf).map_err(|_| anyhow::anyhow!("burn tree full"))?;
        burns.push(json!({"burn_index": k, "block": null, "origin": "STAND-IN", "r": hx(&note.r), "memo": hx(&note.memo),
            "mac": hx(&note.mac), "token": hx(&note.token), "amount": hx(&note.amount), "c_out": hx(&note.c_out), "leaf": hx(&leaf)}));
        Ok(())
    };
    for (i, name) in ["public", "private"].into_iter().enumerate() {
        let target =
            ReleaseTarget { chain_id, vault, recipient: label_addr("crown-proofs: synthetic other recipient") };
        push(&BurnNote::create(&other, &config_id, i as u64, &target, token, amount), &mut burns)?;
        let recipient = label_addr(&format!("crown-proofs: synthetic recipient {name}"));
        let note = BurnNote::create(
            &wallet,
            &config_id,
            i as u64,
            &ReleaseTarget { chain_id, vault, recipient },
            token,
            amount,
        );
        push(&note, &mut burns)?;
        notes
            .push(json!({"name": name, "r": hx(&note.r), "s_out": hx(&burn_secret(&wallet.auth, &config_id, &note.r)),
            "recipient": hx(&recipient)}));
    }
    let label_text = "STAND-IN: synthetic burns of a test wallet (crown-proofs rehearsal), not from LEZ blocks";
    write_json(
        &dir.join("burns.json"),
        &json!({"label": label_text, "config_id": hx(&config_id), "burn_count": burns.len(),
        "burn_root": hx(&tree.root()), "burns": burns}),
    )?;
    write_json(&dir.join("notes.json"), &json!({"label": label_text, "burns": notes}))?;
    println!("wrote {} synthetic burns (root {}) and 2 notes", burns.len(), hx(&tree.root()));
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("image-id") => cmd_image_id(rest),
        Some("program-binary") => cmd_program_binary(rest),
        Some("notes") => cmd_notes(&options(rest)?),
        Some("release") => cmd_release(&options(rest)?),
        Some("exec") => cmd_exec(&options(rest)?),
        Some("prove") => cmd_prove(&options(rest)?),
        Some("verify") => cmd_verify(&options(rest)?),
        Some("p254") => cmd_p254(&options(rest)?),
        Some("groth16") => cmd_groth16(&options(rest)?),
        Some("wrap") => cmd_wrap(&options(rest)?),
        Some("fixture") => cmd_fixture(&options(rest)?),
        Some("crown-release") => cmd_crown_release(&options(rest)?),
        Some("crown-synthetic") => cmd_crown_synthetic(&options(rest)?),
        _ => bail!(
            "usage: ferrata-e2e image-id | notes | release | exec | prove | verify | p254 | groth16 | wrap | fixture \
             (see the module documentation for their options)"
        ),
    }
}
