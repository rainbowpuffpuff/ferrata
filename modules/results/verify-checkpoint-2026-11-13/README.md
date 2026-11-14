# `verify_checkpoint` with its provider checks, live on Sepolia's public providers (13 November 2026)

`ferrata_attest_module`'s `verify_checkpoint` (`../../ferrata_attest_module/rust-lib/calls/src/providers.rs`) checked the
crown run's Sepolia checkpoint (`crates/ferrata-spectests/fixtures/sepolia-2026-11-12`: block root `0x3b901f5e…ccf7c`,
slot 11,562,112) against three beacon API providers run by three operators, a checkpoint-sync server and an execution
JSON-RPC endpoint. Everything was read only (HTTP GETs, and `eth_chainId`, `eth_getBlockByHash`, `eth_getBlockByNumber`),
with User-Agent `ferrata-verify-checkpoint`; nothing was sent to any chain.

| | |
|---|---|
| where | Modal build box, 13 Nov 2026 07:25:20Z–07:25:32Z, the `call` example built from the tree of commit `e8d6d55` (`run.txt`) |
| request | [`verify-checkpoint-sepolia.json`](../../ferrata_attest_module/rust-lib/calls/tests/fixtures/verify-checkpoint-sepolia.json) with the fixture's `bootstrap.ssz` as hex: the instance configuration of the crown zones (layout Gloas, `ws_period_epochs` 3,532), `min_providers` 3 |
| reply | [`reply.json`](reply.json) (every provider's requests included), [`summary.json`](summary.json) |
| record | every request and response (22), [`verify-checkpoint-sepolia.record.json`](../../ferrata_attest_module/rust-lib/calls/tests/fixtures/verify-checkpoint-sepolia.record.json): the tests replay it |
| mode | no proof: the checks run natively (blst for `KeyValidate`) |

## What it showed

| check | result |
|---|---|
| verdict | **ok** (`failed`: none) |
| instance id | `0xa5fb2591…b2b5`, equal to the expected one: the instance of the crown zones (`lez/updater/results/live-sepolia/zone-2/export/instances.tsv`) |
| bootstrap | accepted by the program's `Init` checks (code 0): header root `0x3b901f5e…ccf7c`, slot 11,562,112 |
| committee | the full `KeyValidate` on all 512 keys (each in the prime-order subgroup) and their sum equal to the aggregate key: accepted |
| Lodestar (`lodestar-sepolia.chainsafe.io`, ChainSafe) | **agrees**: the root canonical by root and at slot 11,562,112; finalized slot 11,568,352; genesis and spec (`DEPOSIT_CHAIN_ID` 11155111, preset, the eight scheduled forks) those of the configuration |
| publicnode (`ethereum-sepolia-beacon-api.publicnode.com`, Allnodes) | **agrees**, the same |
| Nimbus (`unstable.sepolia.beacon-api.nimbus.team`, Status) | **agrees**, the same |
| checkpointz (`checkpoint-sync.sepolia.ethpandaops.io`) | **unavailable**: it serves genesis and spec (equal; its spec has no fork schedule, noted) but no headers API (`/eth/v1/beacon/headers/finalized`: 404); not counted |
| agreeing hosts | **3 of 3 needed** |
| execution (`ethereum-sepolia-rpc.publicnode.com`, in the place of `verified_proxy_module`) | **agrees**: `eth_chainId` 11155111; the bootstrap header's execution block hash `0xceaaa1ba…08c7` is block 12,118,225, canonical at that number, at or below the endpoint's finalized block (12,124,386) |

The tests of `ferrata-attest-calls` replay this record: [`tests.txt`](tests.txt), **18 passed** (the eight from before
and ten new ones: the three providers agree; another block at the slot, a provider that has not
finalized the slot, another chain's spec or genesis, a non-canonical block each fail the check; an unreachable provider
is not counted and three are needed; two URLs of one host count once; an execution endpoint on another chain, or
without the block, fails; no providers, nothing fetched (row 59's behaviour); `KeyValidate` refuses a committee with a
key outside the subgroup).

## Re-check

```sh
modules/scripts/stage.sh ferrata_attest_module      # the staged crates; and the builder's SDK source, as check.sh's sdk step:
nix build github:logos-co/logos-module-builder/5f74cdb3cecacde054c68d72fcc0d2cfe2195900#rust-sdk-src -o modules/ferrata_attest_module/logos-rust-sdk-src
cd modules/ferrata_attest_module/rust-lib && cargo test --release --locked -p ferrata-attest-calls   # replays the record
# the live call (reads public endpoints):
F=calls/tests/fixtures; cargo build --release --locked -p ferrata-attest-calls --example call
python3 -I -c 'import json,sys; r=json.load(open(sys.argv[1])); r.pop("_comment"); r["bootstrap"]="0x"+open(sys.argv[2],"rb").read().hex(); print(json.dumps(r))' \
  $F/verify-checkpoint-sepolia.json ../../../crates/ferrata-spectests/fixtures/sepolia-2026-11-12/bootstrap.ssz > req.json
target/release/examples/call verify_checkpoint req.json        # FERRATA_HTTP_RECORD=FILE also records every request
```

## Not shown

- `verified_proxy_module` itself. Its local JSON-RPC server (`httpServer`, `127.0.0.1:8545`) is the endpoint the
  execution check is meant for; here publicnode's unverified RPC stood in. The module reads every endpoint with
  `curl`, not through Logos Core's own modules.
- Independence. The method counts hosts; who runs them is the caller's judgement (here three operators and three
  client implementations, as far as their names say).
- Through Logos Core. The method runs natively here; the module's other calls are compared through both Logos Cores by
  `modules/scripts/check.sh`.

## The upstream pull request (prepared, not opened)

**Repository**: logos-co/logos-verified-proxy-module (at fa98479, v0.2.0). **State**: prepared, not opened.

**Title**: Add `verifyTrustedBlockRoot()`: check a trusted block root against several independent beacon providers

**Body**:

## Why

The README asks operators to take `trustedBlockRoot` "from a source you trust, not from the same provider you are
about to verify", and says that `fetchFinalizedRoot()` "anchors nothing". Nothing in the module helps an operator
check a root they have been handed, so in practice the root comes from one beacon node (the executable doctest takes it
from Lodestar and configures the proxy with it). The proxy's whole guarantee rests on that value: with a false root,
every later answer is "verified" against the wrong chain.

This pull request adds that check. Before `configure()` and `start()`, the module asks several independent beacon
providers whether the root is a canonical, finalized block of the configured network. The same check serves every
Logos component that starts a light client from a checkpoint: this proxy, the light-client instances of RFP-021 (its
requirement F.9: "checkpoint independently checkable"), and any later light client. Basecamp sandboxes `ui_qml` plugins
away from the network (README), so a core module is the place for it.

## What it adds

`verifyTrustedBlockRoot(root, beaconUrls, minAgreeing)` returns `{ ok, agreeing, minAgreeing, providers: [...] }`.

For each beacon URL (`http://` or `https://`, through the existing `beacon_client::httpGet`: same User-Agent, timeout and
redirect rules) it makes five GET requests:

| Request | What must hold |
|---|---|
| `/eth/v1/beacon/genesis` | `genesis_validators_root`, `genesis_time` and `genesis_fork_version` are the configured network's |
| `/eth/v1/config/spec` | `DEPOSIT_CHAIN_ID` equals the network's chain id (`expectedChainId()`). This ties the execution chain id to the beacon chain, which a light client cannot check by itself |
| `/eth/v1/beacon/headers/{root}` | the block exists and has `canonical: true`; the response gives its slot |
| `/eth/v1/beacon/headers/{slot}` | the canonical block at that slot is `root` |
| `/eth/v1/beacon/headers/finalized` | the provider's finalized slot is at or after the root's slot |

Each provider gets one of three verdicts:

- `agrees`: every check passed;
- `disagrees`: an answer contradicts the root (another block at the slot, not canonical, not finalized, another
  network's genesis or chain id);
- `unavailable`: a transport failure, a status other than 200, or no headers API (checkpoint-sync servers serve only
  genesis and spec).

Providers are counted by host. `ok` needs at least `minAgreeing` agreeing hosts (3 by default) and no disagreement: a
provider that contradicts the others raises an alarm and is not outvoted.

When the proxy runs, the result also carries the proxy's own view of the root's execution block. It reads the block's
execution hash from `GET /eth/v2/beacon/blocks/{root}`, then asks the verified path whether that block is canonical and
at or below the proxy's finalized block (`eth_getBlockByHash`, `eth_getBlockByNumber`).

## Not in this pull request

- `configure()` does not change: the check is advisory. A later change could let a configuration require it
  (`"verifyTrustedBlockRoot": true`).
- `KeyValidate` of the sync committee's keys. The library verifies the bootstrap's committee branch; validating each
  key stays with the library.

## Tests

- `tests/test_beacon_client.cpp`: the URL of each request (root, slot, finalized, genesis, spec; trailing slashes;
  path prefixes) and each response parser (well formed, missing fields, wrong types, error pages). They are pure
  functions with no network access, like the existing tests of `fetchFinalizedRoot`.
- A verdict table over recorded responses: all providers agree; a provider holds another block at the slot; the block
  is not canonical; the slot is not finalized; a provider is on another network (genesis, or `DEPOSIT_CHAIN_ID`); a
  provider is unreachable; a checkpoint-sync server; two URLs on one host.
- The doctest calls `verifyTrustedBlockRoot` on the root it already fetches, against Lodestar's, publicnode's and
  Nimbus's public Sepolia beacon APIs, before `configure`.

## Reference

The same checks are implemented in Rust as `verify_checkpoint` of Ferrata's `ferrata_attest_module`
(`modules/ferrata_attest_module/rust-lib/calls/src/providers.rs`). Their tests replay the recorded answers of three
Sepolia providers for one checkpoint (`calls/tests/fixtures/`). That method also compares the checkpoint's execution
block with an execution JSON-RPC endpoint, and this module's local server (`httpServer`, `127.0.0.1:8545`) is the
endpoint it is meant for.
