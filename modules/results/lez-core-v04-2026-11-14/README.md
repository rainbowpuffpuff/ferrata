# Logos Core's `lez_core` at its merged v0.4 state, for `ferrata_bridge_module` (14 November 2026)

**Where:** the Modal build box (`spike/modal-build/mrun`, profile ws-5: gVisor, x86-64, up to 16 physical cores),
13–14 November 2026. **Chains:** a LEZ v0.4.0-rc1 devnet started there (the sequencer and Logos Blockchain node of the
crown-live-3 bundle, the final programs of [`docs/image-ids.md`](../../../docs/image-ids.md)) and a local anvil with
FerrataVault v5; nothing was sent to any public network.

## 1. `lez_core` built from its source, as merged and patched (`build/`)

`lez_core` = logos-blockchain/logos-execution-zone-module at **`cc73ebf`** (`main` after #72 and #74 were merged on
13 November at 13:18 and 13:20 UTC): version 0.6.0, LEZ v0.4.0-rc1 `1806e87` and logos-module-builder `5f74cdb`, the
pins of our programs and modules. Built with Nix from its own flake (`e2e/seqnet/run.sh build-lezcore`):

| step | result | seconds |
|---|---|---:|
| `lez_core` as merged (`github:…/cc73ebf#lgx`: LEZ's `core_ffi`, ~1,160 crates, then the module) | ok | 1,289 |
| the same with [`../../patches/lez_core-cc73ebf-authorizes-public-pda.patch`](../../patches/lez_core-cc73ebf-authorizes-public-pda.patch) | ok | 60 |
| its unit tests (its mocks of LEZ's wallet FFI), as merged | **60 passed** | 36 |
| its unit tests, patched (the four new tests among them) | **64 passed** | 38 |
| `ferrata_bridge_module` through `modules/scripts/check.sh` (native tests and calls, the dev `.lgx`, logosctl `66d098d`, loaded in Logos Core) | 0 step, 0 check failures | 241 |

[`build/steps.txt`](build/steps.txt), [`build/lgx-and-logosctl.txt`](build/lgx-and-logosctl.txt) (the three `.lgx`'
SHA-256), [`build/lez_core-unit-tests-upstream.txt`](build/lez_core-unit-tests-upstream.txt),
[`build/lez_core-unit-tests-patched.txt`](build/lez_core-unit-tests-patched.txt), [`build/check-summary.md`](build/check-summary.md).

## 2. The round trip through `ferrata_bridge_module` beside `lez_core` (`e2e/runs/seqnet-lezcore-1/`, dev mode)

`e2e/seqnet/run.sh lezcore lezcore-1` ([`SUMMARY.md`](../../../e2e/runs/seqnet-lezcore-1/SUMMARY.md)): session A (a new
zone, the final programs deployed), the vault on anvil, then, while the zone runs (`hook-lezcore.sh`), a headless Logos Core
(logosctl `66d098d`) with `ferrata_bridge_module` and the **patched** `lez_core` installed from their `.lgx` and loaded;
the user's LEZ wallet (the storage the chain service made from the seed) opened inside `lez_core` (`open`, `force_rpc`:
a client of this zone's sequencer); the module's wallet session routed through it (`wallet lez`); the user's calls in the
mini-app's order with logosctl; the operator's import, the gas drop's stand-in, the checkpoint and the clocks by `ops.sh`.

| What | Through `lez_core` | Beside the plain path (`seqnet-chain`) |
|---|---|---|
| bridge actor states (`get_account_public`) | 85 reads: `config(cid)` 13, `token(cid, ETH)` 13, `root(cid, 1)` 10, the deposit's `nullifier(…)` 1, `holder(…)` 48; 30 of them non-empty (564, 843 and 117 bytes) | **85 of 85 equal**, byte for byte; 0 failed |
| the private holding (`sync_to_block` + `get_account_private`) | 9 reads (synced to LEZ blocks 20, 21, 29, 30 ×3, 31 ×2, 33): no bridge state before the claim, 74 bytes after it (0.001 ETH held), 6 after the burn (none held) | **9 of 9 equal** to the chain service's wallet |
| the private claim (`ClaimPrivately` of leaf 0: the private account and four PDAs) | `send_generic_private_transaction_with_accounts`: executed and proven (dev mode) by LEZ's wallet inside Logos Core, 603 ms; `poll_transaction_status` true | applied in **LEZ block 30**, a private transaction (33,055 B, receipt 16,519 B), not reverted; visible bridge calls `Read`, `Issue` |
| the private burn (`Burn`: the private account and three PDAs) | the same, 413 ms; `poll_transaction_status` true | applied in **LEZ block 31** (32,018 B, receipt 16,033 B), not reverted; visible call `Burn` |
| the release (Ethereum, the plain path) | — | the module's release by the wallet's rule; 0.000997 ETH paid to the fresh address |

The zone: 101 transactions in blocks 1..=33, none reverted; anvil: 15 transactions. Dev mode: the sequencer accepted
RISC Zero dev receipts, and LEZ's wallet inside `lez_core` made fake ones (`RISC0_DEV_MODE=1` in the Logos Core daemon).

**What `lez_core` as merged does with the same calls** (same run):

- the bridge program's PDA as an account (`wallet lez_probe`, before any wallet is open): `lez_core` `cc73ebf` refuses it,
  `accounts_json[0] unknown kind "public_pda"` ([`lez-probe-upstream.json`](../../../e2e/runs/seqnet-lezcore-1/lezcore/lez-probe-upstream.json));
  patched, the entry is accepted and the call stops later at the wallet (none open yet: `wallet FFI error 1`,
  [`lez-probe-patched.json`](../../../e2e/runs/seqnet-lezcore-1/lezcore/lez-probe-patched.json));
- the private claim with every account mentioned as `cc73ebf` mentions them (none authorizing; the patch's
  `"authorizes": false`), on the live zone: **refused**, LEZ's wallet reports `TransactionBuildError(ProgramExitedWithCode
  { code: 252 })`, the bridge program's `WRONG_DESTINATION` ("a private claim not run as its destination"); nothing sent
  ([`lez-report.json`](../../../e2e/runs/seqnet-lezcore-1/lezcore/lez-report.json), `core-logs-tail.txt`). The same claim,
  its accounts authorizing as LEZ's wallet API mentions them, is the one applied in block 30.

## 3. The same with real proofs (`e2e/runs/seqnet-lezcore-2/`, the sequencer verifying every receipt)

`SEQNET_PROOFS=real run.sh lezcore lezcore-2` (14 Nov 10:11–10:29 UTC, [`SUMMARY.md`](../../../e2e/runs/seqnet-lezcore-2/SUMMARY.md)):
the sequencer without dev mode, every private transaction proven for real on the box's CPU (r0vm 3.0.5 of the bundle).

| What | Through `lez_core` | Beside the plain path |
|---|---|---|
| bridge states, private holding | 85 and 9 reads | **85 of 85**, **9 of 9** equal |
| private claim | proven by LEZ's wallet inside Logos Core in **248.7 s**; receipt 239,532 B, transaction 256,068 B | verified and applied by the sequencer in **LEZ block 74**, not reverted; the same sizes as the plain path's claim in `seqnet-miniapp-3` (proven by `seqnet-chain` in 388.1 s) |
| private burn | proven in **228.8 s**; receipt 239,046 B, transaction 255,031 B | applied in **LEZ block 98**; the same sizes as `seqnet-miniapp-3`'s burn (333.9 s there) |
| the claim as `cc73ebf` sends it | refused before proving (`ProgramExitedWithCode { code: 252 }`) | nothing sent |
| release | — | held by the vault's delay, then 0.000997 ETH paid |

235 LEZ transactions in blocks 1..=102 and 15 on anvil, none reverted.

## 4. Logos Core's Ethereum send stack (`send-stack/`)

With logos-module-builder 5f74cdb's own locks (`modules/scripts/send-stack.sh`, `graft_builder_lock.py`):
**`keystore_module` (e83a899) and `signer_manager_module` (bd5c84d) build, install and load together** in a headless
Logos Core (the cause of their earlier failure was the builder override keeping their old code generator,
`modules/LOGOS-CORE-MODULES.md`); **`eth_rpc_module` (6965f32) does not compile** against the new generator's client of
its optional dependency, so `fee_module` and `tx_sender_module`, which require it, were not loaded, and no Ethereum
transaction was signed or sent through the stack. Details: [`send-stack/README.md`](send-stack/README.md).

## Re-check

```sh
cd spike/ferrata-wt-<name>                          # the inputs beside it: spike/tmp/miniapp-sequencer/inputs
../modal-build/mrun --profile ws-5 --put ../tmp/miniapp-sequencer/inputs -- 'bash e2e/seqnet/run.sh build'
../modal-build/mrun --profile ws-5 --timeout 3600 -- 'bash e2e/seqnet/run.sh build-lezcore core'
../modal-build/mrun --profile ws-5 --timeout 3600 -- 'bash e2e/seqnet/run.sh build-lezcore rest'
../modal-build/mrun --profile ws-5 --timeout 5400 -- 'bash e2e/seqnet/run.sh lezcore lezcore-N'
../modal-build/mrun --profile ws-5 --timeout 5400 -- 'SEQNET_PROOFS=real bash e2e/seqnet/run.sh lezcore lezcore-N'
```
