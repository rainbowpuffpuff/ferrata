# The mini-app inside Logos Basecamp: the private round trip (16 November 2026)

**Logos Basecamp itself hosted the mini-app and its core modules, and a user's private round trip ran in the
view, step by step, on a running LEZ devnet.** Basecamp (pinned commit `b642a9a`, built from source) opened
`ferrata_bridge_ui`; its own Module Inspector loaded `ferrata_bridge_module` and `lez_core`; the view called the
module through the `logos` object Basecamp injects; every user step was a click on the view inside Basecamp's
window: connect, deposit, private claim, private burn, release, recovery, registry. After each step the chains were
read directly and checked. Proofs were in RISC Zero dev mode. **13 of 13 steps passed, with 50 checks.**
Basecamp's window was grabbed at every state: 23 pictures in [`pictures/`](pictures/).

| Level (as the work item states them) | Reached | Evidence |
|---|---|---|
| L1: Basecamp built; the ui module and the core modules installed and loaded in one Basecamp | **yes** | [`probe/`](probe/) (3 of 3), pictures 01–02, `run/basecamp-run.log` lines "Successfully loaded core module" |
| L2: the view calls `ferrata_bridge_module` through Basecamp's `logos` and shows live devnet data: account, balances on both sides, registry with caps | **yes** | pictures 04–06, 11, 15; the checks "the Ethereum balance the view shows equals anvil's" and "the LEZ side of the token is shown" |
| L3: the whole round trip driven inside Basecamp, each step checked on the chains | **yes, dev-mode proofs** | [`run/events.jsonl`](run/events.jsonl) (13 steps, 50 checks), [`run/chain-checks.jsonl`](run/chain-checks.jsonl), [`e2e/runs/seqnet-basecamp-2/`](../../../e2e/runs/seqnet-basecamp-2/SUMMARY.md) |
| L4: the private claim and burn proven for real | **no** | the blocker is in "Not reached: L4" below |
| L5: pictures of Basecamp's window at each step | **yes** | [`pictures/`](pictures/), 23 PNGs, 84–200 kB each, 3.6 MB in all |

## What ran, where

On the Modal build box (gVisor, x86-64, Ubuntu 24.04 image, Nix 2.28.3). Nothing went to a public network.

- **Basecamp**: `logos-basecamp` `b642a9aba732b80343a3a7b622dad8c65e534bdb`, the pin of
  `scripts/basecamp-ui-tests.sh`, version `pre-release-b642a9a (dev build)`, built from source with
  `nix build …#dev` (`logos-basecamp-dev`: Basecamp's standalone mode, its QML inspector logos-qt-mcp `89d1d49`,
  and the lgpm of its package manager). Basecamp ran headless on Qt's offscreen platform, in standalone mode
  (`--module` with the view's install root, `--modules-dir` with the core modules), and its `dev --test` mode ran
  the driver. Its Logos Core runtime is logos-liblogos `af19e519`, logos-cpp-sdk `05e1442`, logos-protocol
  `9d59f5d`; the build commits are in `run/basecamp-run.log`. This liblogos and this protocol are the ones the
  headless Logos Core (`logosctl` `66d098d`) of rows 103 and 113 uses, so the pinned commit loaded the core
  modules as built: no newer Basecamp was needed.
- **The modules**: `ferrata_bridge_ui` (this directory's module, the view at this branch, built by
  logos-module-builder `5f74cdb`); `ferrata_bridge_module`'s dev `.lgx` (sha256 `a6385ff0…`, built on 14 November by
  `modules/scripts/check.sh`, unchanged since); `lez_core` (logos-execution-zone-module `cc73ebf` with
  `modules/patches/lez_core-cc73ebf-authorizes-public-pda.patch`, sha256 `df272fd3…`, EVIDENCE row 113). Basecamp's
  lgpm installed both `.lgx` into the `--modules-dir` root ([`build/VERSIONS`](build/VERSIONS)).
- **The chains** (`e2e/seqnet`, as rows 102, 103 and 113): a LEZ v0.4.0-rc1 sequencer and a Logos Blockchain node
  from the crown-live-3 bundle, with the final attestation (`0x816d1fef…`) and bridge (`0xccde6171…`) programs read
  back after deployment, and FerrataVault v5 in mode (B) on a local anvil (chain id 11155111) with the vault tests'
  mock verifier. The sequencer accepted RISC Zero dev-mode receipts.
- **The driver**: [`basecamp/roundtrip.mjs`](../basecamp/roundtrip.mjs), run by Basecamp's own test mode through
  `e2e/seqnet/hook-basecamp.sh`. Each user step is a click: the inspector posts a mouse press and release to the
  view's QQuickWidget at the button's centre. Reads go through the inspector's `evaluate` in the view's QML
  context, and pictures are the inspector's grab of Basecamp's main window.

### The steps (`run/events.jsonl`)

| # | Step | Seconds | Checked on the chains (`ops.sh chain`: cast on anvil, `seqnet-txs` on the sequencer) |
|---:|---|---:|---|
| 1 | Basecamp opens the mini-app | 0.1 | inside Logos the view's transport is the core module; Basecamp's bridge has `callModuleAsync` |
| 2 | Basecamp's Module Inspector loads the core modules | 3.4 | its Load buttons, for `ferrata_bridge_module` and `lez_core` |
| 3 | The session: `wallet open` at the chain service and `wallet lez` (the session's LEZ through `lez_core`) | 0.2 | the host's step, not the user's (below) |
| 4 | Connect: the account and both sides' balances | 2.9 | the account the view shows is anvil's depositor; the Ethereum balance it shows equals anvil's (10,000 ETH); LEZ /0 private 0 and public 0 at LEZ block 20 |
| 5 | Registry before the operator's first import | 1.2 | ETH registered on the vault, no LEZ side yet |
| 6 | Deposit, private by default | 3.4 | one `Deposit` log more, naming the user; the vault holds 1,002,000,000,000,000 wei more (the log's 0.001 ETH and the 0.000002 ETH fee); the user paid 1,073,119,174,371,296 wei with gas |
| 7 | Positions before the import (recovery from the seed) | 1.2 | the recovered deposit is the vault's leaf 0 |
| 8 | The operator's import and the gas drop (`ops.sh import`, `fund`; not the user's steps) | 84.1 | 25 LEZ transactions applied, none reverted; the registry then shows the LEZ side of the token, with the mint and burn caps |
| 9 | The private claim, once its random delay has passed (the view's clock moved on 14,312 s) | 11.0 | transaction `f2bcb745…` in LEZ block 30, private, not reverted; the block the view names is the sequencer's; the deposit then shows as claimed |
| 10 | The private burn to a fresh address | 8.9 | transaction `bd548236…` in LEZ block 31, private, not reverted, its `Burn` call visible to the checkpoint; the private holding spent |
| 11 | The checkpoint (`ops.sh checkpoint`) and the release by a relayer (the view's delay: 7,176 s; the vault's: 121 s on anvil) | 25.6 | one `CheckpointFinalized` log; one `Release` log to the fresh address; the fresh address holds 997,000,000,000,000 wei of the log's 0.001 ETH (relay fee 0.000002 ETH, and the vault's checkpoint fee) |
| 12 | Positions after the round trip | 2.4 | the view's recovery: 1 deposit (0 not claimed), 1 burn (0 not released) |
| 13 | Records: the view's call log, the module's `lez_report` | 0.0 | |

The whole test took 149 s, and the whole run took 440 s with the zone's start, the deployment, the vault and the
records. Across the run there were 103 LEZ and 15 Ethereum transactions, none reverted
(`e2e/runs/seqnet-basecamp-2/SUMMARY.md`).

### The path of every call

| Call | Path |
|---|---|
| The view → `ferrata_bridge_module` (27 calls: `info`, `scan`, `registry`, `price`, `deposit`, `claim`, `burn`, `release`; [`run/view-calls.json`](run/view-calls.json)) | `logos.callModuleAsync("ferrata_bridge_module", "wallet", [json])`: Basecamp's `LogosQmlBridge` (logos-view-module-runtime `3e69973`), so Basecamp's Logos Core reached the module in its own process. Longest: the claim 5.5 s, the burn 5.4 s |
| The bridge program's actor states (config, token, roots, the deposit's nullifier, holders) | **Logos Core's `lez_core`** (`get_account_public`): 167 reads, all 167 equal to the plain path's (`run/lez-report.json`) |
| The private holding of account /0 | **`lez_core`** (`sync_to_block`, `get_account_private`): 17 reads, all equal |
| The private claim (`ClaimPrivately`) and the private burn (`Burn`) | **`lez_core`** (`send_generic_private_transaction_with_accounts`): executed and proven in dev mode by LEZ's wallet inside Basecamp's `lez_core` process (the claim's dev receipt 16,519 B), then sent and applied in LEZ blocks 30 and 31; the chain service read the block that applied each (`module.tx`) |
| Ethereum reads (the vault, its registry, balances, the deposit's inputs, the roots) | the module's `wallet` method through the stand-in **`seqnet-chain`** (`module.info`, `module.registry`, `module.price`, `module.deposit_inputs`, `module.vault_roots`; [`run/chain-calls.jsonl`](run/chain-calls.jsonl)), not `eth_rpc_module` |
| The deposit transaction | signed with the user's keystore and sent by **`seqnet-chain`** (`module.send_deposit`), not `keystore_module`/`tx_sender_module` |
| The release | sent by the relayer (`ferrata-relayer`) through **`seqnet-chain`** (`module.vault_release`) |
| The plain LEZ path beside `lez_core` | **`seqnet-chain`** (LEZ's wallet library): `module.inputs` 20 and `module.private` 17, which `lez_core`'s reads were compared with |
| Opening the session (`open`, `lez`) and `lez_report` | the driver, through the view's own `logos` (`logos.callModule`, synchronous) |
| The operator, the gas drop, the checkpoint, the clocks | `e2e/seqnet/ops.sh`, run by the driver between the user's steps (crown-driver `crown-b --operator-only`, `seqnet-fund`, `realblocks-crown` with `checkpoint_b` executed and the stand-in prover, the relayer's `submitCandidate` and `finalizeAttested`, anvil's clock) |

### What changed in the view for Basecamp (host code; no program and no image id changed)

- **Basecamp's bridge returns the module's string reply serialised once more as a JSON string.** The probe's raw
  reply was `"{\"error\":\"no wallet session: …\",\"ok\":false}"` ([`probe/call-info.json`](probe/call-info.json)):
  `serializeResultForTesting` dumps the module's `QString` result as a JSON string literal. The headless Logos Core
  runner had handed over the inner text. `Main.qml`'s `moduleReply()` now parses up to twice and reads the host's
  own error payload (`{error, module, method, message}`). After a timeout it says the outcome is unknown and asks
  the user to refresh before repeating the step.
- **The view calls asynchronously where the host can** (`logos.callModuleAsync`, Basecamp's). The synchronous call
  holds Basecamp's GUI thread, and the inspector with it, for as long as the module works. A host without the
  asynchronous call, such as the Qt Quick Test runner, is still called synchronously.
- The header shows both sides of the connected account (the Ethereum balance and block from `info`, the LEZ
  holdings of /0 from `scan`) with a Refresh button. Inside Logos it names the module's endpoints, not the devnet's
  defaults. The view also keeps its own call log (`callLog`: method, outcome, milliseconds).

## Not reached: L4 (real proofs inside Basecamp)

**A ui module's call to a core module cannot last longer than 20 s in this Basecamp.** A private claim or burn
proven for real by `lez_core` takes 229–249 s on this box (EVIDENCE row 114: claim 248.7 s, burn 228.8 s), so the
view would be told the call failed while `lez_core` is still proving. A step reported as failed that still lands
on chain is worse than a clean failure, so no real-proof run was claimed. From the sources at Basecamp's pins:

- logos-protocol `9d59f5d`, `cpp/logos_mode.h` 29–32: `struct Timeout { int ms; explicit Timeout(int milliseconds
  = 20000) … }`.
- logos-view-module-runtime `3e69973`, `src/LogosQmlBridge.cpp`: `callModule` (344–418) passes `Timeout(budgetMs)`
  with `budgetMs = client->isConnected() ? Timeout().ms : kStartupCallBudgetMs` (389), so 20 s. `callModuleAsync`
  (420–534) arms its own timer at `timeoutMs` (default 30,000, and 30,000 when 0 or less: `kReadinessFallbackMs`,
  58 and 509–514), and its dispatch calls `client->invokeRemoteMethodAsync(module, method, args, callback)` with no
  `Timeout` (462–487). logos-protocol `cpp/logos_api_client.h` 223–225 then defaults it to `Timeout()`, 20 s, and
  the transport passes it on (`logos_api_consumer.cpp` 1174, 1185).

The smallest upstream change would be in logos-view-module-runtime: `callModuleAsync` passes `Timeout(timeoutMs)`
to `invokeRemoteMethodAsync` when `timeoutMs > 0` (and a long one when it is 0), and the 30 s fallback timer is
kept for the wait until the module is reachable, not for the call. Without an upstream change, the module can
take the proof off the call: `claim`/`burn` start the work and return a job id, and the view polls `job`. That is
host code, but it needs the module rebuilt and its suites run again; it was not done here. The drafted upstream
issue is in the agent's report.

## What this does not show

- **Ethereum through Logos Core's own modules.** Inside Basecamp the module's Ethereum reads, the deposit's
  signing and sending and the release still go through the stand-in `seqnet-chain`, not `eth_rpc_module`,
  `keystore_module` and `tx_sender_module`. LEZ goes through `lez_core`, and only with our patch (row 115).
- **Real proofs** (L4 above). The vault's verifier is the mock (no Groth16), and anvil's blocks are finalized for
  LEZ's light client by a stand-in committee.
- **The user does not open the session.** `wallet open` (the chain service's address) and `wallet lez` (the user's
  LEZ wallet files for `lez_core`) are made by the driver through the view's `logos`, as the Qt Quick Test's
  `initTestCase` made `open`. In Logos the wallet SDK (LP-0021, unreleased) would stand there.
- **Waiting.** The view's random claim and release delays were waited out on the view's own clock (`clockOffset`,
  14,312 s and 7,176 s), and the vault's on anvil's. None was overridden.
- **The core modules are loaded by a click in the Module Inspector.** `ferrata_bridge_ui` does not declare
  `ferrata_bridge_module` as a dependency. logos-module-builder `5f74cdb` would then require the module's LIDL
  contract (a flake input or `dependency_overrides`), and Basecamp would refuse to open the view without the core
  module, which would break `nix run .#ui-tests`. A release would declare it and ship the contract, so that
  Basecamp loads the module with the view.
- **Basecamp marks QML access to Logos Core as deprecated.** The run logged `DEPRECATED: ferrata_bridge_ui calls
  logos core from QML (logos.callModule). Move the call into a backend module ("backend": "module" in
  metadata.json); QML access to logos core is removed in the next breaking release.` A later Basecamp will need
  the view's calls behind a backend module.
- **Not the build users run.** This is Basecamp's dev build in standalone mode, headless on the offscreen
  platform; the `.lgx` files were installed by lgpm, not through the catalogue (no publisher's signature). It was
  one run, on Linux x86-64.

## The L1 probe ([`probe/`](probe/))

Before the chains existed, [`basecamp/probe.mjs`](../basecamp/probe.mjs) checked the hosting alone, on the view as
of commit `8eff3694`. The view opened in Basecamp with the module transport. The Module Inspector listed and loaded
`ferrata_bridge_module` and `lez_core`; the picture is [`probe/p03-core-modules-loaded.png`](probe/p03-core-modules-loaded.png).
`logos.callModule("ferrata_bridge_module", "wallet", …)` from the view's context reached the module, which
answered that no wallet session was open. 3 of 3 in 6 s.

## Commands, in full

On the build box (`spike/modal-build/mrun`), from the repository root:

```sh
# 1. Basecamp, the view's install root, the --modules-dir root (950 s the first time: 54 derivations built, 239 paths
#    fetched; build/timings.txt):
modules/ferrata_bridge_ui/scripts/basecamp-build.sh OUT \
    <ferrata_bridge_module's dev .lgx dir> <lez_core's patched .lgx dir>
#    which runs
#      nix build -L --cores 30 --max-jobs 2 github:logos-co/logos-basecamp/b642a9aba732b80343a3a7b622dad8c65e534bdb#dev -o OUT/basecamp-dev
#      nix build -L --cores 30 --max-jobs 2 path:modules/ferrata_bridge_ui#install -o OUT/ui-install
#      <lgpm of logos-basecamp-dev's closure> --modules-dir OUT/modules-root/modules --ui-plugins-dir OUT/modules-root/plugins install --file X.lgx
# 2. The probe (L1), no chains:
cd modules/ferrata_bridge_ui/basecamp && BASECAMP_OUT=DIR BASECAMP_CORE_MODULES=ferrata_bridge_module,lez_core \
    OUT/basecamp-dev/bin/logos-basecamp-dev --module OUT/ui-install --modules-dir OUT/modules-root --test probe.mjs
# 3. The round trip (needs e2e/seqnet's inputs and `run.sh build`; e2e/seqnet/README.md):
SEQNET_TMP=<seqnet tmp> BASECAMP_BUILD=OUT bash e2e/seqnet/run.sh basecamp NAME
#    hook-basecamp.sh then runs, with the zone's environment:
#      logos-basecamp-dev --module OUT/ui-install --modules-dir OUT/modules-root --width 1440 --height 1250 \
#        --test modules/ferrata_bridge_ui/basecamp/roundtrip.mjs
#    (BASECAMP_LEZ=plain: everything through seqnet-chain; SEQNET_PROOFS=real: real proofs, see L4)
```

The box used no other setup than `e2e/seqnet`'s (its setup script restores `/nix` and pins Nix 2.28.3).

## Environment

| Input | Version |
|---|---|
| Logos Basecamp | `b642a9a` (`pre-release-b642a9a`, dev build, standalone, offscreen), logos-qt-mcp `89d1d49`, logos-view-module-runtime `3e69973` |
| Logos Core inside Basecamp | logos-liblogos `af19e519`, logos-cpp-sdk `05e1442`, logos-protocol `9d59f5d`, logos-module `ceee1e3`, logos-package-manager `73c21ab` |
| The modules | logos-module-builder `5f74cdb`; `ferrata_bridge_module` 0.1.0 dev `.lgx` `a6385ff0…`; `lez_core` 0.6.0 (`cc73ebf` + patch) `df272fd3…` |
| LEZ | v0.4.0-rc1 sequencer and Logos Blockchain node (crown-live-3 bundle), dev-mode receipts accepted |
| Ethereum | anvil (Foundry 1.3.6), chain id 11155111, FerrataVault v5 mode (B), `MockRiscZeroVerifier` |
| Box | Modal, gVisor, x86-64, Nix 2.28.3; Basecamp's window 1440 × 1250 |

## The suites the view's change touches, run again ([`regression/`](regression/))

On the box, on this branch's view, 16 November:

| Suite | Before (committed) | Now |
|---|---|---|
| Basecamp's `nix run .#ui-tests` (`scripts/basecamp-ui-tests.sh`, Basecamp `b642a9a` as built here) | 10 of 10 (11 Nov) | **10 of 10** in 9 s ([`regression/basecamp-ui-tests.txt`](regression/basecamp-ui-tests.txt)) |
| The devnet Qt Quick Test (`scripts/box-test.sh`: the view against `cli/examples/miniapp_devnet`, Qt 6.4 offscreen) | 17 of 17 (12 Nov) | **17 of 17**, 34.5 s ([`regression/box-test/qmltest.txt`](regression/box-test/qmltest.txt)) |
| The view in a headless Logos Core on a LEZ devnet (`e2e/seqnet/run.sh miniapp`, the `qmltest-logos` runner, dev mode) | 9 of 9 (13 Nov, `seqnet-miniapp-3`, real proofs) | **9 of 9**, 122.7 s, dev mode ([`regression/seqnet-miniapp/`](regression/seqnet-miniapp/qmltest.txt)) |

`ferrata_bridge_module` itself was not changed, so its own tests were not run again. The committed `results/` of 12
November were left as they were: these runs' outputs are only here.

## What failed on the way

The first round trip (`basecamp-1`) stopped at step 6 on the driver's own check. It expected the vault to grow by
exactly the `Deposit` log's amount, but the vault also keeps the deposit fee (1,002,000,000,000,000 wei for a 0.001
ETH deposit). The check now allows for the fee; that run's records were not kept. Steps 1–5 had passed in it.

## Files

- [`pictures/`](pictures/): Basecamp's window at each state, 01–23.
- [`run/`](run/): `basecamp-run.log` (Basecamp's and the driver's output), `events.jsonl` (steps, clicks, checks,
  waits, operator steps), `chain-checks.jsonl` (the chains' state after each step), `view-calls.json`,
  `chain-calls.jsonl` (the module's calls to `seqnet-chain`), `lez-report.json` (the module's calls to `lez_core`),
  `session.json`, `summary.json`, `basecamp-versions.txt`.
- [`probe/`](probe/): the L1 probe. [`build/`](build/): the build's timings and versions.
- [`regression/`](regression/): the suites run again after the view's change.
- The chains' side of the run: [`e2e/runs/seqnet-basecamp-2/`](../../../e2e/runs/seqnet-basecamp-2/SUMMARY.md).
