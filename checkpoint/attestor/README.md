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
2. **Reads its node's final tier** (`attest --node URL --save DIR`, or `--answers DIR` for saved answers of
   the same endpoints): `/cryptarchia/info` (the last irreversible block's slot), `/channel/<id>` (the live
   key set `K` and posting timeframe), `/cryptarchia/blocks` (the node's immutable blocks). It keeps the
   channel's inscriptions in blocks at or below the LIB slot and checks each as the checkpoint program does:
   the canonical transaction encoding reproduces the node's hash, the inscription is well framed and its
   signer's `verify_strict` signature covers the transaction hash, the LEZ block parses strictly and
   re-hashes to the hash it carries, consecutive blocks link by id, `prev_block_hash` and the inscription's
   `parent` message. The final tip is the end of that linked run; its signer must be in `K`.
3. **Signs** the final-tier message v2 over `(deployment, keySetHash(K), tip, finalMsgId)` with the
   certified attestation key, after checking that its certificate certifies that key for this deployment,
   that its channel key is in `K` (a certificate dies with its key), and, with `--state`, that the tip does
   not contradict what it signed before (a lower tip, or another hash at a height it signed: it never signs
   two histories).
4. **Collects** (`collect`): keeps the published attestations whose channel key is in the parent state's
   key set, whose certificate and signature verify for the input's deployment and key set, and which repeat
   no key; refuses below `t(n)`; runs the rule natively and writes the input and its journal. The program
   checks everything again; the collector only avoids proving an input it would refuse.

```text
ferrata_attestor keygen --out KEY [--seed-label L]
ferrata_attestor certify --channel-key KEY --att-pub HEX --deployment DEP --out CERT
ferrata_attestor attest --deployment DEP --cert CERT --att-key KEY (--node URL --save DIR | --answers DIR) [--state F] --out ATT
ferrata_attestor verify --deployment DEP --att ATT...
ferrata_attestor collect --base IN --att ATT... --out IN2 --meta JSON --name NAME
```

Also `pubkey`, `deployment` (the deployment a `checkpoint_b` input serves), `assemble` (an unchecked input,
for refusals), `upgrade` (an input recorded before inputs carried `T`'s inscription) and `rekey` (devnet
set-up). `src/bin/attestor-refusals.rs` writes inputs the program must refuse; it is a test tool.

## Run it

From `checkpoint/`, on the build box (`spike/modal-build/mrun`):

```sh
cargo +1.88 test -p ferrata-attestor                      # 12 tests over the lab node's real answers
cargo +1.93.0 build --release -p host -p ferrata-attestor  # host builds the guests
bash attestor/devnet.sh <lab node answers> attestor/runs/devnet-n3   # three attestors, t of n, refusals
bash attestor/prove.sh                                     # executor, succinct proof, identity_p254
bash attestor/live-node.sh <bundle> <gen> attestor/runs/live-node   # against a running copy of the lab node
bash attestor/zone.sh <bundle> attestor/runs/zone 300      # a real four-sequencer LEZ zone (no Docker)
bash attestor/zone-attest.sh attestor/runs/zone            # its four attestors, 3 of 4 collected
# from the worktree root: the bridge program on the zone, its checkpoint into the vault (stages in the header)
bash checkpoint/attestor/zone-bridge.sh zone e2e/runs/zone-bridge-1
```

`vectors/lab-node/` holds the lab node's answers trimmed to the node blocks that carry LEZ blocks 186..=202.

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
attestor beside each sequencer signing the rising final tip while the zone ran and the last one after it stopped;
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
