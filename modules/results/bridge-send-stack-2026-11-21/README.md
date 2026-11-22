# ferrata_bridge_module's Ethereum side through Logos Core's own modules, 21 November 2026

**Superseded (21 Nov 2026).** This 37/37 did not reproduce at `d12dd396`: the review's four reruns gave 35/37 each, an
approved `setTokenCap` reverting for want of gas (sent with the bare estimate, no margin), and the module reported
reverts as done. Fixed in `787b5448`; the record of the fixed module is `../bridge-send-stack-r2/`.

RFP-022 Usability #1 asks for the user's and the admin's functions in a Logos core module, and the RFP's platform
dependency says "the Ethereum side must use the Logos Ethereum core module". Until this run the bridge module built its
Ethereum calls and a stand-in service (`seqnet-chain`) read the vault and signed and sent them. Here the module reads the
vault through Logos Core's `eth_rpc_module` and hands every transaction to Logos's send stack: `tx_sender_module` prices
it (`fee_module`), reserves the nonce and asks `signer_manager_module` for one approval; a person approves at Logos's
headless approver `evm_signer_cli` with the account's password; `keystore_module` signs; `send_status` broadcasts through
`eth_rpc_module`. The module holds no Ethereum key and signs nothing. Local only: an anvil chain (chain id 11155111,
Sepolia's, as Logos's own doc-test uses), nothing sent to any public network.

## Where it ran

The Modal build box (`spike/modal-build/mrun`, 16 cores), from the worktree's root, as run: `MODULES=ferrata_bridge_module
bash modules/scripts/native.sh`; `bash modules/scripts/bridge-eth-native.sh <scratch>/eth-native`;
`NIX_SAVE=<scratch>/nix-store.tar.zst bash modules/scripts/bridge-send-stack.sh build <scratch>/bss`; then
`bash modules/scripts/bridge-send-stack.sh run <scratch>/bss`. Tree: branch `rfp-logos-core-eth` at `4dce62c8` (the module
code) with the scripts of the same commit. Foundry 1.3.6 (anvil, cast, forge), Nix 2.28.3.

| What | Revision | Built with |
|---|---|---|
| `eth_rpc_module` 0.2.0 | `logos-evm-eth-rpc-module` `6965f32` + `modules/patches/eth_rpc_module-6965f32-generated-clients.patch` | logos-module-builder `5f74cdb` with its own locks (`graft_builder_lock.py`) |
| `fee_module` 0.2.0, `keystore_module` 0.2.0, `signer_manager_module` 0.2.0, `tx_sender_module` 0.2.0 | `f2065e7`, `e83a899`, `bd5c84d`, `f1754c3` (what `tx_sender_module`'s lock pins) | the same |
| `evm_signer_cli` 0.2.1 (the headless approver), `evm_keystore_cli` 0.2.0 (the custodian) | `2cb46d8`, `c6db2ad` (both pin the same signer manager, keystore and builder `a99b32a` as `tx_sender_module`) | the same; first built here |
| `ferrata_bridge_module` 0.1.0 | this branch | `modules/scripts/check.sh` (builder `5f74cdb`) |
| `logosctl` | logos-logoscore-cli `66d098d` | `check.sh` |

`.lgx` SHA-256 (first 16 hex digits): eth_rpc `ea73799a0c2c2f9c`, fee `80ed87e6a561ec93`, keystore `7ffb69823c9f03ed`,
signer manager `42e10ba75807bfdc`, tx_sender `09babe0886a467ef`, evm_signer_cli `3c7579b8021d9f74`, evm_keystore_cli
`c832f409190c1a57`, ferrata_bridge_module `048f893a1de36428` (`logos-core/lgx-sha256.txt`).

## Results (measured)

**Native, against the real vault on anvil: 26 of 26** (`native/`, `modules/scripts/bridge-eth-native.sh`): the module's
vault reads (roles, ETH's and an ERC-20's registry entries, price, the deposit list checked against `depositRoot`, a
depositor's count, accepted roots, released nullifiers, refunds) equal to `cast`; the admin calls it builds
(`registerToken` with its dynamic tuple, `setTokenCap`, `setTokenDepositsFrozen` and lifting it) accepted by the vault when
`cast` sends them; a second registration refused with the vault's error named (`TokenAlreadyRegistered`, from `eth_call`
before sending); a cap from the user's account refused before sending (not the vault's admin); the LEZ `SetCap` message.

**In one headless Logos Core: 37 of 37** (`logos-core/checks.tsv`, `run` 40 s):

| Step | Through | Checked against |
|---|---|---|
| eight modules installed from their `.lgx` and loaded (the send stack, the two CLIs, the bridge module) | `logosctl` | `module list` |
| roles: `keystore_module` and `signer_manager_module` configured as Logos's doc-test configures them (approvers `evm_signer_ui`, `evm_signer_cli`; custodians `evm_keystore_ui`, `evm_keystore_cli`) | `configure` | |
| three keys (the vault's admin, its freeze authority, the user) imported by `evm_keystore_cli` | `keystore_module` | `signer_manager_module accounts` |
| `admin status`: roles and registry read through `eth_rpc_module` (route `direct`) | `eth_rpc_module` | `cast` |
| the admin registers an ERC-20 (two denominations: the permitted amounts) and sets its deposit cap | module -> `tx_sender_module` -> approved at `evm_signer_cli` | `cast`: registered, the cap, sent from the admin |
| the user's ETH deposit: the module's commitment and `depositETH`, the price read through `eth_rpc_module` | module -> `tx_sender_module` -> approved | the vault's `Deposit` event carries the module's commitment; the vault received the price; sent from the user |
| the user's ERC-20 deposit: `approve` + `deposit` as ONE bundle, one approval | the same | two transactions; the vault holds the price; leaf 1 |
| a deposit the person rejects at the approver | the same | the module answers `rejected`; no deposit |
| the freeze authority freezes ETH deposits; a deposit is then refused by the module before anything is asked or signed (`TokenDepositsAreFrozen` from `eth_call`); the freeze lifted | module -> `tx_sender_module` -> approved | `cast` |
| the deposit list read back through `eth_rpc_module`: 2 deposits, root = the vault's `depositRoot` | `eth_rpc_module` | `cast` |

What the person saw at the approver for each send is in `logos-core/prompts.jsonl` (the signer manager's own words): for
example "Requested by: tx_sender_module ... Purpose (claimed by the requester): Ferrata bridge admin (admin): register
token 0x59f2... with 2 denominations (the permitted amounts) [asked by ferrata_bridge_module]", then the account, the
chain, the nonce, the gas and fee ceilings and the full calldata. The module's call from asking to the receipt read back,
the approval given by a script as soon as the prompt appeared (`logos-core/send-ms.tsv`, milliseconds): admin register
3,092; cap 4,205; ETH deposit 2,648; ERC-20 deposit (two transactions) 4,659; refused 1,618; freeze 2,607; lift 2,611.

Build (measured, from an empty Nix store on this workspace's box): the send stack and the two CLIs 863 s
(`eth_rpc_module` needed six attempts of the `/homeless-shelter` retry, as on 14 November); the bridge module and
`logosctl` through `check.sh` 1,274 s (its calls through Logos Core equal to the native ones); the store saved for the
next container (2.0 GB, 20 s).

## What this is not

- **Not a public network.** Anvil; keys are anvil's published dev keys, imported into `keystore_module` with a local
  password. The vault's verifier is the vault tests' mock (no release was sent in this run: the release through the send
  stack is the seqnet run's, `e2e/seqnet` mode `logoseth`).
- **The approval is a script playing the person** at `evm_signer_cli` (`approve handle bundle_id @password-file`),
  as Logos's own doc-test does; in Basecamp the person approves in the Signer app (`evm_signer_ui`), raised by the intent
  `evm.signing.approve {handle}` that the mini-app sends when the host offers intents (not run here).
- **The configuration of the roles is upstream's**: `configure` is ungated in both the keystore and the signer manager at
  these revisions ("any caller can name itself custodian"); keystore PR #25 (open) moves it to an access policy.
