//! Test fixtures, not part of an attestor: `checkpoint_b` inputs that the checkpoint program must
//! refuse, over the same blocks, key set and deployment as the devnet run (`attestor/devnet.sh`). The
//! production `ferrata_attestor` refuses to sign any of these; this program signs them anyway, with
//! the devnet's seeded throwaway keys, so that the guest can be shown refusing them.
//!
//! ```text
//! attestor-refusals --base BASE_N3 --answers DIR --out-dir DIR
//! attestor-refusals --base BASE --real ATT,ATT,... [--stale ATT] --out-dir DIR [--prefix NAME]
//! ```
//! writes `rbm-refuse-<case>.{input,json}` (the json says which refusal the native run gave).
//!
//! With `--real`, the cases are built over a recorded zone run instead (RFP-022 F.7: `zone-bridge.sh input`): the
//! base input of the run and the real attestations of its attestors (`ATT`, the `final.json` files, whose keys were
//! never recorded), of which the first t - 1 are kept in every case, so that one more valid attestation would reach t:
//! `below-t` (those alone), `outsider` (with an attestation by a channel key outside the live key set, certifying its
//! own attestation key for this deployment and signing the same key set and tip: what a key retired by a rotation is
//! to the program), `repeated` (with one of them twice), and with `--stale` (an earlier round's attestation by an
//! accredited attestor, a real signature of an older tip) `stale-tip`. Writes `<prefix>-<case>.{input,json}`.
use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result};
use attestor_core::{sign_certificate, Deployment, SignedAttestation};
use checkpoint_core::hash::sha256_parts;
use checkpoint_core::input::CheckpointInputB;
use ed25519_dalek::SigningKey;
use ferrata_attestor::{assemble_unchecked, bedrock_tier, deployment_of, fixture_meta, NodeAnswers};

fn key(label: &str) -> SigningKey {
    SigningKey::from_bytes(&sha256_parts(&[b"ferrata/attestor/devnet-key/", label.as_bytes()]))
}

/// One refusal case: its name, the attestations its input carries, the refusal expected and what it is.
type Case = (String, Vec<SignedAttestation>, &'static str, String);

/// The refusal cases over a recorded run's base input and its real attestations (`--real`).
fn real_cases(
    base: &[u8],
    real: &[SignedAttestation],
    key_set_hash: &[u8; 32],
    stale: Option<SignedAttestation>,
) -> Result<Vec<Case>> {
    let input = CheckpointInputB::decode(base).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let d = deployment_of(&input.config);
    let n = u32::try_from(input.parent.keys.len())?;
    let t = usize::try_from(checkpoint_core::hash::threshold(n))?;
    anyhow::ensure!(t >= 2, "the run's key set needs t >= 2 (n = {n})");
    anyhow::ensure!(real.len() >= t, "{} real attestations, t(n = {n}) = {t}", real.len());
    let tip = real[0].tip;
    anyhow::ensure!(real.iter().all(|a| a.tip == tip), "the real attestations sign different tips");
    let kept: Vec<SignedAttestation> = real[..t - 1].to_vec();
    let (ch, at) = (key("real-run-outsider-channel"), key("real-run-outsider-attestation"));
    anyhow::ensure!(!input.parent.keys.contains(&ch.verifying_key().to_bytes()), "the outsider's key is in the set");
    let cert = sign_certificate(&ch, &d, &at.verifying_key().to_bytes());
    let outsider = SignedAttestation::new(&at, ch.verifying_key().to_bytes(), cert, &d, key_set_hash, tip);
    let with = |extra: SignedAttestation| -> Vec<SignedAttestation> {
        let mut v = kept.clone();
        v.push(extra);
        v
    };
    let mut cases = vec![
        (
            "below-t".to_string(),
            kept.clone(),
            "InsufficientAttestations",
            format!("{} real attestations of the t = {t} that n = {n} needs", t - 1),
        ),
        (
            "outsider".to_string(),
            with(outsider),
            "NotAccredited",
            format!(
                "{} real attestations and one by a channel key outside the live key set (certifying its own \
                  attestation key for this deployment, signing the same key set and tip): a retired key's case",
                t - 1
            ),
        ),
        (
            "repeated".to_string(),
            with(kept[0]),
            "DuplicateAttestor",
            format!("{} real attestations, one of them twice", t - 1),
        ),
    ];
    if let Some(s) = stale {
        anyhow::ensure!(input.parent.keys.contains(&s.channel_key), "the stale attestation's key is not accredited");
        anyhow::ensure!(s.tip.tip_id < tip.tip_id, "the stale attestation is not of an older tip");
        anyhow::ensure!(!kept.iter().any(|a| a.channel_key == s.channel_key), "the stale attestor is among the kept");
        cases.push((
            "stale-tip".to_string(),
            with(s),
            "NotADescendant",
            format!(
                "{} real attestations and an earlier round's real attestation by another accredited key, of tip \
                     {} (older than T = {})",
                t - 1,
                s.tip.tip_id,
                tip.tip_id
            ),
        ));
    }
    Ok(cases)
}

fn read_att(path: &str) -> Result<SignedAttestation> {
    let v: serde_json::Value = serde_json::from_slice(&fs::read(path).with_context(|| path.to_string())?)?;
    ferrata_attestor::attestation_from_json(&v).with_context(|| path.to_string())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str| -> Result<String> {
        let i = args.iter().position(|a| a == &format!("--{k}")).with_context(|| format!("--{k} is required"))?;
        args.get(i + 1).cloned().with_context(|| format!("--{k} needs a value"))
    };
    let base = fs::read(get("base")?)?;
    let out = get("out-dir")?;
    let out = Path::new(&out);
    fs::create_dir_all(out)?;
    if let Ok(list) = get("real") {
        let paths: Vec<&str> = list.split(',').filter(|p| !p.is_empty()).collect();
        let real = paths.iter().map(|p| read_att(p)).collect::<Result<Vec<_>>>()?;
        let first: serde_json::Value = serde_json::from_slice(&fs::read(paths[0])?)?;
        let ksh = ferrata_attestor::hexa::<32>(&first["key_set_hash"], "key_set_hash")?;
        let stale = get("stale").ok().map(|p| read_att(&p)).transpose()?;
        let prefix = get("prefix").unwrap_or_else(|_| "real-refuse".to_string());
        for (case, atts, expect, what) in real_cases(&base, &real, &ksh, stale)? {
            let name = format!("{prefix}-{case}");
            let bytes = assemble_unchecked(&base, &atts)?;
            let meta = fixture_meta(&name, &bytes, &format!("must be refused: {what}"), Some(expect))?;
            fs::write(out.join(format!("{name}.input")), &bytes)?;
            fs::write(out.join(format!("{name}.json")), serde_json::to_string_pretty(&meta)? + "\n")?;
            println!("{name}: {}", meta["native"].as_str().unwrap_or(""));
        }
        return Ok(());
    }
    let d = deployment_of(&CheckpointInputB::decode(&base).map_err(|e| anyhow::anyhow!("{e:?}"))?.config);
    let tier = bedrock_tier(&NodeAnswers::load(Path::new(&get("answers")?))?, &d.channel_id)?;
    let sign = |att: &SigningKey, channel: &SigningKey, cert_for: &Deployment, msg_for: &Deployment| {
        let cert = sign_certificate(channel, cert_for, &att.verifying_key().to_bytes());
        SignedAttestation::new(att, channel.verifying_key().to_bytes(), cert, msg_for, &tier.key_set_hash, tier.tip)
    };
    let (ch1, ch2, at1, at2) = (key("channel-1"), key("channel-2"), key("attestation-1"), key("attestation-2"));
    let a1 = sign(&at1, &ch1, &d, &d);
    let mut other_vault = d;
    other_vault.vault[19] ^= 1;
    let mut other_chain = d;
    other_chain.evm_chain_id[31] ^= 1;
    // attestation-3 presents channel-2's certificate of attestation-2.
    let cert_of_2 = sign_certificate(&ch2, &d, &at2.verifying_key().to_bytes());
    let uncertified = SignedAttestation::new(
        &key("attestation-3"),
        ch2.verifying_key().to_bytes(),
        cert_of_2,
        &d,
        &tier.key_set_hash,
        tier.tip,
    );
    let cases: Vec<(&str, Vec<SignedAttestation>, &str, &str)> = vec![
        ("too-few", vec![a1], "InsufficientAttestations", "one valid attestation of the t = 2 that n = 3 needs"),
        ("none", vec![], "InsufficientAttestations", "no attestation"),
        ("uncertified", vec![a1, uncertified], "BadCertificate", "an attestation key that its channel key never certified (it presents the certificate of another attestation key)"),
        ("other-vault", vec![a1, sign(&at2, &ch2, &other_vault, &d)], "BadCertificate", "an attestation key certified for another vault (same chain, configuration, channel)"),
        ("other-chain", vec![a1, sign(&at2, &ch2, &other_chain, &other_chain)], "BadCertificate", "an attestation key certified for, and signing, another chain id"),
        ("outsider", vec![a1, sign(&key("attestation-3"), &key("channel-3"), &d, &d)], "NotAccredited", "a channel key outside K certifying its own attestation key"),
        ("duplicate", vec![a1, a1], "DuplicateAttestor", "the same attestation twice"),
    ];
    for (case, atts, expect, what) in cases {
        let name = format!("rbm-refuse-{case}");
        let bytes = assemble_unchecked(&base, &atts)?;
        let meta = fixture_meta(&name, &bytes, &format!("must be refused: {what}"), Some(expect))?;
        fs::write(out.join(format!("{name}.input")), &bytes)?;
        fs::write(out.join(format!("{name}.json")), serde_json::to_string_pretty(&meta)? + "\n")?;
        println!("{name}: {}", meta["native"].as_str().unwrap_or(""));
    }
    Ok(())
}
