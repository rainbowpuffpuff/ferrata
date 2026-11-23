# `ferrata_bridge_ui`: results

## 17 November 2026: real proofs inside Logos Basecamp, the claim and burn as jobs

**With real proofs, the whole private round trip ran in the view inside Logos Basecamp, and no call of the view was
cut** ([`results-basecamp-roundtrip/real-proofs-jobs/`](results-basecamp-roundtrip/real-proofs-jobs/README.md), branch
`basecamp-jobs`, the Modal build box). Basecamp still ends a view's call to a core module at 20 s (below); the
module's `wallet` `claim` and `burn` now start a job in a thread of the module and answer at once, and the view polls
the job every 3 s, shows its state and time under the step, keeps the steps disabled while it runs, applies its
reply once done (the burn's with the release delay it was started with), and picks jobs up when it is opened again.
With `SEQNET_PROOFS=real`: 13 of 13 steps, 64 checks; `lez_core` proved the private claim in 247.1 s (LEZ block 74)
and the burn in 222.9 s (block 97) inside Basecamp, the sequencer verifying each receipt; the release paid 0.000997 ETH
to the fresh address. 191 calls of the view (162 polls), the longest 3,176 ms, none cut. During each proof a second
start of the step answered the running job, and a `scan` was refused at once. 25 pictures of Basecamp's window, two of
a job proving and two of it settled. On the final code (`3b7ffe1d`) the round trip passed again with the view closed
and opened again (Basecamp's Apps Inspector) 6 s into the burn's proof: the new view found the running job and
completed the burn and the release; the first such run had found one call of the reopened view refused, which
`3b7ffe1d` fixed ([`real-proofs-jobs/final/`](results-basecamp-roundtrip/real-proofs-jobs/final/README.md)). Ethereum
stays behind the stand-in `seqnet-chain`, the vault's verifier the mock.

## 16 November 2026: the private round trip inside Logos Basecamp

**Logos Basecamp itself ran the mini-app with its core modules, and a user's private round trip ran in the view
on a running LEZ devnet** ([`results-basecamp-roundtrip/`](results-basecamp-roundtrip/README.md), branch
`basecamp`, the Modal build box). Basecamp at its pin `b642a9a` was built from source (`nix build …#dev`, 950 s;
`scripts/basecamp-build.sh`) and run headless in its standalone mode. It opened the view; its Module Inspector
loaded `ferrata_bridge_module` and `lez_core` (patched); the view called the module through the `logos` object
Basecamp injects (`callModuleAsync`). Every user step was a click through Basecamp's QML inspector, and after each
step the chains were read directly (cast on anvil, `seqnet-txs` on the sequencer): connect, with the account and
the Ethereum balance equal to anvil's; the registry before and after the operator's import, with the caps; a private
deposit on the defaults; recovery from the seed; the private claim once its random delay had passed (LEZ block 30);
a private burn to a fresh address (block 31); the checkpoint; the release by a relayer, 0.000997 ETH to the fresh
address. **13 of 13 steps, 50 checks**, dev-mode proofs, 23 pictures of Basecamp's window. LEZ went through
`lez_core` (167 of 167 bridge-state reads and 17 of 17 private-holding reads equal to the plain path's; the claim
and burn proven and sent by it inside Basecamp), Ethereum through the stand-in `seqnet-chain`
(`e2e/runs/seqnet-basecamp-2/`).

For Basecamp the view now parses the module's reply a second time (Basecamp's bridge serialises the module's
string result as a JSON string), reads the host's error payload, calls asynchronously where the host can, and
shows both sides' balances. Not reached: the round trip with real proofs inside Basecamp. Measured with
`SEQNET_PROOFS=real`, `lez_core` proved the claim and the burn for real inside Basecamp and the sequencer applied them
(LEZ blocks 73 and 97). But Basecamp cut the view's calls at 20 s (`timed out after 20000ms`: logos-protocol's default
`Timeout`, which `LogosQmlBridge::callModuleAsync` does not override), the view had said nothing changed (now: "the
step may still complete"), and the run stopped before the release
([`results-basecamp-roundtrip/l4-real-proofs/`](results-basecamp-roundtrip/l4-real-proofs/README.md)). Suites run again on this view: `results-basecamp-roundtrip/regression/`.

## 23 November 2026: RFP-022 U.8's failure modes on the banner (what `results/` holds now)

On the Modal build box: `scripts/box-test.sh` ran the view's 27 test functions, **27 passed in 40.5 s**
(`results/qmltest.txt`, `results/qmltest.xml`), 48 screenshots (`results/screenshots/`, cropped palette PNGs by
`scripts/shrink-screenshots.py`), the backend's 76 calls in 15.0 s (`results/calls.jsonl`, 15 of them the refusals the
test asks for). Added to the round trip below: each of U.8's failure modes as the user sees it, the banner's message
(what to do) and code, the refusal found before sending and naming no deposit or burn: unsupported token, invalid
amount, cap exceeded, verification failure (a claim's 249 and 252, a release's proof rejected by the verifier),
insufficient finality (a claim before the import, a release before the checkpoint), already claimed and frozen
(`test_00` the message table, `test_06b`, `09`, `09b`–`09f`, `10b`, `11c`, `12`); and the admin view's banner kept
after a failed step (`test_12`). Through the devnet backend, as below, not `ferrata_bridge_module`; the unsupported
token, the invalid amount, the cap, the rejected proof and the redirected claim are direct calls with the devnet's
named levers (the view offers ETH and its denominations only). Mutants that break each check:
[`e2e/runs/cli-u8/`](../../e2e/runs/cli-u8/README.md).

## 12 November 2026: the private round trip on the defaults (B3's done gate)

**A user's private round trip runs through the mini-app on its defaults, headless, against the devnet.**
Branch `miniapp-private`, Modal build box (Modal, the same container image as below):
`scripts/box-test.sh` drove the view through a private deposit with the defaults, the relayer's root
import, the claim's random delay let pass on the devnet's clock (not overridden), a **private claim settled**,
a **private burn settled** to the fresh address the view generated, with the release's random delay on, the
checkpoint, the release's delay let pass, and the **release by a relayer paying 0.1 ETH to that fresh
address**. No default was changed on that path. **17 of 17 test functions passed in 34.4 s**
(`results/qmltest.txt`, `results/qmltest.xml`; the whole script 45 s), 40 screenshots
(`results/screenshots/`), the backend's 66 calls in 12.5 s (`results/calls.jsonl`, 7 of them the refusals
the test asks for). These are that day's numbers; `results/` now holds the run of 23 November (above).

How the private transactions settle: the devnet gives the harness the wallet's own keys. LEZ's wallet
derives private account `/0` from the seed's key tree as an authorization secret key and a viewing secret
key `(d, z)`; those are the fields of the harness's `TestPrivateKeys`, and the account is of kind `Regular`,
so the ids agree (checked at start against the CLI's derivation for `/0`–`/7`). The harness proves each
transaction with `lee::execute_and_prove` (LEZ's privacy-preserving circuit; the witness is
`RegularKey::Authorized(ask)`, as LEZ's wallet builds it for an authorizing private account) and settles it
through LEZ's private settlement; the devnet then checks its own view of the account after it (balance less
the fee reserve, the bridge state, the nonce) against the chain's commitments
(`harness::bridge::private_post_state`).

| Private transaction (dev mode run) | Call | `execute_and_prove` | Receipt | Transaction | Execution gas |
|---|---:|---:|---:|---:|---:|
| private claim of leaf 0 under `root(cid, 2)`, 0.1 ETH to private `/0` | 1,421 ms | 927 ms | 4,429 B (dev-mode receipt) | 8,745 B | 549,233 |
| private burn of 0.1 ETH from private `/0`, note for a fresh recipient | 895 ms | 582 ms | 3,985 B (dev-mode receipt) | 7,857 B | 515,773 |

What the two transactions make public (`proof.public_outputs` in `results/calls.jsonl`): neither names
private account `/0` or any of the seed's sixteen accounts (`/0`–`/7` of each kind; searched in the
transactions' public outputs): the sender appears as an alias, a different one in each (`FyS5gqh7…` in the
claim, `F8Fp7CDT…` in the burn). The claim names its nullifier's account (so recovery from the seed shows
the deposit as claimed, read the way the CLI reads a sequencer's blocks: the public accounts of private
transactions); the burn delivers its note in the clear to `token(cid, ETH)`, where the checkpoint collects
it; the release then pays the note's recipient.

Proofs: in this run the private transactions' receipts are dev-mode ones (`RISC0_DEV_MODE=1`, as the
harness's tests run). **The same test with the private claim and burn proven for real also passed, 17 of 17**
(`FERRATA_DEVNET_PROOFS=real`, `results-real-proofs/`: `qmltest.txt`, `calls.jsonl`, four screenshots):
dev mode off for those two calls, r0vm 3.0.5 on the box's CPU (about 28 cores busy, 9 GB resident during the
claim's proof), each receipt verified by LEZ's settlement (`check_privacy_preserving_circuit_proof_is_valid`)
before the state changed. The rest of the harness ran in dev mode, as its tests do.

| Private transaction (real-proof run) | Call | `execute_and_prove` | Receipt | Transaction | Execution gas |
|---|---:|---:|---:|---:|---:|
| private claim of leaf 0, 0.1 ETH to private `/0` | 242,953 ms | 242.4 s | 227,442 B | 231,758 B | 549,233 |
| private burn of 0.1 ETH from private `/0` | 209,738 ms | 209.4 s | 226,998 B | 230,870 B | 515,773 |

The test functions took 484.9 s in all (the two proofs 451.8 s of it); the release paid 0.1 ETH to the
private burn's fresh recipient in block 17; the box's estimate for the command: 540 s, 3.35 core-hours, about
0.19 USD. The receipt sizes match the earlier real proofs of these two transactions on an L4 GPU (227,477 B
and 226,978 B, `lez/BRIDGE-RESULTS.md` §7, an older image of the program).

Stand-ins on this path (named in the backend's replies): the private account's native balance for LEZ fees
is put in the state on its first private transaction (as the harness's tests fund theirs); the transactions
have the harness's bare shape with fresh randomness (LEZ's wallet also pads with dummy inputs); the
relayer's imports, the checkpoint prover and the release proof are the stand-ins of the run below (the
vault's mock verifier).

| Test (changed or new on this branch) | What it drives and checks | Screenshots |
|---|---|---|
| `test_07_private_claim` | private claim pre-selected; held by the deposit's random delay (12 h in this run); the devnet's "let the delay pass" (anvil's clock and the view's clock moved 43,197 s; no override); the private claim settled: private balance `/0` 0.1 ETH, the deposit shown as claimed by recovery | 14, 15, 15b, 16 |
| `test_10_private_burn_on_defaults` | private holder, fresh recipient and the release's random delay on by default, indicator "Minimal"; the private burn settled: burn 0, private balance 0, the note for the fresh recipient | 20, 20b, 21 |
| `test_11_checkpoint_and_private_release` | checkpoint; the release held by its delay (8 h in this run), let pass (28,800 s; no override); the relayer's release pays 0.1 ETH to the private burn's fresh recipient (block 17); positions: 0 not released | 22, 22b, 23, 23c |
| `test_11b_public_burn_and_release` | the public path keeps its warning: a public burn is refused until acknowledged (indicator "High"); then burned, checkpointed and released to another fresh address; a second release refused ("Already released") | 21b, 21c, 23b, 24 |
| `test_14_wallet_provider_seam` | a stub provider set as `walletProvider` gets the connect (naming the dApp) and the approval of a deposit; the built-in prompts are not used; the deposit goes through | 32 |

The other tests are those of the run below, unchanged.

Packages of this view (`scripts/nix-build.sh` on the box, commit `242625c`'s view):

| Target | Seconds | `.lgx` bytes | SHA-256 |
|---|---:|---:|---|
| `nix build .#lgx` (a fresh container: the builder's inputs fetched again) | 81 | 17,545 | `da228d7fd4624c39da16fcbb363c4475c0cd10c61d4e2e124ee095096871589d` |
| `nix build .#lgx-portable` | 5 | 17,543 | `c771e82669e2fa25704599a53077f0d3a89bf53f186c37be2c97c7c5ed4da9eb` |

Not re-run on this branch: Basecamp (`basecamp-ui-tests.sh`): it needs Basecamp's `#dev` built from source
in the container first (1,656 s last time) and that command is not recorded in full; its 10 of 10 below is
for the view before these changes (which add eight properties, seven functions, two buttons, two checkboxes
and a label, and no import).

## 11 November 2026: the first runs (branch `miniapp`)

**Built as a Logos ui module, loaded in Logos Basecamp, and driven through every screen.** On 11
November 2026, on the project's Modal build box (the same gVisor container as `modules/RESULTS.md`, Ubuntu
24.04), the mini-app was packaged by logos-module-builder as a dev and a portable `.lgx`; Basecamp
(pinned, standalone mode, headless) installed and opened it and its own UI test passed 10 of 10; and its
view was driven by Qt Quick Test against the devnet backend: **15 of 15 test functions passed** (`results/qmltest.txt`,
`results/qmltest.xml`), with a screenshot of each state (`results/screenshots/`, 33 files). Every number
below comes from those runs' logs; the commit that ran is the one that adds `results/`.

## Where it ran

| Input | Version |
|---|---|
| Qt (the view under test) | Ubuntu 24.04's Qt 6.4 (`qmltestrunner`, `QT_QPA_PLATFORM=offscreen`, `QT_QUICK_BACKEND=software`) |
| logos-module-builder (packaging) | `5f74cdb3cecacde054c68d72fcc0d2cfe2195900`, the `flake.lock` of `ferrata_attest_module`; Nix 2.28.3 |
| Ethereum | anvil (Foundry 1.3.6), chain id 1, local; FerrataVault v5 deployed by `vault/script/CliLocal.s.sol` with the vault tests' `MockRiscZeroVerifier` |
| LEZ | the LEZ harness (`lez/harness`, LEZ v0.4.0-rc1 `1806e87`), the bridge guest in RISC Zero dev mode |
| Backend | `cli/examples/miniapp_devnet.rs` over the `ferrata` CLI's library |

## Package

| Target | Seconds | `.lgx` bytes | SHA-256 |
|---|---:|---:|---|
| `nix build .#lgx` (first build, with the builder's Logos inputs) | 162 | — (skeleton view) | — |
| `nix build .#lgx` (final view, commit `358714b`) | 4 | 16,120 | `eebfa58f1820833755fea1f6ce02e219694df24810c11681f3bc833ed26d5e18` |
| `nix build .#lgx-portable` (final view) | 6 | 16,108 | `84fe671077c2a9685f5e6301bcc1a5bde97e4aabe6444d3ca2b30d757a3b38c1` |

The package holds the view (`qml/`), the icon and the module's `metadata.json`; there is no compiled
plugin (a QML-only `ui_qml` module).

## The headless run

`scripts/box-test.sh`, the run committed here (on `eec1e1d`, the last of ten passing runs; the packages
below are from `358714b`, whose view is the same): the 15 test functions in 29.3 s; the whole script 43 s. The backend
answered 54 calls in 10.6 s in total (the slowest 1.71 s, a root import: the harness executes the bridge
guest), 7 of them the refusals the test asks for.

| Test | What it drives and checks | Screenshots |
|---|---|---|
| `test_01_connect` | no deposit before the wallet is connected; the connect prompt names the dApp and lets the user pick the account | 01, 02 |
| `test_02_endpoints` | U.09: the Ethereum RPC switched to an address that does not answer (the next call reports it, the header names it); a LEZ sequencer endpoint the devnet does not have is refused with the reason; back to anvil: "Connected: chain 1" | 03–05 |
| `test_03_registry` | ETH registered, three denominations with their four fees, escrow, the vault's deposit and release caps and the bridge program's mint and burn caps with utilisation | 06 |
| `test_04_deposit_private_by_default` | U.05/U.06: private destination, random delay and relayer pre-selected; the leak indicator reads "Minimal"; the approval prompt (dApp, account, asset, amount + fees, destination, effects); a real anvil transaction (leaf 0, block 6, 0.1001 ETH) | 07–09 |
| `test_05_public_needs_an_explicit_action` | a public destination disables the deposit until acknowledged and turns the indicator to "High"; a deposit rejected in the wallet sends nothing ("Rejected in the wallet"); then a public deposit (leaf 1) | 10–12 |
| `test_06_positions_from_the_seed` | recovery: both deposits found from the seed alone (whole deposit list against `depositRoot`), waiting for the root import | 13 |
| `test_07_private_claim` | the relayer's root import (stand-in); the private deposit held by its random delay until the user overrides it; the private claim is built (`ClaimPrivately`, its nullifier, root of 2 leaves) and shown with what it would publish; **not settled** (below) | 14–16 |
| `test_08_plaintext_claim` | plaintext only after its acknowledgement; settled by the bridge program in the harness: 0.1 ETH at public `/0` | 17 |
| `test_09_error_messages` | U.08: a second claim → "Already claimed" (code 250), with no hash in the message; a burn above the holding → "Insufficient balance" (254) | 18, 19 |
| `test_10_burn` | the private burn (default) is answered "not settled here"; a public burn after its acknowledgement debits the holder | 20, 21 |
| `test_11_checkpoint_and_release` | the checkpoint (stand-in prover: candidate, beacon, finalisation on the vault) and the release sent by a relayer: **0.1 ETH paid to the fresh recipient address** the view generated (block 16); a second release refused ("Already released"); positions show 0 not released | 22–24 |
| `test_12_admin` | roles and floor; `SetCap` below the floor refused by the program (240 `BAD_MESSAGE`); `SetCap` 11 ETH accepted; `Freeze` of mints for 1 h by the freeze authority; the next plaintext claim refused with 242 `FROZEN` ("Frozen: …"); the registry shows the freeze; the freeze authority lifts it and the same claim goes through (leaf 2, root of 3 leaves) | 25–30b |
| `test_13_privacy` | the four steps' public/private tables | 31 |

The settled round trip, public by explicit choice: deposit (anvil) → root import (harness) → plaintext
claim (harness) → public burn (harness) → checkpoint (anvil, mock verifier) → release (anvil, paid to a
fresh address by a relayer).

## In Logos Basecamp

`scripts/basecamp-ui-tests.sh` runs `nix run .#ui-tests` with Basecamp pinned to
`b642a9aba732b80343a3a7b622dad8c65e534bdb` (it reports `LogosBasecamp version pre-release-b642a9a (dev
build)`): Basecamp's standalone mode installs the module's `.lgx` (unsigned: a warning), opens the view in
its sandbox and runs `tests/ui-tests.mjs` through its QML inspector (logos-qt-mcp), headless, with the
devnet backend running beside it. **10 of 10 passed in 14 s** (`results/basecamp-ui-tests.txt`):

- the view loads and says that it calls the core module `ferrata_bridge_module` (the host injected `logos`,
  so the view chose that transport; the module is not built, and the view calls it only on a user action);
- after each of the eight tabs is clicked, its texts are found (the inspector searches the view's tree,
  hidden pages included: this shows that every screen is built inside Basecamp, not what is drawn);
- **Basecamp's sandbox keeps a ui module off the network.** Switched to the devnet backend, the view's
  call is refused by Basecamp, which logs `logos.basecamp.sandbox: Blocked network POST
  "http://127.0.0.1:18600/rpc" [plugin=ferrata_bridge_ui]: sandboxed ui_qml modules may not use the
  network.`; the view reports "Not switched: Endpoint unreachable" and, inside Logos, says why. So inside
  Logos every flow has to go through a core module (`ferrata_bridge_module`), which is the design; the
  HTTP devnet path serves the view outside Logos, where the flows above were driven.

Building Basecamp's `#dev` from source on the box first took 1,656 s (one command of 1,711 s, 11.9
core-hours, about 0.65 USD by the box's estimate). Before the passing run: tabs clicked by object name
(the inspector's `click` matches text: 1 of 9); Basecamp's sidebar has its own "Settings", which the
mini-app's tab of that name lost to (the tab is now "Endpoints"); prefixes given to `expectTexts`, which
matches whole texts; and `app.json`, a helper of Basecamp's own tests, not of the framework.

## What failed on the way

Four runs before the first passing one, all on the test or the view, none on the backend's methods:

1. Qt 6.4's `grabImage(...).save()` returns nothing: the test's check on its return value failed every
   function at its first screenshot, and the rest cascaded (2 of 15 passed).
2. An acknowledgement checkbox kept its tick when the choice above it changed, so the next click unticked
   it (the burn's); fixed in the view: each acknowledgement resets with its choice.
3. `Freeze` named the newest record of the devnet's import blocks, which carry no explicit Ethereum time,
   and reverted without an exit code; it now names a record of Ethereum's current time
   (`World::finalize`, as the bridge program's own tests do), and reverts are reported with their reason.
4. A click placed before the layout had settled (an error banner closing moved the form); and the release
   check compared a lowercase address with the backend's EIP-55 spelling.

`cli/Cargo.lock` lacked `ferrata-lez-holdings-wire`, a dependency of `lez/harness` (the CLI's
dev-dependency); the first build added it (12 lines).

## Not done

- Superseded on 12 November (above): "the harness proves private transactions only for its test keys" was
  a key-format mapping, not a missing feature: the wallet's keys are the harness's `TestPrivateKeys` fields.
  The private round trip now settles on the devnet, and on a running LEZ sequencer through `ferrata_bridge_module`
  in a headless Logos Core (13 November, [`e2e/seqnet/RESULTS.md`](../../e2e/seqnet/RESULTS.md) §2, EVIDENCE
  row 103), the chains' I/O through a stand-in for Logos Core's own modules (`README.md`, "Not done").
- What `README.md` lists under "Not done".

## Reproduce

```sh
modules/ferrata_bridge_ui/scripts/nix-build.sh      # the packages
modules/ferrata_bridge_ui/scripts/box-test.sh       # the headless run; results/ is rewritten
FERRATA_DEVNET_PROOFS=real QT_TIMEOUT=3600 modules/ferrata_bridge_ui/scripts/box-test.sh   # private transactions proven for real; results-real-proofs/
python3 modules/ferrata_bridge_ui/scripts/shrink-screenshots.py modules/ferrata_bridge_ui/results/screenshots   # optional: crop, palette PNG
```
