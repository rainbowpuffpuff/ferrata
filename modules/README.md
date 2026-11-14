# Logos modules

The proposal (§B7) puts everything off chain in Logos modules. This directory holds six, prototypes,
built the way Logos modules are built, loaded together by a headless Logos Core and called there:
`ferrata_attest_module` (eight methods: dry runs, checkpoint checks, an instance's state read back, a
log or storage slot attested end to end: fetched over JSON-RPC, its `Attest` transaction built, the
program's event read back; and `advance`, an update's job planned with the updater's code),
`ferrata_bridge_module` (eleven: deposit, claim (plaintext or private), burn, release (by the wallet's rule),
prove_release, import_root, registry, recover, status, and `wallet`, the mini-app's calls run with the others), and
the four daemons as modules:
`ferrata_attestor_module` (rule (B)'s attestor beside a sequencer and its collector, `checkpoint/attestor`),
`ferrata_monitor_module` (the watchers' `monitor`, `monitor/`), `ferrata_updater_module` (the
light-client updater `ferrata-updater`, `lez/updater`) and `ferrata_relayer_module` (the relayer
`ferrata-relayer`, `cli/`: root imports, rule (B) checkpoints and their finalisation, releases and their wrap,
fees, the burn watch). Where the `ferrata` tool or the updater already
has the logic, the module runs the same code: `ferrata-wallet` (`cli/wallet`) and `ferrata-advance`
(`lez/advance`) are linked by the tool and the updater and by the modules (the updater module links
`ferrata-advance` through the updater; the relayer's root import is `ferrata-wallet`'s `import`, the bridge
module's `import_root`), and the daemon modules run the daemons' own libraries. `catalog/` lays out
the catalog that would release them with `logos-modules-release-action`. `RESULTS.md` records what ran.

```sh
modules/scripts/native.sh     # any Linux with cargo: stage, unit tests, the native call examples
modules/scripts/e2e.sh OUT    # + the lez/ harness: a log end to end, an update advanced (methods called natively)
RELEASE_ELF=... modules/scripts/bridge_e2e.sh OUT
                              # + the mini-app's devnet: root import, registry, private claims, the
                              # release rule, the release guest, releases on the vault (natively)
modules/scripts/check.sh      # Linux x86-64 with Nix: the above, then build, package, load and call all
                              # six modules in Logos Core (dev and portable), e2e.sh through the daemon
                              # (BRIDGE_E2E=1 RELEASE_ELF=...: bridge_e2e.sh through it too), and the four
                              # daemons doing their jobs as modules (natively first)
modules/scripts/relayer-e2e.sh OUT   # the relayer on its devnets: relayer_fork.rs (a fork of Sepolia) and root
                              # imports on the mini-app's devnet (binary; CALL=logosctl: the module)
modules/scripts/attestor-zone-live.sh BUNDLE OUT   # four Logos Cores beside a running zone of four sequencers
```

## How a Logos module is built and loaded

References are to the upstream repositories at the commits named, under `logos-co/` on GitHub.

| Step | How | Where upstream |
|---|---|---|
| Declare | `metadata.json`: name, `type: core`, `interface: cdylib`, `main` (the plugin's file name), `codegen.rust {crate, trait}` for a Rust module whose contract is a trait | logos-module-builder@5f74cdb `templates/rust-module/metadata.json`, `docs/configuration.md` |
| Contract | The trait in the crate's `src/lib.rs` is the module's API. The builder derives the `.lidl` contract from it (`logos-lidl-gen --from-rust`) and generates the C-ABI scaffold the crate includes (`generated/provider_gen.rs`); the trait name is the module name in PascalCase. Argument and return types are LIDL's: `String`, `i64`, `u64`, `f64`, `bool`, `Vec<u8>`, `Option<T>` | `templates/rust-module/rust-lib/src/lib.rs`; `lib/mkLogosModule.nix` 454–520 |
| Build | `flake.nix` calls `logos-module-builder.lib.mkLogosModule`. The builder copies only the crate directory (`rust-lib/`) and its own logos-rust-sdk source beside it, compiles the crate to a static library with `buildRustPackage` against the committed `Cargo.lock`, and links it into a Qt plugin with the generated glue | `lib/mkLogosModule.nix` 586–656; `cmake/LogosModule.cmake` 791 |
| Package | `nix build .#lgx` (dev package, Nix store paths) and `.#lgx-portable` (self-contained, what a catalog ships) | logos-module-builder `README.md` 95–101 |
| Release | A catalog forked from `logos-modules-release-base` builds each module's `.#lgx-portable` per platform with `logos-modules-release-action` | logos-modules-release-base@33fb564 `README.md`; logos-modules-release-action `README.md` 59–62 |
| Run headless | `logosctl` (logos-logoscore-cli) is Logos Core without a GUI: `daemon start --detach`, `install X.lgx -y`, `module load NAME` (resolves dependencies), `call NAME METHOD args...`, `daemon stop`. The dev `logosctl` (`.#ctl`) takes dev packages; the portable one (`.#ctl-bundle-dir`) takes portable packages | logos-logoscore-cli@66d098d `README.md`, `docs/logosctl.md` 83–99, 298–450; logos-module-builder `doctests/cross-language-composition.test.yaml` 546–635 |
| Arguments | Each argument of `call` becomes a JSON value: `@file` passes a file's contents as a string, `json:` parses JSON, decimal numbers become integers, anything else a string | `docs/logosctl.md` 413–450 |

A Python client (`logos-logoscore-py`, `import logosctl`) drives the same daemon; it is not needed here.

## The modules

| Module | Directory | Methods |
|---|---|---|
| `ferrata_attest_module` | `ferrata_attest_module/` | `version`, `dry_run_attest`, `verify_checkpoint`, `fetch_log_proof`, `build_attest_tx`, `read_attestation`, `read_instance`, `advance` |
| `ferrata_bridge_module` | `ferrata_bridge_module/` | `version`, `deposit`, `claim`, `burn`, `release`, `recover`, `status`, `prove_release`, `import_root`, `registry`, `wallet` |
| `ferrata_attestor_module` | `ferrata_attestor_module/` | `version`, `keygen`, `pubkey`, `deployment`, `certify`, `tier`, `attest`, `verify`, `collect`, `assemble`, `base`; the loop: `start`, `status`, `stop` |
| `ferrata_monitor_module` | `ferrata_monitor_module/` | `version`, `check`, `node_view`, `run`; the daemon: `start`, `status`, `stop` |
| `ferrata_updater_module` | `ferrata_updater_module/` | `version`, `check_config`, `run`; the daemon: `start`, `status`, `metrics`, `stop` |
| `ferrata_relayer_module` | `ferrata_relayer_module/` | `version`, `run`; the daemon: `start`, `status`, `stop` |

All six are laid out the same way:

- `metadata.json`, `flake.nix`, `flake.lock`, `CMakeLists.txt`: the builder's Rust template, with the builder pinned
  to the commit above.
- `rust-lib/`: the module crate (the trait and its implementation, a static library) and `calls/`,
  the methods' logic as a plain crate that runs and is tested without the generated scaffold
  (`calls/examples/call.rs` runs one method natively: `call METHOD [REQUEST_FILE]`).
- The crates each module links are copied into `rust-lib/` before a build (`scripts/stage.sh`), because
  the builder copies only `rust-lib/`. The copies are ignored by git; each staging records the commit and
  the copied sources' SHA-256 in a `STAGED.json`, which `version()` reports.
  - The attestation and bridge modules (`scripts/stage.py`): the crates keep their places relative to
    each other, so their own path dependencies hold: the repository's under `rust-lib/staged/<path>`,
    LEZ's `lee_core` (v0.4.0-rc1) under `rust-lib/upstream/lez-v0.4.0-rc1/`. Each staged manifest is
    written with what it inherits from its own workspace resolved, without dev-dependencies or lints;
    `staged/STAGED.json` also records LEZ's commit. The attestation module: `crates/ferrata-core` (with
    blst: `advance` runs the program's job steps natively), `lez/wire` (the program's messages and
    states), `lez/advance` (`ferrata-advance`, the updater's job planning), `lee_core`. The bridge module:
    `bridge/crates/ferrata-bridge-core`, `crates/ferrata-core`, `lez/wire`, `lez/bridge-wire` (the bridge
    program's messages, states, addresses and checks), `cli/wallet` (`ferrata-wallet`, the `ferrata`
    tool's wallet rules and calls, with the RISC Zero client for the release guest), `lee_core`.
  - The attestor module: `checkpoint/attestor` with `checkpoint-core` and `attestor-core` (in
    `checkpoint/`'s layout); the monitor module: `monitor/` (its library) with `attestor-core`. Each
    `rust-lib/Cargo.toml` carries the workspace fields its copies inherit (`stage.sh` checks them).
  - The updater module (`scripts/stage-updater.sh`): into `rust-lib/vendor/`, the updater (with
    `lez/advance`, the job planning it shares with the attestation module's `advance`), the `lez/` harness
    and the crates they use, in the repository's own layout with the workspace manifests they inherit
    from, and the LEZ v0.4.0-rc1 tree (its `artifacts/`, which LEZ's build scripts embed); `lez/methods`,
    whose build script runs risc0-build, is replaced there by a crate of the same name that embeds the
    guests risc0-build made for it on the build box (same constants, same bytes; `version()` reports the
    image id declared and the one computed from the bytes, and `updater-e2e.sh` checks both against
    [`docs/image-ids.md`](../docs/image-ids.md)). No guest is changed or rebuilt by the module builder.
  - The relayer module (`scripts/stage-relayer.sh`): into `rust-lib/vendor/`, `cli/`'s library (its manifest
    with only the package, dependencies, features and workspace; `src/` without the binaries) with `cli/wallet`,
    the repository's crates they link (`lez/wire`, `lez/bridge-wire`, `crates/ferrata-core`,
    `bridge/crates/ferrata-bridge-core`, with the workspace manifests they inherit from) and the LEZ v0.4.0-rc1 tree.
    `cli/` is built without its `lez-wallet` feature: LEZ's wallet library links LEZ's node-side crates (RocksDB,
    libp2p), which the module builder does not build; the module's LEZ side is the devnet's zone (below).
- The programs' messages and states are the programs' own types (`BridgeMessage`, `Message`,
  `TokenState`, `RecordState`, ...), Borsh by their derives: the field-by-field encoders of the first
  versions are gone. The end-to-end tests also decode and execute them with the programs
  (`lez/harness/src/bin/module_e2e.rs`, the devnet). The updater module links LEZ's state machine
  itself: the updater writes to it.
- The monitor and updater modules pin a Rust toolchain in `metadata.json` (`nix.rust.toolchain`, from the
  builder's rust-overlay): 1.93.0 for the monitor (alloy 1.8 needs more than the builder's nixpkgs
  rustc), 1.98.0 for the updater (LEZ pins 1.98.1, which the builder's rust-overlay does not have yet).

**The daemons as modules.** A module stays loaded in its own host process (`logos_host`) for as long as
the Logos Core daemon (`logosctl daemon`) runs it, so each daemon's loop runs in a thread of that
process: `start` takes the daemon's configuration and returns once it has started (or with the
daemon's own error), `status` reads it while it runs, `stop` ends it after the round or poll in hand,
as SIGTERM ends the binary, and the host unloading the module stops it the same way (the SDK's
`AboutToUnload`). The process's signals belong to the host, so no module installs a signal handler.
One-shot methods (`attest`, `collect`, `check`, `run` with a bound) block for their duration; logosctl's
`--timeout` gives them longer than its 20 s default.

**Calling convention (all six).** Each method takes one JSON string and returns one JSON string, so a call needs no type beyond
`String` and the request can come from a file (`logosctl call ferrata_bridge_module claim @request.json`).
Byte strings are `0x` hex; 64-bit integers are JSON numbers, decimal strings or `0x` quantities;
`uint256` amounts are `0x` hex of at most 32 bytes or decimal strings, and are returned as 32-byte hex.
A request that cannot be read returns `{"verdict":"invalid_request","error":...}`; a panic inside a
method returns `{"verdict":"error",...}` instead of crossing into Logos Core. Every other reply has a
`verdict`.

### `ferrata_attest_module`

| Method | Takes | Returns |
|---|---|---|
| `version()` | — | the module's version, what was staged (`STAGED.json`: the commit, LEZ's commit, each crate's version and sources' SHA-256), the BLS backend (blst, for `advance`), the method list |
| `dry_run_attest(request)` | the consumer's `chain_id`; a header `record`: either `{chain_id, encoded}` (the bytes the program stores at `header(id, slot)`, `RecordState::record`, as read from LEZ) or its fields (instance chain id, anchor slot, block number, timestamp, block hash, parent hash, state root, receipts root); and either `storage` (an `eth_getProof` result and the slot; or `address`, `slot` and `eth` or `rpc_url`, the module reading the proof at the record's block, `build_attest_tx` at `storage.block`) or `log` (`{tx_index, log_index, receipt_proof}`, as `fetch_log_proof` returns it) | `verdict` (`attested` or `refused`), `code` (the program's code: 0, or the core's refusal code, which the program returns unchanged, `lez/wire/src/lib.rs` `refusal`), the error's name, and for an attested fact its fields and statement identifiers (`id`, `id_at_block`) |
| `verify_checkpoint(request)` | a `config` (the program's `Config` fields), a `checkpoint` root, optionally the `expected_instance_id`, the `bootstrap` (SSZ, hex), `observed` roots for the checkpoint as other sources report them, `providers` to ask (beacon API base URLs, or `{name, url}`), `min_providers` (3 when providers are given), an `execution` JSON-RPC endpoint (a URL or `{name, url}`, or `{"via": "eth_rpc_module", "chain_id": N}`: through Logos Core's `eth_rpc_module`, whose chain N in `required` mode sends every call to `verified_proxy_module`), `key_validate` (on by default), `timeout_secs` | the recomputed `instance_id`; whether it equals the expected one; the program's `Init` verdict on the bootstrap (`StoreCore::from_bootstrap`: layout, header, header root against the checkpoint, committee branch) with its code; each observed root against the checkpoint; whether the chain id and `genesis_validators_root` are a known pair; `committee`: the full `KeyValidate` on the bootstrap's committee (every key in the subgroup, their sum the aggregate); each provider `agrees`, `disagrees` or is `unavailable` (the root canonical by root and at its slot, finalized there, the provider's genesis and spec those of the configuration, `DEPOSIT_CHAIN_ID` among them), with every request it made; `providers_agreeing` (counted by host); `execution`: the bootstrap's execution block on that endpoint (same chain id, canonical, finalized); an overall `verdict` |
| `fetch_log_proof(request)` | where to read Ethereum: `eth` = `{"via": "eth_rpc_module", "chain_id": N}` (through Logos Core's `eth_rpc_module`, [LOGOS-CORE-MODULES.md](LOGOS-CORE-MODULES.md)) or `rpc_url` (`http://` only: a local node or `scripts/recorded_rpc.py`); `block`, and `tx_index` + `log_index` (within the receipt) or `block_log_index` (JSON-RPC's `logIndex`); optional `timeout_ms` | `verdict` `fetched` (or `inconsistent` when the receipts do not rebuild the block's `receiptsRoot`), the block hash and receipts root, `log` = `{tx_index, log_index, receipt_proof}` ready for `dry_run_attest` and `build_attest_tx`, what the provider said the log is (`provider_log`), `rpc_calls`, `eth` (`{via, chain_id or rpc_url, calls, routes}`: what answered; `routes` are `eth_rpc_module`'s own labels, `direct`, `verified` or `proxied`) |
| `build_attest_tx(request)` | `program_id` (the attestation program's account id), `instance_id`, `chain_id`, `slot` (of the record), and `log`, `logs` or `storage` | `account`: `header(id, slot)` (LEZ's public PDA of the program, seed SHA-256(`ferrata-lez/v1/header` ‖ id ‖ slot LE)), the account list, `message`: Borsh of `Message::Attest { request }` (variant 7), its size and SHA-256, the number of facts |
| `read_attestation(request)` | `event_data` (the program's `FRT-ATT1` event data, hex), optionally `selector`, `expected_ids` (the dry run's `id`s), `expected_instance_id` | `instance_id`, `chain_id`, `block_number`, the `statements`, `matches_expected`, `instance_matches` |
| `advance(request)` | `program_id`; `instance_state` (as `read_instance` takes it); the update as a beacon node serves it, `update` (SSZ of an `updates` entry) or `finality_update` (SSZ); the full committees the store summarises, `bootstrap` (SSZ: its current committee) and/or `committees` (each the keys, then the aggregate key, 48 bytes each); optionally `job_state` (the job's state at `job(id, job_id)`, to continue a job already open) | `verdict`: `useful` (the program would accept it and it moves the store), `no_change`, `unverifiable` (the signing committee is not known) or `invalid`; with `useful`: `job_id`, the layout, the outcome (`advanced`, `adopted`, `rotated`, the new finalized slot) and `steps`: each step's name (`open`, `miller`, `adopt<k>`, `finish`), its root actor, the actors it names and its message (the program's `Message`, Borsh). The planning is the updater's (`ferrata-advance`: `evaluate`, `job_steps`, `job_step`): the program's step code is run natively first (`ferrata_core::steps` with blst), and the committees are taken only where they match the store's summaries |
| `read_instance(request)` | `state`: the instance's state as LEZ holds it at `instance(id)` (the program's `InstanceState`, Borsh, hex); optionally `checkpoint` and `expected_instance_id` | `id`, `config` (the program's `Config`, as `verify_checkpoint` takes it), `active`, `applied` (jobs), `store` (fork, finalized header and slot, whether the next committee is known, the store digest); with `checkpoint`, the id recomputed and `id_matches_config`; `instance_matches` |

The record is built the program's way (`witness::HeaderRecord`, then `attest::HeaderRecord::from_recorded`,
as `evaluate` does), and the proof is checked by `attest_storage` or `attest_log`, so the verdict and
code are the ones the program would return for that proof against that record.

`verify_checkpoint` reads the providers and the execution endpoint with `curl` (`http://` and `https://`, a
generic User-Agent; `calls/src/providers.rs`), as `ferrata-updater` reads beacon nodes, or the execution endpoint
through `eth_rpc_module` when the request names it (in Logos Core: with Sepolia in `required` mode the check was
answered by `verified_proxy_module`, [`results/verified-proxy-2026-11-14/`](results/verified-proxy-2026-11-14/README.md));
nothing they serve is trusted beyond the comparison. Its tests replay the requests and responses of a live run against Sepolia's public providers
(`calls/tests/fixtures/`, [`results/verify-checkpoint-2026-11-13/`](results/verify-checkpoint-2026-11-13/README.md)).
`verify_checkpoint` recomputes the instance id as the program defines it (`lez/wire/src/lib.rs`
`instance_id`: SHA-256 over the domain, `instance-id`, the Borsh encoding of `Config` and the
checkpoint). The module encodes `Config` itself; for the attestation fixture's checkpoint it gives
`0xc112f292…23bfe0`, the id of the harness's instance built from that checkpoint (`module_e2e info`).

A log end to end: `fetch_log_proof` → `dry_run_attest` (with the record read from LEZ) → `build_attest_tx`
→ the transaction at `account` → `read_attestation` on the program's event (`scripts/e2e.sh`).

### `ferrata_bridge_module`

The user's side of the bridge, over `ferrata-bridge-core` (specification v2), the crate the bridge
program, the release guest and the CLI use. Every method that derives keys takes the wallet's
`bridge_seed` (32 bytes) or the LEZ wallet's `lez_seed` (its 64-byte BIP-39 seed; the bridge seed is
`HMAC-SHA256(lez_seed, "ferrata/bridge-seed/v1")`, as `cli/src/seed.rs`). No reply contains `kAuth`,
`kView`, a deposit secret `s` or a burn secret `sOut`, except the claim messages: the plaintext `Claim`
carries `s` as the program's public claim does, and `ClaimPrivately` carries it as the input of the private
execution the LEZ wallet runs (it is not sent in the clear). The calls that read LEZ (a claim with state,
the root import, the registry) and the release rule run the `ferrata` tool's own code (`ferrata-wallet`,
`cli/wallet`: the tool's `lez::plan_claim`, `flow::pick_root`, `release::plan`, `release::prove` are thin
readers around it). The chain is `chain_id` (a number) or `evm_chain_id`
(a 32-byte word); a destination `kind` is `1`/`"private"` or `2`/`"public"`.

| Method | Takes | Returns |
|---|---|---|
| `version()` | — | the module's version, what was staged (`STAGED.json`), the method list |
| `deposit(request)` | seed, chain, `vault`, `depositor`, `j` (the depositor's deposit count), `kind`, `dest` (the LEZ account id), `token` (zero for ETH), optional `denom_index`; optionally `leaf_index` and `amount` (from the vault's `Deposit` event) | `commitment`; `vault_call` (`to`, `from`, `function`: `deposit(address,uint8,bytes32,uint64)` or `depositETH(uint8,bytes32,uint64)`, ABI `data`, what `value` must be); with `leaf_index`: `leaf`, `deposit_id`, `nullifier` |
| `claim(request)` | seed, chain, `vault`, `depositor`, `j`, `leaf_index`, `token`, `amount`, `kind`, `dest`; the deposit tree as `leaves` (all of them) or as `path` (`directions`, 32 `siblings`) and `root`; optionally `config_id`, `leaf_count` (of the imported root), `range` (`index`, `from_leaf`, `until_leaf` of the fee schedule), `relayer`, `private` | `verdict` `claimable` (with `nullifier`, `root`, `leaf`, `commitment`, `deposit_id`, `path`) or `refused` (`RootMismatch`, `DirectionsMismatch`); with `config_id`, `message`: Borsh of the bridge program's `Claim`, or of `ClaimPrivately` for a private destination (or `private`), sent at `root(cid, n)` |
| `claim(request)` with LEZ state | as above with `program` (the bridge program's account), `config_id`, the vault's `leaves`, `imported` (the leaf counts of the configuration's imported roots) or `leaf_count`, and the states as LEZ holds them: `root_state` (`root(cid, n)`), `token_state` (`token(cid, token)`), `nullifier_state` (`nullifier(chain, vault, N)`, absent or empty: unspent); `private` (default: a private destination's), `relayer`, `prover` (the private account that proves a public-kind private claim) | the tool's claim (`ferrata_wallet::claim`, as `ferrata claim`): the newest imported root that covers the leaf, the tree checked against it, the path, the schedule range, then the program's checks (`verify_claim`, the denomination, the destination kind, the nullifier, the fees); `claimable` with `variant` (`Claim` or `ClaimPrivately`), `message`, `accounts` (each account, its program and how the sender names it: `pda` with its seed, `private_self`, `signer`, `public`), `root_index`, `leaf_count`, `range`, `mint_fee`, `relay_fee`; or `refused` with the program's `code` and its name |
| `burn(request)` | seed, `config_id`, `i` (the wallet's burn counter), chain, `vault`, `recipient`, `token`, `amount`; optionally `burn_index` (from the program's burn record) | the note (`r`, `memo`, `mac`, `token`, `amount`, `c_out`); `message`: Borsh of the bridge program's `Burn` (variant 0); with `burn_index`: `burn_leaf`, `burn_id`, `nullifier` |
| `release(request)` | seed, `config_id`, `i`, `burn_index`, chain, `vault`, `recipient`, `token`, `amount`, optional `relayer`, `relay_fee`; the burn tree as `leaves` or `path` and `burn_root` | `verdict` `releasable` (with `nullifier`, `burn_root`, `r`, `journal`: the release guest's 352-byte journal the vault verifies, `journal_sha256`, `path`, the public inputs) or `refused` |
| `release(request)` by the wallet's rule | seed, `config_id`, chain, `vault`, `i` (the burn counter), `burns` (the zone's published burns, in order: `burn_index`, `note`), `roots` (the vault's accepted burn roots, oldest first: `root`, `count`, `index`, `finalized_at`, `delay` = `releaseDelayFor(token, index)`), `now` (Ethereum time); optional `relayer`, `relay_fee`, `gap_limit` | the tool's release (`ferrata_wallet::release`, as `ferrata release`): the burn recovered from the seed, the newest accepted root that covers it and whose release delay has passed (`picked`: root, count, index, delay, when it opened), the tree of its first `count` burns checked against it, the witness, `verify_release`, the journal and the guest's `input`; or `refused` with `BurnRootNotAccepted`, or `ReleaseDelayNotElapsed` and `first_open` |
| `prove_release(request)` | a `release` request (either form) and `program` (the release guest's binary); optional `expected_image_id`, `receipt_out` (a path for the receipt) | the release, then the release guest run on its input through r0vm (`ferrata_wallet::release::prove`, as `ferrata release --prove`; `RISC0_DEV_MODE=1`: executed, a fake receipt; unset: a succinct proof on the host's CPU, verified against the image id): `verdict` `proven`, `image_id`, `dev_mode`, `receipt_bytes`, `receipt_sha256`, `prove_ms`, `image_id_matches`. The guest's journal must equal the native one. The Groth16 wrap the vault takes is the relayer's |
| `import_root(request)` | `program`, `config_id`, `config_state` (`config(cid)` as LEZ holds it), `record_slot`, `get_proof` (`eth_getProof` of the vault with slots 0 and 1, at the record's block) or, for the module to read it itself from the vault the configuration names, `block` and `eth` or `rpc_url` (as `fetch_log_proof`; the reply then carries `eth`), optional `header` (the block's RLP header, when the record binds its hash only), `root` and `leaf_count` (default: the proof's slot values), `record_state` (the attestation program's `RecordState` at `header(id, slot)`) | the relayer's `ImportRoot` (`ferrata_wallet::import`): `message`, `accounts` (`config(cid)`, `header(id, slot)`, `root(cid, n)`); with `record_state`, a dry run with the two programs' own functions (the bridge's `import_request`, the attestation program's `evaluate`, the bridge's `check_import`): `importable` and the `root(cid, n)` state it writes, or `refused` with the code the transaction would exit with |
| `registry(request)` | any of `token_state` (with `leaf_index`, and `amount`), `config_state` (with `bridge_header`: the program's account), `root_state` | each decoded with the program's types: the token's entry (denominations, relay fees, LEZ definition and unit), its fee schedules, `known_until`, controls and gate; the range a claim of `leaf_index` names (the tool's `range_for`) and its mint and relay fees; the configuration (phase, instance, vault, controls) and `configId` recomputed; the imported root |
| `recover(request)` | seed, chain, `vault`, `gap`, `destinations` (`{kind, index, dest}`: the wallet's candidate accounts), `deposits` (the vault's `Deposit` events: `depositor`, `j`, `leaf_index`, `token`, `amount`, `commitment`); optionally `config_id`, `burns` (published notes with `burn_index`) and `gap_limit` | the wallet's own deposits (with `kind`, `account_index`, `leaf`, `nullifier`) and burns (with `i`, `recipient`, `leaf`, `nullifier`), `next_i` |
| `status(request)` | `deposits` (`leaf_index`, `nullifier`), `imported_leaf_count` (of the newest root imported on LEZ), `spent_nullifiers` (on LEZ); `burns` (`burn_index`, `nullifier`), `checkpointed_burn_count` (of the newest checkpoint the vault accepted), `released_nullifiers` (on the vault) | each deposit `awaiting_root`, `claimable` or `claimed`; each burn `awaiting_checkpoint`, `releasable` or `released` |
| `wallet(request)` | `{"method", "params"}`: one call of the mini-app's view (`open` with `chain`, the chain service's `http://` URL; then `info`, `registry`, `price`, `deposit`, `scan`, `claim`, `burn`, `release`, with the view's parameters); `lez` with `{"via": "lez_core", "wallet": {config, storage, password_file}, "program_bin"}` routes the session's LEZ reads, private holding and private sends through Logos Core's `lez_core` (`{"via": "plain"}` back), `lez_report` lists those calls beside the plain path, `lez_probe` asks `lez_core` about one of the bridge's PDAs | `{"ok": true, "result"}` in the view's shapes, or `{"ok": false, "error"}`. Each call is run with the methods above (`deposit`'s commitment and vault call; `recover` and the states for the positions; the claim with LEZ state and its `ClaimPrivately` message and accounts; `burn`'s note and `Burn` message; the release by the wallet's rule) over the chains' state, which a chain service reads and to which it signs and sends: the stand-in for Logos Core's own modules (`eth_rpc_module`, `keystore_module`/`tx_sender_module`, `lez_core`); on a LEZ sequencer `cli/src/bin/seqnet_chain.rs` (e2e/seqnet), whose methods are the mini-app devnet's `module.*` ones (`calls/src/session.rs`) |

### `ferrata_attestor_module`

`ferrata_attestor` (`checkpoint/attestor`, [its README](../checkpoint/attestor/README.md)) as a module:
the signer of finality rule (B) that each LEZ sequencer runs beside its Logos Blockchain node, and the
collector of `t` of `n` attestations into a `checkpoint_b` input. The methods are the CLI's commands over
the same library: the same records (`ferrata-attestation/v1`), the same checks, the same refusals. Keys
are 32-byte files (as LEZ keeps a sequencer's `channel_signing_key`); requests name them, replies never
carry a secret. Verdicts: the method's own, `refused` (the attestor or the collector refuses), `error`
(the node could not be read), `invalid_request`.

| Method | Takes | Returns |
|---|---|---|
| `version()` | — | the module's version, the attestor's version and the sources it was staged from (commit, sha256), the method list |
| `keygen(request)` | `out` (a new file: never overwritten, mode 600); `seed_label` only for reproducible devnet keys | `public_key` |
| `pubkey(request)` | `key_file` | `public_key` |
| `deployment(request)` | `input` (hex) or `input_file`: a `checkpoint_b` input; optional `out` | the `deployment` (chain, vault, `lezConfigId`, channel) its configuration serves |
| `certify(request)` | `channel_key_file`, `att_pub`, `deployment` or `deployment_file`; optional `out` | `cert`: the channel key's one certificate of the attestation key for this deployment (the CLI's record) |
| `tier(request)` | `answers_dir` (saved answers) or `node` (its HTTP API) with `save_dir` and `slot_from`; optional `channel`, `out` | the node's final tier as the attestor checks it: tip, final message id, key set, blocks checked, blocks per signer |
| `attest(request)` | `deployment`/`deployment_file`, `cert`/`cert_file`, `att_key_file`, the node (`answers_dir`, or `node` + `save_dir` [+ `slot_from`]); optional `state_file`, `out` | `attested` with the tip, the blocks checked, the key-set hash and the `attestation` record; or `refused` (certificate, channel key outside the live set, a tip below or beside one it signed, a final tier that does not check) |
| `verify(request)` | `deployment`/`deployment_file`; `attestations` (records) and/or `att_files` | per attestation whether its certificate and final-tier signature verify; `verified` or `refused` |
| `collect(request)` | `base`/`base_file` (a `checkpoint_b` input), `attestations` and/or `att_files`; optional `name`, `out`, `meta_out` | `collected` with `n`, `t`, kept and dropped (with the reason), the checkpoint rule's native run, the input's size and sha256, the fixture meta; `refused` below `t(n)` |
| `assemble(request)` | as `collect`, with `name` and `expect` | an input with exactly these attestations, unchecked, and the native run, which must refuse it with `expect` |
| `base(request)` | `answers_dir`, `genesis_file`, `n_min`, `t_min`, `out`; optional `from`, `to` (default: after the first inscribed block to the tip), `channel` | devnet set-up (the CLI's `base`): the input over those final blocks, its `lezConfigId`, parent state and deployment |
| `start(request)` | `deployment`/`deployment_file`, `cert`/`cert_file`, `att_key_file`, `node`, `work_dir`, `publish_dir`, `state_file`; optional `poll_secs` (10) or `poll_ms`, `slot_from`, `max_rounds`, `keep_rounds` | `started`: the loop asks the node every poll (its answers saved per round), checks the final tier, signs the tip, keeps the state file (never two histories) and publishes `att-<tip>.json` and `latest.json`; refused at once for a certificate that does not certify the key |
| `status()` | — | running, rounds, how many signed a new tip, found it unchanged, were refused or failed, the last round and attestation, the last 50 rounds |
| `stop()` | — | the loop ended after the round in hand; its final status |

### `ferrata_monitor_module`

The watchers' `monitor` ([`monitor/`](../monitor/README.md)) as a module. `run` and `start` take the
arguments of `monitor run` as a list (`{"args": [...]}`) and parse them with the binary's own parser
(`ferrata_monitor::cli`, shared since this module); the daemon is the binary's (`daemon::run_with`):
the same scan of the vault, verdicts, cancellations, pauses, revocations, attestation audit, alerts
(standard error, `--alert-log`, `--alert-cmd`) and state file. The watcher key reaches the module as a
file (`--key-file`, added for it) or a keystore: `--key-env` would name a variable of the module's own
process. The module sends the monitor's Ethereum transactions itself, over the JSON-RPC endpoint `--rpc`
names, as the binary does.

| Method | Takes | Returns |
|---|---|---|
| `version()` | — | the module's version, the monitor's version and sources (commit, sha256), the method list |
| `check(request)` | `rule` (`A`/`B`), `candidate` or `candidate_file`, `nodes` (node views, or sources `file:PATH`/`cmd:COMMAND`), `now`; optional `turn_seconds`, `b_lag_grace`, `delta_g`, `margin` | `monitor check`'s output: the verdict (`pass`, `wait` with `until`, `cancel` with the reason, `blind`) |
| `node_view(request)` | `chain_tsv` or `chain_tsv_file` (the lab driver's block record); optional `label`, `key_sets`/`key_sets_file` | `view`: `monitor node-view`'s node view |
| `run(request)` | `args`: `monitor run`'s arguments, with `--max-polls` (a run without a bound is `start`) | `ok` (the binary's exit 0) or `error` (its exit 1, with its message: a lock held by another monitor, an unreachable node), and the state file after the run |
| `start(request)` | `args`: `monitor run`'s arguments | `started` once the daemon has taken its lock and saved its state, or the daemon's error |
| `status()` | — | running, the exit when it ended (0 or 1 with the error), the state file (polls, candidates, cancellations, audits) |
| `stop()` | — | the daemon ended after the poll in hand: a `stopped` alert, the state saved, the lock released (the binary's SIGTERM) |

### `ferrata_updater_module`

`ferrata-updater` ([`lez/updater`](../lez/updater/README.md)) as a module: the same configuration file
(`ferrata-updater.example.toml`), the same set-up (`ferrata_updater::daemon`, shared since this module)
and the same rounds, over either of the binary's backends. `ledger = "harness"`: LEZ v0.4.0-rc1's state
machine in the module's process with LEZ's fee program and settlement and the attestation program at genesis
(the chain the updater's tests run on, `RISC0_DEV_MODE=1` in the daemon's environment as for those tests).
`ledger = "sequencer"`: a running LEZ sequencer on this machine (`[sequencer]`), through
`lez/updater/sequencer/`'s library, the code its binary runs: LEZ's wallet client sends, a follower applies
every block final on Bedrock with LEZ's block application and reads each result; `stop` and the periodic save
keep the follower in the state directory, and `start` restores it. The backend lives in the daemon's thread,
so `stop` ends it. `status` and `metrics` give what `/health` and `/metrics` give; with `daemon.listen` set,
the module also serves those on that address. A source may be Logos Core's `beacon_api_module`
(`[[chain.source]] via = "beacon_api_module"`, `chain_id`, optionally `provider`, the one it asks first): the module
asks its `lc_request` by name and the updater turns the providers' JSON into the SSZ it reads
(`lez/updater/src/lc_json.rs`); on Sepolia it followed the crown runs' checkpoint to the current finalized header that
way ([`results/verified-proxy-2026-11-14/`](results/verified-proxy-2026-11-14/README.md)).

| Method | Takes | Returns |
|---|---|---|
| `version()` | — | the module's version, the updater's sources (commit, sha256), LEZ's revision, the guests' image ids as staged, the attestation program's id declared and computed from the bytes the module carries, `RISC0_DEV_MODE` |
| `check_config(request)` | `config_file` | the file as the daemon reads it (chains, sources, ledger), or `invalid_request` with the binary's refusal |
| `run(request)` | `config_file`, `rounds`; optional `until_up_to_date`, `quiet` | after the rounds: each instance's health, its state on the chain (finalized slot and header root, period, jobs applied), the notes, the metrics |
| `start(request)` | `config_file`; optional `rounds`, `quiet` | `started` once the set-up is done, or the binary's error (an unreadable key, the sequencer backend) |
| `status()` | — | running, the round, `up_to_date`, each instance's health and state on the chain, the last notes |
| `metrics()` | — | the Prometheus text of `/metrics` as the last round left it |
| `stop()` | — | the daemon ended after the round in hand; its final status |

### `ferrata_relayer_module`

`ferrata-relayer` ([the operator guide](../docs/guides/operator-guide.md#6-the-relayer-and-the-wrapper) §6) as a
module: the daemon is `ferrata_cli::relayerd`, the code the binary runs (moved into `cli/`'s library for this
module), so the arguments, the rounds, the checks and the JSON lines are the binary's. `run` and `start` take the
binary's arguments as a list (`{"args": [...]}`: the global options, then `run ...` or `status`) and parse them with
its own parser; the lines the binary prints are returned (`run`) or kept (`start`, read with `status`). The
relayer's Ethereum side is the binary's (alloy over the JSON-RPC endpoint `--eth-rpc` names; a keystore file and its
password file, or `--unlocked` on a local anvil); its LEZ side is the mini-app devnet's zone (`--lez-devnet`, a
stand-in: reads with `zone.applied` and `zone.actor_state`, calls sent with `module.submit`, which the devnet signs
and settles in its LEZ harness); `--sequencer` is refused by this build (no LEZ wallet). Paths in the arguments are
read by the module's process: give them absolute.

| Method | Takes | Returns |
|---|---|---|
| `version()` | — | the module's version, what was staged (`STAGED.json`: the commit, the sources' SHA-256, LEZ's revision), `lez_wallet: false`, the zones it reaches, the method list |
| `run(request)` | `args`: `ferrata-relayer`'s arguments ending in `run --rounds N` (N > 0) or `status` | `ok` (the binary's exit 0) or `error` (its exit 1, with the message it prints), and `events`: the lines it prints; `invalid_request` for arguments the binary's parser refuses, or a run without an end (that is `start`) |
| `start(request)` | `args`: as `run`, ending in `run ...` (`--rounds` 0 or absent: until `stop`) | `started` once the daemon has set itself up (its `start` line: the endpoint read, the vault and chain checked, the zone reached), with its status; or its error |
| `status(request)` | optional `since` (a line index) | `running`, how it ended (`exit` 0 or 1, `error`), the arguments, `events_total`, and the lines from `since` on (the newest 10,000 are kept) |
| `stop(request)` | optional `reason` (default `stop`) and `since` | the daemon ends after the round in progress and logs `stop` with that reason (`scripts/relayer-module.sh` passes the signal it received, as the binary names its own); its final status |

**Root imports** (`--import-roots`, also in the binary): in the first round of each `--import-every` period of
Ethereum time (default 3,600 s; bridge.md §5.4: a fixed cadence, never in response to a deposit) the relayer reads
`config(cid)`, the configuration's light-client instance (`instance(id)`: its store's finalized slot) and its
record `header(id, slot)`, checks that its Ethereum endpoint's block at the record's number has the recorded hash,
reads the vault's slots 0 and 1 there with `eth_getProof`, and when the count is above the newest imported one
builds `ImportRoot` with `ferrata_wallet::import` (`import_proof`, `import_call`) and dry-runs it with the two
programs' own functions (`dry_run_import`: the attestation program's `evaluate`, the bridge program's
`check_import`) before sending it: the code `ferrata_bridge_module`'s `import_root` runs, so the relayer's message is
byte for byte the module's for the same record and proofs (`relayer-e2e.sh` compares them). An import the programs
would refuse is not sent (`root-import-refused` with the code); one sent is confirmed from `root(cid, n)`
(`root-imported`). Lines: `root-imported`, `root-import-waiting` (no newer record covers the vault's deposits, or
no instance or record yet), `root-import-refused`, `root-import-failed`, `root-import-closed` (sunset).

### Tests

| Script | What it runs | Where |
|---|---|---|
| `scripts/native.sh` | stages the modules' crates, the calls crates' unit tests (`cargo test`), builds the `call` examples (default the four that need only cargo; `MODULES=...` adds the updater module, which needs the guests of `lez/methods` built) | any Linux with cargo |
| `scripts/check.sh [OUT_DIR]` | the above, then the six modules built and packaged by logos-module-builder (dev `.lgx` and `.lgx-portable`), installed and loaded together in a headless Logos Core (`logosctl`, dev and portable builds), every one-shot method called (`requests.py`: 10 attest requests from the attestation fixtures; `bridge_requests.py`: 13 bridge requests from `bridge/vectors/*-v2.json`; `daemon_requests.py`: 29 requests for the four daemon modules, from the zone run's records, a node view and the lab record, the updater's configurations, the relayer's arguments), each reply compared with the native reply and with what the request must produce; then `e2e.sh` and the four daemons' jobs (below) through the same daemon (the relayer's in a Logos Core of its own), each first run natively, and with `BRIDGE_E2E=1` `bridge_e2e.sh` too | Linux x86-64 with Nix, the RISC Zero toolchain of `lez/`, r0vm, Foundry (`RELEASE_ELF` for `BRIDGE_E2E=1`) |
| `scripts/e2e.sh OUT_DIR` | a log end to end: `recorded_rpc.py` serves the recorded mainnet block 26,384,851 over JSON-RPC; `fetch_log_proof` → `dry_run_attest` on the record read from the chain → `build_attest_tx` → `lez/harness`'s `module_e2e` decodes the bytes with the program's types and executes them in LEZ v0.4.0-rc1's state machine → `read_attestation`; two logs, two refusals (a tampered proof, another instance); `advance`: the same instance before its finality update, the update planned by the module, its `Open`, `Miller`, `Finish` executed in order, the instance compared with the one the harness's own job leaves, read back with `read_instance`, and the update again (no change); and the bridge module's `Burn` and `Claim` decoded by the bridge program's types (`CALL=logosctl` runs it through a Logos Core daemon) | Linux with cargo (and `logosctl`) |
| `scripts/bridge_e2e.sh OUT_DIR` | the bridge module against the mini-app's devnet (`cli/examples/miniapp_devnet.rs`: the v5 vault on anvil, chain id 1, its verifier the vault tests' mock; the zone in the LEZ harness with the bridge program): two private deposits; `import_root` from a block the devnet records (its proofs in `eth_getProof`'s shape), dry-run and applied, and a wrong count refused by the dry run and by the program with the same code; `registry`; both deposits claimed privately (`claim` with state → `ClaimPrivately`, proven and settled privately by the devnet with the wallet's keys) and a second claim refused from the nullifier's state; two private burns, a checkpoint over the first (its delay passed) and one over both (in its delay): `release` by the rule picks the older root, refuses the newer burn until its root opens, then picks the newer; `prove_release`; both releases accepted by the vault; `status` (`CALL=logosctl` through a daemon) | Linux with cargo, Foundry, `RELEASE_ELF` (and `logosctl`) |
| `scripts/attestor-e2e.sh OUT_DIR` | the attestor's job on the four-sequencer zone's records (`checkpoint/attestor/runs/zone`): tier, base, each sequencer's key, certificate and attestation (each equal to the run's), verify, collect 3 of 4 (the input the checkpoint program proved, byte for byte), 4 of 4, 2 of 4 refused, three refusals to sign; then the module's loop against a node served from the same answers over HTTP (`recorded_node.py`): 23 checks | Linux with cargo (and `logosctl`) |
| `scripts/attestor-zone-live.sh BUNDLE OUT_DIR` | a running zone of four LEZ sequencers (`checkpoint/attestor/zone.sh`, the lab bundle) and four Logos Cores, one per sequencer, each running the attestor module's loop against the zone's node while it runs; then the module's collector | Linux x86-64, the lab bundle, `logosctl` |
| `scripts/updater-e2e.sh OUT_DIR` | the updater's job on its tests' chain: the in-process LEZ v0.4.0-rc1 chain, the lab's recorded mainnet and Sepolia data, from the checkpoints to the lab's final roots (`start`, `status` until up to date, `metrics`, `stop` through Logos Core; `run` natively); the sequencer backend carried (with no sequencer, its set-up fails on the connection) | Linux with cargo, r0vm (and `logosctl`) |
| `scripts/monitor-e2e.sh OUT_DIR` | `monitor/tests/anvil-e2e.sh` (a local anvil chain, a v5 vault, scenarios S1–S11, A0–A4, R1–R4, the alerting check: 46 checks) with the binary, or with the module through `scripts/monitor-module.sh` (`CALL=logosctl`): S5's SIGKILL then kills the module's host process and the module is loaded again | Linux with cargo and Foundry (and `logosctl`) |
| `scripts/relayer-e2e.sh OUT_DIR` | the relayer's job on its devnets, with the binary (the reference) or the module (`CALL=logosctl`, `scripts/relayer-module.sh` in the binary's place, in a Logos Core of its own started without `RISC0_DEV_MODE`): `cli/tests/relayer_fork.rs` (an anvil fork of Sepolia at the block before `crown-live-1`'s `submitCandidate`, that run's real checkpoint and release proofs: the checkpoint, its finalisation after W, the releases, one wrapped from a succinct receipt, fees, refusals, `status`, a clean stop on SIGTERM); root imports on the mini-app's devnet (the light client's finality update stood in by the devnet's `lc.finalize`: anvil's newest block recorded, `instance(id)` moved to it; the proofs anvil's own `eth_getProof`): waiting without a light client, n = 2 imported and byte-equal to `ferrata_bridge_module`'s `import_root`, nothing sent when up to date, waiting for a later record, a record the proofs do not match refused with the bridge module's code and not sent, n = 3, and the daemon on the default cadence (no import within the hour, n = 4 in the next) stopped cleanly | Linux with cargo, Foundry, r0vm (and `logosctl`); the fork reads public Sepolia state |
| `RELEASE_ELF=... scripts/logos-io.sh OUT_DIR` | Logos Core's own modules built from their sources with our builder pin (`eth_rpc_module`, `beacon_api_module`, `keystore_module`, `signer_manager_module`, `fee_module`, `tx_sender_module`), `check.sh` for the attestation and bridge modules, then one Logos Core holding those that build (`keystore_module` does not with our pin): `e2e.sh` and `bridge_e2e.sh` with `ETH_VIA=eth_rpc_module` (every Ethereum read through `eth_rpc_module`, compared with the plain path's), and a send through `tx_sender_module` on anvil driven from the command line, recorded step by step; `report/summary.md` ([LOGOS-CORE-MODULES.md](LOGOS-CORE-MODULES.md), [results/logos-core-io-2026-11-13/](results/logos-core-io-2026-11-13/)) | Linux x86-64 with Nix, cargo, Foundry, r0vm; GitHub for the Logos modules' sources |
| `scripts/release-local.sh OUT_DIR` | `logos-modules-release-action`'s steps for `linux-amd64`, locally, for the six modules (`MODULES`; `DRY_RUN_DIR` for the record of some of them), nothing published (`catalog/README.md`) | Linux x86-64 with Nix |
| `scripts/catalog-install.sh RELEASE_DIR LOGOSCTL WORK_DIR` | a Logos Core installs the six modules (`MODULES`) from that catalog, served over HTTPS on 127.0.0.1: `catalog add`, `search`, `install`, `module load`, `call`; with `CATALOG_E2E=1` (and `CATALOG_E2E_MORE`), the daemons' jobs through the installed modules | Linux x86-64 with Nix |

Logos Core keeps each module's socket in `TMPDIR`; a Unix socket path holds 107 bytes, so the scripts
that start a Logos Core refuse a `TMPDIR` (or `LOGOS_TMPDIR`) longer than 62 bytes.

### Left out

- **Logos Core's own network and keys** ([LOGOS-CORE-MODULES.md](LOGOS-CORE-MODULES.md)). The attestation
  module's Ethereum reads (`fetch_log_proof`'s block and receipts, the storage proofs, `verify_checkpoint`'s execution
  check) and the bridge module's `import_root` (the vault's `eth_getProof`) go through Logos Core's `eth_rpc_module`
  when the request names it (`eth`), plain `http://` JSON-RPC staying the fallback; shown in a headless Logos Core
  with `eth_rpc_module` built from its source (`scripts/logos-io.sh`). Through `verified_proxy_module` (built from its
  source, nimbus-eth1 v0.4.4's library included) with Sepolia in `required` mode: `fetch_log_proof` of the live runs'
  deposits, the capture blocks' headers, the vault's storage values (`eth_getStorageAt`, verified against the state
  root) and `verify_checkpoint`'s execution check, each equal to the runs' records and to a plain read
  ([`results/verified-proxy-2026-11-14/`](results/verified-proxy-2026-11-14/README.md), `scripts/verified-proxy.sh`);
  not on a devnet (the proxy serves mainnet, Sepolia and Hoodi), and not for `eth_getProof`, which it refuses
  (`unknown method`: nimbus's library has no verified `eth_getProof`), so storage proofs come from a chain in `off`
  mode and are checked by the programs against a record. The updater's beacon data goes through `beacon_api_module`
  when its configuration says so (`via = "beacon_api_module"`; shown on Sepolia on the in-process LEZ chain); the
  other modules' reads (the relayer's and monitor's alloy providers, the attestor's node) do not. LEZ
  state (a record, an event, nullifiers, counts) comes in the request, except in `ferrata_bridge_module`'s wallet
  session, which reads its bridge states and its private holding and sends its private claim and burn through
  `lez_core` at its merged v0.4 state (`cc73ebf`, with
  [`patches/lez_core-cc73ebf-authorizes-public-pda.patch`](patches/lez_core-cc73ebf-authorizes-public-pda.patch):
  as merged it mentions no account authorizing and has no PDA account kind) when the session says so
  (`wallet lez`; [`results/lez-core-v04-2026-11-14/`](results/lez-core-v04-2026-11-14/)); the other modules' requests
  and the zone's history do not go through it. The modules build each program message and name the account it goes
  to; apart from those two private sends they do not sign or submit: `fee_module` and `tx_sender_module` do not load
  with our builder pin (`keystore_module` and `signer_manager_module` do, with the builder's own locks; built with an
  overridden builder, `keystore_module`'s install fails on its interface contract, because the override keeps its old
  code generator; with the builder's own locks, `eth_rpc_module` 6965f32, which both need, does not compile against
  the new generator's clients), so no Ferrata module sends Ethereum transactions through them. In the end-to-end test the
  harness wraps the message in a LEZ transaction and executes it in LEZ's state machine in-process,
  not on a networked sequencer. The daemon modules likewise use no other Logos Core module: the attestor reads
  its node over plain HTTP (`curl`, as the binary does) and keeps its keys in files; the monitor signs
  and sends its Ethereum transactions itself (alloy, over the JSON-RPC endpoint it is given) with a key
  file or keystore; the updater's sources are beacon API URLs or recorded directories unless its configuration names
  `beacon_api_module`.
- **`ferrata_attest_module`:** `advance` plans a job for an active instance (`Open`, `Miller`, `Adopt`,
  `Finish`); an instance's initialization (`Init` and its committee's `Adopt` parts) is not planned, nor
  the updater's own work around the plan (several sources cross-checked, cooperation with other updaters,
  retries); the instance's configuration is read from its state (`read_instance`), which comes in the
  request; the dry run still lacks the program-only checks (`WRONG_INSTANCE` is shown by the program in
  `e2e.sh`); no history hops, batches, conditions or relations, no accumulator leaves; no `KeyValidate` on
  the committee (§11.4 of the attestation note).
- **`ferrata_bridge_module`:** the private claim is built and checked by the module (`ClaimPrivately`),
  but its private execution and proof are the LEZ wallet's (on the devnet, the harness proves and settles
  it with the wallet's keys, as LEZ's wallet would); `import_root` takes the vault's `eth_getProof` and the
  record in the request (on the devnet the block is a synthetic one holding the vault's real storage);
  `prove_release` needs r0vm 3.0.5 on the module's host (`RISC0_SERVER_PATH`) and gives a succinct
  receipt, not the Groth16 seal the vault takes (the relayer wraps it); the vault's `Deposit` events, the
  program's burn records and every LEZ state come in the request. `wallet` (the mini-app's calls) runs these methods
  over a chain service that reads both chains and signs and sends for it (`cli`'s `seqnet-chain`, a stand-in for
  `eth_rpc_module`, `keystore_module`/`tx_sender_module` and `lez_core`: LEZ's wallet library, a keystore, the
  relayer); so it ran the private round trip on a running LEZ sequencer (`e2e/seqnet/`, EVIDENCE row 103), and does
  not read through `eth_rpc_module` yet. With `wallet lez` its LEZ side goes through Logos Core's `lez_core` instead
  (every bridge state it reads, compared with the chain service's; the private holding; the private claim and burn,
  proven by LEZ's wallet inside Logos Core), the chain service kept beside it and as the fallback
  (`e2e/runs/seqnet-lezcore-1/`, EVIDENCE rows 113, 114).
- **`ferrata_attestor_module`:** attestations are published as files (`att-<tip>.json`, `latest.json`),
  not to a relay or a watcher feed, and the collector reads files; the loop asks its node for the whole
  final tier every round (from `slot_from`); the zone of the live run has one Logos Blockchain node that
  the four Logos Cores share (as the zone run's attestors did), and no bridge program; the module's
  checkpoint input is checked by the checkpoint rule natively, not proven here.
- **`ferrata_monitor_module`:** node views still come through the adapter (`file:`/`cmd:`): no client of
  the LEZ sequencer's or the Logos Blockchain node's RPC (`monitor/README.md`, "Not built"); one daemon
  per module instance.
- **`ferrata_relayer_module`:** no LEZ wallet: its LEZ side is the mini-app devnet's zone (a stand-in over the
  devnet's HTTP methods; the devnet signs and settles), not a sequencer; the binary keeps the sequencer path
  (`--sequencer`, LEZ's wallet), which has not run with root imports (the crown runs imported their roots with
  their own driver). Gas drops need a sequencer (the devnet's zone sends no native credit). On the devnet the light
  client's finality update is the devnet's (`lc.finalize`): the record of anvil's block and `instance(id)` are
  written into the harness's state, with as state root the root of the trie anvil's `eth_getProof` answers from.
  A Gloas record (a block hash only) is waited on, not imported: the relayer does not fetch the block's header yet.
  The Groth16 wrap runs RISC Zero's prover image through `--wrap-command` (on the build box a stub returning the
  proof the image made on Modal, as in `relayer_fork.rs`).
- **`ferrata_updater_module`:** two updater modules share a chain only on a sequencer (the in-process chain
  lives in one module's process and ends with `stop`); the cooperative A3 gate (two honest updaters and an
  adversary) is the binary's test, in process; the sequencer backend ran in one Logos Core on a one-validator
  crown zone (EVIDENCE row 104), not beside other updaters; the package links LEZ's node-side crates (61 MB
  portable, against 16 MB with the harness alone) and its host needs `curl` and `libpcsclite`; built with Rust
  1.98.0, LEZ pins 1.98.1; the guests it embeds are the ones risc0-build made on the build box (their ids checked against
  `docs/image-ids.md`).
- **Catalog:** laid out, dry-run and installed from on 127.0.0.1 (`catalog/`); nothing published, one
  variant (`linux-amd64`), unsigned.
- **Linux x86-64 only** was tried.
