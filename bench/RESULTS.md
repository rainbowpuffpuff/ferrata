# Ferrata: cycles to verify one mainnet light-client update in a RISC Zero zkVM 3.0.5 guest

**Former name.** Measured at commit f030475 under the project's former name, Plumbline
(`crates/plumbline-core`); the earlier snapshot run below used commit 25aa07e. The core differs
from f030475 only by the rename, checked by `scripts/check-bench-provenance.sh`. The raw outputs in
`results/` are left as produced and use the former name; `results/PROVENANCE.txt` adds the
per-file hashes after the rename, which changed six of them. A guest built after the rename is a
different image from the measured one (`651d5639…`), which contains the former name and paths.

RISC Zero **executor only** (`default_executor().execute`, r0vm 3.0.5, no proving). Every guest
asserts that the checks pass (signatures verify, committee keys sum to `aggregate_pubkey`, the
final finalized root is the canonical `0x9694…e09a`), so no number below comes from a run that
computed the wrong answer. Commands: `README.md`.

## Headline: `blst-accel` against the real core

**Measured code:** the `crates/plumbline-core` of commit **f030475** (byte-identical; checked with
`results/PROVENANCE.txt`), measured before that commit as commit **25aa07e** plus the working-tree changes to
`crates/plumbline-core` as of 6 Nov 2026 09:43 CET (`check_committee_onchain`,
`Adoption::{Full, OnChain}`, `initialize_with`, `Blst::miller_half` / `Blst::final_half`).
`git diff HEAD -- crates/plumbline-core` hashes to `3beb359a…`; per-file hashes are in
`results/PROVENANCE.txt`, and the sources did not change between the guest build and the end of
the run. Configuration: RISC Zero's accelerated blst (`v0.3.16-risczero.0`) and SHA-256 patch.
Data: mainnet update_1 (period 1900, the handoff to the period-1901 committee, 510/512 signers)
against the store state after update_0. M = 10^6 user cycles; "seg." = segments of 2^20 cycles.

| update_1 (period 1900, 510/512 signers), user cycles | blst-accel |
|---|---:|
| `process_update`, finality update (no adoption) | 10.21 M |
| `process_update`, handoff update, `Adoption::OnChain` | 59.67 M |
|   of which `check_committee_onchain` (512 keys on-curve + not identity, sum) | 48.70 M |
|   of which committee `hash_tree_root` + next-committee branch | 1.13 M |
| one whole execution, finality update: decode saved state + update, policy, new root | 10.37 M (11 seg.) |
|   the same incl. paging + padding (Σ 2^po2) | 11.53 M |
| one whole execution, handoff update (OnChain adoption) | 59.83 M (60 seg.) |
|   the same incl. paging + padding (Σ 2^po2) | 62.91 M |
| split via the core, execution 1: finality checks + `Blst::miller_half` | 6.41 M (7 seg.) |
|   of which `Blst::miller_half` itself | 6.09 M |
| split via the core, execution 2: `Blst::final_half` | 3.60 M (4 seg.) |
|   of which `Blst::final_half` itself | 3.59 M |
| 3-step job, step 1: everything before the Miller loops | 2.31 M (3 seg.) |
| 3-step job, step 2: Miller loops | 4.20 M (5 seg.) |
|   of which the two Miller loops + Fp12 mul | 4.09 M |
| 3-step job, step 3: final exponentiation + check (`final_half`) | 3.60 M (4 seg.) |
| `initialize_with(.., Adoption::OnChain)` from the bootstrap | 49.94 M |
| `check_committee` (Full: per-key subgroup check), for comparison | 229.28 M |
| `process_update`, handoff update, `Adoption::Full` (for comparison) | 240.26 M |
| `FinalizedStore::from_core` (restore; forces the Full check on both committees) | 458.60 M |

* "Finality update" is update_1 with `next_sync_committee` and its branch zeroed: same
  signature, same finalized header, nothing to adopt.
* A "whole execution" is a complete executor run. It reads and decodes the saved store state
  (finalized header and both committees, 49 KB of SSZ) and the update, runs the OnChain policy and
  commits the new finalized root. It cannot use `FinalizedStore`; see the next section.
* Both split paths hand 576 bytes from one execution to the next. In each, the bytes equal the
  core's `miller_half` computed natively on the host, byte for byte (asserted).
* The handoff (A1, 59.67 M) is 0.37 M less than finality + `check_committee_onchain` + committee
  root and branch (A2 + B1 + B3). The finality path does some work that the handoff path skips;
  this was not broken down further.

## Where the core API stops a program

1. **A store cannot be restored from saved state without re-checking.** The only constructor from
   saved state, `FinalizedStore::from_core`, runs the *Full* `check_committee` on both committees
   (458.60 M) and fixes `Adoption::Full`. A restored store therefore also adopts later committees
   with the Full check: 240.26 M per handoff instead of 59.67 M. No constructor takes an
   `Adoption` or trusts the program's own state. The "whole execution" rows therefore run the
   policy of `FinalizedStore::process_update` with public functions on a `StoreCore` decoded from
   the saved state: finality and supermajority checks, `validate_light_client_update` on the
   `CheckedCommittee` path, `update_advances`, `apply_core`, and `check_committee_onchain` when
   adopting. In the same guest, this gives the same outcome and the same store as
   `FinalizedStore` with `Adoption::OnChain` (asserted; M1/M2 differ from A1/A2 by under 50
   cycles).
2. **A split verification cannot be applied.** `process_update` always verifies the signature
   itself, through `validate_light_client_update` and `verify_checked`, and
   `validate_light_client_update` cannot skip or defer the signature. To split the pairing across
   transactions, a program must repeat the non-signature checks itself, then call `apply_core`
   (and the adoption check) after `final_half`. Going through `process_update` would pay the
   whole pairing again (9.66 M). The split rows run the costly pieces through the public API:
   header validity ×2, the finality branch, the signing root, `miller_half` and `final_half`.
   They leave out the remaining slot, period and relevance checks, about 2 k cycles in total (V
   minus its measured parts, PHASES table).
3. **There is no seam before the Miller loops.** `miller_half` covers the signing key, the
   signature checks, hash-to-G2 and the Miller loops in one call. So steps 1 and 2 of the 3-step
   job were measured with raw blst calls that mirror the core's `checked_signing_key` and
   `miller`. Step 2's 576-byte output equals the core's `miller_half` byte for byte, and step 3 is
   the core's `final_half`.

## Results against the real core: every mode, `blst-accel`

Generated by `python3 -I tools/table.py results` from `results/blst-accel.all.txt`. The row codes
are defined under Method. The PHASES, E2E, SINGLE and SPLIT modes repeat the earlier snapshot's
measurements with the Full policy; they moved by at most 0.21 M.

#### ONCHAIN: Adoption::OnChain store; update_1 with and without handoff; committee checks; restore paths

| step | blst-accel |
|---|---:|
| O1 decode bootstrap (SSZ) | 41,313 |
| O2 initialize_with(Adoption::OnChain): bootstrap checks + check_committee_onchain | 49,935,413 |
| O3 decode update_0 (SSZ) | 55,816 |
| O4 process_update(update_0), OnChain: adopts the period-1900 committee | 60,029,131 |
| O5 decode update_1 (SSZ) | 55,092 |
| A1 process_update(update_1), OnChain: finality + handoff, check_committee_onchain on period 1901 | 59,673,874 |
| A2 process_update(update_1 as finality update), OnChain store: no adoption | 10,213,137 |
| B1 check_committee_onchain(period-1901 committee) alone | 48,696,769 |
| B2 check_committee (Full: KeyValidate per key) alone, for comparison | 229,282,241 |
| B3 the rest of a handoff's extra work: committee hash_tree_root + next-committee branch | 1,131,299 |
| R1 decode saved store state (finalized header + 2 committees, SSZ) | 73,803 |
| R2 FinalizedStore::from_core: the only constructor from saved state (Full check on both committees; adoption fixed to Full) | 458,603,015 |
| M1 OnChain policy on the decoded StoreCore (public building blocks, no re-check), update_1 as served | 59,673,826 |
| M2 OnChain policy on the decoded StoreCore, finality update | 10,213,097 |
| **execution total, user cycles** (SessionInfo) | 989,341,277 |
| segments (2^20 cycles each) | 988 |
| **execution total incl. paging + padding** (Σ 2^po2) | 1,035,993,088 |
| executor wall-clock | 32.0 s |

#### SINGLE-ONCHAIN: one execution = decode saved state + update, OnChain policy, no re-check (update_1 as served, handoff)

| step | blst-accel |
|---|---:|
| Q1 decode saved store state (SSZ) | 73,802 |
| Q2 decode update (SSZ) | 55,094 |
| Q3 OnChain policy: validate (verify_checked), apply, check_committee_onchain if adopting | 59,673,828 |
| Q  sum Q1..Q3 + new root (one cycle_count delta) | 59,811,275 |
| info: adopted a committee (1 = yes) | 1 |
| **execution total, user cycles** (SessionInfo) | 59,829,912 |
| segments (2^20 cycles each) | 60 |
| **execution total incl. paging + padding** (Σ 2^po2) | 62,914,560 |
| executor wall-clock | 2.2 s |

#### SINGLE-ONCHAIN: one execution = decode saved state + update, OnChain policy, no re-check (update_1 as finality update)

| step | blst-accel |
|---|---:|
| Q1 decode saved store state (SSZ) | 73,802 |
| Q2 decode update (SSZ) | 55,094 |
| Q3 OnChain policy: validate (verify_checked), apply, check_committee_onchain if adopting | 10,213,095 |
| Q  sum Q1..Q3 + new root (one cycle_count delta) | 10,350,542 |
| info: adopted a committee (1 = yes) | 0 |
| **execution total, user cycles** (SessionInfo) | 10,369,179 |
| segments (2^20 cycles each) | 11 |
| **execution total incl. paging + padding** (Σ 2^po2) | 11,534,336 |
| executor wall-clock | 0.4 s |

#### CORESPLIT execution 1: finality checks + Blst::miller_half (commits 576 bytes)

| step | blst-accel |
|---|---:|
| C1a decode update_1 (SSZ) | 55,085 |
| C1b decode the signing committee from saved state (SSZ) | 34,013 |
| C1c supermajority + header validity x2 + finality branch | 115,334 |
| C1d signing root | 10,275 |
| C1e Blst::miller_half: signing key, signature checks, hash-to-G2, Miller loops, encode | 6,086,126 |
| C1 sum (one cycle_count delta, excl. input read and journal commit) | 6,312,970 |
| **execution total, user cycles** (SessionInfo) | 6,406,690 |
| segments (2^20 cycles each) | 7 |
| **execution total incl. paging + padding** (Σ 2^po2) | 7,340,032 |
| executor wall-clock | 0.3 s |

#### CORESPLIT execution 2: Blst::final_half from the 576 bytes

| step | blst-accel |
|---|---:|
| C2 sum (one cycle_count delta) | 3,591,151 |
| **execution total, user cycles** (SessionInfo) | 3,597,597 |
| segments (2^20 cycles each) | 4 |
| **execution total incl. paging + padding** (Σ 2^po2) | 4,194,304 |
| executor wall-clock | 0.2 s |

#### STEP 1 of 3: everything before the Miller loops (finality update; hands over 480 bytes)

| step | blst-accel |
|---|---:|
| T1a decode update_1 (SSZ) | 55,085 |
| T1b decode the signing committee from saved state (SSZ) | 34,013 |
| T1c supermajority + header validity x2 + finality branch | 115,334 |
| T1d signing root | 10,275 |
| T1e signing key, subtractive + identity check | 286,049 |
| T1f signature decompression + G2 subgroup check + not identity | 526,791 |
| T1g hash_to_G2 + to affine | 1,169,099 |
| T1h negate key + to affine | 1,833 |
| T1i serialize H(m), -key, signature (480 bytes) | 7,811 |
| T1 sum (one cycle_count delta, excl. input read and journal commit) | 2,222,387 |
| **execution total, user cycles** (SessionInfo) | 2,305,240 |
| segments (2^20 cycles each) | 3 |
| **execution total incl. paging + padding** (Σ 2^po2) | 2,621,440 |
| executor wall-clock | 0.1 s |

#### STEP 2 of 3: Miller loops (hands over 576 bytes)

| step | blst-accel |
|---|---:|
| T2a deserialize H(m), -key, signature (on-curve checks) | 13,935 |
| T2b Miller loops: ML(H(m),-key) + ML(sig,g1) + Fp12 mul | 4,087,685 |
| T2c encode the product (576 bytes, the core's canonical encoding) | 7,809 |
| T2 sum (one cycle_count delta, excl. input read and journal commit) | 4,110,700 |
| **execution total, user cycles** (SessionInfo) | 4,203,683 |
| segments (2^20 cycles each) | 5 |
| **execution total incl. paging + padding** (Σ 2^po2) | 4,456,448 |
| executor wall-clock | 0.2 s |

#### STEP 3 of 3: Blst::final_half (final exponentiation + check)

| step | blst-accel |
|---|---:|
| C2 sum (one cycle_count delta) | 3,591,151 |
| **execution total, user cycles** (SessionInfo) | 3,597,597 |
| segments (2^20 cycles each) | 4 |
| **execution total incl. paging + padding** (Σ 2^po2) | 4,194,304 |
| executor wall-clock | 0.2 s |

#### PHASES: update_1 against the post-update_0 store

| step | blst-accel |
|---|---:|
| 1  SSZ decode of update_1 (26,906 bytes) | 55,090 |
| 2a header validity, attested (payload header root + 4-deep execution branch) | 48,174 |
| 2b header validity, finalized | 48,168 |
| 3a SyncCommittee hash_tree_root (512 keys + aggregate) | 1,123,291 |
| 3b next-committee branch check (6 deep) | 8,086 |
| 3c finality: finalized beacon header root + branch check (7 deep) | 17,102 |
| 4  signing root (attested header root, fork version, domain) | 10,279 |
| info: update_1 signers | 510 |
| info: update_1 non-signers | 2 |
| 5s aggregation, subtractive: decompress aggregate + non-signers, add, negate | 285,875 |
| 5a aggregation, additive: decompress every signer key + add | 48,318,251 |
| 5a' additive, point additions only (keys already decompressed) | 2,468,202 |
| 5d one G1 decompression (for scale) | 89,959 |
| 5g one G1 subgroup check, affine (for scale) | 352,595 |
| 5k G1 subgroup check of the aggregate key (earlier design; not in verify_checked) | 352,307 |
| 5n identity check + negate key + to affine (as pairing_check) | 1,871 |
| 6a signature decompression (G2) | 183,728 |
| 6b signature G2 subgroup check + not identity | 343,140 |
| 7  hash_to_G2(signing root) + to affine | 1,169,105 |
| 8  Miller loops as ferrata: ML(H(m),-key) + ML(sig,g1) + Fp12 mul | 4,087,690 |
| 8m alternative: one 2-pair blst_miller_loop_n | 3,251,802 |
| 9  final exponentiation + is_one | 3,581,111 |
| B1 backend verify_checked, subtractive (ferrata code, policy store path) | 9,657,332 |
| B2 backend verify_checked, additive (ferrata code) | 57,685,120 |
| B3 backend fast_aggregate_verify (spec path: KeyValidate on every signer key) | 237,619,457 |
| 10a adoption, compressed: 512 x KeyValidate (decompress + G1 subgroup) + sum + compare aggregate | 229,193,154 |
| 10a0 same, on-curve only (no per-key subgroup check; earlier design) | 48,598,974 |
| 10b adoption, uncompressed: 512 x (deserialize+on-curve, recompress==committed, G1 subgroup) + sum + compress==aggregate | 185,103,996 |
| 10b0 same, uncompressed without the per-key subgroup check | 4,562,038 |
| 10g 512 G1 subgroup checks alone (for scale) | 180,486,735 |
| 10a' backend check_committee (ferrata code) | 229,282,242 |
| V  validate_light_client_update(update_1), CheckedCommittee path, Auto (= subtractive) | 10,914,681 |
| P1 process_update(update_1) as served: finality + handoff (adopts next committee) | 240,259,352 |
| P2 process_update(update_1 stripped to a finality update): no committee adoption | 10,213,139 |
| **execution total, user cycles** (SessionInfo) | 2,063,948,491 |
| segments (2^20 cycles each) | 2060 |
| **execution total incl. paging + padding** (Σ 2^po2) | 2,159,542,272 |
| executor wall-clock | 63.8 s |

#### E2E: bootstrap -> update_0 -> update_1

| step | blst-accel |
|---|---:|
| E1 decode bootstrap (SSZ) | 41,317 |
| E2 FinalizedStore::initialize (incl. check_committee on 512 keys) | 230,520,891 |
| E3 decode update_0 (SSZ) | 55,818 |
| E4 process_update(update_0): current committee signs, adopts next committee | 240,614,607 |
| E5 decode update_1 (SSZ) | 55,094 |
| E6 process_update(update_1): handoff, next committee signs, rotates, adopts | 240,259,348 |
| E  sum E1..E6 (one cycle_count delta) | 711,548,889 |
| info: update_0 signers | 512 |
| info: update_1 signers | 510 |
| **execution total, user cycles** (SessionInfo) | 711,583,262 |
| segments (2^20 cycles each) | 711 |
| **execution total incl. paging + padding** (Σ 2^po2) | 745,013,248 |
| executor wall-clock | 19.6 s |

#### SINGLE: one execution = restore store + decode + process_update(update_1 as served, handoff)

| step | blst-accel |
|---|---:|
| R1 decode store state (finalized header + 2 committees, SSZ) | 74,128 |
| R2 FinalizedStore::from_core (re-checks both committees) | 458,564,136 |
| R3 decode update (SSZ) | 55,094 |
| R4 process_update | 240,259,349 |
| R  sum R1..R4 + new root (one cycle_count delta) | 698,962,333 |
| info: adopted a committee (1 = yes) | 1 |
| **execution total, user cycles** (SessionInfo) | 698,981,381 |
| segments (2^20 cycles each) | 698 |
| **execution total incl. paging + padding** (Σ 2^po2) | 731,906,048 |
| executor wall-clock | 22.7 s |

#### SINGLE: one execution = restore store + decode + process_update(update_1 as finality update)

| step | blst-accel |
|---|---:|
| R1 decode store state (finalized header + 2 committees, SSZ) | 74,128 |
| R2 FinalizedStore::from_core (re-checks both committees) | 458,564,136 |
| R3 decode update (SSZ) | 55,094 |
| R4 process_update | 10,213,138 |
| R  sum R1..R4 + new root (one cycle_count delta) | 468,916,122 |
| info: adopted a committee (1 = yes) | 0 |
| **execution total, user cycles** (SessionInfo) | 468,935,170 |
| segments (2^20 cycles each) | 469 |
| **execution total incl. paging + padding** (Σ 2^po2) | 490,799,104 |
| executor wall-clock | 15.3 s |

#### SPLIT execution 1: up to and including the Miller loop (commits Fp12)

| step | blst-accel |
|---|---:|
| S1 decode update_1 (SSZ) | 55,093 |
| S2 decode signing committee (SSZ, 513 keys) | 34,019 |
| S3 header validity x2 + finality branch | 113,289 |
| S4 signing root | 10,282 |
| S5 signing key, subtractive, negated, affine | 287,660 |
| S6 signature decompression + G2 subgroup check | 526,791 |
| S7 hash_to_G2 | 1,169,103 |
| S8 Miller loops (as ferrata: 2 x single-pair + Fp12 mul) | 4,087,683 |
| S  sum (one cycle_count delta, excl. input read and journal commit) | 6,290,348 |
| **execution total, user cycles** (SessionInfo) | 6,391,612 |
| segments (2^20 cycles each) | 7 |
| **execution total incl. paging + padding** (Σ 2^po2) | 7,340,032 |
| executor wall-clock | 0.3 s |

#### SPLIT execution 2: final exponentiation + check from the Fp12 bytes

| step | blst-accel |
|---|---:|
| F9 final exponentiation + is_one | 3,581,112 |
| F  sum (one cycle_count delta) | 3,581,827 |
| **execution total, user cycles** (SessionInfo) | 3,588,250 |
| segments (2^20 cycles each) | 4 |
| **execution total incl. paging + padding** (Σ 2^po2) | 4,194,304 |
| executor wall-clock | 0.2 s |

## Earlier snapshot (commit 25aa07e): all six configurations

These were measured earlier the same day, against a copy of ferrata-core equal to commit 25aa07e
apart from rustfmt. At that point `check_committee_onchain` and `miller_half` / `final_half` did not
exist yet: every committee adoption ran the Full check, and "one execution" restored the store with
`from_core`. The copy was deleted when the bench moved into the repository. The raw files are in
`results/2026-11-06_snapshot-25aa07e/`.

### Headline at the time (`blst-accel`)

| What (update_1: mainnet, period 1900, 510/512 signers) | user cycles |
|---|---:|
| SSZ decode of the update | 0.055 M |
| `process_update`, finality update (no committee adoption) | **10.21 M** |
| `process_update` with the committee handoff (adopts the next committee: `check_committee`) | **240.16 M** |
| of which `check_committee` (512 × KeyValidate, sum, compare) | 229.18 M |
| of which the 512 per-key G1 subgroup checks alone | 180.49 M |
| One whole execution that restores the store with `from_core` (the API's only constructor from saved state), decodes and runs `process_update`: finality / handoff | 468.73 M / 698.68 M |
| of which `FinalizedStore::from_core` (re-runs `check_committee` on both committees) | 458.36 M |
| Split, execution 1: decode → SSZ checks → signing key → signature checks → hash-to-G2 → Miller loops (commits the 576-byte Fp12) | 6.39 M user (7 segments, 7.34 M with padding) |
| Split, execution 2: final exponentiation + `== 1` from those bytes | 3.59 M user (4 segments, 4.19 M with padding) |

### All configurations at a glance

M = 10^6 user cycles. P1/P2 and the adoption rows are `env::cycle_count()` deltas inside the guest; the execution rows are `SessionInfo` totals. Generated by `tools/table.py results/2026-11-06_snapshot-25aa07e --summary`.

| | blst-accel | blst-accel-v0314 | zkcrypto-accel | zkcrypto-plain | blst-plain | nopatch |
|---|---:|---:|---:|---:|---:|---:|
| SSZ decode of the update (1) | 0.055 M | 0.055 M | 0.055 M | 0.055 M | 0.055 M | 0.055 M |
| `process_update`, finality update, no committee adoption (P2) | 10.21 M | 12.16 M | 25.02 M | 162.70 M | 172.82 M | 163.61 M |
|   of which `verify_checked`, subtractive (B1) | 9.66 M | 11.60 M | 24.46 M | 162.14 M | 172.27 M | 162.21 M |
| `process_update` with committee handoff + adoption (P1) | 240.16 M | 241.74 M | 554.94 M | 3,292.19 M | 4,002.35 M | 3,300.98 M |
|   of which adoption, `check_committee`: 512 × KeyValidate (10a′) | 229.18 M | 228.82 M | 529.16 M | 3,128.73 M | 3,828.76 M | 3,128.73 M |
|   of which the 512 G1 subgroup checks (10g) | 180.49 M | 178.57 M | 433.92 M | 2,170.47 M | 2,797.51 M | 2,170.47 M |
| adoption without per-key subgroup check, compressed keys (10a0) | 48.60 M | 49.95 M | 94.74 M | 957.73 M | 1,030.95 M | 957.73 M |
| adoption from uncompressed keys, with subgroup checks (10b) | 185.10 M | 185.85 M | 440.18 M | 2,205.09 M | 2,846.20 M | 2,205.09 M |
| adoption from uncompressed keys, no subgroup checks (10b0) | 4.56 M | 7.23 M | 6.26 M | 34.62 M | 48.63 M | 34.63 M |
| one execution, finality update: total user cycles | 468.73 M | 469.95 M | 1,083.50 M | 6,420.32 M | 7,830.51 M | 6,421.28 M |
|   of which `from_core` (2 × `check_committee`) | 458.36 M | 457.64 M | 1,058.33 M | 6,257.47 M | 7,657.53 M | 6,257.47 M |
| one execution, handoff update: total user cycles | 698.68 M | 699.53 M | 1,613.43 M | 9,549.82 M | 11,660.03 M | 9,558.65 M |
| one execution, handoff update: Σ 2^po2 (incl. paging, padding) | 730.33 M | 729.84 M | 1,718.62 M | 10,134.49 M | 12,094.28 M | 10,143.92 M |
| split execution 1 (… Miller loops), user cycles | 6.39 M | 7.38 M | n/a | n/a | 122.63 M | n/a |
|   segments | 7 | 8 | n/a | n/a | 122 | n/a |
| split execution 2 (final exp + check), user cycles | 3.59 M | 4.55 M | n/a | n/a | 49.96 M | n/a |
|   segments | 4 | 5 | n/a | n/a | 50 | n/a |
| E2E execution (init + update_0 + update_1), user cycles | 711.28 M | 713.98 M | 1,640.65 M | 9,711.37 M | 11,831.24 M | 9,737.25 M |

### Observations at the time

1. **A finality update is pairing-bound.** With `blst-accel` (10.21 M): the two Miller loops are
   4.09 M (40%), the final exponentiation 3.58 M (35%), hash-to-G2 1.17 M (11%), signature
   decompression + G2 subgroup check 0.53 M (5%), the subtractive signing key 0.29 M (3%), and
   the SSZ checks inside `process_update` (two header-validity checks, finality branch, signing
   root) 0.12 M. The remaining 0.44 M is the rest of `process_update` (store clone, empty-committee
   checks, participation counts; not broken down). Decoding the update adds 0.055 M.
2. **A committee handoff costs 23× a finality update** (240.16 M vs 10.21 M), and almost all of it
   is `check_committee` (229.18 M). Within that, the 512 per-key G1 subgroup checks are 180.49 M
   (0.35 M each, four times a G1 decompression at 0.09 M). Taking the 512 keys uncompressed
   removes the decompressions: 48.60 M → 4.56 M without subgroup checks (10a0 → 10b0), but
   185.10 M with them (10b). So the subgroup checks, not the decompressions, set the cost of
   adoption in the current design.
3. **Subtractive vs additive signing key:** 0.29 M vs 48.32 M at 510/512 signers. Even with every
   key already decompressed, the 510 additions alone are 2.47 M (5a′).
4. **`from_core` on every execution would dominate.** Rebuilding the store from saved state with
   `FinalizedStore::from_core` re-runs `check_committee` on both committees: 458.36 M per
   execution (`blst-accel`), 45× the finality update it wraps. A one-update program that restores
   its store this way costs 468.73 M (finality) or 698.68 M (handoff) per execution; one that
   trusts its own saved, already-checked state would cost ≈ 10.4 M or ≈ 240.3 M.
5. **One 2-pair Miller loop** (`blst_miller_loop_n`) instead of ferrata's two single-pair
   loops costs 3.25 M instead of 4.09 M, saving 0.84 M (8%) per update.
6. **Library and acceleration.** Per finality update, accelerated blst (10.21 M) is 2.45× cheaper
   than accelerated zkcrypto (25.02 M). Unaccelerated, the two are close (blst ≈ 172.8 M,
   zkcrypto ≈ 162.7 M), so RISC Zero's blst patch buys ≈ 17×, its zkcrypto patch ≈ 6.5×. The blst
   tag listed in the v3.0.5 docs (`blst/v0.3.14-risczero.0`) gives the same verdicts at 12.16 M,
   19% more than `v0.3.16-risczero.0`.
7. **The SHA-256 patch** makes SSZ hashing ≈ 8× cheaper (SyncCommittee root 8.95 M → 1.12 M,
   header validity 0.38 M → 0.05 M each; `nopatch` vs `zkcrypto-plain`). With accelerated BLS
   that is visible: without it the SSZ work inside a finality update would grow from 0.12 M to
   ≈ 0.96 M, and a handoff would cost ≈ 8.7 M more.
8. **The split works at the planned seam.** Execution 1 (decode, SSZ checks, signing root,
   signing key, signature checks, hash-to-G2, both Miller loops) is 6.39 M user cycles in 7
   segments; it commits the 576-byte `blst_fp12`. Execution 2 (final exponentiation and the
   `== 1` check from those bytes) is 3.59 M user cycles in 4 segments. Together, 9.98 M, close to
   a finality update's 10.27 M (decode + `process_update`): the split adds only the I/O of 576
   bytes and leaves out the store bookkeeping.

### Full tables, exact user cycles

Generated by `tools/table.py results/2026-11-06_snapshot-25aa07e` from the `<config>.e2e+phases+split+single.txt` files there. For zkcrypto, row 8 is 8p + 8l and row 9 compares with `Gt::identity()`; `n/a` means the step does not apply to that library (8m is blst-only; the split needs blst's raw `blst_fp12`).

#### PHASES: update_1 against the post-update_0 store

| step | blst-accel | blst-accel-v0314 | zkcrypto-accel | zkcrypto-plain | blst-plain | nopatch |
|---|---:|---:|---:|---:|---:|---:|
| 1  SSZ decode of update_1 (26,906 bytes) | 55,090 | 55,090 | 55,092 | 55,095 | 55,090 | 55,092 |
| 2a header validity, attested (payload header root + 4-deep execution branch) | 48,174 | 48,174 | 48,175 | 48,175 | 48,174 | 378,781 |
| 2b header validity, finalized | 48,168 | 48,168 | 48,168 | 48,168 | 48,168 | 378,774 |
| 3a SyncCommittee hash_tree_root (512 keys + aggregate) | 1,123,291 | 1,123,291 | 1,123,291 | 1,123,291 | 1,123,291 | 8,950,980 |
| 3b next-committee branch check (6 deep) | 8,086 | 8,086 | 8,085 | 8,087 | 8,086 | 53,495 |
| 3c finality: finalized beacon header root + branch check (7 deep) | 17,101 | 17,101 | 17,101 | 17,104 | 17,101 | 123,989 |
| 4  signing root (attested header root, fork version, domain) | 10,279 | 10,279 | 10,280 | 10,280 | 10,279 | 79,327 |
| info: update_1 signers | 510 | 510 | 510 | 510 | 510 | 510 |
| info: update_1 non-signers | 2 | 2 | 2 | 2 | 2 | 2 |
| 5s aggregation, subtractive: decompress aggregate + non-signers, add, negate | 285,875 | 293,564 | 556,248 | 5,595,438 | 6,051,513 | 5,595,428 |
| 5a aggregation, additive: decompress every signer key + add | 48,318,251 | 49,574,863 | 94,190,765 | 950,372,361 | 1,024,808,593 | 950,372,357 |
| 5a' additive, point additions only (keys already decompressed) | 2,468,202 | 2,495,742 | 4,493,494 | 21,777,107 | 32,239,452 | 21,777,106 |
| 5d one G1 decompression (for scale) | 89,958 | 92,400 | 175,964 | 1,820,866 | 1,946,302 | 1,820,864 |
| 5g one G1 subgroup check, affine (for scale) | 352,595 | 348,848 | 847,586 | 4,239,280 | 5,463,978 | 4,239,277 |
| 5k G1 subgroup check of the aggregate key (earlier design; not in verify_checked) | 352,307 | 348,548 | 849,155 | 6,052,034 | 5,463,678 | 6,052,034 |
| 5n identity check + negate key + to affine (as pairing_check) | 1,871 | 92,811 | 1,951 | 1,813,141 | 178,527 | 1,813,138 |
| 6a signature decompression (G2) | 183,728 | 189,066 | 593,010 | 11,386,760 | 3,949,220 | 11,386,757 |
| 6b signature G2 subgroup check + not identity | 343,140 | 442,798 | 760,560 | 7,826,580 | 7,187,856 | 7,826,578 |
| 7  hash_to_G2(signing root) + to affine | 1,169,105 | 1,597,276 | 2,603,759 | 35,082,628 | 25,192,835 | 35,155,831 |
| 8p G2Prepared::from x2 (line precomputation) | n/a | n/a | 2,415,747 | 16,297,418 | n/a | 16,297,418 |
| 8l multi_miller_loop, 2 pairs (prepared) | n/a | n/a | 6,396,415 | 37,956,706 | n/a | 37,956,706 |
| 8  Miller loops as ferrata: ML(H(m),-key) + ML(sig,g1) + Fp12 mul | 4,087,690 | 4,439,734 | 8,812,162 | 54,254,124 | 79,753,003 | 54,254,124 |
| 8m alternative: one 2-pair blst_miller_loop_n | 3,251,802 | 3,589,657 | n/a | n/a | 65,831,039 | n/a |
| 9  final exponentiation + is_one | 3,581,111 | 4,540,856 | 10,955,696 | 46,008,641 | 49,950,200 | 46,008,641 |
| B1 backend verify_checked, subtractive (ferrata code, policy store path) | 9,656,126 | 11,600,307 | 24,460,930 | 162,140,669 | 172,267,356 | 162,213,907 |
| B2 backend verify_checked, additive (ferrata code) | 57,683,915 | 60,976,389 | 118,135,083 | 1,106,915,469 | 1,191,119,219 | 1,106,988,708 |
| B3 backend fast_aggregate_verify (spec path: KeyValidate on every signer key) | 237,568,558 | 239,050,028 | 550,656,680 | 3,269,268,545 | 3,977,909,158 | 3,269,341,784 |
| 10a adoption, compressed: 512 x KeyValidate (decompress + G1 subgroup) + sum + compare aggregate | 229,193,153 | 228,628,890 | 528,771,299 | 3,128,358,422 | 3,828,573,244 | 3,128,358,422 |
| 10a0 same, on-curve only (no per-key subgroup check; earlier design) | 48,598,973 | 49,953,174 | 94,737,624 | 957,732,885 | 1,030,950,968 | 957,734,421 |
| 10b adoption, uncompressed: 512 x (deserialize+on-curve, recompress==committed, G1 subgroup) + sum + compress==aggregate | 185,103,996 | 185,853,390 | 440,178,481 | 2,205,094,943 | 2,846,203,554 | 2,205,094,943 |
| 10b0 same, uncompressed without the per-key subgroup check | 4,562,038 | 7,229,896 | 6,256,426 | 34,624,028 | 48,633,500 | 34,625,561 |
| 10g 512 G1 subgroup checks alone (for scale) | 180,486,735 | 178,568,271 | 433,921,615 | 2,170,467,411 | 2,797,514,831 | 2,170,467,407 |
| 10a' backend check_committee (ferrata code) | 229,181,886 | 228,818,340 | 529,162,054 | 3,128,732,202 | 3,828,762,694 | 3,128,732,202 |
| V  validate_light_client_update(update_1), CheckedCommittee path, Auto (= subtractive) | 10,913,471 | 12,857,652 | 25,718,275 | 163,398,013 | 173,524,701 | 172,181,495 |
| P1 process_update(update_1) as served: finality + handoff (adopts next committee) | 240,157,634 | 241,738,269 | 554,942,607 | 3,292,192,494 | 4,002,349,672 | 3,300,975,974 |
| P2 process_update(update_1 stripped to a finality update): no committee adoption | 10,211,785 | 12,155,966 | 25,016,588 | 162,696,326 | 172,823,015 | 163,606,709 |
| **execution total, user cycles** (SessionInfo) | 2,063,489,130 | 2,083,859,614 | 4,696,739,865 | 29,048,041,403 | 35,077,084,973 | 29,075,578,835 |
| segments (2^20 cycles each) | 2057 | 2074 | 4761 | 29359 | 34703 | 29387 |
| **execution total incl. paging + padding** (Σ 2^po2) | 2,156,920,832 | 2,174,746,624 | 4,991,352,832 | 30,784,126,976 | 36,388,208,640 | 30,813,978,624 |
| executor wall-clock | 64.1 s | 76.8 s | 140.5 s | 592.7 s | 590.0 s | 591.3 s |

#### E2E: bootstrap -> update_0 -> update_1

| step | blst-accel | blst-accel-v0314 | zkcrypto-accel | zkcrypto-plain | blst-plain | nopatch |
|---|---:|---:|---:|---:|---:|---:|
| E1 decode bootstrap (SSZ) | 41,313 | 41,313 | 41,313 | 41,313 | 41,313 | 41,313 |
| E2 FinalizedStore::initialize (incl. check_committee on 512 keys) | 230,421,731 | 230,060,076 | 530,403,782 | 3,129,973,930 | 3,830,004,430 | 3,138,231,544 |
| E3 decode update_0 (SSZ) | 55,816 | 55,816 | 55,816 | 55,816 | 55,816 | 55,816 |
| E4 process_update(update_0): current committee signs, adopts next committee | 240,512,955 | 241,998,019 | 555,119,378 | 3,289,011,652 | 3,998,699,260 | 3,297,795,132 |
| E5 decode update_1 (SSZ) | 55,094 | 55,094 | 55,092 | 55,092 | 55,094 | 55,092 |
| E6 process_update(update_1): handoff, next committee signs, rotates, adopts | 240,157,633 | 241,738,268 | 554,942,602 | 3,292,192,488 | 4,002,349,671 | 3,300,975,968 |
| E  sum E1..E6 (one cycle_count delta) | 711,246,348 | 713,950,392 | 1,640,619,756 | 9,711,332,064 | 11,831,207,390 | 9,737,156,638 |
| info: update_0 signers | 512 | 512 | 512 | 512 | 512 | 512 |
| info: update_1 signers | 510 | 510 | 510 | 510 | 510 | 510 |
| **execution total, user cycles** (SessionInfo) | 711,280,694 | 713,984,738 | 1,640,654,530 | 9,711,366,838 | 11,831,241,736 | 9,737,245,323 |
| segments (2^20 cycles each) | 710 | 711 | 1668 | 9829 | 11705 | 9855 |
| **execution total incl. paging + padding** (Σ 2^po2) | 743,964,672 | 745,537,536 | 1,748,500,480 | 10,305,470,464 | 12,272,795,648 | 10,332,930,048 |
| executor wall-clock | 24.0 s | 28.1 s | 46.5 s | 194.3 s | 204.8 s | 198.0 s |

#### SINGLE: one execution = restore store + decode + process_update(update_1 as served, handoff)

| step | blst-accel | blst-accel-v0314 | zkcrypto-accel | zkcrypto-plain | blst-plain | nopatch |
|---|---:|---:|---:|---:|---:|---:|
| R1 decode store state (finalized header + 2 committees, SSZ) | 74,128 | 74,128 | 74,127 | 74,127 | 74,128 | 74,127 |
| R2 FinalizedStore::from_core (re-checks both committees) | 458,362,750 | 457,637,976 | 1,058,325,402 | 6,257,465,698 | 7,657,526,684 | 6,257,465,698 |
| R3 decode update (SSZ) | 55,094 | 55,094 | 55,092 | 55,092 | 55,094 | 55,092 |
| R4 process_update | 240,157,632 | 241,738,267 | 554,942,600 | 3,292,192,486 | 4,002,349,670 | 3,300,975,966 |
| R  sum R1..R4 + new root (one cycle_count delta) | 698,659,224 | 699,515,085 | 1,613,406,660 | 9,549,796,842 | 11,660,015,196 | 9,558,634,233 |
| info: adopted a committee (1 = yes) | 1 | 1 | 1 | 1 | 1 | 1 |
| **execution total, user cycles** (SessionInfo) | 698,678,272 | 699,534,133 | 1,613,425,939 | 9,549,816,121 | 11,660,034,244 | 9,558,653,512 |
| segments (2^20 cycles each) | 697 | 697 | 1639 | 9665 | 11534 | 9674 |
| **execution total incl. paging + padding** (Σ 2^po2) | 730,333,184 | 729,841,664 | 1,718,616,064 | 10,134,487,040 | 12,094,275,584 | 10,143,924,224 |
| executor wall-clock | 19.5 s | 25.8 s | 49.6 s | 142.4 s | 145.6 s | 144.1 s |

#### SINGLE: one execution = restore store + decode + process_update(update_1 as finality update)

| step | blst-accel | blst-accel-v0314 | zkcrypto-accel | zkcrypto-plain | blst-plain | nopatch |
|---|---:|---:|---:|---:|---:|---:|
| R1 decode store state (finalized header + 2 committees, SSZ) | 74,128 | 74,128 | 74,127 | 74,127 | 74,128 | 74,127 |
| R2 FinalizedStore::from_core (re-checks both committees) | 458,362,750 | 457,637,976 | 1,058,325,402 | 6,257,465,698 | 7,657,526,684 | 6,257,465,698 |
| R3 decode update (SSZ) | 55,094 | 55,094 | 55,092 | 55,092 | 55,094 | 55,092 |
| R4 process_update | 10,211,782 | 12,155,963 | 25,016,582 | 162,696,320 | 172,823,012 | 163,606,703 |
| R  sum R1..R4 + new root (one cycle_count delta) | 468,713,374 | 469,932,781 | 1,083,480,642 | 6,420,300,676 | 7,830,488,538 | 6,421,264,970 |
| info: adopted a committee (1 = yes) | 0 | 0 | 0 | 0 | 0 | 0 |
| **execution total, user cycles** (SessionInfo) | 468,732,422 | 469,951,829 | 1,083,499,921 | 6,420,319,955 | 7,830,507,586 | 6,421,284,249 |
| segments (2^20 cycles each) | 468 | 468 | 1101 | 6498 | 7746 | 6499 |
| **execution total incl. paging + padding** (Σ 2^po2) | 490,209,280 | 490,733,568 | 1,153,957,888 | 6,813,646,848 | 8,122,269,696 | 6,814,695,424 |
| executor wall-clock | 15.7 s | 18.5 s | 40.5 s | 101.0 s | 100.6 s | 100.3 s |

#### SPLIT execution 1: up to and including the Miller loop (commits Fp12)

| step | blst-accel | blst-accel-v0314 | blst-plain |
|---|---:|---:|---:|
| S1 decode update_1 (SSZ) | 55,093 | 55,093 | 55,093 |
| S2 decode signing committee (SSZ, 513 keys) | 34,017 | 34,017 | 34,017 |
| S3 header validity x2 + finality branch | 113,289 | 113,289 | 113,289 |
| S4 signing root | 10,281 | 10,281 | 10,281 |
| S5 signing key, subtractive, negated, affine | 287,657 | 386,286 | 6,229,951 |
| S6 signature decompression + G2 subgroup check | 526,789 | 631,785 | 11,136,997 |
| S7 hash_to_G2 | 1,169,097 | 1,597,268 | 25,192,827 |
| S8 Miller loops (as ferrata: 2 x single-pair + Fp12 mul) | 4,087,676 | 4,439,720 | 79,752,989 |
| S  sum (one cycle_count delta, excl. input read and journal commit) | 6,290,343 | 7,274,183 | 122,531,888 |
| **execution total, user cycles** (SessionInfo) | 6,391,604 | 7,375,444 | 122,633,149 |
| segments (2^20 cycles each) | 7 | 8 | 122 |
| **execution total incl. paging + padding** (Σ 2^po2) | 7,340,032 | 7,864,320 | 127,926,272 |
| executor wall-clock | 0.2 s | 0.3 s | 1.6 s |

#### SPLIT execution 2: final exponentiation + check from the Fp12 bytes

| step | blst-accel | blst-accel-v0314 | blst-plain |
|---|---:|---:|---:|
| F9 final exponentiation + is_one | 3,581,112 | 4,540,857 | 49,950,201 |
| F  sum (one cycle_count delta) | 3,581,827 | 4,541,572 | 49,950,916 |
| **execution total, user cycles** (SessionInfo) | 3,588,243 | 4,547,988 | 49,957,332 |
| segments (2^20 cycles each) | 4 | 5 | 50 |
| **execution total incl. paging + padding** (Σ 2^po2) | 4,194,304 | 5,242,880 | 52,428,800 |
| executor wall-clock | 0.1 s | 0.2 s | 0.7 s |

## Method

**Data.** `crates/ferrata-spectests/fixtures/mainnet-p1899/` (SHA256SUMS checked by the host):
the period-1899 bootstrap (slot 15,556,608), `update_0` (period 1899, 512/512 signers, carries the
period-1900 committee) and `update_1` (period 1900, **510/512 signers**, signed by the store's
*next* committee, finalizes slot 15,564,800, carries the period-1901 committee). Fulu data in the
Electra light-client layout, mainnet preset, n = 512, mainnet `ChainSpec` and
`genesis_validators_root` as in `tests/mainnet_replay.rs`.

**Metric.** Each phase is a delta of `env::cycle_count()`. In zkVM 3.0.5 that counter is the
session's *user* cycles (executed instructions plus accelerator cycles, summed across segments;
it excludes paging and power-of-two padding; source: `risc0-circuit-rv32im` 4.0.x
`get_cycle() = self.cycles.user`). Per execution the host also reports `SessionInfo::cycles()`
(user cycles) and Σ 2^po2 over the segments (what proving pays: user + paging + padding), with the
default segment size po2 = 20. Runs are deterministic: repeated runs gave identical counts.

**Guest executions** (all in `guests/bench-core/src/lib.rs`):

* `E2E`: decode bootstrap → `FinalizedStore::initialize` → decode + `process_update(update_0)`
  → decode + `process_update(update_1)`, `Aggregation::Auto`, each step timed. This is the
  suggested run.
* `PHASES`: phases 1–10 for `update_1` against the store state after `update_0`, with raw library
  calls that mirror ferrata's backends step by step (`lib_blst.rs`, `lib_zk.rs`), then the
  ferrata backend calls themselves (B1–B3, 10a′) and whole-update numbers (V, P1, P2). P2 is
  `update_1` with `next_sync_committee` and its branch zeroed, i.e. the same update as a
  finality update: same signature, same finalized header, no committee to adopt.
* `SINGLE`: one update per execution, as a program would run it: read the store state (finalized
  header + both committees as SSZ, 49 KB) and the update, rebuild the store with
  `FinalizedStore::from_core` (the only constructor from saved state; it re-runs
  `check_committee` on both committees), decode, `process_update`, commit the new root.
* `SPLIT` (blst only, earlier run): execution 1 runs everything up to and including the two
  Miller loops and commits the raw `blst_fp12` (576 bytes, Montgomery form); execution 2 reads
  those bytes, runs `blst_final_exp` and `blst_fp12_is_one`.
* `ONCHAIN` (real core): `initialize_with(.., Adoption::OnChain)` from the bootstrap, then
  `process_update(update_0)`, then on clones of that store `process_update(update_1)` as served
  (A1) and as a finality update (A2). It also times the committee checks alone (B1–B3), the
  restore paths (R1 decode the saved state, R2 `from_core`) and the OnChain policy run with public
  functions on the decoded `StoreCore` (M1, M2), asserting that M1/M2 equal A1/A2 in outcome and
  state.
* `SINGLE-ONCHAIN` (real core): one whole execution per update. It decodes the saved state and the
  update, runs that OnChain policy and commits the new finalized root; no committee is re-checked.
* `CORESPLIT` (real core, blst): execution 1 decodes update_1 and the signing committee, runs the
  supermajority check, both header-validity checks, the finality branch and the signing root,
  then `Blst::miller_half`, and commits its 576 bytes; execution 2 runs `Blst::final_half` on them.
* `STEPS` (real core, blst): the 3-step job for a finality update. Step 1 is the same prelude, then
  the signing key, the signature checks, hash-to-G2 and the negated key, handing over 480 bytes
  (H(m), −key, signature, uncompressed). Step 2 deserializes them (on-curve checks), runs the
  Miller loops and hands over the 576-byte product in the core's encoding. Step 3 is
  `Blst::final_half`.

**Phase definitions** (the row codes in the tables):

| code | what is timed |
|---|---|
| 1 | `LightClientUpdate::decode` of update_1 (26,906 bytes) |
| 2a / 2b | `is_valid_light_client_header` for the attested / finalized header (Deneb payload-header root, 4-deep branch) |
| 3a | `SyncCommittee::hash_tree_root` (512 keys + aggregate: 1,025 SHA-256 hashes of 64-byte inputs, 2,050 compressions) |
| 3b | the 6-deep `next_sync_committee` branch, given 3a |
| 3c | finalized beacon header root + 7-deep finality branch |
| 4 | `sync_committee_signing_root` (attested header root, fork version, domain) |
| 5s / 5a | signing key, subtractive (decompress aggregate + 2 non-signers, add, negate) / additive (decompress 510 signer keys, add) |
| 5a′ | the 510 point additions alone, keys already decompressed |
| 5d / 5g | one G1 decompression / one G1 subgroup check, for scale |
| 5k | subgroup check of the aggregate signing key: in the earlier design, **not** in the current `verify_checked` (adopted committees are KeyValidated per key instead). zkcrypto's figure includes the conversion to affine (an inversion) |
| 5n | identity check, negate, to affine (the inversion), as `pairing_check` does |
| 6a / 6b | signature decompression / G2 subgroup check + not-identity |
| 7 | `hash_to_G2(signing_root, DST)` + to affine (expand_message_xmd SHA-256 included) |
| 8 | the two Miller loops as ferrata computes them: blst `ML(H(m), −key)`, `ML(sig, g1)`, one Fp12 mul; zkcrypto `G2Prepared` × 2 (8p) + 2-pair `multi_miller_loop` (8l) |
| 8m | blst alternative: one 2-pair `blst_miller_loop_n` (shares the Fp12 squarings) |
| 9 | final exponentiation + `== 1` |
| B1 / B2 | ferrata `verify_checked`, subtractive / additive |
| B3 | ferrata `fast_aggregate_verify` (the spec path: KeyValidate on each of the 510 signer keys) |
| 10a | adoption check from compressed keys, as `check_committee` now does it: 512 × KeyValidate (decompress, not identity, G1 subgroup check), sum, decompress aggregate, compare |
| 10a0 | the same without the per-key subgroup check (the earlier design) |
| 10b | adoption check from 512 *uncompressed* keys (96 bytes each, supplied by the host): deserialize with on-curve check, re-compress and compare with the committed 48 bytes, G1 subgroup check, sum, compress the sum and compare with `aggregate_pubkey` |
| 10b0 | 10b without the per-key subgroup check |
| 10g | the 512 G1 subgroup checks alone |
| 10a′ | ferrata `check_committee` itself |
| V | `validate_light_client_update(update_1)`, `CheckedCommittee` path, Auto (= subtractive at 510/512) |
| P1 / P2 | `FinalizedStore::process_update(update_1)` as served (handoff: adopts the period-1901 committee) / as a finality update (no adoption) |
| ONCHAIN O1–O5 | decode bootstrap; `initialize_with(.., OnChain)`; decode + `process_update(update_0)`; decode update_1 |
| ONCHAIN A1 / A2 | `process_update(update_1)` on the OnChain store: as served (handoff, `check_committee_onchain`) / as a finality update |
| ONCHAIN B1 / B2 / B3 | `check_committee_onchain` / `check_committee` (Full) on the period-1901 committee / its `hash_tree_root` + next-committee branch |
| ONCHAIN R1 / R2 | decode the saved post-update_0 state / `FinalizedStore::from_core` on it |
| ONCHAIN M1 / M2 | the OnChain policy on the decoded `StoreCore` with public functions: handoff / finality |
| Q1–Q3 | SINGLE-ONCHAIN: decode saved state; decode update; the OnChain policy |
| C1a–C1e, C2 | CORESPLIT: decode update_1; decode committee; supermajority + header validity ×2 + finality branch; signing root; `miller_half` / `final_half` |
| T1a–T1i | STEP 1: as C1a–C1d, then signing key, signature decompression + G2 subgroup check, hash-to-G2, negate key + to affine, serialize 480 bytes |
| T2a–T2c | STEP 2: deserialize the three points, the two Miller loops + Fp12 mul, encode 576 bytes |

## Configurations

| name | BLS library | SHA-256 (`sha2` 0.10 for SSZ, 0.9 for zkcrypto hash-to-curve) |
|---|---|---|
| `blst-accel` | RISC Zero blst fork, tag `v0.3.16-risczero.0` (commit 7d1fc3e, crate 0.3.16; newest tag, its dev-deps were moved to risc0-zkvm 3.0; uses `risc0-bigint2` 1.4.14) | RISC Zero patch `sha2-v0.10.8-risczero.0` |
| `blst-accel-v0314` | RISC Zero blst fork, tag `blst/v0.3.14-risczero.0` (commit 4bb7133, crate 0.3.14): the tag the zkVM **v3.0.5** docs list (`website/api/zkvm/precompiles.md` at tag v3.0.5) | patched |
| `zkcrypto-accel` | RISC Zero zkcrypto fork, tag `bls12_381/v0.8.0-risczero.1` (commit a3192e1, crate 0.8.0, `risc0-bigint2` 1.4.14) | patched (0.10.8 and `sha2-v0.9.9-risczero.0`) |
| `zkcrypto-plain` | crates.io `bls12_381` 0.8.0, pure Rust | patched |
| `blst-plain` | crates.io `blst` 0.3.16, no patch (portable C, built with the rzup riscv32 GCC) | patched |
| `nopatch` | crates.io `bls12_381` 0.8.0 | crates.io `sha2` 0.10.8 / 0.9.9 (no RISC Zero patch at all) |

Priority (i) is `blst-accel`, (ii) `zkcrypto-accel`, (iii) `zkcrypto-plain`; the other three are
extra comparisons. All six built and ran.

## Versions

| component | version |
|---|---|
| zkVM crates (guest and host) | `risc0-zkvm` 3.0.5, `risc0-build` 3.0.5, `risc0-zkvm-platform` 2.2.3, `risc0-binfmt` 3.0.5, `risc0-circuit-rv32im` 4.0.5, `risc0-bigint2` 1.4.14 (accelerated configs) |
| executor | `r0vm` 3.0.5 (rzup component `r0vm` 3.0.5, from the `cargo-risczero` 3.0.5 release), run over IPC by `default_executor()` (host crate built without the `prove` feature) |
| guest toolchain | RISC Zero Rust `rustc 1.88.0-dev (de85b1d3d 2025-06-26)` (rzup `rust` 1.88.0, the version risc0-build 3.0.5's Docker tag `r0.1.88.0` names); C: rzup `cpp` 2024.1.5, `riscv32-unknown-elf-gcc 13.2.0` |
| host toolchain | `rustc 1.93.0 (254b59607 2026-01-19)`, `cargo 1.93.0` |
| guest profile | `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, built by risc0-build (release) |
| patched crates | blst `v0.3.16-risczero.0` (7d1fc3e) and `blst/v0.3.14-risczero.0` (4bb7133) from github.com/risc0/blst; bls12_381 `bls12_381/v0.8.0-risczero.1` (a3192e1) from github.com/risc0/zkcrypto-bls12_381; sha2 `sha2-v0.10.8-risczero.0` (244dc3b) and `sha2-v0.9.9-risczero.0` (c2b5d71) from github.com/risc0/RustCrypto-hashes. Each guest's `Cargo.lock` pins them. |
| guest image measured against the real core | `blst_accel` sha256 `651d5639e25c6008…` |
| guest images of the earlier snapshot run (sha256 of the `.bin` that `methods` embeds) | `blst_accel` `adc296e76c5816d1…`; `blst_accel_v0314` `0a273f7e2157167a…`; `blst_plain` `2e80b4e3540e6cf9…`; `nopatch` `626ac0527518e3b6…`; `zkcrypto_accel` `843df2e115cc5b83…`; `zkcrypto_plain` `c3b05d0697c408aa…` |
| ferrata-core, headline run | `../crates/plumbline-core` by path: commit 25aa07e plus the uncommitted working tree of 6 Nov 2026 09:43 CET (`results/PROVENANCE.txt`) |
| ferrata-core, earlier snapshot run | a copy equal to commit **25aa07e** ("Apply round-1 review: exact BLS semantics, mainnet replay, guard tests") except rustfmt line wrapping at three places in the two BLS backend files |

**Why the earlier run used a snapshot.** While the bench was first being written, ferrata-core's
working tree changed under it (between 08:35 and
08:55: `verify_sync_aggregate` became `fast_aggregate_verify` / `verify_checked`,
`check_committee` gained the per-key subgroup check, `FinalizedStore.core` became private and
`from_core` appeared; the result was committed as 25aa07e at 08:55). To measure one consistent
version, the sources were copied at 08:50 into a snapshot (`diff -r` against 25aa07e showed only
formatting). The bench now lives in the repository as `bench/` and depends on
`../crates/ferrata-core` by path; the snapshot was deleted.

## Reproduce

See `README.md` for the commands. In short, from `spike/ferrata/bench`:

```sh
export TMPDIR=$PWD/../../tmp                 # spike/tmp; never /tmp on this machine
cargo build --release -p host                # risc0-build compiles the six guests, then the host
./target/release/host blst-accel             # every mode for blst-accel, ~2.5 min -> results/blst-accel.all.txt
python3 -I tools/table.py results --headline # the headline table above
python3 -I tools/table.py results > results/tables.md
```

`./target/release/host` with no argument runs all six configurations (about an hour; the three
unaccelerated ones take about 15 min each). The guest lockfiles were resolved for Rust 1.88 with
`CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo update --manifest-path guests/<g>/Cargo.toml`
(and `rust-version = "1.88"` in the guest manifests); keep them, or regenerate them that way.

## What was installed, and how to remove it

No `sudo`, and no shell rc or profile file was touched: `rzup` was installed from crates.io (its
0.5.2 source contains no rc-file code; the `curl … | bash` installer, which edits rc files, was
not used), and `~/.cargo/bin` is already on `PATH`.

| what | where |
|---|---|
| `cargo install rzup --version 0.5.2 --locked --no-default-features --features cli,install` | `~/.cargo/bin/rzup` (7.4 MB) |
| `rzup install rust 1.88.0` | `~/.risc0/toolchains/v1.88.0-rust-x86_64-unknown-linux-gnu` (1.5 GB) and the symlink `~/.rustup/toolchains/risc0` |
| `rzup install cpp` (2024.1.5) | `~/.risc0/toolchains/v2024.1.5-cpp-x86_64-unknown-linux-gnu` (1.5 GB) and the symlink `~/.risc0/cpp` |
| `rzup install r0vm 3.0.5` | `~/.risc0/extensions/v3.0.5-cargo-risczero-x86_64-unknown-linux-gnu/{r0vm,cargo-risczero}` (119 MB) and the symlinks `~/.cargo/bin/r0vm`, `~/.cargo/bin/cargo-risczero` |
| rzup bookkeeping | `~/.risc0/settings.toml`, `~/.risc0/.rzup`, `~/.risc0/tmp/` (lock files) |
| cargo caches (normal) | git checkouts of risc0/blst, risc0/zkcrypto-bls12_381, risc0/RustCrypto-hashes under `~/.cargo/git/`; crates under `~/.cargo/registry/` |
| this bench | `spike/ferrata/bench/`, moved there from `spike/plumbline-bench/` (`target/` is 3.0 GB of build output, ignored by `bench/.gitignore`); `spike/tmp/` (TMPDIR, left empty). Sources fetched only to read them (crate sources of rzup, risc0-build, risc0-zkvm, risc0-zkvm-platform, risc0-circuit-rv32im, bls12_381; git metadata of the two RISC Zero forks) were kept in `spike/downloads/` during the work and deleted afterwards |

Remove everything RISC Zero:

```sh
rm -rf ~/.risc0 ~/.rustup/toolchains/risc0 ~/.cargo/bin/r0vm ~/.cargo/bin/cargo-risczero
cargo uninstall rzup
rm -rf spike/ferrata/bench/target      # build output only
```

rzup looks for a GitHub token in `GITHUB_TOKEN` or in `~/.config/gh/hosts.yml` (`oauth_token`);
neither held one here, so its downloads from GitHub were unauthenticated. Only public toolchains,
crates and git tags were downloaded; nothing about the user was sent anywhere, and no proving
service (Bonsai/Boundless) was used.

## What did not work, and caveats

* **Only `blst-accel` was re-run against the real core.** The other five configurations' numbers
  come from the earlier snapshot run. Their guests still build against the real core
  (`cargo build` compiles all six), but they were not run again.
* **The core API limits what a program can do;** see "Where the core API stops a program". As a
  result, the whole-execution rows apply the OnChain policy through public functions rather than
  through `FinalizedStore`, and the 3-step rows use raw blst calls for steps 1 and 2 (their
  output was checked byte for byte against the core).
* **The measured core was the working tree, not a commit** (commit 25aa07e plus uncommitted
  changes; `results/PROVENANCE.txt` has the per-file hashes for comparison with the commit that
  follows).
* **First guest build failed on toolchain MSRV.** With lockfiles resolved by the host's cargo
  1.93, risc0-build stopped with: `rustc 1.88.0-dev is not supported by the following packages:
  enum-ordinalize@4.4.2 requires rustc 1.89, enum-ordinalize-derive@4.4.2 requires rustc 1.89,
  ruint@1.21.1 requires rustc 1.90`. Fixed by resolving the guest lockfiles for Rust 1.88
  (enum-ordinalize 4.3.2, ruint 1.17.2), see Reproduce. Nothing else failed to build.
* **RISC Zero's tag documentation is inconsistent.** The v3.0.5 precompiles page gives the
  bls12_381 patch as `tag = "v0.8.0-risczero.0"`, which does not exist (the fork's tags are
  `bls12_381/v0.8.0-risczero.0` and `…risczero.1`). The main-branch page gives blst as
  `blst/v0.3.16-risczero.0`, which does not exist either (the tag is `v0.3.16-risczero.0`). The
  v3.0.5 page's blst tag, `blst/v0.3.14-risczero.0`, predates later fixes in the fork
  ("ZKVM-1383: fix: inverse in `reciprocal`", "don't accelerate modinv", "do not alias input and
  output"). It verified this data correctly (config `blst-accel-v0314`) and costs about 19% more
  per finality update than `v0.3.16-risczero.0` (12.16 M vs 10.21 M). The bench uses the newest
  tag as the primary configuration and reports both.
* **No zkcrypto split.** `bls12_381::MillerLoopResult` keeps its `Fp12` private and has no
  serialization, so splitting zkcrypto at the Miller loop needs a fork or an unsafe transmute. The
  split is shown with blst (`blst-accel`, `blst-accel-v0314`, `blst-plain`).
* **The split's second half trusts its input.** Execution 2 only checks `final_exp(f) == 1` for
  the 576 bytes it is given; binding `f` to execution 1 (receipt composition, or a commitment in
  program state) is the design's job and is not measured.
* **Not measured:** proving (time, memory, receipt size); segment sizes other than po2 = 20;
  LEZ's own per-transaction overhead (account I/O, program shell); update_2 … update_4.
* **Per-phase numbers are user cycles.** Paging appears only in the per-execution totals:
  Σ 2^po2 is 3.7–6.3% above user cycles in the long executions, more in short ones, which pay
  up to one segment of padding (split execution 1: 6.39 M user, 7.34 M total).
  `env::cycle_count()` is reported by the host and is not constrained by the circuit, which is
  fine for measurement.
* **Raw-call phases agree with ferrata's own code.** For `blst-accel` (earlier snapshot run), 5s + 5n + 6a + 6b + 7 +
  8 + 9 = 9,652,520 against `verify_checked` (B1) 9,656,126; 10a 229.19 M against
  `check_committee` (10a′) 229.18 M. zkcrypto agrees within 1% (its backend decodes the signature
  with `from_compressed`, which fuses 6a and 6b).
* **The PHASES store is built with `from_core`** (untimed), not through `initialize` +
  `process_update(update_0)`; `E2E` runs that real path. Both reach the canonical finalized root
  after update_1 (asserted in each).
* Small run-to-run shifts of a few cycles appear only between *different builds* of the guest
  (code layout); repeated runs of one build are identical.
