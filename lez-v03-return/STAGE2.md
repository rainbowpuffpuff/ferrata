# How far a real checkpoint gets on the public LEZ testnet's own data (18 November 2026)

A measurement, read-only, made after the stand-in ([`README.md`](README.md)) was done. Question: what would a
checkpoint program for the public LEZ testnet (LEZ v0.3.1) need, what of it can already be checked from public data,
and what cannot be proven without the zone's sequencers. Nothing was built for a guest; one native reader and its
test are here ([`stage2.py`](stage2.py), [`test_stage2.py`](test_stage2.py)).

Sources: `https://testnet.lez.logos.co` (`getBlock`, `getLastBlockId`); the public Logos Blockchain testnet's
explorer API under `https://testnet.blockchain.logos.co/web/explorer/api/v1` (`channels/<id>`, `transactions/<hash>`,
`blocks/list`, `blocks/<hash>`); LEZ's source at tag `v0.3.1` (`1a003bfb`) and at `v0.4.0-rc1`; this repository's
checkpoint core. Paths below without a repository are LEZ's at `v0.3.1`.

## 1. What public data already lets anyone check

Run offline over committed fixtures (`python3 -I lez-v03-return/stage2.py check`, four inscriptions, LEZ blocks
83000 to 83003):

| Fact a checkpoint guest checks | From public data | Result |
|---|---|---|
| the Mantle transaction hash of the transaction that inscribes a block | recomputed from the explorer's fields as `BLAKE2b-256("MANTLE_TXHASH_V1" ‖ u8 count ‖ ops)`, with the inscription operation `0x11 ‖ channel ‖ u32 len ‖ payload ‖ parent ‖ signer` and the fee transfer `0x00 ‖ inputs ‖ outputs`: the layout of [`checkpoint/crates/checkpoint-core/src/mantle.rs`](../checkpoint/crates/checkpoint-core/src/mantle.rs) (lines 7–19, 96–98), written for logos-blockchain `d480efc6` | equal to the explorer's hash for 4 of 4 fixtures, and for 19 of 19 inscriptions read live (LEZ blocks 82985 to 83003) |
| an accredited key signed it | Ed25519 over that hash, key = the operation's `signer` (verified here in plain Python, RFC 8032) | 4 of 4 verify; the signer is one of the channel's keys |
| the channel's key set | the channel's first operation, a `ChannelConfig` at L1 height 7,443: **4 keys**, `posting_timeframe` 900, `posting_timeout` 150, `configuration_threshold` 3. No other configuration operation was in 504 sampled operations | `n = 4`, so `t(4) = 3` |
| the message chain | each inscription's `parent` is the previous inscription's `MsgId` = `BLAKE2b-256(channel ‖ u32 len ‖ payload ‖ parent ‖ signer)`; deposits in between do not break it | holds for the 17 links checked, two of them across deposits (13 between blocks 83001 and 83002, 1 before 83003) |
| the payload is the LEZ block | the inscribed bytes are `borsh(Block)` of v0.3.1: equal to the node's `getBlock` bytes except the last byte (the status, `Pending` when inscribed) | 4 of 4 |
| the block's hash and link | `sha256("/LEE/v0.3/Message/Block/" ‖ 8 zero bytes ‖ block_id ‖ prev_block_hash ‖ timestamp ‖ transactions)` (`lez/common/src/block.rs:104-131`) | recomputed and linked, 4 of 4 |
| every transaction of the block | parsed field by field, no byte left: `Message {program_account_id, shard_selectors, nonces, instruction_data, fee}` and `WitnessSet` (`lee/state_machine/src/public_transaction/message.rs:13-20`, `witness_set.rs:6-8`, `fees.rs:13-21`) | blocks 2, 83000 to 83005: all transactions are public ones; 83003 holds 9 (6 with a fee, 3 system) |

So the **Logos Blockchain transaction layout that network uses is the one the checkpoint programs already read**:
no fork digest in the hash, the same operation codes and field order. The inscription half of a checkpoint (an
accredited key inscribed this block, on this message chain, under this key set) needs no change for this network.

Who signs: in 501 inscriptions sampled over the channel's history all four keys appear (142, 120, 134 and 105
times); the last 19 inscriptions, from LEZ block 82985 on, are all by one key, `30cfd05a…`.

## 2. What a v0.3.1 checkpoint program would have to change

| | LEZ v0.4.0-rc1 (what `checkpoint` and `checkpoint_b` read) | LEZ v0.3.1 (the public testnet) | Consequence |
|---|---|---|---|
| block body | `transactions`, then **`reverted: Vec<u64>`**, the positions of charged transactions whose action failed; it is under the block hash (`lez/common/src/block.rs:60-67, 103, 127-128` at `v0.4.0-rc1`; [`checkpoint-core/src/lez.rs`](../checkpoint/crates/checkpoint-core/src/lez.rs) 128–167) | `transactions` only (`lez/common/src/block.rs:59-62, 104-110`) | **a failed transaction stays in its block with no mark.** A failed action "was validly included and paid its fee, its effects just did not take" (`lez/chain_state/src/apply.rs:377-383`; `lee/state_machine/src/validated_state_diff/mod.rs:124-127`), and the RPC has no receipts |
| block hash | the same prefix, over `…, transactions, reverted` | the same prefix, over `…, transactions` | one line in the reader |
| a public transaction | an actor call: `message.execution.root` with a sender, a target and message bytes | a plan/apply call: `program_account_id`, the shards it names, `instruction_data` (`message.rs:13-20`) | a new, shorter transaction reader (the one in `stage2.py` is 25 lines) |
| a privacy-preserving transaction | `ProvenExecution`: a public root, boundary deliveries between private and public actors | `Message {public_actions: [{account_id, effects: Vec<DeferredPublicEffect>}], nonces, private_actions, validity windows, program_image_claims}` (`privacy_preserving_transaction/message.rs:17-32`) | a new reader; a private burn shows as a deferred public effect on the bridge's accounts |
| where a burn is visible | the `Burn` message to the bridge program, in the clear | the bridge port's `Burn` instruction (public), or its deferred public effect (private) | the same idea, other bytes |
| the burn tree | counted by the checkpoint program from the blocks; the bridge keeps none | the same would hold; the bridge port also keeps one in its state ([`INTERFACE.md`](INTERFACE.md) §4), but no block commits to state | the on-chain tree helps an attestor, not a proof |

The first row is the one that matters. On v0.4 a proof can skip a burn whose transaction the block lists as reverted,
because the list is part of what the sequencer signed. On v0.3.1 a proof that counts `Burn` transactions from the
blocks counts failed ones too: someone can send a burn that fails (a holder with no balance) and it would enter the
burn root and be releasable. A sound v0.3.1 checkpoint therefore needs one of: the bridge program made unable to fail
after inclusion; private burns only (the bridge port's note says a failed private transaction invalidates its block;
not checked here); re-execution of every block from genesis inside the proof; or a state commitment in blocks, which
v0.3.1 does not have. None of these is a change to a reader.

## 3. What cannot be proven without the sequencers, on any version

1. **Validity.** Blocks carry no state root. That a block's transactions were executed correctly is known only to
   nodes that executed them ([`docs/trust.md`](../docs/trust.md) §3.1). Unchanged from v0.4.
2. **Finality.** That an inscription is at or below Logos Blockchain's last irreversible block is known only to a
   node of that chain. Its blocks carry a Groth16 proof of leadership (the explorer's `proof_of_leadership`), whose
   public inputs are ledger and stake commitments that the header does not give; following the chain inside a proof
   was not attempted here and is not in the design. Rule (B) takes finality from `t` sequencers' attestations.
3. **The signers.** Rule (B) on this channel needs 3 of its 4 key holders to run `ferrata_attestor` and to certify
   an attestation key each for the deployment. Rule (A) needs later inscriptions by 3 keys other than the tip's
   producer; with one key inscribing, as now, it could not finalise anything.

So the honest summary for the public testnet today: inscription, key set, message chain, block hash and transaction
parsing are provable from public data with the programs' existing Logos Blockchain reader and a new, small LEZ
reader; validity and finality are not, and need the four key holders. That is the same line the design draws for
v0.4; v0.3.1 adds the unmarked failed transaction.

## 4. Finality on this network, measured

The LEZ node's `Finalized` flag follows Logos Blockchain's last irreversible block. Three observations place it
about 1,000 L1 blocks behind the head:

| Observation | L1 head | Block | Its inscription | Depth | The node says |
|---|---|---|---|---:|---|
| 14 Nov, 13:08 UTC (an earlier read of ours) | height 87,585 | 82093 | height 86,586 | 999 | Finalized |
| | | 82094 | height 86,595 | 990 | Pending |
| 18 Nov, 10:35 UTC | height 94,025 | 83001 | height 91,459 | 2,566 | Finalized |
| | | 83002 | height 93,358 | 667 | Pending |

Slots are one second (block 83001's own timestamp is slot 3,328,992; its inscription is in slot 3,329,031). From 14
to 18 November the chain made 6,440 blocks in 336,464 slots, 52 s per block, so 1,000 blocks are about 14.5 hours.

Two more things were seen on 17 and 18 November, causes not determined:

- **Inscriptions arrive a day late.** Block 83002 (timestamp 17 Nov, 03:14:40 UTC) was inscribed in slot 3,415,536
  and block 83003 (17 Nov, 10:06:40) in slot 3,440,254: each about 24 hours and 80 seconds after its own timestamp.
  Blocks before 83002 were inscribed within a minute. Blocks 83004 and 83005 (17 Nov, 18:54 and 23:26) were not
  inscribed yet at 10:35 UTC on 18 Nov. A search of LEZ's source at `v0.3.1` found no 24-hour constant.
- **Pending blocks are replaced.** A block 83006 with timestamp 08:50:23 UTC and 11 transactions was the node's head
  at 08:54 UTC; at 10:28 UTC the head was 83005; at 10:37 UTC the head was another block 83006, timestamp 10:30:23,
  12 transactions.

At that pace a transaction sent to the testnet is in a block within hours, inscribed a day after that block, and
final about 14 hours later. The stand-in attestor signs only a tip the node reports `Finalized`, so a live return leg
there takes about two days from the burn, of which the Ethereum side is five minutes.

## 5. Not done

- No guest was written or changed. The readers here are Python, for measurement.
- The privacy-preserving transaction of v0.3.1 was read in the source, not parsed from a real block: none was in the
  blocks parsed (2 and 83000 to 83005).
- The channel's history was sampled (504 of 83,454 operations), not read in full: a later configuration change
  would have to be followed as the checkpoint programs follow them.
- Inclusion of an inscription's transaction in its L1 block (the explorer's `block_root`) was not recomputed.
- Ed25519 is verified here by the cofactorless equation; the checkpoint programs use `verify_strict`.
