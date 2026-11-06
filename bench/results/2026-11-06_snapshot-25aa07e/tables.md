Exact user cycles (env::cycle_count deltas; execution rows from SessionInfo). n/a: not applicable to that library.

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
| 8  Miller loops as plumbline: ML(H(m),-key) + ML(sig,g1) + Fp12 mul | 4,087,690 | 4,439,734 | 8,812,162 | 54,254,124 | 79,753,003 | 54,254,124 |
| 8m alternative: one 2-pair blst_miller_loop_n | 3,251,802 | 3,589,657 | n/a | n/a | 65,831,039 | n/a |
| 9  final exponentiation + is_one | 3,581,111 | 4,540,856 | 10,955,696 | 46,008,641 | 49,950,200 | 46,008,641 |
| B1 backend verify_checked, subtractive (plumbline code, policy store path) | 9,656,126 | 11,600,307 | 24,460,930 | 162,140,669 | 172,267,356 | 162,213,907 |
| B2 backend verify_checked, additive (plumbline code) | 57,683,915 | 60,976,389 | 118,135,083 | 1,106,915,469 | 1,191,119,219 | 1,106,988,708 |
| B3 backend fast_aggregate_verify (spec path: KeyValidate on every signer key) | 237,568,558 | 239,050,028 | 550,656,680 | 3,269,268,545 | 3,977,909,158 | 3,269,341,784 |
| 10a adoption, compressed: 512 x KeyValidate (decompress + G1 subgroup) + sum + compare aggregate | 229,193,153 | 228,628,890 | 528,771,299 | 3,128,358,422 | 3,828,573,244 | 3,128,358,422 |
| 10a0 same, on-curve only (no per-key subgroup check; earlier design) | 48,598,973 | 49,953,174 | 94,737,624 | 957,732,885 | 1,030,950,968 | 957,734,421 |
| 10b adoption, uncompressed: 512 x (deserialize+on-curve, recompress==committed, G1 subgroup) + sum + compress==aggregate | 185,103,996 | 185,853,390 | 440,178,481 | 2,205,094,943 | 2,846,203,554 | 2,205,094,943 |
| 10b0 same, uncompressed without the per-key subgroup check | 4,562,038 | 7,229,896 | 6,256,426 | 34,624,028 | 48,633,500 | 34,625,561 |
| 10g 512 G1 subgroup checks alone (for scale) | 180,486,735 | 178,568,271 | 433,921,615 | 2,170,467,411 | 2,797,514,831 | 2,170,467,407 |
| 10a' backend check_committee (plumbline code) | 229,181,886 | 228,818,340 | 529,162,054 | 3,128,732,202 | 3,828,762,694 | 3,128,732,202 |
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
| S8 Miller loops (as plumbline: 2 x single-pair + Fp12 mul) | 4,087,676 | 4,439,720 | 79,752,989 |
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
