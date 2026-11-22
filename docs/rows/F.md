# Stream F: the Ethereum side through Logos Core's own modules, proposed statuses (after two reviews' fixes)

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. Branch
`rfp-logos-core-eth` from `f13faee2`. Nothing pushed or sent to a public network. Runs on the Modal build box (Linux
x86-64); local chains only (anvil, chain id 11155111; a LEZ v0.4.0-rc1 zone we ran). This replaces the earlier versions
of this file: the first one's "shown with conditions" did not meet the strict rule (the first review's M3 and M4); the
second one overstated the registry's LEZ side, the LEZ admin path and some test coverage (the second review).

What the module does now: `ferrata_bridge_module` reads the vault through Logos Core's `eth_rpc_module` and sends every
user and admin Ethereum transaction through Logos's send stack (`tx_sender_module` -> one approval at the Logos signer,
`evm_signer_cli` headless -> `keystore_module` signs -> broadcast through `eth_rpc_module`); it holds no Ethereum key.
Each call is sent with a gas limit of the stack's own estimate (`tx_sender_module.prepare`) + 25% + 20,000, or the
caller's own; every call's receipt is read back: a status-0 receipt is reported as a revert, and a receipt not seen
within the wait (600 s by default) as "broadcast, not yet mined: <hash> ... do not send it again" (job `unconfirmed`,
never "failed"; the session refuses another deposit while one is unmined). The registry and the admin tab read every
token's LEZ states again through Logos Core's `lez_core` and compare them with the chain service's. LEZ admin messages
are built at the role's own actor with the program's accounts and are never sent without a person's approval: the
module hands them only to a signer that asks first (here the stand-in for the role's LEZ wallet, `seqnet-chain`, which
holds the message until a person approves exactly it with the role password) and waits for that decision.

## RFP-022

| ID | Proposed | Clause by clause | Evidence (path; command; commit) |
|---|---|---|---|
| U.1 | **in part** | *Depositing* through the module and the send stack, ETH and an ERC-20 (`approve` + `deposit`, one approval): **shown**; a deposit broadcast and not mined in time is reported as such and lands with the next block (send stack step 9). *Claiming on LEZ, redeeming*: **shown** through `lez_core` with our patch (EVIDENCE 113, 114, 125, 126). *Releasing on Ethereum*: **shown** for the session's release through the stack; the module-level `release` with `send` (`user.rs release_and_send`) is not run. *Recovering a position*: **shown** (`scan`; EVIDENCE 59, 84). *Reading the registry*: **shown**, every registered token, through `eth_rpc_module` (route `direct`: on anvil the verified proxy cannot run, not exercised), each token's LEZ side read again through `lez_core` (logoseth-4: 3 of 3 equal per token; 293 `lez_core` reads in the run, all equal). *Administering the registry and permitted amounts on the vault*: **shown** for `registerToken`, `setTokenCap`, `setDepositsEnabled` (delist, list), `announceFees`, `setReleaseDelay`, the token-deposit freeze and its lift, `acceptAdmin`; a send forced to revert after approval is reported as reverted and changes nothing; **not run**: `setGlobalCap`, `raiseDelta`, `transferAdmin` through the module (the deployment's own `transferAdmin` is the deployer's, outside the stack), `proposeRoles`/`applyRoles`, `revokeFrom`, `retire`, `applyFees`, `setTokenReleasesFrozen`, the global freezes (each encoded by the module; signatures checked against the vault by the review). *Administering on LEZ*: `SetCap` built by the module at the admin's own actor, held at the role's wallet until a person approved exactly that message with the role password, then **accepted** by the bridge program on our sequencer and read back from `token(cid, ETH)`; the same message signed by a non-admin account **refused** (reverted in a block); a `Sunset` built, held and **rejected by the person**: nothing signed or sent, the configuration not sunset. Built with unit tests and **not sent**: `Freeze` and `Tick` (`admin.rs lez_role_messages_are_rooted_at_the_roles_own_actor`), `Pause` (`pause_and_sunset_are_rooted_at_the_roles_own_actor`), whose time record in the Gloas layout needs the execution block's RLP header, which the module does not fetch; `Sunset` (same test) is not sent on purpose: it ends the configuration for good and the run goes on after it, so the run shows it held and rejected instead. The LEZ admin signer is a stand-in (the chain service's LEZ wallet holding the run's role keys, with the approval step), the fee paid by the user's public account. *All off-chain software as Logos modules*: **no** — `seqnet-chain` (not a module) still gives the session's seed and accounts, the zone's history, the release's stand-in prover and the LEZ admin signer. | `modules/results/bridge-send-stack-r3/` (five runs, 53 of 53 each; `native/` 27 of 27); `e2e/runs/seqnet-logoseth-4/` (`tst_miniapp_logos.qml` test_03-07c, 12 of 12); unit tests `MODULES=ferrata_bridge_module bash modules/scripts/native.sh` (37) and `RISC0_SKIP_BUILD=1 cargo test --features seqnet --bin seqnet-chain` in `cli/` (debug build: cli's dev-dependency `harness` turns on `lee`'s `test-utils`, refused in a release build; 1). Commands below. Commits `787b5448` (gas, reverts), `ffcf0da3` (LEZ messages), `f1bcb9a1` (registry, admin panel), `1050cb8a` (registry LEZ side), `c7ec9d36` (approval step), `ee21c585`, `69380b10` (unmined sends), `765e0f5b` (Pause, Sunset tests). |
| U.2 | **in part** | *Deposit and redemption flows end to end* in the mini-app, its Ethereum side through Logos Core's modules (reads `eth_rpc_module`; deposit, release, vault admin through `tx_sender_module`, each approved at `evm_signer_cli`): **shown in a headless Qt Quick Test host**, not inside Basecamp with Logos's approval app (`evm_signer_ui`): no run has both, a gap we can close, not a condition. The view's text for a send not yet mined ("Sent, not confirmed yet (not a failure)", never "Not deposited") is in the view, not shown in a run (anvil mines at once in the seqnet run; the module's side is shown by the send stack's step 9). *Position recovery*: **shown**. *Registry view: supported tokens, permitted amounts, caps, utilisation*: **shown** on this path (every token the vault registered, each in its units; LEZ's caps for ETH; each token's LEZ side read through `lez_core`). *A UI for the admin functionality*: vault: a panel for every vault action, run for `setTokenCap` (and a forced revert, reported where the admin acted), the freeze and lift, delisting and listing a token, a fee schedule, a release delay, each read back; LEZ: `SetCap` from the admin tab, the header naming the approval it waits for at the role's wallet (screenshot 26b), accepted and read back; LEZ `Freeze`/lift not on this path (hidden, with a note: not sent). The privacy panel and the release's step table say the release goes from the user's own release account on this path (no relayer); routing it through `ferrata_relayer_module` is not done. | `e2e/runs/seqnet-logoseth-4/` (screenshots 12, 20-29, 26b; `miniapp/qmltest.txt`). Commits `787b5448`, `325c104e` (privacy panel), `72e2c2fb` (step table), `ffcf0da3`, `f1bcb9a1`, `c7ec9d36`. |
| R.10 | in part (unchanged) | Not advanced. The round trip against a standalone sequencer in CI needs `lez/crown/session.sh` on LEZ's mock Bedrock built from source and a rule (B) checkpoint input from the standalone sequencer's own blocks (stream G's rows file agrees). | — |

Commands (Modal build box, from the worktree's root; Nix, Foundry 1.3.6):

    bash modules/scripts/bridge-send-stack.sh build <out>          # the send stack's seven .lgx at tx_sender_module f1754c3's pins,
                                                                   # the module's .lgx with LOGOS_TMPDIR=/var/tmp/bss (the hash
                                                                   # be17842b... depends on that directory), logosctl
    MODULES=ferrata_bridge_module bash modules/scripts/native.sh   # 37 unit tests and the native call example
    bash modules/scripts/bridge-eth-native.sh <out>/native          # 27 of 27
    bash modules/scripts/bridge-send-stack.sh run <out>             # 53 of 53; run five times in a row
    export SEQNET_TMP=<scratch>/seqnet SEQNET_SHORT=/var/tmp/rfx    # the crown-live-3 inputs in $SEQNET_TMP/inputs (run.sh's header)
    bash e2e/seqnet/run.sh build-logoseth                           # the zone's tools, logosctl, the send stack, lez_core patched, the runner
    cp -r <out>/check/lgx-ferrata_bridge_module "$SEQNET_TMP/logos/"   # the same module .lgx as the send stack's run
    bash e2e/seqnet/run.sh logoseth logoseth-4                      # 12 of 12

## RFP-021

| ID | Proposed | Clause by clause | Evidence |
|---|---|---|---|
| U.1 (send half) | in part (unchanged) | Not advanced: the attestation module's updates and attestations would go through `lez_core.send_generic_private_transaction_with_accounts` with the patch's `public_pda` mentions (its public send has no PDA kind). | — |

## Proposed EVIDENCE rows

- **Bridge module, Ethereum through Logos Core's send stack (anvil), with gas margins, reverts and unmined sends
  reported.** `ferrata_bridge_module` in one headless Logos Core with Logos's `eth_rpc_module` (with our patch for its
  verified-proxy client glue), `fee_module`, `keystore_module`, `signer_manager_module`, `tx_sender_module` (at the
  revisions `tx_sender_module` f1754c3 pins) and the headless approver and custodian `evm_signer_cli` 2cb46d8 and
  `evm_keystore_cli` c6db2ad (`send-stack.sh`'s pins): the admin registers an ERC-20, sets its cap, delists and lists
  it, announces a fee schedule and sets a release delay; the user deposits ETH and the ERC-20 (`approve` + `deposit`,
  one approval); a deposit rejected at the approver; the freeze authority freezes and lifts ETH deposits; deposits
  refused before signing while frozen or delisted; a send forced to revert after approval (its own gas limit 30,000)
  reported as reverted with nothing changed; a deposit broadcast and not mined within its 8-s wait reported as
  "broadcast, not yet mined ... do not send it again", then mined; every other send with the stack's estimate + 25% +
  20,000 and gas used below it. Every user and admin send approved at `evm_signer_cli`, every read through
  `eth_rpc_module` (route `direct`); 53 of 53 checks against `cast` in each of five consecutive runs (the cap: estimate
  48,564 or 51,089, limit 80,705 or 83,861, used 50,771), no approval lost in them (an earlier rerun lost one in the
  upstream approver, cause not identified; the test's approver now looks such a request up, shows it by handle and
  records it: 0 such recoveries in these five runs); native 27 of 27. Build box, 22 Nov 2026; anvil,
  mock verifier; `modules/results/bridge-send-stack-r3/`.
- **The mini-app's round trip and admin, Ethereum through Logos Core's own modules, LEZ through `lez_core`.** The view
  driven headless (Qt Quick Test) calling `ferrata_bridge_module` in one Logos Core with Logos's Ethereum send stack,
  its headless approver and custodian, and `lez_core` (patched), on a LEZ v0.4.0-rc1 zone we ran and anvil: the private
  deposit, claim and burn, the release to a fresh address (sent from the user's release account, as the privacy panel
  and the release's step table say), the vault's admin (accept, cap, a forced revert reported in the view, freeze and
  lift, delist and list, fees, release delay), the registry listing both tokens with each token's LEZ side read again
  through `lez_core` (293 reads in the run, all equal), the bridge program's `SetCap` built by the module at the admin's
  own actor, held at the role's wallet until a person approved it and then accepted, the same from a non-admin account
  refused, and a `Sunset` held there and rejected by the person (nothing sent). Every user and admin Ethereum send of
  the run after the vault's deployment through `tx_sender_module`, approved at `evm_signer_cli`; outside the stack: the
  deployment (the deployer's contract creations, `registerToken` twice, `renounceOwnership`, `transferAdmin` offering
  the admin role), the stand-in prover's `accept` to the mock verifier and the relayer's checkpoint sends; the LEZ admin
  messages signed by a stand-in for the role's wallet (the chain service's LEZ wallet holding the run's role keys, with
  the approval step). Qt Quick Test 12 of 12 (10 test functions). Build box, 22 Nov 2026; dev-mode private proofs;
  `e2e/runs/seqnet-logoseth-4/`.
