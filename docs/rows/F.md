# Stream F: the Ethereum side through Logos Core's own modules, proposed statuses (after the review's fixes)

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. Branch
`rfp-logos-core-eth` from `f13faee2`. Nothing pushed or sent to a public network. Runs on the Modal build box (Linux
x86-64); local chains only (anvil, chain id 11155111; a LEZ v0.4.0-rc1 zone we ran). This replaces the first version
of this file, whose U.1 and U.2 "shown with conditions" did not meet the strict rule (review-rfp-F, M3 and M4).

What the module does now: `ferrata_bridge_module` reads the vault through Logos Core's `eth_rpc_module` and sends every
user and admin Ethereum transaction through Logos's send stack (`tx_sender_module` -> one approval at the Logos signer,
`evm_signer_cli` headless -> `keystore_module` signs -> broadcast through `eth_rpc_module`); it holds no Ethereum key.
Each call is sent with a gas limit of the stack's own estimate (`tx_sender_module.prepare`) + 25% + 20,000, or the
caller's own; every call's receipt is read back and a status-0 receipt is reported as a revert, never as done (module
replies, session jobs, the view). LEZ admin messages are built at the role's own actor with the program's accounts.

## RFP-022

| ID | Proposed | Clause by clause | Evidence (path; command; commit) |
|---|---|---|---|
| U.1 | **in part** | *Depositing* through the module and the send stack, ETH and an ERC-20 (`approve` + `deposit`, one approval): **shown**. *Claiming on LEZ, redeeming*: **shown** through `lez_core` with our patch (EVIDENCE 113, 114, 125, 126). *Releasing on Ethereum*: **shown** for the session's release through the stack; the module-level `release` with `send` (`user.rs release_and_send`) is not run. *Recovering a position*: **shown** (`scan`; EVIDENCE 59, 84). *Reading the registry*: **shown**, every registered token, through `eth_rpc_module` (route `direct`: on anvil the verified proxy cannot run, not exercised). *Administering the registry and permitted amounts on the vault*: **shown** for `registerToken`, `setTokenCap`, `setDepositsEnabled` (delist, list), `announceFees`, `setReleaseDelay`, the token-deposit freeze and its lift, `acceptAdmin`; a send forced to revert after approval is reported as reverted and changes nothing; **not run**: `setGlobalCap`, `raiseDelta`, `transferAdmin`, `proposeRoles`/`applyRoles`, `revokeFrom`, `retire`, the global freezes (encoded; signatures checked against the vault by the review). *Administering on LEZ*: `SetCap` built by the module at the admin's own actor, **accepted** by the bridge program on our LEZ v0.4.0-rc1 sequencer and read back from `token(cid, ETH)`; the same message from a non-admin account **refused** (reverted in a block); signed by a stand-in (the chain service's LEZ wallet holding the run's role key), fee paid by the user's public account; `Freeze`, `Pause`, `Tick`, `Sunset` built (unit test) but **not sent**: their time record in the Gloas layout needs the execution block's RLP header, which the module does not fetch. *All off-chain software as Logos modules*: **no** — `seqnet-chain` (not a module) still gives the session's seed and accounts, the zone's history, the release's stand-in prover and the LEZ admin signer. | `modules/results/bridge-send-stack-r2/` (`bash modules/scripts/bridge-send-stack.sh run <out>`, three runs; `native/`: `bash modules/scripts/bridge-eth-native.sh <out>`); `e2e/runs/seqnet-logoseth-3/` (`SEQNET_TMP=<scratch>/seqnet SEQNET_SHORT=/var/tmp/rfx bash e2e/seqnet/run.sh logoseth logoseth-3`, `tst_miniapp_logos.qml` test_03-07c); unit tests `MODULES=ferrata_bridge_module bash modules/scripts/native.sh` (32). Commits `787b5448` (H1), `ffcf0da3` (M1), `f1bcb9a1` (registry, admin panel). |
| U.2 | **in part** | *Deposit and redemption flows end to end* in the mini-app, its Ethereum side through Logos Core's modules (reads `eth_rpc_module`; deposit, release, vault admin through `tx_sender_module`, each approved at `evm_signer_cli`): **shown in a headless Qt Quick Test host**, not inside Basecamp with Logos's approval app (`evm_signer_ui`): no run has both, a gap we can close, not a condition. *Position recovery*: **shown**. *Registry view: supported tokens, permitted amounts, caps, utilisation*: **shown** on this path (every token the vault registered, each in its units; LEZ's caps for ETH). *A UI for the admin functionality*: vault: a panel for every vault action, run for `setTokenCap` (and a forced revert, reported where the admin acted), the freeze and lift, delisting and listing a token, a fee schedule, a release delay, each read back; LEZ: `SetCap` from the admin tab, accepted and read back; LEZ `Freeze`/lift not on this path (hidden, with a note: not sent). The privacy panel says the release goes from the user's own release account on this path (M2); routing it through `ferrata_relayer_module` is not done. | `e2e/runs/seqnet-logoseth-3/` (screenshots 20-29; `miniapp/qmltest.txt`). Commits `787b5448`, `325c104e` (M2), `ffcf0da3`, `f1bcb9a1`. |
| R.10 | in part (unchanged) | Not advanced. The round trip against a standalone sequencer in CI needs `lez/crown/session.sh` on LEZ's mock Bedrock built from source and a rule (B) checkpoint input from the standalone sequencer's own blocks (stream G's rows file agrees). | — |

## RFP-021

| ID | Proposed | Clause by clause | Evidence |
|---|---|---|---|
| U.1 (send half) | in part (unchanged) | Not advanced: the attestation module's updates and attestations would go through `lez_core.send_generic_private_transaction_with_accounts` with the patch's `public_pda` mentions (its public send has no PDA kind). | — |

## Proposed EVIDENCE rows

- **Bridge module, Ethereum through Logos Core's send stack (anvil), with gas margins and reverts reported.**
  `ferrata_bridge_module` in one headless Logos Core with Logos's `eth_rpc_module`, `fee_module`, `keystore_module`,
  `signer_manager_module`, `tx_sender_module`, `evm_signer_cli`, `evm_keystore_cli` (built at the revisions
  `tx_sender_module` f1754c3 pins): the admin registers an ERC-20, sets its cap, delists and lists it, announces a fee
  schedule and sets a release delay; the user deposits ETH and the ERC-20 (`approve` + `deposit`, one approval); a
  deposit rejected at the approver; the freeze authority freezes and lifts ETH deposits; deposits refused before
  signing while frozen or delisted; a send forced to revert after approval (its own gas limit 30,000) reported as
  reverted with nothing changed; every other send with the stack's estimate + 25% + 20,000 and gas used below it.
  Every user and admin send approved at `evm_signer_cli`, every read through `eth_rpc_module` (route `direct`);
  49 of 49 checks against `cast` in each of three runs (the cap: estimate 51,089 or 48,564, limit 83,861 or 80,705, used 50,771); native 27 of 27. Build box, 21 Nov 2026; anvil, mock verifier;
  `modules/results/bridge-send-stack-r2/`.
- **The mini-app's round trip and admin, Ethereum through Logos Core's own modules, LEZ through `lez_core`.** The view
  driven headless (Qt Quick Test) calling `ferrata_bridge_module` in one Logos Core with Logos's Ethereum send stack,
  its headless approver and custodian, and `lez_core` (patched), on a LEZ v0.4.0-rc1 zone we ran and anvil: the private
  deposit, claim and burn, the release to a fresh address (sent from the user's release account, as the privacy panel
  says), the vault's admin (accept, cap, a forced revert reported in the view, freeze and lift, delist and list, fees,
  release delay), the bridge program's `SetCap` built by the module at the admin's own actor and accepted, the same from
  a non-admin refused; the registry listing both tokens. Every user and admin Ethereum send through `tx_sender_module`
  approved at `evm_signer_cli`; outside the stack: the stand-in prover's `accept` to the mock verifier and the relayer's
  checkpoint sends; the LEZ admin message signed by the chain service's LEZ wallet holding the run's role key. Qt Quick Test 12 of 12 (10 test functions).
  Build box, 21 Nov 2026; dev-mode private proofs; `e2e/runs/seqnet-logoseth-3/`.
