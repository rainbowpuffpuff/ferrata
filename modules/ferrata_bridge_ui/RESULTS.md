# `ferrata_bridge_ui`: results

**Built as a Logos ui module, loaded in Logos Basecamp, and driven through every screen.** On 11
November 2026, on the project's Modal build box (the same gVisor container as `modules/RESULTS.md`, Ubuntu
24.04), the mini-app was packaged by logos-module-builder as a dev and a portable `.lgx`; Basecamp
(pinned, standalone mode, headless) installed and opened it and its own UI test passed 10 of 10; and its
view was driven by Qt Quick Test against the devnet backend: **15 of 15 test functions passed** (`results/qmltest.txt`,
`results/qmltest.xml`), with a screenshot of each state (`results/screenshots/`, 32 files). Every number
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
| `nix build .#lgx` (final view, commit `5588726`) | 5 | 15,923 | `46e19a5c47841cc0be7bc7df4c262bfe09e24389bab84e01641e3c27b3acdc07` |
| `nix build .#lgx-portable` (final view) | 8 | 15,919 | `904f54338e780f012e57c1bca45b693592be0339b0c485082076e91f795a3429` |

The package holds the view (`qml/`), the icon and the module's `metadata.json`; there is no compiled
plugin (a QML-only `ui_qml` module).

## The headless run

`scripts/box-test.sh`, the run committed here (on `5588726`, the last of seven passing runs): backend
built in 10 s (warm), up 2 s later; the 15 test functions in 28.6 s; the whole script 41 s. The backend
answered 53 calls in 10.4 s in total (the slowest 1.62 s, a root import: the harness executes the bridge
guest), 6 of them the refusals the test asks for.

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
| `test_11_checkpoint_and_release` | the checkpoint (stand-in prover: candidate, beacon, finalisation on the vault) and the release sent by a relayer: **0.1 ETH paid to the fresh recipient address** the view generated (block 16); positions show 0 not released | 22–24 |
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

- **The private round trip on the devnet.** The harness proves and settles private transactions only for
  its test keys (`TestPrivateKeys { ask, d, z }`, account ids from `for_regular_private_account`), while
  the wallet's private accounts come from LEZ's key tree (`KeyChain { authorization_secret_key,
  viewing_secret_key }`, ids from `for_private_account(npk, vpk, kind)`), so the seed's private claim
  cannot be settled there. The view builds it and stops. B3's done gate (a new user completes a private
  round trip on defaults alone) needs a LEZ sequencer devnet with the wallet's proving.
- What `README.md` lists under "Not done".

## Reproduce

```sh
modules/ferrata_bridge_ui/scripts/nix-build.sh      # the packages
modules/ferrata_bridge_ui/scripts/box-test.sh       # the headless run; results/ is rewritten
python3 modules/ferrata_bridge_ui/scripts/shrink-screenshots.py modules/ferrata_bridge_ui/results/screenshots   # optional: crop, palette PNG
```
