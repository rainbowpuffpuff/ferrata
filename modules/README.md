# Logos modules

The proposal (§B7) puts everything off chain in Logos modules. This directory holds five, prototypes,
built the way Logos modules are built, loaded together by a headless Logos Core and called there:
`ferrata_attest_module` (seven methods: dry runs, checkpoint checks, an instance's state read back, and a
log or storage slot attested end to end: fetched over JSON-RPC, its `Attest` transaction built, the
program's event read back),
`ferrata_bridge_module` (seven: deposit, claim, burn, release, recover, status), and the three daemons
as modules: `ferrata_attestor_module` (rule (B)'s attestor beside a sequencer and its collector,
`checkpoint/attestor`), `ferrata_monitor_module` (the watchers' `monitor`, `monitor/`) and
`ferrata_updater_module` (the light-client updater `ferrata-updater`, `lez/updater`). `catalog/` lays out
the catalog that would release them with `logos-modules-release-action`. `RESULTS.md` records what ran.

```sh
modules/scripts/native.sh     # any Linux with cargo: stage, unit tests, the native call examples
modules/scripts/e2e.sh OUT    # + the lez/ harness: a log end to end (methods called natively)
modules/scripts/check.sh      # Linux x86-64 with Nix: the above, then build, package, load and call all
                              # five modules in Logos Core (dev and portable), e2e.sh through the daemon,
                              # and the three daemons doing their jobs as modules (natively first)
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
| `ferrata_attest_module` | `ferrata_attest_module/` | `version`, `dry_run_attest`, `verify_checkpoint`, `fetch_log_proof`, `build_attest_tx`, `read_attestation`, `read_instance` |
| `ferrata_bridge_module` | `ferrata_bridge_module/` | `version`, `deposit`, `claim`, `burn`, `release`, `recover`, `status` |
| `ferrata_attestor_module` | `ferrata_attestor_module/` | `version`, `keygen`, `pubkey`, `deployment`, `certify`, `tier`, `attest`, `verify`, `collect`, `assemble`, `base`; the loop: `start`, `status`, `stop` |
| `ferrata_monitor_module` | `ferrata_monitor_module/` | `version`, `check`, `node_view`, `run`; the daemon: `start`, `status`, `stop` |
| `ferrata_updater_module` | `ferrata_updater_module/` | `version`, `check_config`, `run`; the daemon: `start`, `status`, `metrics`, `stop` |

All five are laid out the same way:

- `metadata.json`, `flake.nix`, `flake.lock`, `CMakeLists.txt`: the builder's Rust template, with the builder pinned
  to the commit above.
- `rust-lib/`: the module crate (the trait and its implementation, a static library) and `calls/`,
  the methods' logic as a plain crate that runs and is tested without the generated scaffold
  (`calls/examples/call.rs` runs one method natively: `call METHOD [REQUEST_FILE]`).
- The crates each module links are copied into `rust-lib/` before a build (`scripts/stage.sh`), because
  the builder copies only `rust-lib/`: `crates/ferrata-core` into the attestation module (without a
  BLS backend: no method verifies a signature), `bridge/crates/ferrata-bridge-core` into the bridge
  module, `checkpoint/attestor` with `checkpoint-core` and `attestor-core` into the attestor module
  (in `checkpoint/`'s layout), `monitor/` (its library) with `attestor-core` into the monitor module. The
  copies are ignored by git; each `rust-lib/Cargo.toml` carries the workspace fields its copies inherit
  (`stage.sh` checks them).
- The attestation and bridge modules link no LEZ crate: their messages are Borsh-encoded field for field,
  and the end-to-end test decodes and executes them with the programs' own types
  (`lez/harness/src/bin/module_e2e.rs`). The updater module does link LEZ: the updater writes to LEZ's
  state machine. `scripts/stage-updater.sh` copies into `rust-lib/vendor/` the updater, the `lez/` harness
  and the crates they use in the repository's own layout with the workspace manifests they inherit from,
  and the LEZ v0.4.0-rc1 tree (its `artifacts/`, which LEZ's build scripts embed); `lez/methods`, whose
  build script runs risc0-build, is replaced there by a crate of the same name that embeds the guests
  risc0-build made for it on the build box (same constants, same bytes; `version()` reports the image id
  declared and the one computed from the bytes, and `updater-e2e.sh` checks both against
  [`docs/image-ids.md`](../docs/image-ids.md)). No guest is changed or rebuilt by the module builder.
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

**Calling convention (all five).** Each method takes one JSON string and returns one JSON string, so a call needs no type beyond
`String` and the request can come from a file (`logosctl call ferrata_bridge_module claim @request.json`).
Byte strings are `0x` hex; 64-bit integers are JSON numbers, decimal strings or `0x` quantities;
`uint256` amounts are `0x` hex of at most 32 bytes or decimal strings, and are returned as 32-byte hex.
A request that cannot be read returns `{"verdict":"invalid_request","error":...}`; a panic inside a
method returns `{"verdict":"error",...}` instead of crossing into Logos Core. Every other reply has a
`verdict`.

### `ferrata_attest_module`

| Method | Takes | Returns |
|---|---|---|
| `version()` | — | the module's version, the `ferrata-core` version and the source revision it was staged from, which BLS backend is compiled in (none), the method list |
| `dry_run_attest(request)` | the consumer's `chain_id`; a header `record`: either `{chain_id, encoded}` (the bytes the program stores at `header(id, slot)`, `RecordState::record`, as read from LEZ) or its fields (instance chain id, anchor slot, block number, timestamp, block hash, parent hash, state root, receipts root); and either `storage` (an `eth_getProof` result and the slot) or `log` (`{tx_index, log_index, receipt_proof}`, as `fetch_log_proof` returns it) | `verdict` (`attested` or `refused`), `code` (the program's code: 0, or the core's refusal code, which the program returns unchanged, `lez/wire/src/lib.rs` `refusal`), the error's name, and for an attested fact its fields and statement identifiers (`id`, `id_at_block`) |
| `verify_checkpoint(request)` | a `config` (the program's `Config` fields), a `checkpoint` root, optionally the `expected_instance_id`, the `bootstrap` (SSZ, hex) and `observed` roots for the checkpoint as other sources report them | the recomputed `instance_id`; whether it equals the expected one; the program's `Init` verdict on the bootstrap (`StoreCore::from_bootstrap`: layout, header, header root against the checkpoint, committee branch) with its code; each observed root against the checkpoint; whether the chain id and `genesis_validators_root` are a known pair; an overall `verdict` |
| `fetch_log_proof(request)` | `rpc_url` (`http://` only: a local node or `scripts/recorded_rpc.py`), `block`, and `tx_index` + `log_index` (within the receipt) or `block_log_index` (JSON-RPC's `logIndex`); optional `timeout_ms` | `verdict` `fetched` (or `inconsistent` when the receipts do not rebuild the block's `receiptsRoot`), the block hash and receipts root, `log` = `{tx_index, log_index, receipt_proof}` ready for `dry_run_attest` and `build_attest_tx`, what the provider said the log is (`provider_log`), `rpc_calls` |
| `build_attest_tx(request)` | `program_id` (the attestation program's account id), `instance_id`, `chain_id`, `slot` (of the record), and `log`, `logs` or `storage` | `account`: `header(id, slot)` (LEZ's public PDA of the program, seed SHA-256(`ferrata-lez/v1/header` ‖ id ‖ slot LE)), the account list, `message`: Borsh of `Message::Attest { request }` (variant 7), its size and SHA-256, the number of facts |
| `read_attestation(request)` | `event_data` (the program's `FRT-ATT1` event data, hex), optionally `selector`, `expected_ids` (the dry run's `id`s), `expected_instance_id` | `instance_id`, `chain_id`, `block_number`, the `statements`, `matches_expected`, `instance_matches` |
| `read_instance(request)` | `state`: the instance's state as LEZ holds it at `instance(id)` (the program's `InstanceState`, Borsh, hex); optionally `checkpoint` and `expected_instance_id` | `id`, `config` (the program's `Config`, as `verify_checkpoint` takes it), `active`, `applied` (jobs), `store` (fork, finalized header and slot, whether the next committee is known, the store digest); with `checkpoint`, the id recomputed and `id_matches_config`; `instance_matches` |

The record is built the program's way (`witness::HeaderRecord`, then `attest::HeaderRecord::from_recorded`,
as `evaluate` does), and the proof is checked by `attest_storage` or `attest_log`, so the verdict and
code are the ones the program would return for that proof against that record.

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
`kView`, a deposit secret `s` or a burn secret `sOut`, except the plaintext `Claim` message, which
carries `s` as the program's public claim does. The chain is `chain_id` (a number) or `evm_chain_id`
(a 32-byte word); a destination `kind` is `1`/`"private"` or `2`/`"public"`.

| Method | Takes | Returns |
|---|---|---|
| `version()` | — | the module's version, the `ferrata-bridge-core` version and source revision, the method list |
| `deposit(request)` | seed, chain, `vault`, `depositor`, `j` (the depositor's deposit count), `kind`, `dest` (the LEZ account id), `token` (zero for ETH), optional `denom_index`; optionally `leaf_index` and `amount` (from the vault's `Deposit` event) | `commitment`; `vault_call` (`to`, `from`, `function`: `deposit(address,uint8,bytes32,uint64)` or `depositETH(uint8,bytes32,uint64)`, ABI `data`, what `value` must be); with `leaf_index`: `leaf`, `deposit_id`, `nullifier` |
| `claim(request)` | seed, chain, `vault`, `depositor`, `j`, `leaf_index`, `token`, `amount`, `kind`, `dest`; the deposit tree as `leaves` (all of them) or as `path` (`directions`, 32 `siblings`) and `root`; optionally `config_id`, `leaf_count` (of the imported root), `range` (`index`, `from_leaf`, `until_leaf` of the fee schedule), `relayer` | `verdict` `claimable` (with `nullifier`, `root`, `leaf`, `commitment`, `deposit_id`, `path`) or `refused` (`RootMismatch`, `DirectionsMismatch`); with `config_id` and a public destination, `message`: Borsh of the bridge program's `Claim` (variant 5), sent at `root(cid, n)` |
| `burn(request)` | seed, `config_id`, `i` (the wallet's burn counter), chain, `vault`, `recipient`, `token`, `amount`; optionally `burn_index` (from the program's burn record) | the note (`r`, `memo`, `mac`, `token`, `amount`, `c_out`); `message`: Borsh of the bridge program's `Burn` (variant 0); with `burn_index`: `burn_leaf`, `burn_id`, `nullifier` |
| `release(request)` | seed, `config_id`, `i`, `burn_index`, chain, `vault`, `recipient`, `token`, `amount`, optional `relayer`, `relay_fee`; the burn tree as `leaves` or `path` and `burn_root` | `verdict` `releasable` (with `nullifier`, `burn_root`, `r`, `journal`: the release guest's 352-byte journal the vault verifies, `journal_sha256`, `path`, the public inputs) or `refused` |
| `recover(request)` | seed, chain, `vault`, `gap`, `destinations` (`{kind, index, dest}`: the wallet's candidate accounts), `deposits` (the vault's `Deposit` events: `depositor`, `j`, `leaf_index`, `token`, `amount`, `commitment`); optionally `config_id`, `burns` (published notes with `burn_index`) and `gap_limit` | the wallet's own deposits (with `kind`, `account_index`, `leaf`, `nullifier`) and burns (with `i`, `recipient`, `leaf`, `nullifier`), `next_i` |
| `status(request)` | `deposits` (`leaf_index`, `nullifier`), `imported_leaf_count` (of the newest root imported on LEZ), `spent_nullifiers` (on LEZ); `burns` (`burn_index`, `nullifier`), `checkpointed_burn_count` (of the newest checkpoint the vault accepted), `released_nullifiers` (on the vault) | each deposit `awaiting_root`, `claimable` or `claimed`; each burn `awaiting_checkpoint`, `releasable` or `released` |

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
and the same rounds. The chain is the binary's `ledger = "harness"`: LEZ v0.4.0-rc1's state machine in
the module's process with LEZ's fee program and settlement and the attestation program at genesis (the
chain the updater's tests run on, `RISC0_DEV_MODE=1` in the daemon's environment as for those tests); it
lives in the daemon's thread, so `stop` ends it. `ledger = "sequencer"` is refused as the binary refuses
it. `status` and `metrics` give what `/health` and `/metrics` give; with `daemon.listen` set, the module
also serves those on that address.

| Method | Takes | Returns |
|---|---|---|
| `version()` | — | the module's version, the updater's sources (commit, sha256), LEZ's revision, the guests' image ids as staged, the attestation program's id declared and computed from the bytes the module carries, `RISC0_DEV_MODE` |
| `check_config(request)` | `config_file` | the file as the daemon reads it (chains, sources, ledger), or `invalid_request` with the binary's refusal |
| `run(request)` | `config_file`, `rounds`; optional `until_up_to_date`, `quiet` | after the rounds: each instance's health, its state on the chain (finalized slot and header root, period, jobs applied), the notes, the metrics |
| `start(request)` | `config_file`; optional `rounds`, `quiet` | `started` once the set-up is done, or the binary's error (an unreadable key, the sequencer backend) |
| `status()` | — | running, the round, `up_to_date`, each instance's health and state on the chain, the last notes |
| `metrics()` | — | the Prometheus text of `/metrics` as the last round left it |
| `stop()` | — | the daemon ended after the round in hand; its final status |

### Tests

| Script | What it runs | Where |
|---|---|---|
| `scripts/native.sh` | stages both modules' crates, the calls crates' unit tests (`cargo test`), builds the `call` examples | any Linux with cargo |
| `scripts/check.sh [OUT_DIR]` | the above, then both modules built and packaged by logos-module-builder (dev `.lgx` and `.lgx-portable`), installed and loaded together in a headless Logos Core (`logosctl`, dev and portable builds), every method called (`requests.py`: 10 attest requests from the attestation fixtures; `bridge_requests.py`: 13 bridge requests from `bridge/vectors/*-v2.json`), each reply compared with the native reply and with what the request must produce; then `e2e.sh` through the same daemon | Linux x86-64 with Nix |
| `scripts/e2e.sh OUT_DIR` | a log end to end: `recorded_rpc.py` serves the recorded mainnet block 26,384,851 over JSON-RPC; `fetch_log_proof` → `dry_run_attest` on the record read from the chain → `build_attest_tx` → `lez/harness`'s `module_e2e` decodes the bytes with the program's types and executes them in LEZ v0.4.0-rc1's state machine → `read_attestation`; two logs, two refusals (a tampered proof, another instance), and the bridge module's `Burn` and `Claim` decoded by the bridge program's types (`CALL=logosctl` runs it through a Logos Core daemon) | Linux with cargo (and `logosctl`) |
| `scripts/release-local.sh OUT_DIR` | `logos-modules-release-action`'s steps for `linux-amd64`, locally, nothing published (`catalog/README.md`) | Linux x86-64 with Nix |
| `scripts/catalog-install.sh RELEASE_DIR LOGOSCTL WORK_DIR` | a Logos Core installs both modules from that catalog, served over HTTPS on 127.0.0.1: `catalog add`, `search`, `install`, `module load`, `call` | Linux x86-64 with Nix |

### Left out

- **Logos Core's own network and keys.** The modules read Ethereum over plain `http://` JSON-RPC
  (a local node or `recorded_rpc.py`), not through `eth_rpc_module`, `verified_proxy_module` or
  `beacon_api_module`; LEZ state (a record, an event, nullifiers, counts) comes in the request. They
  build each program message and name the account it goes to, but do not sign or submit: no
  `keystore_module`, `tx_sender_module`, `lez_core` or LEZ wallet. In the end-to-end test the
  harness wraps the message in a LEZ transaction and executes it in LEZ's state machine in-process,
  not on a networked sequencer.
- **`ferrata_attest_module`:** no `advance` (the update's `Open`/`Miller`/`Adopt`/`Finish` messages);
  the instance's configuration is read from its state (`read_instance`), which comes in the request; the dry run still lacks the program-only checks (`WRONG_INSTANCE` is
  shown by the program in `e2e.sh`); no history hops, batches, conditions or relations, no accumulator
  leaves; no `KeyValidate` on the committee (§11.4 of the attestation note: it needs the BLS backend).
- **`ferrata_bridge_module`:** no private claim (`ClaimPrivately` runs in a private execution of the
  LEZ wallet), no `ImportRoot`, registry or fee-schedule reads, no release guest input or proof (the
  module gives the journal and the witness; proving is the CLI's `release --prove`); the vault's
  `Deposit` events and the program's burn records come in the request.
- **Catalog:** laid out, dry-run and installed from on 127.0.0.1 (`catalog/`); nothing published, one
  variant (`linux-amd64`), unsigned.
- **Linux x86-64 only** was tried.
