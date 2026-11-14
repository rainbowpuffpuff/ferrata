# Logos's verified proxy on Sepolia, beside Ferrata's modules (14 November 2026)

**Where:** the Modal build box (`spike/modal-build/mrun`, one of the owner's Modal profiles: gVisor, x86-64, up to 16
physical cores), 14 Nov 2026 13:05–14:07 UTC. **Reads only:** every request went to public Sepolia endpoints without an
API key (execution: `rpc.sepolia.ethpandaops.io`, `ethereum-sepolia-rpc.publicnode.com`; beacon:
`lodestar-sepolia.chainsafe.io`, `unstable.sepolia.beacon-api.nimbus.team`, `ethereum-sepolia-beacon-api.publicnode.com`);
nothing was signed or sent to any chain. The LEZ side (the updater's instance) is the in-process LEZ chain in dev mode.

## What was built

| | revision | how | result |
|---|---|---|---|
| `libverifproxy` (the proxy's Nim library) | nimbus-eth1 **v0.4.4** `7219184` (with submodules), Nim 2.2.12 | from source, `nix build …/logos-verified-proxy-module/fa98479#libverifproxy` | **828 s**, `libverifproxy.a` 55,809,754 B, `nbs-nim-2.2.12` |
| `verified_proxy_module` 0.2.0 | logos-verified-proxy-module `fa98479` | its own lock (logos-module-builder `a57dc7f`) | 184 s, lgx `59f27092…` |
| `beacon_api_module` 0.2.0 | logos-evm-beacon-api-module `aca138e` | its own lock (also builder `a57dc7f`: the proxy's generated client is the one this module answers) | 74 s, lgx `90d79436…` |
| `eth_rpc_module` 0.2.0 | logos-evm-eth-rpc-module `6965f32` | builder `5f74cdb`, its two optional dependencies called by name (`scripts/eth_rpc_by_name.py`, as logos-io.sh) | 75 s, lgx `ba29ec02…` |
| `ferrata_attest_module`, `ferrata_updater_module`, `logosctl` `66d098d` | this tree | `check.sh` (dev packages; every method called in Logos Core and compared with the native call) | 1,746 s; 0 step and 0 check failures; native tests: attest calls 20, updater calls 3, `ferrata-updater` lib 5 and config 6 |

`build/`: [`steps.md`](build/steps.md), [`lgx-sha256.txt`](build/lgx-sha256.txt), [`check-summary.md`](build/check-summary.md),
[`updater-tests.txt`](build/updater-tests.txt).

## What the session showed (`session/`, 48 of 48 checks: [`summary.md`](session/summary.md))

One headless Logos Core (logosctl `66d098d`) with the five modules loaded (`session/rep/modules-loaded.json`):

1. **Trusted block root** ([`trusted-root.json`](session/trusted-root.json)): Lodestar's finalized header
   `0xa9cb778f…c361`, slot 11,577,536 (epoch 361,798); publicnode holds the same root canonical at that slot and has
   finalized epoch 361,798; Nimbus's header of that root is at the same slot.
2. **The proxy synced from it.** `verified_proxy_module` configured for `sepolia` with that root, execution and archive URL
   ethpandaops (it keeps every proof), its beacon providers taken from `beacon_api_module` (`beaconSource: module`:
   Lodestar and Nimbus, `status().beacon`); `start` returned after the light client's bootstrap was verified (1.4 s; the
   daemon log: "LC bootstrap verified … trustedBlockRoot=a9cb778f"). Four status samples 40 s apart: the verified head
   12,133,577 → 12,133,580 → 12,133,584 while ethpandaops's own block number read 12,133,579, 12,133,583, 12,133,586
   (2–3 blocks behind: the optimistic head), `lastSyncedAt` advancing every sample, state `running`, no sync or heartbeat
   failure; 54 calls, 4 of them failed (the four `eth_getProof` below). `libraryVersion` = nimbus-eth1 `721918411e`.
3. **`eth_rpc_module` with Sepolia in `required` mode** (`verified_proxy_status`: `required`, usable, "Verified."):
   - `fetch_log_proof` (ferrata_attest_module, `eth: {via: eth_rpc_module}`) of the live runs' four vault deposits —
     `crown-live-3` blocks 12,119,421 and 12,119,492, `crown-live-1` blocks 12,111,477 and 12,111,574 — each through the
     proxy (`eth_getBlockByNumber`, `eth_getBlockReceipts`: route `proxied`, i.e. answered by `verified_proxy_module`, which
     checks the header's hash and that it is canonical from its light client's anchor through EIP-2935, and the receipts
     against `receiptsRoot`); each log is the run's deposit (transaction, block, status, the vault, as the run's manifest
     records it); each reply equal field for field (block hash, receipts root, 158 / 135 / 479 / 117 receipts, the
     receipt proof, the log) to the same call with the mode `off`, and each block hash equal to publicnode's.
   - The four capture blocks' headers (12,119,428, 12,119,519, 12,111,509, 12,111,602) through the proxy: hash and state
     root equal to the captures' records; the vault's six slots at each (`eth_getStorageAt`, route **`verified`**: the
     proxy checked an account proof and a storage proof against the verified state root) **24 of 24 equal** to the
     captures' values.
   - **`eth_getProof` refused** in required mode, four times: `verified proxy: eth_getProof: bad request: unknown method`.
     With the mode `off` the same `eth_getProof` equals, every field, the proof each run used (4 of 4).
   - `verify_checkpoint` (the crown runs' checkpoint `0x3b901f5e…ccf7c`) with its execution check through `eth_rpc_module`
     (`execution: {via: eth_rpc_module, chain_id}`): verdict **ok**, `failed` none; execution **agrees** — block
     12,118,225 `0xceaaa1ba…08c7`, canonical, at or below the proxy's finalized block 12,133,493 — the block 13 November's
     check found through publicnode's plain RPC; the same with the mode `off`, and through the proxy's own JSON-RPC
     endpoint (`httpServer` on 127.0.0.1, read with curl), agree on that block.
4. **Light-client data through `beacon_api_module.lc_request`**: the fixture's bootstrap (Lodestar answered HTTP 500,
   the module failed over to Nimbus), the updates of periods 1411–1413 (Lodestar) and the finality update (Lodestar), each
   body equal to the provider's own JSON answer at the same time; turned into SSZ by the updater's converter
   (`lez/updater/src/lc_json.rs`), **each equal byte for byte** to the SSZ the provider served for the same request
   (bootstrap 25,472 B, three updates of 26,424 B, the finality update 1,448 B), and the bootstrap and the period-1411
   update to the 12 November fixture's files ([`lc-ssz-compare.json`](session/lc-ssz-compare.json)).
5. **`ferrata_updater_module` on it**: `run` with both sources through `beacon_api_module` (one asking Lodestar first, one
   Nimbus; `min_agreeing` 2), on the in-process LEZ chain (dev mode), from the fixture's checkpoint (period 1411): up to
   date after **22 rounds (24.3 s)**, **21 transactions applied, none refused** (`init` and its two adoption parts; the
   updates of periods 1411, 1412 and 1413, five steps each; a finality update, three steps), no source refused; the instance's
   finalized header, slot 11,577,568 `0x40dffa33…224b`, is the block publicnode holds canonical at that slot.

## `eth_getProof`: why the proxy does not verify it

nimbus-eth1 v0.4.4, the library under the module (and its `master` of 14 Nov, `87dcde6`, unchanged):
`nimbus_verified_proxy/engine/rpc_frontend.nim` 589–606 defines `frontend.eth_getProof` under the comment `# pass-forward`:
it asks the execution backend and returns the answer **without checking it** against any verified header; the C library's
dispatch (`library/c_frontend.nim` 894–1227), which the module calls, has no `eth_getProof` case and answers `unknown method`
(1226–1227: the refusal measured above); the standalone binary's server (`json_rpc_frontend.nim`) does not register it
either. What the proxy does verify is the **values**: `eth_getBalance`, `eth_getCode`, `eth_getTransactionCount` and
`eth_getStorageAt` fetch an `eth_getProof` themselves and check it (`engine/accounts.nim`: `getAccountFromProof`, an MPT
proof of the account against the verified header's state root, then `getStorageFromProof` per slot). So no patch "exposes"
a verified `eth_getProof`: dispatching the existing frontend proc would expose an unverified pass-through. A verified one
would be new code upstream (resolve the block tag to a verified header, run those two checks on the response, return it).
Ferrata does not need it: the programs check every storage proof against the state root of a record the light client
verified; the session shows the proofs the runs used are the ones the endpoint serves, and the proxy's verified values of
the same slots equal them.

## What failed on the way (all in `build/steps.md`)

- Session run 1: the session script did not set `TMPDIR` to the short `LOGOS_TMPDIR`, so the long-named modules' sockets
  exceeded 107 bytes and their host processes died at load (signal 11 after "failed to listen", the trap `check.sh`
  documents); `beacon_api_module` and `eth_rpc_module` loaded. Fixed (`export TMPDIR`).
- Session run 2: 46 of 48 checks passed; the updater's configuration lacked `[policy]` fields (`max_lag_slots`), and two
  checks of the summary read fields the proxy names otherwise. Fixed; run 3 is the one recorded here.

## Not shown

- The verified proxy on mainnet or Hoodi (only Sepolia was read), or under load: one session of about four minutes.
- Independence of sources: the trusted root comes from Lodestar, cross-checked against publicnode and Nimbus; the
  proxy's beacon providers and the updater's sources are Lodestar and Nimbus too.
- The updater on a LEZ sequencer through `beacon_api_module` (it ran on the in-process chain); the other modules' reads
  (the bridge module's `import_root`, the relayer, the monitor) through the proxy; any Ethereum transaction through
  Logos Core.
- `eth_rpc_module` labels only five methods `verified` (`eth_getBalance`, `eth_getTransactionCount`, `eth_getCode`,
  `eth_getStorageAt`, `eth_call`); the blocks and receipts above are labelled `proxied`, and what makes them verified is
  the proxy's code (`engine/blocks.nim` 371–422, `engine/receipts.nim` 49–80 at v0.4.4), not the label.

## Re-check

On the build box (or any Linux x86-64 host with Nix, cargo and the RISC Zero toolchain of `lez/`), from the repository:

```sh
LOGOS_TMPDIR=/var/tmp/vp modules/scripts/verified-proxy.sh build-logos OUT     # libverifproxy, the proxy, beacon_api_module, eth_rpc_module
LOGOS_TMPDIR=/var/tmp/vp modules/scripts/verified-proxy.sh build-ferrata OUT   # the updater's tests, check.sh for the two modules, logosctl
LOGOS_TMPDIR=/var/tmp/vp modules/scripts/verified-proxy.sh run OUT             # the session on public Sepolia (reads only); OUT/session/summary.md
python3 -I modules/scripts/verified_proxy.py summary . modules/results/verified-proxy-2026-11-14/session   # these records: 48 checks
cd lez && cargo test --locked -p ferrata-updater --lib lc_json                  # the converter against recorded provider answers
```

`verified-proxy.sh build-logos-graft OUT` builds the proxy and `beacon_api_module` with builder `5f74cdb` instead (not
needed here: the own-lock builds loaded beside logosctl `66d098d`).
