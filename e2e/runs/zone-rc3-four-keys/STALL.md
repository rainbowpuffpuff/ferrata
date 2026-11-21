# The channel stall in the last two minutes of this run: what the logs show, what the code rules out, what is unknown

Sources: [`stall-excerpts.txt`](stall-excerpts.txt) (the lines quoted below, from the run's logs, which keep their last
300 lines each, and the channel's state at the end), [`inscriptions.json`](inscriptions.json) (the node's final
blocks), and Logos Blockchain at `f6d4267f` (9 Nov 2026), the revision LEZ v0.4.0-rc3's `Cargo.lock` pins (crates
0.3.5-rc.5; the node of crown-rc3's bundle reports 0.3.5-rc.1). Read on 21 November 2026; nothing re-run.

## What happened (measured, from the logs)

The channel: four accredited keys, key 0 = seq-0, 1 = seq-2, 2 = seq-3, 3 = seq-1; posting timeframe 60 slots, posting
timeout 25 slots; 4 s slots; epochs of 60 slots. Turns run `[start, start + 60)`.

1. **seq-1's turn** (key 3) ran from slot 2447759 to 2447819. Its last mined inscription is block 93, in the node's
   block of slot 2447817 (18:04:55). One block later, at **18:05:02 (slot 2447818)**, seq-1 published **its own block
   94** on pin `385ff440…` (block 93's inscription, the channel's tip). The node's block for slot 2447818 had been
   proposed at 18:04:59, before that submission; from slot 2447819 on, key 3 is no longer the round-robin signer.
2. **seq-0's turn** (key 0) began at slot 2447819 ("Turn update: our_turn=true, starting_slot=Some(2447819)" at
   18:05:03). In the same second, **18:05:03 (slot 2447819)**, seq-0 published **a different block 94** on the same pin.
3. The node's leader proposed the blocks of slots 2447819 (18:05:03.11) and 2447820 (18:05:07.11) with **0
   transactions, 1 removed** each; an **epoch transition** (epoch 40797) happened at slot 2447820. Neither block 94
   was ever mined: the channel's tip stayed at slot 2447817 to the end.
4. seq-0 went on publishing blocks 95 … 105 every 10 s, each on the pin of its own previous (unmined) inscription
   ("Publishing block 95 on pin b7b4e857…"); the leader removed that growing lineage at its proposals ("2 removed" …
   "5 removed"). seq-1 kept its own block 94 as head; seq-2 and seq-3 stayed at 93 (the poll's heights 105/94/93/93).
5. At slot 2447842 (tip + 25), the posting timeout moved the turn by one key, back to key 0 (seq-0: "our_turn=true,
   starting_slot=Some(2447842)", 18:06:35). The sequencers were stopped at 18:07:27 (about slot 2447855), 12 slots
   before slot 2447867 (tip + 50), where the timeout would have moved the turn to key 1 (seq-2, still on block 93).

The earlier three turn changes of this run went through: each new turn's first inscription was mined 3 slots after
its turn began (slots 2447642, 2447702, 2447762), and the heights were equal at every poll across them.

## What the code rules out

At `f6d4267f`, an inscription is valid when its parent is the channel's `tip_message` and its signer is
`accredited_keys[round_robin(block_slot).0]` (`core/src/mantle/ops/channel/inscribe.rs`, `verify`; `round_robin` in
`core/src/mantle/channel.rs`). With the channel's state at the stall (tip slot 2447817, tip sequencer 3, its starting
slot 2447759, timeframe 60, timeout 25), `round_robin` gives key 0 at slots 2447819 and 2447820, and seq-0's block 94
names the tip as parent. **So seq-0's inscription passes both channel checks at both slots: what removed it was
another check of its transaction.** seq-1's block 94 fails the signer check from slot 2447819 on (key 3), which by
itself explains one of the two removals. The leader removes a transaction from the mempool whenever
`try_add_transaction` fails with any error other than the gas limits (`services/chain/chain-leader/src/tx_selection.rs`,
`select_transactions`); a dependent transaction whose parent was never applied fails the parent check, which explains
the removal of seq-0's whole lineage once its block 94 was gone.

## What is unknown, and the candidates

**Unknown: the error that removed seq-0's block 94.** The leader logs it only at TRACE level (`tx_selection`, "%err"),
and this run's node logged at INFO. The candidates the evidence allows:

- **The fee inputs.** All four sequencers pay their inscriptions from one funding key (`attestor/zone.sh`: every
  `sequencer_service setup … --funding-public-key $FUND`, the dev node's funded key). seq-1's late block 94 and seq-0's
  block 94 were built one second apart on the same funding-key state; a transaction whose input another sequencer's
  transaction spends, or that its wallet believes unspent, is invalid. Upstream's zone SDK changed right after
  `f6d4267f` in this direction: `b570ec75` "expire bundles with missing inputs (#3941)", `ee5e3444` "config shed drops
  chained custom txs too (#3940)", `ad5f6663` "refuse a hand-built tx behind the publish tip (#3942)", `7e099cf7` "run
  the lineage sheds on every block (#3943)", `a159f8c1` "expire mirrored entries stuck unmined (#3944)" (subjects
  only; their bodies are empty). That these fix this stall is not shown.
- **The epoch transition at slot 2447820.** rc3's known stall (LEZ issue 1109: a storage gas price step at an epoch,
  seen in our single-sequencer rc3 run as "storage gas price 2→3", then nothing final for 20 minutes and a rewind)
  coincides with the second removal; no gas price line is in these logs to confirm or exclude it.

**Unknown: whether the zone recovers by itself.** The posting timeout would have handed the turn to seq-2 at slot
2447867; the run had stopped 12 slots earlier.

## How to settle it

Run the same zone (`ZONE_BEDROCK=bundle bash attestor/zone.sh <crown-rc3 bundle> OUT 1800`) with the node's tracing at
TRACE for `chain-leader` (the removed transaction's error) and long enough to pass two posting timeouts after a stall
(≥ 30 minutes); separately, one funding key per sequencer, to see whether the stall needs the shared key. The race
needs one sequencer to publish in the last slot of its turn and the next in the first slot of its own, which a 10 s
block interval against 4 s slots produces now and then; a run cannot force it.

What it means for a bridge run on this zone (F.7): a stalled channel stops new blocks, so the checkpoint's window must
end before a stall or after the zone recovers; the crown drivers already wait through rc3's stalls, and a run that
needs the zone to keep going must budget the posting timeout (25 slots, 100 s per key skipped) or restart.
