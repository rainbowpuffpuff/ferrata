# `ferrata_attestor`

The signer of finality rule (B) (SPEC-v3 D7 and D13; `docs/design/bridge.md` §8.3) that each LEZ sequencer
runs beside its Logos Blockchain node, and the collector that puts `t` of `n` of its attestations into a
`checkpoint_b` input. The signed bytes (the certificate, the final-tier message v2, the key-set hash) are
`crates/attestor-core`'s; this crate is the program around them. Results: [RESULTS.md](RESULTS.md).

Not audited. A prototype: a command-line program, not yet the Logos module the proposal names (below).

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
cargo +1.88 test -p ferrata-attestor                      # 10 tests over the lab node's real answers
cargo +1.93.0 build --release -p host -p ferrata-attestor  # host builds the guests
bash attestor/devnet.sh <lab node answers> attestor/runs/devnet-n3   # three attestors, t of n, refusals
bash attestor/prove.sh                                     # executor, succinct proof, identity_p254
bash attestor/live-node.sh <bundle> <gen> attestor/runs/live-node   # against a running copy of the lab node
```

`vectors/lab-node/` holds the lab node's answers trimmed to the node blocks that carry LEZ blocks 186..=202.

## Several sequencers: what is real and what stands in

The lab's LEZ v0.4.0-rc1 chain has one sequencer (`n = 1`). LEZ v0.4.0-rc1 can run a committee
(`integration_tests/tests/multi_sequencer.rs`, `channel_config_threshold.rs` upstream), but its fixtures
start Bedrock under Docker and stake the sequencers at genesis with LEZ's own signing code; the build box
has no Docker. The checkpoint program counts only distinct channel keys of `K`, and `t(3) = 2` needs
`n >= 3`, so the devnet run uses a **stand-in key set** `K = {the lab's key, k1, k2}`: the lab's key is the
real producer of `T` (its real inscription is checked in the proof); `k1`, `k2` are seeded throwaway
channel keys of two sequencers that do not run, and the node's channel answer is given the same three keys.
Real: the 17 blocks 186..=202 and `T`'s inscription as the node holds them, the whole final tier
(blocks 1..=202) each attestor checks before signing, and the lab key's certificate (from the real-blocks
run). A zone of three running sequencers with key rotation is still to do (proposal B1).

## Not done here

- `ferrata_attestor` as a Logos module: the module builder compiles only the module's own crate directory
  (`modules/README.md`), so the module needs `attestor-core` and the checkpoint core's parsers vendored into
  it; a cold module build took 47 minutes on the box (`modules/RESULTS.md`).
- Publishing attestations (a relay, a watcher feed); the collector reads files.
- Key rotations inside a proof: the collector pre-checks against the parent's key set only.
