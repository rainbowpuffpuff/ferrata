Exact user cycles (env::cycle_count deltas; execution rows from SessionInfo). n/a: not applicable to that library.

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
