# MODEL: what LEZ v0.4 sequencers and Logos Blockchain actually do

The finality rule of `notes/design/SPEC-v2.md` §4 is an inference from sequencer behaviour. This file lists the behaviour the inference relies on, each item with the code that implements it, and flags (**W1–W9**) every place where the code is weaker than SPEC §4.6 assumes. The simulator (`sim/sim.py`) implements exactly these items; the item numbers are referenced in its comments.

**Clones** (read only):
- `[lez]` = `spike/upstream/lez-v0.4.0-rc1`, tag `v0.4.0-rc1`, commit `1806e87`.
- `[lb]` = `spike/upstream/logos-blockchain`, commit `ee5e344` (2026-11-09). LEZ pins `d480efc6`, an older revision (R1 header); the line numbers below are for `ee5e344`.

"UNVERIFIED" marks a statement I could not tie to code.

## M1. What an honest sequencer builds on, and when

**M1.1 The head is the final tier folded over the view.** `ChainState` keeps `final_state`/`final_tip` and `view`, "the unfinalized message lineage, oldest first, chained on `final_msg`"; the head "is always `final_state` folded over `view`" ([lez] `lez/chain_state/src/chain.rs:57-79`). The fold "applies every entry whose block extends the tip so far and skips the rest" (`chain.rs:60-62`, `fold_next` `:456-473`, `fold_block` `:523-538`). "Extends" means: the block hash recomputes, `block_id = tip + 1`, `prev_block_hash = tip.hash` (`lez/chain_state/src/apply.rs:97-138`), and then the whole block re-executes (`apply.rs:88-95`, retried up to 3 times for transient errors, `chain.rs:18-19`, `:540-561`). An entry without a block, or an invalid block, moves the pin but not the head (tests `chain.rs:706-737`).

**M1.2 Parent choice.** The next block is built on `head_tip()` (its `prev_block_hash`) with the head state as validation base; its inscription is chained on `pin()`, the last view entry ([lez] `lez/sequencer/core/src/lib.rs:1880-1909`, `chain.rs:144-165`). A view with a lineage gap skips the turn (`lib.rs:1895-1901`). Blocks bundling withdrawals let the SDK choose the inscription parent (`lib.rs:1359-1377`); the LEZ parent is still the head tip.

**M1.3 When it builds.** Only on its own turn: the executor drops `ProduceBlock` unless `is_our_turn` ([lez] `lez/sequencer/actors/executor/src/actor.rs:177-226`), which follows the zone-sdk turn notifications ([lez] `lez/sequencer/actors/bedrock/src/actor.rs:272-289`; executor `actor.rs:632-633`). The SDK computes the turn from the channel state of the chain its node follows and the slot clock ([lb] `zone-sdk/DECENTRALIZED_SEQUENCING.md:94-97`; `zone-sdk/src/sequencer/actor/mod.rs:136-145`). One block per `block_create_timeout` (testnet 10 s, [lez] `lez/sequencer/service/networks/testnet.json:5`); a node posting faster than `posting_timeout` keeps its turn (`lez/sequencer/core/src/config.rs:228-233`). A turn that finds the channel moved while building is given up (`lib.rs:1337-1346`).

**M1.4 Own unmined publishes are in the view at once.** After publishing, `record_publish` appends the inscription to the view if it chains on the pin, so the head gains the block immediately (`chain.rs:300-324`, `lib.rs:1654-1719`). The SDK's canonical chain "includes your own unmined publishes, which chain on the mined tip" ([lb] `DECENTRALIZED_SEQUENCING.md:266`, `:273`). A producer therefore builds on its own unmined blocks for its whole turn. A publish whose pin has moved is `Stale` and dropped (`chain.rs:313-315`, `lib.rs:1708-1715`).

**M1.5 Other sequencers' blocks enter the view only from the node's channel updates.** The LEZ Bedrock actor turns each SDK `BlocksProcessed` event into `ViewChange::Extension` (adopted entries) or `ViewChange::Conflict` (the whole new view) plus finalized entries ([lez] `actor.rs:179-271`); the SDK derives them from blocks its node processed, on the chain the node currently follows ([lb] `zone-sdk/src/sequencer/actor/mod.rs:67-108`, `block_fetch/mod.rs:144-200`). The core applies finalized entries first, then the view change (`lib.rs:959-1014`). So SPEC (ii) holds as worded: a peer's block reaches the head only after the node's Bedrock node processed an inscription carrying it. "Processed" means "in a block of the chain my node follows now", not "final".

**M1.6 Any inscription carrying the block's bytes will do.** A LEZ block has no producer field and no signature (R1 A1; [lez] `lez/common/src/block.rs:51-91`); the fold looks only at the block (`chain.rs:456-473`). The same block bytes can be inscribed by any key that is scheduled at some slot, and every honest node folds the copy as if it were the original.
- **W1.** "The key that inscribed `T`" (SPEC §4.2 item 4) is not unique. A malicious key can re-inscribe an honest block, and the prover chooses which inscription to present. SPEC §4.2 item 4 also says only counted inscriptions are checked, so as written the excluded key is never checked at all.

**M1.7 Re-execution before extending: holds.** Every block in the head was applied by `apply_block` on the state of its parent (M1.1). The final tier re-applies or reuses the fold's result and parks blocks that do not apply (`chain.rs:476-520`; tests `:1285-1328`).

**M1.8 Republishing.** A sequencer republishes its stored pending blocks only when it creates the channel (`lib.rs:507-601`). Orphaned blocks are not re-inscribed; their user transactions go back to the mempool (`lib.rs:1023-1035`, `:1119-1132`).

**M1.9 Timestamps.** A block's timestamp is the producer's wall clock (`lib.rs:2029-2030`); `apply` checks no bound or monotonicity (`apply.rs:97-138`; R1 "Unconfirmed"). SPEC v2 no longer uses timestamps.

## M2. What a Bedrock reorganisation does to the view and to pending blocks

**M2.1 View.** A conflict replaces the view by the canonical chain and refolds from the first entry the two views disagree on (`chain.rs:271-298`). Blocks that left the head without finalizing lose their place; their user transactions are pushed back into the mempool, not as the same block (`lib.rs:1023-1035`, `:1119-1132`).

**M2.2 Own pending inscriptions.** The SDK keeps an own pending inscription while its lineage reaches the channel tip, and sheds it, with everything chained on it, when it is off-branch ([lb] `zone-sdk/src/sequencer/state.rs:1022-1054`) or unmined for more than `stale_refund_slots` *LIB* slots since it was funded (`state.rs:555-590`, `:495-499`). LEZ submits through `submit_signed_tx`, which is shed rather than re-funded (`zone-sdk/src/sequencer/types.rs:83-89`; [lez] `actor.rs:314-336`); LEZ's minimum window is 50 slots ([lez] `lez/sequencer/core/src/config.rs:43-45`). Because the window counts LIB slots, an unmined block stays in its producer's head for about `k` Bedrock blocks plus 50 s, and indefinitely if the node's LIB stops moving.

**M2.3 Re-inclusion after a reorganisation is automatic.** The Bedrock mempool re-admits the transactions of blocks that left the canonical chain unless they are older than the TTL ([lb] `services/tx-service/src/backend/pool.rs:382-444`), default 1 h (`pool.rs:43`). Any party can also re-broadcast a signed inscription later. It is valid wherever its `parent` is the channel tip and its signer is the scheduled key at the including block's slot ([lb] `core/src/mantle/ops/channel/inscribe.rs:175-216`), and its fee note is unspent.
- **W2.** An inscription, or another inscription carrying the same block bytes (M1.6), can therefore be on an honest node's chain at time t1, off every honest chain later, and back on one at time t2. Seeing a block before time a and again after a + Δ does not mean it stayed on chain in between. This is the gap behind the counterexample in `LEMMA.md` §3.

**M2.4 Finality of entries.** The final tier advances only with "message-lineage entries whose L1 block reached finality" ([lez] `lez/sequencer/actors/bedrock/src/protocol.rs:110-111`), applied by `apply_finalized` (`chain.rs:326-421`); a finalized entry that does not apply is parked and the lineage moves past it (`chain.rs:476-505`, tests `:1285-1297`, `:1316-1328`).

**M2.5 Fork choice on one node.** In online mode a node switches to a longer chain only if the fork is at most `k` deep ([lb] `consensus/cryptarchia-engine/src/lib.rs:130-156`); the LIB is the deepest block within `k` of the tip (`lib.rs:64-86`). A bootstrapping node uses the density rule instead and can switch deeper (`lib.rs:89-128`).
- **W3.** "No reorganisation deeper than `k`" is a property of each node's chain. A node whose view is controlled by an attacker (eclipse) follows whatever chain it is shown, at whatever growth rate. SPEC (iii) talks about "Logos Blockchain" as one chain and says nothing about the chains the sequencers' own nodes follow.

## M3. How the round-robin schedule restricts who can inscribe when

**M3.1 Rule.** An inscription is valid only if `parent == channel.tip_message` and `signer == accredited_keys[round_robin(block_slot)]` (`inscribe.rs:186-203`); executing it moves the tip and the rotation state (`inscribe.rs:249-262`). `round_robin` advances one index per `posting_timeframe` slots from the current sequencer's starting slot, or, once nothing was inscribed for `posting_timeout` slots, one index per `posting_timeout` from the last inscription ([lb] `core/src/mantle/channel.rs:218-258`; `DECENTRALIZED_SEQUENCING.md:40-70`). The first valid inscription wins (`DECENTRALIZED_SEQUENCING.md:242-246`). Testnet values: `posting_timeframe = 900`, `posting_timeout = 150` slots of 1 s ([lez] `networks/testnet.json:15-16`).

**M3.2 Consequences.**
- At any slot, exactly one key can write on a given chain. The schedule is computed from that chain's own channel state, so it differs across forks.
- An honest key builds only in its own windows (M1.3). Support from `t` keys other than the producer therefore takes about `t` windows: 15 min each on testnet values when every key posts, 150 s when windows time out.
- A silent key costs the channel `posting_timeout` slots per round; it is skipped, not removed.
- **W4.** None of this binds the checkpoint proof: it checks signatures and hash links only. Malicious keys sign blocks off chain, outside their windows, in any number.

## M4. What "final" means for a LEZ node

A block is final for a LEZ node when it is in the node's final tier: the fold of the channel entries at or below the LIB of the chain the node's Bedrock node follows, which is at least `k` Bedrock blocks deep there (M2.4, M2.5; R2 Q2: indexer `lez/chain_state/src/zone_indexer.rs:44-60`, sequencer storage `lez/sequencer/actors/storage/src/actor.rs:247-262`). There is no finality certificate. `BedrockStatus::Safe` is never set, and the inscribed `bedrock_status` is not covered by the block hash (R1 A2, issue #1053).

## M5. How key sets change

- **Who is accredited.** Each key whose net stake reaches `minimum_sequencer_stake` is accredited: one key, one seat, not stake-weighted ([lez] `lez/programs/sequencer_stake/core/src/lib.rs:289-293`; minimum 25e9 on testnet, `networks/testnet.json:14`). The desired committee is read from the *final* LEZ state (`lez/sequencer/core/src/lib.rs:2630-2677`; `committee_discovery.rs:37-87`), "level-triggered" until Bedrock matches it.
- **Threshold.** `t(n) = max(1, min(⌈2n/3⌉, n − 1))` (`committee_discovery.rs:6-35`). Bedrock requires exactly the live `configuration_threshold` signatures of the current keys, on `parent == config_tip_hash` ([lb] `core/src/mantle/ops/channel/config.rs:222-288`). Executing the op replaces the key list and resets the rotation (`config.rs:296-335`).
- **Signing.** An honest sequencer signs a config only if it matches the target it derived from its own final state. It signs every funding variant of that target (`lez/sequencer/actors/channel_signing/src/actor.rs:271-317`).
- **Exit.** `UnstakeRequest` reduces net stake at once, so the key loses accreditation when the request lands. The stake is released `exit_delay` LEZ blocks later (`sequencer_stake/core/src/lib.rs:88-101`, `:164-180`): 240 blocks on testnet (`networks/testnet.json:17`), about 40 min at 10 s per block.
- **Slashing.** `Slash` needs ⌈2n/3⌉ ≥ 2 approvals, and "the reason for the offence is not checked" (`sequencer_stake/core/src/lib.rs:103-112`, `:312-322`). Only finalized inscriptions that are not valid blocks are reported as faults ([lez] `lez/sequencer/actors/slasher/src/protocol.rs:24-32`); a missed turn is not a fault.
- **W5.** The checkpoint program learns rotations only from the `ChannelConfigOp`s the prover chooses to supply (SPEC §4.2 item 3). Until a finalised checkpoint carries a rotation, the vault keeps counting the old keys. Their stake can be withdrawn about 40 min after they leave, while a checkpoint needs at least Δ (≥ 2 h) to finalise. SPEC (i) covers this only because it says "every key set the program has accepted"; the stake no longer backs it.
- **W6.** Seats are bought per key. Anyone holding `t(n + j)` minimum stakes can reach the threshold by adding `j` keys: no identity bound. Adding silent keys raises `t` and lowers liveness (`LEMMA.md` §6).

## M6. Bedrock parameters used

`security_param` k = 120, `slot_activation_coeff` f = 1/30, `slot_duration_in_seconds` = 1 ([lb] `deployment/ceremony/genesis/testnet/deployment-template.yaml:26-29`, `:134`; devnet identical; "standalone" uses k = 30). These are repo templates, UNVERIFIED for any live network (R2 §3 Q1). Expected time for k blocks: k/f = 3,600 slots = 1 h.

## Where the code is weaker than SPEC §4.6 assumes

| | SPEC §4.6 says or implies | The code does | Consequence |
|---|---|---|---|
| W1 | one "key that inscribed T" | any key can inscribe the same bytes; item 4 checks only counted inscriptions | as written, the exclusion is vacuous; with `t` keys an invalid `T` can be accepted (`LEMMA.md` §2, §5) |
| W2 | (ii) + (iii) imply `T` stayed on chain between the two proofs | the mempool and any key re-include inscriptions or copies after a reorganisation (M2.3) | the finality inference needs an extra progress assumption (`LEMMA.md` §3) |
| W3 | (iii) about "Logos Blockchain" | each node follows the chain it is shown (M2.5) | the assumption must hold on every honest sequencer's node: no eclipse |
| W4 | — | the schedule binds inscriptions, not the proof | malicious keys add support blocks at no cost |
| W5 | (i) "every key set the program has accepted" | rotations reach the vault only through finalised checkpoints; stake leaves after `exit_delay` ≈ 40 min | retired keys stay counted without stake behind them |
| W6 | "fewer than `t` keys" | seats are per minimum stake | the bound is a stake bound: about 2× the honest stake |
| W7 | (iv) "`prevrandao` is unpredictable Δ ahead" | it is public once the parent Ethereum block is out, about 12 s ahead, earlier for proposers of consecutive slots | harmless if Δ includes a margin ε (`LEMMA.md` §4) |
| W8 | (iii) "at least `k` blocks in any window of length Δ" | growth is f per slot minus forks; no bound under participation loss | a slowdown turns into "not `k` deep at finalisation" (RESULTS (e)) |
| W9 | — | a producer keeps its unmined blocks in its head while its node's LIB does not move (M2.2) | a partitioned producer keeps building on a block nobody else has; exclusion is what stops this (RESULTS (c)) |
