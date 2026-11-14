# e2e/seqnet: the user's private round trip on a running LEZ sequencer

**Prototype, local devnet only: nothing is sent to any public network.**

The user's private round trip of RFP-022 (a private deposit, a private claim, a private burn to a fresh
address, the release by a relayer), on a running LEZ v0.4.0-rc1 sequencer and Logos Blockchain node with
the final programs ([`docs/image-ids.md`](../../docs/image-ids.md)) and FerrataVault v5 in mode (B) on a
local anvil, three ways:

1. through the `ferrata` tool's sequencer path (`cli/`: LEZ's wallet library for the private sends and the
   wallet's sync, `lez::visible_calls` for a private burn's note): `run.sh cli NAME`;
2. through the mini-app's view (`modules/ferrata_bridge_ui/qml/Main.qml`) driving `ferrata_bridge_module`
   inside a headless Logos Core, the view driven by Qt Quick Test as in EVIDENCE rows 86 and 87:
   `run.sh miniapp NAME`;
3. through `ferrata_bridge_module` inside a headless Logos Core **beside Logos Core's own LEZ module, `lez_core`**
   (logos-execution-zone-module at its merged v0.4 state, `cc73ebf`, built from its source, as merged and with
   [`modules/patches/lez_core-cc73ebf-authorizes-public-pda.patch`](../../modules/patches/lez_core-cc73ebf-authorizes-public-pda.patch)):
   the module's wallet session routed through `lez_core` (`wallet lez`): every bridge actor state it reads (the
   configuration, the token, the imported root, the deposit's nullifier, the public holders), the wallet's private
   holding, and the private claim and burn (executed and proven by LEZ's wallet inside Logos Core), each beside the
   plain path's (`seqnet-chain`); the module's calls made with `logosctl` in the mini-app's order (no view):
   `run.sh lezcore NAME` (`hook-lezcore.sh`, after `run.sh build-lezcore`).

Results: [`RESULTS.md`](RESULTS.md); each run's records in `e2e/runs/seqnet-NAME/`.

## What runs where

Everything runs in one command on the Modal build box (`spike/modal-build/mrun`), in this order
(`drive.sh`):

| Step | What | Who does it |
|---|---|---|
| zone | LEZ session A (`lez/crown/session.sh a`, `CROWN_CHAIN=stand-in`): a new zone (sequencer + node, LEZ's local configuration, dev-mode leader proofs, `--create-channel`), the attestation and bridge programs deployed, the light-client instance initialised; the zone stopped and saved | the operator (`crown-driver crown-a`, as the crown runs) |
| vault | anvil (chain id 11155111, the stand-in light client's configuration being Sepolia's), the vault tests' `MockRiscZeroVerifier`, FerrataVault v5 in mode (B) with session A's `configId` and `genesisState` (`e2e/crown/sepolia.sh deploy`) | the deployer (an address no one holds a key for, impersonated by anvil) |
| deposit | `ferrata deposit` (cli) or the view's Deposit screen (miniapp) on the defaults: one denomination to private account `/0` | the user |
| import | capture of the vault after the deposit; the zone restarted (`session.sh x`, the hook below runs while it runs); `crown-driver crown-b --operator-only`: the stand-in committee's finality update for the deposit's block, bridge `Init`, `Register` ETH, `ImportRoot` | the operator and the relayer |
| fees | `seqnet-fund`: native LEZ to the user's private account `/0` for its fees | **stand-in** for the gas drop |
| claim | the claim held by its delay, the delay let pass (cli: anvil's blocks; miniapp: the view's clock), the private claim on the defaults, executed and proven by LEZ's wallet, verified and applied by the sequencer | the user |
| burn | a private burn of that holding to a fresh address, proven by LEZ's wallet | the user |
| checkpoint | the node's final blocks, `realblocks-crown` (the attestor's signature and the `checkpoint_b` input over the zone's blocks), `checkpoint_b` executed (`ferrata-e2e exec`: its journal equals the native one), the mock verifier told to accept that journal, the relayer's `submitCandidate`, Δ let pass on anvil's clock, the relayer's `finalizeAttested` | the checkpoint prover and the relayer (**stand-in**: the mock verifier instead of a Groth16 proof) |
| release | the release held until its delay, then the wallet's rule (the newest accepted root whose delay has passed) and the relayer's job; `ferrata-relayer` sends `release` and is paid the relay fee | the user, then the relayer |
| records | every Ethereum transaction (`ethtxs.py`), every LEZ transaction (`seqnet-txs`), what the user ran and saw (`user/`, or the view's screenshots and calls) | |

## Stand-ins, named

- **Ethereum consensus**: a committee the driver holds finalizes anvil's blocks for LEZ's light client
  (`crown-driver`'s stand-in chain, as `crown-rehearsal-5`); the vault's state is anvil's own, proven by
  `eth_getProof` under the header the committee signs.
- **The vault's verifier**: the vault tests' `MockRiscZeroVerifier` behind the emergency stop; a stand-in
  prover tells it to accept the exact journals (the checkpoint's, computed by `checkpoint_b` executed over
  the zone's blocks, and the release's, computed by the wallet's rule). No Groth16 proof is made.
- **The gas drop**: `seqnet-fund` sends native LEZ from a funded genesis account of LEZ's local network to the
  user's public account `/0`, which the user's wallet moves to private account `/0` (this links the two
  accounts for that amount; the gas drop's sealed credit to a one-time account is not built in LEZ's wallet).
- **Time**: the claim's and the release's delays are let pass on the devnet's clocks (anvil's blocks and time;
  the view's own clock in the mini-app), never overridden.
- **Logos Core's own modules** (miniapp): `ferrata_bridge_module` runs inside Logos Core and decides everything
  that is the bridge's; the chains' state and the signing and sending come from `seqnet-chain`, standing in for
  `eth_rpc_module`, `keystore_module`/`tx_sender_module` and `lez_core`, which are not available here (in the
  `lezcore` mode `lez_core` is there: the module's LEZ reads and its private claim and burn go through it, and
  `seqnet-chain` is kept beside them, for the zone's history and for Ethereum). The
  view's `logos` object is the test runner's, whose `callModule` is `logosctl call` against the daemon.
- **Bedrock**: the node's leader proofs are in dev mode (LEZ's local configuration, as every crown session).

## Commands

```sh
cd spike/ferrata-wt-<name>                       # a worktree; the inputs beside it in spike/tmp/miniapp-sequencer/inputs/
../modal-build/mrun --profile <yours> --put ../tmp/miniapp-sequencer/inputs -- 'bash e2e/seqnet/run.sh build'
../modal-build/mrun --profile <yours> -- 'bash e2e/seqnet/run.sh build-miniapp'        # Nix: the module, logosctl
../modal-build/mrun --profile <yours> --timeout 10800 -- 'bash e2e/seqnet/run.sh cli cli-N'
../modal-build/mrun --profile <yours> --timeout 10800 -- 'SEQNET_PROOFS=real bash e2e/seqnet/run.sh cli cli-N'
../modal-build/mrun --profile <yours> --timeout 10800 -- 'bash e2e/seqnet/run.sh miniapp miniapp-N'
../modal-build/mrun --profile <yours> --timeout 3600 -- 'bash e2e/seqnet/run.sh build-lezcore core'   # lez_core as merged (Nix)
../modal-build/mrun --profile <yours> --timeout 3600 -- 'bash e2e/seqnet/run.sh build-lezcore rest'   # patched; the module
../modal-build/mrun --profile <yours> --timeout 10800 -- 'bash e2e/seqnet/run.sh lezcore lezcore-N'
```

The inputs: the crown-live-3 LEZ bundle (`bundle-crown-live-3.tar.zst`, sha256 `0013bab1…`: the sequencer, the
node, r0vm and the two LEZ programs of [`docs/image-ids.md`](../../docs/image-ids.md)) and the final R0BF
programs (`r0bf/checkpoint_b.bin`, `r0bf/release.bin`), which `scripts/reproduce-image-ids.sh build` rebuilds.

| File | What |
|---|---|
| `run.sh` | `build`, `build-miniapp`, `cli NAME`, `miniapp NAME`, `build-lezcore [core\|rest]`, `lezcore NAME` |
| `drive.sh` | one run: session A, anvil and the vault, the user's keys, the deposit and capture (cli), the zone with the hook, Ethereum's transactions |
| `hook-cli.sh`, `hook-miniapp.sh`, `hook-lezcore.sh` | what runs while the zone runs (`lez/crown/session.sh x`) |
| `common.sh` | the steps the hooks share: the operator's import, the gas drop's stand-in, the checkpoint, the relayer, the records |
| `ops.sh` | the same steps for the mini-app's test, run between the user's actions |
| `ethtxs.py` | anvil's transactions, named |
| `cli/src/bin/seqnet_fund.rs`, `seqnet_txs.rs`, `seqnet_chain.rs` | the gas drop's stand-in; the zone's transactions; the chain service of the mini-app's run |
| `modules/ferrata_bridge_ui/qmltest-logos/` | the Qt Quick Test runner with Logos Core behind the view, and the test |
