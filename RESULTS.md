# RESULTS: simulation of the SPEC-v2 §4 rule

## How these numbers were produced

`cd sim && python3 experiments.py` (Python 3.14, standard library only, 14 worker processes). Raw per-run records are in `results/raw.jsonl`; the generated tables are in `results/tables.md`. Every run is deterministic for its seed (seeds 1…N per configuration).

**Parameters**, unless a row says otherwise. They are the repository's testnet values (MODEL M3, M6):

| parameter | value |
|---|---|
| Bedrock `k` | 120 |
| Bedrock `f` | 1/30 |
| slot length | 1 s |
| block propagation delay | 2 slots, giving 3–4 % natural forks |
| `posting_timeframe` / `posting_timeout` | 900 / 150 slots |
| `block_create_timeout` | 10 s |
| `stale_refund_slots` | 50 LIB slots |
| mempool TTL | 1 h |
| Δ | 2 h |
| run length | 24 h of operation, then 4 h of settling with no new finalisations, so every finalised tip is decided (none of the 26,372 finalisations ended undecided) |

**Properties checked on every finalisation:**
- **P1:** the tip is on the canonical LEZ chain at the end of the run, i.e. in the fold of the final part of the public Bedrock chain.
- **P2:** when the vault finalised it, the tip was folded at least `k` blocks below the public Bedrock tip.
- **Invalid:** the tip or one of its ancestors is an invalid block.

**What the adversary can do in the model:**
- control `m` keys: sign anything off chain, re-inscribe any block, equivocate, go silent;
- mint private Bedrock blocks that follow the channel rules;
- for limited periods, decide which chain chosen sequencer nodes follow ("view control": a short eclipse, or a short private fork kept ahead);
- record beacons.

The tables report the longest view control and the number of private blocks each attack used.

**Not modelled:**
- an adversary with Bedrock stake racing honest leaders, so P2 failures cannot turn into P1 failures here; the analytic table prices that;
- key rotation (`LEMMA.md` §5.6 by argument);
- transaction contents: a burn is "being in a block";
- Ethereum reorganisations;
- `ε = 0`: the nonce is known at its block's time.

The honest prover runs one candidate at a time and always targets the current public LEZ head.

`sim/test_sim.py` checks the building blocks against upstream behaviour:
- `t(n)`;
- the round-robin worked example of `DECENTRALIZED_SEQUENCING.md:78-90`, and the timeout grid;
- the fold's skipping of invalid and duplicate-height blocks, as in the `chain.rs` tests;
- determinism.

Abbreviations in the tables: P1 = finalised tip not canonical; P2 = not `k` deep when finalised; inv. = invalid tip; "attack" = runs in which the scripted adversary got its tip finalised.

## (a) The rule as specified, `m = t − 1`: no violation while honest sequencers are live

The adversary's keys sequence like everyone else ("honest operation"). In the last four rows they also run the fork attack of (h) without the outage, with the same control over two honest nodes' views.

| n | t | m | scenario | runs | finalisations | P1 | P2 | inv. | attack | latency h, p10 / p50 / p90 | min depth at finalisation |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 3 | 2 | 1 | honest operation | 100 | 860 | 0 | 0 | 0 | – | 2.42 / 2.65 / 2.88 | 228 |
| 4 | 3 | 2 | honest operation | 100 | 699 | 0 | 0 | 0 | – | 3.06 / 3.29 / 3.47 | 293 |
| 7 | 5 | 4 | honest operation | 100 | 502 | 0 | 0 | 0 | – | 3.97 / 4.24 / 4.45 | 362 |
| 10 | 7 | 6 | honest operation | 100 | 400 | 0 | 0 | 0 | – | 4.94 / 5.20 / 5.41 | 437 |
| 3 | 2 | 1 | fork attack, honest live | 100 | 772 | 0 | 0 | 0 | 0/100 | 2.45 / 2.69 / 4.01 | 233 |
| 4 | 3 | 2 | fork attack, honest live | 100 | 326 | 0 | 0 | 0 | 0/100 | 3.08 / 3.36 / 6.55 | 305 |
| 7 | 5 | 4 | fork attack, honest live | 100 | 394 | 0 | 0 | 0 | 0/100 | 3.97 / 4.31 / 7.10 | 384 |
| 10 | 7 | 6 | fork attack, honest live | 100 | 392 | 0 | 0 | 0 | 0/100 | 4.92 / 5.24 / 7.98 | 491 |

- **Safety.** 0 violations in 4,345 finalisations, and every attack failed for the same reason: by the time of the beacon, honest blocks had moved the canonical chain past the attacked block's parent more than `k` Bedrock blocks back, so no fork the honest nodes accept can bring it back.
- **Latency.** From a tip's creation to its finalisation the latency is Δ (2 h) plus about two rounds of `t` posting turns of 15 min. It grows with `n` because `t` does.
- **Depth.** The shallowest finalised tip was 228 Bedrock blocks deep (k = 120). The attack rows have longer tails because the honest prover pauses while an attack runs.

## (b) `m = t`: the bound is tight

| n | t = m | attack | runs | finalisations | P1 | P2 | inv. | attack |
|---|---|---|---|---|---|---|---|---|
| 3 | 2 | an orphaned honest block, `D` checked | 50 | 100 | 50 | 50 | 0 | 50/50 |
| 3 | 2 | an invalid block, `D` checked | 50 | 428 | 0 | 0 | 0 | 0/50 (candidate refused) |
| 3 | 2 | an invalid block, §4.2(4) as written | 50 | 100 | 50 | 50 | 50 | 50/50 |
| 4 | 3 | orphaned honest block, `D` checked | 50 | 100 | 50 | 50 | 0 | 50/50 |
| 4 | 3 | invalid block, `D` checked | 50 | 349 | 0 | 0 | 0 | 0/50 |
| 4 | 3 | invalid block, as written | 50 | 100 | 50 | 50 | 50 | 50/50 |
| 7 | 5 | orphaned honest block, `D` checked | 50 | 54 | 50 | 50 | 0 | 50/50 |
| 7 | 5 | invalid block, `D` checked | 50 | 250 | 0 | 0 | 0 | 0/50 |
| 7 | 5 | invalid block, as written | 50 | 75 | 50 | 50 | 50 | 50/50 |
| 10 | 7 | orphaned honest block, `D` checked | 50 | 51 | 50 | 50 | 0 | 50/50 |
| 10 | 7 | invalid block, `D` checked | 50 | 200 | 0 | 0 | 0 | 0/50 |
| 10 | 7 | invalid block, as written | 50 | 52 | 50 | 50 | 50 | 50/50 |

- **Finality falls to `t` keys alone, with no Bedrock capability at all.** The `t` keys sign everything off chain and finalise an honest block that never became canonical (the last blocks of a turn often miss their window, M2.2).
- **Validity also falls to `t` keys if the excluded key is not checked**, which is what §4.2(4) says.
- **Checked, an invalid block needs `t + 1` keys.**
- After a wrong finalisation the honest prover never finalises again: every later candidate must descend from the bad tip (`LEMMA.md` §6).

## (c) Without excluding `T`'s producer

| n | t | m | scenario | runs | finalisations | P1 | P2 | attack | latency p50 |
|---|---|---|---|---|---|---|---|---|---|
| 3 | 2 | 1 | no exclusion, honest operation | 100 | 955 | 0 | 0 | – | 2.41 h (with exclusion: 2.65) |
| 4 | 3 | 2 | no exclusion, honest operation | 100 | 800 | 0 | 0 | – | 2.87 h (3.29) |
| 7 | 5 | 4 | no exclusion, honest operation | 100 | 600 | 0 | 0 | – | 3.83 h (4.24) |
| 10 | 7 | 6 | no exclusion, honest operation | 100 | 473 | 0 | 0 | – | 4.78 h (5.20) |
| 3 | 2 | 1 | producer partitioned > Δ, `D` checked | 50 | 362 | 0 | 0 | 0/50 | |
| 3 | 2 | 1 | producer partitioned > Δ, exclusion as written | 50 | 100 | 50 | 50 | 50/50 | |
| 3 | 2 | 1 | producer partitioned > Δ, no exclusion | 50 | 100 | 50 | 50 | 50/50 | |
| 4 | 3 | 2 | partitioned, `D` checked | 50 | 290 | 0 | 0 | 0/50 | |
| 4 | 3 | 2 | partitioned, as written | 50 | 100 | 50 | 50 | 50/50 | |
| 4 | 3 | 2 | partitioned, no exclusion | 50 | 100 | 50 | 50 | 50/50 | |
| 7 | 5 | 4 | partitioned, `D` checked | 50 | 201 | 0 | 0 | 0/50 | |
| 7 | 5 | 4 | partitioned, as written | 50 | 75 | 50 | 50 | 50/50 | |
| 7 | 5 | 4 | partitioned, no exclusion | 50 | 75 | 50 | 50 | 50/50 | |
| 10 | 7 | 6 | partitioned, `D` checked | 50 | 175 | 0 | 0 | 0/50 | |
| 10 | 7 | 6 | partitioned, as written | 50 | 52 | 50 | 50 | 50/50 | |
| 10 | 7 | 6 | partitioned, no exclusion | 50 | 52 | 50 | 50 | 50/50 | |

- **Without a partition, dropping the exclusion costs no safety** (0 violations in 2,828 finalisations) and saves 15–25 minutes, as Theorem F predicts: the exclusion is not used in the proof.
- **With a partition, the exclusion is the only defence.** One honest producer's node is cut off for more than Δ. It keeps building on its own unmined blocks (W9), and `m = t − 1` keys sign on top.
  - Without the exclusion, or with the exclusion as §4.2(4) writes it (the excluded key is named, not checked), the vault finalises a block that was never published: every run.
  - With `D` checked: never.
  - No private Bedrock block and no fork are needed: only one node cut off.

## (d) The earlier timestamp rule and v2, same scenario

The scenario, the reviewers' attack (r2-crypto §2):
- `m = t − 1` adversary keys;
- one honest sequencer's clock Δ ahead;
- the views of two honest nodes controlled for a few minutes: 3 private Bedrock blocks, 5 min (n = 3) to 18 min (n = 10).

The earlier rule is implemented as the reviewers attacked it: t keys after `T`, then t keys whose timestamps are ≥ Δ later, no exclusion.

| n | t | m | rule | runs | P1 | P2 | attack | view control, max |
|---|---|---|---|---|---|---|---|---|
| 3 | 2 | 1 | earlier (timestamps) | 50 | 50 | 50 | 50/50 | 317 s |
| 3 | 2 | 1 | v2 | 50 | 0 | 0 | 0/50 | 317 s |
| 4 | 3 | 2 | earlier | 50 | 50 | 50 | 50/50 | 468 s |
| 4 | 3 | 2 | v2 | 50 | 0 | 0 | 0/50 | 466 s |
| 7 | 5 | 4 | earlier | 50 | 50 | 50 | 50/50 | 764 s |
| 7 | 5 | 4 | v2 | 50 | 0 | 0 | 0/50 | 764 s |
| 10 | 7 | 6 | earlier | 50 | 50 | 50 | 50/50 | 1068 s |
| 10 | 7 | 6 | v2 | 50 | 0 | 0 | 0/50 | 1068 s |

- **The beacon removes the clock attack completely.**
  - Under the earlier rule the skewed honest block supplies the "Δ later" group at once. The adversary's keys date their own blocks early and late, and the vault finalises within minutes.
  - Under v2 the same scenario only produces a candidate. At the beacon, Δ later, the live honest sequencers have moved the canonical chain on.
- After any successful attack, here or in (b), (c), (h), no honest finalisation follows: every later candidate must descend from the bad tip, which is off the canonical chain. Checked on 49 sampled runs: 0 honest finalisations after the attack. Only `revokeFrom` could unstick the vault.

## (e) Δ against depth, and a Bedrock slowdown (n = 4)

"Fastest finalisation":
- the adversary's 2 keys sign every candidate's support at once;
- only one honest non-producer (two if `T`'s producer is an adversary key) must build on `T` before `a`, and after `Y`;
- this minimises `T`'s depth when the vault finalises, the worst case for P2.

"Honest prover only": nobody helps. Half rate: honest leader rate f/2 from hour 2 on, for the whole run.

| Δ | Bedrock | prover | finalisations | depth p1 / p10 / p50 (blocks) | share finalised below k = 120 | P1 |
|---|---|---|---|---|---|---|
| 1 h (1× k/f) | normal | fastest | 1,589 | 107 / 126 / 165 | **6.2 %** | 0 |
| 1 h | normal | honest only | 500 | 199 / 235 / 265 | 0 % | 0 |
| 1 h | half rate | fastest | 1,577 | 50 / 63 / 89 | **83.7 %** | 0 |
| 1 h | half rate | honest only | 602 | 72 / 87 / 111 | **64.0 %** | 0 |
| 2 h (2× k/f) | normal | fastest | 889 | 217 / 240 / 293 | 0 % | 0 |
| 2 h | normal | honest only | 349 | 313 / 339 / 379 | 0 % | 0 |
| 2 h | half rate | fastest | 940 | 106 / 119 / 146 | **10.1 %** | 0 |
| 2 h | half rate | honest only | 392 | 126 / 148 / 173 | 0 % | 0 |

- **Δ = 1 h is too short** even at full rate when the adversary hurries.
- **Δ = 2 h is enough at full rate** (shallowest 204 blocks) but not under a halved block rate: 10 % of fast finalisations land at 106–119 blocks.
- **No wrong finalisation in the model:** a block finalised at depth 37 (the shallowest, Δ = 1 h at half rate) stays canonical unless Bedrock reorganises it, and the model has no Bedrock adversary.
- The analytic table below prices that step. A block 60 deep is overtaken by a 45 % adversary with probability 6e-6, a 30-deep one with 2.4e-3.
- A slowdown therefore turns into a wrong finalisation only together with an adversary strong enough to reorganise tens of blocks, which is exactly what `k` is meant to exclude.

## (f) n = 2 and n = 1

| configuration | runs | finalisations | P1 | P2 | inv. | attack |
|---|---|---|---|---|---|---|
| n = 2, t = 1, m = 0: honest operation | 100 | 1,025 | 0 | 0 | 0 | – |
| n = 2, t = 1, m = 1: orphaned honest block | 50 | 100 | 50 | 50 | 0 | 50/50 |
| n = 2, t = 1, m = 1: invalid block, `D` checked | 50 | 515 | 0 | 0 | 0 | 0/50 |
| n = 2, t = 1, m = 1: invalid block, as written | 50 | 100 | 50 | 50 | 50 | 50/50 |
| n = 1, `allowSingleKey`: honest | 50 | 550 | 0 | 0 | 0 | – |
| n = 1, `allowSingleKey`: the key is the adversary's, invalid block | 50 | 100 | 50 | 50 | 50 | 50/50 |

- n = 2 gives `t = 1`. One key forges finality: it signs the support for an honest block that never became canonical. That is `m = t`, so the bound holds, but the tolerated number of bad keys is zero.
- With `D` checked, one key cannot get an invalid block accepted (it needs `t + 1 = 2`).
- n = 1 is one hot key, for validity and finality alike.
- The zero-key counterexample also runs at n = 2 (table (h)).

## (g) A withholding minority

`w` adversary keys never produce and never sign. Their turns are skipped after `posting_timeout`, so LEZ keeps running.

| n | t | w = 0 | w = 1 | w = 2 | w = 3 |
|---|---|---|---|---|---|
| 3 | 2 | 8.5 / day, p50 2.65 h | **0** in 30/30 runs | **0** | – |
| 4 | 3 | 7.0 / day, 3.30 h | **0** in 30/30 runs | **0** | **0** |
| 7 | 5 | 5.0 / day, 4.24 h | 5.0 / day, 4.17 h | **0** in 30/30 runs | **0** |
| 10 | 7 | 4.0 / day, 5.20 h | 4.0 / day, 5.27 h | 4.0 / day, 5.36 h | **0** in 30/30 runs |

- Finalisation stops exactly when `w > n − 1 − t`.
- With the exclusion, `t = n − 1` for n ≤ 5, so **one silent key stalls the vault** for n = 3 and 4. Nothing in v0.4 punishes it (MODEL M5).

## (h) The counterexample: two short forks around an honest outage

The attack of `LEMMA.md` §3. In each fork the adversary controls what two honest nodes see for a few minutes, using 3 private Bedrock blocks. Between the forks, all honest sequencers are down. Δ = 2 h unless stated.

| n | t | m | scenario | runs | P1 | P2 | attack | view control per fork, max | private blocks |
|---|---|---|---|---|---|---|---|---|---|
| 3 | 2 | 1 | outage from the candidate to the beacon | 100 | 94 | 94 | **94/100** | 317 s | 6 |
| 4 | 3 | 2 | same | 100 | 96 | 96 | **96/100** | 467 s | 6 |
| 7 | 5 | 4 | same | 100 | 98 | 98 | **98/100** | 767 s | 6 |
| 10 | 7 | 6 | same | 100 | 92 | 92 | **92/100** | 1068 s | 6 |
| 2 | 1 | **0** | no adversary key: the honest producer's own block | 50 | 50 | 50 | **50/50** | 169 s | 4 |
| 3 | 2 | **0** | same | 50 | 50 | 50 | **50/50** | 619 s | 6 |
| 4 | 3 | **0** | same | 50 | 50 | 50 | **50/50** | 777 s | 8 |
| 3 | 2 | 1 | 2 honest nodes eclipsed for Δ, other honest sequencers live | 50 | 50 | 50 | **50/50** | 7977 s | 6 |
| 4 | 3 | 2 | same | 50 | 50 | 50 | **50/50** | 8276 s | 6 |
| 7 | 5 | 4 | same | 50 | 50 | 50 | **50/50** | 9174 s | 6 |
| 10 | 7 | 6 | same | 50 | 50 | 50 | **50/50** | 8588 s | 6 |

How long must the outage last? n = 3, m = 1. The honest sequencers come back online this long before the beacon:

| back before the beacon | 0 min | 30 min | 45 min | 55 min | 60 min | 65 min | 75 min | 90 min |
|---|---|---|---|---|---|---|---|---|
| Δ = 2 h, attack succeeds | 45/50 | 45/50 | 45/50 | 36/50 | 18/50 | 8/50 | **0/50** | **0/50** |
| Δ = 3 h, attack succeeds | 45/50 | – | 45/50 | 37/50 | 17/50 | 7/50 | **0/50** | – |

- **Every finalisation the attack produced is a P1 violation:** the block never became canonical, and no honest node ever had it in its final tier.
- **The failures** (6, 4, 2 and 8 of 100) are attempts started while an honest Bedrock block was still in flight. The zero-key variant waits for a quiet moment and never fails.
- **The outage only has to last until about one hour before the beacon.** The cutoff is the time Bedrock needs for `k` blocks (k/f = 1 h), so it is the same for Δ = 2 h and Δ = 3 h. An honest block that lands later is less than `k` deep at the beacon, and a fork below it is still accepted.
- **So the outage needed is Δ − D_k:** about 1 h at Δ = 2 h and 2 h at Δ = 3 h. Raising Δ raises the price; it never removes the attack.
- **With honest sequencers live** (experiment (a), same adversary): 0/400.
- **The long eclipse needs no outage:** other honest sequencers keep LEZ live. It breaks the rule in every run because the eclipsed nodes' chain does not grow (assumption A4 per node).

## Analytic

P(fewer than k = 120 Bedrock blocks within a window Δ), slots of 1 s, a block per slot with probability f. This is the Binomial(Δ, f) lower tail, computed exactly in log space and cross-checked against `scipy.stats.binom`. Forks and withheld blocks only lower the growth.

| window | f = 1/30 | f = 1/60 (half the participation) |
|---|---|---|
| 1 h | 0.487 | ≈ 1 |
| 1.5 h | 5.71e-07 | 0.999 |
| 2 h | 1.27e-18 | 0.488 |
| 3 h | 1.20e-50 | 6.88e-07 |
| 4 h | 9.25e-89 | 2.17e-18 |

The smallest window `Δ_g` that keeps P(fewer than 120 blocks) below a target, at f = 1/30:

| target | Δ_g |
|---|---|
| 1e-6 | 1.50 h |
| 1e-9 | 1.65 h |
| 1e-12 | 1.78 h |
| 1e-18 | 2.02 h |

The chance that a Bedrock adversary with stake share `q` ever overtakes a lead of `z` blocks is `(q / (1 − q))^z` (gambler's ruin). It is the classic bound and ignores Praos details. It prices the step from "finalised before `k` deep" (P2) to "reverted" (P1):

| z | q = 0.10 | q = 0.25 | q = 0.40 | q = 0.45 |
|---|---|---|---|---|
| 30 | 2.4e-29 | 4.9e-15 | 5.2e-06 | 2.4e-03 |
| 60 | 5.6e-58 | 2.4e-29 | 2.7e-11 | 5.9e-06 |
| 90 | 1.3e-86 | 1.1e-43 | 1.4e-16 | 1.4e-08 |
| 120 | 3.1e-115 | 5.6e-58 | 7.4e-22 | 3.5e-11 |
