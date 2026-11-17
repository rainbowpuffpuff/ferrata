# The residue-witness check: one light-client update in one LEZ transaction?

**Experimental and unaudited. No deployed program uses it.** The light-client program, the bridge
and every other guest with an image id (`docs/image-ids.md`) still compute the final
exponentiation; nothing in this note changed them. Everything here is a bench measurement of the
core's check (`bench/residue`, `bench/guests/residue-accel`), plus one run through LEZ's state
machine of an experimental copy of the program (§6), made on 17 Nov 2026.

## 1. Verdict

| one whole update, as `VerifyAll` runs it, bare RISC Zero guest (user cycles) | current path (final exponentiation) | witness path | witness folded into the Miller loop | of `MAX_GAS_EXEC` (10M): witness, folded |
|---|---:|---:|---:|---:|
| finality update, mainnet update_1, 510 of 512 signers | 9,076,609 | **6,573,265** | **5,911,714** | 65.7%, 59.1% |
| finality update, 342 of 512 signers (the fewest the policy accepts) | 13,312,372 | 10,809,028 | 10,147,477 | 108.1%, 101.5%: **does not fit** |
| period update (adopts the next committee's 512 keys), 510 of 512 | 15,100,180 | 12,596,637 | 11,935,103 | 126.0%, 119.4%: **does not fit** |
| period update, 342 of 512 | 19,335,943 | 16,832,400 | 16,170,866 | 168.3%, 161.7%: **does not fit** |

* The witness check replaces the final exponentiation's **3,611,883** cycles by **1,108,036**
  (−2,503,847, −69%); folded into the Miller loop, the loop and the check together cost 3,722,465
  instead of 4,383,105 (a further −660,640). Nothing else moves.
* A **finality update at mainnet participation fits one transaction with a third of the limit to
  spare**: LEZ's state machine charged an experimental copy of the program **6,629,246** execution
  gas for it (66.3% of `MAX_GAS_EXEC`), against 9,124,896 for the deployed `VerifyAll` in the same
  build (§6).
* It does **not** fit at the worst participation the policy accepts (342 of 512), even folded: each
  of the 170 non-signers costs 25,213 cycles (its key and its Merkle branch to the committee root in
  step 1, and its 384 bytes of witness), 4.3M in all, and on LEZ its input costs about 0.37M more
  (§6). A **period update does not fit at any participation**: adopting 512 keys costs 5,991,991
  cycles on top of the verification.
* The deployed program already takes a finality update in one public transaction (`VerifyAll`,
  9,124,896 execution gas at 510 of 512 here, 91.2% of the limit; `lez/RESULTS.md` §2 has 9,121,514
  on another build); the live updater (`lez/updater`) uses the job path (three transactions, five
  with an adoption, each under the 5M target). The witness check would move the one-transaction
  path from 91% to 66% of the limit (folded, about 60%: derived, §6); it does not remove the job
  path, which period updates and low participation still need.

## 2. What the check is

A signature check computes the Miller-loop product `f = ML(H(m), −key) · ML(signature, g1)` (step
2 of the job, a two-pair Miller loop) and accepts iff `f^h == 1`, `h = (p^12 − 1)/r`. The current
path computes that power: blst's final exponentiation (it computes `f^(3h)`, measured; the same
verdict, since `f^h` lies in the group of order r and `gcd(3, r) = 1`).

The witness path lets the submitter compute two elements off-chain (`residue::witness`: three
exponentiations in Fp12, natively; not timed separately): the **residue witness** `c ∈ Fp12` and the **scaling factor** `w ∈ Fp6`. The program
checks, on its own `f`:

1. `c` and `w` are canonical encodings (each 48-byte coefficient below p);
2. `c ≠ 0`;
3. `w ∈ Fp6`, the subfield of `Fp12 = Fp6[w]/(w² − v)`: the second half of `w`'s 576 bytes is zero;
4. `f · w == c^λ` with `λ = p − u = p + |u|`, so `c^λ = Frobenius(c) · c^|u|`: 63 generic Fp12
   squarings and 5 multiplications for `c^|u|` (|u| = 0xd201000000010000), one Frobenius map, two
   multiplications.

`bench/residue/src/lib.rs` (`check`, `check_bytes`); constants derived from p, r and u only by
`gen_constants.py`, which asserts every relation below, and re-derived with num-bigint by
`tests/constants.rs`.

## 3. Soundness: why a passing check means the signature verifies

Two facts, both asserted (`gen_constants.py`, `tests/constants.rs`):

* **(F1)** `r | λ`. For the BLS12 family `p = (u−1)²(u⁴−u²+1)/3 + u` and `r = u⁴−u²+1`, so
  `λ = p − u = 3·POLY²·r` with `POLY = (1−u)/3`.
* **(F2)** `(p^6 − 1) | h`. The embedding degree is 12, so `r | p^4 − p^2 + 1 | p^6 + 1` and
  `r ∤ p^6 − 1`; hence `h = (p^6 − 1) · (p^6 + 1)/r`.

**Claim.** If `c ≠ 0`, `w ∈ Fp6` and `f · w = c^λ`, then `f^h = 1`.

**Proof.** `c ≠ 0` gives `c^λ ≠ 0`, so `f·w ≠ 0` and `w ≠ 0`. Every nonzero element of Fp6 has
`w^(p^6−1) = 1`, so `w^h = 1` by (F2). Every nonzero `c` has `c^(p^12−1) = 1`, and
`λh = (λ/r)(p^12 − 1)` by (F1), so `c^(λh) = 1`. Then `f^h = (c^λ)^h · (w^h)^−1 = 1`. ∎

Nothing is assumed about how `c` and `w` were made: any pair that passes proves `f^h = 1`, which is
exactly what the final exponentiation checks. **What an adversarial witness would need**, for an
`f` with `f^h ≠ 1`: a `c ≠ 0` and a `w ∈ Fp6` with `f·w = c^λ`. By the claim, none exists. Each
side condition is needed, and test (b) shows both failures on a real forged product:

* without `c ≠ 0`, `c = w = 0` satisfies `f·w == c^λ` for every `f`. gnark constrains its residue
  witness to be invertible for exactly this reason (`pairing.go:608-614` at the commit below:
  "the all-zero witness would satisfy it (0 == 0) for any x");
* without `w ∈ Fp6`, `w = c^λ / f` satisfies it for every `f` and every `c ≠ 0`. gnark (`pairing.go:616-644`)
  and OpenVM (`pairing.rs:325-332`) constrain the scaling factor to Fp6.

Canonical encodings are not needed by the algebra (blst reduces), but make every accepted input
have one meaning. **Witnesses are not unique**: `c·z` for any `z` with `z^λ = 1` (a cube root of
unity, for one; `3 | λ`) also passes (test (b)). That does not weaken the check; it means no program
may use `c` as an identifier.

**Completeness** (an honest witness exists for every valid `f`, and `residue::witness` finds it):
`h = 27·POLY·F` with 27, POLY and F pairwise coprime and `gcd(F, λ) = 1`, and `27·POLY | p^6 − 1`
(asserted). For `f^h = 1`, split `f = f_F · f_27 · f_P` over the subgroups of orders F, 27 and
POLY; then `w = (f^F)^(27A + POLY·B) = f_27^−1 · f_P^−1` lies in Fp6, `f·w = f_F`, and
`c = (f·w)^(λ^−1 mod F)` has `c^λ = f·w`. This is the algorithm of gnark's `finalExpWitness`
(`hints.go:151-218`) and OpenVM's `final_exp_hint`. Run on 5,032 valid products whose 3-parts have
every possible order (1, 3, 9, 27) and on all 632 real signature checks (test (c), (a)).

**References and their status.** The method is A. Novakovic and L. Eagen, "On Proving Pairings"
(IACR ePrint 2024/640). The paper's text could not be fetched for this note (eprint.iacr.org
answered automated requests with a bot check or HTTP 403), so this note cites none of its sections:
the argument above is self-contained, and its BLS12-381 form is the one two implementations use,
read at pinned commits: gnark `std/algebra/emulated/sw_bls12381` at
`645991746bd91d3ce137f86bbdd21cbcfa0bdbe4` (`AssertFinalExponentiationIsOne`, `pairing.go:598-657`;
`finalExpWitness`, `hints.go:151-218`) and OpenVM at `df4e8c5cce33b31e103be78856a6cc66ed08e410`
(`try_honest_pairing_check`, `guest-libs/pairing/src/bls12_381/pairing.rs:320-358`, which falls
back to the full final exponentiation when the hint fails, l. 304-312; its comment at
`final_exp.rs:31-32` says the paper describes BN254 only, and it adapts gnark's code for
BLS12-381). Copies and line notes: `spike/upstream/residue-refs/` beside the repository. No code was
copied from either; `bench/residue` re-implements the check and the witness algorithm on blst.

## 4. What was tested

All on Modal (one container, `cargo test --release -p residue` and `residue-host`), 17 Nov 2026;
the test logs' summaries are in `bench/results/residue/tests.txt`.

| test | what | count | result |
|---|---|---:|---|
| constants | p, r, u relations; F1, F2; the decomposition of h; every exponent re-derived with num-bigint; `c^λ` by the check's formula equals the generic power on 32 random elements | 2 tests | pass |
| (a) real data | every signature check the core makes that reaches the pairing, through a backend that recomputes the product and asserts that the final exponentiation and the witness check (honest witness) agree with the verdict: the pinned official `sync` vectors (52 cases, consensus-specs v1.7.0-beta.4, spec store and policy store, Full and OnChain committee checks), the mainnet replay (bootstrap and 5 updates, both stores, and the step API's own product), Sepolia through the Gloas fork (32 period updates and a finality update, both stores) | 632 checks (556 + 10 + 66) and the step API's product of the 5 mainnet updates; 143 distinct products | all agree; all are valid signatures, so the invalid side is (b) and (c) |
| (b) tampering | mainnet update_1 through the step API with: another update's signature; a changed message (attested header); a signer marked absent; an absent member marked a signer (the final exponentiation refuses each; the witness check refuses each, with the honest witness and with update_1's); `c` with one bit flipped, from another update, zero, non-canonical; `w` times a cube root of unity, from another update, outside Fp6, zero, non-canonical; the two forged witnesses of §3 | 15 refused | pass; also: `w = 1` refused, `c·z` accepted (not unique) |
| (c) differential | 11,034 products: 3,000 valid signatures, 1,000 valid aggregates of 2–16 signers, 1,000 constructed `x^r·z`, 32 edge (1, pure 27- and POLY-parts); 2,000 wrong message, 1,000 wrong key, 2,000 random Fp12, 1,000 valid-times-order-r, zero, an order-r element. Ground truth twice: blst's final exponentiation and an independent square-and-multiply `f^h`; both must also equal the verdict the product was built for | 5,032 valid, 6,002 invalid | the witness check accepts exactly the valid ones; every invalid one's honest output is refused (6,001 not in Fp6, zero by `c = 0`) |
| (d) guest = native | the 143 products of (a) checked in the guest (accelerated blst) by both paths, against the native verdicts | 143 | equal; also four refusals in the one-update guest (witness path: a flipped `c`, `w` outside Fp6; folded: `c` given as its own inverse, a flipped `c`) |
| folded | `residue::fold`: the re-implemented two-pair Miller loop against `blst_miller_loop_n`, byte for byte; the folded check against the plain check and the final exponentiation; per valid product a `c^−1` that is not the inverse, `w` outside Fp6 and a flipped `c`; `c = 0` | 2,000 products (1,000 valid, 1,000 invalid), 3,000 tampered | equal, agree, all refused |

Commands: `cd bench && cargo test --release -p residue` (the vectors first:
`scripts/fetch-vectors.sh`); `cargo build --release -p residue-host && ./target/release/residue-host measure`
then `./target/release/residue-host check` (it reads `target/residue-tuples-a.bin`, which the
`real_data` test writes). Outputs: `bench/results/residue/`.

## 5. Numbers

`bench/results/residue/tables.md` and `one-tx.txt` (r0vm 3.0.5 executor; accelerated blst
`v0.3.16-risczero.0` and SHA-256 as `guests/blst-accel`; core of the branch, unchanged), all from
one run of `residue-host measure`. One execution decodes the compact store, the compact update and
the signer witness, runs step 1, the two-pair Miller loop, the final check, the adoption (period
updates: 512 keys in one part), encodes the job and applies it, as the program's `VerifyAll` and
`ApplyVerified` do. The 342- and 512-signer rows put a committee whose keys the bench holds in place
of the signing committee in the store (its summary), and sign the real update_1 with it; headers
and branches stay real. At 510 signers this gives the real update's cycles within 187.

| case (update_1) | path | step 1 | Miller | final check | Miller + check | adoption | rest | whole (user cycles) | segments |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| finality, 510 (real) | current | 2,087,956 | 3,275,069 | 3,611,883 | 6,886,952 | 0 | 58,373 | 9,076,609 | 10 |
| finality, 510 (real) | witness | 2,087,956 | 3,275,069 | 1,108,036 | 4,383,105 | 0 | 58,747 | 6,573,265 | 7 |
| finality, 510 (real) | folded | 2,087,956 | | | 3,722,465 | 0 | 58,620 | 5,911,714 | 7 |
| finality, 342 | current | 5,838,325 | 3,275,069 | 3,611,883 | 6,886,952 | 0 | 543,767 | 13,312,372 | 14 |
| finality, 342 | witness | 5,838,325 | 3,275,069 | 1,108,036 | 4,383,105 | 0 | 544,141 | 10,809,028 | 12 |
| finality, 342 | folded | 5,838,325 | | | 3,722,465 | 0 | 544,014 | 10,147,477 | 11 |
| finality, 512 | witness | 2,042,367 | 3,275,069 | 1,108,036 | 4,383,105 | 0 | 52,944 | 6,521,873 | 7 |
| finality, 512 | folded | 2,042,367 | | | 3,722,465 | 0 | 52,817 | 5,860,322 | 7 |
| period, 510 (real) | current | 2,098,043 | 3,275,069 | 3,611,883 | 6,886,952 | 5,991,991 | 66,382 | 15,100,180 | 16 |
| period, 510 (real) | witness | 2,098,043 | 3,275,069 | 1,108,036 | 4,383,105 | 5,991,991 | 66,756 | 12,596,637 | 13 |
| period, 510 (real) | folded | 2,098,043 | | | 3,722,465 | 5,991,991 | 66,629 | 11,935,103 | 13 |
| period, 342 | witness | 5,848,412 | 3,275,069 | 1,108,036 | 4,383,105 | 5,991,991 | 552,150 | 16,832,400 | 18 |
| period, 342 | folded | 5,848,412 | | | 3,722,465 | 5,991,991 | 552,023 | 16,170,866 | 17 |
| period, 512 | folded | 2,052,454 | | | 3,722,465 | 5,991,991 | 60,826 | 11,883,711 | 13 |

The other rows (every case with every path) are in `tables.md`. The operations, in the guest
(`OPS`): generic Fp12 squaring 14,693; multiplication 19,667; Frobenius map 4,732; cyclotomic
squaring 9,661; `c^|u|` 994,258; the whole check 1,085,690; blst's final exponentiation 3,581,110.
A non-signer costs 25,213 cycles: 22,324 in step 1, 2,889 to read and decode its 384 bytes.

**Folding the witness into the Miller loop** (as gnark's `PairingCheck` and OpenVM do;
`residue::fold`). blst has no Miller loop whose accumulator can be seeded, so `fold::miller_loop`
re-implements blst's two-pair loop (`miller_loop_n`) on blst's own line precomputation
(`blst_precompute_lines`), line evaluation and Fp12 arithmetic; unseeded it equals
`blst_miller_loop_n` byte for byte (2,000 products, `tests/fold.rs`). Seeded with `conj(c^−1)`, it
yields `f · c^−|u|` along the loop's own squarings, and the check is `c · c^−1 == 1`, `w ∈ Fp6`,
`(f · c^−|u|) · w == Frobenius(c)`, equivalent to the plain one (multiply by `c^|u|`). The
submitter supplies `c^−1` (576 bytes more). It costs 3,722,465 for the loop and the check against
4,383,105 unfolded: **−660,640** (estimated from the operation costs beforehand: at most 0.79M).
On the same 2,000 products the folded check, the plain check and the final exponentiation agree,
and 3,000 tampered witnesses are refused; in the guest it refuses a `c^−1` that is not the inverse
and a flipped `c`. It is a second implementation of the Miller loop's schedule to audit, for 7% of
the limit, and it does not make the 342-signer finality update (10,147,477) or any period update
fit.

**Execution gas and cycles in LEZ v0.4.** LEZ charges a public transaction the user cycles of
every handler it runs (`SessionInfo::cycles`, as `lee` meters them), clamped to the gas limit
(`lez/RESULTS.md` §2: "Execution gas = the cycles LEZ metered for the transaction (all its
handlers), clamped to its gas limit"). A handler also pays for its envelope: about 5.7 cycles per
byte it receives and 4.1 per byte it sends on (`lez/RESULTS.md`, "What the LEZ v0.4 shell
costs"). For the finality update the bench's whole execution (9,076,609) and LEZ's charge for the
deployed `VerifyAll` in the run of §6 (9,124,896) differ by 0.5%. `MAX_GAS_EXEC` = 10,000,000 =
2 × `TARGET_GAS_EXEC` is both the cap on a transaction's gas limit and the block's execution-gas
cap (LEZ v0.4.0-rc1 and rc3: `lez/programs/fee/core/src/market.rs:10-15`, `validity.rs:56`,
`lez/programs/fee/src/main.rs:43`).

## 6. Through LEZ's state machine (experimental copy of the program)

`lez/guests/program-residue-experimental` is a copy of the deployed program (`guests/program`)
with one change, in `VerifyAll`: the `signers` field carries the signer witness followed by `c` and
`w`, and the residue-witness check runs where `Job::step3` ran (`diff` the two `main.rs`). It is
built by its own methods crate (`lez/methods-residue-experimental`), so the deployed programs'
methods crate and image ids are untouched, and it is deployed only inside one harness test
(`lez/harness/tests/residue_experimental.rs`), at a named account of its own. In that test an
instance is initialized and takes update_0 through the copy's job path (unchanged), and mainnet
update_1 as a finality update is then sent as one public `VerifyAll` transaction, settled by the
harness's copy of LEZ's `settle_charged_transaction` and metered by `lee` (in-process harness,
LEZ v0.4.0-rc1, as `lez/RESULTS.md` §1; RISC Zero dev mode, which changes nothing for public
transactions: they are executed, not proven). The deployed program's `VerifyAll` takes the same
update in the same build, for comparison (`scenarios::one_transaction_update`).

| transaction | handlers (user cycles each) | execution gas | of `MAX_GAS_EXEC` | storage gas | base fee | result |
|---|---|---:|---:|---:|---:|---|
| experimental `VerifyAll`, witness with one bit of `c` flipped | `VerifyAll` 6,532,961 | 6,532,961 | 65.3% | 5,257 | 52,305,744 | refused: exit 40 (`SignatureInvalid`); nothing changed |
| experimental `VerifyAll`, honest witness | `VerifyAll` 6,553,714 → `ApplyVerified` 58,186 → `Record` 17,346 | **6,629,246** | **66.3%** | 5,257 | 53,076,024 | applied; the store equals the native store's, the record is written |
| deployed `VerifyAll` (final exponentiation) | `VerifyAll` 9,049,362 → `ApplyVerified` 58,184 → `Record` 17,350 | 9,124,896 | 91.2% | 4,105 | 73,032,008 | applied |

The witness check saves **2,495,650** execution gas on LEZ (the bare guest's whole update: 2,503,344); the
transaction is 1,152 bytes longer (`c` and `w`). The harness labels the copy's handlers by its
account (`program 7MNn…`), not by message; the order of the messages is the program's. Earlier
measurements of the deployed `VerifyAll` on this update (`lez/RESULTS.md` §2: 9,121,514) were made
on other builds of the same source. Output: [`results/residue/lez-experimental.txt`](results/residue/lez-experimental.txt).
Re-check: `cd lez && RISC0_HOME=$PWD/../../risc0-home-lez RISC0_DEV_MODE=1 cargo test -p harness
--test residue_experimental -- --nocapture` (needs `spike/upstream/lez-v0.4.0-rc1` beside the
repository, as every harness test).

Not run through the state machine: the folded variant (derived: 6,629,246 − 660,640 + 576 bytes
of `c^−1` at 5.7 cycles each ≈ 5.97M, 60% of the limit), and the 342-signer and period cases (the
instance would need a committee whose keys we hold, from its bootstrap on): their bare-guest
numbers (§5) are over the limit already, and LEZ adds about 5.7 cycles per byte of input (the
342-signer finality update carries 64,512 bytes more signer witness than the 510-signer one: about
0.37M more).

## 7. What an auditor must check

1. The constants and the relations F1, F2 and the decomposition of h (`gen_constants.py`,
   `tests/constants.rs`): two independent derivations, but both mine.
2. That `blst_fp12_sqr` is the generic squaring and `blst_fp12_frobenius_map(·, 1)` is `x ↦ x^p`
   on every element, in native blst and in RISC Zero's fork: `c` is not in the cyclotomic subgroup,
   where blst's faster formulas would be wrong. (`tests/constants.rs` compares `c^λ` with a
   generic power on 32 elements natively; (d) checks the guest on 143 products.)
3. That `in_fp6` reads the coefficient of `w` in blst's tower (`fp6[1]`) and that the 576-byte
   encoding's second half is that coefficient.
4. That zero, membership of Fp6 and equality are decided on the limbs, which is right only because
   blst keeps every element fully reduced (inputs are refused unless canonical). In the `blst` Rust
   crate `blst_fp12::default()` is **one**, not zero (found here; the tests build zero from 576 zero
   bytes).
5. That the product checked is the program's own Miller-loop output (step 2 in the same
   execution), never an input of the transaction.
6. That nothing downstream relies on `c` being unique.
7. That the witness algorithm is complete for every valid product (§3), so that an honest
   submitter is never refused: proved in §3, run on the 5,032 valid products of (c) and the 143
   distinct products of (a).
8. The experimental program copy (§6) against `guests/program`: `diff` shows the only change.

## 8. The production change, as an option

The light-client program's `VerifyAll` (the one-transaction path of milestone A1's production
form) could take `c` and `w` and run this check in place of `Job::step3`, which would put a
finality update at mainnet participation at about two thirds of the limit. It would cost: a new
program image and so a new image id (`docs/image-ids.md`), a new deployment and instances (an
immutable program cannot be changed in place), new real proofs of the private path (the M0 proofs
were made on the current image), the witness computation in the updater (host code, milliseconds),
and an audit of the check. It would not replace the job path, which period updates and low
participation still need. Nothing of this is done; the deployed programs are unchanged.
