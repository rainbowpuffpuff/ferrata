# Results

Every number below comes from a command run in this directory on 10 Nov 2026; logs and JSON lines
are in `results/` (`bench.log`, `micro.log`, `compose-*.log`, `logs/*prove*.log`, `*.jsonl`).
Derived figures show their inputs (`python3 -I scripts/derive.py` recomputes them from the JSON
lines). The program measured implements SPEC-v2 §4.1–§4.2 with SPEC-v3 D3 (checked producer
inscription, 17-word journal, posting timeframe pinned in the key-set hash); image ids: checkpoint
`0x95b816ed2614e340c85fed3ba3dad1f70e7dff2da1363406ade87e1333583caf`, segment
`0xbdfb94a6b6ea12e996f332b28ce4791dc7362f28da602769192f589a61fee043` (local toolchain build, not
reproducible elsewhere as such). The two real proofs (§6) were made just before the
posting-timeframe amendment, with checkpoint image `0xbbf58fed…34c0`; that program differs only in
the key-set hash encoding and the timeframe check, and runs the same input in 3,704,942 user cycles
against 3,705,001 now (same 4 segments).

In the committed logs, the absolute path of the directory the commands ran in is written `<workspace>`
(this directory was then `<workspace>/ferrata-checkpoint`, and the upstream sources were under
`<workspace>/upstream`).

**Specification.** "SPEC-v2 §x" is [`docs/design/spec/bridge-spec-v2.md`](../docs/design/spec/bridge-spec-v2.md)
and "SPEC-v3 Dn" is [`docs/design/spec/bridge-spec-v3-changes.md`](../docs/design/spec/bridge-spec-v3-changes.md).

**Machine.** AMD Ryzen 9 7940HS (8 cores, 16 threads), 54 GiB RAM, no NVIDIA GPU, Fedora 44, Linux
7.2.13. **Versions.** r0vm 3.0.5, risc0-zkvm 3.0.5 (host `client` feature), risc0-build 3.0.5, RISC
Zero guest toolchain Rust 1.88.0, host Rust 1.93.0 (LEZ crates: 1.98.1); RISC Zero patches
`sha2-v0.10.9-risczero.0`, `curve25519-4.1.3-risczero.0`, `crypto-bigint v0.5.5-risczero.0`;
Groth16 prover image `risczero/risc0-groth16-prover:v2025-04-03.1`. **Load (which numbers were measured under load).** Other agents used
the machine for most of the session. Executor cycle counts are deterministic and do not depend on
load; only wall times do. Under load: the final executor runs (`bench.log`, `micro.log`,
`compose-dev.log`, 07:16–07:17, 1-minute load 23–34; the same fixtures were run earlier at load
9–10 in `bench-before-timeframe.log`, differing only by the timeframe change), the first real proof
(03:33–04:02, load 13.2 → 36.8) and its Groth16 wrap (04:02–04:06, 32.9 → 33.2), and the second
Groth16 wrap (07:12–07:15, 36.1 → 55.1). The second succinct proof (06:53–07:12) *started* on a quiet
machine (load 2.95, only this prover running) but other agents resumed before it ended (36.1 at the
end).

## Headline

| | |
|---|---|
| core tests | 13 pass (`cargo +1.88 test -p checkpoint-core`): unit tests and the cross-implementation vectors of `ferrata-bridge`, now `../bridge/` (configId, burn leaf/frontier/root, the v1 keySetHash convention, S from parts, journal words 1–12) |
| fidelity tests | 17 pass (`cargo +1.98.1 test -p fidelity`): 7 format tests against LEZ and Logos Blockchain code, 10 rule tests |
| empty LEZ block | 839 bytes (fee tx 447, clock tx 303); identical size to the empty blocks of LEZ's own sequencer dump |
| marginal cost per empty block | **6,405 cycles** |
| marginal cost per KB of block data | **2,684 cycles/KB** |
| fixed cost per checkpoint (n = 4, t = 3, small `T` and support blocks) | **3.70M cycles** (four inscription checks of 0.89M each) |
| one idle hour (360 blocks) | **5.99M cycles** in one checkpoint, 7 segments |
| one hour with 50 private transactions | **37.9M cycles**, 60 segments |
| `T` and 3 support blocks each holding 4 private transactions | **224.8M cycles**, 232 segments (BLAKE2b over the inscriptions) |
| components | SHA-256 2.59 cycles/B (accelerated; 63.1 without the patch), BLAKE2b-256 **55.9 cycles/B** (no accelerator), Ed25519 `verify_strict` **0.83M** (3.49M without the patch), burn leaf + append 4,234 |
| composition | two pieces (segment of 180 blocks, then a checkpoint verifying it) prove the same journal as one piece (dev mode) |
| real proof (4 segments) | succinct 1,758.9 s under load, 1,157.5 s starting quiet; Groth16 wrap 215.2 s / 195.1 s under load; peak 9.62 GB (host + r0vm), Groth16 prover 7.3 GiB; seal 260 bytes, selector `0x73c457ba` |

## 1. Fidelity to the real formats

`cargo +1.98.1 test -p fidelity -- --nocapture` (output excerpts):

```
empty block 2: 839 bytes (fee tx 447 bytes, clock tx 303 bytes)
dump block 2: 28792 bytes, status Pending
dump block 2: privacy-preserving tx of 27953 bytes
LEZ dump: 7 blocks (ids 1..=7), 19 public and 2 privacy-preserving transactions
mutations: 9000; agree 8972; core lenient on secp256k1 keys 28; core strict on reverted 0
test result: ok. 7 passed  (formats)
test result: ok. 10 passed (rule)
```

- Every block built with LEZ's crates and every block of LEZ's committed sequencer dump is parsed by
  the core with the same header fields, the same transaction count and byte span per transaction,
  the same `reverted` list and status, the same visible calls, and the same hash as LEZ's
  `recompute_hash`. The dump's blocks form a hash-linked chain (ids 1–7), its empty blocks are 839
  bytes like ours, and its two privacy-preserving transactions (27,953 bytes each, with dev-mode
  receipts) parse exactly.
- 9,000 single-byte mutations of real blocks: the core and LEZ's borsh decoder agree on 8,972; the
  28 differences are all the documented one (LEZ validates secp256k1 x-only keys, the core does not).
- Inscriptions: for transactions `[Inscribe, Transfer]`, `[Inscribe, ChannelTransfer, Transfer]`,
  `[Inscribe, ChannelWithdraw, Transfer]` built and signed with logos-blockchain-core d480efc6, the
  core's opcode framing, BLAKE2b transaction hash, `MsgId`, elided-payload split and signature
  check equal upstream's. `ChannelConfigOp`: id, fields, transaction hash and multi-signature equal.
- Rule tests (all on these real-format blocks and inscriptions): burns collected only from
  non-reverted, visible calls for this configuration (a reverted burn, a burn of another
  configuration, a fee-fallback burn, a reply to a private actor and an unknown variant are
  ignored); a block out of order, a fork block with the right id, and a tampered amount under a
  stale hash are refused; support by `T`'s producer is checked but not counted; too few distinct
  keys, a non-accredited key, another channel, a payload that is not the block (another status
  byte; another block), a tampered signature are refused; the SPEC-v3 attack (three of four keys
  name the honest key as `D`; forged signature, the honest key's real inscription of another block)
  is refused, and with the true `D` the attackers have 2 of 3; rotations with too few signatures,
  a repeated index, a wrong index, a stale parent, a set below `nMin`, a threshold below `tMin`,
  a changed posting timeframe and a timeframe of 0 are refused, a valid rotation (keeping the
  timeframe) accepted, and the timeframe changes `S`; `ImportRoot` never lowers the count and stops after
  `Sunset`; an absent echo is refused and support at or before `Y` too; two segments compose to the
  one-piece journal, broken or foreign segment chains are refused; a one-key zone needs `nMin = 1`.

## 2. Cycle tables (r0vm 3.0.5 executor, `target/release/host bench`, `results/bench.log`)

Deployment n = 4 keys (t = 3), `nMin` = 3, `tMin` = 2, unless stated. Each checkpoint starts at LEZ's
genesis block; "blocks" counts all blocks in the input: up to `T`, then the support blocks (3
here) after it. Every guest journal equals the native run's.

| fixture | what | blocks | block bytes | user cycles | segments (2^20) | padded cycles |
|---|---|---|---|---|---|---|
| `cp-empty-1` | 1 empty block + 3 support | 4 | 3,356 | **3,705,001** | 4 | 4,194,304 |
| `cp-empty-60` | 60 empty blocks + 3 | 63 | 52,857 | **4,070,948** | 5 | 4,718,592 |
| `cp-empty-360` | 360 empty blocks (one hour at 10 s) + 3 | 363 | 304,557 | **5,992,503** | 7 | 7,340,032 |
| `cp-echo-360` | same, finalisation: `Y` = `T`+2 carries the echo | 366 | 307,569 | 6,001,721 | 7 | 7,340,032 |
| `cp-pp5-360` | hour with 5 PP txs (LEZ-wallet message part, 223,551-byte proof) | 363 | 1,492,272 | **8,821,992** | 12 | 12,058,624 |
| `cp-pp50-360` | hour with 50 PP txs (237,543 bytes each: 13,988 message + 223,551 proof) | 363 | 12,181,707 | **37,865,667** | 60 | 62,914,560 |
| `cp-burns10-360` | hour with 10 public burns | 363 | 311,027 | **6,074,318** | 7 | 7,340,032 |
| `cp-privburns10-360` | hour with 10 private burns (PP tx, boundary `Burn`, full-size proof) | 363 | 2,662,907 | 11,298,256 | 16 | 16,777,216 |
| `cp-busytip-t3` | `T` and the 3 support blocks each with 4 PP txs | 4 | 3,804,044 | **224,848,717** | 232 | 243,269,632 |
| `seg-empty-180` | segment over 180 empty blocks (no support) | 180 | 151,020 | 1,168,480 | 2 | 1,572,864 |

In the busy hours the transactions sit in blocks other than `T` and the support blocks; `cp-busytip-t3`
measures busy `T`/support blocks separately.

### Support check alone (1 empty block `T` + `t` support blocks; the cost is `t + 1` inscription checks)

| t | n | fixture | user cycles | segments |
|---|---|---|---|---|
| 2 | 3 | `cp-support-t2` | 2,807,019 | 3 |
| 3 | 4 | `cp-support-t3` | 3,705,001 | 4 |
| 5 | 7 | `cp-support-t5` | 5,525,618 | 6 |

That is ~0.9M per checked inscription of an empty block (micro: 891,531, of which Ed25519 0.83M and
BLAKE2b of the ~1 KB transaction ~0.06M). Over a block with 4 private transactions one inscription
check costs **54.1M** (BLAKE2b of 0.95 MB).

### Per component (`target/release/host micro`, `results/micro.log`; marginal costs from two input sizes)

| component | cost | unpatched (A/B, `host micro --plain`) |
|---|---|---|
| reading input into the guest | 0.092 cycles/byte | |
| SHA-256, RustCrypto `sha2` with RISC Zero's patch | **2.59 cycles/byte** (the zkVM's own `risc0_zkvm::sha::Impl`: 2.59 too) | 63.1 cycles/byte |
| BLAKE2b-256 (no RISC Zero accelerator; 64-bit words on RV32IM) | **55.9 cycles/byte** | (no patch exists) |
| Ed25519 `verify_strict`, 32-byte message | **826,530 cycles** | 3,492,647 |
| Ed25519 `verify_strict`, 200-byte message | 854,264 cycles | |
| burn leaf (SHA-256 of 189 bytes) + frontier append | 4,234 cycles | |
| parsing an empty block (839 B) / hashing it | 1,670 / 3,805 cycles | |
| parsing a block with 4 PP txs (951,011 B) | 98,048 cycles (0.103 cycles/byte: the proofs are skipped, not read) | |
| parse + hash of that block | 2,562,746 cycles | |
| inscription check of an empty block (BLAKE2b ~1 KB tx, Ed25519, framing) | 891,531 cycles | 3,523,833 (single) |
| inscription check of the 4-PP-tx block | 54,080,473 cycles | |

Unpatched figures: `(c(6) − c(1)) / 5` for Ed25519 = (20,977,381 − 3,514,146) / 5; SHA-256
(63,225,025 − 639,440) / 990,000 − 0.092.

## 3. Derived

- **Marginal cost per empty block**: (cp-empty-360 − cp-empty-60) / 300 = (5,992,503 − 4,070,948) / 300
  = **6,405 cycles** (from cp-empty-1: 6,372). Of these, hashing 3,805 and parsing 1,670 (micro); the
  rest is reading, linking and the per-block visitor.
- **Marginal cost per KB of block data**: (cp-pp50-360 − cp-empty-360) / 11,877 KB = **2,684 cycles/KB**,
  i.e. SHA-256's 2.59 cycles/byte plus reading and parsing. One private transaction (~237 KB) in a
  block that is neither `T` nor support: 637k cycles.
- **Fixed part** (n = 4, t = 3, small `T` and support blocks): cp-empty-1 − one block = 3,705,001 −
  6,405 = **3.70M**: four
  inscription checks (`T`'s and three supporters'), of which Ed25519 is 4 × 826,530 = 3.31M (89%).
- **Hours per proof** (fixed part paid once):

  | budget | idle zone (360 × 6,405 = 2.31M / h) | busy zone, 50 PP txs / h (37.87M − 3.70M = 34.17M / h) |
  |---|---|---|
  | 50M cycles | (50 − 3.70) / 2.306 = **20.1 h** | (50 − 3.70) / 34.17 = **1.4 h** |
  | 200M cycles | (200 − 3.70) / 2.306 = **85.1 h** | (200 − 3.70) / 34.17 = **5.7 h** |

  Both assume small `T` and support blocks. If those four blocks are as busy as LEZ allows (4 PP
  txs each), the support check alone is **224.8M** (`cp-busytip-t3`) and no checkpoint fits in 200M;
  in a zone where every block is full, each counted block costs ~56M (54.1M BLAKE2b + 2.6M hashing).
- **Backlog**: proving `h` hours of backlog costs the per-hour figure above times `h`, plus one fixed
  part. In proving time on this machine (289 s per segment, §6, EXTRAPOLATION), counted in
  segments rather than user cycles because data-heavy segments hold fewer user cycles (paging: 631k
  per segment in `cp-pp50-360` against 856k in `cp-empty-360`): an idle hour adds 3 segments
  (`cp-empty-360` 7 vs `cp-empty-1` 4) ≈ 15 min; an hour with 50 private transactions adds 56
  (60 vs 4) ≈ 4.5 h. With composition the backlog is split into segments (1.17M per 180 empty blocks, i.e. 2.34M
  per idle hour, ~34M per busy hour) that can be proven independently and in parallel, then one
  checkpoint over the last stretch and the support blocks verifies them all (one `env::verify` each,
  ~a few thousand cycles in the guest; the cost is one recursion `resolve` per segment at proving).
- R2's estimate ("5–20M cycles per hourly checkpoint, t × 0.2–1M for the signatures") holds for idle
  and moderately busy hours with small support blocks (measured 6.0M idle, 8.8M with 5 PP txs,
  37.9M with 50); it misses that each signature check needs BLAKE2b over the whole inscribed block.

## 4. Mode (B) of SPEC-v3 D7, estimate (no program built)

Mode (B) checks `t` Ed25519 signatures over a ~210-byte attestation (`"ferrata/final-tier/v1"`(21) ‖
evmChainId(32) ‖ vault(20) ‖ lezConfigId ‖ channelId ‖ tipId(8) ‖ tipHash ‖ finalMsgId) and walks
the hash links from `T` to the attested tips. Estimate = `t` × 854,264 (measured, 200-byte message)
+ `d` × 6,405 (measured marginal per empty block; 2,684 cycles/KB for busy blocks), where `d` is the
number of blocks from `T` to the furthest attested tip. **ESTIMATE**, not a measured program:

| t | signatures | + walk d = 1 | + walk d = 60 (10 min) | + walk d = 360 (1 h) | mode (A) as built: (t + 1) inscription checks, empty blocks (measured) |
|---|---|---|---|---|---|
| 2 | 1.71M | 1.72M | 2.09M | 4.01M | 2.81M (`cp-support-t2`) |
| 3 | 2.56M | 2.57M | 2.95M | 4.87M | 3.71M (`cp-support-t3`) |
| 5 | 4.27M | 4.28M | 4.66M | 6.58M | 5.53M (`cp-support-t5`) |
| 7 | 5.98M | 5.99M | 6.36M | 8.29M | ~7.35M (t = 5 plus 2 × 0.91M, extrapolated) |

The difference that matters is not in this table: mode (B) never hashes an inscription with BLAKE2b,
so its cost does not grow with how busy the support blocks are (mode (A): +54M per counted block
holding 4 private transactions).

## 5. Composition

`RISC0_DEV_MODE=1 target/release/host compose` (`results/compose-dev.log`):

```
piece 1 (segment, 180 blocks): dev-mode (fake) receipt | user cycles 1168480 | segments 2 | 0.1 s | receipt verifies against the segment image
piece 2 (checkpoint, 180 blocks + 3 support, verifies piece 1): user cycles 4867931 | segments 6 | 0.34 s (executor)
piece 2 proved: fake (dev mode) | 0.3 s | verifies against the checkpoint image | journal == one-piece cp-empty-360
piece 2 without piece 1's receipt: refused (sys_verify_integrity: no receipt found to resolve assumption: claim digest 561c482b…)
piece 2 with a receipt of another segment (blocks 1..=179): refused (sys_verify_integrity: no receipt found to resolve assumption: …)
```

Piece 1 is the segment guest over blocks 1–180 (`seg-empty-180`); piece 2 is the checkpoint guest
over blocks 181–360 plus 3 support blocks, continuing from piece 1's end state and verifying piece
1's journal with `env::verify(SEGMENT_ID, journal)`. The final journal is byte-for-byte the one the
one-piece checkpoint `cp-empty-360` commits (same `parentState` = genesis state, same `T`). Cost:
1,168,480 + 4,867,931 = 6.04M user cycles in two pieces against 5.99M in one: the fixed part
(support) is paid once, in piece 2; the extra 0.04M is the segment's own state hashing and journal.

Exercised: the segment and checkpoint guests in the real executor; the claim the checkpoint guest
asks for (image id compiled in, journal digest) matching exactly the segment receipt's claim; the
guest refusing to run without that receipt or with another segment's; the core's checks that
segments chain and belong to the configuration (fidelity test `segments_compose_to_the_same_journal`).
Not exercised in dev mode: the cryptographic recursion (`lift`/`join`/`resolve` of a real succinct
segment receipt into the checkpoint receipt). `host compose --real` (the same with real succinct
receipts, so that the prover resolves the assumption by recursion) exists but was not run, for time;
the single real proof below covers the proving pipeline without composition.

## 6. A real proof

`scripts/prove-bench.sh prove cp-empty-1`, then `scripts/prove-bench.sh groth16 cp-empty-1`
(`results/logs/*-prove-cp-empty-1.log`, `*-groth16-cp-empty-1.log`), twice. `cp-empty-1` is a
complete checkpoint: genesis state → one empty block `T`, its producer's inscription and three
support inscriptions (n = 4, t = 3), 3,704,942 user cycles with the proven image `0xbbf58fed…34c0`
(3,705,001 with the final one), 4 segments of 2^20. Peak memory is GNU time's
maximum resident set of the host and its r0vm child; for the Groth16 step the Docker prover
container is polled every 0.2 s.

| run | step | wall | CPU (user) | load before → after | peak memory | output |
|---|---|---|---|---|---|---|
| **under load** (other agents) | succinct | **1,758.9 s** (29:19) | 13,989 s (798% CPU) | 13.15 → 36.76 | 9.62 GB (host + r0vm) | receipt 224,314 bytes; total cycles 4,194,304 (paging 276,050, reserved 213,312) |
| **under load** | identity_p254 + Groth16 (Docker) | **215.2 s** | 1,023 s (host + r0vm) | 32.90 → 33.19 | host + r0vm 1.44 GB; container seen 48.2 s, cgroup peak 8.48 GiB (incl. page cache), `prover` VmHWM 7.30 GiB | seal 260 bytes, selector `0x73c457ba` |
| second run, **started quiet** (only this prover; others resumed before the end) | succinct | **1,157.5 s** (19:17) | 11,726 s (1017% CPU) | 2.95 → 36.05 | 9.62 GB (host + r0vm) | receipt 224,314 bytes; same cycle counts |
| second run, **under load** | identity_p254 + Groth16 (Docker) | **195.1 s** | 986 s (host + r0vm) | 36.05 → 55.14 | host + r0vm 1.44 GB; container seen 53.8 s, cgroup peak 7.36 GiB, `prover` VmHWM 7.31 GiB | seal 260 bytes, selector `0x73c457ba` |

Both receipts of each run verify against the proven image id and carry the journal the native
run produced for that program (`host verify cp-empty-1`, `results/verify-under-load.log`,
`results/verify-quiet.log`); the seal's selector `0x73c457ba` is the one risc0-ethereum 3.0.x's
`RiscZeroGroth16Verifier` expects (as `ferrata-bridge`, now `../bridge/`, confirmed on that verifier for 3.0.5
receipts; not re-checked on a contract here). Seals, journals and Groth16 receipts are in
`artifacts/cp-empty-1-under-load/` and `artifacts/cp-empty-1-run2/`. The final image was not proven
again (same input, 59 more user cycles, same 4 segments).

The CPU time is the stabler measure: 11,726–13,989 CPU-seconds for 4 segments, i.e. about
2,900–3,500 CPU-s per 2^20 segment including recursion and compression to one succinct receipt;
the Groth16 step adds ~1,000 CPU-s (identity_p254, run by r0vm on the CPU) plus 49–54 s of the Docker
prover, whatever the checkpoint size.

**Extrapolation (labelled as such, from the second run: 1,157.5 s / 4 segments = 289 s per
segment, wall, on this 8-core laptop, CPU only).** `cp-empty-360` (one idle hour, 7 segments)
≈ 34 min succinct + ~3.5 min Groth16; `cp-pp50-360` (60 segments) ≈ 4.8 h; `cp-busytip-t3`
(232 segments) ≈ 18.6 h. Proving one idle hour per hour is feasible on this machine; an hour with
50 private transactions is not (~4.5 h of proving per such hour, §3), and busy support blocks make
a single checkpoint take most of a day. Segments are independent, so a backlog parallelises across
machines; a GPU prover was not available to measure.

## 7. Findings about the rule (SPEC-v2 §4, with SPEC-v3 D3)

Numbered for reference; "SPEC" is SPEC-v2 §4 as amended by SPEC-v3 D3.

1. **The producer had to be a checked inscription (fixed by SPEC-v3 D3, implemented here before the
   change arrived).** As SPEC-v2 was written, `T`'s producer was a free claim: any key can sign *an*
   inscription of `T`'s bytes, and nothing tied the "excluded key" to one. This implementation
   always checked one inscription of `T` by a key of `K` (the SPEC-v3 rule), so the v3 change cost no
   extra cycles here: `cp-empty-1` took 3,704,465 user cycles with the 13-word journal (an earlier
   run in this session, whose log was replaced), 3,704,942 with the 17-word one
   (`bench-before-timeframe.log`) and 3,705,001 with the timeframe amendment (`bench.log`). Counting: with at most `t − 1`
   bad keys, `t` counted keys other than `D` contain an honest one; if `D` itself is bad (named
   truthfully or not) the bad keys among the counted ones are at most `t − 2`, so two honest keys are
   counted and at most one of them produced `T`. The residual assumption is that an honest key
   never signs an inscription of a block it did not produce (UNVERIFIED for LEZ's sequencer).
2. **The busy-zone cost is in the support check, and it comes from the real inscription format.**
   A counted inscription is checked by hashing the whole Mantle transaction with BLAKE2b, and the
   transaction contains the whole block. BLAKE2b has no RISC Zero accelerator and runs at 55.9
   cycles/byte, 21.6× SHA-256's 2.59. An empty block's inscription costs 0.89M (mostly Ed25519); a
   block with 4 private transactions (0.95 MB, LEZ's per-block maximum) 54.1M. With `t = 3` and
   busy `T` and support blocks the check alone is 224.8M cycles (232 segments), before any history.
   The prover can choose the smallest blocks of each key after `Y`, and the smallest `T`, but in a
   zone that is always busy there are none. SPEC's "only these counted inscriptions are … BLAKE2b-
   hashed" is right about *which*, but not cheap: the cost is per counted block *byte*. Options:
   mode (B) of D7 (no inscription hashing, table in §4); or keep blocks small at the chosen points
   (an empty block costs nothing to LEZ); or a Logos change that inscribes a commitment instead of
   the block (not available).
3. **Retired keys stay usable for support until a rotation is proven** (SPEC-v3 D8 now lists this).
   Rotations are witnesses the prover chooses to supply; the program cannot learn of a rotation it
   is not shown, so an attacker holding `t` keys of an old set simply never supplies the rotation
   that retired them. V1 must hold for every key set the program ever accepted, indefinitely.
4. **Rotation rule ≠ Bedrock's.** SPEC: at least `t(current n)` signatures. Bedrock: *exactly* the
   channel's recorded `configuration_threshold` signatures, and its key list is a vector that may
   repeat a key (one key could then fill two indices). LEZ sets the threshold to `t(n)`, so on LEZ
   channels the two agree; here repeated keys are refused, and `t(current n)` distinct keys are
   required. The program cannot know whether a rotation it accepts was the one Bedrock accepted
   (two ops with the same parent can both carry `t` signatures); it follows a configuration chain
   approved by `t` keys, which is what V1 covers.
5. **`allowSingleKey` must be bound to something the vault pins.** As a separate deployment
   constant outside the journal, a prover could set it in production. Here it is `nMin ≤ 1`.
6. **`importedLeafCount` must never decrease** (now also a vault check in SPEC-v3 D4). "Successful
   `ImportRoot` updates it" would let an import of an older root lower it and make leaves refundable
   that were claimable on LEZ. Here: the maximum, and nothing after `Sunset`.
7. **Invisible burns.** Only root calls and private→public boundary deliveries are in the block. A
   burn reached through a public program's call to the bridge, an async message, or the bridge
   running as a private actor destroys tokens without a burn leaf. The bridge program must refuse
   those paths; whether a v0.4 program can tell a private caller from a public one is UNVERIFIED.
8. **The inscribed bytes are not the served bytes.** The payload is `borsh(Block)` *with*
   `header.hash` and `bedrock_status`; the status byte is not covered by the block hash but is
   covered by the inscription signature. A prover that takes blocks from a LEZ RPC or indexer, which
   may report `Finalized`, gets blocks that re-hash correctly but whose inscriptions do not verify
   (tested: another status byte gives `BadSignature`). Provers must read the channel's inscriptions.
9. **Each checkpoint re-hashes the previous one's `(T, E]`**, since the next starts at `T`; with
   an echo the gap `(T, Y]` is waited for and hashed too. Small for idle zones (t blocks), up to
   `t` × 56M for busy ones (item 2).
10. **Echo in a reverted transaction.** SPEC is silent; it counts here (inclusion dates the block).
11. **What the program does not check** (by design, SPEC-v2 §4.2): transaction signatures, proofs
    of private transactions, the `reverted` list, the fee and clock transactions and every other
    validity rule of LEZ. A block that LEZ would reject is caught only through support (V2). Two
    parser differences are deliberate: secp256k1 keys are not validated (LEZ's decoder does; such a
    block is invalid for LEZ), and a `reverted` list that is not strictly ascending is refused
    (LEZ's decoder accepts it, `apply_block` refuses it).
12. **Pinned turn length (SPEC-v3 D3 amendment): implemented, two edges.** A `ChannelConfigOp` must
    keep the `posting_timeframe` bound in `keySetHash` and must not set 0. (a) As written, the rule
    does not refuse a *parent* state whose timeframe is 0: from such a state no rotation can ever
    apply (every op either differs or is 0), so the configuration is stuck rather than unsafe; the
    deployment check (`turnSeconds` = timeframe × slot duration, D3) must refuse a genesis state with
    0. (b) The program sees the timeframe in slots only; the slot duration is a Bedrock parameter it
    never sees, so `turnSeconds` rests on the deployment check alone. A zone that changes its turn
    length, or Bedrock's slot length, cannot be followed and must be retired, as D3 says.
13. **Finality (SPEC-v3 D1) is outside what a cycle table shows**: the program proves that `t` keys
    other than `D` inscribed blocks after `Y`; whether that implies finality is the question D1/D6
    answer, not this prototype.

## The end-to-end fixture: two checkpoint proofs a contract verified

The return leg of [`../e2e/`](../e2e/README.md) has a stand-in zone of 16 blocks built by
`fidelity/src/bin/e2e.rs` (n = 4, t = 3, posting timeframe 900): an `ImportRoot`, five burns of the
configuration beside a burn of another configuration and a burn in a reverted transaction, `T` =
block 6, and an `Echo` in block 12. Its two checkpoints were proven with the final image
(`0x95b816ed…3caf`, the recorded program binary, not rebuilt):

| checkpoint | blocks | input | user cycles | segments | succinct proof | Groth16 wrap |
|---|---|---|---|---|---|---|
| candidate (`cp-e2e-candidate`, `echoNonce` = 0) | 2–9, support 7–9 | 14,385 B | 3,786,158 | 5 (4 × 2^20 + 2^15) | 15.0 s on one NVIDIA L4 (segment proofs 10.7 s, lift and join 4.1 s); receipt 224,314 B | identity_p254 18.7 s in r0vm on 16 cores, Groth16 prover image 14 s; seal 260 B |
| finalisation (`cp-e2e-finalize`, echoes the beacon) | 2–16, `Y` = 12, support 13, 15, 16 | 20,781 B | 3,831,033 | 5 (4 × 2^20 + 2^17) | 15.4 s on the same L4 (11.0 s, 4.1 s); receipt 224,314 B | identity_p254 18.6 s, prover image 14 s; seal 260 B |

Cycle counts are from the r0vm 3.0.5 executor (on a 16-core cloud machine); both guest journals equal the
native rule's. The journals are 544 bytes and are, byte for byte, the v3 vault's own encoding of the
`submitCandidate` and `finalize` arguments; the bridge side recomputes both independently
(`bridge/e2e/src/checkpoint_v3.rs`) and gets the same bytes. In `vault/test/real/` the v3 vault
accepts the candidate seal, then, after the wait and the beacon, the finalisation seal, through
RISC Zero's unmodified Groth16 verifier behind its emergency stop; a flipped byte in either seal,
either seal with a changed journal field, and the candidate's seal offered as the finalisation are
refused. Receipts, seals and logs: `e2e/runs/sepolia/receipts/`.

Two things the fixture showed about this program's inputs. A finalisation must prove the
candidate's very blocks (the vault looks the candidate up by `newState`), and LEZ signs
transactions with fresh randomness, so a zone cannot be rebuilt bit for bit: the prover has to keep
the blocks it proved (the builder now takes them from the candidate input). And only the echo and
what follows it is new work for the second proof: blocks 2–9 are hashed again (finding 9).

## GPU proofs of the final image

The final image (`0x95b816ed…3caf`) was later proven on one NVIDIA L4 GPU for four fixtures, with every succinct receipt verified locally and every journal equal to the native run's: `cp-empty-1` in 14.0 s, the idle hour in 24.1 s, the hour with 5 private transactions in 41.4 s and the hour with 50 in 219.3 s. See `gpu-prover/RESULTS.md`. No Groth16 wrap was made for these.
