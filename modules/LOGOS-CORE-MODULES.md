# Logos Core's own modules, and how Ferrata's modules use them

RFP-021 U.3 asks for an updater that "follows Ethereum through the Logos Ethereum core module"; RFP-022 U.3
asks for long-running components "using LEZ module and Ethereum module for the respective chains access".
This note maps the modules Logos ships for that, how one module calls another, and what Ferrata's modules
use today. Read 13 November 2026 from the sources below (all `github.com/logos-co/` unless named); the
commits are the repositories' `main` on that day, and the ones built and run in
[`results/logos-core-io-2026-11-13/`](results/logos-core-io-2026-11-13/).

| Module (name in Logos Core) | Repository @ commit | Kind | Depends on |
|---|---|---|---|
| `eth_rpc_module` 0.2.0 | `logos-evm-eth-rpc-module` @ `6965f32` | Rust cdylib | optional: `modules_state`, `verified_proxy_module` |
| `verified_proxy_module` 0.2.0 | `logos-verified-proxy-module` @ `fa98479` | C++ universal + `libverifproxy` (nimbus-eth1 v0.4.4, Nim) | `beacon_api_module` |
| `beacon_api_module` 0.2.0 | `logos-evm-beacon-api-module` @ `aca138e` | Rust cdylib | none |
| `keystore_module` 0.2.x | `logos-evm-keystore-module` @ `9d9a432` (`main`), `e83a899` (what the send stack pins) | Rust cdylib | none |
| `signer_manager_module` 0.2.x | `logos-signer-manager-module` @ `152c96d` (`main`), `bd5c84d` (pinned) | Rust cdylib | `keystore_module`; optional: keycard, keycard-shell, QR signers |
| `fee_module` 0.2.0 | `logos-evm-fee-module` @ `f2065e7` | Rust cdylib | `eth_rpc_module` |
| `tx_sender_module` 0.2.0 | `logos-evm-tx-sender-module` @ `f1754c3` | Rust cdylib | `eth_rpc_module`, `fee_module`, `signer_manager_module` |
| `lez_core` 0.5.1 | `logos-blockchain/logos-execution-zone-module` @ `fdd6a75` | C++ universal + LEZ's wallet FFI | none |

Built into `logosctl` (logos-logoscore-cli `66d098d`) and listed by `module load` without being installed:
`capability_module`, `modules_state`, `package_manager`, `package_downloader`, `storage_module`
(run 1's `MODULE_LOAD_FAILED` reply lists them as known modules).

## How a module calls another

- **Declare it.** `metadata.json` `dependencies` (loaded first, the load fails without it) or
  `optional_dependencies` ("the loader brings one up only if it is installed, ahead of this module, and
  never fails a load because one is missing"; not bundled): logos-module-builder `5f74cdb`
  `docs/configuration.md` 328–370.
- **Its contract.** The builder generates a typed client from the dependency's LIDL contract, taken from
  the dependency's flake input (`packages.<system>.lidl`) or from a file named by `dependency_overrides`
  (`{ "<name>": { "file": "...", "input": optional } }`): `lib/common.nix` `classifyConcreteDeps` (179–),
  `lib/parseMetadata.nix` 304–315; passed as `--dep name=<lidl>` to `logos-lidl-gen` (`lib/mkLogosModule.nix`
  255–262, 500–580). A Rust module gets `pub mod <dep> { pub struct <Dep>Client … }` with `new()`,
  `bind(name)` and, per method, `<method>(…)`, `<method>_with_timeout(…, Duration)` and async twins
  (logos-rust-sdk `040e94e`, the builder's pin: `lidl-gen/src/rustgen.rs` 414–432, 749–768); each call is
  `PluginProxy::call_json_with_timeout(method, args, timeout)` (`src/plugin.rs` 816–840) over Qt Remote
  Objects, after a capability handshake that names the calling module (`src/plugin.rs` 160–200).
  `eth_rpc_module` calls `verified_proxy_module` this way (`rust-lib/src/glue.rs` 212, 246–257).
- **What a contract mismatch costs.** The contract decides the interface hash; a caller generated from
  another contract than the provider's is refused at run time (`docs/configuration.md` 827–829). Every
  module in our Logos Core is therefore built with one builder (our pin, `5f74cdb`, by
  `--override-input logos-module-builder`), and the send stack at the revisions `tx_sender_module`'s lock
  pins. (A call by name skips the generated client and its contract check: the caller and the provider
  agree on the method's name and argument order only.)

**What Ferrata's modules do.** `ferrata_attest_module` and `ferrata_bridge_module` call `eth_rpc_module`
**by name**, through the SDK (`LogosModuleSDK::new().plugin("eth_rpc_module").call_json_with_timeout("raw_rpc",
[chain_id, method, params_json], timeout)`: what a generated client does inside), without declaring it a
dependency. Declaring it (optional, its contract in a committed `.lidl` named by `dependency_overrides`)
was the first design; it was dropped because **with our builder pin (`5f74cdb`) a module with optional
dependencies fails to install**: `eth_rpc_module` itself, built with that builder, compiles and links, then
stops in its install phase at `logos-cpp-generator --normalize-lidl` on its optional dependencies'
contracts ("Plugin file does not exist: --normalize-lidl", `lib/common.nix` 255–275; runs 1 and 2a of
[`results/logos-core-io-2026-11-13/`](results/logos-core-io-2026-11-13/)); modules with required dependencies
(`tx_sender_module`, `fee_module`) install. So `eth_rpc_module` is built here from its source with its two
optional dependencies called by name in the same way (`scripts/eth_rpc_by_name.py`: four call sites, its
verified-proxy gate and routing unchanged), and our modules keep loading without it. Each module crate
installs that call into its calls crate (`rpc.rs`, the same file in both); a request then chooses the
path: `"eth": {"via": "eth_rpc_module", "chain_id": N}` (Logos Core) or `"rpc_url": "http://…"` (the plain
path, kept as the fallback and for native runs). Every reply that read Ethereum carries
`eth: {via, chain_id | rpc_url, calls, routes}`, `routes` being `eth_rpc_module`'s own label per call
(`direct`, `verified`, `proxied`).

## The modules

### `eth_rpc_module`: Ethereum JSON-RPC, per chain

- **Offers** (`rust-lib/src/glue.rs` 34–117): a chain registry (`set_chain_config(chain_id, json)`,
  `get_chain_config`, `list_chains`, `init_defaults`, …) and calls keyed by chain id: `block_number`,
  `get_balance`, `call`, `estimate_gas`, `fee_history`, `send_raw_transaction`,
  `get_transaction_receipt`, …, and **`raw_rpc(chain_id, method, params_json)`** for any JSON-RPC method,
  answering `{ok: true, result, route}` or `{ok: false, code, error}` (`glue.rs` 362–368, 567–575).
  No typed `eth_getProof`, `eth_getLogs` or `eth_getBlockReceipts`: `raw_rpc` carries them.
- **Configured** per chain by method call (persisted in its instance directory, `chains.json`):
  `{endpoint, proxy?, proxyRequired?, timeoutSecs?, verifiedProxyMode?}`; any `http(s)` endpoint (a
  devnet's `http://127.0.0.1:…` is accepted: `rpc.rs` 287–301 checks names and the proxy rules only).
- **Verified mode** (`set_verified_proxy_mode(chain_id, "required")`): every call on that chain, `raw_rpc`
  included, goes to `verified_proxy_module.rpc` and is refused rather than answered directly when the
  proxy cannot answer (`rpc.rs` 1043–1080). Only `eth_getBalance`, `eth_getTransactionCount`, `eth_getCode`,
  `eth_getStorageAt` and `eth_call` are labelled `verified`; the rest `proxied` (`rpc.rs` 462–468).
- **Used by Ferrata:** the attestation module's `fetch_log_proof` (`eth_getBlockByNumber`,
  `eth_getBlockReceipts`), its storage path (`eth_getProof` at the record's block, in `dry_run_attest` and
  `build_attest_tx`), and the bridge module's `import_root` (`eth_getProof` of the vault the configuration
  names, slots 0 and 1). Shown in a headless Logos Core (`results/logos-core-io-2026-11-13/`): the attestation
  module's four reads equal to the plain reads and its end-to-end run 43/43; the bridge module's `import_root`
  read through it equal to the proofs given, `importable`, and its end-to-end run 24/24.

### `verified_proxy_module`: light-client-verified JSON-RPC

- **Offers** (`src/verified_proxy_impl.h`; README "API"): `configure`, `start`, `stop`, `status`,
  `rpc(method, params)` and 30 typed `eth*` wrappers (`ethGetBlockByNumber`, `ethGetBlockReceipts`,
  `ethGetLogs`, `ethGetStorageAt`, …); an optional local JSON-RPC endpoint.
- **Configured:** `network` ∈ {`mainnet`, `sepolia`, `hoodi`} (README 177), a `trustedBlockRoot`, execution
  URLs that serve `eth_getProof` (README 58), beacon providers from `beacon_api_module`.
- **What it cannot do for us.** (1) It serves only those three networks: it syncs nimbus's light client
  from a trusted root against real beacon providers, so it cannot answer for a devnet (anvil has no beacon
  chain). (2) It has **no `eth_getProof`** (absent from `tools/rpc_methods.json` and from nimbus v0.4.4's C
  dispatch, `library_c_frontend.nim` 895–1223, R3 §4): on a chain in `required` mode `eth_rpc_module`
  routes `eth_getProof` there and gets an error, so the storage proofs and `import_root`'s proofs must come
  from a chain configured `off` (or `raw_rpc_url`, which `required` refuses only for the five `verified`
  methods). That is safe for Ferrata: the programs check every proof against a record the light client
  verified. Blocks and receipts it does verify (receipts against `receiptsRoot`, nimbus
  `engine_receipts.nim` 49–67), which is what `fetch_log_proof` reads.
- **Not built or run here:** its Nim library is a ~25-minute build of its own, and the only networks it
  serves are public ones, which this work did not read (the brief: nothing to Sepolia or any public
  network). The read path above takes it unchanged: set the chain's mode to `required`.

### `beacon_api_module`: consensus-layer providers

- **Offers** (`rust-lib/src/glue.rs` 18–53): provider lists per chain id (any id), failover, a fail-closed
  SOCKS proxy, `lc_request(chain_id, endpoint, params_json, provider, deadline_ms)` with `endpoint` one of
  `getLightClientBootstrap`, `getLightClientUpdatesByRange` (≤ 128), `getLightClientOptimisticUpdate`,
  `getLightClientFinalityUpdate` (47, 233), `raw_get(chain_id, path, provider)` under `/eth/v1|v2/` (53),
  `finalized_header` ("not a trust anchor").
- **It does not verify** what providers return (README "Trust"; `beacon.rs` 219–258); the updater's program
  does, which is why it is fit to carry the updater's data.
- **Status here:** built from source and loaded in our Logos Core; the updater module still reads its
  sources itself (beacon API URLs or recorded directories). Moving its source to `lc_request` is the
  same change as `rpc.rs` above, not made in this work.

### `keystore_module`, `signer_manager_module`, `fee_module`, `tx_sender_module`: keys and sends

- **`tx_sender_module`** (`rust-lib/src/glue.rs` 45–137): `prepare(request)` prices a bundle of calls
  (`fee_module`'s estimate, the nonce) without reserving or signing; `send(request)` reserves nonces and
  asks `signer_manager_module` for **one human approval** over the bundle, answering a `requestId`, never a
  hash; `send_status(requestId)` is polled and is what broadcasts once approved (through
  `eth_rpc_module.send_raw_transaction`).
- **`signer_manager_module`** (`rust-lib/src/glue.rs` 27–186): roles set by `configure` (ungated for now),
  `request_approval`, the approver's `pending` / `acknowledge` / `approve(handle, bundle_id, password)`;
  and the path an unattended daemon needs: the custodian's `unlock({account, password, ttlMs?, count?,
  apps, confirm?})` (111), after which requests are signed without being shown, listed by
  `unattended_signatures` (118) and announced by `signed_without_asking`.
- **`keystore_module`** (README 1–150): scrypt vaults, BIP-39/32 accounts (`import_private_key`,
  `import_mnemonic`, …, custodian only), signing only through the approval tiers ("No method signs on
  demand", 146) or as `logos.signer/1` for the signer manager.
- **Status here:** `fee_module`, `signer_manager_module` and `tx_sender_module` built from source with our
  builder and installed beside our modules; **`keystore_module` (`e83a899`) did not build**: it compiles and
  links, then its install stops at `logos-cpp-generator --normalize-lidl` on its interface contract
  (`interface_dependencies`: `wallets.lidl` of logos-signer-interface), the step that stops unchanged
  `eth_rpc_module` on its optional dependencies. **The cause is the override, not the builder** (found 13 November
  by lez-core-v04): `--override-input logos-module-builder …/5f74cdb` relocks the builder but nix keeps the module's
  old lock entries for the builder's own inputs, so `keystore_module` (its lock: builder `9f420c2`) and
  `eth_rpc_module` (`e1a2570`) were built with logos-cpp-sdk `e1d6bf2` (3 September), whose `logos-cpp-generator`
  has no `--normalize-lidl` (`3bf5365`, builder 5f74cdb's own pin, has it: `cpp-generator/main.cpp` 462, 492); the
  three that installed already lock `3bf5365`. [`scripts/graft_builder_lock.py`](scripts/graft_builder_lock.py) gives a
  module the builder with the builder's own locks (every other input as the module locks it) and
  [`scripts/send-stack.sh`](scripts/send-stack.sh) builds the five so; not run yet. Without it `signer_manager_module` (which requires it) and so
  `tx_sender_module` do not load, and nothing was signed or sent ([`results/…/send-stack.log`](results/logos-core-io-2026-11-13/send-stack.log)).
  Its five calls to the `wallets` interface are bound clients with callbacks (`rust-lib/src/glue.rs` 320–476),
  not the four plain calls `eth_rpc_module` needed. The monitor's cancellations and the relayer's releases
  still sign with their own key file (alloy). Moving them means a key imported into `keystore_module`, the
  signer manager's roles and an unattended `unlock` scoped to `tx_sender_module`, and the monitor's
  `Chain::send` (one place, `monitor/src/chain.rs` 309–326) calling `send` and polling `send_status`; the
  approval model is the open question for a watcher that must cancel within the challenge window without a
  person present. `logos-io.sh` drives that sequence from the command line once the stack loads.

### `lez_core`: LEZ's wallet (and sequencer) as a module

At **`cc73ebf`** (`main` after #72 "Sequencer FFI adaptation" and #74 "sequencer query API" were merged on 13 November,
13:18 and 13:20 UTC): `lez_core` 0.6.0, LEZ **v0.4.0-rc1 `1806e87`** (exactly ours), logos-module-builder `5f74cdb`
(ours), no module dependencies; a C++ universal module over LEZ's `core_ffi` (`src/lez_core_module.h`).

- **Two sides.** The wallet (`create_new`/`open(config, storage, password, force_rpc)`, accounts,
  `get_account_public` = the sequencer's `getAccount` through LEZ's wallet library, `get_account_private` after
  `sync_to_block`, `send_generic_public_transaction`, `send_generic_private_transaction(_with_accounts)`,
  `poll_transaction_status`, program deployment) and, new in #72/#74, a sequencer: `start_sequencer(config)` runs LEZ's
  sequencer **inside the module**, and `getAccount`, `getBlockById`, `getBlocks`, `getTransaction`,
  `getTransactionsByAccount`, `getLastFinalizedBlockId` answer only for that one (otherwise "sequencer not started"
  and an empty reply). That sequencer binds no RPC (core_ffi `sequencer/api/lifecycle.rs`: the service's `rpc` feature
  is off), so nothing outside the module reaches it: a zone whose operator, relayer, checkpoint prover and other
  wallets use the sequencer's RPC keeps its sequencer outside, and `lez_core` is then a wallet client of it
  (`open(…, force_rpc = true)`), without the block queries.
- **What Ferrata routes through it** (`ferrata_bridge_module`'s wallet session, `rust-lib/calls/src/lez.rs`; the module
  calls `lez_core` by name through the SDK, as `eth_rpc_module`): the user's LEZ wallet opened inside `lez_core`;
  every bridge actor state the session reads (`config(cid)`, `token(cid, ETH)`, `root(cid, n)`, a deposit's
  `nullifier(…)`, `holder(account)`) through `get_account_public`, compared byte for byte with the chain service's
  read; the private holding through `sync_to_block` + `get_account_private`; the **private claim and the private burn**
  through `send_generic_private_transaction_with_accounts` (the module's `Borsh` message, its accounts, the bridge
  program's binary as a `disclosed` program at its header account), executed and proven by LEZ's wallet inside Logos
  Core; the chain service then reads the block that applied it (`module.tx`) beside `poll_transaction_status`. The
  zone's history (imported roots, published burns, accounts named by private transactions) stays with the chain
  service: the wallet side serves no blocks.
- **What it cannot do at `cc73ebf`, and the patch.** (1) **No account authorizes**: `buildAccountMentions`
  value-initialises `FfiAccountMention`, so `authorizes` is false for every account of every generic send, and
  LEZ's wallet then neither signs for a public account (`lez/wallet/src/account_manager.rs` 1035–1038) nor gives a
  private one its spending key (1112–1118), nor takes it as the fee payer (`lib.rs` ~552); LEZ's own wallet API
  mentions accounts authorizing (`select_program_actor_state`). (2) **No PDA account kind**: the private send takes
  `private_owned`, `public`, `public_no_sign` and `private_foreign`; the bridge's claim and burn mention three or four
  of the program's PDAs, one of them new (the deposit's nullifier, the burn record), which the program creates only
  through the mention's seed; core_ffi has the kind (`FfiAccountIdentityKind_PublicPda`, `types.rs` 558–564). (3) The
  header lets `actor_state_program_account_ids_hex` be empty; the code requires every entry.
  [`patches/lez_core-cc73ebf-authorizes-public-pda.patch`](patches/lez_core-cc73ebf-authorizes-public-pda.patch)
  fixes (1) and (2) (accounts authorize as the wallet API mentions them; `"authorizes": false` for one that only
  receives; `{"kind": "public_pda", "account_id", "program", "seed"}`) with four unit tests; the draft pull request is in
  the lez-core-v04 report. `e2e/seqnet/run.sh build-lezcore` builds both, `run.sh lezcore NAME` runs the round trip.
