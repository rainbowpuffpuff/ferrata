# e2e/seqnet: results

What ran, where and with what, for the user's private round trip on a running LEZ sequencer ([`README.md`](README.md)).
Every number below is from the run's own records under `e2e/runs/seqnet-NAME/` (each has a `SUMMARY.md`: every
transaction on both chains with who sent it, and what the user saw).

**Where:** the Modal build box (`spike/modal-build/mrun`, our Modal profile: gVisor, Ubuntu 24.04, up to 16 physical
cores), 13 November 2026. **Chains:** a LEZ v0.4.0-rc1 devnet started there (the sequencer and the Logos Blockchain
node of the crown-live-3 bundle, LEZ's local configuration, dev-mode leader proofs) with the final programs of
[`docs/image-ids.md`](../../docs/image-ids.md) (attestation `0x816d1fef…`, bridge `0xccde6171…`, read back from the
zone after their deployment), and a local anvil (chain id 11155111) with FerrataVault v5 in mode (B). Nothing was
sent to any public network.

## 1. Through the `ferrata` tool: `seqnet-cli-1` (dev mode)

[`e2e/runs/seqnet-cli-1/`](../runs/seqnet-cli-1/SUMMARY.md): `run.sh cli cli-1`, 13 Nov 09:05–09:12 UTC (session A
4.5 min; the round trip, from the deposit to the release, 3 min).

| Step | What the user ran | Result |
|---|---|---|
| deposit | `ferrata deposit --denomination 0` (dry run, then `--keystore … --send --yes`; private `/0` by default) | `depositETH` in anvil block 9, leaf 0, 0.001 ETH |
| import (operator) | — | `crown-b --operator-only`: bridge `Init` (LEZ block 23), `Register` ETH (24), `ImportRoot` n=1 (25) |
| fees (stand-in) | — | `seqnet-fund`: 3·10¹² native LEZ to public `/0` (block 26), then 2·10¹² to private `/0` by LEZ's wallet (block 27, private; the private account holds 1,999,994,929,384 after the 5,070,616 fee its proof paid) |
| claim held | `ferrata claim` | refused: `held by your claim delay: … from Ethereum block 609` (the seed's 2 h delay) |
| delay | — | 601 anvil blocks mined (the tool counts 12 s per block); not overridden |
| private claim | `ferrata claim --bridge-program … --send --yes` (defaults) | `ClaimPrivately` executed and proven by LEZ's wallet, applied in LEZ block 28 (transaction `148ad854…`), a privacy-preserving transaction whose visible bridge calls are `Read` and `Issue`; `recover` then shows the deposit **claimed** (from the nullifier account the private transaction named) and **private `/0` holding 0.001 ETH**, decrypted by LEZ's wallet after its sync |
| private burn | `ferrata burn --amount 0.001 --recipient 0xa015…1dAf` (a fresh address; private `/0` by default) | applied in LEZ block 29 (`a4ea74ad…`), private; its note reaches the zone in the clear as a `Burn` call to `token(cid, ETH)` (`lez::visible_calls`), from which `recover` finds the burn from the seed |
| checkpoint (prover, relayer) | — | `realblocks-crown`: the node's final inscriptions of LEZ blocks 1..=30 re-hashed, linked and signature-checked, all 30 accepted; the input over blocks 20..=29 (10 blocks, 30 transactions, 119,217 B); `checkpoint_b` (`0xf8116722…`) executed: journal (544 B) equal to the native one, 5,505,024 padded cycles; **stand-in**: the mock verifier accepts that journal; `ferrata-relayer`: `submitCandidate` (anvil block 612), Δ = 3600 s let pass on anvil's clock, `finalizeAttested` (614) |
| release held | `ferrata release` | refused: `ReleaseDelayNotElapsed()` (the vault's 120 s after `finalizeAttested`) |
| release | `ferrata release --seal … --job-out job.json` (any relayer, the registered relay fee) | the wallet's rule picked the root of checkpoint 0 (1 burn); **stand-in**: the mock accepts the release journal; `ferrata-relayer` sends `release` (anvil block 617): **0.000997 ETH to the fresh address** (0.001 less the checkpoint fee 0.000001 and the relay fee 0.000002, paid to the relayer, who collects its fees with `claimOwed`, 618) |
| recover | `ferrata recover` | `1 deposits … claimed`, `1 burn … released`, `0 holdings` |

On the zone: 97 transactions in blocks 1..=31, none reverted (63 of session A: the two programs' deployment and the
instance's `Init` and adoptions; the operator's 3; the stand-in's 2; the user's 2 private ones). On anvil: 15
transactions, none reverted (the vault's deployment 8, the user's deposit, the stand-in prover's 2 `accept`, the
relayer's `submitCandidate`, `finalizeAttested`, `release` and `claimOwed`).

Dev mode (`SEQNET_PROOFS=dev`): the sequencer was started with `RISC0_DEV_MODE=1` and LEZ's wallet made fake receipts
for the three private transactions (the shield, the claim, the burn).

## 2. Through the mini-app driving `ferrata_bridge_module` inside a headless Logos Core

The view (`modules/ferrata_bridge_ui/qml/Main.qml`, its module transport: every call is the module's `wallet` method)
driven by Qt Quick Test (`modules/ferrata_bridge_ui/qmltest-logos/`), offscreen; its `logos` object is the runner's,
whose `callModule` is `logosctl call ferrata_bridge_module wallet @request` against a headless Logos Core daemon
(logosctl `pre-release-66d098d`, built from logos-logoscore-cli at its pin) into which the module was installed from its
dev `.lgx` (logos-module-builder `5f74cdb`; `modules/scripts/check.sh` first: the module's native tests and calls, the
package built, loaded and called in Logos Core, 0 step and 0 check failures). The module runs each of the view's calls
with its own methods; `seqnet-chain` reads the chains and signs and sends (the stand-in for Logos Core's own modules).
The operator's import, the gas drop's stand-in, the checkpoint and the clocks are `ops.sh`, which the test runs between
the user's actions (the view shows no button for them inside Logos).

### `seqnet-miniapp-1` (13 Nov 09:52–10:05 UTC): the private claim and burn proven for real; the release not reached

[`e2e/runs/seqnet-miniapp-1/`](../runs/seqnet-miniapp-1/SUMMARY.md): 8 of 9 test functions passed (720 s), 17 calls of
the view to the module, 15 screenshots.

| Step | Through the view and the module | Result |
|---|---|---|
| connect, registry | `open` (the host's), `info`, `registry` | the vault's registry before the bridge's first import on LEZ (no LEZ side yet), the LEZ side after it (`token(cid, ETH)` decoded by the module) |
| deposit | Deposit screen on the defaults (private `/0`, 0.001 ETH), the wallet's approval | the module's commitment and `depositETH` call, signed with the user's keystore by `seqnet-chain`: anvil block 9, leaf 0 |
| positions | Positions, recover | `waiting for the root import`, then (after the operator's import) `claimable` |
| private claim | Claim: held by the view's random delay (59 min, waited out on the view's clock), then the private claim on the defaults | `ClaimPrivately` built by the module (the tool's claim with LEZ state), executed and **proven for real** by LEZ's wallet in `seqnet-chain` (316.6 s, receipt 239,532 B, transaction 256,068 B), applied in LEZ block 59; the deposit then `claimed`, private `/0` holding 0.001 ETH |
| private burn | Burn and release: private holder, a fresh recipient, the release delay on (defaults) | the module's note and `Burn` with its four accounts, **proven for real** (289.1 s, receipt 239,046 B), applied in LEZ block 89; holding 0 |
| checkpoint | `ops.sh checkpoint` | `realblocks-crown` over the zone's final blocks (all 90 LEZ blocks' inscriptions accepted; the input over blocks 20..=89: 70 blocks, 150 transactions, 615,855 B), `checkpoint_b` executed (journal equal to the native one, 20,447,232 padded cycles), the stand-in prover's `accept`, the relayer's `submitCandidate` and, Δ later, `finalizeAttested` |
| release | Redeem, refresh | **failed**: the module's `recover` read each published burn's note fields flat and the chain service nests them under `note` (`burns[0].r: missing`), so the view could not list the burn; fixed in the module (56e8973) |

`seqnet-chain` had been started without `RISC0_DEV_MODE`, so LEZ's wallet proved both private transactions for real while
the replies still named them `dev` (the label came from the run's setting; it now comes from the process's environment).
The sequencer ran in dev mode, which still verifies a real receipt as one. 217 transactions in LEZ blocks 1..=91.

### `seqnet-miniapp-2` (13 Nov 10:51–11:13 UTC, `SEQNET_PROOFS=real`): the sequencer verifying real receipts

[`e2e/runs/seqnet-miniapp-2/`](../runs/seqnet-miniapp-2/SUMMARY.md): the same path with the recovery fixed, the
sequencer started **without** dev mode (it verifies every private transaction's receipt) and every private transaction
proven for real by LEZ's wallet on the box's CPU (r0vm 3.0.5 of the bundle): the gas drop's shield (213.2 s), the
mini-app's private claim (369.4 s; receipt 239,532 B, transaction 256,068 B; LEZ block 87) and private burn (332.7 s;
receipt 239,046 B, transaction 255,031 B; LEZ block 121). Then the checkpoint over LEZ blocks 20..=121 (all 122 blocks'
inscriptions accepted; input 865,844 B; `finalizeAttested`), the view's refresh finding the burn from the seed and its
release held by its random delay. 8 of 9 test functions passed (1,045 s): the last stopped at `ops("pass-time …")`,
whose argument held a quote that broke the `bash -c` line, so the release was not sent. 281 transactions in LEZ blocks
1..=123.

### `seqnet-miniapp-3` (13 Nov 11:16–11:39 UTC, `SEQNET_PROOFS=real`): the whole round trip, 9 of 9

[`e2e/runs/seqnet-miniapp-3/`](../runs/seqnet-miniapp-3/SUMMARY.md): **all 9 test functions passed** (1,103 s), 20 calls
of the view to the module through Logos Core, 19 screenshots. The sequencer verified every private transaction's
receipt (no dev mode on LEZ), and LEZ's wallet proved each for real on the box's CPU:

| Step | Through the view and the module | Result |
|---|---|---|
| deposit | Deposit on the defaults, approved in the wallet prompt | the module's commitment and `depositETH`, anvil block 9, leaf 0 |
| import, fees | `ops.sh import`, `ops.sh fund` | `Init`, `Register` ETH, `ImportRoot` n=1; the gas drop's shield proven for real (248.4 s) |
| private claim | Claim: held by the view's random delay, waited out, then the private claim on the defaults | `ClaimPrivately` from the module, proven by LEZ's wallet (388.1 s; receipt 239,532 B; transaction 256,068 B), verified and applied by the sequencer in LEZ block 93; the deposit `claimed`, private `/0` holding 0.001 ETH |
| private burn | Burn and release on the defaults (private holder, a fresh recipient, the release delay on) | the module's `Burn`, proven (333.9 s; receipt 239,046 B; transaction 255,031 B), LEZ block 127; holding 0 |
| checkpoint | `ops.sh checkpoint` | over LEZ blocks 20..=127 (108 blocks, 226 transactions; every block's final inscription accepted); `checkpoint_b` executed (22,544,384 padded cycles, journal equal to the native one); mock `accept`; `submitCandidate`, Δ, `finalizeAttested` |
| release | Redeem: the burn found from the seed, held by its random delay (waited out on the view's clock) and the vault's 120 s (anvil's), then the release on the defaults | the module's release by the wallet's rule (checkpoint 0, the registered relay fee, any relayer); `seqnet-chain` hands it to `ferrata-relayer`: **0.000997 ETH paid to the fresh address** (anvil block 16); Positions: `0 not claimed`, `0 not released` |

293 transactions in LEZ blocks 1..=129 and 15 on anvil, none reverted (`SUMMARY.md` lists each with its sender).

## 3. Through `ferrata_bridge_module` beside Logos Core's own `lez_core`

`lez_core` (logos-execution-zone-module) at **`cc73ebf`**, `main` after its v0.4 pull requests were merged on 13 November
(LEZ v0.4.0-rc1 `1806e87`, logos-module-builder `5f74cdb`: our pins), built from its source as merged and with
[`modules/patches/lez_core-cc73ebf-authorizes-public-pda.patch`](../../modules/patches/lez_core-cc73ebf-authorizes-public-pda.patch)
(`run.sh build-lezcore`; [`modules/results/lez-core-v04-2026-11-14/`](../../modules/results/lez-core-v04-2026-11-14/)).
`run.sh lezcore NAME`: a headless Logos Core (logosctl `66d098d`) with `ferrata_bridge_module` and the patched `lez_core`;
the user's LEZ wallet opened inside `lez_core` (`open`, `force_rpc`: a client of the zone's sequencer); the module's
session routed through it (`wallet lez`); the user's calls in the mini-app's order with logosctl (no view, so the
wallet's privacy delays are not waited for); `seqnet-chain` kept beside every LEZ read and send (the plain path) and for
the zone's history and Ethereum.

### `seqnet-lezcore-1` (14 Nov 10:02–10:09 UTC, dev mode): the round trip, every LEZ read equal

[`e2e/runs/seqnet-lezcore-1/`](../runs/seqnet-lezcore-1/SUMMARY.md):

| Step | Through `lez_core` | Beside it |
|---|---|---|
| bridge states | 85 reads through `get_account_public`: `config(cid)` 13, `token(cid, ETH)` 13, `root(cid, 1)` 10, the deposit's `nullifier(…)` 1, `holder(…)` 48 (30 non-empty) | 85 of 85 equal to `seqnet-chain`'s reads |
| private holding | 9 reads (`sync_to_block`, `get_account_private`) | 9 of 9 equal to `seqnet-chain`'s wallet |
| private claim | `ClaimPrivately` (the private account, four PDAs) executed and proven (dev) by LEZ's wallet inside Logos Core and sent, 603 ms | applied in LEZ block 30 (private, 33,055 B; visible calls `Read`, `Issue`) |
| private burn | `Burn` (the private account, three PDAs), 413 ms | applied in LEZ block 31 (private, 32,018 B; visible call `Burn`) |
| release | — | 0.000997 ETH to the fresh address (the module's rule, `seqnet-chain`'s relayer) |

`lez_core` as merged, on the same calls: the bridge program's PDA as an account is refused
(`accounts_json[0] unknown kind "public_pda"`); the claim with no account authorizing (as `cc73ebf` mentions them) is
refused by the bridge program (`ProgramExitedWithCode { code: 252 }`, `WRONG_DESTINATION`). 101 LEZ transactions in
blocks 1..=33 and 15 on anvil, none reverted.

### `seqnet-lezcore-2` (14 Nov 10:11–10:29 UTC, `SEQNET_PROOFS=real`): proven inside Logos Core, verified by the sequencer

[`e2e/runs/seqnet-lezcore-2/`](../runs/seqnet-lezcore-2/SUMMARY.md): the same path with the sequencer verifying every
receipt. Through `lez_core`: 85 of 85 bridge-state reads and 9 of 9 private-holding reads equal to the plain path's; the
private claim proven by LEZ's wallet inside Logos Core in 248.7 s (receipt 239,532 B, transaction 256,068 B: the sizes
of `seqnet-miniapp-3`'s claim) and applied in LEZ block 74; the private burn proven in 228.8 s (239,046 B, 255,031 B)
and applied in block 98; the release paid 0.000997 ETH. 235 LEZ and 15 Ethereum transactions, none reverted.

## 4. The mini-app inside Logos Basecamp, beside `lez_core`

### `seqnet-basecamp-2` (16 Nov 15:50–15:58 UTC, dev mode): the round trip in Basecamp, 13 of 13 steps

[`e2e/runs/seqnet-basecamp-2/`](../runs/seqnet-basecamp-2/SUMMARY.md); Basecamp's records and its window's pictures:
[`modules/ferrata_bridge_ui/results-basecamp-roundtrip/`](../../modules/ferrata_bridge_ui/results-basecamp-roundtrip/README.md).
Logos Basecamp `b642a9a` (dev build, standalone mode, offscreen) hosted the view; its Module Inspector loaded
`ferrata_bridge_module` and `lez_core` (patched); the view called the module through Basecamp's `logos`
(`callModuleAsync`, 27 calls); each user step was a click through Basecamp's inspector, and the chains were read
directly after each step: 13 of 13 steps, 50 checks, 149 s. Through `lez_core`: 167 of 167 bridge-state reads and
17 of 17 private-holding reads equal to the plain path's; the private claim and burn executed, proven (dev) and
sent by `lez_core` inside Basecamp, applied in LEZ blocks 30 and 31; the release paid 0.000997 ETH to the fresh
address. Ethereum through `seqnet-chain`. 103 LEZ and 15 Ethereum transactions, none reverted.

### `seqnet-basecamp-real-1` (16 Nov 17:09–17:29 UTC, `SEQNET_PROOFS=real`): proven inside Basecamp, the view's calls cut

[`e2e/runs/seqnet-basecamp-real-1/`](../runs/seqnet-basecamp-real-1/SUMMARY.md). `lez_core` proved the private claim and
burn for real inside Basecamp, and the sequencer, verifying, applied them in LEZ blocks 73 and 97 (transactions 256,068 and
255,031 B). Basecamp cut each of the view's calls after 20 s (`timed out after 20000ms`), so the view learned the
outcome only by refreshing, and lost the burn's release delay; the run stopped before the release. 235 LEZ and 12
Ethereum transactions, none reverted
([`l4-real-proofs/`](../../modules/ferrata_bridge_ui/results-basecamp-roundtrip/l4-real-proofs/README.md)).

### `seqnet-basecamp-jobs-real` (17 Nov 14:50–15:08 UTC, `SEQNET_PROOFS=real`): the whole round trip in Basecamp, the claim and burn as jobs

[`e2e/runs/seqnet-basecamp-jobs-real/`](../runs/seqnet-basecamp-jobs-real/SUMMARY.md); Basecamp's records and pictures:
[`real-proofs-jobs/`](../../modules/ferrata_bridge_ui/results-basecamp-roundtrip/real-proofs-jobs/README.md). The
module's claim and burn ran as jobs the view polled (`ferrata_bridge_module` `calls/src/jobs.rs`): `lez_core` proved
the private claim in 247.1 s and the burn in 222.9 s inside Basecamp, the sequencer, verifying, applied them in LEZ
blocks 74 and 97 (transactions 256,068 and 255,031 B), and the release paid 0.000997 ETH to the fresh address. 13 of 13
steps, 64 checks; 191 calls of the view, 162 of them polls, the longest 3,176 ms, none cut by Basecamp's 20 s.

### `seqnet-basecamp-reopen-real` and `seqnet-basecamp-final-real` (17 Nov 15:57–16:17 and 16:31–16:49 UTC, `SEQNET_PROOFS=real`): the view closed and opened again while the burn was proven

[`e2e/runs/seqnet-basecamp-reopen-real/`](../runs/seqnet-basecamp-reopen-real/SUMMARY.md),
[`e2e/runs/seqnet-basecamp-final-real/`](../runs/seqnet-basecamp-final-real/SUMMARY.md). With `BASECAMP_REOPEN=1` the
driver unloaded the view in Basecamp's Apps Inspector 6 s into the burn's real proof and loaded it again; the new view
found the running job and completed the burn and the release, 13 of 13 each. The first run (the view of `48a3b13d`)
found one call of the reopened view refused (a `scan` sent before it had adopted the running job), fixed in
`3b7ffe1d`; on that final code: the claim proven in 245.9 s (LEZ block 73), the burn in 229.0 s (block 97), 196 calls of
the two views, none cut, none refused
([`real-proofs-jobs/final/`](../../modules/ferrata_bridge_ui/results-basecamp-roundtrip/real-proofs-jobs/final/README.md)).

