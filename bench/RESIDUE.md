# The residue-witness check: one light-client update in one LEZ transaction?

**Experimental and unaudited. No deployed program uses it.** The light-client program, the bridge
and every other guest with an image id (`docs/image-ids.md`) still compute the final
exponentiation; nothing in this note changed them. Everything here is a bench measurement of the
core's check (`bench/residue`, `bench/guests/residue-accel`), plus one run through LEZ's state
machine of an experimental copy of the program (§6), made on 17 Nov 2026; §9 (18 Nov) adds one multiproof for the
non-signers, and §10 (18 Nov) a committee handoff in one transaction, which is sound only under the light client's
honest-supermajority assumption.

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
* It does **not** fit at the lowest participation the policy accepts (342 of 512), even folded: each
  of the 170 non-signers costs 25,213 cycles (its key and its Merkle branch to the committee root in
  step 1, and its 384 bytes of witness), 4.3M in all, and on LEZ its input costs about 0.37M more
  (§6). **With the non-signers' keys checked through one multiproof instead (§9), it fits at every
  placement of the absent members that was measured**: LEZ's state machine charged an experimental
  copy 8,318,645 execution gas (83.2%) for it with every third member absent and 8,326,572 (83.3%)
  at the costliest placement found (the maximum over all placements is not established, §9.6); bare
  guest 7,458,783 folded (every third absent). A **period update does not fit at any participation**: adopting 512 keys costs 5,991,991
  cycles on top of the verification.
* **A period update (a committee handoff) in one transaction** (§10, 18 Nov 2026): with the next committee adopted by
  its summary (its key root and aggregate key as the update's branch proves them in the signed attested state) instead
  of its 512 keys, which is sound only under the honest-supermajority assumption the light client already makes,
  LEZ's state machine charged an experimental copy **6,621,551** execution gas (66.2%) for mainnet update_1 as a
  handoff and **8,341,469** (83.4%) for a 342-signer handoff at the costliest placement found; the bare guest takes
  6,578,036 and 8,154,987 cycles. The rows of the table above with the 512-key adoption stand: that adoption alone is
  5,991,991 cycles.
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
of what depends on it, builds the experimental guest. Its methods crate is a member of the `lez/`
workspace, so `cargo build --workspace` (or `cargo test --workspace`) in `lez/` does build the
experimental guest; the documented commands and the CI jobs, which name their packages with `-p`
and do not set the feature, do not.

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

Measured on 17 Nov 2026, evening; other placements of the absent members on 18 Nov 2026, after an
independent review. **Experimental and unaudited, a bench measurement and an
experimental copy of the program; nothing deployed changed** (no program or guest with an image id
in `docs/image-ids.md`, and `crates/ferrata-core`, are untouched).

**Verdict.** With the signing key composed from one Merkle multiproof for all the non-signers
(`bench/residue-agg`) instead of a branch per non-signer, a finality update signed by the fewest
members the policy accepts (342 of 512) fits one LEZ transaction at every placement of the 170
absent members that was measured. With every third member absent (the placement that needs the most
hashes, 425, and the most witness nodes, 256) LEZ's state machine charged the experimental copy of
the program **8,318,645** execution gas (**83.2%** of `MAX_GAS_EXEC`) with the residue-witness check
(§6). That is not the costliest placement: of the 12 placements run through LEZ the costliest has
every absent index odd, **8,326,572** gas (83.3%, 7,927 more), and the cheapest (the first 170
members absent) 8,013,965. The maximum over all placements and committees is not established; the
costliest measured leaves about 1.67M gas to the limit (§9.6). With the non-signers summed by the
core's chain of additions instead of blst's batch addition (exact for any points; the batch sum is
exact for keys in G1, §9.3 (v)) the charge is 8,653,530 (86.5%) with every third absent and 8,661,457
(86.6%) at the odd placement. The same update with a branch per non-signer reverted on the cycle
budget (10M). In the bare guest the whole update with every third member absent costs 8,120,369 user
cycles with the witness check and **7,458,783** folded, against 10,788,327 and 10,126,621 with the
core's composition in the same build (the folded figures are the bare guest's counts: the LEZ copy
does not fold the check, and no folded run was made through LEZ).
A non-signer now costs 9,418 cycles instead of 25,094. The composition makes the same checks on
every key; §9.3 gives the argument. A period update still does not fit (13,482,391 folded at 342,
11,913,877 at 510: the adoption of 512 keys is 5,991,991). With a branch per non-signer the fewest
signers that fit are 386 through LEZ's state machine (bare guest: 348 folded, 374 with the witness
check); with one multiproof the policy's floor of 342 fits at every placement measured, and in the
bare guest so does every number of absent members from 1 to 170 (evenly spaced).

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
| the branch: 9 SHA-256 nodes of 64 bytes (1,247 each) | 11,776 | 53% |
| the sum: one `blst_p1_add_or_double_affine` | 4,839 | 22% |
| bookkeeping (absent indices, count and length checks) | 44 | 0% |
| **step 1, per non-signer** (the whole composition: 3,857,505 for 170, less the once-only 96,448) | **22,124** | |
| read and decode its 384 bytes of witness (`SignerWitness::decode`: 469,468 for 170) | 2,762 | |

There is **no subgroup check** of a non-signer's key in the core (nor of any key at adoption:
`witness.rs`, "it leaves out the per-key subgroup check", as `check_committee_onchain`); for
comparison, `blst_p1_affine_in_g1` would cost 352,513 cycles per key. **Nothing else depends on the
participation**: at 512, 511, 510, 450, 400, 343 and 342 signers the Miller loop (3,275,068 in this
section's guest) and the final check (1,108,212; folded 3,722,450 together: `S2`, `S3w` and `S23f` in
`results/residue/agg/one-tx.txt`) are the same, the job's encoding and application
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
   of opposite points (the identity), but not the identity paired with a point of order 3, (0, ±2), in
   a later round: the identity is stored as (0, 0), so its pairwise step sees equal x-coordinates with
   y-coordinates that do not cancel and computes a doubling. It is
   exact for keys in G1, every Ethereum committee's (§9.3 (v)). The chain of mixed additions
   (`Sum::Chain`, the core's, exact for any points on the curve) is the conservative variant, measured
   too.
4. **SHA-256 of 64 bytes as two compressions** (`hash64`, `node`, `leaf`): the message and the fixed
   padding block in one word-aligned 128-byte buffer, `sha2::compress256` (in the guest RISC Zero's
   accelerator, one call for both blocks), instead of the streaming hasher, whose blocks RISC Zero's
   `sha2` patch copies when they are unaligned.

The witness (`NonSigners`): a `u16` key count, the keys (96 bytes each, in index order), a `u16` node
count, the nodes (32 bytes each); strict (each count at most the committee size, the input consumed
exactly). The `u16` counts hold every witness of a committee of fewer than 2^16 members (an honest
multiproof needs fewer nodes than members; 65,536 keys would not fit the count, and
`NonSigners::encode` panics on them); the mainnet committee has 512. At 342 signers it is at most 24,516 bytes (every third member absent; 22,756 at random
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
over Fp has odd order). So `P_i` is the core's point for index `i`. (v) *The sum*: the chain
(`Sum::Chain`) is the core's own additions, exact for any points on the curve, and `A − Σ P_i` is the
core's key. `blst_p1s_add` (`Sum::Batch`) is exact unless, after its first round, one of its pairwise
sums is a point of order 3, (0, ±2), and is paired with the identity (stored as (0, 0)): its pairwise
step (`HEAD`) then sees equal x-coordinates and computes a doubling. Keys in G1 sum to points of G1,
which has prime order r ≠ 3 and so no point of order 3; every Ethereum committee's keys are in G1
(deposits pass KeyValidate), so for them the batch key is the core's. Neither composition checks that
(the core makes no subgroup check on committee keys). For a committee built with keys outside G1 the
batch key can differ from the core's by a point of order 3, and the verdict is still the same: every
point of order 3 lies in rE(Fp) (3 ∤ r), on which the reduced pairing is trivial; the test
`batch_sum_outside_g1_differs_by_an_order_3_point_and_keeps_the_verdict` builds such a committee and
finds the keys differing by (0, ±2), a valid signature accepted and a wrong message refused under
both. (vi) *Completeness*: `multiproof_siblings` writes exactly the
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
| (a) one multiproof for all non-signers | **kept**: sound (§9.3), tested | the branch work 11,776 → 2,460 (425 hashes for 170, at 984 with bookkeeping; random positions 2,141); witness 65,282 → 24,516 bytes |
| (b) the smaller side | nothing to gain: under the floor the non-signers are always fewer | none |
| (c) batched affine additions | **kept**: blst's own `blst_p1s_add`, exact for keys in G1 (every Ethereum committee; outside G1 its key can be off by a point of order 3, with the same verdict, §9.3 (v)); the core's chain, exact for any points, stays as the conservative variant: 342 on LEZ 8,653,530 gas (86.5%) with every third absent, 8,661,457 (86.6%) at the costliest placement found (§9.6) | the sum 4,839 → 2,869 |
| (c') an affine chain with one inversion per addition (inversion costs 244 cycles here) | **not done**: about 1,200 per addition by the unit costs, but new elliptic-curve code with its own edge cases to audit, for about 0.3M | estimated, not measured |
| (d) dropping a check | **none dropped** (§9.3) | none |
| the compressed key from the bytes | **kept**: equal to blst's on every accepted encoding | 1,173 → 307 |
| SHA-256 as two compressions | **kept** | 1,247 → 811 per node |
| decoding the key without blst (word-wise bytes, RISC Zero's bigint accelerator) | **not done**: more custom code for at most about 2,000 per key | not measured |

### 9.5 Tests

All on the Modal build box (`cargo test --release -p residue-agg`, 9 tests in `tests/agg.rs`, 22.3 s, and 1 in `tests/batch_g1.rs`, 9.6 s), 17 Nov 2026;
the existing suites unchanged and passing (`cargo test --release -p residue`; the harness's test of
§6). On 18 Nov 2026 an independent review's tests were added (the last three rows) and run on the
same box with `cargo test --release -p residue-agg` (9 + 1 + 7 tests pass: 21.5 s, 9.1 s, 6.1 s), the
harness's `residue_experimental` (5 pass, every figure of `lez.txt` printed again unchanged) and
`residue-host profile`, `measure-agg` and `boundary` (every figure unchanged).

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
| batch sum on colliding G1 points | `tests/batch_g1.rs`, a second check written apart from the rest: `blst_p1s_add` against the chain of mixed additions and against one scalar multiplication per distinct point, on multisets of 1 to 200 keys drawn with repetition from at most eight G1 points and their negations (doublings, cancelling pairs and the identity as an intermediate sum, in every round), and five structured families at every length to 170 | 20,850 multisets (19,329 of 16 or more points, 292 summing to the identity) | all three equal |
| batch sum outside G1 | a committee with one key K + T (T = (0, 2), of order 3) and −K, members 2 to 31 cancelling in pairs, all 32 absent; a valid signature and a wrong message | 2 | the batch key differs from the core's (= the chain's) by (0, ±2); the same verdict under both |
| guest | `residue-host profile`: the three compositions give the same key, `compressed` and `leaf` equal blst's and the core's on every key, the multiproof gives the key root, `node` equals `hash_pair` on a chain of 1,000 (4 bitfields); `residue-host measure-agg` and `boundary`: every one of the 70 executions gives the native new store (the signature verified with the key composed in the guest); 3 refusals in the guest (a node altered, a key outside the committee, another bitfield's witness: code 80) | 4 + 70 + 3 | pass |
| LEZ | `lez/harness/tests/residue_experimental.rs`: 510 and 342 of 512 applied with one multiproof (batch sum), 342 also with the chain of additions, the store equal to the native one; at 342 a branch per non-signer over the budget and one altered node refused (exit 80); the fewest signers that fit with a branch per non-signer | 4 tests (and the test of §6, unchanged) | pass |
| edge cases (18 Nov, from the review) | `tests/edge_cases.rs`, with references built in the test (output `results/residue/agg/edge-cases.txt`): every non-empty leaf set at depths 1 to 4 against one branch per leaf, with tamperings; committees of 1, 2, 3, 5, 6, 7, 9, 12, 32, 33, 100, 300, 511 and 513 members; the identity as the non-signers' sum, as the signing key and as the aggregate's encoding, an aggregate that does not decode, the empty set with a node, 61 copies of one key; `NonSigners::decode` at its edges; `blst_p1s_add` on G1 multisets to 600 points and on 1,100 distinct points; outside G1, random multisets of curve points shifted by the points of order 3; `compressed` on curve points outside G1 | 65,808 leaf sets, 142,020 tamperings; 224 bitfields; 6,286 G1 multisets (3,130 summing to the identity); 4,000 multisets outside G1; 20,000 points | round trips and refusals as specified; in G1 batch = chain = scalar multiplications; outside G1 435 of the 4,000 differ from the chain, always by (0, ±2), the pairing with a G2 point equal in every case |
| placements, bare guest (18 Nov, from the review) | `residue-host/tests/placements.rs` (§9.6): the 342-signer update at 26 placements of the absent members, five of them also with the chain of additions, and 1 to 170 members absent, evenly spaced; the guest's phases for seven placements with 425 hashes; 171 absent | 248 accepted executions, each giving the natively applied job's store; 5 refusals in the guest (code 51) and natively by both compositions | pass (the cycle counts are written, not asserted) |
| placements, LEZ (18 Nov, from the review) | `lez/harness/tests/residue_placements.rs` (§9.6): the 342-signer update at 12 placements with the batch sum and two with the chain; 341 signers under either marker and with the core's witness; the multiproof witness without its marker, the core's witness behind a marker, the marker twice; every other two-byte prefix before the multiproof witness | 14 transactions applied, the store the natively applied job's; 3 refused with exit 51, 3 with exit 1, the store unchanged; 0 of 65,534 prefixes give a valid `SignerWitness` | pass (the gas is printed, not asserted) |

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
| 342 (every third absent), the multiproof with the core's chain of additions | — / 7,793,334 | — / 8,454,920 | | — / 24,516 |

Execution user cycles of the whole execution (`SessionInfo::cycles`). With the core's composition
this guest reproduces §5's figures within 549 cycles at 510 and 512 signers; at 342 it gives about
20,800 fewer (10,126,621 against 10,147,477 folded: another guest binary). Per non-signer (342, every third
absent, against 512): **9,418** cycles in all (step 1 9,224, decoding the witness 181, reading it 13)
instead of 25,094 (step 1 22,298, decoding 2,760, reading 35); at random positions 9,090.

**Where the absent members sit** (18 Nov 2026, after an independent review; bare guest:
`residue-host`'s test `tests/placements.rs`, outputs [`placements.md`](results/residue/agg/placements.md)
and `placement-phases.txt`; through LEZ: `lez/harness/tests/residue_placements.rs`, output
`results/residue/agg/lez-placements.txt`). Every third member absent is the placement with the most
hashes (at most the sum over the levels of min(k, 2^ℓ), 425 for k = 170) and the most witness nodes
(256). It is **not the costliest placement**. At 342 signers, with the witness check and blst's batch
sum:

| placement of the 170 absent members | hashes / witness nodes | bare guest, user cycles | LEZ, execution gas |
|---|---:|---:|---:|
| every third (2, 5, …, 509) | 425 / 256 | 8,120,369 | 8,318,645 (83.2%) |
| every absent index odd (in each group of four members 4g … 4g+3 member 4g+3, and in groups 0 to 41 also 4g+1): the costliest found | 425 / 256 | 8,128,806 (+8,437) | **8,326,572** (83.3%, +7,927) |
| every absent index even (4g, and in groups 0 to 41 also 4g+2) | 425 / 256 | 8,111,790 (−8,579) | 8,310,576 (−8,069) |
| evenly spaced (the boundary's placement) | 425 / 256 | 8,120,212 (−157) | 8,318,494 (−151) |
| the first 170 members (one block): the cheapest found | 174 / 5 | 7,866,334 (−254,035) | 8,013,965 (−304,680) |

Placements tried in the bare guest: 26 (nine with 425 hashes and 256 nodes: 8,111,790 to 8,128,806;
the odd indices 1 to 339: 8,042,978; four of one or two blocks: 7,866,334 to 7,871,363; 12 random:
8,052,553 to 8,076,856). Through LEZ: 12 (five of those with 425 hashes and 256 nodes: 8,310,576
to 8,326,572 gas; the first and the last 170: 8,013,965 and 8,013,973; five random: 8,246,845 to
8,260,343); every one was applied and left the natively applied job's store. Among seven placements
with the same 425 hashes and 256 nodes the whole difference is in step 1 (`placement-phases.txt`),
ordered by where the absent members sit in their groups of four: 4g+3 (+8,437 and +8,423), 4g+1
(+2,802), 4g+2 (−2,788), 4g (−8,579). With the chain of additions the odd placement costs the same
amount more than every third: 8,463,357 cycles in the bare guest, 8,661,457 gas on LEZ (86.6%).
**The maximum over all placements and committees is not established**: the placements were chosen,
not searched, and what depends on the keys themselves was not bounded. The costliest measured leaves
1,673,428 gas (about 1.67M) to the limit.

**How many are absent.** The cost does not grow at every step with the number of non-signers. With
1 to 170 members absent, evenly spaced (the most hashes for each count), the bare guest's whole
update with the witness check costs less than with one member fewer absent at 16 (−6,713), 32
(−11,656), 64 (−17,074), 128 (−22,639: 7,800,472 at 127, 7,777,833 at 128), 144 (−30) and 160
(−1,214) absent (`placements.md`). Each of these is a count at which blst's batch addition adds fewer
points one by one: it works in rounds, adding the points in pairs with one inversion per round while
16 or more remain (an odd one singly) and the fewer than 16 left one by one (blst's
`bulk_addition.c`), so that 127 points take 18 single additions and 128 take 8 (the column "single
additions" of `placements.md`: that rule, not a measurement). Over 1 to 170 absent the maximum is at
170 (8,120,212 for evenly spaced absences).

**The fewest signers that fit one transaction** (`residue-host boundary`, `results/residue/agg/boundary.md`;
absent members evenly spaced):

| path | a branch per non-signer (before) | one multiproof (after) |
|---|---|---|
| witness folded, bare guest | **348** (9,976,122; 347: 10,001,188) | **342**, the policy's floor (7,458,783 with every third absent; 7,467,220 at the costliest placement found) |
| witness check, bare guest | **374** (9,985,559; 373: 10,010,661) | **342** (8,120,369 with every third absent; 8,128,806 at the costliest placement found) |
| witness check, LEZ's state machine (the copy of §6) | **386** (9,985,702 gas; 380 to 385: over the 10M budget) | **342** (8,318,645 gas with every third absent; 8,326,572 at the costliest placement found) |
| final exponentiation, bare guest | none at or below 512 | above 410 (410: 10,076,335): the final exponentiation alone is 3.6M |

**Through LEZ's state machine** (`lez/harness/tests/residue_experimental.rs`, in-process harness, LEZ
v0.4.0-rc1, as §6; output `results/residue/agg/lez.txt`): the experimental copy now also takes a
signer witness that starts with the marker `0xffff` or `0xfffe` (see below) as a `NonSigners`
witness, composed by `residue_agg::step1` with blst's batch addition or with the chain of additions;
any other witness goes to the core's step 1 as before. For the 342-signer update the instance's store is given the
bench's committee after update_0, as in the bench (§5), with LEZ's `force_insert_account`.

| transaction | handlers run (cycles each) | execution gas | of `MAX_GAS_EXEC` | storage gas | result |
|---|---|---:|---:|---:|---|
| 342 of 512, a branch per non-signer + residue witness | `VerifyAll` | 10,000,000 | 100% | 69,769 | reverted: "Execution exceeded its cycle budget of 10000000 cycles" |
| 342 of 512, one multiproof with one node altered | `VerifyAll` 1,494,785 | 1,494,785 | 14.9% | 29,005 | refused: exit 80 |
| **342 of 512, one multiproof + residue witness** | `VerifyAll` 8,243,106 → `ApplyVerified` 58,187 → `Record` 17,352 | **8,318,645** | **83.2%** | 29,005 | applied; the store is the natively applied job's, the record is written |
| 342 of 512, one multiproof summed by the chain of additions + residue witness | `VerifyAll` 8,577,991 → 58,187 → 17,352 | 8,653,530 | 86.5% | 29,005 | applied; the store is the natively applied job's |
| 510 of 512 (mainnet), one multiproof + residue witness | `VerifyAll` 6,531,161 → 58,187 → 17,352 | 6,606,700 | 66.1% | 5,037 | applied; the store equals the native store |
| 510 of 512 (mainnet), a branch per non-signer + residue witness (§6's transaction) | `VerifyAll` 6,553,796 → 58,187 → 17,352 | 6,629,335 | 66.3% | 5,257 | applied (§6: 6,629,246 with the copy before this change) |

At 342 signers (every third absent) LEZ charges 198,276 more than the bare guest's whole update
(8,120,369): the copy's `VerifyAll` alone is 122,737 above it, and the two handlers after it take
58,187 and 17,352 (`lez.txt`). The difference is not a fixed charge per input byte: at 510 signers the
copy's `VerifyAll` (6,531,161) is 20,694 below the bare guest's whole update (6,551,855). Folded into
the Miller loop (not in the copy) the bare guest saves another 661,586 at 342, so about 7.66M on LEZ
(derived, not run). The 342-signer rows above are for every third member absent; the costliest
placement found was charged 8,326,572 (83.3%; "Where the absent members sit", above). The copy's image
id changed with this change to its code (`a097e928…` in §6's run, `4f1b2fa6…` in this build and in the
re-run of 18 Nov; neither is in `docs/image-ids.md`); the deployed program's, printed by the same
test, is still `816d1fef…`, the id `docs/image-ids.md` lists. The
copy reads `0xffff` as a `NonSigners` witness summed by blst's batch addition and `0xfffe` as one
summed by a mixed addition each; both markers (65,535 and 65,534 as counts) are counts no committee of
at most 65,533 members has (mainnet: 512). The copy's `check_config` takes only committee sizes that
are powers of two; the next one that could have such a count is 65,536, and for a committee that large
a marker could be the count of a valid `SignerWitness` (the copy is for 512).

### 9.7 What an auditor must check (in addition to §7)

1. `multiproof_root`: the pairing rule (a known sibling is computed, never read), the order the
   witness is read in, that it is read to its end, the fixed depth, strictly increasing leaf indices
   (they come from the bitfield), and `multiproof_siblings` as its exact counterpart.
2. `compressed`: the constant `(p − 1)/2` and that it is used only on encodings `key_point` accepted.
3. `key_point` against the core's `witness_key`: the same accepted set (tested on 352,256 encodings).
4. `hash64`: the initial state, the padding block, the cast of the aligned buffer to two
   `GenericArray` blocks, and RISC Zero's `sha2` patch of `compress256` in the guest.
5. `blst_p1s_add`: blst's `bulk_addition.c` (not constant time: no secret is involved; scratch on the
   stack with `alloca`, about 24 KB for 170 points); its doubling and identity cases, and that it is
   exact only when no later-round pairwise sum is a point of order 3 (keys in G1), §9.3 (v); or use the
   chain (`Sum::Chain`), exact for any points, at 335K cycles more at 342 signers.
6. `residue_agg::step1` against `ferrata_core::steps::step1` at 510f05cf: `diff` shows the one change.
7. The marker of the experimental copy, and that the copy refuses what the core refuses.
8. The costliest update the policy accepts: the maximum over the placements of the absent members
   and over committees is not established (§9.6); the costliest measured was charged 8,326,572 gas.

### 9.8 What this does not change

The deployed programs, their image ids and the deployed update path (three transactions, five with an
adoption) are unchanged; the policy is unchanged (342 signers is still its floor). As for §8, the
production change would be a new program image, deployment and real proofs, the multiproof witness
in the updater (host code: `NonSigners::build`), and an audit of both the residue check and this
composition. A **period update** still does not fit at any participation (folded: 11,913,877 at 510,
13,482,391 at 342 in the bare guest); the job path stays for it. Re-check:
`cd bench && cargo test --release -p residue-agg && cargo build --release -p residue-host &&
./target/release/residue-host profile && ./target/release/residue-host measure-agg &&
./target/release/residue-host boundary && cargo test --release -p residue-host --test placements`;
`cd lez && export RISC0_HOME=$PWD/../../risc0-home-lez && RISC0_DEV_MODE=1 cargo test -p harness
--features residue-experimental --test residue_experimental -- --nocapture --test-threads 1`, and the
same with `--test residue_placements` (`RISC0_HOME`: the LEZ guest toolchain, as in §6; the charged
gas depends on the guest binary it builds).

## 10. A committee handoff in one transaction: the next committee adopted by its summary

Written on 18 Nov 2026. **Experimental and unaudited, a bench measurement and two experimental program
copies; it is sound only under the honest-supermajority assumption the light client already makes (at
least two thirds of each sync committee honest, [`docs/trust.md`](../docs/trust.md) §2.2), and nothing
deployed changed** (no program or guest with an image id in `docs/image-ids.md`, and `crates/ferrata-core`,
are untouched).

**Verdict on soundness.** Sound under that assumption. A period update (a handoff) carries the next
committee as its summary, the root of its 512 key leaves (`pubkeys_root`) and its aggregate key
(`aggregate_pubkey`), proven by `next_sync_committee_branch` in the state of the attested header that at
least 342 of the 512 members of the signing committee signed (the policy's floor). The core adopts that
summary only after a witness of the 512 uncompressed keys has shown that every key decodes and that the
keys hash to the root and sum to the aggregate (`witness.rs`, `AdoptionProgress::add_part`). The summary
adoption takes the proven summary as it is, in the same transaction as the update, and keeps every check
made at use time: each non-signer's key is decoded canonically and proven against `pubkeys_root` (the
core's composition or the multiproof of §9), and the aggregate is decoded. On every state the beacon
chain's state transition produced, the two adopt the same summary: the aggregate there is the sum of the
keys by construction, and every key passed `KeyValidate`. They differ only on a state in which the
aggregate field is not the sum of the keys, which only 342 or more members of a committee, signing a state
the transition did not produce, can bring; those signers can as well bring a committee of keys of their
own whose aggregate is right, and the core's adoption accepts that. So the core's check refuses only a
forgery no forger needs to make, and the summary adoption does not weaken the light client under its
assumption. Without the assumption the light client is not sound with either adoption.

Sources (read-only copies beside the repository under `spike/upstream/`): consensus-specs v1.7.0-beta.4
(commit `aa16bb4c`, the version of the pinned `sync` vectors); the IETF BLS signature draft 04, which the
specification names (`draft-irtf-cfrg-bls-signature-04.txt`, sha256 `3836b050…`); Helios tag 0.12.0
(commit `2af9b418`); Lodestar tag v1.51.0 (commit `7923280c`; its light client now lives in
`packages/state-transition/src/lightClient/`). Paths below are relative to each.

### 10.1 What the core's adoption establishes that the summary adoption does not

When applying an update adopts a committee (`apply_core_compact`: a store learns its next committee, or
rotates and learns the one after), the core's `step1` gives the job an `AdoptionProgress` for the
update's next-committee summary, and `apply` refuses the job until the adoption is complete
(`AdoptionIncomplete`, 83). The adoption (`AdoptionProgress::add_part`) takes the 512 keys uncompressed
and establishes, up to SHA-256 collisions:

* **(A1)** every leaf under `pubkeys_root` is the leaf of the compressed form of a point on the curve that
  is not the identity, given in canonical encoding (no subgroup check: `witness.rs`, as for every key the
  core reads);
* **(A2)** `aggregate_pubkey` is the compression of the sum of those 512 points.

It does not establish where the committee comes from. In both designs that is the branch to the attested
state root (`validate_compact`, refusal `NextCommitteeBranchInvalid`) and the signature over the
attested header by the committee the store already holds. (A1) and (A2) are the internal consistency of a
summary that is trusted for its provenance either way. The summary adoption keeps the provenance and drops
(A1) and (A2).

What the light client does with the summary afterwards: the key that verifies an update signed by the
adopted committee is `aggregate_pubkey − Σ non-signers` (`steps.rs`, `signing_key`;
`residue_agg::signing_key`). (A2) is what makes that the specification's key, the sum of the signers'
keys. (A1) is not used by the composition: a signer's key is never decoded (it is inside the aggregate),
and a non-signer's key is decoded where it is used, by the same checks as (A1) (`witness_key`,
`residue_agg::key_point`), and refused otherwise (80).

### 10.2 Why the difference does not matter under the light client's assumption

The assumption: at least two thirds of each sync committee are honest, so at most 170 of 512 members are
not, and any 342 signatures include signatures of honest members. An honest member signs the root of the
block it holds as its head (`get_sync_committee_message`, `specs/altair/validator.md:363-375`, signing
`block_root` at l. 368) and holds a block only if its `state_root` is the root of the state the state
transition computes (`state_transition`, `specs/phase0/beacon-chain.md:1822-1835`, the assertion at
l. 1835). The light client already relies on this for the finalized header it records and for
`pubkeys_root`. The aggregate field is in the same `SyncCommittee` container, under the same branch
(`CommitteeSummary::root` is `hash(pubkeys_root ‖ leaf(aggregate_pubkey))`): it is exactly as trustworthy
as `pubkeys_root`.

In a state the transition produced, `next_sync_committee` was set at a period boundary by
`get_next_sync_committee` (`specs/altair/beacon-chain.md:356-363`), which computes
`aggregate_pubkey = eth_aggregate_pubkeys(pubkeys)` (l. 362). `eth_aggregate_pubkeys`
(`specs/altair/bls.md:36-52`) asserts `bls.KeyValidate` of every key (l. 47) and returns their sum
(l. 49-52). `KeyValidate` (draft 04 §2.5, l. 578-603; the draft and ciphersuite the specification names
at `specs/phase0/beacon-chain.md:1070-1084`) accepts a key only if it decodes to a point that is not the
identity and is in the subgroup G1. So in every such state (A1) holds, in a stronger form (G1
membership, which the core's adoption does not check), and (A2) holds by construction: the core's
adoption would pass, and the summary adoption adopts the summary the core adopts. The store after the
update is the same (tested on every handoff of the fixtures, §10.7).

The beacon chain itself uses this field for the same purpose. The state transition verifies every
block's sync aggregate (`process_sync_aggregate`, `specs/altair/beacon-chain.md:635-672`) with
`state.current_sync_committee.aggregate_pubkey` as the key when all members participated (l. 640-641)
and with `aggregate_pubkey − Σ non-participants` when more than half did (l. 643-655), and no step of the
transition checks the field against the keys after `get_next_sync_committee` computed it. A node of the
pinned specification trusts the field of its own state as the summary adoption trusts it in a signed
state.

### 10.3 What a wrong aggregate field would do

It needs a state the transition did not produce, signed by at least 342 members of the committee the
store already holds: the assumption has failed for that committee. If the adopted summary's aggregate is
`Σ P_i + D` with `D ≠ 0` (the `P_i` the keys under `pubkeys_root`):

* every later update signed by the adopted committee is checked against `Σ signers + D`: the honest
  committee's signatures are refused (`SignatureInvalid`), and an aggregate field that does not decode
  refuses every such update (`PubkeyInvalid`). The instance stalls until a new `Init` from a trusted
  checkpoint;
* whoever chose `D` signs for that committee alone: with `aggregate_pubkey` set to a key `Q` it holds,
  an update with every bit set is verified against `Q`. The test of §10.7 builds the refused honest
  committee and the one member signing for all (not the aggregate that does not decode).

The same signers can instead sign a state whose next committee is 512 keys of their own with the right
aggregate. The core's adoption accepts that committee (it checks consistency, not provenance) and they
hold the light client from then on, with either adoption. A wrong aggregate field gives them nothing that
a consistent committee of their own does not. So the summary adoption changes what is accepted only on
updates from states the transition did not produce: there the core refuses at adoption
(`CommitteeInvalid`, 45) and the one-transaction path accepts (tested, §10.7). It changes nothing on an
update of a real chain.

### 10.4 Could a key of a real committee be invalid?

Not in a state the transition produced, for two independent reasons.

* A key enters the validator registry only with a valid proof of possession: `apply_deposit`
  (`specs/phase0/beacon-chain.md:2525-2546`, "Verify the deposit signature (proof of possession) which is
  not checked by the deposit contract", l. 2534, `bls.Verify` at l. 2543), and since Electra
  `apply_deposit` (`specs/electra/beacon-chain.md:1754-1768`) and `apply_pending_deposit` (l. 1108-1121)
  through `is_valid_deposit_signature` (l. 1786-1797, `bls.Verify` at l. 1797). In the proof-of-possession
  scheme `Verify` is `CoreVerify` (draft 04 §3.3, l. 930-931), which returns INVALID unless
  `KeyValidate(PK)` (§2.7 step 4, l. 655).
* Whatever the way a validator entered (a test network's genesis tooling, for one), the committee's
  aggregate is computed by `eth_aggregate_pubkeys`, which asserts `KeyValidate` of every key of the
  committee (`specs/altair/bls.md:47`): a state that holds a committee with a key that is not a valid G1
  point is not a state the transition produced.

Every committee the tests adopt (§10.7) also passes the core's 512-key adoption: none of the fixtures'
committees has a key the core refuses.

### 10.5 What the specification, Helios and Lodestar do with the aggregate field

| | how the next committee is adopted | what is done with `aggregate_pubkey` |
|---|---|---|
| specification, light client (`specs/altair/light-client/sync-protocol.md`) | the whole committee, after the branch (l. 458-467), stored as it is (`apply_light_client_update`, l. 494-504): no check of the aggregate against the keys, no `KeyValidate` at adoption | never read: the signature is checked by `FastAggregateVerify` over the participants' keys taken from the stored committee (l. 470-488) |
| specification, state transition (`specs/altair/beacon-chain.md`) | computed by `get_next_sync_committee` (l. 356-363) | the verification key of every sync aggregate: the field itself when all participate (l. 640-641), the field minus the non-participants when more than half do (l. 643-655) |
| Helios 0.12.0 (`ethereum/consensus-core/src/`) | the whole committee after the branch (`consensus_core.rs:324-349`), stored as it is (`consensus_core.rs:208-221`) | the verification key when more than half participate: the field minus the non-participants (`get_participating_aggregate_pubkey`, `utils.rs:60-84`, called at `consensus_core.rs:353-362`); never checked against the keys, so a committee whose field is not the sum makes it refuse that committee's honest updates (the departure pinned in [`docs/design/attestation.md`](../docs/design/attestation.md), "The Helios departures") |
| Lodestar v1.51.0, light client (`packages/state-transition/src/lightClient/spec/`) | the whole committee, every key and the aggregate deserialized by `PublicKey.fromBytes(…, true)` (`utils.ts:59` and `66-71`; the meaning of `true` is in the `lodestar-z` bindings, not read here), adopted at `processLightClientUpdate.ts:95-96` | not used to verify: `fastAggregateVerify` over the participants (`validateLightClientUpdate.ts:103-113`); never checked against the keys. Lodestar's state transition computes the field (`epoch/processSyncCommitteeUpdates.ts:36-39`) and verifies sync aggregates over the participants' indices (`block/processSyncCommittee.ts:101-125`) |
| Ferrata's core | the summary after the branch, and the 512-key witness: (A1), (A2) | the verification key: the field minus the non-signers |
| this section (experimental) | the summary after the branch | the verification key: the field minus the non-signers, each non-signer's key proven against `pubkeys_root` and decoded |

None of the three checks the aggregate field against the keys. The specification's state transition and
Helios use it as the verification key, as Ferrata does; with the summary adoption Ferrata trusts it as
Helios does, and still proves against the key root every non-signer key it subtracts (Helios takes them
from the committee it stored after the branch, which is equivalent up to SHA-256 collisions).

### 10.6 What changed

* `bench/residue-handoff` (new; `src/lib.rs`): `adopt_summary(job)` drops the job's pending
  `AdoptionProgress` when the job applies an update whose next committee is the one the progress
  names (the summary `steps::step1` took from the update, after `validate_compact` checked its branch);
  the core's `steps::apply` then writes that summary into the store, as after the core's adoption. It
  refuses (`NoAdoption`, 84) a job that adopts nothing, one whose adoption is complete, and an
  initialization: the bootstrap's committee keeps the core's 512-key adoption (its trust is the
  checkpoint, not a signature; not covered by this note). `one_step` runs a whole update as the
  experimental programs do: `residue_agg::step1` (one multiproof), `Job::step2`, the residue-witness
  check (`residue_check`, any failure `SignatureInvalid`, 40), `adopt_summary` if the update adopts a
  committee, and `steps::apply`. Nothing of the core changed: its `Job`, `step1`, `apply` and the
  adoption are used through their public API.
* The bench guest `guests/residue-agg-accel`: a period update given no adoption keys is adopted by its
  summary (`A2`); given the keys, by the core's adoption as before. `residue-host measure-handoff`
  (`src/handoff.rs`) measures it.
* The LEZ v0.4 experimental copy (`lez/guests/program-residue-experimental`, behind the harness's
  feature `residue-experimental`, as §6): a `VerifyAll` of an update that adopts a committee, given an
  empty `adoption`, adopts by the summary; given the 512 keys, as before. The wire (`ferrata-lez-wire`)
  is unchanged, so the deployed program's image id is unchanged (printed by the test, §10.8).
* The LEZ v0.3.1 port (`lez-v03/`, not deployed anywhere): a new `Verify::ResidueSummary { c, w }`,
  appended to its wire's `Verify`, does the same in its `Update`; its image id changes (§10.8).

### 10.7 Tests

All on the Modal build box, 18 Nov 2026 (`cargo test --release -p residue-handoff`: 4 tests pass in 3.4 s;
output [`results/residue/handoff/tests.txt`](results/residue/handoff/tests.txt)).

| test | what | count | result |
|---|---|---:|---|
| the one-step path = the core's steps | on every update: `one_step` (one multiproof, the honest residue witness, the summary adoption; blst's batch sum and the chain of additions agree) against the core's own steps (a branch per non-signer, the final exponentiation, the 512-key adoption in two parts, `apply`): the official `sync` vectors (52 cases, consensus-specs v1.7.0-beta.4), the mainnet replay (5 updates as served), Sepolia through Gloas (32 period updates and the finality update); the policy store's `process_update` (`Adoption::OnChain`) moves alike | 198 updates: vectors 160 (91 accepted, **66 of them handoffs**; 69 refused alike), mainnet 5 (**5 handoffs**), Sepolia 33 (**32 handoffs**) | the same store and outcome, the same job but for its adoption progress (`None` against the core's completed `AdoptionProgress`), or the same refusal code |
| refusals of a handoff | mainnet update_1 as served (a handoff, 510 of 512): a node of its next-committee branch altered (three positions), its next committee's aggregate or one of its keys replaced with the branch unchanged: 27; signed by the bench's committee, 341 signers: 51 (342: accepted alike); the handoff without finality: 50; the handoff again on the store it produced: 52; update_0 on that store: 23 | 10 | refused alike by both paths |
| `adopt_summary` | a finality update's job (it adopts nothing), an initialization's (the bootstrap keeps the core's adoption), a handoff's job adopted twice | 3 | `NoAdoption` (84), nothing changed |
| the one difference (§10.3) | a chain that never existed (the store of the Electra `sync` vector case with a committee the test holds, fabricated updates as `ferrata-spectests`' `mutation_guards.rs`): a handoff to a next committee whose aggregate field is its first key; then an update of the next period signed by that committee | 1 handoff, 4 later updates | the core refuses at its adoption (45), the one-step path accepts; afterwards the committee's honest signature is refused (40) and its first member alone, every bit set, is accepted; from the consistent handoff (accepted alike) the reverse, and the next update alike by both paths |
| guest | `residue-host measure-handoff`: 18 executions of a handoff, each giving the store the core's steps give (natively also `one_step`, both sums); in the guest, a next-committee branch node flipped (27), 341 signers (51), the handoff again (52) | 18 + 3 | pass |
| LEZ v0.4 | `lez/harness/tests/residue_handoff.rs` (2 tests, §10.8); the earlier `residue_experimental` tests with the changed copy (4; the fewest-signers search not re-run) | 2 + 4 | pass |
| LEZ v0.3.1 | `lez-v03/harness/tests/v03.rs`, `handoffs_in_one_transaction_each` (§10.8); the other `v03` (4), `third_party` (4) and `adversarial` (5) tests with the new image; the client builds; `tests/sendguard.sh`, `tests/verifystatus.sh` | 1 + 13 + 2 scripts | pass |

### 10.8 Numbers

**Bare guest** (`residue-host measure-handoff`, [`results/residue/handoff/tables.md`](results/residue/handoff/tables.md),
`one-tx.txt`; the bench guest `guests/residue-agg-accel`, r0vm 3.0.5 executor, as §9.6): mainnet update_1 as served
(a handoff) against the store after update_0, and signed by the bench's committee (§5's method); one multiproof
(blst's batch sum unless said). Execution user cycles of the whole update:

| handoff | witness check | witness folded | witness check, chain of additions | with the core's 512-key adoption instead (witness; folded) |
|---|---:|---:|---:|---:|
| 510 of 512, the real committee | **6,578,036** (65.8%) | 5,916,026 (59.2%) | 6,577,593 | 12,575,468; 11,913,881 |
| 512 of 512 | 6,545,429 | 5,883,419 | 6,545,429 | 12,542,861 |
| 342, every third absent | 8,146,550 (81.5%) | 7,484,540 (74.8%) | 8,481,101 | 14,143,982 |
| **342, every absent index odd (the costliest placement found, §9.6)** | **8,154,987** (81.5%) | **7,492,977** (74.9%) | 8,489,538 (84.9%) | 14,152,419 |

The summary adoption costs **686** cycles (`A2`) where the 512-key adoption costs 5,991,991. A handoff then costs
26,181 cycles more than the finality update at the same participation measured in §9.6 (6,551,855, 8,120,369 and
8,128,806 there): the update carries the next committee's summary and its branch, which step 1 checks, and applying it
writes the summary. With the final exponentiation and the core's composition (no residue witness), the 510-signer
handoff with the summary adoption takes 9,102,704 (91.0%). The guest's change moved §9.6's period update (510, folded,
512 keys) from 11,913,877 to 11,913,881.

**Through LEZ v0.4's state machine** (`lez/harness/tests/residue_handoff.rs`, in-process harness, LEZ v0.4.0-rc1,
RISC Zero dev mode, as §6; output [`results/residue/handoff/lez.txt`](results/residue/handoff/lez.txt)); the
342-signer rows give the instance's store the bench's committee after update_0, as §9.6:

| transaction (`VerifyAll` at the experimental copy) | handlers run (cycles each) | execution gas | of `MAX_GAS_EXEC` | storage gas | result |
|---|---|---:|---:|---:|---|
| handoff update_1 (510 of 512), one multiproof + residue witness + the 512 keys adopted | `VerifyAll` | 10,000,000 | 100% | 54,269 | reverted: "Execution exceeded its cycle budget of 10000000 cycles" |
| the same, one node of its next-committee branch altered, summary | `VerifyAll` 204,819 | 204,819 | 2.0% | 5,117 | refused: exit 27 |
| **the same, the committee adopted by its summary** | `VerifyAll` 6,544,510 → `ApplyVerified` 59,689 → `Record` 17,352 | **6,621,551** | **66.2%** | 5,117 | applied; the store is the native policy store's and the core's after all its steps (the 512-key adoption included); the record is written |
| the same handoff again | `VerifyAll` 217,565 | 217,565 | 2.2% | 5,117 | refused: exit 52 |
| signed by 341 of 512 (the bench's committee) | `VerifyAll` 120,590 | 120,590 | 1.2% | 21,149 | refused: exit 51 |
| **342 of 512, every absent index odd (the costliest placement found), summary** | `VerifyAll` 8,264,428 → 59,689 → 17,352 | **8,341,469** | **83.4%** | 29,085 | applied; the store the core's after all its steps |
| 342 of 512, every third absent, summary | `VerifyAll` 8,256,501 → 59,689 → 17,352 | 8,333,542 | 83.3% | 29,085 | applied |
| 342 of 512, every absent index odd, the chain of additions, summary | `VerifyAll` 8,599,313 → 59,689 → 17,352 | 8,676,354 | 86.8% | 29,085 | applied |

At the costliest placement measured, 1,658,531 gas is left to the limit; as in §9.6, the maximum over all placements
and committees is not established. The copy's image id is now `ea4b8f44…` (`4f1b2fa6…` in §9's build; neither is in
`docs/image-ids.md`); the deployed program's, printed by the same test, is still `816d1fef…`. The earlier tests of §6
and §9.6, re-run with this copy, give each figure 9 gas higher (8,318,654 for §9's 342-signer finality update, 6,606,709
and 6,629,344 for the 510-signer one, 8,653,539 with the chain of additions); the deployed `VerifyAll` 9,124,896, as before.

**Through LEZ v0.3.1's block application** (`lez-v03`, `handoffs_in_one_transaction_each`; `lez-v03/results/mainnet-handoff-one-tx.tsv`,
`sepolia-handoff-one-tx.tsv`; [`../lez-v03/README.md`](../lez-v03/README.md)): every handoff in one `Update`
(`Verify::ResidueSummary`, one multiproof): mainnet update_0 to update_3 (512, 510, 510, 512 signers) **6,565,465**,
**6,636,683**, **6,647,863**, **6,597,601** execution gas (65.7% to 66.5%); Sepolia's 32 handoffs, periods 1378 to 1409
through the Gloas layout change, 6,448,420 to **6,592,610** (at most 65.9%); after each the instance's store is the
native policy store's. Refused, nothing changed: a next-committee branch node altered (27), the summary adoption with
the 512 keys beside it (84), update_0 again (23). The program's image id changed to `f8ec1b21…` (deployed nowhere; the
local sequencer run of `lez-v03/README.md` used `8d3b59ff…`), and its other figures moved by 69 to 89 gas. The
342-signer handoff was not run on v0.3.1: its harness cannot write a substituted committee into an instance's state.

### 10.9 What an auditor must check (in addition to §7 and §9.7)

1. The argument of §10.2: that the light client's assumption covers the aggregate field exactly as it
   covers `pubkeys_root` (one container, one branch), and the spec lines it rests on
   (`get_next_sync_committee`, `eth_aggregate_pubkeys`, `KeyValidate`, the deposit proofs of
   possession) at the pinned version.
2. `adopt_summary`: that it is reached only after step 1's next-committee branch check, that the summary
   it lets `apply` write is the one that branch proved (`pending.next` equal to the progress's
   committee), and that it refuses an initialization.
3. That every use of the adopted summary keeps its checks: each non-signer's key decoded canonically and
   proven against `pubkeys_root` at the index of its clear bit (core or §9's multiproof), the aggregate
   decoded (`PubkeyInvalid` otherwise).
4. The experimental copies' signals: an empty `adoption` on v0.4 (any other length goes to the core's
   adoption), `Verify::ResidueSummary` on v0.3.1 (adoption parts beside it are refused, 84).
5. That nothing downstream reads the adoption progress of an applied job (the v0.4 `VerifyAll` path
   never stores the job; the v0.3.1 instance clears it when the job applies).

### 10.10 What this does not change

The deployed programs, their image ids and the deployed update path are unchanged; the policy is
unchanged; the core's adoption still runs for every bootstrap and wherever the keys are given. The
production change would be a new program image (as §8 and §9.8 say for the residue check and the
multiproof), and with it the decision to rely, for the aggregate field, on the assumption the light
client already relies on for the header and the key root; an audit of the three changes together.

Re-check: `cd bench && cargo test --release -p residue-handoff && cargo build --release -p residue-host &&
./target/release/residue-host measure-handoff` (the vectors first: `scripts/fetch-vectors.sh`);
`cd lez && export RISC0_HOME=$PWD/../../risc0-home-lez && RISC0_DEV_MODE=1 cargo test -p harness --features
residue-experimental --test residue_handoff -- --nocapture --test-threads 1` (as §6);
`cd lez-v03 && export RISC0_HOME=$PWD/../../risc0-home-lez && cargo test --release -p harness-v03 --test v03
handoffs_in_one_transaction_each -- --nocapture` (`lez-v03/README.md`, "Re-check", for its LEZ v0.3.1 checkout).
