//! Test helper of `tests/anvil-e2e.sh`: four throwaway channel keys (seeds from public labels), each
//! certifying an attestation key for one deployment, and their final-tier messages v2: keys 0-2 attest
//! the honest tip, key 3 (a live, certified key: a corrupted attestor) attests a conflicting tip. Prints
//! `{"keys": [...], "attestations": [...]}` for a node view. Never for real keys.
//!
//! attestations CHAIN_ID VAULT LEZ_CONFIG_ID CHANNEL_ID HONEST_TIP_ID HONEST_HASH CONFLICT_TIP_ID CONFLICT_HASH

use attestor_core::{key_set_hash, sign_certificate, Deployment, FinalTip, SignedAttestation};
use ed25519_dalek::SigningKey;
use ferrata_monitor::node::H32;

fn seed(label: &str, i: u8) -> [u8; 32] {
    let mut s = [0u8; 32];
    for (k, b) in format!("ferrata/monitor-rehearsal/{label}/{i}").bytes().enumerate() {
        s[k % 32] ^= b;
    }
    s
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        a.len() == 8,
        "usage: attestations CHAIN_ID VAULT LEZ_CONFIG_ID CHANNEL_ID TIP HASH CONFLICT_TIP HASH"
    );
    let mut evm_chain_id = [0u8; 32];
    evm_chain_id[24..].copy_from_slice(&a[0].parse::<u64>()?.to_be_bytes());
    let vault: [u8; 20] =
        hex::decode(a[1].trim_start_matches("0x"))?.try_into().map_err(|_| anyhow::anyhow!("vault"))?;
    let d =
        Deployment { evm_chain_id, vault, lez_config_id: a[2].parse::<H32>()?.0, channel_id: a[3].parse::<H32>()?.0 };
    let channel: Vec<SigningKey> = (0..4).map(|i| SigningKey::from_bytes(&seed("channel", i))).collect();
    let keys: Vec<[u8; 32]> = channel.iter().map(|k| k.verifying_key().to_bytes()).collect();
    let ksh = key_set_hash(&keys, 900);
    let mut attestations = Vec::new();
    for (i, ch) in channel.iter().enumerate() {
        let att = SigningKey::from_bytes(&seed("attestation", i as u8));
        let certificate = sign_certificate(ch, &d, &att.verifying_key().to_bytes());
        let (id, hash) = if i < 3 { (&a[4], &a[5]) } else { (&a[6], &a[7]) };
        let tip =
            FinalTip { tip_id: id.parse()?, tip_hash: hash.parse::<H32>()?.0, final_msg_id: seed("msg", i as u8) };
        let sa = SignedAttestation::new(&att, keys[i], certificate, &d, &ksh, tip);
        attestations.push(format!("0x{}", hex::encode(sa.encode())));
    }
    let keys: Vec<String> = keys.iter().map(|k| format!("0x{}", hex::encode(k))).collect();
    println!("{}", serde_json::json!({ "keys": keys, "attestations": attestations }));
    Ok(())
}
