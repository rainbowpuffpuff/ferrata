//! `host gen-b`: rule (A) and rule (B) fixtures over the same blocks, at the committee sizes of
//! the specification, written next to the rule (A) fixtures they start from.
//!
//! Each source is a fixture of `fidelity`'s gen (real-format LEZ blocks, `T`, then three blocks
//! after it with their inscriptions). For each committee size `n` the parent state's key list is
//! replaced by `n` seeded test keys, and:
//! - `ra-<source>-n<n>` (rule (A)): `T`'s inscription is re-signed by key 0 and `t(n)` support
//!   inscriptions by keys 1..=t over the three blocks after `T`, in turn. Each is the source's own
//!   inscription transaction (made with `logos-blockchain-core`) with only the signer field and the
//!   signature changed, so every inscription check hashes the same bytes as in the source.
//! - `rb-<source>-n<n>` (rule (B)): keys 0..t certify seeded attestation keys, which attest final
//!   tips `T`, `T + 1`, `T + 2`, `T + 3`, `T`, … in turn; `finalMsgId` is the message id of the
//!   source's inscription of that block.
//! - `rb-tonly-n<n>` (rule (B)): `T` alone (the first source's one block up to `T`), every
//!   attestation on `T`.
//!
//! The rule (A) candidate journal and the rule (B) journal over the same blocks must be equal byte
//! for byte; the generator refuses to write fixtures otherwise.

use std::fs;
use std::path::Path;

use anyhow::{ensure, Context as _, Result};
use attestor_core::{sign_certificate, Deployment, FinalTip, SignedAttestation};
use checkpoint_core::hash::{blake2b256_parts, sha256_parts, threshold};
use checkpoint_core::input::{CheckpointInput, CheckpointInputB, Support};
use checkpoint_core::mantle::{inscription_msg_id, ElidedInscription, TX_HASH_PREFIX};
use checkpoint_core::rule::run_checkpoint;
use checkpoint_core::rule_b::run_checkpoint_b;
use checkpoint_core::state::key_set_hash;
use checkpoint_core::Hash;
use ed25519_dalek::{Signer as _, SigningKey};
use serde_json::{json, Value};

/// The specification's committees n = 3, 4, 7, 10 (t = 2, 3, 5, 7: `docs/design/bridge.md` §11.4
/// and the finality-rule tables), and n = 15 for t = 10, which none of its tables uses (15 is the
/// largest n with t(n) = 10).
pub const COMMITTEES: [u32; 5] = [3, 4, 7, 10, 15];
/// One empty `T` + 3 blocks; the idle hour; the hours with 5 and with 50 private transactions.
pub const SOURCES: [&str; 4] = ["cp-empty-1", "cp-empty-360", "cp-pp5-360", "cp-pp50-360"];

fn seeded(label: &str, i: u32) -> SigningKey {
    SigningKey::from_bytes(&sha256_parts(&[b"ferrata/rule-b/test-key/", label.as_bytes(), &i.to_be_bytes()]))
}

/// The source's inscription of block `index`: `T`'s or a support entry's.
fn source_inscription<'a>(src: &CheckpointInput<'a>, index: u32) -> Result<ElidedInscription<'a>> {
    if index == src.t_index {
        return Ok(src.producer);
    }
    src.support.iter().find(|s| s.block_index == index).map(|s| s.inscription).context("no inscription of that block")
}

/// The same inscription transaction with `key` as its signer, signed by `key`.
struct Resigned {
    prefix: Vec<u8>,
    suffix: Vec<u8>,
    signature: [u8; 64],
}

fn resign(src: &ElidedInscription<'_>, block: &[u8], key: &SigningKey) -> Resigned {
    let prefix = src.prefix.to_vec();
    let mut suffix = src.suffix.to_vec();
    // The suffix starts with the inscription op's parent MsgId(32) and signer(32).
    suffix[32..64].copy_from_slice(key.verifying_key().as_bytes());
    let tx_hash = blake2b256_parts(&[TX_HASH_PREFIX, &prefix, block, &suffix]);
    Resigned { prefix, suffix, signature: key.sign(&tx_hash).to_bytes() }
}

fn elided(r: &Resigned) -> ElidedInscription<'_> {
    ElidedInscription { prefix: &r.prefix, suffix: &r.suffix, signature: &r.signature }
}

fn write(dir: &Path, name: &str, input: &[u8], meta: &Value) -> Result<()> {
    fs::write(dir.join(format!("{name}.input")), input)?;
    fs::write(dir.join(format!("{name}.json")), serde_json::to_string_pretty(meta)? + "\n")?;
    println!("{name:<22} {:>9} B input  {}", input.len(), meta["description"].as_str().unwrap_or(""));
    Ok(())
}

fn block_bytes(blocks: &[&[u8]]) -> usize {
    blocks.iter().map(|b| b.len()).sum()
}

pub fn cmd_gen_b(dir: &Path) -> Result<()> {
    for source in SOURCES {
        let bytes = fs::read(dir.join(format!("{source}.input"))).with_context(|| format!("source fixture {source}"))?;
        let src = CheckpointInput::decode(&bytes).map_err(|e| anyhow::anyhow!("{source}: {e:?}"))?;
        ensure!(
            src.config_changes.is_empty() && src.segments.is_empty() && src.echo_nonce == [0; 32],
            "{source}: expected a plain candidate fixture"
        );
        let stem = source.strip_prefix("cp-").unwrap_or(source);
        let t_index = src.t_index;
        let after = u32::try_from(src.blocks.len())? - t_index; // T and the blocks after it
        let d = Deployment {
            evm_chain_id: src.config.evm_chain_id,
            vault: src.config.vault,
            lez_config_id: src.config.config_id(),
            channel_id: src.config.channel_id,
        };
        // Final tips T, T + 1, … with the message ids of the source's inscriptions of those blocks.
        let mut tips = Vec::new();
        for k in 0..after {
            let index = t_index + k;
            let block = src.blocks[usize::try_from(index)?];
            let ins = source_inscription(&src, index)?;
            let parent: Hash = ins.suffix[..32].try_into()?;
            let signer: Hash = ins.suffix[32..64].try_into()?;
            let id = u64::from_le_bytes(block[..8].try_into()?);
            let hash: Hash = block[40..72].try_into()?;
            tips.push(FinalTip {
                tip_id: id,
                tip_hash: hash,
                final_msg_id: inscription_msg_id(&src.config.channel_id, block, &parent, &signer),
            });
        }

        for n in COMMITTEES {
            let t = threshold(n);
            let channel: Vec<SigningKey> = (0..n).map(|i| seeded("channel", i)).collect();
            let attest: Vec<SigningKey> = (0..n).map(|i| seeded("attest", i)).collect();
            let mut parent = src.parent.clone();
            parent.keys = channel.iter().map(|k| k.verifying_key().to_bytes()).collect();
            let ksh = key_set_hash(&parent.keys, parent.posting_timeframe);

            // Rule (A): producer key 0, supporters 1..=t over the blocks after T in turn.
            let producer_block = src.blocks[usize::try_from(t_index)?];
            let producer = resign(&source_inscription(&src, t_index)?, producer_block, &channel[0]);
            let mut support = Vec::new();
            for j in 0..t {
                let index = t_index + 1 + j % (after - 1);
                let block = src.blocks[usize::try_from(index)?];
                let key = &channel[usize::try_from(j + 1)?];
                support.push((index, resign(&source_inscription(&src, index)?, block, key)));
            }
            let a = CheckpointInput {
                parent: parent.clone(),
                producer: elided(&producer),
                support: support.iter().map(|(i, r)| Support { block_index: *i, inscription: elided(r) }).collect(),
                ..src.clone()
            };
            let a_out = run_checkpoint(&a).map_err(|e| anyhow::anyhow!("ra-{stem}-n{n}: {e:?}"))?;

            // Rule (B): keys 0..t through their certified attestation keys, tips in turn.
            let attestations: Vec<SignedAttestation> = (0..t)
                .map(|i| {
                    let (ch, at) = (&channel[i as usize], &attest[i as usize]);
                    let cert = sign_certificate(ch, &d, &at.verifying_key().to_bytes());
                    SignedAttestation::new(at, ch.verifying_key().to_bytes(), cert, &d, &ksh, tips[(i % after) as usize])
                })
                .collect();
            let b = CheckpointInputB {
                config: src.config.clone(),
                parent: parent.clone(),
                config_changes: Vec::new(),
                blocks: src.blocks.clone(),
                t_index,
                attestations: attestations.clone(),
                segments: Vec::new(),
            };
            let b_out = run_checkpoint_b(&b).map_err(|e| anyhow::anyhow!("rb-{stem}-n{n}: {e:?}"))?;
            ensure!(a_out.journal.encode() == b_out.journal.encode(), "{stem} n = {n}: the two journals differ");

            let blocks = src.blocks.len();
            let common = |kind: &str, entries: u32, what: String| {
                json!({"kind": kind, "journal": hex::encode(a_out.journal.encode()), "source": source,
                       "blocks": blocks, "blocks_to_t": t_index + 1, "block_bytes": block_bytes(&src.blocks),
                       "block_bytes_to_t": block_bytes(&src.blocks[..=t_index as usize]),
                       "support_entries": entries, "n": n, "t": t, "description": what})
            };
            write(
                dir,
                &format!("ra-{stem}-n{n}"),
                &a.encode(),
                &common("checkpoint", t, format!("rule (A) over {source}'s blocks: T's inscription and {t} support inscriptions, n = {n}")),
            )?;
            write(
                dir,
                &format!("rb-{stem}-n{n}"),
                &b.encode(),
                &common("checkpoint-b", t, format!("rule (B) over {source}'s blocks: {t} certified attestations on T..T+{}, n = {n}", after - 1)),
            )?;

            if source == SOURCES[0] {
                // T alone: every attestation on T, no block after it.
                let on_t: Vec<SignedAttestation> = attestations
                    .iter()
                    .zip(&attest)
                    .map(|(x, at)| SignedAttestation::new(at, x.channel_key, x.certificate, &d, &ksh, tips[0]))
                    .collect();
                let only = CheckpointInputB {
                    blocks: src.blocks[..=t_index as usize].to_vec(),
                    attestations: on_t,
                    ..b.clone()
                };
                let out = run_checkpoint_b(&only).map_err(|e| anyhow::anyhow!("rb-tonly-n{n}: {e:?}"))?;
                ensure!(out.journal.encode() == a_out.journal.encode(), "tonly n = {n}: journal differs");
                let mut meta = common("checkpoint-b", t, format!("rule (B) over {source}'s T alone: {t} certified attestations on T, n = {n}"));
                meta["blocks"] = json!(t_index + 1);
                meta["block_bytes"] = json!(block_bytes(&src.blocks[..=t_index as usize]));
                write(dir, &format!("rb-tonly-n{n}"), &only.encode(), &meta)?;
            }
        }
    }
    Ok(())
}
