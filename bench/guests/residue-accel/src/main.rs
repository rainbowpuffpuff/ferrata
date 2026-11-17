//! Bench guest (experimental, unaudited): one whole light-client update in one execution, as the
//! LEZ program's `VerifyAll` runs it (decode the compact store, the update and the signer witness;
//! step 1; the two-pair Miller loop; the final check; the committee adoption in one part; the
//! job's encoding; apply), with either the final exponentiation (`Job::step3`, the current path)
//! or the residue-witness check (`residue::check_bytes`, the witness path). No deployed program
//! runs this code. Every number is a delta of `env::cycle_count()` (user cycles).
//!
//! Modes: 0 one update (`one_tx`); 1 a batch of (f, c, w) checked by both paths (test (d));
//! 2 the cost of the Fp12 operations (`ops`).

use std::hint::black_box;

use blst::{blst_fp12, blst_fp12_cyclotomic_sqr};
use ferrata_core::{
    fork::{ChainSpec, Preset, FAR_FUTURE_EPOCH},
    steps::{self, Phase, StepError},
    witness::{CompactStore, CompactUpdate, SignerWitness, UncompressedPubkey},
    Error, Root,
};
use risc0_zkvm::guest::env;

const MODE_ONE_TX: u32 = 0;
const MODE_BATCH: u32 = 1;
const MODE_OPS: u32 = 2;

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
        versions: [[0, 0, 0, 0], [1, 0, 0, 0], [2, 0, 0, 0], [3, 0, 0, 0], [4, 0, 0, 0], [5, 0, 0, 0], [6, 0, 0, 0], [7, 0, 0, 0], [8, 0, 0, 0]],
        epochs: [0, 74_240, 144_896, 194_048, 269_568, 364_032, 411_392, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    }
}

fn main() {
    let mode: u32 = env::read();
    let mut rec = Rec(Vec::new());
    let extra = match mode {
        MODE_ONE_TX => one_tx(&mut rec),
        MODE_BATCH => batch(&mut rec),
        MODE_OPS => ops(&mut rec),
        _ => panic!("unknown mode {mode}"),
    };
    env::commit(&(rec.0, extra));
}

/// The refusal of a witness, as a bench-only code (not a program code).
fn refusal_code(r: residue::Refusal) -> u16 {
    240 + r as u16
}

/// One update. Inputs: path (0 current, 1 witness), the compact store, the compact update, the
/// signer witness, the adoption keys (uncompressed, 96 bytes each; empty if none), c, w (576
/// bytes each; empty on the current path). Commits the new store's encoding, or `[0xff, code]`.
fn one_tx(rec: &mut Rec) -> Vec<u8> {
    let t0 = env::cycle_count();
    let witness_path: u32 = env::read();
    let (store_b, update_b, signers_b, keys_b, c_b, w_b) =
        (read_blob(), read_blob(), read_blob(), read_blob(), read_blob(), read_blob());
    let t1 = env::cycle_count();
    rec.note("R  read the inputs", t1 - t0);
    rec.note("info: input bytes (store, update, signer witness, adoption keys, c, w)", (store_b.len() + update_b.len() + signers_b.len() + keys_b.len() + c_b.len() + w_b.len()) as u64);
    let result = verify(rec, witness_path == 1, &store_b, &update_b, &signers_b, &keys_b, &c_b, &w_b);
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
    witness_path: bool,
    store_b: &[u8],
    update_b: &[u8],
    signers_b: &[u8],
    keys_b: &[u8],
    c_b: &[u8],
    w_b: &[u8],
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
    let signers = rec.time("D3 decode the signer witness", || SignerWitness::decode(signers_b, n)).map_err(core)?;
    rec.note("info: non-signers", signers.keys.len() as u64);
    let mut job = rec
        .time("S1 step 1: policy, validation, signing key (non-signers' keys and branches), H(m)", || {
            steps::step1(&store, &update, &signers, update.signature_slot, &spec, &gvr, 1)
        })
        .map_err(step)?;
    rec.time("S2 step 2: two-pair Miller loop", || job.step2()).map_err(step)?;
    if witness_path {
        rec.time("S3w witness check: decode c and w (canonical), c != 0, w in Fp6, f*w == c^lambda", || -> Result<(), u16> {
            let Phase::FinalExp { product } = &job.phase else { return Err(StepError::Phase.code()) };
            let f = residue::decode_fp12(product).ok_or(Error::Malformed.code())?;
            let c: &[u8; residue::FP12_BYTES] = c_b.try_into().map_err(|_| Error::Malformed.code())?;
            let w: &[u8; residue::FP12_BYTES] = w_b.try_into().map_err(|_| Error::Malformed.code())?;
            residue::check_bytes(&f, c, w).map_err(refusal_code)?;
            job.phase = Phase::Verified;
            Ok(())
        })?;
    } else {
        rec.time("S3 step 3: final exponentiation and check (Blst::final_half)", || job.step3()).map_err(step)?;
    }
    if job.adoption_pending() {
        let keys: Vec<UncompressedPubkey> = rec.time("A0 the adoption keys into 96-byte arrays", || {
            keys_b.chunks_exact(96).map(|c| c.try_into().unwrap()).collect()
        });
        rec.time("A1 adoption: 512 keys in one part (on curve, not identity, key root, sum)", || job.adopt(0, &keys, n))
            .map_err(step)?;
    }
    let encoded = rec.time("J  encode the job (VerifyAll hands it to the instance)", || job.encode());
    rec.note("info: job bytes", encoded.len() as u64);
    let (next, outcome) = rec.time("P  apply (the instance's ApplyVerified, without its decode)", || {
        steps::apply(&stored, &job, &spec)
    })
    .map_err(step)?;
    rec.note("info: adopted a committee (1 = yes)", u64::from(outcome.adopted));
    Ok(next.encode())
}

/// Test (d): for each record (f, c, w, expected), the witness check and the final exponentiation
/// in the guest. Commits one byte per record: bit 0 the witness check's verdict, bit 1 the final
/// exponentiation's.
fn batch(rec: &mut Rec) -> Vec<u8> {
    let records = read_blob();
    const L: usize = 3 * residue::FP12_BYTES + 1;
    assert_eq!(records.len() % L, 0);
    let (mut witness_cycles, mut final_cycles) = (0u64, 0u64);
    let mut out = Vec::with_capacity(records.len() / L);
    for r in records.chunks_exact(L) {
        let f: &[u8; residue::FP12_BYTES] = r[..576].try_into().unwrap();
        let c: &[u8; residue::FP12_BYTES] = r[576..1152].try_into().unwrap();
        let w: &[u8; residue::FP12_BYTES] = r[1152..1728].try_into().unwrap();
        let f = residue::decode_fp12(f).expect("canonical product");
        let t0 = env::cycle_count();
        let by_witness = residue::check_bytes(&f, c, w).is_ok();
        let t1 = env::cycle_count();
        let by_final = residue::final_exp_is_one(&f);
        let t2 = env::cycle_count();
        witness_cycles += t1 - t0;
        final_cycles += t2 - t1;
        out.push(u8::from(by_witness) | (u8::from(by_final) << 1));
    }
    rec.note("records", out.len() as u64);
    rec.note("witness checks, cycles in all", witness_cycles);
    rec.note("final exponentiations, cycles in all", final_cycles);
    out
}

/// The cost of each operation, on the inputs f, c, w of one real update.
fn ops(rec: &mut Rec) -> Vec<u8> {
    let (fb, cb, wb) = (read_blob(), read_blob(), read_blob());
    let f = residue::decode_fp12(fb.as_slice().try_into().unwrap()).unwrap();
    let cbytes: &[u8; residue::FP12_BYTES] = cb.as_slice().try_into().unwrap();
    let wbytes: &[u8; residue::FP12_BYTES] = wb.as_slice().try_into().unwrap();
    let c = residue::decode_fp12(cbytes).unwrap();
    let w = residue::decode_fp12(wbytes).unwrap();
    const K: u64 = 16;
    let mut x = c;
    let t = rec.time("O1 16 generic Fp12 squarings (blst_fp12_sqr)", || {
        for _ in 0..K {
            x = residue::sqr(&x);
        }
    });
    black_box(t);
    let mut y = c;
    rec.time("O2 16 Fp12 multiplications (blst_fp12_mul)", || {
        for _ in 0..K {
            y = residue::mul(&y, &f);
        }
    });
    let mut z = c;
    rec.time("O3 16 Frobenius maps (blst_fp12_frobenius_map)", || {
        for _ in 0..K {
            z = residue::frobenius(&z);
        }
    });
    let mut q = blst_fp12::default();
    let g = residue::mul(&f, &w); // any element; cyclotomic squaring is timed, not used
    let mut s = g;
    rec.time("O4 16 cyclotomic squarings (blst_fp12_cyclotomic_sqr; for comparison)", || {
        for _ in 0..K {
            // SAFETY: valid destination and source.
            unsafe { blst_fp12_cyclotomic_sqr(&mut q, &s) };
            s = q;
        }
    });
    rec.time("O5 c^|u| (63 squarings, 5 multiplications)", || residue::pow_abs_u(&c));
    rec.time("O6 c^lambda = Frobenius(c) * c^|u|", || residue::pow_lambda(&c));
    rec.time("O7 decode c and w (canonical)", || (residue::decode_fp12(cbytes), residue::decode_fp12(wbytes)));
    let ok = rec.time("O8 the whole witness check (residue::check_bytes)", || residue::check_bytes(&f, cbytes, wbytes));
    assert_eq!(ok, Ok(()));
    let one = rec.time("O9 final exponentiation and is_one (blst_final_exp)", || residue::final_exp_is_one(&f));
    assert!(one);
    black_box((x, y, z, s));
    Vec::new()
}
