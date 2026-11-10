# Logos modules

The proposal (§B7) puts everything off chain in Logos modules. This directory holds the first one, a
prototype of `ferrata_attest_module`: three methods, built the way Logos modules are built, loaded by a
headless Logos Core and called there. `RESULTS.md` records what ran.

```sh
modules/scripts/check.sh      # on a Linux x86-64 box with Nix: stage, test, build, package, load, call
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

## This module

`modules/ferrata_attest_module/`:

- `metadata.json`, `flake.nix`, `CMakeLists.txt`: the builder's Rust template, with the builder pinned
  to the commit above.
- `rust-lib/`: the module crate (the trait and its implementation, a static library) and `calls/`,
  the methods' logic as a plain crate that runs and is tested without the generated scaffold.
- `ferrata-core` is copied into `rust-lib/ferrata-core/` before a build (`scripts/stage.sh`), because
  the builder copies only `rust-lib/`. The copy is ignored by git; `rust-lib/Cargo.toml` carries the
  workspace fields the copy inherits. It is built without a BLS backend: none of the three methods
  verifies a signature.

### Interface

Each method takes one JSON string and returns one JSON string, so a call needs no type beyond
`String` and the request can come from a file (`logosctl call ferrata_attest_module dry_run_attest @request.json`).

| Method | Takes | Returns |
|---|---|---|
| `version()` | — | the module's version, the `ferrata-core` version and the source revision it was staged from, and which BLS backend is compiled in (none) |
| `dry_run_attest(request)` | the consumer's `chain_id`; a header `record` with the fields the program records for a finalized header (instance chain id, anchor slot, block number, timestamp, block hash, parent hash, state root, receipts root); and either `storage` (an `eth_getProof` result and the slot) or `log` (transaction index, log index within its receipt, receipt proof) | `verdict` (`attested` or `refused`), `code` (the program's code: 0, or the core's refusal code, which the program returns unchanged, `lez/wire/src/lib.rs` `refusal`), the error's name, and for an attested fact its fields and statement identifiers (`id`, `id_at_block`) |
| `verify_checkpoint(request)` | a `config` (the program's `Config` fields), a `checkpoint` root, optionally the `expected_instance_id`, the `bootstrap` (SSZ, hex) and `observed` roots for the checkpoint as other sources report them | the recomputed `instance_id`; whether it equals the expected one; the program's `Init` verdict on the bootstrap (`StoreCore::from_bootstrap`: layout, header, header root against the checkpoint, committee branch) with its code; each observed root against the checkpoint; whether the chain id and `genesis_validators_root` are a known pair; an overall `verdict` |

The record is built the program's way (`witness::HeaderRecord`, then `attest::HeaderRecord::from_recorded`,
as `evaluate` does), and the proof is checked by `attest_storage` or `attest_log`, so the verdict and
code are the ones the program would return for that proof against that record.

`verify_checkpoint` recomputes the instance id as the program defines it (`lez/wire/src/lib.rs`
`instance_id`: SHA-256 over the domain, `instance-id`, the Borsh encoding of `Config` and the
checkpoint). The program's crate links LEZ (`lee_core`, which pulls in the RISC Zero client), so the
module encodes `Config` itself; `check.sh` compares the result with an instance id the program produced.

### Left out of this prototype

- **No network.** Proofs, records, bootstraps and providers' roots come in the call. The full module reads
  them through `eth_rpc_module`, `beacon_api_module` and `verified_proxy_module`, and reads records
  from the program through `lez_core`.
- **No transactions.** Producing and submitting an attestation, advancing an instance and reading its
  configuration from LEZ (the rest of §B7's list) are not here, nor is the updater daemon.
- **No program-only checks.** The dry run checks a proof against a record; the program first checks
  the request's instance (`WRONG_INSTANCE`), which needs an instance. Requests with history hops,
  several facts, conditions and relations (`attest_batch`) and accumulator leaves are not accepted yet.
- **No `KeyValidate`** on the committee (§11.4 of the attestation note): it needs the BLS backend.
- **No catalog release**: the package is built as a catalog would build it, not published.
- **Linux x86-64 only** was tried.
