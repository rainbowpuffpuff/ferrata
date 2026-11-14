# Ferrata's Ethereum transactions through Logos Core's send stack (14 November 2026, Modal build box)

Where it ran: the Modal build box of this worktree (profile ws-5; x86-64, gVisor, Nix 2.28.3, Foundry 1.3.6), a
local anvil chain (id 31337, hardfork Prague). Nothing was sent to any public network. Dev builds (`.#lgx`) of every
module; logosctl `66d098d` (Logos Core, headless). Script: [`modules/scripts/send-stack.sh`](../../scripts/send-stack.sh)
(`build`, then `run`); comparison: [`modules/scripts/send_stack_compare.py`](../../scripts/send_stack_compare.py).

## 1. The send stack builds with one builder

`send-stack.sh build`: each module at the revision `tx_sender_module`'s lock pins, its `flake.lock` given
logos-module-builder `5f74cdb`'s own subtree ([`graft_builder_lock.py`](../../scripts/graft_builder_lock.py)),
`eth_rpc_module` with [`patches/eth_rpc_module-6965f32-generated-clients.patch`](../../patches/eth_rpc_module-6965f32-generated-clients.patch)
applied ([`build/graft-eth_rpc_module.log`](build/graft-eth_rpc_module.log): "Applied patch rust-lib/src/glue.rs
cleanly"); `verified_proxy_module` and `beacon_api_module` with their own lock (builder `a57dc7f`), as
[`verified-proxy-2026-11-14`](../verified-proxy-2026-11-14/README.md) built them. [`build/steps.tsv`](build/steps.tsv):

| module | revision | result | seconds |
|---|---|---|---:|
| `eth_rpc_module` (patched) | `6965f32` | built, packaged | 94 |
| `fee_module` | `f2065e7` | built, packaged | 49 |
| `keystore_module` | `e83a899` | built, packaged | 115 |
| `signer_manager_module` | `bd5c84d` | built, packaged | 108 |
| `tx_sender_module` | `f1754c3` | built, packaged | 82 |
| `beacon_api_module` | `aca138e` | built (own lock) | 7 |
| `verified_proxy_module` | `fa98479` | built (own lock; `libverifproxy` from the store) | 60 |
| logosctl | `66d098d` | built | 58 |

Each grafted build's first attempt stopped on the box's leftover `/homeless-shelter` (a known trap of this box, not of
the modules) and the retry built it. Without the patch, `eth_rpc_module` stops at `E0308` twice
([`lez-core-v04-2026-11-14/send-stack/build-eth_rpc_module-errors.txt`](../lez-core-v04-2026-11-14/send-stack/build-eth_rpc_module-errors.txt)):
the `verified_proxy_module` client builder 5f74cdb generates (logos-rust-sdk `040e94e`, after #64 "Generate typed Rust
collection APIs from LIDL", `569ecc5`) returns `status()` as `BTreeMap<String, Value>` and takes `rpc`'s params as
`&[Value]`; the glue (written for rust-sdk `f46f40e`, before #64) passed `Value` and `&Value`. The patch converts both
(+6 lines, [`build/eth_rpc_module-applied.diff`](build/eth_rpc_module-applied.diff)): nothing on the wire changes.
`ferrata_monitor_module` and `ferrata_relayer_module` with the new path (`check.sh`, [`check/steps.md`](check/steps.md)):
native tests, the `.lgx`, installed, loaded, called: 0 step failures, 0 check failures; the monitor's unit tests 5 of 5
([`monitor-unit-tests.log`](monitor-unit-tests.log)).

## 2. All of them in one headless Logos Core

[`modules-loaded.json`](modules-loaded.json): 14 modules loaded, the 7 above, `ferrata_monitor_module`,
`ferrata_relayer_module` and logosctl's five. Loading `tx_sender_module` brought up `beacon_api_module`,
`keystore_module`, `verified_proxy_module`, `signer_manager_module`, `eth_rpc_module` and `fee_module` (the signer
manager's optional keycard and QR signers skipped: not installed) ([`run/logos-setup.log`](run/logos-setup.log)).

The patched glue at run time ([`calls.jsonl`](calls.jsonl)): with chain 31337 in `required` mode,
`eth_rpc_module.verified_proxy_status` asked `verified_proxy_module.status` through the generated client and classified
the proxy's own snapshot (`state: "uninitialized"` → `unconfigured`, `usable: false`; that verdict comes only from the
proxy's reply, `rust-lib/src/verdict.rs` 113–121), and `block_number` was refused (`verified_blocked`), fail closed;
back in `off` mode, `disabled`. The other changed call, `rpc` through the proxy (glue.rs 247), was compiled, not run: a
devnet has no beacon chain for the proxy to follow, and this run touched no public network.

## 3. The key: which module holds it, by which rule

`keystore_module`'s and `signer_manager_module`'s `configure` are ungated upstream (keystore `e83a899`
`rust-lib/src/glue.rs` 36–45: "UNGATED, deliberately and for now: any caller can name itself custodian"; signer manager
`bd5c84d` glue.rs 27–30) and TOTAL. From logosctl the run kept every default role and added the two Ferrata daemon
modules as custodians of both (configuration; no fork):

- `keystore_module`: approvers `evm_signer_ui`, custodians `evm_keystore_ui`, `ferrata_monitor_module`,
  `ferrata_relayer_module`, managers `signer_manager_module`;
- `signer_manager_module`: the same approvers and custodians, signers `keystore_module`.

Then each Ferrata module's new method `custody({key_file, password_file, ttl_ms})` (the key never in a request), made
by the module itself, so the keystore sees a named custodian (tier D; the host's own `import_private_key` is refused by
design, [`lez-core-v04-2026-11-14/send-stack/keystore-load.txt`](../lez-core-v04-2026-11-14/send-stack/keystore-load.txt)):
`keystore_module.import_private_key` (the keystore enrols the new wallet with the signer manager itself, glue.rs
466–479); `signer_manager_module.accounts` until it lists the account; `signer_manager_module.unlock({account, password,
ttlMs: 7200000, apps: ["tx_sender_module"], confirm: false})` (tier D); `tx_sender_module.history` until the account's
history is open (tx_sender opens it with its key from the signer manager, in its own thread, and refuses a send with
`unavailable` until then: glue.rs 280–390, 640–649). Monitor: verdict ok in 74 ms; relayer: 68 ms
([`calls.jsonl`](calls.jsonl), `method: custody`). From then on every request `tx_sender_module` makes for that
account is signed as it arrives, with no person (signer manager glue.rs 646–670, access.rs 791–800:
`Cover::WithoutAsking`).

## 4. Each daemon transaction twice from one state: plain, and through `tx_sender_module`

`send-stack.sh run`: a rule (B) vault from `vault/script/MonitorFixture.s.sol` (its verifier accepts every seal, as in
the monitor's own rehearsal), ETH registered (0.1 ETH denomination, relay fee 0.001, checkpoint fee 0.0001) and one
deposit. For each step, an anvil snapshot; the daemon (`start`, then `status` until it ends by its bound) with the plain
path (the relayer's Web3 keystore, the monitor's key file: alloy signs and sends over JSON-RPC); revert; the same daemon
with `--send-via logos --from ADDRESS` (no key file: `tx_sender_module` prices the call through `fee_module`, the signer
manager has the keystore sign, `send_status` broadcasts through `eth_rpc_module`). The chain's clock is pinned (12 s
per block, the same block times in both branches). [`run/compare.md`](run/compare.md), [`run/compare.json`](run/compare.json):

| step | daemon | call (calldata) | equal on both paths | events | gas used | gas limit (both) |
|---|---|---|---|---|---:|---:|
| R1 | relayer (checkpoint job) | `submitCandidate` (484 B) | to, calldata, value, from, nonce, status, gas used, logs, block; the vault's state after | CandidateSubmitted | 141,271 | 141,271 |
| R2 | relayer (after the 1 h wait) | `finalizeAttested` (420 B) | all | CheckpointFinalized | 216,167 | 235,367 |
| R3 | relayer (release job) | `release` (292 B) | all | Release, OwedCredited | 173,273 | 175,312 |
| R3 | relayer (same round) | `claimOwed` (36 B) | all | OwedClaimed | 35,786 | 47,107 |
| M1 | monitor (tip 12 conflicts with the node) | `cancelCandidate` (36 B) | all | CandidateCancelled | 41,477 | 52,163 |
| M2 | monitor (the same wrong state again) | `cancelCandidate` (36 B) | all | CandidateCancelled | 41,477 | 52,163 |
| M2 | monitor (second cancellation of a state) | `pauseCheckpoints` (4 B) | all | CheckpointsPaused | 47,260 | 47,260 |

5 of 5 steps, 7 of 7 transaction pairs equal; the vault's state after each step equal (final state, checkpoint count,
both candidates, the burn root's entry, the nullifier, escrow, the relayer's credit, the checkpoint freeze, the
recipient's and the vault's balances: `*/probe.json`). Each daemon reported the path it used, per transaction
(`via`: the monitor's alerts, the relayer's lines): `plain` on the plain branch; on the other `logos`, with
`tx_sender_module`'s request id, one `send_status` poll and 29–66 ms from `send` to the broadcast.

Fees: both paths paid a 1 wei tip; `fee_module` set `maxFeePerGas` to 2 × base fee + tip, alloy's filler higher. The
effective gas prices differ between the branches by anvil, not by the path: `evm_revert` does not restore anvil's next
base fee, so the second branch starts from the base fee the first branch's blocks lowered (ratio 0.875 after one plain
block, 0.766 = 0.875² after two; derived from `compare.json`). The costs of the two paths are therefore not compared
here; gas used and gas limits are.

Not on Ethereum: the relayer's root import (`ImportRoot`) is a LEZ transaction (`cli/src/relayerd.rs`, `import_root` →
the zone), so it is not `tx_sender_module`'s; through Logos Core it would be `lez_core`'s public send, which has no PDA
account kind ([`lez-core-v04-2026-11-14`](../lez-core-v04-2026-11-14/README.md)). The monitor's third transaction,
`revokeFrom` (when its key is the freeze authority's), goes through the same `Chain::send` and was not run here.

## 5. The fallback, and what is refused

- **Fallback** ([`run/F-fallback/`](run/F-fallback/)): `tx_sender_module` unloaded, the monitor started with
  `--send-via logos` and its key file as well; a third wrong candidate: cancelled the plain way, reported
  `via: {path: plain, fallback: true, reason: "... failed to acquire remote object 'tx_sender_module' (module not
  loaded, ...)"}`. The fallback is taken only when nothing was broadcast (`tx_sender_module` refused or did not
  answer, or the request was withdrawn unapproved); a send that may have left (`stuck`) is never sent again.
- **`run` with `--send-via logos` is refused** ([`run/run-refused.log`](run/run-refused.log)): see finding F2.

## 6. Findings

- **F1** (`eth_rpc_module`): does not compile against builder 5f74cdb's generated client (above); fixed by the patch;
  draft upstream PR in `spike/tmp/send-stack/REPORT.md`. No upstream branch or PR fixes it (checked 14 Nov: `main`
  `6965f32` is the newest branch; no open PR).
- **F2** (Logos Rust SDK threading, measured): a call to another module from a module's worker thread blocks until the
  module's Qt main thread services it (on the Qt-affine transport, `lp_client_create` ends in a
  `Qt::BlockingQueuedConnection`: logos-rust-sdk `040e94e` `src/plugin.rs` 212–222). A module method that blocks while
  its daemon thread calls another module therefore deadlocks, and the call's own timeout does not bound it: the first run
  drove the relayer with its blocking `run` method, and `ferrata_relayer_module.run` timed out after 900,000 ms with no
  transaction sent (`"code":"timeout","message":"call to 'ferrata_relayer_module.run' timed out after 900000ms"`).
  Daemons that call modules run with `start` (the method returns; the Qt thread stays free); `run` now refuses
  `--send-via logos` with that reason.
- **F3** (custody rests on an ungated `configure`): naming a Ferrata module custodian works because both modules'
  `configure` is ungated "for now"; keystore PR #25 (open, "Tier A and D grants come from the access policy;
  configure is removed") would move that grant to the access policy, where a deployment would name the module instead.
- **F4** (anvil's clock): unpinned (the run before this one, [`unpinned-clock/`](unpinned-clock/compare.md)), R1
  differed in gas used (141,271 plain, 138,471 through `tx_sender_module`) and in the candidate's `submittedAt`. Both
  are the block's timestamp: `submitCandidate` writes `submittedAt` and `keylessFrom = block.timestamp`, and the second
  write costs 2,800 gas less when the value is unchanged (warm `SSTORE` of the same value instead of a reset); the
  branches had mined at different anvil times. The gas does not depend on the limit: from one state, 138,471 gas used at
  limits 138,471, 141,271, 160,000 and 300,000 ([`unpinned-clock/gas-probe.txt`](unpinned-clock/gas-probe.txt)).
  Pinned, every pair is equal.

## Re-check

On the build box (Nix, Foundry, `vault/lib`):

```sh
modules/scripts/send-stack.sh build OUT                       # the Logos modules and logosctl (Nix)
MODULES="ferrata_monitor_module ferrata_relayer_module" PORTABLE=0 E2E=0 modules/scripts/check.sh CHECK
FERRATA_LGX=CHECK modules/scripts/send-stack.sh run OUT       # OUT/run/compare.md; exit 0 of the comparison
```

`spike/modal-build/mrun` ran them as `B1` (build, 574 s), `B4` (check.sh, 317 s) and `B5` (run, 32 s; the comparison's
exit 0). Spend for the whole item on ws-5: about 0.35 USD (mrun ledger).
