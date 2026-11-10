//! Component benchmarks: each mode does one kind of work `k` times on its input, so that the
//! difference between two runs is the marginal cost of that work. Modes (first input byte):
//!
//! 0 read only · 1 SHA-256 of the payload · 2 BLAKE2b-256 of the payload · 3 Ed25519
//! `verify_strict` of `(key, msg32, sig)` triples · 4 burn leaves + burn-tree appends (count in the payload) ·
//! 5 parse LEZ blocks (no hashing) · 6 hash parsed LEZ blocks · 7 inscription checks (BLAKE2b of
//! the whole transaction, Ed25519) of `(block, elided inscription)` pairs · 8 SHA-256 of the payload
//! through `risc0_zkvm::sha::Impl::hash_bytes` (the zkVM's own API) instead of RustCrypto `sha2` ·
//! 9 Ed25519 `verify_strict` of `(key, sig, u32 len, msg)` (variable-length messages).
#![no_main]

use checkpoint_core::hash::{blake2b256_parts, sha256};
use checkpoint_core::lez::{hash_parsed_block, parse_block, NoVisitor};
use checkpoint_core::mantle::{verify_strict, ElidedInscription};
use checkpoint_core::reader::Reader;
use checkpoint_core::tree::{burn_leaf, zero_hashes, Frontier};
use risc0_zkvm::guest::env;

risc0_zkvm::guest::entry!(main);

fn read_input() -> Vec<u8> {
    let mut len = [0u8; 4];
    env::read_slice(&mut len);
    let mut bytes = vec![0u8; u32::from_le_bytes(len) as usize];
    env::read_slice(&mut bytes);
    bytes
}

fn main() {
    let input = read_input();
    let (mode, payload) = input.split_first().expect("mode");
    let mut r = Reader::new(payload);
    let out: [u8; 32] = match mode {
        0 => [payload.len() as u8; 32],
        1 => sha256(payload),
        2 => blake2b256_parts(&[payload]),
        3 => {
            let mut n = 0u8;
            while !r.is_empty() {
                let key = r.array::<32>().unwrap();
                let msg = r.array::<32>().unwrap();
                let sig = r.array::<64>().unwrap();
                verify_strict(key, msg, sig).expect("signature");
                n += 1;
            }
            [n; 32]
        }
        4 => {
            let k = r.u64_le().unwrap();
            let mut f = Frontier::default();
            for i in 0..k {
                // What the rule does per burn: the leaf hash, then the append.
                f.append(burn_leaf(&[1; 32], i, &[2; 20], &[3; 32], &[4; 32], &[5; 32])).unwrap();
            }
            f.root(&zero_hashes())
        }
        5 | 6 => {
            let mut last = [0u8; 32];
            while !r.is_empty() {
                let block = r.bytes_u32("block").unwrap();
                let s = parse_block(block, &mut NoVisitor).expect("block");
                last = if *mode == 6 { hash_parsed_block(block).unwrap() } else { s.header.hash };
            }
            last
        }
        // SHA-256 through the zkVM's own API on the (aligned) input buffer, for comparison with
        // mode 1 (RustCrypto sha2, which the core uses).
        8 => {
            use risc0_zkvm::sha::{Impl, Sha256 as _};
            Impl::hash_bytes(payload).as_bytes().try_into().unwrap()
        }
        9 => {
            let mut n = 0u8;
            while !r.is_empty() {
                let key = r.array::<32>().unwrap();
                let sig = r.array::<64>().unwrap();
                let msg = r.bytes_u32("message").unwrap();
                verify_strict(key, msg, sig).expect("signature");
                n += 1;
            }
            [n; 32]
        }
        7 => {
            let mut last = [0u8; 32];
            let channel = *r.array::<32>().unwrap();
            while !r.is_empty() {
                let block = r.bytes_u32("block").unwrap();
                let e = ElidedInscription {
                    prefix: r.bytes_u32("prefix").unwrap(),
                    suffix: r.bytes_u32("suffix").unwrap(),
                    signature: r.array().unwrap(),
                };
                last = e.check(block, &channel).expect("inscription").tx_hash;
            }
            last
        }
        _ => panic!("unknown mode"),
    };
    env::commit_slice(&out);
}
