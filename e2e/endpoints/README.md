# e2e/endpoints: the mini-app switching its endpoints inside Logos Core (RFP-022 U.9)

**Prototype. Not audited.** RFP-022 U.9 asks that the UI let users change the Ethereum RPC and the LEZ sequencer or
zone. Inside Logos the mini-app's view may not use the network (Basecamp's sandbox refuses it): its calls go to
`ferrata_bridge_module`, and the connections are held by other modules. So the view's "Apply and test"
(`modules/ferrata_bridge_ui/qml/Main.qml`, `settings.apply`) sends `wallet set_endpoints {eth_rpc, sequencer}` to the
module, and the module switches each endpoint where its connection is held:

| Endpoint | Held by | How it is switched |
|---|---|---|
| LEZ sequencer | the chain service, `seqnet-chain` (`cli/src/bin/seqnet_chain.rs`): **a stand-in** for Logos Core's own LEZ module, not a Logos module | `module.set_endpoints`: the new sequencer read first with a client of its own (it answers; its zone holds the session's configuration: `config(cid)` of the session's bridge program, whose `BridgeConfig` hashes to `cid` and names the session's vault and chain; its `getChannelId` is the channel that configuration names); then the LEZ wallet opened again on it |
| Ethereum RPC, the session's `eth` path | Logos Core's `eth_rpc_module`: its endpoint for the session's chain (`chains.json`, the device's own: every Logos app that reads that chain uses it) | the new endpoint read first through `eth_rpc_module.raw_rpc_url` (nothing stored: the chain id, the vault's code, its `lezConfigId`, and the block the endpoint in use holds final); then `patch_chain_endpoint` (its proxy, verified-proxy mode and timeouts unchanged); then the vault and that block read again through `eth_rpc_module`'s own route, for the session's chain |
| Ethereum RPC, the plain path | the chain service | `module.set_endpoints`, read first as above with a provider of its own |

Both or neither: every new endpoint is read before anything switches, so a refused one keeps both endpoints in use;
when the reading through `eth_rpc_module`'s own route fails after its switch (on a chain set to `required`, the
proof-backed reads wait for it), its endpoint and a sequencer switched with it are put back. A refusal names the
endpoint, the reason and the endpoints kept. While a private claim or burn is being proven (a job), the module refuses.
While the session's LEZ goes through Logos Core's `lez_core` (`wallet lez`), whose wallet holds its own connection (the
sequencer its wallet config names, `sequencer.sequencer_addr`; `lez_core` has no method that sets it), the switch moves
it too: after the chain service has read the new sequencer and switched, `lez_core` opens the session's wallet again on
a copy of that config naming the new sequencer (`<config>.seq-<8 hex>.json`, mode 0600; the config in use is never
written), and the switch is kept only if `lez_core` then names the new sequencer and reads a block through it; otherwise
`lez_core` is opened again on the config it had and the chain service's endpoints are put back (both or neither;
`calls/src/lez.rs` `switch_sequencer`, `put_back`). Scenario Z of `run.sh live` runs it live (below). The view shows the endpoints in use (`info`, read from each owner) and
who holds each. The Ethereum and LEZ paths belong to the deployment the session was opened on: opened on another one
(another chain service or configuration), the session drops them (`open`'s `paths_reset`), so that no switch writes
another chain's record of `eth_rpc_module`.

**What the readings check: the endpoint's configuration, not its honesty.** An Ethereum endpoint is refused when it
serves another chain, lacks the vault, names another configuration, or (when the endpoint in use answers) holds another
block where the endpoint in use holds its final block: another network or a fork made before that block. A fork that
shares the history up to that block (the run's anvil fork is one) is accepted, and so is an endpoint that answers all of
it falsely. A sequencer is refused when its zone does not hold the configuration or names another channel; two zones on
one channel id (upstream's default `0x0101…01`, which every devnet here uses) are told apart only by the configuration's
presence. What a node answers is not proven by these readings (the bridge's own checks on LEZ and in the vault are).

## The tests

| Test | Where | What |
|---|---|---|
| `modules/ferrata_bridge_module/rust-lib/calls/tests/endpoints.rs` | CI (`modules-resolve`), the build box | the module against stand-ins on 127.0.0.1: a chain service with seqnet-chain's contract (a set of endpoints it accepts: its own readings are not run here), six Ethereum endpoints (two of the deployment, another chain, no vault, another configuration, a fork) and a closed port, `eth_rpc_module`'s registry and routes (a `required` chain too), `lez_core`, a running job, a second deployment on another chain |
| `cli/src/bin/seqnet_chain.rs`'s tests | CI (`roundtrip`) | the chain service compiles with its feature; its check of a candidate sequencer's configuration (the right one; none; not a configuration; another program, configuration, vault or chain) |
| `run.sh live NAME` | the build box | the view, headless (Qt Quick Test, `modules/ferrata_bridge_ui/qmltest-logos/tst_endpoints_logos.qml`), with `ferrata_bridge_module` and `eth_rpc_module` loaded in one headless Logos Core behind it; live endpoints (below); each owner's record read independently after every step (`ops.sh`) |

The live run:

1. **A LEZ zone of four sequencers** (LEZ v0.4.0-rc1, upstream's four-sequencer devnet on one Logos Blockchain node,
   as `checkpoint/attestor/zone-bridge.sh` sets it up), the attestation program and the bridge program B1
   ([`docs/image-ids.md`](../../docs/image-ids.md)) deployed by `crown-driver crown-a` (STAND-IN beacon chain), a local
   anvil (chain id 11155111, no fork) with the v5 vault paired with that configuration, one deposit, and the operator's
   pairing, registration and import (`crown-driver crown-b --operator-only`). Every one of the four sequencers serves
   the zone and so the configuration. A second anvil forked from the first is a second live endpoint of the chain.
2. **Scenario L** (the plain path; local): the view switches the sequencer between sequencers 0, 1 and 2 and the
   Ethereum RPC between the two anvils, and is refused, keeping both endpoints: a closed sequencer port, a closed
   Ethereum port, a good sequencer with a bad RPC (neither switches); with `U9_PUBLIC=1` also our public LEZ network
   (another zone), Sepolia's public RPC (the chain, without the vault) and Hoodi's (another chain), read only.
3. **Scenario E** (`U9_PUBLIC=1`, read only; it needs our test network's RPC URL, which the repository does not hold:
   ask the owner for it): the live deployment, the vault on Sepolia
   (`0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787`, configuration `0x3773b588…67cd`) and its configuration on our public LEZ
   network (its RPC URL given as `U9_LIVE_LEZ_URL`: its host names a person, so the repository does not hold it and the
   records write it `public-lez.example`), through a second chain service; Ethereum through `eth_rpc_module`. The
   view switches `eth_rpc_module`'s endpoint for chain 11155111 from publicnode's Sepolia RPC to ethpandaops' and back,
   and is refused, keeping both endpoints: Hoodi's RPC, a closed port, a local sequencer (its zone lacks the
   configuration). Nothing is sent to any network: the session only reads (no deposit, claim, burn or release).

## Stand-ins, named

The chain service stands in for Logos Core's own LEZ module on scenarios L and E (scenario Z switches `lez_core`'s wallet
too, `lez_core` built from its source with our patch, `modules/patches/lez_core-*.patch`); on scenario L
anvil is Ethereum and a committee the driver holds finalizes its blocks for LEZ; leader proofs are in dev mode (as every
LEZ devnet here). The view runs in a Qt Quick Test runner over a headless Logos Core, not in Basecamp (no sandbox, no
20 s cut of a call). Real: the four sequencers and their zone, the programs, the vault, `eth_rpc_module` (built from its
source at `6965f32` with our compile patch for builder `5f74cdb`'s generated clients,
`modules/patches/eth_rpc_module-6965f32-generated-clients.patch`; `modules/scripts/send-stack.sh`),
`ferrata_bridge_module` and Logos Core (`logosctl`), the view's code, and in scenario E the public Sepolia RPCs and our
public LEZ network.

## Commands

```sh
M=../modal-build   # from the worktree root
$M/mrun --profile <yours> --timeout 9000 -- 'bash e2e/endpoints/run.sh build'
$M/mrun --profile <yours> --timeout 7200 --put <bundle.tar.zst> --put <ferrata_bridge.bin> \
  -- 'U9_BUNDLE=<bundle.tar.zst> U9_BRIDGE_BIN=<ferrata_bridge.bin> U9_COMMIT=<commit> bash e2e/endpoints/run.sh live N'
# scenario E too (read only), with our test network's RPC URL from the owner:
#   ... -- 'U9_PUBLIC=1 U9_LIVE_LEZ_URL=<from the owner> U9_COMMIT=<commit> bash e2e/endpoints/run.sh live N'
```

`U9_BUNDLE` and `U9_BRIDGE_BIN` name the inputs (a crown LEZ bundle, `lez/crown/build.sh`'s; the bridge program B1,
sha256 `537cbe7a…`, which `scripts/reproduce-image-ids.sh` rebuilds). Records: `e2e/runs/endpoints-NAME/` (`summary.md`
first: every `set_endpoints` the view sent and its answer; `calls.jsonl`, `ops.jsonl`, `qmltest.txt`, the chain
services' call logs, `versions.txt` (the build's artefacts and `logosctl`'s version), `screenshots/`). The box's copy of
the worktree is not a git checkout, so a run names its commit only when given `U9_COMMIT`.
