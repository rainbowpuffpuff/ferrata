# Privacy suite with real proofs (milestone B2's gate on the delivered programs)

The privacy suite of [`../RESULTS.md`](../RESULTS.md) ran with RISC Zero in dev mode and a mock verifier on the
Ethereum side. This run measures the same gate on what is delivered: the final programs of
[`../../image-ids.md`](../../image-ids.md), every private claim and burn **proven for real** and its receipt verified
by LEZ's settlement, every release **proven for real**, wrapped to Groth16 and checked in the vault by RISC Zero's
Groth16 verifier. The question is what real proofs add to the public data (the seals, the receipt sizes, the Groth16
calldata) and whether any of it links a deposit to its claim or a burn to its release.

**Result.** 20 of 20 deposit-and-claim pairs and 20 of 20 burn-and-release pairs ran with real proofs (none in dev
mode). The gate passes for every private step: private claims (231,950 bytes each), private burns (230,870) and
releases (580 bytes of calldata) are byte for byte identical once nullifiers, fresh randomness, a user's own address,
time, and fields shown to be functions of the transaction's own public data are cut; the plaintext variant still
fails it, as designed. The adversary stays at chance (5.0%) on deposit → claim and burn → release, and at 100% on the
linkable variants. **No field links a deposit to its claim or a burn to its release.** Every new field is classified
and tested: the receipt's STARK proof and the Groth16 proof are fresh randomness (the runs that recur across LEZ
seals recur as much in receipts of other programs proven two days earlier, and no more between a user's claim and
burn than between strangers; no run of a Groth16 proof recurs), the receipt's claim and the claim
digest in the seal are functions of the transaction's message (checked from the public bytes), everything else in a
proof is constant. The files of [`generated/`](generated/) are this run's output; [`dev/`](dev/) is the same code on
the same programs in dev mode (fake receipts, mock verifier), the baseline against which "what real proofs add" is read.

## What ran, and where

| | |
|---|---|
| programs | bridge program `0xccde6171…71dd` (the test stops before its first proof unless its id is this, docs/image-ids.md); release `0xb64900bc…29e9` (rebuilt R0BF binary, sha256 `7be1349c…11bf`, equal to docs/image-ids.md's); LEZ v0.4.0-rc1's privacy-preserving circuit as built in this tree, `8fc948d1…f257` (LEZ's own program, not in docs/image-ids.md); vault v5 (`vault/src` of this commit) with RISC Zero's `RiscZeroGroth16Verifier` (risc0-ethereum v3.0.0 `ControlID`) behind RISC Zero's emergency stop |
| LEZ half | the suite's test binary (built on the build box at commit `2686e55`, the test code of `c892b7e`; stripped, sha256 `eece2d83…`) with `FERRATA_PRIVACY_PROOFS=real` and dev mode off, in one Modal container (profile `ws-5`): **one NVIDIA L4** (23,034 MiB, driver 610.57.04), 4 cores, 32 GiB; r0vm 3.0.5 built with CUDA (sha256 `781ca946…6fb7`, the binary of [`../../../gpu-prover/`](../../../gpu-prover/), checked before use) as the prover (`RISC0_SERVER_PATH`) |
| releases | each release statement's succinct proof and `identity_p254` step in the same L4 container (`ferrata-e2e prove`, `ferrata-e2e p254`); the Groth16 step in RISC Zero's prover image (`risczero/risc0-groth16-prover:v2025-04-03.1`, CPU, Modal, `e2e/crown/wrap.py`); the Groth16 receipt assembled and verified on the build box (`ferrata-e2e groth16`, `ferrata-e2e verify`) |
| Ethereum half | Foundry on the build box: `vault/test/PrivacySuite.t.sol` with `release-seals.json`: the vault at P on chain 1, its release image id the real one, releases verified by `RiscZeroGroth16Verifier`; the checkpoint (the burn root's acceptance, one transaction for all 20) on the mock, as before |
| when | 12 November 2026; the L4 container ran 11:38:31Z to 11:50:04Z (692.7 s), 46 minutes after the app started (the image build and the wait for an L4, which is not billed) |

Real here: 40 of 40 private LEZ transactions (each receipt verified by LEZ's settlement before the state changed:
`check_privacy_preserving_circuit_proof_is_valid` through `Proof::is_valid_for`) and 20 of 20 releases (succinct and
Groth16 receipts verified by `ferrata-e2e` against the release image id and the native journal, 20 of 20; each
release accepted by the vault's real Groth16 verifier; the first one, with one seal byte flipped, refused). Not real,
as in every earlier pass: the burn root's acceptance on Ethereum (mock checkpoint seal), the Ethereum records LEZ
reads (synthetic, cross-checked equal to the vault's deposit roots), and the two naive variants (models: no
delivered program computes `N = fid` or `NOut = burnId`; the naive releases run on the mock).

## The gate ([generated/summary.md](generated/summary.md))

| records | bytes (LEZ transaction / Ethereum calldata) | identical once nullifiers, fresh randomness, own addresses, time and functions of public data are cut |
|---|---|---|
| deposit (A: private destinations) | 100 | yes |
| deposit (B: public destinations) | 100 | yes |
| claim, private (default) | 231,950 (dev mode: 8,937) | yes |
| claim, naive nullifier N = fid | 231,950 | yes |
| claim, plaintext | 2,053 | **no**: from byte 133 (the declared accounts), then the witness, as in every pass |
| burn, private holding | 230,870 (dev mode: 7,857) | yes |
| release, NOut of the release statement | 580 (dev mode: 324) | yes |
| release, naive NOut = burnId | 324 | yes |

The cut is the one of every pass, with one class added for real proofs: **a function of public data**, allowed to
vary only where this analysis shows, from the public bytes, that the field is computed from the record's own
message. It applies to the receipt's claim, the claim digest the seal carries and the calldata's gas (below).

## The adversary

| game | success (chance 5.0%) | exactly identified | candidate-set sizes | without derived identifiers | with order-bearing state |
|---|---:|---:|---|---:|---:|
| inbound, private (default) | **5.0%** | 0 of 20 | 20: 20 | 5.0% | 5.0% |
| inbound, naive nullifier N = fid | 100.0% | 20 of 20 | 1: 20 | 5.0% | 100.0% |
| inbound, plaintext claim | 100.0% | 20 of 20 | 1: 20 | 100.0% | 100.0% |
| outbound, private (default) | **5.0%** | 0 of 20 | 20: 20 | 5.0% | 5.0% |
| outbound, naive NOut = burnId | 100.0% | 20 of 20 | 1: 20 | 5.0% | 100.0% |
| round trip: deposit to release (informational) | 5.0% | 0 of 20 | 20: 20 | 5.0% | 5.0% |
| on LEZ: claim to burn (informational) | 5.0% | 1 of 20 | **1: 20** | 5.0% | 5.0% |

The adversary now reads every part of every proof (the 222,540-byte STARK of each LEZ receipt, each Groth16 proof)
as public bytes. On the deposit and release sides there is no STARK seal, and nothing of a Groth16 proof recurs
anywhere, so the inbound and outbound games are unchanged. Between a claim and a burn, both carrying STARK seals, the
recurring runs below give every candidate some score: the adversary now names one burn per claim (20 singleton
guesses instead of 20 sets of 20) and is right once in 20, which is chance. In dev mode the same game is 4.5% (two
wrong singletons), as in the sixth pass.

## What real proofs add to the public data

The collector splits LEZ's witness set (borsh `Vec<(Signature, PublicKey)>`, then `Proof`, a borsh `InnerReceipt`)
and the release's seal into named fields; [generated/fields.md](generated/fields.md) has them with every other field.

| step | field | bytes | across the 20 records | class and test |
|---|---|---:|---|---|
| claim, burn | `witness_set.signatures` | 4 | constant | none (the sender is an alias) |
| claim, burn | `proof.length`, `proof.kind` | 4, 1 | constant | the receipt's size (227,538 claim; 226,998 burn) and kind (succinct) |
| claim, burn | `proof.seal.length` | 4 | constant | 55,667 words, the same for claims and burns, whose executions differ (charged cycles 140,004–144,339 against 106,046): the receipt hides the execution's size |
| claim, burn | `proof.seal.control_root` | 64 | constant | the seal's first 16 output words: the recursion control root |
| claim, burn | `proof.seal.claim_digest` | 64 | 20 distinct | **a function of public data**: the seal's output words 16–31, BabyBear elements in Montgomery form, decode to 16-bit halves of the claim's digest, which this script computes from `proof.claim` by RISC Zero's rules: 40 of 40 |
| claim, burn | `proof.seal.stark` | 222,540 | 20 distinct | **fresh randomness**: the STARK proper; no window shared with a deposit's or a release's public bytes (the inbound and outbound games score every candidate alike); the runs it shares with other seals are the recursion circuit's (below) |
| claim, burn | `proof.control_id` | 32 | constant | `53a7b23d…da4a`, the same for claims and burns: the recursion program that made the receipt |
| claim, burn | `proof.claim` | 4,524 claim; 3,984 burn | 20 distinct | **a function of public data**: unpruned, it holds the journal, which is the frame LEZ builds from the transaction's message (a copy of the message's first bytes: 40 of 40), and its digest is `ReceiptClaim::ok(circuit id, journal)`'s (40 of 40); the collector's own check agrees (40 of 40) |
| claim, burn | `proof.hashfn`, `proof.verifier_parameters`, `proof.control_inclusion_proof` | 9, 32, 264 | constant | `poseidon2`; `ece5e9b8…a013`; the control id's path |
| release | `calldata.seal.offset`, `.length`, `.padding` | 32, 32, 28 | constant | the seal is 260 bytes |
| release | `calldata.seal.selector` | 4 | constant | `0x73c457ba`, the verifier's parameters |
| release | `calldata.seal.groth16.a`, `.b`, `.c` | 64, 128, 64 | 20 distinct | **fresh randomness**: the Groth16 proof (pi_A, pi_B, pi_C); no run of 19 bytes or more of one recurs in another release or in any LEZ seal |
| every Ethereum transaction | `receipt.calldataGas` | 32 | releases: 4 values; deposits: as their `C` | **a function of public data**: EIP-2028's 4 gas per zero byte and 16 per other byte of the calldata, which a real receipt's `gasUsed` includes and the Foundry call does not; among the releases it follows the zero bytes of the Groth16 proof (0 to 3) and of NOut (0 or 1) |

What did not change: LEZ's fee accounting is identical with real and fake receipts (execution gas 549,768 for the
first claim and 554,103 for the others, 515,810 per burn; storage gas 224,063 for every private transaction); the
receipts' order effects are those of the sixth pass. On Ethereum the real Groth16 check costs a constant **224,926**
gas per release: 305,387 gas (330,387 for the first, whose relay fee creates the relayer's account) against 80,461
with the mock. The section B6 comparison ([generated/b6.md](generated/b6.md)) gains the proof fields among what §B6 as
first drafted does not list (finding 3); its hidden column is still 0 of 320 values found, now searched in the real
receipts' and seals' bytes too; recovery from the seed alone is unchanged (10 of 10 unclaimed deposits, 10 of 10
unreleased burns, `NOut` equal to the vault's for 20 of 20).

### The runs that recur across LEZ seals

Of the 2,080,440 16-byte windows of the 40 STARK parts (word-aligned, at least 12 non-zero bytes), 22,708 recur in 2 to
40 records, and every one of the 780 pairs of seals shares some: 429–755 windows per pair of burns (median 590.5),
426–873 per pair of claims (589), 409–794 per claim and burn of different users (592) and **491–732 per claim and burn
of the same user (592)**. Against three receipts of the same recursion circuit proven on 10 November for other
programs and statements ([`../../../gpu-prover/results/`](../../../gpu-prover/results/): the private claim and burn of an
earlier bridge program, row 10, and a private attestation, row 5), each seal of this run shares 506 to 931 windows
(medians 671 to 675). The recurring runs are therefore properties of the recursion circuit, not of a statement or
a user; most likely openings of its fixed (code) Merkle tree, which every proof opens at its own random queries
(inferred: the seal was not parsed beyond its output words). They give the adversary nothing: chance in every game.

## Proving times and sizes (measured; [proofs.json](proofs.json) has every proof)

| proof | where | time per proof | size |
|---|---|---|---|
| private claim (LEZ's privacy-preserving circuit over the bridge program's transitions and the fee transfer) | L4 | **15.8–16.6 s**, median 16.1 (20 proofs, 321.8 s) | receipt 227,538 B, transaction 231,950 B |
| private burn | L4 | **12.8–13.0 s**, median 12.9 (20 proofs, 257.8 s) | receipt 226,998 B, transaction 230,870 B |
| release statement, succinct (1 segment, 77,736 user cycles) | L4 | 1.28–1.43 s | receipt 223,930 B |
| `identity_p254` | L4 | 1.05–1.21 s | |
| Groth16 (RISC Zero's prover image) | Modal CPU container | 14–15 s (298 s for 20) | Groth16 receipt 1,201 B; on-chain seal 260 B |

The whole LEZ half, with the 40 proofs, the public transactions and the collection, took 639.3 s on the L4 container
(GNU time; 0.78 GiB peak host memory); peak GPU memory 11,355 MiB, mean utilisation 52% over 691 one-second samples,
peak power 75.1 W. The 20 releases' succinct proofs and `identity_p254` steps took 51.7 s together. As in row 10, a
private claim or burn takes about 13–16 s on one L4 (row 10: 15.3 and 12.4 s, earlier program).

## Findings

1. **No linking field.** With real receipts and real Groth16 seals in the public bytes, the gate passes for every
   private step and the adversary is at chance; the seals, the receipt sizes and the Groth16 calldata are classified
   above and tested, none skipped.
2. **The collector's seal-output check was wrong in this run's binary** (it compared the seal's raw words, which are
   BabyBear elements in Montgomery form, with the claim digest, and recorded `false` for 40 of 40). The receipts were
   verified by LEZ's settlement regardless, and this analysis now decodes the words and checks them from the public
   bytes (40 of 40); the test is fixed for later runs (`af0e0aa`), its `meta` flag in this run's data is superseded.
3. **Section B6's release row lists no proof.** The release shows a 260-byte seal: a constant selector and a 256-byte
   Groth16 proof drawn fresh for each release. The claim and burn rows' "LEZ's own private-transaction fields" cover
   the receipt, which also carries a copy of the message (the journal, 4,524 or 3,984 bytes).
4. **A flipped byte inside a Groth16 curve point costs a release all the gas it is given**: the pairing precompile
   fails and consumes it. The vault refuses the release either way (here with a 5,000,000-gas cap, as `test/real/`
   caps its tampered calls); a relayer simulates before it sends.
5. **The vault requires an emergency stop as its verifier** (its constructor reads `paused()`), so the test's split
   verifier sits behind RISC Zero's stop, as a real deployment's verifier does.

## What is not covered

- The burn root's acceptance (one candidate and one finalisation for all 20 burns) is on the mock verifier: a real
  checkpoint proof over the harness's blocks was not made; row 24 and the crown run prove checkpoints for real.
- The Ethereum records LEZ reads are synthetic, as in all bridge-program tests; the naive variants are models.
- One run: receipts are randomized, so another run gives other seals (and, by the classes above, the same verdicts).
- The LEZ circuit's image id is LEZ's own build in this tree; it is not in docs/image-ids.md and was not compared
  with the live run's.
- The recurring runs are attributed to the recursion circuit by the control comparison, not by parsing the seal.
- Timing, amounts and network metadata: as in [`../RESULTS.md`](../RESULTS.md).

## Modal spend

On profile `ws-5` (billing report, `spike/modal-build/mcost`): 5.1233 USD before this work, 5.5183 USD after the
GPU run, the wraps and the analysis (ferrata-privacy-real 0.2458 USD: the L4 container and its image; the build box
and the Groth16 prover containers in ferrata-build). See the end of this file for the final figure.

## Reproduce

```sh
# Build box (spike/modal-build/mrun beside the repository; any Linux machine with lez/'s, bridge/'s and vault/'s
# toolchains and glibc <= 2.39 does): the test binary, ferrata-e2e and the release program, ids checked, staged;
# the dev-mode baseline -> docs/privacy/real/dev/.
mrun --profile <p> --env FERRATA_COMMIT=$(git rev-parse HEAD) -- \
  'bash scripts/privacy/real.sh build && bash scripts/privacy/real.sh dev'
mrun --profile <p> --get target/privacy/real/stage -- true
# Laptop, Modal only: the CUDA r0vm into the Volume, the GPU run, the Groth16 steps.
MODAL_PROFILE=<p> R0VM_CUDA=<r0vm-cuda of gpu-prover> scripts/privacy/real.sh volume
MODAL_PROFILE=<p> scripts/privacy/real.sh gpu privacy-real-1
MODAL_PROFILE=<p> scripts/privacy/real.sh wrap privacy-real-1
# Build box: Groth16 receipts, the Foundry half with the real verifier, the analysis -> docs/privacy/real/generated/.
tar -czf target/privacy/real/privacy-real-1-wrapped.tar.gz -C target/privacy/real privacy-real-1
mrun --profile <p> --put target/privacy/real/privacy-real-1-wrapped.tar.gz -- \
  'cd target/privacy/real && tar -xzf privacy-real-1-wrapped.tar.gz && cd ../../.. && bash scripts/privacy/real.sh finish privacy-real-1'
```

The raw data (105 MB: `lez.json` with the 40 real transactions, the release receipts and Groth16 inputs) stays under
`target/privacy/real/` and is not committed; [proofs.json](proofs.json) keeps every proof's time, size, sha256 and
the 20 Groth16 seals.
