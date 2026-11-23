# Results

Every number below comes from a command run in this directory on 10 Nov 2026; logs and JSON lines
are in `results/` (`bench.log`, `micro.log`, `compose-*.log`, `logs/*prove*.log`, `*.jsonl`).
Derived figures show their inputs (`python3 -I scripts/derive.py` recomputes them from the JSON
lines). §4 (rule (B), and rule (A) again on the same blocks) was measured later the same day on the
Modal build box, with that build's own image ids (§4). The program measured elsewhere implements
SPEC-v2 §4.1–§4.2 with SPEC-v3 D3 (checked producer
inscription, 17-word journal, posting timeframe pinned in the key-set hash); image ids: checkpoint
`0x95b816ed2614e340c85fed3ba3dad1f70e7dff2da1363406ade87e1333583caf`, segment
`0xbdfb94a6b6ea12e996f332b28ce4791dc7362f28da602769192f589a61fee043` (local toolchain build, not
reproducible elsewhere as such). The two real proofs (§6) were made just before the
posting-timeframe amendment, with checkpoint image `0xbbf58fed…34c0`; that program differs only in
the key-set hash encoding and the timeframe check, and runs the same input in 3,704,942 user cycles
against 3,705,001 now (same 4 segments).

**The images after the `ImportRoot` fix.** On the integration branch (commit 12d17e3) variant 1 of
the bridge messages carries the bridge program's `ImportProof`, which `wire.rs` walks strictly and
skips (README, "Real format vs stand-in"). That changes `checkpoint-core`, so both guests change.
Built under the integration checkout's path on the build box (`host ids`): before the fix checkpoint
`0x2b25a9f248b14a7db950e1e35ddc334687cf6e8b6a6ed815081efe8669f81093`, segment
`0xe384a99fbea39d289a22d8f8b80eba31fcb3c10eabae479bc26491e94e6a689c`; with the fix checkpoint
`0xd3c22b181ca5201cfc4501bc09be6013793238e14e1df5bd03c278213d7319c0`, segment
`0x7d0c6d4d2021010398023970840e3f4cba8dfeef2b66a6814b368405dc7082e2` (`micro`, which decodes no
message, is `0x57a0aa8d…18e3` in both). Every figure and proof in this file is of images from
before the fix (the ids above, or earlier ones where a section says so); the benches were not
re-run on the fixed images (their fixtures are rebuilt by `gen all`, about 25 min).

**The images after the merge of rule (B)** (branch `vault-fees`, which carries `rule-b`, merged into
`integration`): `checkpoint-core` now also holds rule (B) and depends on `crates/attestor-core`, so
every guest changes again. Built under the integration checkout's path on the build box (`host ids`,
second integration pass): checkpoint
`0xcfcbe41edeaf2c216e96de230f0b4fdff146fa936b1454bd0f097732a4972a20`, checkpoint_b
`0xe18205281d624e54bcc1c1cd920fbd6147eaf4a62d1b711285a953a81c4fc19c`, segment
`0x073db46707af69d8c1f0333df87cb9b18cf4e138e57ff8680d0a530fb441004e`, micro
`0xe42a76fa920e0f2a9a115ad8530c2f5fb139b9a1e40741a18045a36d7d31c79a`. No proof or bench here is of
these images: §4's are of the `rule-b` build below (before the `ImportRoot` fix), the rest as above.

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
| fidelity tests | 17 pass (`cd fidelity && cargo +1.98.1 test`; a workspace of its own since RISC Zero 3.1): 7 format tests against LEZ and Logos Blockchain code, 10 rule tests |
| empty LEZ block | 839 bytes (fee tx 447, clock tx 303); identical size to the empty blocks of LEZ's own sequencer dump |
| marginal cost per empty block | **6,405 cycles** |
| marginal cost per KB of block data | **2,684 cycles/KB** |
| fixed cost per checkpoint (n = 4, t = 3, small `T` and support blocks) | **3.70M cycles** (four inscription checks of 0.89M each) |
| one idle hour (360 blocks) | **5.99M cycles** in one checkpoint, 7 segments |
| one hour with 50 private transactions | **37.9M cycles**, 60 segments |
| `T` and 3 support blocks each holding 4 private transactions | **224.8M cycles**, 232 segments (BLAKE2b over the inscriptions) |
| components | SHA-256 2.59 cycles/B (accelerated; 63.1 without the patch), BLAKE2b-256 **55.9 cycles/B** (no accelerator), Ed25519 `verify_strict` **0.83M** (3.49M without the patch), burn leaf + append 4,234 |
| rule (B) (§4) | **1.69M cycles per attestor** (certificate + attestation, two Ed25519 `verify_strict`); the check 5.1M at t = 3, 11.8M at t = 7, 16.9M at t = 10; one proof per checkpoint; rule (A) 0.89M per supporter, two proofs |
| composition | two pieces (segment of 180 blocks, then a checkpoint verifying it) prove the same journal as one piece (dev mode) |
| real proof (4 segments) | succinct 1,758.9 s under load, 1,157.5 s starting quiet; Groth16 wrap 215.2 s / 195.1 s under load; peak 9.85 GB (host + r0vm), Groth16 prover 7.3 GiB; seal 260 bytes, selector `0x73c457ba` |
| real LEZ blocks (§ "Real LEZ blocks") | rule (B) over **195 real blocks** of LEZ v0.4.0-rc1's sequencer (the lab's devnet: one validator, dev-mode leader proofs, n = 1): parser and re-hash accept all 202 blocks and 202 inscriptions as made; 7.66M cycles, 11 segments, **39.5 s on one NVIDIA L4**; accepted by the v5 vault in mode (B) with RISC Zero's verifier, 347,832 + 195,149 gas |

## 1. Fidelity to the real formats

`cd fidelity && cargo +1.98.1 test -- --nocapture` (output excerpts; before RISC Zero 3.1: `cargo +1.98.1 test -p fidelity` from `checkpoint/`):

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

## 4. Rule (B) of SPEC-v3 D7 and D13: built and measured

`crates/attestor-core` (the attestor's certificate and final-tier message v2), `rule_b.rs`
(`run_checkpoint_b`) and the guest `guests/checkpoint-b` (bin `checkpoint_b`). Rule (B) shares steps
1–3 with rule (A) (`rule::up_to_tip`: configuration, segments, rotations, blocks up to `T`), links
the blocks after `T`, and accepts `T` on `t(n)` attestations, each by a channel key of the live set
through an attestation key that channel key certified for this deployment, over `T` or a block after
it on the hash-linked path. Its journal is rule (A)'s 17 words with `echoNonce` = 0, byte for byte
(test `journal_is_rule_a_candidate_journal_over_the_same_blocks`; every generated fixture pair is
checked the same way); a vault tells the rules apart by the image id.

**Where and what.** Executor (r0vm 3.0.5) on the Modal build box through `mrun`, 10 Nov 2026,
`target/release/host bench` (`results/ruleb-bench.log`, `results/ruleb-bench.jsonl`); user cycles are
deterministic. Image ids of that build (`results/ruleb-ids.log`): `checkpoint_b`
`0xd359754364c10357103359276867f4fe58842c42cbb30a3ada5b241de6aa0d64`, `checkpoint` (rule (A))
`0xe0b86641a069bf1ed2def38c8c14b846e7fe55626e518defc3cc7f71146d61b9`, `segment`
`0xd42edfd361b12f83b20996241e5622c62d6e362b349b361117cfcd8dee496594` (box build; ids embed paths).
Fixtures: `host gen-b` takes the blocks of `cp-empty-1`, `cp-empty-360`, `cp-pp5-360` and `cp-pp50-360`
and, for each committee, replaces the parent's keys with `n` seeded test keys and writes a rule (A)
input (`ra-*`: `T`'s inscription by key 0 and `t` support inscriptions by keys 1..t over the three
blocks after `T`, in turn; each is the source's own inscription transaction with only the signer and
signature changed) and a rule (B) input (`rb-*`: `t` attestations on `T`, `T + 1`, `T + 2`, `T + 3`, …
in turn, `finalMsgId` the message id of the source's inscription of that block), plus `rb-tonly-n*`
(`T` alone, every attestation on `T`). Committees: the specification's `n` = 3, 4, 7, 10 (`t` = 2, 3, 5,
7) and `n` = 15 for `t` = 10. The original rule (A) fixtures re-run on this build: `cp-empty-1`
3,706,275, `cp-empty-360` 5,993,418, `cp-pp5-360` 8,822,907, `cp-pp50-360` 37,866,582 user cycles, each
journal equal to the one recorded when the fixture was made (+915 to +1,274 cycles against §2: the
shared-prefix refactor and another build).

**The check alone** (user cycles, segments in brackets; `T` + 3 is one empty `T` and the three empty
blocks after it, the input of rule (A)'s support check in §2):

| n | t | rule (B), `T` alone | rule (B), `T` + 3 | rule (A), `T` + 3 | (B) − (A) |
|---|---|---:|---:|---:|---:|
| 3 | 2 | 3,512,323 (4) | 3,531,918 (4) | 2,839,418 (4) | +692,500 |
| 4 | 3 | 5,204,341 (6) | 5,217,212 (6) | 3,740,489 (4) | +1,476,723 |
| 7 | 5 | 8,567,359 (9) | 8,578,098 (9) | 5,528,554 (6) | +3,049,544 |
| 10 | 7 | 11,926,488 (13) | 11,946,328 (13) | 7,331,406 (8) | +4,614,922 |
| 15 | 10 | 17,047,608 (18) | 17,080,999 (18) | 10,017,922 (11) | +7,063,077 |

**The same hours as §2** (whole checkpoint proof, user cycles and segments):

| n | t | idle hour (A) | idle hour (B) | 5 PP txs (A) | 5 PP txs (B) | 50 PP txs (A) | 50 PP txs (B) |
|---|---|---:|---:|---:|---:|---:|---:|
| 3 | 2 | 5,093,496 (6) | 5,779,891 (7) | 7,940,328 (11) | 8,645,883 (12) | 36,995,607 (59) | 37,698,589 (60) |
| 4 | 3 | 5,984,291 (7) | 7,505,344 (9) | 8,833,632 (12) | 10,325,095 (13) | 37,883,560 (60) | 39,389,729 (62) |
| 7 | 5 | 7,771,007 (9) | 10,863,894 (12) | 10,618,888 (14) | 13,672,957 (17) | 39,658,893 (62) | 42,755,918 (65) |
| 10 | 7 | 9,543,975 (11) | 14,245,801 (16) | 12,399,755 (16) | 17,083,183 (20) | 41,451,827 (64) | 46,143,502 (69) |
| 15 | 10 | 12,202,341 (14) | 19,307,122 (21) | 15,062,287 (19) | 22,180,769 (26) | 44,132,718 (67) | 51,146,220 (74) |

**Derived** (`python3 -I scripts/ruleb-table.py`, least squares over the five committees on one
source's blocks): rule (B) costs **1.69M cycles per attestor** (slopes 1,681,779 to 1,692,370 on the
five sources; residuals at most 32,375), rule (A) **0.89M per supporter** (888,654 to 897,332),
whatever the hour holds. Each attestor is two Ed25519 `verify_strict`: the certificate (169-byte
message) and the attestation (241 bytes); from §2's micro figures (826,530 cycles at 32 bytes,
854,264 at 200, so about 165 cycles per message byte of unaccelerated SHA-512) that is about 849k +
861k = 1.71M. The other costs of the check (membership, repetition, the path) are below the
residuals. So per proof, on the same blocks with small support blocks, rule (B) costs more than rule
(A): +0.69M at `t` = 2 to +7.1M at `t` = 10.

Per checkpoint the comparison turns. Rule (A) proves twice (candidate and finalisation), and the
finalisation proof also links every block from `T` to the block carrying the beacon's echo, at
least `W = 3 h 01 min + (t − 1) × 15 min` of blocks on the default floor (§3: 6,405 cycles per idle
block, 34.17M per hour with 50 private transactions). DERIVED examples at `n` = 4: idle hour, rule (A)
2 × 5.98M + 1,266 blocks × 6,405 = 20.1M against rule (B) 7.5M; an hour with 50 private transactions
throughout, rule (A) 2 × 37.88M + 3.52 h × 34.17M = 196.0M against rule (B) 39.4M. Rule (B) also does
not grow with the support blocks: rule (A)'s inscription check is BLAKE2b over the whole block (54.1M
for a block with 4 private transactions, §2), rule (B) hashed no inscription (since §4a it hashes one,
`T`'s, 0.88–0.90M cycles for an empty `T`).

**The estimate this replaces** (`t` × 854,264 + 6,405 per block walked: 2.56M at `t` = 3 to 5.98M at
`t` = 7) counted the attestations only. The certificates, which D13 added after the estimate, double
the signature count: measured 5.07M at `t` = 3 and 11.8M at `t` = 7 for the check (slope × `t`), 3.4M
at `t` = 2, 8.5M at `t` = 5, 16.9M at `t` = 10. Not built, and what it would save: carrying each
certified attestation key in the checkpoint state, so a certificate is checked once rather than in
every proof, would remove about half the check (≈ 0.85M per attestor) at the price of a change to
`S`; signing attestations with the channel key itself, which would do the same, is what D13 forbids.

**A real proof** (10 Nov, one NVIDIA L4 on Modal, `gpu-prover/modal_app.py` with the build box's
program binaries; `gpu-prover/results/ruleb-l4b/`): `rb-empty-360-n4` (idle hour, `n` = 4, `t` = 3),
9 segments, 7,505,344 user cycles (the executor's count) and 8,650,752 in total, proven in 31.9 s
(33.7 s of container). The succinct receipt (224,314 bytes) verifies on the laptop against
`checkpoint_b`'s image id and carries the native journal (`gpu-prover/results/verify-local.jsonl`).
Rule (A)'s idle hour took 24.1 s on the same GPU type (7 segments, "GPU proofs of the final image").
No Groth16 wrap was made. Two earlier attempts got no L4 within 30 minutes; their waiting clients gave
up ("Deadline exceeded") and nothing ran; the proof was then spawned without a waiting client
(`modal_app.py::spawn`) and ran within two minutes.

**Refusals** (`crates/checkpoint-core/tests/rule_b.rs`, 13 tests at this build, 14 since §4a; every input goes through its byte
encoding first): too few (`refuses_too_few`); a duplicate signer, as the same attestation twice, one
channel key with two attestation keys, or one attestation key under two channel keys
(`refuses_a_duplicate_signer`); an uncertified key (`refuses_an_uncertified_key`); a certificate for
another channel, configuration, chain or vault (`refuses_a_certificate_from_another_channel_or_configuration`);
a key outside the live set (`refuses_a_key_not_in_the_live_set`); a key removed by a rotation, with
the key the rotation added accepted (`a_key_removed_by_a_rotation_is_refused_and_a_key_added_counts`);
a stale or wrong key-set hash (`refuses_a_wrong_key_set_hash`); a wrong tip, final message id, chain,
vault, configuration, channel or domain (`refuses_a_wrong_tip_chain_vault_configuration_or_domain`);
an ancestor of `T`, a fork block, a block beyond the input, an id that does not match its hash
(`refuses_an_attestation_over_a_block_that_is_not_a_descendant`); a tampered block after `T`
(`blocks_after_the_tip_are_linked`); the single-key case only under `nMin <= 1`. The attestor's own
tests (`crates/attestor-core/tests/attestor.rs`, 6) bind every field of both messages, and its vectors
(`crates/attestor-core/vectors/attestor-v1.json`) are reproduced by `scripts/check-attestor-vectors.py`,
an RFC 8032 reference implementation in Python that shares no code with the crate.

### 4a. Rule (B) checks `T`'s producer inscription (branch `lez-time-fix`)

**Why.** Review r3 (§1.2, §2 C): rule (B) checked no inscription, so `t` attestation keys, a second
key class no LEZ node trusts, could certify a fabricated history alone. **What changed.**
`CheckpointInputB` carries `T`'s inscription after `t_index` (`Inscription (of T)`, the encoding of
rule (A)'s producer field), and `run_checkpoint_b` checks it exactly as rule (A) does
(`ElidedInscription::check`: a well-framed Mantle transaction inscribing `T`'s bytes on this channel,
the signer's `verify_strict` over the transaction hash) and refuses unless its signer is in `K`, after
the attestations and their count. A forged history now also needs one channel key of `K`. The journal
is unchanged (still rule (A)'s candidate journal word for word, checked by every generated fixture
pair); the honest path is unchanged except that `T`'s producer must still be in `K` after the proof's
rotations, as under rule (A). `OutcomeB` names the producer. **Sound and small**, so built: one field,
five lines in the rule, the generator and `realblocks` supply the inscription they already had.

**Tests** (`crates/checkpoint-core/tests/rule_b.rs`, now 14, all pass on the build box with
`cargo +1.88 test -p checkpoint-core -p attestor-core`; clippy and rustfmt clean):
`checks_the_producer_inscription_of_t` refuses `T` inscribed by a key outside the live set
(`NotAccredited`), a signature that does not verify (`BadSignature`), an inscription on another channel
(`WrongChannel`), the inscription of another block in `T`'s place (`BadSignature` or
`PayloadMismatch`), and a `T` whose producer a rotation in the same proof removed (`NotAccredited`);
the other 13 tests run with `T`'s inscription by channel key 0.

**Cycles.** Executor (r0vm 3.0.5) on the Modal build box through `mrun`, 11 Nov 2026: `fidelity`'s
`gen empty` and `gen busy` (the four sources), `host gen-b`, then `host bench` over the four sources
and every `ra-*`/`rb-*` fixture (`results/ruleb-producer-bench.log`; ids of that build,
`results/ruleb-producer-ids.log`: checkpoint `0x0f2e2e695d61a4330a9a1cb4df127fe163ba4c11b8d3966c4afe3641a82a8158`,
checkpoint_b `0xbda325a882db9e73fa6b9eeedc522b7e67360d0c2216d58989945a906e1d7dc2`, segment
`0x9f6b9a6efc1b1e1ad9aa37ecc61e55f94173e4e5c88976770b7b46ef28d659ea`, micro
`0xd2b2eeba0a6e1b1d2661109dbd3992a215bc20a9e2311ca01685cbed9d47a8dd`; ids embed the build path). Every
journal equals the native run's. Rule (A) runs no changed code, so its move against §4 (+5 cycles on
`T` + 3, +364 on the hours, from the new build's layout) is the drift subtracted below:

| fixture | n | t | rule (B) before (§4) | rule (B) now | difference | rule (A) before → now (drift) | check alone (difference − drift) |
|---|---|---|---:|---:|---:|---:|---:|
| rb-tonly-n3 | 3 | 2 | 3,512,323 | 4,404,041 (5) | +891,718 | 2,839,418 → 2,839,423 (+5) | 891,713 |
| rb-tonly-n4 | 4 | 3 | 5,204,341 | 6,096,510 (7) | +892,169 | 3,740,489 → 3,740,494 (+5) | 892,164 |
| rb-tonly-n7 | 7 | 5 | 8,567,359 | 9,460,493 (10) | +893,134 | 5,528,554 → 5,528,559 (+5) | 893,129 |
| rb-tonly-n10 | 10 | 7 | 11,926,488 | 12,820,588 (14) | +894,100 | 7,331,406 → 7,331,411 (+5) | 894,095 |
| rb-tonly-n15 | 15 | 10 | 17,047,608 | 17,943,159 (19) | +895,551 | 10,017,922 → 10,017,927 (+5) | 895,546 |
| rb-empty-1-n3 | 3 | 2 | 3,531,918 | 4,422,627 (5) | +890,709 | 2,839,418 → 2,839,423 (+5) | 890,704 |
| rb-empty-1-n4 | 4 | 3 | 5,217,212 | 6,107,899 (7) | +890,687 | 3,740,489 → 3,740,494 (+5) | 890,682 |
| rb-empty-1-n7 | 7 | 5 | 8,578,098 | 9,468,744 (10) | +890,646 | 5,528,554 → 5,528,559 (+5) | 890,641 |
| rb-empty-1-n10 | 10 | 7 | 11,946,328 | 12,836,964 (14) | +890,636 | 7,331,406 → 7,331,411 (+5) | 890,631 |
| rb-empty-1-n15 | 15 | 10 | 17,080,999 | 17,971,547 (19) | +890,548 | 10,017,922 → 10,017,927 (+5) | 890,543 |
| rb-empty-360-n3 | 3 | 2 | 5,779,891 | 6,676,199 (8) | +896,308 | 5,093,496 → 5,093,860 (+364) | 895,944 |
| rb-empty-360-n4 | 4 | 3 | 7,505,344 | 8,401,597 (10) | +896,253 | 5,984,291 → 5,984,655 (+364) | 895,889 |
| rb-empty-360-n7 | 7 | 5 | 10,863,894 | 11,760,096 (13) | +896,202 | 7,771,007 → 7,771,371 (+364) | 895,838 |
| rb-empty-360-n10 | 10 | 7 | 14,245,801 | 15,141,953 (17) | +896,152 | 9,543,975 → 9,544,339 (+364) | 895,788 |
| rb-empty-360-n15 | 15 | 10 | 19,307,122 | 20,203,199 (22) | +896,077 | 12,202,341 → 12,202,705 (+364) | 895,713 |
| rb-pp5-360-n3 | 3 | 2 | 8,645,883 | 9,527,431 (13) | +881,548 | 7,940,328 → 7,940,692 (+364) | 881,184 |
| rb-pp5-360-n4 | 4 | 3 | 10,325,095 | 11,206,651 (14) | +881,556 | 8,833,632 → 8,833,996 (+364) | 881,192 |
| rb-pp5-360-n7 | 7 | 5 | 13,672,957 | 14,554,442 (18) | +881,485 | 10,618,888 → 10,619,252 (+364) | 881,121 |
| rb-pp5-360-n10 | 10 | 7 | 17,083,183 | 17,964,628 (21) | +881,445 | 12,399,755 → 12,400,119 (+364) | 881,081 |
| rb-pp5-360-n15 | 15 | 10 | 22,180,769 | 23,062,156 (27) | +881,387 | 15,062,287 → 15,062,651 (+364) | 881,023 |
| rb-pp50-360-n3 | 3 | 2 | 37,698,589 | 38,594,066 (61) | +895,477 | 36,995,607 → 36,995,971 (+364) | 895,113 |
| rb-pp50-360-n4 | 4 | 3 | 39,389,729 | 40,285,719 (63) | +895,990 | 37,883,560 → 37,883,924 (+364) | 895,626 |
| rb-pp50-360-n7 | 7 | 5 | 42,755,918 | 43,652,843 (66) | +896,925 | 39,658,893 → 39,659,257 (+364) | 896,561 |
| rb-pp50-360-n10 | 10 | 7 | 46,143,502 | 47,041,393 (70) | +897,891 | 41,451,827 → 41,452,191 (+364) | 897,527 |
| rb-pp50-360-n15 | 15 | 10 | 51,146,220 | 52,045,560 (75) | +899,340 | 44,132,718 → 44,133,082 (+364) | 898,976 |

So the check costs **0.88–0.90M cycles** on these fixtures, whose `T` is an empty block (839 bytes),
whatever `n`, and adds one segment in most cases: as rule (A)'s check of one inscription of an empty
block (0.89M, §2). It is BLAKE2b over the whole of `T`'s inscription (55.9 cycles per byte, §2) plus
one Ed25519 `verify_strict`, so for a busy `T` it grows as rule (A)'s does: about 54M cycles for a `T`
with four private transactions [derived from §2's `cp-busytip-t3`, not measured for rule (B)]. A
prover picks a small `T`. Rule (B) stays the cheaper per checkpoint (one proof, no wait to link): at
`n` = 4, the idle hour is 8.40M against rule (A)'s 2 × 5.98M plus the wait's blocks.

**What this leaves.** The real proof of §4 (`rb-empty-360-n4`), the v5 vault's real rule (B) proofs
(`vault/test/real/fixtures/rule-b.json`, `real-blocks.json`, "Real LEZ blocks" below) and §18.8's
lab chain are of the guest before this check (`0xd3597543…0d64` and its successors); they stay valid
tests of the vault and of that guest, not of this one. No real proof of this guest was made.

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
| **under load** (other agents) | succinct | **1,758.9 s** (29:19) | 13,989 s (798% CPU) | 13.15 → 36.76 | 9.85 GB (host + r0vm) | receipt 224,314 bytes; total cycles 4,194,304 (paging 276,050, reserved 213,312) |
| **under load** | identity_p254 + Groth16 (Docker) | **215.2 s** | 1,023 s (host + r0vm) | 32.90 → 33.19 | host + r0vm 1.48 GB; container seen 48.2 s, cgroup peak 8.48 GiB (incl. page cache), `prover` VmHWM 7.30 GiB | seal 260 bytes, selector `0x73c457ba` |
| second run, **started quiet** (only this prover; others resumed before the end) | succinct | **1,157.5 s** (19:17) | 11,726 s (1017% CPU) | 2.95 → 36.05 | 9.85 GB (host + r0vm) | receipt 224,314 bytes; same cycle counts |
| second run, **under load** | identity_p254 + Groth16 (Docker) | **195.1 s** | 986 s (host + r0vm) | 36.05 → 55.14 | host + r0vm 1.48 GB; container seen 53.8 s, cgroup peak 7.36 GiB, `prover` VmHWM 7.31 GiB | seal 260 bytes, selector `0x73c457ba` |

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
refused. Both seals also verify, by read-only calls, on RISC Zero's own Groth16 verifier deployed on
Sepolia. Receipts, seals and logs: `e2e/runs/sepolia/receipts/`.

These proofs are of the image before the `ImportRoot` fix (header): the fixture's import is the
bare 81-byte form, which the fixed program refuses, and over the same two inputs the fixed image
commits `importedLeafCount` 0 instead of 2 and another `newState`, the other words unchanged
(`../e2e/README.md`, "After the checkpoint's `ImportRoot` fix").

Two things the fixture showed about this program's inputs. A finalisation must prove the
candidate's very blocks (the vault looks the candidate up by `newState`), and LEZ signs
transactions with fresh randomness, so a zone cannot be rebuilt bit for bit: the prover has to keep
the blocks it proved (the builder now takes them from the candidate input). And only the echo and
what follows it is new work for the second proof: blocks 2–9 are hashed again (finding 9).

## GPU proofs of the final image

The final image (`0x95b816ed…3caf`) was later proven on one NVIDIA L4 GPU for four fixtures, with every succinct receipt verified locally and every journal equal to the native run's: `cp-empty-1` in 14.0 s, the idle hour in 24.1 s, the hour with 5 private transactions in 41.4 s and the hour with 50 in 219.3 s. See `gpu-prover/RESULTS.md`. No Groth16 wrap was made for these.

## Real LEZ blocks: a rule (B) checkpoint over the lab's sequencer chain, accepted by the v5 vault

Every earlier checkpoint proof here was made over blocks built by a fixture builder with test keys.
This one is over **real sequencer blocks**: the chain of the lab ([`../lez/lab/`](../lez/lab/README.md)),
LEZ v0.4.0-rc1's own `sequencer_service` with its Logos Blockchain node (`d480efc6`) run unmodified on
Modal, a **devnet with one validator and leader proofs in dev mode** (`POL_PROOF_DEV_MODE=true`, as LEZ's
container sets it), whose channel holds **one key (n = 1, t = 1)**. The lab was only read: its committed
record (`lez/lab/results/record`: the blocks as the sequencer's RPC served them, `chain.tsv`, `segments.tsv`)
and a copy of its saved generation 3 (the state after its 22:17 UTC session on 10 Nov: 30,970,636 bytes,
sha256 `e0ef796b…5716`, fetched with `modal volume get`), whose node was started alone on the build box to
answer `/channel/<id>` and `/cryptarchia/blocks` (immutable blocks only; [`realblocks/extract.sh`](realblocks/extract.sh)).
The running lab was neither stopped, changed nor redeployed. Code and outputs: [`realblocks/`](realblocks/).
Where each step ran: the build box is the Modal build container of `spike/modal-build/mrun`; the GPU is
one NVIDIA L4 on Modal.

### The parser and re-hashing on the real blocks and inscriptions

`realblocks` (build box, `out/blocks.tsv`, `out/inscriptions.jsonl`) runs checkpoint-core's own code natively
over every block the node holds an inscription of:

| check | result |
|---|---|
| node | 2,103 immutable blocks; 202 transactions on channel `0x01…01`, each exactly `[ChannelInscribe (0x11), Transfer (0x00)]` with an Ed25519 and a ZK proof; one inscription of each LEZ block 1..=202 |
| blocks as inscribed (`borsh(Block)`, the inscription payloads): `parse_block` (strict), `hash_parsed_block`, ids and links | **202 of 202 accepted**; every re-hash equals the hash the block carries and `chain.tsv`'s; ids consecutive, each linked to the previous hash |
| inscriptions: framing (`ElidedInscription::split`, `check`), BLAKE2b-256 transaction hash, `verify_strict` by the signer the op names | **202 of 202**; every hash equals the node's, 201 equal the hashes the sequencer logged (block 1 was inscribed at setup, before any session log); the signer is the channel's one accredited key `3b14e398…b0a2` |
| message ids | each inscription's parent is the previous block's `MsgId` (block 1's is zero); block 202's `MsgId` is the channel's `tip_message` `cc5ae8ce…a79e` |
| the record against the inscribed bytes | 196 blocks byte-equal except the status byte; 6 (blocks 2–7, the program's deployment) with the bytecode the record elides (header bytes equal; the inscribed blocks re-hash to the recorded hashes) |

Findings:
1. **No mismatch.** The parser and the block hash accept LEZ's real blocks and inscriptions exactly as the
   sequencer made them; nothing on the checkpoint side changed, and the image ids are unchanged.
2. **The status byte.** All 202 blocks are inscribed with `bedrock_status = Pending` (0) and served by the
   sequencer's RPC with `Finalized` (2). The block hash leaves the byte out (as `lez.rs` assumed), but an
   inscription signs the inscribed bytes: a rule (A) prover must take `T`'s bytes from the inscription, not from
   the RPC, or the producer check fails. Rule (B) never re-checked an inscription at this build; since
   §4a it checks `T`'s, so a rule (B) prover must also read `T` from its inscription.
3. **The channel's turn length is 0 on Bedrock.** The node reports the channel with `posting_timeframe` 0,
   `posting_timeout` 0 and `config_tip_hash` 0 (made by `sequencer_service setup --create-channel`, no
   configuration op), although LEZ's sequencer configuration says 300. The proven key-set hash binds 0, as the
   chain holds it. With 0 no key rotation could ever be accepted by the program (SPEC-v3 D3: an op's timeframe
   must equal the bound one and must not be 0), and a rule (A) vault needs a non-zero turn: a bridge needs its
   channel created with a configuration op. Neither matters here: one key, no rotation, rule (B).
4. **UNVERIFIED item 1 observed, on a devnet.** Every inscription the sequencer made is
   `[ChannelInscribe, Transfer]` of its own block, as read from LEZ's code; one sequencer, not a network.

### The attestor on the lab's data

`out/attestor.json`. The lab sequencer's channel key (generated by LEZ in the sequencer's home at setup, a
throwaway devnet key, read from the copied generation; its public key `3b14e398…b0a2` is the one the sequencer
logs and the channel's only key, configuration threshold 1) certifies, with attestor-core's certificate
(`"ferrata/attest-key/v1" ‖ evmChainId ‖ vault ‖ lezConfigId ‖ channelId ‖ attKey`), an attestation key derived
from the public label `ferrata/realblocks/attestation-key/v1` (`b707f1ee…bc6a`, throwaway). That key signs the
final-tier message v2 for `T` = block 202 (`774780c6…2bbb`) with `finalMsgId` = its inscription's `MsgId`
`cc5ae8ce…a79e`. `T` is final by the node's own rule: its inscription sits in node block `cfd9acd4…a1ee` at slot
8,856,117, which the node serves among its immutable blocks (its LIB was at slot 8,856,126 when the copy
started); the lab recorded block 202 `Finalized` and the sequencer logged `finalized 202..=202`. The committee is
one key (n = 1, t = 1), which is what this devnet has; the program allows a one-key set only because the
configuration's `nMin` is 1.

The configuration's other values have no lab counterpart, since no bridge program runs in the lab:
`bridgeHeader`, `bridgeImageId`, `attestationRootId`, `salt` and the vault address are SHA-256 of labels
(`ferrata/realblocks/stand-in/…`), `evmChainId` is 31337 (a local EVM), `bedrockGenesisId` is the SHA-256 of the
node's deployment settings; `lezConfigId` = `da0edd60…9d40`. There are no burns: the burn root is the empty tree's.

### The proof

| span | parent tip | blocks | transactions | block bytes | input | user cycles | segments |
|---|---|---|---|---|---|---|---|
| **8..=202** (every block since the program's deployment; proven) | 7 | 195 | 585 | 2,130,225 | 2,132,694 B | **7,659,550** | 11 × 2^20 (11,534,336 total) |
| 186..=202 (the two scheduled sessions; executor only) | 185 | 17 | 51 | 87,349 | 89,106 B | 2,120,216 | 3 |

Executor: r0vm 3.0.5 on the build box (`out/executor.jsonl`), both journals equal to the native run's. The
program is `checkpoint_b` `0xd3597543…0d64`, the recorded ELF of §4 (sha256 `386a3a31…80d9`), not rebuilt (the
guest sources are unchanged since), the same program as the v5 vault's earlier rule (B) test.

GPU (`../gpu-prover/results/realblocks-l4/`): one NVIDIA L4 (our Modal workspace, GCP us-east), 10 Nov
22:48 UTC: **39.5 s** (segment proofs 29.2 s, recursion 9.6 s; container 41.4 s), succinct receipt 224,314 bytes,
verified in the container and again on the build box, journal equal to the native one (sha256
`887c8d44…0d5f`). Groth16 wrap: identity_p254 17.8 s in r0vm on the build box, RISC Zero's Groth16 prover image
15 s through `mwrap`; Groth16 receipt 1,585 bytes, verified; seal 260 bytes, selector `0x73c457ba`.

### The vault

`vault/test/real/RealBlocks.t.sol` (fixture `test/real/fixtures/real-blocks.json`, written by
`realblocks/vault-fixture.py`): the v5 vault in mode (B) at the address the journal binds on chain 31337, RISC
Zero's unmodified Groth16 verifier behind its emergency stop, the genesis state recomputed in Solidity from the
lab channel's key, its timeframe and configuration tip and block 7, and equal to the proof's parent. Forge 1.3.6
on the build box (`realblocks/out/forge-real.txt`): the 10 tests pass, and all 31 of `test/real/`.

| step | verdict |
|---|---|
| `submitCandidate` with the real seal, by anyone | accepted; the vault keeps sha256 of the journal |
| `finalizeAttested` at once, and one second before the hour | refused, `WaitNotOver` |
| `finalizeAttested` at the hour, by anyone | accepted: `finalState` = the proof's new state at block 202, its burn root accepted |
| one flipped byte in the selector or in any of the 8 proof words | refused |
| the real seal with another tip hash, tip id, new state or `n` | refused, `VerificationFailed` |
| a vault of another key set (the attestation key alone; the channel key and the attestation key) | refused, `ParentNotFinal` |
| a vault whose floors ask for more (nMin 3, tMin 2) | refused, `WrongCommitteeFloor` |
| a rule (A) vault (rule (A)'s image id) | refused, `VerificationFailed` |

Gas (`forge test --isolate --match-contract RealBlocksGas`): `submitCandidate` **347,832**, `finalizeAttested`
**195,149**, the same as the fixture-built rule (B) proof (347,820 and 195,149): the vault's cost does not
depend on the blocks.

### What is real and what is still stand-in

Real: the blocks (LEZ v0.4.0-rc1's sequencer: in each of the 195, one of the lab's light-client job transactions
with the block's forced fee and clock transactions, 585 in all), their inscriptions on a Logos Blockchain node, the channel and its key, the node's finality, the
proof (GPU), the wrap, RISC Zero's verifier, the vault's code. Still stand-in or missing: the devnet itself
(one validator with dev-mode leader proofs, one sequencer, one channel key, n = 1: no adversary, no network);
no bridge program, so no burns, imports or sunsets in the blocks and labels in the configuration's bridge fields;
the attestor is attestor-core driven by `realblocks` after the fact from a copy, not a running Logos module, and
its key is label-derived; no release against this checkpoint (nothing was burned); a local EVM (chain 31337),
nothing sent to a public network.
