//! Rule (B) (`run_checkpoint_b`) on structured edits of a real input: the lab's recorded rule (B)
//! input over blocks 186..=202 (`ferrata_fuzz_programs::recorded_rule_b_input`), which the rule
//! accepts. The fuzz input is a list of 4-byte edits: `T`'s index; drop, swap or duplicate a block;
//! flip a bit of a block; re-seal a block (its stored hash set to its recomputed hash); flip a bit of
//! the producer inscription's prefix, suffix or signature; flip a bit of an attestation, drop or
//! duplicate one; change the parent's tip id, tip hash or a key; flip a bit of the channel id.
//!
//! Property, re-checked independently of the rule whenever it accepts: every block parses and
//! carries its own hash; the blocks chain from the parent's tip (hash and consecutive ids); the new
//! tip is `blocks[t_index]` (id and hash); `T`'s producer inscription checks on the configured
//! channel against `T`'s bytes, by a key of the parent's set (when no configuration change moves it).
#![no_main]

use std::sync::OnceLock;

use attestor_core::{SignedAttestation, SIGNED_ATTESTATION_LEN};
use checkpoint_core::input::CheckpointInputB;
use checkpoint_core::lez::{hash_parsed_block, parse_block, NoVisitor};
use checkpoint_core::mantle::ElidedInscription;
use checkpoint_core::rule_b::run_checkpoint_b;
use libfuzzer_sys::fuzz_target;

fn base() -> &'static [u8] {
    static BASE: OnceLock<Vec<u8>> = OnceLock::new();
    BASE.get_or_init(|| {
        let bytes = ferrata_fuzz_programs::recorded_rule_b_input();
        let input = CheckpointInputB::decode(&bytes).expect("the recorded input decodes");
        run_checkpoint_b(&input).expect("the rule accepts the recorded input");
        bytes
    })
}

fn flip(bytes: &mut [u8], at: usize, bit: u8) {
    if !bytes.is_empty() {
        let k = at % bytes.len();
        bytes[k] ^= 1 << (bit % 8);
    }
}

fuzz_target!(|data: &[u8]| {
    let base = CheckpointInputB::decode(base()).unwrap();
    let mut blocks: Vec<Vec<u8>> = base.blocks.iter().map(|b| b.to_vec()).collect();
    let mut t_index = base.t_index;
    let (mut prefix, mut suffix, mut signature) =
        (base.producer.prefix.to_vec(), base.producer.suffix.to_vec(), *base.producer.signature);
    let mut attestations: Vec<[u8; SIGNED_ATTESTATION_LEN]> = base.attestations.iter().map(|a| a.encode()).collect();
    let (mut parent, mut config) = (base.parent.clone(), base.config.clone());
    for edit in data.chunks_exact(4).take(8) {
        let (op, a, b, c) = (edit[0], usize::from(edit[1]), usize::from(edit[2]), edit[3]);
        let wide = b << 8 | usize::from(c);
        let n = blocks.len();
        match op % 13 {
            0 => t_index = (a % (n + 2)) as u32,
            1 if n > 1 => {
                blocks.remove(a % n);
            }
            2 => blocks.swap(a % n, b % n),
            3 => {
                let copy = blocks[a % n].clone();
                blocks.insert(b % (n + 1), copy);
            }
            4 => flip(&mut blocks[a % n], wide, c),
            5 => {
                let i = a % n;
                if let Ok(h) = hash_parsed_block(&blocks[i]) {
                    blocks[i][40..72].copy_from_slice(&h);
                }
            }
            6 => flip(&mut prefix, wide, c),
            7 => flip(&mut suffix, wide, c),
            8 => flip(&mut signature, b, c),
            9 if !attestations.is_empty() => {
                let i = a % attestations.len();
                flip(&mut attestations[i], wide, c);
            }
            10 if !attestations.is_empty() => {
                if a & 1 == 0 {
                    attestations.remove(b % attestations.len());
                } else {
                    let copy = attestations[b % attestations.len()];
                    attestations.push(copy);
                }
            }
            11 => match a % 3 {
                0 => parent.tip_id = parent.tip_id.wrapping_add(b as u64).wrapping_sub(u64::from(c)),
                1 => flip(&mut parent.tip_hash, b, c),
                _ => {
                    let k = parent.keys.len();
                    if k > 0 {
                        flip(&mut parent.keys[b % k], usize::from(c), c);
                    }
                }
            },
            12 => flip(&mut config.channel_id, b, c),
            _ => {}
        }
    }
    let input = CheckpointInputB {
        config,
        parent,
        config_changes: base.config_changes.clone(),
        blocks: blocks.iter().map(Vec::as_slice).collect(),
        t_index,
        producer: ElidedInscription { prefix: &prefix, suffix: &suffix, signature: &signature },
        attestations: attestations.iter().map(SignedAttestation::decode).collect(),
        segments: base.segments.clone(),
    };
    let Ok(out) = run_checkpoint_b(&input) else { return };

    let (mut id, mut hash) = (input.parent.tip_id, input.parent.tip_hash);
    for (i, block) in input.blocks.iter().enumerate() {
        let s =
            parse_block(block, &mut NoVisitor).unwrap_or_else(|e| panic!("accepted: block {i} does not parse: {e:?}"));
        assert_eq!(hash_parsed_block(block).unwrap(), s.header.hash, "accepted: block {i} does not carry its hash");
        assert_eq!(s.header.prev_block_hash, hash, "accepted: block {i} does not chain");
        assert_eq!(Some(s.header.block_id), id.checked_add(1), "accepted: block {i}'s id does not follow");
        (id, hash) = (s.header.block_id, s.header.hash);
    }
    let t = *input.blocks.get(input.t_index as usize).expect("accepted: T is one of the blocks");
    let ts = parse_block(t, &mut NoVisitor).unwrap();
    assert_eq!(out.new_state.tip_id, ts.header.block_id, "accepted: another tip id");
    assert_eq!(out.new_state.tip_hash, ts.header.hash, "accepted: another tip hash");
    let checked = input
        .producer
        .check(t, &input.config.channel_id)
        .unwrap_or_else(|e| panic!("accepted: T's inscription does not check on the channel: {e:?}"));
    if input.config_changes.is_empty() {
        assert!(input.parent.keys.contains(&checked.signer), "accepted: T inscribed by a key outside the set");
    }
});
