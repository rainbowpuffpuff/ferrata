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
  (§6). **With the non-signers' keys checked through one multiproof instead (§9), it fits**: LEZ's
  state machine charged an experimental copy 8,318,805 execution gas (83.2%) for it; bare guest
  7,458,783 folded. A **period update does not fit at any participation**: adopting 512 keys costs 5,991,991
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
and branches stay real. At 510 signers this gives the real update's cycles within 187. The run was
repeated after the guest was rebuilt (reformatted source): every user-cycle count was identical.

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
--features residue-experimental --test residue_experimental -- --nocapture` (needs
`spike/upstream/lez-v0.4.0-rc1` beside the repository, as every harness test). The copy and this test
are behind the harness's feature `residue-experimental`: no ordinary build or test of the harness, or
of what depends on it, builds the experimental guest.

Not run through the state machine: the folded variant (derived: 6,629,246 − 660,640 + 576 bytes
of `c^−1` at 5.7 cycles each ≈ 5.97M, 60% of the limit), and the 342-signer and period cases (the
instance would need a committee whose keys we hold, from its bootstrap on; the 342-signer case was
later run with the bench's committee written into the instance's store, §9.6): their bare-guest
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

One option for milestone A1 (the light client in production form) would be the light-client
program's update as one transaction: its `VerifyAll` would take `c` and `w` (and, folded, `c^−1`)
and run this check in place of `Job::step3`, which would put a finality update at mainnet
participation at about two thirds of the limit (about 60% folded). It would cost: a new program
image and so a new image id (`docs/image-ids.md`), a new deployment and new instances (an immutable
program cannot be changed in place), new real proofs of the private path (the M0 proofs were made
on the current image), the witness computation in the updater (host code: three exponentiations in
Fp12 per update, not timed here), and an audit of the check. It would not replace the job path,
which period updates still need (and low participation, unless the composition of §9 is adopted
too). Nothing of this is done; the deployed
programs are unchanged.

## 9. The fewest signers in one transaction: one multiproof for the non-signers

Measured on 17 Nov 2026, evening. **Experimental and unaudited, a bench measurement and an
experimental copy of the program; nothing deployed changed** (no program or guest with an image id
in `docs/image-ids.md`, and `crates/ferrata-core`, are untouched).

**Verdict.** With the signing key composed from one Merkle multiproof for all the non-signers
(`bench/residue-agg`) instead of a branch per non-signer, a finality update signed by the fewest
members the policy accepts (342 of 512, the 170 absent members placed so that the multiproof needs
the most hashes) fits one LEZ transaction: LEZ's state machine charged the experimental copy of the
program **8,318,805** execution gas (**83.2%** of `MAX_GAS_EXEC`) with the residue-witness check
(§6), and refused the same update with a branch per non-signer for exceeding the 10M budget. In the
bare guest the whole update costs 8,120,369 user cycles with the witness check and **7,458,783**
(74.6%) folded, against 10,788,327 and 10,126,621 with the core's composition in the same build.
A non-signer now costs 9,418 cycles instead of 25,094. The composition makes the same checks on
every key; §9.3 gives the argument. A period update still does not fit (13,482,391 folded at 342,
11,913,877 at 510: the adoption of 512 keys is 5,991,991). With a branch per non-signer the fewest
signers that fit are 386 through LEZ's state machine (bare guest: 348 folded, 374 with the witness
check); with one multiproof every participation the policy accepts fits.

### 9.1 Where a non-signer's cycles went

`residue-host profile` (`results/residue/agg/profile.txt`): the parts of the core's composition
(`steps.rs:632-666`, copied into the guest so that its parts can be timed), each run over the 170
non-signers of the 342-signer case (every third member absent), divided by 170.

| part (the core's composition) | cycles per non-signer | share |
|---|---:|---:|
| decode the uncompressed key: `blst_p1_deserialize` (flags, canonical, to Montgomery form 1,707, on the curve 834) | 2,862 | 13% |
| not the identity | 110 | 0% |
| re-compress the key (`blst_p1_affine_compress`: x out of Montgomery form, the sign of y) | 1,173 | 5% |
| the key's SSZ leaf: SHA-256 of 64 bytes (sha2's streaming API) | 1,244 | 6% |
| the branch: 9 SHA-256 nodes of 64 bytes (1,247 each) | 11,778 | 53% |
| the sum: one `blst_p1_add_or_double_affine` | 4,839 | 22% |
| bookkeeping (absent indices, count and length checks) | 44 | 0% |
| **step 1, per non-signer** (the whole composition: 3,857,505 for 170, less the once-only 96,448) | **22,124** | |
| read and decode its 384 bytes of witness (`SignerWitness::decode`: 469,468 for 170) | 2,762 | |

There is **no subgroup check** of a non-signer's key in the core (nor of any key at adoption:
`witness.rs`, "it leaves out the per-key subgroup check", as `check_committee_onchain`); for
comparison, `blst_p1_affine_in_g1` would cost 352,513 cycles per key. **Nothing else depends on the
participation**: at 512, 511, 510, 450, 400, 343 and 342 signers the Miller loop (3,275,069) and the
final check (1,108,036; folded 3,722,450 together) are the same, the job's encoding and application
differ by under 50 cycles, and step 1 and the witness's decoding grow by a fixed amount per
non-signer (§9.6). Unit costs in the guest: `blst_fp_mul` 205, `blst_fp_inverse` 244 (RISC Zero's
accelerated inversion), one SHA-256 node by sha2's streaming API 1,247, by two compressions of an
aligned buffer 811.

### 9.2 What changed

`bench/residue-agg` (`src/lib.rs`), with `step1`: the core's `steps::step1` copied with one change,
the composition of the signing key; the job it returns is the core's `Job` and the rest (`Job::step2`,
the final check, `steps::apply`) is the core's code.

1. **One multiproof** (`multiproof_root`): the leaves are computed from the supplied keys at the
   positions of the clear bits of the bitfield; level by level from the leaves up, each known node is
   hashed with its sibling, which is the computed node when the sibling is known too and otherwise
   the next node of the witness; after the tree's depth (9) the one node left must be the committee's
   key root, and every node of the witness must have been read. A node shared by several non-signers'
   paths is hashed once: 170 non-signers need at most 425 hashes (every third member absent; any 170
   absent members need at most Σ min(170, 2^k) over the levels = 425) instead of 1,530, and at most
   256 witness nodes instead of 1,530.
2. **The compressed key read off its bytes** (`compressed`): for an encoding blst has just accepted
   (flags clear, x and y below p), the compressed key is x's 48 bytes with the compression flag and,
   iff `y > (p − 1)/2`, the sign flag; blst gets the same bytes by converting x and y out of Montgomery
   form.
3. **The batch addition** (`blst_p1s_add`, blst's `bulk_addition.c`): Montgomery's trick, the
   points added in pairs with one inversion per round; it handles a pair of equal points (doubling) and
   of opposite points (the identity). The chain of mixed additions stays available (`Sum::Chain`) for
   comparison.
4. **SHA-256 of 64 bytes as two compressions** (`hash64`, `node`, `leaf`): the message and the fixed
   padding block in one word-aligned 128-byte buffer, `sha2::compress256` (in the guest RISC Zero's
   accelerator, one call for both blocks), instead of the streaming hasher, whose blocks RISC Zero's
   `sha2` patch copies when they are unaligned.

The witness (`NonSigners`): a `u16` key count, the keys (96 bytes each, in index order), a `u16` node
count, the nodes (32 bytes each); strict (each count at most the committee size, the input consumed
exactly). At 342 signers it is at most 24,516 bytes (every third member absent; 22,756 at random
positions) instead of 65,282.

### 9.3 Why it is sound

Let the store hold, for the signing committee, the key root `R` (the SSZ root of the leaves
`L_i = SHA-256(K_i ‖ 0^16)`, `K_i` the compressed keys) and the aggregate key `A`. The core accepts
non-signer `i`'s key `P_i` iff its encoding decodes to a point on the curve that is not the identity
(blst's canonical decoding) and the leaf of `P_i`'s compressed form with the branch hashes to `R` at
index `i`; the key is `A − Σ P_i`.

**Claim.** If `residue_agg::signing_key` accepts, then for every absent member `i` the supplied point
is the one the core accepts at index `i` (the curve point whose compressed form is `K_i`), unless
SHA-256 has a collision; so the key it returns is the core's. Conversely, on the honest witness it
accepts (completeness).

**Proof.** (i) *Decoding*: `key_point` accepts exactly the encodings the core's `witness_key`
accepts (the same blst call; the flag check refuses only encodings blst or the identity check
refuses anyway). (ii) *Compressed form*: for those encodings x and y are below p, so x's bytes are
its canonical encoding with the three flag bits free, and the order of y's big-endian bytes is its
numeric order; ZCash's sign flag is "y is the larger of y and p − y", i.e. `y > (p − 1)/2`. So
`compressed(P) = blst_p1_affine_compress(P)`. (iii) *Multiproof*: the positions of the leaves are
the clear bits of the bitfield and the depth is fixed, so every node the verifier computes has a
position fixed before any witness node is read. Suppose the computed root equals `R`. Going down
from the root, a computed node equal to the true node at its position has its two children equal to
the true children, else `SHA-256(a ‖ b) = SHA-256(A ‖ B)` with `(a, b) ≠ (A, B)` is a collision; a
child the verifier computed is therefore the true node, whatever witness node may have stood beside
it. By induction every computed leaf is the true leaf at its position: `SHA-256(compressed(P_i) ‖
0^16) = L_i`, so `compressed(P_i) = K_i` (else a collision). This is the argument for one branch,
applied to the union of the paths. (iv) *The point*: a point on the curve is determined by its
compressed form (x, and the sign selects one of y and p − y; y = 0 has no point, as the curve's group
over Fp has odd order). So `P_i` is the core's point for index `i`. (v) *The sum*: `blst_p1s_add`
returns the group sum of the points (the group law is the same for points outside the subgroup);
`A − Σ P_i` is then the core's key. (vi) *Completeness*: `multiproof_siblings` writes exactly the
nodes `multiproof_root` reads, in its order. ∎

The same refusals: a count that disagrees with the bitfield, a key that does not decode, a multiproof
that does not give `R` or is not read to its end: `StepError::Witness` (80); an aggregate that does
not decompress: `PubkeyInvalid`; the identity as the signing key: `SigningKeyIdentity`; as the core.

**What each rule of the multiproof is for** (test `forged_multiproofs_show_why_each_rule_is_needed`):
* *A known sibling is computed, never read.* A verifier that reads the sibling of node 40 from the
  witness even though member 41 is absent too never looks at 41's key: the test replaces it by a point
  chosen so that the signing key becomes an attacker's own key, puts 41's true leaf in the witness,
  and that verifier accepts; the attacker's signature then verifies under the composed key. The real
  verifier refuses it, with the forged and with the honest nodes.
* *Every witness node is read.* Without it a witness with a node appended is accepted: not a forgery,
  but a second encoding of the same witness; the real verifier refuses it.
* *Positions from the bitfield.* A valid multiproof of other members, with their true keys, proves
  those members; the bitfield's absent members are what must be subtracted. The real verifier,
  computing the leaves at the bitfield's positions, refuses it.

**What the store must hold** (candidate b: sum the signers or subtract the non-signers, whichever is
fewer). Subtracting the non-signers needs the committee's key root and its aggregate key; summing the
signers needs only the key root. The deployed store holds both (`CommitteeSummary`, 80 bytes per
committee); the aggregate is checked at adoption to be the sum of the committee's keys
(`AdoptionProgress::add_part`). Under the policy's floor the non-signers are at most 170 and the
signers at least 342, so subtracting is always the smaller set: there is nothing to gain, and the
composition stays subtractive (as the core's, `steps.rs` module documentation).

**Checks considered for dropping** (candidate d). None is dropped. The on-curve check is not implied
by the multiproof: the leaf binds only x and the sign of y, so a y off the curve with the right sign
would pass the multiproof (test `refusals_are_the_cores`, "a key off the curve", refused by the
on-curve check). There is no subgroup check to drop: the core makes none on a non-signer's key, and
adoption makes none either, so the premise "keys checked when the committee was adopted" does not
hold for the subgroup; that is the core's existing property, unchanged. The identity check is implied
by the flag check and kept (110 cycles).

### 9.4 The candidates

| candidate | status | effect at 342 (per non-signer) |
|---|---|---|
| (a) one multiproof for all non-signers | **kept**: sound (§9.3), tested | the branch work 11,778 → 2,460 (425 hashes for 170, at 984 with bookkeeping; random positions 2,141); witness 65,282 → 24,516 bytes |
| (b) the smaller side | nothing to gain: under the floor the non-signers are always fewer | none |
| (c) batched affine additions | **kept**: blst's own `blst_p1s_add` | the sum 4,839 → 2,869 |
| (c') an affine chain with one inversion per addition (inversion costs 244 cycles here) | **not done**: about 1,200 per addition by the unit costs, but new elliptic-curve code with its own edge cases to audit, for about 0.3M | estimated, not measured |
| (d) dropping a check | **none dropped** (§9.3) | none |
| the compressed key from the bytes | **kept**: equal to blst's on every accepted encoding | 1,173 → 307 |
| SHA-256 as two compressions | **kept** | 1,247 → 811 per node |
| decoding the key without blst (word-wise bytes, RISC Zero's bigint accelerator) | **not done**: more custom code for at most about 2,000 per key | not measured |

### 9.5 Tests

All on the Modal build box (`cargo test --release -p residue-agg`, 8 tests, 22.8 s), 17 Nov 2026;
the existing suites unchanged and passing (`cargo test --release -p residue`; the harness's test of
§6).

| test | what | count | result |
|---|---|---:|---|
| constants and the hash | `(p − 1)/2` re-derived with num-bigint; `hash64`, `node`, `leaf` against SHA-256, `hash_pair`, `pubkey_leaf` | 20,000 messages | equal |
| keys | `compressed` against `blst_p1_affine_compress` and the committed bytes on every key of 43 distinct committees (4 of the bench's, the mainnet and Sepolia committees of the fixtures) and its negation; `key_point` against the core's decoding on mutated encodings (each flag, the identity, zero, x + p, y + p, p − y, random bit flips) | 44,032 keys, 352,256 encodings | equal |
| multiproof | round trips on leaf sets of every size at depths 1 to 10 (all, one, the last, every other, every third, a half, 40 random sets per depth); every single witness node altered, a leaf altered, a node missing, one left over, two swapped, leaves out of order, a duplicated leaf, an index beyond the tree, no leaves; the node count `2 × hashes − hashes − leaves + 1` | 459 sets, 12,255 tamperings | all round trip, all refused |
| forgeries | the three of §9.3 | 3 | each accepted by the weakened rule, refused by `multiproof_root` |
| step 1 = the core's | `residue_agg::step1` (batch and chain sums) against the core's `steps::step1`, job for job (the same `−key` bytes) or refusal code for refusal code: every update of the official `sync` vectors (52 cases, consensus-specs v1.7.0-beta.4), the mainnet replay (5 updates as served and as finality updates), the Sepolia chain through Gloas (32 period updates and the finality update) | 160 + 10 + 33 updates (91 + 9 + 33 accepted by step 1, 69 + 1 refused alike) | equal |
| differential | the same comparison on (committee, bitfield) pairs: 44 committees (the 43 above and one with a negated and two duplicated keys) × 52 bitfields (342, 343, 400, 510, 511, 512 signers at random, every third absent, 170 first or last absent, the six members of the doubled and cancelling pairs absent, and random participations), on the mainnet store with the committee substituted | 2,288 pairs, 171 participations (342: 275, 343: 96, 400: 100, 510: 99, 511: 99, 512: 101) | equal |
| refusals | another member's key, a key outside the committee, a non-signer's key twice, a negated key, a key off the curve, the compression flag, one altered node (every node of the multiproof, against a branch node for the core), a node missing, one left over, a key missing, one too many, the witness of another bitfield: the core's code (80) by both; 341 signers: `BelowSupermajority` by both; a trailing byte and a truncated witness: malformed | 268 tampered witnesses, 1 policy refusal, 2 encodings | refused alike |
| batch sum | 16 pairs of opposite keys and 8 of equal keys absent (the first 32 sum to the identity) at 32 to 170 absent | 5 | batch = chain = the signers' sum |
| guest | `residue-host profile`: the three compositions give the same key, `compressed` and `leaf` equal blst's and the core's on every key, the multiproof gives the key root, `node` equals `hash_pair` on a chain of 1,000 (4 bitfields); `residue-host measure-agg` and `boundary`: every one of the 69 executions gives the native new store (the signature verified with the key composed in the guest); 3 refusals in the guest (a node altered, a key outside the committee, another bitfield's witness: code 80) | 4 + 69 + 3 | pass |
| LEZ | `lez/harness/tests/residue_experimental.rs`: 510 and 342 of 512 applied with one multiproof, the store equal to the native one; at 342 a branch per non-signer over the budget and one altered node refused (exit 80); the fewest signers that fit with a branch per non-signer | 3 tests (and the test of §6, unchanged) | pass |

### 9.6 Numbers

`residue-host measure-agg` (`results/residue/agg/tables.md`, `one-tx.txt`), the bench guest
`guests/residue-agg-accel` (r0vm 3.0.5 executor; the same accelerated blst and SHA-256 as §5): one
whole update per execution, as `VerifyAll` and `ApplyVerified`; finality update_1 against the store
after update_0, signed by the bench's committee substituted in the store (§5's method) except the
first row. Both compositions in the same guest build.

| signers | witness folded: core, one multiproof | witness check: core, one multiproof | final exponentiation: core, one multiproof | signer witness bytes: core, multiproof |
|---|---:|---:|---:|---:|
| 512 | 5,860,716 / 5,857,662 | 6,522,422 / 6,519,248 | 9,025,611 / 9,022,557 | 2 / 4 |
| 511 | 5,886,846 / 5,878,311 | 6,548,552 / 6,539,897 | 9,051,741 / 9,043,206 | 386 / 388 |
| 510 (mainnet, real committee) | 5,911,862 / 5,890,269 | 6,573,568 / 6,551,855 | 9,076,757 / 9,055,164 | 770 / 548 |
| 450 (62 absent at random) | 7,417,118 / 6,519,708 | 8,078,824 / 7,181,294 | 10,582,013 / 9,684,603 | 23,810 / 10,532 |
| 400 (112 at random) | 8,671,506 / 6,955,916 | 9,333,212 / 7,617,502 | 11,836,401 / 10,120,811 | 43,010 / 17,092 |
| 343 (169 at random) | 10,101,434 / 7,391,247 | 10,763,140 / 8,052,833 | 13,266,329 / 10,556,142 | 64,898 / 22,532 |
| 342 (170 at random) | 10,126,502 / 7,403,007 | 10,788,208 / 8,064,593 | 13,291,397 / 10,567,902 | 65,282 / 22,756 |
| **342 (every third absent: the most hashes)** | 10,126,621 / **7,458,783** | 10,788,327 / **8,120,369** | 13,291,516 / 10,623,678 | 65,282 / 24,516 |

Execution user cycles of the whole execution (`SessionInfo::cycles`). With the core's composition
this guest reproduces §5's figures within 549 cycles at 510 and 512 signers; at 342 it gives about
20,800 fewer (10,126,621 against 10,147,477 folded: another guest binary). Per non-signer (342, every third
absent, against 512): **9,418** cycles in all (step 1 9,224, decoding the witness 181, reading it 13)
instead of 25,094 (step 1 22,298, decoding 2,760, reading 35); at random positions 9,090. Every
third member absent is the worst case among the updates the policy accepts: what depends on where
the absent members sit is the multiproof's hashes (at most the sum over the levels of min(k, 2^ℓ),
425 for k = 170) and its nodes (at most 256), both reached there; the rest grows with the number of
non-signers, at most 170.

**The fewest signers that fit one transaction** (`residue-host boundary`, `results/residue/agg/boundary.md`;
absent members evenly spaced):

| path | a branch per non-signer (before) | one multiproof (after) |
|---|---|---|
| witness folded, bare guest | **348** (9,976,122; 347: 10,001,188) | **342**, the policy's floor (7,458,783) |
| witness check, bare guest | **374** (9,985,559; 373: 10,010,661) | **342** (8,120,369) |
| witness check, LEZ's state machine (the copy of §6) | **386** (9,985,704 gas; 380 to 385: over the 10M budget) | **342** (8,318,805 gas) |
| final exponentiation, bare guest | none at or below 512 | above 410 (410: 10,076,335): the final exponentiation alone is 3.6M |

**Through LEZ's state machine** (`lez/harness/tests/residue_experimental.rs`, in-process harness, LEZ
v0.4.0-rc1, as §6; output `results/residue/agg/lez.txt`): the experimental copy now also takes a
signer witness that starts with the marker `0xffff` (a count above any committee's size, so never a
valid `SignerWitness`) as a `NonSigners` witness, composed by `residue_agg::step1`; any other witness
goes to the core's step 1 as before. For the 342-signer update the instance's store is given the
bench's committee after update_0, as in the bench (§5), with LEZ's `force_insert_account`.

| transaction | handlers run (cycles each) | execution gas | of `MAX_GAS_EXEC` | storage gas | result |
|---|---|---:|---:|---:|---|
| 342 of 512, a branch per non-signer + residue witness | `VerifyAll` | 10,000,000 | 100% | 69,769 | reverted: "Execution exceeded its cycle budget of 10000000 cycles" |
| 342 of 512, one multiproof with one node altered | `VerifyAll` 1,494,948 | 1,494,948 | 14.9% | 29,005 | refused: exit 80 |
| **342 of 512, one multiproof + residue witness** | `VerifyAll` 8,243,266 → `ApplyVerified` 58,187 → `Record` 17,352 | **8,318,805** | **83.2%** | 29,005 | applied; the store is the natively applied job's, the record is written |
| 510 of 512 (mainnet), one multiproof + residue witness | `VerifyAll` 6,531,153 → 58,187 → 17,352 | 6,606,692 | 66.1% | 5,037 | applied; the store equals the native store |
| 510 of 512 (mainnet), a branch per non-signer + residue witness (§6's transaction) | `VerifyAll` 6,553,798 → 58,187 → 17,352 | 6,629,337 | 66.3% | 5,257 | applied (§6: 6,629,246 with the copy before this change) |

At 342 signers LEZ charges 198,436 more than the bare guest's whole update (8,120,369): the
handlers' envelope (about 6 cycles per input byte on 24,518 bytes of signer witness) and the two
handlers after `VerifyAll`. Folded into the Miller loop (not in the copy) the bare guest saves another
661,586 at 342, so about 7.66M on LEZ (derived, not run). The copy's image id changed with this
change (`a097e928…` in §6's run, `adbf28c8…` in this build; both depend on the build path, and neither
is in `docs/image-ids.md`); the deployed program's, printed by the same test, is still `816d1fef…`.

### 9.7 What an auditor must check (in addition to §7)

1. `multiproof_root`: the pairing rule (a known sibling is computed, never read), the order the
   witness is read in, that it is read to its end, the fixed depth, strictly increasing leaf indices
   (they come from the bitfield), and `multiproof_siblings` as its exact counterpart.
2. `compressed`: the constant `(p − 1)/2` and that it is used only on encodings `key_point` accepted.
3. `key_point` against the core's `witness_key`: the same accepted set (tested on 352,256 encodings).
4. `hash64`: the initial state, the padding block, the cast of the aligned buffer to two
   `GenericArray` blocks, and RISC Zero's `sha2` patch of `compress256` in the guest.
5. `blst_p1s_add`: blst's `bulk_addition.c` (not constant time: no secret is involved; scratch on the
   stack with `alloca`, about 24 KB for 170 points); its doubling and identity cases.
6. `residue_agg::step1` against `ferrata_core::steps::step1` at 510f05cf: `diff` shows the one change.
7. The marker of the experimental copy, and that the copy refuses what the core refuses.

### 9.8 What this does not change

The deployed programs, their image ids and the deployed update path (three transactions, five with an
adoption) are unchanged; the policy is unchanged (342 signers is still its floor). As for §8, the
production change would be a new program image, deployment and real proofs, the multiproof witness
in the updater (host code: `NonSigners::build`), and an audit of both the residue check and this
composition. A **period update** still does not fit at any participation (folded: 11,913,877 at 510,
13,482,391 at 342 in the bare guest); the job path stays for it. Re-check:
`cd bench && cargo test --release -p residue-agg && cargo build --release -p residue-host &&
./target/release/residue-host profile && ./target/release/residue-host measure-agg &&
./target/release/residue-host boundary`; `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --features
residue-experimental --test residue_experimental -- --nocapture --test-threads 1`.
