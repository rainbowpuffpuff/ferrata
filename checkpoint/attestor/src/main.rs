//! `ferrata_attestor`: rule (B)'s signer, run by each sequencer beside its Logos Blockchain node, and
//! the collector of its attestations (library docs: `ferrata_attestor::*`).
//!
//! ```text
//! keygen  --out KEY [--seed-label L]          a 32-byte attestation key (OS randomness; a label only
//!                                              for reproducible devnet keys); prints its public key
//! pubkey  --key KEY                            the public key of a 32-byte secret key file
//! deployment --input IN --out DEP              the deployment (chain, vault, configId, channel) a
//!                                              checkpoint_b input's configuration serves
//! certify --channel-key KEY --att-pub HEX --deployment DEP --out CERT
//!                                              the channel key's one certificate of an attestation key
//! attest  --deployment DEP --cert CERT --att-key KEY --out ATT
//!         (--node URL --lez URL --save DIR --state FILE [--slot-from N] | --answers DIR [--lez-answers DIR] [--state FILE])
//!                                              read the node's final tier, check its tip against the
//!                                              LEZ node (applied and final there), sign it; the state
//!                                              records the tip before ATT is written, and the next read
//!                                              starts at its slot (--slot-from only without a state)
//! verify  --deployment DEP --att ATT...        check published attestations (both signatures)
//! collect --base IN --att ATT... --out IN2 --meta JSON --name NAME
//!                                              t of n valid attestations into the input; the native
//!                                              rule run; refuses below t
//! assemble --base IN --att ATT... --out IN2 --meta JSON --name NAME --expect WHY
//!                                              the same, unchecked: an input the program must refuse
//! upgrade --old IN --answers DIR --channel HEX --tip N --out IN2
//!                                             a checkpoint_b input recorded before inputs carried T's
//!                                              inscription, with T's inscription from the node's answers
//! retip   --base IN --answers DIR --tip N --out IN2   T moved to block N of the input (its inscription
//!                                              from the node); the later blocks become the path after T
//! tier    --answers DIR [--channel HEX]        the Bedrock node's view alone (not signable): tip, key
//!                                              set, blocks per signer
//! devnet-lez-standin --answers DIR [--channel HEX]
//!                                              devnet replays of recorded runs without a LEZ node: saves
//!                                              every inscribed block as the LEZ node's final block
//! base    --answers DIR --genesis FILE --from A --to T --n-min N --t-min T --out IN
//!                                              devnet set-up: an input over final blocks A..=T from the
//!                                              node's answers alone (bridge values are stand-ins)
//! rekey   --base IN --keys HEX,HEX,... --n-min N --t-min T [--posting-timeframe P] --out IN2
//! rotate  --base IN --keys HEX,HEX,... --threshold C --signer I:KEY... --out IN2
//!                                              devnet set-up: one rotation of the base's set, signed
//!                                              by the keys at indices I of the parent's set
//!                                              devnet set-up: a stand-in key set and floors
//! ```
use std::fs;
use std::io::Read as _;
use std::path::Path;

use anyhow::{anyhow, bail, ensure, Context as _, Result};
use checkpoint_core::hash::sha256_parts;
use checkpoint_core::input::CheckpointInputB;
use checkpoint_core::Hash;
use ed25519_dalek::SigningKey;
use ferrata_attestor::{
    assemble_unchecked, attestation_from_json, attestation_json, bedrock_tier, collect, deployment_from_json,
    deployment_json, deployment_of, devnet_lez_standin, final_tier, fixture_meta, hex_str, hexa, read_key, read_state,
    round, save_lez_answers, write_atomic, write_secret, write_state, Attestor, AttestorState, LezSaved, NodeAnswers,
};
use serde_json::{json, Value};

struct Args(Vec<(String, String)>);

impl Args {
    fn parse() -> Result<(String, Self)> {
        let v: Vec<String> = std::env::args().skip(1).collect();
        let cmd = v.first().context("a command is required (see the module docs)")?.clone();
        let rest = &v[1..];
        ensure!(rest.len().is_multiple_of(2), "options come in --key value pairs");
        let pairs = rest
            .chunks(2)
            .map(|p| Ok((p[0].strip_prefix("--").context("expected --key")?.to_string(), p[1].clone())))
            .collect::<Result<_>>()?;
        Ok((cmd, Self(pairs)))
    }
    fn opt(&self, k: &str) -> Option<&str> {
        self.0.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str())
    }
    fn get(&self, k: &str) -> Result<&str> {
        self.opt(k).with_context(|| format!("--{k} is required"))
    }
    fn all(&self, k: &str) -> Vec<&str> {
        self.0.iter().filter(|(key, _)| key == k).map(|(_, v)| v.as_str()).collect()
    }
}

fn read_json(path: &str) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| path.to_string())?).with_context(|| path.to_string())
}

fn write_json(path: &str, v: &Value) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(v)? + "\n").with_context(|| path.to_string())
}

fn cmd_keygen(a: &Args) -> Result<()> {
    let secret: [u8; 32] = match a.opt("seed-label") {
        Some(label) => sha256_parts(&[b"ferrata/attestor/devnet-key/", label.as_bytes()]),
        None => {
            let mut b = [0u8; 32];
            fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
            b
        }
    };
    write_secret(Path::new(a.get("out")?), &secret)?;
    println!("{}", hex::encode(SigningKey::from_bytes(&secret).verifying_key().to_bytes()));
    Ok(())
}

fn cmd_deployment(a: &Args) -> Result<()> {
    let bytes = fs::read(a.get("input")?)?;
    let input = CheckpointInputB::decode(&bytes).map_err(|e| anyhow!("input: {e:?}"))?;
    let d = deployment_json(&deployment_of(&input.config));
    write_json(a.get("out")?, &d)?;
    println!("{d}");
    Ok(())
}

fn cmd_certify(a: &Args) -> Result<()> {
    let channel = read_key(Path::new(a.get("channel-key")?))?;
    let d = deployment_from_json(&read_json(a.get("deployment")?)?)?;
    let att_pub: Hash = hex_str(a.get("att-pub")?, "att-pub")?;
    let certificate = attestor_core::sign_certificate(&channel, &d, &att_pub);
    let channel_pub = channel.verifying_key().to_bytes();
    ensure!(attestor_core::verify_certificate(&channel_pub, &d, &att_pub, &certificate), "certificate does not verify");
    let v = json!({
        "channel_key": hex::encode(channel_pub), "attestation_key": hex::encode(att_pub),
        "deployment": deployment_json(&d), "certificate_message": hex::encode(d.certificate_message(&att_pub)),
        "certificate": hex::encode(certificate),
    });
    write_json(a.get("out")?, &v)?;
    println!("certified {} by channel key {}", hex::encode(att_pub), hex::encode(channel_pub));
    Ok(())
}

fn cmd_attest(a: &Args) -> Result<()> {
    let d = deployment_from_json(&read_json(a.get("deployment")?)?)?;
    let cert = read_json(a.get("cert")?)?;
    let attestor = Attestor {
        deployment: d,
        channel_key: hexa(&cert["channel_key"], "cert channel_key")?,
        certificate: hexa(&cert["certificate"], "cert certificate")?,
        att_key: read_key(Path::new(a.get("att-key")?))?,
    };
    let out = a.get("out")?;
    let (tier, att) = match (a.opt("node"), a.opt("answers")) {
        (Some(node), None) => {
            let state = a.get("state").context("a live attestor keeps a state (--state): what it signed")?;
            let from = a.opt("slot-from").map(str::parse).transpose()?.unwrap_or(0);
            let r = round(&attestor, node, a.get("lez")?, Path::new(a.get("save")?), Path::new(state), from)
                .map_err(|e| anyhow!("{e}"))?;
            println!("read the node from slot {}", r.scanned_from);
            (r.tier, r.attestation)
        }
        (None, Some(dir)) => {
            let state = a.opt("state").map(Path::new);
            let before = match state {
                Some(p) => read_state(p, &d.channel_id)?,
                None => None,
            };
            let answers = NodeAnswers::load(Path::new(dir))?;
            let mut lez = LezSaved::new(Path::new(a.opt("lez-answers").unwrap_or(dir)));
            let tier = final_tier(&answers, &mut lez, &d.channel_id, before.as_ref())?;
            let att = attestor.attest(&tier, before.as_ref().map(|s| s.last))?;
            if let Some(p) = state {
                write_state(p, &d.channel_id, &AttestorState::of(&tier))?;
            }
            (tier, att)
        }
        _ => bail!("exactly one of --node URL (with --lez URL, --save DIR, --state FILE) or --answers DIR"),
    };
    // Published only after the state records it.
    let record = attestation_json(&d, &att, &tier);
    write_atomic(Path::new(out), (serde_json::to_string_pretty(&record)? + "\n").as_bytes())?;
    for n in &tier.notes {
        eprintln!("note: {n}");
    }
    println!(
        "attested: channel key {} | attestation key {} | final tip {} {} (final message {}) | node lib slot {}, \
         blocks {}..={} followed, the tip final in the LEZ node | key set n = {} hash {}",
        hex::encode(att.channel_key),
        hex::encode(att.att_key),
        att.tip.tip_id,
        hex::encode(att.tip.tip_hash),
        hex::encode(att.tip.final_msg_id),
        tier.lib_slot,
        tier.first_block,
        att.tip.tip_id,
        tier.keys.len(),
        hex::encode(tier.key_set_hash)
    );
    Ok(())
}

fn cmd_lez_standin(a: &Args) -> Result<()> {
    let dir = Path::new(a.get("answers")?);
    let channel: Hash = hex_str(a.opt("channel").unwrap_or(DEFAULT_CHANNEL), "channel")?;
    let answers = NodeAnswers::load(dir)?;
    let lez = devnet_lez_standin(&answers, &channel)?;
    save_lez_answers(dir, &lez)?;
    eprintln!(
        "devnet stand-in: {} inscribed blocks saved as the LEZ node's final blocks under {}/lez (this assumes every \
         inscribed block applied: replays of recorded honest runs only)",
        lez.len(),
        dir.display()
    );
    Ok(())
}

fn cmd_verify(a: &Args) -> Result<()> {
    let d = deployment_from_json(&read_json(a.get("deployment")?)?)?;
    let mut bad = 0;
    for path in a.all("att") {
        let v = read_json(path)?;
        let att = attestation_from_json(&v)?;
        let ksh: Hash = hexa(&v["key_set_hash"], "key_set_hash")?;
        match att.verify(&d, &ksh) {
            Ok(()) => {
                println!("{path}: verifies (channel key {}, tip {})", hex::encode(att.channel_key), att.tip.tip_id)
            }
            Err(e) => {
                bad += 1;
                println!("{path}: REFUSED {e:?}");
            }
        }
    }
    ensure!(bad == 0, "{bad} attestation(s) refused");
    Ok(())
}

fn load_atts(a: &Args) -> Result<Vec<attestor_core::SignedAttestation>> {
    a.all("att").into_iter().map(|p| attestation_from_json(&read_json(p)?)).collect()
}

fn cmd_collect(a: &Args) -> Result<()> {
    let base = fs::read(a.get("base")?)?;
    let atts = load_atts(a)?;
    let c = collect(&base, &atts)?;
    for (i, why) in &c.dropped {
        println!("dropped attestation {} ({}): {why:?}", i, a.all("att")[*i]);
    }
    println!("kept {} of {} (n = {}, t = {})", c.kept.len(), atts.len(), c.n, c.t);
    let name = a.get("name")?;
    let description = a.opt("description").unwrap_or("rule (B): t of n attestations collected by ferrata_attestor");
    let meta = fixture_meta(name, &c.input, description, None)?;
    fs::write(a.get("out")?, &c.input)?;
    write_json(a.get("meta")?, &meta)?;
    println!("{name}: {}", meta["native"].as_str().unwrap_or(""));
    Ok(())
}

fn cmd_assemble(a: &Args) -> Result<()> {
    let base = fs::read(a.get("base")?)?;
    let bytes = assemble_unchecked(&base, &load_atts(a)?)?;
    let name = a.get("name")?;
    let meta = fixture_meta(
        name,
        &bytes,
        a.opt("description").unwrap_or("an input the program must refuse"),
        Some(a.get("expect")?),
    )?;
    fs::write(a.get("out")?, &bytes)?;
    write_json(a.get("meta")?, &meta)?;
    println!("{name}: {}", meta["native"].as_str().unwrap_or(""));
    Ok(())
}

fn cmd_upgrade(a: &Args) -> Result<()> {
    let old = fs::read(a.get("old")?)?;
    let answers = NodeAnswers::load(Path::new(a.get("answers")?))?;
    let channel: Hash = hex_str(a.get("channel")?, "channel")?;
    let tip_id: u64 = a.get("tip")?.parse()?;
    let ins = ferrata_attestor::final_inscriptions(&answers, &channel)?;
    let t = ins.get(&tip_id).with_context(|| format!("the node holds no final inscription of block {tip_id}"))?;
    let bytes = ferrata_attestor::upgrade_pre_producer_input(&old, &t.elided()?)?;
    fs::write(a.get("out")?, &bytes)?;
    println!("upgraded: T = {tip_id}, its inscription from node block {} (slot {})", t.node_block, t.slot);
    Ok(())
}

fn cmd_retip(a: &Args) -> Result<()> {
    let base = fs::read(a.get("base")?)?;
    let answers = NodeAnswers::load(Path::new(a.get("answers")?))?;
    let bytes = ferrata_attestor::retip(&base, &answers, a.get("tip")?.parse()?)?;
    fs::write(a.get("out")?, &bytes)?;
    let input = CheckpointInputB::decode(&bytes).map_err(|e| anyhow!("{e:?}"))?;
    println!("T moved: tIndex {} of {} blocks", input.t_index, input.blocks.len());
    Ok(())
}

const DEFAULT_CHANNEL: &str = "0101010101010101010101010101010101010101010101010101010101010101";

fn cmd_tier(a: &Args) -> Result<()> {
    let answers = NodeAnswers::load(Path::new(a.get("answers")?))?;
    let channel: Hash = hex_str(a.opt("channel").unwrap_or(DEFAULT_CHANNEL), "channel")?;
    let tier = bedrock_tier(&answers, &channel)?;
    let signers = ferrata_attestor::signers(&answers, &channel)?;
    let mut per: std::collections::BTreeMap<String, Vec<u64>> = std::collections::BTreeMap::new();
    for (id, k) in &signers {
        per.entry(hex::encode(k)).or_default().push(*id);
    }
    let v = json!({
        "tip_id": tier.tip.tip_id, "tip_hash": hex::encode(tier.tip.tip_hash),
        "final_msg_id": hex::encode(tier.tip.final_msg_id), "tip_signer": hex::encode(tier.tip_signer),
        "first_block": tier.first_block, "blocks_checked": tier.hashes.len(), "lib_slot": tier.lib_slot,
        "keys": tier.keys.iter().map(hex::encode).collect::<Vec<_>>(), "posting_timeframe": tier.posting_timeframe,
        "blocks_per_signer": per.iter().map(|(k, ids)| (k.clone(), json!({"count": ids.len(),
            "first": ids.first(), "last": ids.last()}))).collect::<serde_json::Map<_, _>>(),
    });
    match a.opt("out") {
        Some(p) => write_json(p, &v)?,
        None => println!("{}", serde_json::to_string_pretty(&v)?),
    }
    Ok(())
}

fn cmd_base(a: &Args) -> Result<()> {
    let answers = NodeAnswers::load(Path::new(a.get("answers")?))?;
    let channel: Hash = hex_str(a.opt("channel").unwrap_or(DEFAULT_CHANNEL), "channel")?;
    let genesis_id = sha256_parts(&[fs::read(a.get("genesis")?)?.as_slice()]);
    let bytes = ferrata_attestor::base_from_answers(
        &answers,
        &channel,
        genesis_id,
        a.get("from")?.parse()?,
        a.get("to")?.parse()?,
        a.get("n-min")?.parse()?,
        a.get("t-min")?.parse()?,
    )?;
    fs::write(a.get("out")?, &bytes)?;
    let input = CheckpointInputB::decode(&bytes).map_err(|e| anyhow!("{e:?}"))?;
    println!(
        "base: blocks {}..={}, n = {}, configId {}, parent state {}",
        input.parent.tip_id + 1,
        input.parent.tip_id + input.blocks.len() as u64,
        input.parent.keys.len(),
        hex::encode(input.config.config_id()),
        hex::encode(input.parent.digest())
    );
    Ok(())
}

fn cmd_rotate(a: &Args) -> Result<()> {
    let base = fs::read(a.get("base")?)?;
    let input = CheckpointInputB::decode(&base).map_err(|e| anyhow!("{e:?}"))?;
    let keys: Vec<Hash> = a.get("keys")?.split(',').map(|k| hex_str(k, "key")).collect::<Result<_>>()?;
    let ops = ferrata_attestor::channel_config_tx(
        &input.config.channel_id,
        &input.parent.config_tip_hash,
        &keys,
        input.parent.posting_timeframe,
        a.get("threshold")?.parse()?,
    )?;
    let signers: Vec<(u16, SigningKey)> = a
        .all("signer")
        .into_iter()
        .map(|s| {
            let (i, path) = s.split_once(':').context("--signer INDEX:KEYFILE")?;
            Ok((i.parse()?, read_key(Path::new(path))?))
        })
        .collect::<Result<_>>()?;
    let refs: Vec<(u16, &SigningKey)> = signers.iter().map(|(i, k)| (*i, k)).collect();
    let bytes = ferrata_attestor::rotate_base(&base, &ops, &refs)?;
    fs::write(a.get("out")?, &bytes)?;
    println!("rotation to {} keys signed by {} of {}", keys.len(), refs.len(), input.parent.keys.len());
    Ok(())
}

fn cmd_rekey(a: &Args) -> Result<()> {
    let base = fs::read(a.get("base")?)?;
    let keys: Vec<Hash> = a.get("keys")?.split(',').map(|k| hex_str(k, "key")).collect::<Result<_>>()?;
    let timeframe = a.opt("posting-timeframe").map(str::parse).transpose()?;
    let bytes = ferrata_attestor::rekey_base_with_timeframe(
        &base,
        &keys,
        a.get("n-min")?.parse()?,
        a.get("t-min")?.parse()?,
        timeframe,
    )?;
    fs::write(a.get("out")?, &bytes)?;
    let input = CheckpointInputB::decode(&bytes).map_err(|e| anyhow!("{e:?}"))?;
    println!(
        "re-keyed: n = {}, nMin = {}, tMin = {}, configId {}, parent state {}",
        input.parent.keys.len(),
        input.config.n_min,
        input.config.t_min,
        hex::encode(input.config.config_id()),
        hex::encode(input.parent.digest())
    );
    Ok(())
}

fn main() -> Result<()> {
    let (cmd, a) = Args::parse()?;
    match cmd.as_str() {
        "keygen" => cmd_keygen(&a),
        "pubkey" => {
            println!("{}", hex::encode(read_key(Path::new(a.get("key")?))?.verifying_key().to_bytes()));
            Ok(())
        }
        "deployment" => cmd_deployment(&a),
        "certify" => cmd_certify(&a),
        "attest" => cmd_attest(&a),
        "verify" => cmd_verify(&a),
        "collect" => cmd_collect(&a),
        "assemble" => cmd_assemble(&a),
        "rekey" => cmd_rekey(&a),
        "upgrade" => cmd_upgrade(&a),
        "retip" => cmd_retip(&a),
        "rotate" => cmd_rotate(&a),
        "tier" => cmd_tier(&a),
        "base" => cmd_base(&a),
        "devnet-lez-standin" => cmd_lez_standin(&a),
        other => bail!("unknown command {other}"),
    }
}
