# LEMMA: what the SPEC-v2 §4 rule guarantees

Short answer.
- **Validity holds.** If the vault accepts a candidate for `T`, then `T` and every block below it are valid LEZ blocks, provided fewer than `t` keys are adversarial, honest sequencers re-execute, and the proof system is sound. Nothing about Logos Blockchain ("Bedrock") or time is needed.
- **Finality, as specified, does not hold.** With SPEC §4.6's assumptions (i)–(iv), the vault can finalise a block that never becomes final on LEZ, with one adversarial key out of three, and even with none (§3).
- **Finality holds with an added assumption.** It is true under a liveness ("progress") assumption on honest sequencers that §4.6 does not state, with Bedrock's assumptions applied to each sequencer's own node, and with Δ sized by a budget (§4).
- **SPEC-v3 (§8).** Its wait `W` absorbs turn hogging by up to `t − 1` adversarial keys, which breaks any flat Δ for a large enough committee. It still needs honest progress (F2). The checkpoint freeze carries everything else, and only if a bad candidate is cancelled rather than merely frozen.

Model items (M…, W…) refer to `MODEL.md`. Experiment letters refer to `RESULTS.md`. "SPEC-v2" (also "SPEC") is [`docs/design/spec/bridge-spec-v2.md`](../spec/bridge-spec-v2.md); "SPEC-v3" is
[`docs/design/spec/bridge-spec-v3-changes.md`](../spec/bridge-spec-v3-changes.md).

## 0. The rule, as analysed

- `K`: the key set a proof uses, after the `ChannelConfigOp`s the prover supplied (§4.2 item 3); `n = |K|`, `t = t(n) = max(1, min(⌈2n/3⌉, n − 1))`.
- A *path*: LEZ blocks `P.tip → … → T → … → E` linked by `prev_block_hash`, ids increasing by one (§4.2 item 1).
- An *inscription of B by k*: a Mantle transaction on `channelId`, signed by `k`, whose payload is `B`'s bytes. Any key can sign one for any block, off chain, at any time (M1.6, W4).
- `D`, "the key that inscribed T". As written, §4.2 item 4 signature-checks only the counted inscriptions, so `D` is a name the prover picks (W1). Below, "rule as specified" means `D` is the signer of an inscription of `T` that the program checks like the counted ones. "As written" means it is not checked.
- `S(Y, E)`: the keys other than `D` that signed checked inscriptions of blocks in `(Y, E]`.
- **Candidate** for `T`, accepted at Ethereum time `a`: some path has `|S(T, E₁)| ≥ t`.
- **Finalisation**: a beacon with nonce `Nᵢ` recorded at Ethereum time `b ≥ a + Δ`. `Y` is the first block after `T` on the path whose transactions carry `Nᵢ`. Some path has `|S(Y, E₂)| ≥ t`. Both proofs start from `finalState` and end at the same `newState`, so the same `T` and the same `K`.

## 1. Assumptions

| | assumption | in SPEC §4.6? |
|---|---|---|
| A1 | In every key set a proof uses, fewer than `t` keys are adversarial, now or later: retired keys included (W5). | (i) |
| A2 | Honest sequencers behave as M1.1–M1.5 (code-backed). They sign inscriptions only of their own blocks. They build only on their head. The head contains only final-tier blocks, blocks carried by inscriptions on the chain their own node follows, and their own unmined blocks. Every block in it re-executed on its parent's state. The head always extends the final tip (`chain.rs:144-150`, `:447-473`, `:394-421`). Re-execution is deterministic across nodes, including cycle metering (`apply.rs:628-648`); LEZ's own consensus assumes the same. | (ii); determinism implicit |
| A3 | Bedrock safety *on every honest sequencer's node*: the part of a node's chain at or below its LIB is a prefix of every honest node's later chain and of the eventual canonical chain `C∞`. | (iii), but only for "Logos Blockchain" (W3) |
| A4 | Chain growth *on every honest sequencer's node*: on the chain its LEZ node has processed, at least `k` blocks are added in every interval of length `Δ_g`. Fails under eclipse, lag or slowdown. | (iii), with `Δ_g = Δ` (W8) |
| A5 | LEZ progress: if at time `s` an honest node's head has height ≥ `h`, then `C∞` includes, in a Bedrock block of slot ≤ `s + Δ_p`, an entry whose fold reaches height ≥ `h`. In words: honest sequencers are up and not censored, so what they hold reaches the canonical chain within `Δ_p`. | **no** |
| A6 | The proof system is sound. | (iv) |
| A7 | Nobody knows a beacon's nonce more than `ε` before the beacon block's timestamp. | (iv) asks only that it be unpredictable Δ ahead, which is too weak: if it were known Δ/2 ahead, the rule would wait only Δ/2 (§5.7) |
| A8 | `Δ ≥ Δ_g + Δ_p + ε`. | no |

## 2. Validity: true under A1, A2, A6

**Claim V.** If the vault accepts a candidate for `T`, every block on the path from `P.tip` to `T` applies on the state its ancestors produce.

**Proof.**
1. `|S(T, E₁)| ≥ t` and at most `t − 1` keys of `K` are adversarial (A1, A6). So some honest key `h` signed an inscription of a block `B` in `(T, E₁]`.
2. `h` signs only its own blocks and builds them on its head (A2). So `B`'s ancestors, `T` among them, were in `h`'s head.
3. Every block in a head re-executed on its parent's state, or was produced by `h` on such a head (A2, M1.1, M1.7). Hence the path to `T` is valid. ∎

Notes:
- The exclusion of `D` is not used.
- Bedrock and Δ are not used.
- Validity of `T` does not imply that its burns are canonical (§3).
- With `D` unchecked (as written), `m = t` keys suffice for an *invalid* `T`: name an honest key as `D` and sign `t` blocks off chain. Experiment (b) and (f): accepted in every run.
- With `D` checked, an invalid `T` needs `t + 1` keys: only adversarial keys sign an invalid block's inscription, and `D` must be one of them. Experiment (b): refused in every run.

## 3. Finality as specified is false: the counterexample

**Lemma as specified.** "Under (i)–(iv), the vault finalised `T` ⇒ `T` is in the final tier of every honest LEZ node." **False.**

**Smallest counterexample with an adversarial key: n = 3, t = 2, m = 1.**
- Keys: `H0`, `H1` honest, `M` adversarial. Δ = 2 h; testnet turn parameters (M3.1).
- The adversary can, twice for a few minutes, decide which Bedrock chain the nodes of `H0` and `H1` follow. It does so with a short private fork (1–3 blocks) that it keeps ahead, or by delaying blocks to those two nodes.
- All honest sequencers are down (or their inscriptions are not included) for most of the time between the two forks.

Steps:
1. **M's turn on the public chain.** `M` publishes an *empty* inscription. It takes the slot after the current channel tip, so no honest unmined inscription waiting there can land later. An empty payload is "a missed turn", not an offence ([lez] `lez/sequencer/actors/bedrock/src/actor.rs:236-240`; `slasher/src/protocol.rs:24-32`).
2. `M` builds a valid block `T_A` on the canonical head, holding a burn it will double-spend. It inscribes `T_A` only in a private Bedrock block `β1` on the public tip. `M` is scheduled, so the inscription is valid (M3.1).
3. **First fork.** The nodes of `H0` and `H1` follow `β1`. On `β1` nothing more is inscribed, so the turn passes after 150 slots (M3.1). `H0` builds on `T_A`, and the adversary includes `H0`'s inscription in `β1'`. 150 slots later `H1` builds on top. Both honest sequencers re-executed `T_A` and processed it on the chain their node followed. That is exactly (ii).
4. **Candidate** at Ethereum time `a`: `S(T_A, ·) = {H0, H1}` (`D = M`). The nodes return to the public chain. `T_A` is not on it, and their blocks on `T_A` are shed (M2.2).
5. **Stall.** `H0` and `H1` are down until about `b − D_k`, where `D_k` is the time Bedrock takes for `k` blocks: ≈ 1 h. The canonical LEZ fold stays at `T_A`'s parent. The channel tip stays at `M`'s empty inscription. Bedrock grows normally.
6. **Second fork, at the first Ethereum block `b ≥ a + Δ`.** Anyone records a beacon. In a slot where `M` is scheduled on the public chain, `M` re-inscribes `T_A` (M1.6) and a block `Y` on top carrying the nonce, in a private block `β2`. `β2` sits on the public tip, or up to `k − 1` blocks below it if honest blocks landed in the last hour. `T_A` folds, since nothing advanced the canonical fold past its parent.
7. The two nodes follow `β2` for a few minutes. In their turns they build on `Y`. The finalisation proof has `S(Y, ·) = {H0, H1}`. **The vault finalises `T_A`.**
8. The fork is dropped. The next honest block at `T_A`'s height becomes canonical. `T_A` is never final on any honest node, and the burn in it is undone.

Each of §4.6's assumptions holds in this run:
- (i): one adversarial key, below `t = 2`.
- (ii): honest nodes built only on blocks their node processed on chain and re-executed.
- (iii): no node reorganises deeper than `k` (with a full outage, no deeper than the 3 private blocks of a fork), and the canonical chain grew ≥ k blocks in every Δ.
- (iv): the nonce was unknown before `b`.

What fails is A5: no honest block reached the canonical chain for about Δ − D_k.

**Without any adversarial key: n = 2, t = 1, m = 0.** Same timeline, with three differences:
- The adversary captures the producer `H0`'s node at the start of its turn, so `H0`'s next block `T_H` is inscribed only on the private fork. The attack starts only when no other honest unmined inscription waits on the channel tip (`sim.py` `competitor`).
- `H1` provides the support on both forks.
- On the second fork `H0`'s original inscription is re-included (M2.3). When `H0` comes back, its pending inscription has expired (M2.2), so it builds a different block at that height.

So the rule's finality can fail with zero corrupted sequencer keys.

**Simulator** (`RESULTS.md` (h), Bedrock and LEZ testnet parameters):
- Rule as specified, `m = t − 1`, two short forks around an honest outage: violations in 380 of 400 runs (94, 96, 98 and 92 of 100 for n = 3, 4, 7, 10). The 20 failures are attempts that started while an honest Bedrock block was still in flight.
- `m = 0`, `n ∈ {2, 3, 4}`: violations in 150 of 150 runs.
- Each fork controls what two honest nodes see for 5 min (n = 3) to 18 min (n = 10) and uses 3 private Bedrock blocks.
- The same attack with honest sequencers live (experiment (a)): 0 successes in 400 runs. The canonical chain has moved past `T_A`'s parent more than `k` blocks before `b`.
- How long the outage must last (n = 3, Δ = 2 h): the attack succeeds in 45/50 runs if the honest sequencers return 45 min or less before the beacon. It succeeds in 36, 18 and 8 of 50 if they return 55, 60 and 65 min before, and never if they return 75 min before. The cutoff is the time Bedrock needs for `k` blocks, and it is the same at Δ = 3 h (45, 37, 17, 7, 0 of 50). So the outage needed is Δ − D_k: about 1 h at Δ = 2 h, about 2 h at Δ = 3 h. Raising Δ raises the price; it never removes the attack.

## 4. Finality, repaired

**Theorem F.** Assume A1–A8. If the vault finalises `T` at time `τ`:
- `T` is in the final tier, at least `k` Bedrock blocks deep, of the honest node `h₂` named below, at some time `s₂ ≤ τ`;
- `T` is on the eventual canonical LEZ chain;
- `T` therefore reaches the final tier of every honest node that follows the canonical chain.

**Proof** (`j` = height of `T`).

| step | claim | uses |
|---|---|---|
| 1 | From the candidate: some honest `h₁ ∈ S(T, E₁)` had `T` in its head at a time `s₁ < a`. Its signed inscription exists before the proof that the vault checked at `a`. | A1, A2, A6 |
| 2 | From the finalisation: some honest `h₂ ∈ S(Y, E₂)` had `Y`, hence `T`, in its head at a time `s₂ ≥ b − ε ≥ a + Δ − ε`. `Y` carries `Nᵢ`, so it was built after `Nᵢ` was known, and `h₂` built after `Y`. | A1, A2, A6, A7 |
| 3 | `h₁`'s head had height ≥ `j` at `s₁`. So `C∞` folds a block `X` of height ≥ `j` at a Bedrock block `x` with `slot(x) ≤ s₁ + Δ_p < a + Δ_p`. | A5 |
| 4 | On the chain `h₂` had processed at `s₂`, the LIB has slot ≥ `s₂ − Δ_g ≥ a + Δ − ε − Δ_g ≥ a + Δ_p > slot(x)`. | A4, A8 |
| 5 | The part of `h₂`'s chain up to its LIB is a prefix of `C∞` (A3). `x` and that LIB are both on `C∞`, and `x` comes first, so `x` is final for `h₂`. The fold is deterministic, so `h₂`'s final tier holds `X`, of height ≥ `j`. | A3, A2 |
| 6 | `h₂`'s head extends its final tip, so a head block of height ≤ height(`X`) is a final-tier block. `T` is in the head and has height `j`, so `T` is in `h₂`'s final tier: at or below `h₂`'s LIB, hence `k` deep there, at `s₂ ≤ τ`. | A2 |
| 7 | `h₂`'s final tier is a prefix of `C∞`'s fold, so `T` is canonical and every honest node's final tier eventually contains it. | A3 |

Notes on the proof:
- Neither the exclusion of `T`'s producer nor any check of Bedrock inclusion is used. Under A1–A8 the rule without exclusion is equally safe; experiment (c) shows no violation in honest runs without it.
- Exclusion matters only when A3/A4 fail for one node: a partitioned producer keeps building on its own unmined block (W9). With `D` checked, the vault refused in every run of (c). Without exclusion, or with the exclusion as written, it finalised in every run.

**Δ budget at the template values** (k = 120, f = 1/30, 1 s slots; `RESULTS.md`, Analytic):
- A4 fails in a given window with probability 1.27e-18 at Δ_g = 2 h.
- `Δ_p` is a policy choice: the longest stall of honest progress the deployment must survive. Normal operation needs minutes. A coordinated upgrade or a LEZ-wide halt needs hours.
- ε ≈ 1 min.
- With `Δ_p = 1 h`: `Δ ≥ 2.02 + 1 + 0.02 ≈ 3.05 h`.

## 5. The cases examined

**5.1 Bedrock short forks and re-inclusion.**
- An inscription that left the chain is re-admitted by the mempool for up to 1 h, and anyone can re-post it later (M2.3). Any scheduled key can inscribe a copy of the block's bytes (M1.6). So "seen before `a`" and "seen after `a + Δ`" do not imply "on chain throughout" (W2).
- Re-inclusion is harmless only while the canonical chain moves past the block's height in time, which is A5.
- Natural forks in the simulator: 3–4 % of Bedrock blocks.

**5.2 Equivocation.**
- Two different blocks at one height, on two forks, by one key: on any single chain the fold keeps the first and skips the other (`chain.rs:739-755`).
- Support is counted on one hash-linked path, so it never mixes the two. Theorem F does not care what else the key signed.
- The counterexample is equivocation-shaped (`T_A` on the private forks, an empty entry on the public chain). It succeeds only through the stall.

**5.3 Malicious keys appending blocks off chain.**
- They count toward `t` at no cost (W4), and the simulator gives them that for free.
- They never put a block into an honest head. The blocks between `T` and an honest supporter's block are ancestors of that block, so they are in its head: on its node's chain, valid.

**5.4 The excluded producer.** See the notes under Theorem F.
- As written, item 4 never checks `D`, so the exclusion removes a key of the prover's choosing: it is vacuous.
- Checked, it raises the cost of the eclipse attack. Instead of partitioning one producer's node, with no Bedrock block needed (c), the attacker must eclipse two honest nodes and mint a private chain ((h), long eclipse).
- `D` may be any key that signed *an* inscription of `T`'s bytes. The adversary can re-inscribe an honest block and name itself. That costs it one key from the support, so the bound `m ≤ t − 1` still leaves one honest non-producer.

**5.5 Small n and the formula.** `t(n)` gives (with exclusion, honest-produced tips):

| n | t | adversarial keys tolerated (finality, A1) | silent keys tolerated (liveness, `n − 1 − t`) | honest nodes the counterexample must fool |
|---|---|---|---|---|
| 1 | 1 | none: `allowSingleKey`, one key decides validity and finality ((f): an invalid block finalised in every run) | 0 | – |
| 2 | 1 | 0 for finality ((f): one key finalises an orphaned honest block in every run); 1 for validity if `D` is checked | 0 | 1 (with `m = 0`) |
| 3 | 2 | 1 | 0 | 2 |
| 4 | 3 | 2 | 0 | 2 |
| 5 | 4 | 3 | 0 | 2 |
| 6 | 4 | 3 | 1 | 2 |
| 7 | 5 | 4 | 1 | 2 |
| 10 | 7 | 6 | 2 | 2 |

- For n ≤ 5 one silent key stops every finalisation except of blocks it produced itself (§6).
- The tolerance `t − 1` above n/2 is not a strength in practice: with `t` keys the adversary also controls LEZ's own `ChannelConfigOp` threshold (M5).
- Every key set below `nMin = 3` or `tMin = 2` should be refused for anything but a testnet.

**5.6 Key rotation between candidate and finalisation.**
- `newState` contains `keySetHash` and `configTipHash`. The finalisation must reproduce the candidate's `newState`, so both proofs count against the same `K`. Theorem F needs A1 for that `K` only.
- A rotation on Bedrock between `a` and `b` changes nothing the vault accepts. It can only stop *this* candidate from finalising if fewer than `t` of the old keys keep producing. That is liveness, and a new candidate with the rotation fixes it.
- The safety cost is W5. The prover decides which config ops to supply, so the vault keeps counting retired keys until some finalised checkpoint includes the rotation. On testnet their stake is released 240 LEZ blocks (≈ 40 min) after they leave, while a checkpoint needs ≥ Δ.
- The guest also cannot tell a config op that landed from one that collected its signatures but never landed. Honest keys sign every funding variant of a target, and those carry the same op (`channel_signing/src/actor.rs:309-311`). But a draft whose target moved before it landed is dropped locally while its signatures remain (`:425-433`), so a different op on the same `config_tip_hash` can exist with valid signatures. Both carry a committee derived from final stake, so A1 covers them. But a prover can steer the vault onto a config branch Bedrock never took, which stalls it.

**5.7 `prevrandao`.**
- `block.prevrandao` of block `B` is the RANDAO mix left by `B`'s parent. Everybody knows it once the parent is published, 12 s before `B`.
- The parent's proposer knows it about 24 s ahead: its own reveal is a deterministic BLS signature.
- A coalition proposing `r` consecutive slots before `B` knows it about `12(r + 1)` s ahead and can bias it by `r` bits (skipping slots).
- `recordBeacon` is permissionless and anyone picks the block. That is harmless, since every usable beacon is ≥ `a + Δ`.
- So `ε ≈ 12(1 + r)` s; `ε = 1 min` covers `r ≤ 4`.
- §4.6 (iv)'s "unpredictable Δ ahead" is true but too weak for the argument, which needs the nonce unknown until `ε` before its block. Ethereum provides that. Write A7 and include `ε` in Δ.
- An Ethereum reorganisation that moves the beacon block changes the nonce. That invalidates proofs built on the old one: liveness only.

**5.8 Bedrock slowdown.**
- If fewer than `k` blocks arrive in `Δ_g` (A4 fails), step 4 fails. The vault can then finalise `T` before it is `k` deep (P2), although A5 still makes `T` canonical (P1) unless Bedrock reorganises it.
- Experiment (e), fastest finalisation (`m = t − 1` keys signing at once), share of finalisations below `k` blocks deep:
  - Δ = 1 h: 6.2 % (98 of 1,589); 0 % when only the honest prover acts.
  - Δ = 2 h: 0 of 889.
  - Δ = 2 h with Bedrock at half rate: 10.1 % (95 of 940). At Δ = 1 h with Bedrock at half rate: 84 %, and 64 % with the honest prover alone.
- No P1 violation in the model: it has no adversarial Bedrock reorganisation.
- The table in RESULTS (Analytic) prices one: depth `z` against stake share `q`, e.g. `z = 60`, `q = 0.45`: 6e-6.

**5.9 Honest sequencers offline longer than Δ.**
- While all are down, nothing finalises (each support set needs an honest member).
- When they return, the counterexample applies. It needs the stall to cover only about Δ − D_k, not Δ (§3, (h) sweep).
- If at least one honest sequencer keeps landing blocks, A5 holds and the rule is safe.

**5.10 Eclipsed sequencer nodes.**
- A3/A4 fail for the eclipsed nodes.
- Two honest nodes held on a private chain for Δ (about 6 private Bedrock blocks over 2.5 h) suffice with `m = t − 1`, *even while the other honest sequencers keep the canonical chain live* ((h), long eclipse: 200 of 200 runs, n = 3, 4, 7, 10).
- Eclipses of a few minutes are harmless under A5.

## 6. Liveness

Finalisation of a tip `T` needs all of these:
- **L1.** `n ≥ 2` (or `allowSingleKey`), `n ≥ nMin`, `t ≥ tMin` at every step; otherwise the program refuses forever (§4.2 item 3).
- **L2.** `t` keys other than `D` build descendants of `T` on one path, and Bedrock includes their inscriptions so that each builds on the previous one. Each key builds only in its own turn (M1.3). That is about `t` turns of `posting_timeframe` (15 min on testnet), or 150 s per turn of keys that are silent.
- **L3.** A relayer records a beacon at or after `a + Δ` and gets the echo transaction into a block `Y` that descends from `T`.
- **L4.** `t` keys other than `D` build after `Y`.
- **L5.** Ethereum, the prover, and the parent state stay put. A new `finalState` invalidates every pending candidate.

Measured on testnet parameters (experiment (a)): from a tip's creation to its finalisation, median 2.65 h (n = 3), 3.29 h (n = 4), 4.24 h (n = 7), 5.20 h (n = 10); 90th percentile 2.88, 3.47, 4.45, 5.41 h. That is Δ plus about two rounds of `t` turns. Without the exclusion: 2.41, 2.87, 3.83, 4.78 h.

**Every way a minority can stall finalisation:**
1. **Silence.** `w ≥ n − t` keys never produce and never sign. For n ≤ 5 that is one key ((g): no finalisation in any run). v0.4 has no liveness fault (MODEL M5), so silence costs nothing until honest keys vote a `Slash` (⌈2n/3⌉ ≥ 2 approvals).
2. **Silent seats.** Staking additional silent keys raises `t` with `n` (M5, W6): `w` silent stakes stall `n_h` honest keys once `n_h − 1 < t(n_h + w)`; e.g. 3 stakes against 7 honest keys.
3. **Churn.** Key-set changes between `a` and `b` that remove supporters force a new candidate. A minority can only remove its own keys, which lowers `t`.
4. **Front-running finalisation.** Finalising an older valid candidate first forces honest provers to restart from the new `finalState`: bounded griefing, one cycle per finalisation.
5. **Config branch.** Feeding the vault a signed-but-unlanded config branch (5.6): the vault can no longer follow Bedrock's key set. That is permanent; it needs a new vault or a revocation.
6. **One bad finalisation**, by any of the attacks above. Every later candidate must descend from the bad tip, so the vault halts. In the simulator no honest finalisation followed any successful attack. `revokeFrom` (§4.4) is the only way back, and only within `revokeWindow`.
7. **Bedrock level.** Leaders that drop channel transactions, or a Bedrock slowdown (L2–L4 take longer). This needs Bedrock stake, not sequencer keys.
8. **Echo censorship.** Refusing to include the echo transaction delays only, as long as one honest producer includes it.

## 7. Repairing the rule

The smallest changes that make the finality statement true:
- check `D`;
- state A3, A4 per node, add A5, restate (iv) as A7;
- fix `deltaFloor = Δ_g + Δ_p + ε`;
- add a reject-only stall guard.

No change to *what the two proofs show* removes A5. Every honest signature the vault can see certifies "`T` was in my head at time τ". No finite set of such certificates rules out "`T` left every honest chain and came back". Only a certificate of a node's final tier does: option (B), the final-tier attestations of SPEC-v3 D7. SPEC-v3 adopted the repair; §8 tests it.

## 8. SPEC-v3, the repaired rule, under test

SPEC-v3 (D3, D4, D6) does three things:
- checks `D`;
- waits `W = delta + (t − 1) × turn` with `delta ≥ Δ_g + S + ε`: 2 h + 1 h + 1 min by default, `turn = posting_timeframe = 900 s`;
- adds a checkpoint freeze that `monitor` sets when a candidate's tip is not in an honest node's final tier by `a + Δ_g`, or when that final tier has not advanced for `S/2`.

Experiments (i)–(k) in `RESULTS.md`.

### 8.1 Turn hogging: the attack the first repair did not price

How adversary keys hold their turns (code path):
- A scheduled key keeps its turn for the whole `posting_timeframe` as long as one of its inscriptions lands at least every `posting_timeout` slots. Executing an inscription sets `tip_slot` to the block's slot ([lb] `inscribe.rs:249-262`). `round_robin` stays on the timeframe branch while `slot − tip_slot < posting_timeout` (`channel.rs:221-241`).
- The inscriptions can be empty. LEZ treats an empty payload as a missed turn, not an offence ([lez] `bedrock/src/actor.rs:236-240`; `slasher/src/protocol.rs:24-32`).
- The turn order is the order of `accredited_keys`, which LEZ sorts by key bytes (`committee_discovery.rs:58-68`). An attacker grinds its keys into one consecutive run.

So `m` adversarial keys keep every honest inscription off the chain for `m × posting_timeframe` while every honest sequencer is up: 15 min × (t − 1) at testnet values. The simulator reproduces it exactly: in a traced n = 10 run, keys 4–9 held 900 s each and the canonical fold sat at the attacked block's parent for 5,400 s.

Combined with the two-fork attack of §3, with no outage:
1. `T_A` is made at the start of the first adversarial turn (`s₀`).
2. The fooled nodes' turns on the private fork come only after the adversary's own keys have timed out there, so the candidate lands at `a ≈ s₀ + (m + 1) × posting_timeout`.
3. No honest block lands before `s₀ + m × posting_timeframe`.
4. At the beacon `b = a + wait`, the second fork can still root below the first honest block if that block is less than `k` Bedrock blocks old.

Hence the attack succeeds when `wait + (m + 1) × posting_timeout − m × posting_timeframe` is less than the time Bedrock takes for `k` blocks (≈ 1 h ± 5 min).

The same fork-and-turn timing applies to any variant in which the attacked block is honest, so making `T_A` an honest block gains nothing. Moving the fork point earlier lets honest blocks overtake `T_A`.

Experiment (i), attack successes (each success is a never-canonical tip finalised; 100 runs, 50 for n ≥ 13):

| n | t | m | flat Δ = 2 h | flat Δ = 3 h 01 | v3 W |
|---|---|---|---|---|---|
| 3 | 2 | 1 | 0 | 0 | 0 |
| 4 | 3 | 2 | 0 | 0 | 0 |
| 7 | 5 | 4 | 0 | 0 | 0 |
| 10 | 7 | 6 | **67/100** | 0 | 0 |
| 13 | 9 | 8 | **48/50** | 0 | 0 |
| 16 | 11 | 10 | **50/50** | **16/50** | 0 |
| hogging + honest turns kept off chain, n = 3 / 4 / 7 / 10 | | | – | – | **43 / 48 / 50 / 45 of 50** |

**Verdict on the claim.** The claim is right about the mechanism and wrong about n = 7.
- A flat 2 h fails from n = 10 (t − 1 = 6 turns, 90 min of hog). It held at n = 7: 0/100, against a predicted ≈ 1 % per attempt.
- A flat 3 h 01 min holds up to n = 13 and fails from n = 16.
- In general a flat Δ fails once `(t − 1) × (posting_timeframe − posting_timeout) − posting_timeout > Δ − D_k`, roughly `t ≥ 7` at 2 h and `t ≥ 11` at 3 h 01.
- v3's `W` holds at every n (0 of 500 attempts), because its `(t − 1) × turn` term is exactly the hog.
- It does not hold once honest inscriptions are kept off the chain. That breaks F2, and only the freeze helps.

### 8.2 Is Theorem F's Δ_p "(t − 1) turns + S"?

Yes. The proof is unchanged; turn hogging is one way A5's window is used up while every honest sequencer is up. Write `Δ_p = (t − 1) × turn + S`, with `S` also absorbing the time an honest producer's first inscription takes to land (one Bedrock block; the turn is lost after `posting_timeout` if Bedrock produces nothing). It holds only under three conditions:
1. **`t − 1` must bound the adversarial keys in the channel's *live* accredited set,** not just in the vault's `K`. The hog length is set by the live schedule. If the vault's `K` is stale (W5), or the attacker staked extra keys (W6), the live set can hold more than `t(K) − 1` adversarial keys in a row.
2. **`turn` must be the live `posting_timeframe` and must be non-zero.**
   - With `posting_timeframe = 0`, a key that keeps posting keeps its turn indefinitely (`channel.rs:232-241`: with no timeframe and no timeout, the index stays at `tip_sequencer`).
   - LEZ copies the posting parameters from its genesis `ChannelParams` into every config it signs (`lib.rs:2674-2675`). A `ChannelConfigOp` signed by the threshold could still change them, and the checkpoint program applies config ops without looking at those fields.
3. **Honest inscriptions must be able to land.** A network adversary that keeps each honest sequencer's inscriptions off the chain for the first `posting_timeout` of its turn makes every honest turn pass with no block. Every adversarial turn is held by hogging. That is a total stall, F2 fails, and `W` does not help: experiment (i), last rows.

### 8.3 The monitor's checkpoint freeze

D4 does not say when the freeze ends. The tip condition ("not in an honest final tier by `a + Δ_g`") never clears by itself, so I tested both readings and an alternative:
- **held:** the freeze is kept until it expires (`maxFreeze`, 30 days);
- **lifted:** the freeze authority lifts it once another block is final at the tip's height;
- **cancel:** a reject-only call that SPEC-v3 does not have; the authority cancels the candidate instead of freezing.

The stall trigger freezes while the final tier is stalled, under all three.

Experiment (j), attack successes, 50 runs per cell, v3 W:

| attack | n | guard off | held | lifted | cancel |
|---|---|---|---|---|---|
| honest outage + two short forks, m = t − 1 | 3 / 4 / 7 / 10 | 45 / 48 / 50 / 45 | 0 | 0 | 0 |
| same, no adversary key | 2 / 3 / 4 | 50 / 50 / 50 | 0 | 0 | 0 |
| two honest nodes eclipsed for W | 3 / 4 / 7 / 10 | 50 each | 0 | 0 / 0 / **50 / 50** | 0 |
| hogging + honest turns kept off chain | 3 / 4 / 7 / 10 | 43 / 48 / 50 / 45 | 0 | **1** / 0 / 0 / 0 | 0 |

False pauses at S/2 = 30 min:
- Honest operation: 0 freezes in 800 runs, no honest finalisation delayed.
- A minority holding every one of its turns (within F2): the stall trigger froze every run, 7.3 h (n = 7) and 10.1 h (n = 10) per 28 h, delaying honest finalisations by up to 2.1 h.
- Bedrock at half rate: the tip trigger fired on honest candidates in 19 of 50 runs.

- **The freeze is load-bearing.** Without it, v3 falls to every F2 or F1 failure tested.
- **Read literally (held), it stops every attack and stops the vault.** In the attack runs no honest checkpoint is finalised afterwards. In 19 of 50 honest runs with Bedrock at half rate it halts the vault for the rest of the run.
- **Lifted once the tip is overtaken, it is unsafe.** Support collected before the lift remains valid. The long eclipse succeeds whenever a third honest sequencer keeps LEZ live (100 of 100 at n = 7, 10), and one hogging run won the race.
- **Cancelling the candidate stopped every attack and kept the vault finalising.** Its cost: under a Bedrock slowdown it cancels some honest candidates (20 in 50 half-rate runs), which then resubmit.
- **The stall trigger adds nothing once candidates are cancelled at `a + Δ_g`**, and at `S/2` it hands a turn-holding minority a pause.

### 8.4 Latency under v3

Experiment (k), from a tip's creation to its finalisation, honest operation:

| n | t | W | median | p90 | v2 flat 2 h median |
|---|---|---|---|---|---|
| 3 | 2 | 3.27 h | 4.00 h | 4.21 h | 2.65 h |
| 4 | 3 | 3.52 h | 4.67 h | 4.96 h | 3.29 h |
| 7 | 5 | 4.02 h | 6.11 h | 6.37 h | 4.24 h |
| 10 | 7 | 4.52 h | 7.68 h | 7.92 h | 5.20 h |

These numbers cover 1,661 finalisations; none was less than 379 Bedrock blocks deep.

### 8.5 D6 wording that matches what is proved

Proposed replacement for the Finality and "What breaks it" parts of SPEC-v3 D6. Validity, Floors and Chains stand.

> **Finality.** Those blocks are also final on LEZ (in every honest node's final tier, never reverted), provided in addition:
> - (F1) on the Logos Blockchain node of every honest sequencer, as its LEZ node has processed it, no reorganisation goes deeper than `k`, and at least `k` blocks arrive in every window Δ_g (no eclipse, no lag, no slowdown);
> - (F2) whenever an honest sequencer holds a LEZ block of height `h`, a block of height at least `h` enters the canonical chain's fold within `(t − 1) × turn + S`. This needs:
>   - at most `t − 1` adversarial keys in the channel's live accredited set, not only in `K`;
>   - `turn` = the live `posting_timeframe`, and > 0;
>   - honest sequencers up except for stalls of at most S;
>   - their inscriptions neither censored nor kept off the chain by eclipse;
> - (F3) nobody learns a beacon value more than ε before the Ethereum block that records it;
> - (F4) `W ≥ Δ_g + (t − 1) × turn + S + ε`. The vault enforces this only together with D3's rule that the checkpoint program refuses a `ChannelConfigOp` changing `posting_timeframe` (or setting it to 0).
>
> **What breaks it.**
> - `t` keys can sign any history. The candidate is public for `W` before it can finalise; the monitor's tip check stops it, and `revokeFrom` un-accepts it within the window.
> - Turn hogging by the adversary's own keys does not break the rule: `W` absorbs it.
> - A never-final block can be finalised with fewer than `t` keys, even with none, if F2 fails for more than `S` beyond the hog and the attacker can twice show two honest sequencer nodes a short private fork. F2 fails if honest sequencers stall, or their inscriptions are kept off the chain.
> - Two honest sequencer nodes eclipsed for about `W` defeat the wait whatever the honest sequencers do.
> - The monitor's tip check stops all of these, provided:
>   - `monitor`'s own node is not eclipsed;
>   - `monitor` and the freeze authority are live;
>   - the offending candidate is cancelled (or the freeze held until it can no longer be finalised).
>
>   Lifting the freeze once the tip is overtaken re-opens the eclipse attack. The stall check fires only after `(t − 1) × turn + S/2` without final-tier progress.
