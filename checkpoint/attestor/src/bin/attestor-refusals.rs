//! Test fixtures, not part of an attestor: `checkpoint_b` inputs that the checkpoint program must
//! refuse, over the same blocks, key set and deployment as the devnet run (`attestor/devnet.sh`). The
//! production `ferrata_attestor` refuses to sign any of these; this program signs them anyway, with
//! the devnet's seeded throwaway keys, so that the guest can be shown refusing them.
//!
//! ```text
//! attestor-refusals --base BASE_N3 --answers DIR --out-dir DIR
//! ```
//! writes `rbm-refuse-<case>.{input,json}` (the json says which refusal the native run gave).
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
