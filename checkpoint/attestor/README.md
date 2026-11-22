# `ferrata_attestor`

The signer of finality rule (B) (SPEC-v3 D7 and D13; `docs/design/bridge.md` §8.3) that each LEZ sequencer
runs beside its Logos Blockchain node, and the collector that puts `t` of `n` of its attestations into a
`checkpoint_b` input. The signed bytes (the certificate, the final-tier message v2, the key-set hash) are
`crates/attestor-core`'s; this crate is the program around them. Results: [RESULTS.md](RESULTS.md).

Not audited. A prototype: a command-line program, and the same library as a Logos module,
[`ferrata_attestor_module`](../../modules/README.md#ferrata_attestor_module) (its methods are these commands;
`start` runs the attestor as a loop beside its node; [`modules/RESULTS.md`](../../modules/RESULTS.md)).

## What it does

1. **Certificate, once per deployment.** The sequencer's channel key signs
   `"ferrata/attest-key/v1" ‖ evmChainId ‖ vault ‖ lezConfigId ‖ channelId ‖ attKey` (`certify`). The channel
   key signs nothing else for the bridge.
2. **Reads its node's final tier** (`attest --node URL --lez URL --save DIR --state F`, or `--answers DIR` for
   saved answers of the same endpoints): `/cryptarchia/info` (the last irreversible block's slot),
   `/channel/<id>` (the live key set `K` and posting timeframe), `/cryptarchia/blocks` (the node's immutable
   blocks). It takes every inscription op of the channel in blocks at or below the LIB slot and reads each
   payload as a LEZ block (strict parse, re-hash). What does not parse, does not link or repeats is noted and
   passed over: one turn holder cannot stop the attestors (review M-4). From the tip it signed last (its
   state) it follows the chain as LEZ folds it: id + 1 and `prev_block_hash`.
3. **Checks the tip against its own LEZ node** (review H-2). Under rule (B) the program executes nothing, and
   Bedrock admits any payload from the key whose turn it is; LEZ re-executes every finalized inscription and
   parks a block that does not apply. So the attestor asks its LEZ node `getBlock(id)` and signs a tip only
   when that node serves it with status `Finalized` and byte for byte the inscribed block (a LEZ node stores
   only blocks that applied, and marks `Finalized` the ids up to its final tip, except that a block adopted
   from another sequencer keeps the status inscribed with it, LEZ #1053, so a head block can read `Finalized`
   early; the attestor therefore also requires the block's inscription to be at or below its Bedrock node's
   LIB, and never signs above it). Where two
   inscriptions offer the next block (a parked one and its honest replacement), the LEZ node decides. The
   tip's inscription is checked as the program checks `T`'s: the transaction's canonical encoding reproduces
   the node's hash (inscriptions, transfers, channel transfers and channel withdrawals: review M-3), the
   inscription is well framed, its signer's `verify_strict` signature covers the transaction hash, and the
   signer is in `K`; a tip that fails is passed over for the block below it.
4. **Signs** the final-tier message v2 over `(deployment, keySetHash(K), tip, finalMsgId)` with the
   certified attestation key, after checking that its certificate certifies that key for this deployment,
   that its channel key is in `K` (a certificate dies with its key), and that the tip descends from what it
   signed before (the tier must hold the signed tip with its hash). It records the tip, its message id, its
   slot and its signer in the state file, atomically and before the attestation is written. The next read
   starts at that slot (the work per round does not grow with the channel's age) and first asks the LEZ node
   whether it still holds the signed tip final; another block there refuses (two histories). It never signs
   two histories, across restarts too (review M7).
5. **Collects** (`collect`): keeps the published attestations whose channel key is in the parent state's
   key set, whose certificate and signature verify for the input's deployment and key set, and which repeat
   no key; refuses below `t(n)`; runs the rule natively and writes the input and its journal. The program
   checks everything again; the collector only avoids proving an input it would refuse.

```text
ferrata_attestor keygen --out KEY [--seed-label L]
ferrata_attestor certify --channel-key KEY --att-pub HEX --deployment DEP --out CERT
ferrata_attestor attest --deployment DEP --cert CERT --att-key KEY --out ATT \
    (--node URL --lez URL --save DIR --state F [--slot-from N] | --answers DIR [--lez-answers DIR] [--state F])
ferrata_attestor verify --deployment DEP --att ATT...
ferrata_attestor collect --base IN --att ATT... --out IN2 --meta JSON --name NAME
```

Also `pubkey`, `deployment` (the deployment a `checkpoint_b` input serves), `assemble` (an unchecked input,
for refusals), `upgrade` (an input recorded before inputs carried `T`'s inscription) and `rekey` (devnet
set-up). `src/bin/attestor-refusals.rs` writes inputs the program must refuse; it is a test tool.
`tier` prints the Bedrock node's view alone (diagnostics, devnet set-up; `attest` never signs it), and
`devnet-lez-standin` writes every inscribed block of saved answers as the LEZ node's final block, for replays
of recorded runs that saved no LEZ answers (it assumes what the attestor checks; never for a live attestor).

## Run it

From `checkpoint/`, on the build box (`spike/modal-build/mrun`):

```sh
cargo +1.88 test -p ferrata-attestor                      # 33 tests (the lab node's real answers, a node stub)
cargo +1.93.0 build --release -p host -p ferrata-attestor  # host builds the guests
bash attestor/devnet.sh <lab node answers> "$TMPDIR/devnet-n3"   # three attestors, t of n, refusals (a replay:
                                                           # outputs to the second directory, records untouched)
bash attestor/prove.sh                                     # executor, succinct proof, identity_p254
LEZ=<lez rpc> bash attestor/live-node.sh <bundle> <gen> attestor/runs/live-node   # a running copy of the lab node
bash attestor/zone.sh <bundle> attestor/runs/zone 300      # a real four-sequencer LEZ zone (no Docker)
bash attestor/zone-attest.sh attestor/runs/zone "$TMPDIR/zone"   # its four attestors, 3 of 4 collected (a replay:
                                                           # outputs to the second directory, records untouched)
# from the worktree root: the bridge program on the zone, its checkpoint into the vault (stages in the header)
bash checkpoint/attestor/zone-bridge.sh zone e2e/runs/zone-bridge-1
python3 -I scripts/test_scrub_records.py   # what every zone-bridge.sh stage but build and verify ends with (and
                                           # return-leg.sh): the run's files scrubbed of this machine's paths, the
                                           # stage refused if one is left (scripts/scrub_records.py); scratch in $TMPDIR
```

`vectors/lab-node/` holds the lab node's answers trimmed to the node blocks that carry LEZ blocks 186..=202.

## Operating it

- **Its own LEZ node.** `--lez` (the module's `lez`) must be a LEZ node the attestor's operator runs (the
  sequencer's own RPC, or a follower of the zone it trusts as itself), not a shared public RPC: the LEZ node
  decides which inscribed block is final in LEZ, so a lying node can get any Bedrock-final block that links to
  the signed tip signed, and attestors sharing one node share its lies. A node that lags only lowers the tip or
  makes the round refuse.
- **The state file is the guard.** It holds the tip signed last; each round first asks the LEZ node whether it
  still holds that tip final. A LEZ node that lost it (wiped, re-synced on another history, or not synced yet)
  makes every round refuse ("another history", "holds no block"): the attestor stops signing until its operator
  has checked the LEZ node (it must hold the signed tip again, or the zone really forked) and, only then,
  removes the state file. A deleted or lost state file loses the guard: the next round is a first read, which
  signs whatever its LEZ node holds final now.
- **An upstream interface at risk.** The attestor asks the sequencer's JSON-RPC `getBlock`, which LEZ marks for
  removal ("TODO: These functions should be removed after wallet starts using indexer", above it in
  `lez/sequencer/service/rpc/src/lib.rs`, v0.4.0-rc3 line 59). When it goes, the attestor must ask the
  indexer's `getBlockById`, which serves the block as JSON fields (`indexer_service_protocol::Block`: `header`,
  `body`, `bedrock_status`), not as base64 `borsh(Block)`: the attestor would encode it back to borsh before comparing it
  with the inscribed bytes (not built here).

## Several sequencers

**A real zone of four** (`zone.sh`, `runs/zone/`, [RESULTS.md](RESULTS.md) §7): upstream's own four-sequencer
devnet configuration of LEZ v0.4.0-rc1 (one genesis staking the four channel keys), run with the lab bundle's
`sequencer_service` and Logos Blockchain node on the build box without Docker (gossip by explicit loopback
addresses). The channel accredits the four keys, the turn rotates among them and all four inscribe blocks; the
four sequencers' attestors sign the final tip; `t(4) = 3` of 4 is collected (one sequencer silent), proven,
and accepted by a v5 vault with nMin 4, tMin 3; 2 of 4 is refused. With `SILENT=3`, sequencer 3 stops during
the run: the chain waits out its turn and goes on, and the other three attestors' 3 of 4 is accepted.

**The bridge program on that zone** (`zone-bridge.sh`, [`e2e/runs/zone-bridge-1/`](../../e2e/runs/zone-bridge-1/),
[RESULTS.md](RESULTS.md) §8): the same zone with the final programs deployed by crown-driver's session A, a vault
on a local anvil over the zone's four keys (nMin 4, tMin 3) taking two deposits, crown-driver's session B on the
zone (pairing, registration, imports, a public and a private claim, a public and a private burn; the private ones
proven by LEZ's wallet; the transactions sent to one sequencer and included by whichever held the turn), an
attestor beside each sequencer signing the rising final tip while the zone ran and the last one after it stopped (since
the LEZ check, §10, each attestor asks its own sequencer's LEZ node, so the script takes the last round while the
sequencers still run, every attestor on one tip at or above session B's T1, and reads the node's answers itself);
`realblocks-crown --att` puts three of the four attestations into the checkpoint over the zone's blocks 35..=116,
`checkpoint_b` proves it on the CPU, and the vault accepts it, its burn root and both burns' releases. With
`ZB_ROTATE=1` ([`e2e/runs/zone-bridge-2/`](../../e2e/runs/zone-bridge-2/), [RESULTS.md](RESULTS.md) §9) a fifth key is
staked on LEZ (`sequencer_service stake`) before session B; the committee proposes the five-key set, three of the
four sign it, Bedrock applies it, the fifth sequencer and its attestor join; the checkpoint carries that
`ChannelConfigOp` as it was inscribed and four of the five attestations, and the vault accepts it.

**A stand-in key set over the lab's chain** (`devnet.sh`, `runs/devnet-n3/`): the lab's chain has one
sequencer, so its run uses `K = {the lab's key (the real producer of T), k1, k2}` with seeded k1, k2 for
sequencers that do not run, and gives the node's channel answer the same keys; it carries the cases a running
zone did not produce here: another deployment's certificate, an uncertified key, an attestor behind `T`,
attestations of a descendant of `T`, and a key rotation in the proof (with a stand-in posting timeframe).

## Not done here

- `ferrata_attestor` as a Logos module is built (`modules/ferrata_attestor_module`, the crates staged into it
  by `modules/scripts/stage.sh`), and run beside each sequencer of a running zone of four
  (`modules/scripts/attestor-zone-live.sh`, one Logos Core per sequencer, `zone.sh`'s `ZONE_DURING` and
  `ZONE_AFTER` hooks); its key is a file, not `keystore_module`'s.
- Publishing attestations (a relay, a watcher feed); the collector reads files.
- Key rotations inside a proof: the collector checks against the set of the input's last rotation; the rotation
  itself comes from the node's blocks only in `realblocks-crown --att` (§9), not in `base`.
