//! Bench guest (experimental, unaudited): one whole light-client update in one execution, as
//! `guests/residue-accel` runs it (decode the compact store, the update and the signer witness;
//! step 1; the two-pair Miller loop; the final check; the adoption in one part; the job's
//! encoding; apply), with the signing key composed either by the core (`steps::step1`: a Merkle
//! branch per non-signer) or by `residue_agg` (one multiproof for all non-signers, the compressed
//! key read off its bytes, blst's batch addition); and the profile of what a non-signer costs. No
//! deployed program runs this code. Every number is a delta of `env::cycle_count()` (user cycles).
//!
//! Modes: 0 one update (`one_tx`); 3 the profile of the non-signers' work (`profile`). In mode 0 a
//! period update given no adoption keys adopts the next committee by its summary
//! (`residue_handoff::adopt_summary`, bench/RESIDUE.md §10).

use std::hint::black_box;

use blst::{
    blst_fp, blst_fp_from_bendian, blst_fp_inverse, blst_fp_mul, blst_p1, blst_p1_add_or_double,
    blst_p1_add_or_double_affine, blst_p1_affine, blst_p1_affine_compress, blst_p1_affine_in_g1, blst_p1_affine_is_inf,
    blst_p1_affine_on_curve, blst_p1_cneg, blst_p1_deserialize, blst_p1_from_affine, blst_p1_is_inf,
    blst_p1_uncompress, blst_p1s_add, blst_p2_affine, blst_p2_deserialize, BLST_ERROR,
};
use ferrata_core::{
    fork::{ChainSpec, Preset, FAR_FUTURE_EPOCH},
    hash::hash_pair,
    merkle::is_valid_merkle_branch,
    steps::{self, Phase, StepError},
    witness::{
        key_branch_depth, pubkey_leaf, CommitteeSummary, CompactStore, CompactUpdate, SignerWitness, UncompressedPubkey,
    },
    Error, Root,
};
use residue_agg::{NonSigners, Sum};
use risc0_zkvm::guest::env;

const MODE_ONE_TX: u32 = 0;
const MODE_PROFILE: u32 = 3;

/// Mainnet's `genesis_validators_root`.
const GVR: &str = "4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";

struct Rec(Vec<(String, u64)>);

impl Rec {
    fn time<T>(&mut self, label: &str, f: impl FnOnce() -> T) -> T {
        let start = env::cycle_count();
        let out = black_box(f());
        let end = env::cycle_count();
        self.0.push((label.to_string(), end - start));
        out
    }
    fn note(&mut self, label: &str, value: u64) {
        self.0.push((label.to_string(), value));
    }
}

fn read_blob() -> Vec<u8> {
    let len: u32 = env::read();
    let mut buf = vec![0u8; (len as usize + 3) & !3];
    env::read_slice(&mut buf);
    buf.truncate(len as usize);
    buf
}

fn h32(s: &str) -> Root {
    let b = s.as_bytes();
    let nib = |c: u8| match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("bad hex"),
    };
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        *o = (nib(b[2 * i]) << 4) | nib(b[2 * i + 1]);
    }
    out
}

fn mainnet() -> ChainSpec {
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [
            [0, 0, 0, 0],
            [1, 0, 0, 0],
            [2, 0, 0, 0],
            [3, 0, 0, 0],
            [4, 0, 0, 0],
            [5, 0, 0, 0],
            [6, 0, 0, 0],
            [7, 0, 0, 0],
            [8, 0, 0, 0],
        ],
        epochs: [0, 74_240, 144_896, 194_048, 269_568, 364_032, 411_392, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    }
}

fn main() {
    let mode: u32 = env::read();
    let mut rec = Rec(Vec::new());
    let extra = match mode {
        MODE_ONE_TX => one_tx(&mut rec),
        MODE_PROFILE => profile(&mut rec),
        _ => panic!("unknown mode {mode}"),
    };
    env::commit(&(rec.0, extra));
}

/// The refusal of a witness, as a bench-only code (not a program code).
fn refusal_code(r: residue::Refusal) -> u16 {
    240 + r as u16
}

/// One update. Inputs: path (0 current, 1 witness, 2 witness folded into the Miller loop),
/// aggregation (0 the core's `step1` with a `SignerWitness`; 1 `residue_agg::step1` with a
/// `NonSigners` witness, batch addition; 2 the same with one mixed addition per non-signer), the
/// compact store, the compact update, the signer witness in that aggregation's encoding, the
/// adoption keys (uncompressed, 96 bytes each; empty if none), c, w, c^-1 (576 bytes each; empty
/// where the path takes none). Commits the new store's encoding, or `[0xff, code]`.
fn one_tx(rec: &mut Rec) -> Vec<u8> {
    let t0 = env::cycle_count();
    let path: u32 = env::read();
    let aggregation: u32 = env::read();
    let (store_b, update_b, signers_b, keys_b, c_b, w_b, c_inv_b) =
        (read_blob(), read_blob(), read_blob(), read_blob(), read_blob(), read_blob(), read_blob());
    let t1 = env::cycle_count();
    rec.note("R  read the inputs", t1 - t0);
    rec.note("info: signer witness bytes", signers_b.len() as u64);
    rec.note(
        "info: input bytes (store, update, signer witness, adoption keys, c, w, c^-1)",
        (store_b.len() + update_b.len() + signers_b.len() + keys_b.len() + c_b.len() + w_b.len() + c_inv_b.len())
            as u64,
    );
    let result = verify(rec, path, aggregation, &store_b, &update_b, &signers_b, &keys_b, &c_b, &w_b, &c_inv_b);
    let t2 = env::cycle_count();
    rec.note("T  whole update: inputs read to new store (one cycle_count delta)", t2 - t0);
    match result {
        Ok(store) => store,
        Err(code) => {
            rec.note("refused: code", code as u64);
            vec![0xff, code as u8]
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn verify(
    rec: &mut Rec,
    path: u32,
    aggregation: u32,
    store_b: &[u8],
    update_b: &[u8],
    signers_b: &[u8],
    keys_b: &[u8],
    c_b: &[u8],
    w_b: &[u8],
    c_inv_b: &[u8],
) -> Result<Vec<u8>, u16> {
    let core = |e: Error| e.code();
    let step = |e: StepError| e.code();
    let spec = mainnet();
    let gvr = h32(GVR);
    let n = 512;
    let stored = rec.time("D1 decode the compact store", || CompactStore::decode(store_b)).map_err(core)?;
    let (update, store) = rec
        .time("D2 decode the compact update, store_for_update", || -> Result<(CompactUpdate, CompactStore), Error> {
            let layout = CompactUpdate::layout_of(update_b)?;
            let update = CompactUpdate::decode(update_b, layout, n)?;
            let store = steps::store_for_update(&stored, layout, update.attested_header.beacon.slot, &spec)?;
            Ok((update, store))
        })
        .map_err(core)?;
    let mut job = if aggregation == 0 {
        let signers = rec
            .time("D3 decode the signer witness (SignerWitness)", || SignerWitness::decode(signers_b, n))
            .map_err(core)?;
        rec.note("info: non-signers", signers.keys.len() as u64);
        rec.time("S1 step 1: policy, validation, signing key (non-signers' keys and branches), H(m)", || {
            steps::step1(&store, &update, &signers, update.signature_slot, &spec, &gvr, 1)
        })
        .map_err(step)?
    } else {
        let sum = if aggregation == 1 { Sum::Batch } else { Sum::Chain };
        let signers =
            rec.time("D3 decode the signer witness (NonSigners)", || NonSigners::decode(signers_b, n)).map_err(core)?;
        rec.note("info: non-signers", signers.keys.len() as u64);
        rec.note("info: multiproof siblings", signers.siblings.len() as u64);
        rec.time("S1 step 1: policy, validation, signing key (non-signers' keys, one multiproof), H(m)", || {
            residue_agg::step1(&store, &update, &signers, update.signature_slot, &spec, &gvr, 1, sum)
        })
        .map_err(step)?
    };
    if path == 2 {
        rec.time(
            "S23f Miller loop with c^-1 folded in (blst lines, own loop) and the folded witness check",
            || -> Result<(), u16> {
                let Phase::Miller { hm, neg_key, signature } = &job.phase else { return Err(StepError::Phase.code()) };
                let (mut q1, mut p1, mut q2) =
                    (blst_p2_affine::default(), blst_p1_affine::default(), blst_p2_affine::default());
                // SAFETY: the 192- and 96-byte serializations step 1 wrote; valid destinations.
                let ok = unsafe {
                    blst_p2_deserialize(&mut q1, hm.as_ptr()) == BLST_ERROR::BLST_SUCCESS
                        && blst_p1_deserialize(&mut p1, neg_key.as_ptr()) == BLST_ERROR::BLST_SUCCESS
                        && blst_p2_deserialize(&mut q2, signature.as_ptr()) == BLST_ERROR::BLST_SUCCESS
                };
                if !ok {
                    return Err(Error::Malformed.code());
                }
                let fp12 = |b: &[u8]| -> Result<blst::blst_fp12, u16> {
                    let a: &[u8; residue::FP12_BYTES] = b.try_into().map_err(|_| Error::Malformed.code())?;
                    residue::decode_fp12(a).ok_or(refusal_code(residue::Refusal::NonCanonical))
                };
                let (c, w, c_inv) = (fp12(c_b)?, fp12(w_b)?, fp12(c_inv_b)?);
                let (l1, l2) = (residue::fold::precompute_lines(&q1), residue::fold::precompute_lines(&q2));
                residue::fold::check_folded(&l1, &p1, &l2, &c, &c_inv, &w).map_err(refusal_code)?;
                job.phase = Phase::Verified;
                Ok(())
            },
        )?;
    } else {
        rec.time("S2 step 2: two-pair Miller loop", || job.step2()).map_err(step)?;
    }
    if path == 1 {
        rec.time(
            "S3w witness check: decode c and w (canonical), c != 0, w in Fp6, f*w == c^lambda",
            || -> Result<(), u16> {
                let Phase::FinalExp { product } = &job.phase else { return Err(StepError::Phase.code()) };
                let f = residue::decode_fp12(product).ok_or(Error::Malformed.code())?;
                let c: &[u8; residue::FP12_BYTES] = c_b.try_into().map_err(|_| Error::Malformed.code())?;
                let w: &[u8; residue::FP12_BYTES] = w_b.try_into().map_err(|_| Error::Malformed.code())?;
                residue::check_bytes(&f, c, w).map_err(refusal_code)?;
                job.phase = Phase::Verified;
                Ok(())
            },
        )?;
    } else if path == 0 {
        rec.time("S3 step 3: final exponentiation and check (Blst::final_half)", || job.step3()).map_err(step)?;
    }
    if job.adoption_pending() && keys_b.is_empty() {
        // EXPERIMENTAL (bench/RESIDUE.md §10; sound only under the light client's honest-supermajority
        // assumption): no keys given, the next committee is adopted by its summary as step 1 proved it.
        rec.time("A2 adoption by the summary (no keys; residue_handoff::adopt_summary)", || {
            residue_handoff::adopt_summary(&mut job)
        })
        .map_err(step)?;
    } else if job.adoption_pending() {
        let keys: Vec<UncompressedPubkey> = rec.time("A0 the adoption keys into 96-byte arrays", || {
            keys_b.chunks_exact(96).map(|c| c.try_into().unwrap()).collect()
        });
        rec.time("A1 adoption: 512 keys in one part (on curve, not identity, key root, sum)", || {
            job.adopt(0, &keys, n)
        })
        .map_err(step)?;
    }
    let encoded = rec.time("J  encode the job (VerifyAll hands it to the instance)", || job.encode());
    rec.note("info: job bytes", encoded.len() as u64);
    let (next, outcome) = rec
        .time("P  apply (the instance's ApplyVerified, without its decode)", || steps::apply(&stored, &job, &spec))
        .map_err(step)?;
    rec.note("info: adopted a committee (1 = yes)", u64::from(outcome.adopted));
    Ok(next.encode())
}

// ---------------------------------------------------------------------------------------------
// The profile
// ---------------------------------------------------------------------------------------------

/// The core's `witness_key` (witness.rs, `blst_witness`), copied: flags, decode, not identity,
/// re-compressed.
fn core_witness_key(bytes: &UncompressedPubkey) -> Result<(blst_p1_affine, [u8; 48]), ()> {
    if bytes[0] & 0x80 != 0 {
        return Err(());
    }
    let mut point = blst_p1_affine::default();
    // SAFETY: valid destination; 96 bytes.
    if unsafe { blst_p1_deserialize(&mut point, bytes.as_ptr()) } != BLST_ERROR::BLST_SUCCESS {
        return Err(());
    }
    // SAFETY: valid point.
    if unsafe { blst_p1_affine_is_inf(&point) } {
        return Err(());
    }
    let mut compressed = [0u8; 48];
    // SAFETY: 48 bytes written.
    unsafe { blst_p1_affine_compress(compressed.as_mut_ptr(), &point) };
    Ok((point, compressed))
}

/// The core's `signing_key` (steps.rs:632-666 at 510f05cf), copied, so that its parts can be timed.
fn core_signing_key(committee: &CommitteeSummary, bits: &[bool], witness: &SignerWitness) -> Result<blst_p1, ()> {
    let depth = key_branch_depth(bits.len());
    let absent = bits.iter().enumerate().filter(|(_, b)| !**b).map(|(i, _)| i);
    if witness.keys.len() != bits.iter().filter(|b| !**b).count() || witness.branches.len() != witness.keys.len() {
        return Err(());
    }
    let mut aggregate = blst_p1_affine::default();
    // SAFETY: 48 bytes read.
    if unsafe { blst_p1_uncompress(&mut aggregate, committee.aggregate_pubkey.as_ptr()) } != BLST_ERROR::BLST_SUCCESS {
        return Err(());
    }
    let mut absent_sum = blst_p1::default();
    for ((index, key), branch) in absent.zip(&witness.keys).zip(&witness.branches) {
        let (point, compressed) = core_witness_key(key)?;
        if branch.len() != depth
            || !is_valid_merkle_branch(&pubkey_leaf(&compressed), branch, depth, index as u64, &committee.pubkeys_root)
        {
            return Err(());
        }
        let acc: *mut blst_p1 = &mut absent_sum;
        // SAFETY: aliasing allowed.
        unsafe { blst_p1_add_or_double_affine(acc, acc, &point) };
    }
    let mut key = blst_p1::default();
    let key_ptr: *mut blst_p1 = &mut key;
    // SAFETY: valid points.
    unsafe {
        blst_p1_from_affine(key_ptr, &aggregate);
        blst_p1_cneg(&mut absent_sum, true);
        blst_p1_add_or_double(key_ptr, key_ptr, &absent_sum);
    }
    // SAFETY: valid point.
    if unsafe { blst_p1_is_inf(&key) } {
        return Err(());
    }
    Ok(key)
}

/// The non-signers' work, by part. Inputs: the committee summary (80 bytes), the bits (one byte
/// each, 0 or 1), the core's signer witness, the `NonSigners` witness. Each part runs over every
/// non-signer in a loop of its own; the three whole compositions must give the same key, and the
/// candidates' parts the same bytes as the core's (asserted: a mismatch panics).
fn profile(rec: &mut Rec) -> Vec<u8> {
    let (summary_b, bits_b, old_b, new_b) = (read_blob(), read_blob(), read_blob(), read_blob());
    let committee = CommitteeSummary::decode(&summary_b).expect("summary");
    let bits: Vec<bool> = bits_b.iter().map(|b| *b == 1).collect();
    let n = bits.len();
    let depth = key_branch_depth(n);
    let old = rec
        .time("D3 decode the core's witness (SignerWitness::decode)", || SignerWitness::decode(&old_b, n))
        .expect("old witness");
    let new = rec
        .time("D3n decode the multiproof witness (NonSigners::decode)", || NonSigners::decode(&new_b, n))
        .expect("new witness");
    let k = old.keys.len();
    rec.note("info: non-signers", k as u64);
    rec.note("info: core witness bytes", old_b.len() as u64);
    rec.note("info: multiproof witness bytes", new_b.len() as u64);
    rec.note("info: multiproof siblings", new.siblings.len() as u64);

    // Whole compositions.
    let a = rec
        .time("K0 the core's composition (copy of steps::signing_key)", || core_signing_key(&committee, &bits, &old));
    let b = rec.time("K1 residue_agg::signing_key, batch addition", || {
        residue_agg::signing_key(&committee, &bits, &new, Sum::Batch)
    });
    let c = rec.time("K2 residue_agg::signing_key, one mixed addition per non-signer", || {
        residue_agg::signing_key(&committee, &bits, &new, Sum::Chain)
    });
    let (a, b, c) = (a.expect("core composition"), b.expect("batch composition"), c.expect("chain composition"));
    assert_eq!(residue_agg::neg_key_bytes(&a), residue_agg::neg_key_bytes(&b), "batch key = core key");
    assert_eq!(residue_agg::neg_key_bytes(&a), residue_agg::neg_key_bytes(&c), "chain key = core key");

    // The core's composition, part by part.
    let absent: Vec<usize> = rec.time("P1 bookkeeping: the absent indices, the count and branch-length checks", || {
        let absent: Vec<usize> = bits.iter().enumerate().filter(|(_, b)| !**b).map(|(i, _)| i).collect();
        assert!(absent.len() == old.keys.len() && old.branches.iter().all(|br| br.len() == depth));
        absent
    });
    let points: Vec<blst_p1_affine> =
        rec.time("P2 decode: blst_p1_deserialize (flags, canonical, to Montgomery form, on the curve, x != 0)", || {
            old.keys
                .iter()
                .map(|key| {
                    let mut p = blst_p1_affine::default();
                    // SAFETY: 96 bytes read.
                    assert_eq!(unsafe { blst_p1_deserialize(&mut p, key.as_ptr()) }, BLST_ERROR::BLST_SUCCESS);
                    p
                })
                .collect()
        });
    rec.time("P2a  of which: x and y to Montgomery form (2 x blst_fp_from_bendian, no range check)", || {
        for key in &old.keys {
            let (mut x, mut y) = (blst_fp::default(), blst_fp::default());
            // SAFETY: 48 bytes read each.
            unsafe {
                blst_fp_from_bendian(&mut x, key.as_ptr());
                blst_fp_from_bendian(&mut y, key[48..].as_ptr());
            }
            black_box((x, y));
        }
    });
    rec.time("P2b  of which: the on-curve check (blst_p1_affine_on_curve)", || {
        for p in &points {
            // SAFETY: valid point.
            assert!(unsafe { blst_p1_affine_on_curve(p) });
        }
    });
    rec.time("P3 not the identity (blst_p1_affine_is_inf)", || {
        for p in &points {
            // SAFETY: valid point.
            assert!(!unsafe { blst_p1_affine_is_inf(p) });
        }
    });
    let compressed: Vec<[u8; 48]> =
        rec.time("P4 re-compress (blst_p1_affine_compress: x out of Montgomery form, sign of y)", || {
            points
                .iter()
                .map(|p| {
                    let mut out = [0u8; 48];
                    // SAFETY: 48 bytes written.
                    unsafe { blst_p1_affine_compress(out.as_mut_ptr(), p) };
                    out
                })
                .collect()
        });
    let leaves: Vec<Root> = rec.time("P5 the leaf: pubkey_leaf (SHA-256 of 64 bytes, sha2's streaming API)", || {
        compressed.iter().map(pubkey_leaf).collect()
    });
    rec.time("P6 the branch: is_valid_merkle_branch, depth 9 (9 x hash_pair, sha2's streaming API)", || {
        for ((leaf, branch), &index) in leaves.iter().zip(&old.branches).zip(&absent) {
            assert!(is_valid_merkle_branch(leaf, branch, depth, index as u64, &committee.pubkeys_root));
        }
    });
    let chain = rec.time("P7 the sum: one blst_p1_add_or_double_affine per non-signer", || {
        let mut acc = blst_p1::default();
        for p in &points {
            let a: *mut blst_p1 = &mut acc;
            // SAFETY: aliasing allowed.
            unsafe { blst_p1_add_or_double_affine(a, a, p) };
        }
        acc
    });
    rec.time("P8 once: the aggregate decompressed, the sum subtracted, the identity check", || {
        let mut aggregate = blst_p1_affine::default();
        let mut sum = chain;
        let mut key = blst_p1::default();
        let key_ptr: *mut blst_p1 = &mut key;
        // SAFETY: valid inputs.
        unsafe {
            assert_eq!(
                blst_p1_uncompress(&mut aggregate, committee.aggregate_pubkey.as_ptr()),
                BLST_ERROR::BLST_SUCCESS
            );
            blst_p1_from_affine(key_ptr, &aggregate);
            blst_p1_cneg(&mut sum, true);
            blst_p1_add_or_double(key_ptr, key_ptr, &sum);
            assert!(!blst_p1_is_inf(&key));
        }
        key
    });
    rec.time("P9 (not in the core) a subgroup check of each key, blst_p1_affine_in_g1, for comparison", || {
        for p in &points {
            // SAFETY: valid point.
            black_box(unsafe { blst_p1_affine_in_g1(p) });
        }
    });

    // The candidates' parts.
    let mine: Vec<[u8; 48]> = rec.time("C1 the compressed key from the bytes (residue_agg::compressed)", || {
        old.keys.iter().map(residue_agg::compressed).collect()
    });
    assert_eq!(mine, compressed, "compressed() = blst_p1_affine_compress on every key");
    let mine_leaves: Vec<Root> = rec
        .time("C2 the leaf by two compressions (residue_agg::leaf)", || mine.iter().map(residue_agg::leaf).collect());
    assert_eq!(mine_leaves, leaves, "leaf() = pubkey_leaf on every key");
    let indexed: Vec<(u32, Root)> = absent.iter().map(|&i| i as u32).zip(mine_leaves.iter().copied()).collect();
    let root = rec.time("C3 one multiproof of all the leaves (residue_agg::multiproof_root)", || {
        residue_agg::multiproof_root(&indexed, &new.siblings, depth)
    });
    assert_eq!(root, Some(committee.pubkeys_root), "the multiproof gives the key root");
    rec.time(
        "C3b the branches by two-compression hashes (9 x residue_agg::node per non-signer; for comparison)",
        || {
            for ((leaf, branch), &index) in mine_leaves.iter().zip(&old.branches).zip(&absent) {
                let mut v = *leaf;
                for (i, s) in branch.iter().enumerate() {
                    v = if (index >> i) & 1 == 1 { residue_agg::node(s, &v) } else { residue_agg::node(&v, s) };
                }
                assert_eq!(v, committee.pubkeys_root);
            }
        },
    );
    let batch = rec.time("C4 the sum by blst_p1s_add (batch affine addition)", || {
        let mut acc = blst_p1::default();
        if !points.is_empty() {
            let ptrs: [*const blst_p1_affine; 2] = [points.as_ptr(), core::ptr::null()];
            // SAFETY: contiguous valid points.
            unsafe { blst_p1s_add(&mut acc, ptrs.as_ptr(), points.len()) };
        }
        acc
    });
    let (mut x, mut y) = (blst_p1_affine::default(), blst_p1_affine::default());
    // SAFETY: valid points.
    unsafe {
        blst::blst_p1_to_affine(&mut x, &chain);
        blst::blst_p1_to_affine(&mut y, &batch);
        assert!(blst::blst_p1_affine_is_equal(&x, &y), "batch sum = chain sum");
    }
    rec.time("C5 key_point on every key (flag check, blst_p1_deserialize, not identity)", || {
        for key in &old.keys {
            black_box(residue_agg::key_point(key).expect("key"));
        }
    });

    // Unit costs, 1,000 each.
    let (l, r) = (leaves.first().copied().unwrap_or([1; 32]), committee.pubkeys_root);
    rec.time("U1 1,000 x hash_pair (sha2's streaming API, as the core)", || {
        let mut v = l;
        for _ in 0..1000 {
            v = hash_pair(&v, &r);
        }
        v
    });
    rec.time("U2 1,000 x residue_agg::node (two compressions)", || {
        let mut v = l;
        for _ in 0..1000 {
            v = residue_agg::node(&v, &r);
        }
        v
    });
    let mut s = l;
    for _ in 0..1000 {
        s = hash_pair(&s, &r);
    }
    let mut t = l;
    for _ in 0..1000 {
        t = residue_agg::node(&t, &r);
    }
    assert_eq!(s, t, "node() = hash_pair on a chain of 1,000");
    let (mut fa, mut fb) = (blst_fp::default(), blst_fp::default());
    // SAFETY: 48 bytes read each.
    unsafe {
        blst_fp_from_bendian(&mut fa, old.keys.first().map_or([7u8; 96], |k| *k).as_ptr());
        blst_fp_from_bendian(&mut fb, committee.aggregate_pubkey.as_ptr());
    }
    rec.time("U3 1,000 x blst_fp_mul (Montgomery multiplication)", || {
        let mut v = fa;
        for _ in 0..1000 {
            let p: *mut blst_fp = &mut v;
            // SAFETY: aliasing allowed.
            unsafe { blst_fp_mul(p, p, &fb) };
        }
        v
    });
    rec.time("U4 1,000 x blst_fp_inverse", || {
        let mut v = fa;
        for _ in 0..1000 {
            let p: *mut blst_fp = &mut v;
            // SAFETY: aliasing allowed.
            unsafe { blst_fp_inverse(p, p) };
        }
        v
    });
    rec.time("U5 1,000 x blst_p1_add_or_double_affine", || {
        let mut acc = blst_p1::default();
        let p = points.first().copied().unwrap_or_default();
        for _ in 0..1000 {
            let a: *mut blst_p1 = &mut acc;
            // SAFETY: aliasing allowed.
            unsafe { blst_p1_add_or_double_affine(a, a, &p) };
        }
        acc
    });
    Vec::new()
}
