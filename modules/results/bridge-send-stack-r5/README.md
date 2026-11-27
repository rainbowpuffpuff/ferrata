# ferrata_bridge_module through Logos Core's send stack, every vault action (27 Nov 2026)

Follows `../bridge-send-stack-r4/` (22 Nov, 55 checks) with step 10 of `modules/scripts/bridge-send-stack.sh`: the twelve
vault actions the module had not yet sent through Logos's send stack (RFP-022 U.1). The module was built from this
branch (`t-mod`, after `50b396a8`; its vault admin code, `calls/src/admin.rs`, is unchanged since `50b396a8`) with
`modules/scripts/check.sh` (logos-module-builder `5f74cdb`, `LOGOS_TMPDIR=/var/tmp/bss`), the send stack with
`modules/scripts/send-stack.sh build` at the revisions it pins, logosctl `66d098d`. Measured on the Modal build box, one
headless Logos Core on a local anvil (chain id 11155111; nothing public):

    bash modules/scripts/bridge-send-stack.sh run <out>     # <out>/stack (send-stack.sh build), <out>/check (check.sh)

**81 of 81 checks** (`checks.tsv`, `summary.txt`); approver recoveries 0. Every send approved at Logos's headless
approver `evm_signer_cli` by a person's step (the test), signed by `keystore_module`, sent by `tx_sender_module`; the
module holds no key. Every effect read back with Foundry's `cast` against the vault's own getters.

Step 10, all passed:

| Action | Sent by | What was checked |
|---|---|---|
| `set_global_cap` (release) | admin | settled; `globalCap(1)` capacity and refill set; the deposit bucket untouched (unlimited) |
| `raise_delta` 600 → 1200 | admin | settled, `delta()` 1200; 900 (below) refused before signing (`DeltaCanOnlyRise`), 30 days + 1 s refused (`DeltaTooLong`), nothing at the approver either time |
| `transfer_admin`, `accept_admin` | admin, then the pending admin | the role handed to the user's account and accepted by it, then handed back and accepted: `admin()`, `pendingAdmin()` and each sender read back |
| `set_deposits_frozen`, `set_releases_frozen`, `set_checkpoints_frozen`, `set_token_releases_frozen` | freeze authority | each frozen until its block's time + `maxFreeze()` and lifted (the checkpoint freeze's lift keeps its block's time); while deposits were frozen the module refused a deposit before signing (`DepositsAreFrozen`) |
| `revoke_from(0)` | freeze authority | **refused before signing** (`UnknownCheckpoint`): this fixture finalises no checkpoint (`checkpointCount()` 0), so a revocation is not executed here; the module named the vault's error from its `eth_call` and the approver got nothing |
| `propose_roles`, `apply_roles` | admin, then anyone | the proposal in force 14 days after its block; applied before that, refused (`TimelockNotOver`); after anvil's clock moved 14 days, applied by the user's account: watcher 0 the user, the freeze authority kept, no proposal left |
| `apply_fees` | anyone | step 3c's schedule put in force after its seven days: schedule 0 → 1, mint fee 1000 |
| `retire` | anyone, then the freeze authority | by anyone before `retireOpensAt()` refused (`RetireNotOpen`); by the freeze authority settled, `retired()` true; a deposit afterwards refused before signing (`VaultRetired`) |

Each refusal check also reads `evm_signer_cli status` (no request pending or on screen) and `tx_sender_module
live_sends` (none). The run's records: `run/` (every request and reply of the module, `calls.jsonl`; the approver's
prompts, `prompts.jsonl`), `run.log`. No key or password is in these files (the run's keys are anvil's published test
keys, in the script).

The `.lgx` run (sha256): `ferrata_bridge_module` `3472ae76…af00a`; the send stack: `eth_rpc_module` `5fa8f0b0…6514`,
`fee_module` `2cde5579…f55a`, `keystore_module` `fc1736c2…31b0`, `signer_manager_module` `a0425063…1d1f`,
`tx_sender_module` `59a00723…da6b`, `evm_signer_cli` `6d12dfa7…5697`, `evm_keystore_cli` `a8f2ece8…4419`.

Not covered here: LEZ's `Freeze`, `Pause` and `Tick` through the module (those need `lez_core` and a sequencer), a
revocation that executes (it needs a finalised checkpoint), the release sent by the module itself.
