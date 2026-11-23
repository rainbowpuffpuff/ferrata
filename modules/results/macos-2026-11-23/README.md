# The first macOS run's failures, reproduced and fixed without a Mac (RFP-022 Supportability #2)

23 November 2026. `.github/workflows/macos.yml` ran on GitHub's `macos-15` for the first time at `907736e8` (run
45110242235) and failed in both jobs. No macOS machine was available for the fix, so each failure was reproduced on
the Linux build box (16 cores; measured there unless marked otherwise) and the fix checked as far as Linux allows. What
stays unverified until the next macOS run is listed at the end.

## 1. user-side: Clippy of an unpinned stable toolchain

The job installed `dtolnay/rust-toolchain@stable`, Rust 1.100.0 that day, whose Clippy flags
`clippy::unnecessary_map_or` in `Job::is_verified` (`crates/ferrata-core/src/steps.rs:390`) under `-D warnings`; every
test step before it passed. `ci.yml`'s `test` job takes the same stable and passes only with
`-A clippy::unnecessary_map_or`: `is_verified` is compiled into the attestation program (`lez/guests/program`), so
rewriting it as `is_none_or` could move that program's image id.

Fix: the job takes Rust 1.93.0 (`dtolnay/rust-toolchain` at the commit `ci.yml` uses for its `1.93.0` branch), and
`is_verified` carries `#[allow(clippy::unnecessary_map_or, reason = "...")]`, inserted where the blank line between
`const JOB_VERSION` and `impl Job` was: every line from 389 to the end of `steps.rs` is byte-identical to `3d984678`'s
(same sha256), so no panic location compiled into a guest moves, and a lint attribute does not change code.

| Check (box, `cargo ... --locked`) | Result |
|---|---|
| Clippy, Rust 1.100.0 (`rustc 1.100.0 (0ef1bc386 2026-11-09)`), `steps.rs` as at `3d984678` | fails: the one error `unnecessary_map_or` at `steps.rs:390:42`, as on macOS |
| Clippy, Rust 1.100.0, this tree: `cargo clippy --workspace --all-targets -- -D warnings` | passes |
| the same with Rust 1.93.0 (the pinned toolchain) | passes |
| `cargo fmt --all -- --check`, 1.93.0 and 1.100.0 | passes |
| `cargo +1.85.0 check --workspace` (the declared `rust-version`; lint reasons are stable since 1.81) | passes |
| `cargo build -p ferrata-core --no-default-features --features zkcrypto --target riscv32im-unknown-none-elf --release`, 1.93.0 | passes |
| `cargo test --workspace`, 1.93.0, with `scripts/fetch-vectors.sh` | 26 suites: 168 passed, 0 failed, 1 ignored |
| `cd bridge && cargo test -p ferrata-bridge-core`, 1.93.0; Clippy 1.100.0 on it | 10 suites: 60 passed; Clippy passes |

## 2. modules: `risc0-sys` in the relayer's Nix build

Four modules built and passed `lgx verify` on macOS; `ferrata_relayer_module` stopped in `risc0-sys 1.5.1`'s build
script: `risc0-build-kernel 2.0.1` (`src/lib.rs:303`) unwrapped `fs::create_dir_all` on RISC Zero's kernel cache,
`directories::ProjectDirs::from("com.risczero", "RISC Zero", "risc0").cache_dir()`, which is under `$HOME`; Nix's
`$HOME` is `/homeless-shelter`, on macOS's read-only system volume (EROFS). Past that it would have run
`xcrun --sdk macosx metal`, which a Nix build does not have either.

Why only the relayer: `cli/Cargo.toml` enables `risc0-groth16`'s `prove` feature for `prove::to_json` (the Groth16
step's `input.json`), which enables `risc0-zkp/prove`, which depends on `risc0-sys`; `risc0-sys`'s build script
compiles RISC Zero's Metal kernels whenever the target OS is macOS or iOS (no feature turns that off), and
`risc0-zkp`'s embeds them (`ZKP_METAL_PATH`) for its Metal prover. The other four modules do not enable `prove`. The
relayer never proves in-process: it links `risc0-zkvm` with `client` only, proves through `r0vm` over IPC, and runs
the Groth16 step as an external command (`wrap_command`).

Fix: `modules/ferrata_relayer_module/flake.nix` passes `RISC0_SKIP_BUILD_KERNELS=1` to the crate build
(`mkLogosModule`'s `rustEnv`). That is `risc0-build-kernel`'s own switch: it returns before the cache and writes an
empty kernel library into `OUT_DIR`, which only the Metal prover reads. (LEZ's own flake instead builds the real
kernels on darwin, with a writable `$HOME`, an `xcrun` wrapper to Xcode's Metal toolchain and the sandbox off;
the relayer has no use for them.)

Checked by cross-compiling `risc0-zkp 3.0.4` with `prove` and `std` (the relayer's features) for
`aarch64-apple-darwin` with Rust 1.98.0 (the relayer's `nix.rust.toolchain`), lockfile copied from the relayer's
(`risc0-sys 1.5.1`, `risc0-build-kernel 2.0.1`, `metal 0.29.0`, `objc 0.2.7`): [`tools/zkp-darwin/`](tools/zkp-darwin/).
There is no macOS SDK on the box, so `nvtx`'s one C file (`risc0-core`'s `perf`) is compiled by a stand-in
([`tools/cc-darwin-stub.sh`](tools/cc-darwin-stub.sh): clang for the target, every C source replaced by an empty one);
every Rust crate is the real one.

| `cargo +1.98.0 build --release --target aarch64-apple-darwin` | Result |
|---|---|
| without the variable | `risc0-sys`'s build script fails: it creates the cache under `$HOME` (writable on the box) and runs `xcrun --sdk macosx metal`, which is not there |
| with `RISC0_SKIP_BUILD_KERNELS=1` | passes: `risc0-sys` writes `out/skip-metal_kernels_zkp.metallib` (0 bytes) and prints `cargo:metal_kernels_zkp=` it; `risc0-zkp` gets `ZKP_METAL_PATH` to it; `risc0-zkp`, `metal`, `objc` compile |
| Linux (`x86_64-unknown-linux-gnu`), one target directory, built without then (after `cargo clean`) with the variable | the five `risc0_*` rlibs byte-identical; `risc0-sys`'s build-script output the same single line (`cargo:cxx_root=...`): on Linux nothing reads the variable |

## 3. The relayer's macOS link, which would have failed next

`LogosModule.cmake` links a Rust module's static library with `-framework CoreFoundation -framework Security` only.
The cross-built `risc0-zkp` has six codegen-unit objects ([`risc0-zkp-objects-aarch64-darwin.txt`](risc0-zkp-objects-aarch64-darwin.txt),
[`tools/members.py`](tools/members.py) over `llvm-nm`): the one holding `verify::Verifier` (which the relayer uses, through
`risc0-zkvm`, to check receipts) also holds `hal::metal` code and references `objc_msgSend` and `sel_registerName`, and
each of the other five references it, directly or through another. ld64 loads an archive member whole and needs every
symbol it references, so the plugin's link would stop on the Objective-C runtime. Every other external C symbol of
the graph ([`undef-aarch64-darwin.txt`](undef-aarch64-darwin.txt), [`tools/undef.py`](tools/undef.py)) is
libSystem's or CoreFoundation's, except the `metal` crate's `MTLCreateSystemDefaultDevice` and `MTLCopyAllDevices`
(Metal.framework); `nvtx`'s two `ffi_range_*` are its own C, stubbed here.

Fix: `modules/ferrata_relayer_module/CMakeLists.txt` links `-framework Metal` and `objc` when `APPLE`; Linux reads
none of it.

## 4. Linux unaffected

The relayer module exported and built as `macos.yml` (and `modules/scripts/release-local.sh`) builds it, on
`x86_64-linux`: `modules/scripts/export.sh ferrata_relayer_module DIR` (771 files, as on macOS), `nix build
.#lgx-portable` (Nix 2.28.3; `logos-module-builder` `5f74cdb3`), `lgx verify` with `logos-package` `4bdd9041`. Twice:
with this branch's module sources (commit `9163d4a3`; `13b551fe` changes only comments in `CMakeLists.txt`) and with
`3d984678`'s `flake.nix` and `CMakeLists.txt` in the same export.

| | base (`3d984678`'s two files) | this branch |
|---|---|---|
| `nix build .#lgx-portable` | passes (193 s, after the first) | passes (614 s, the Logos dependencies included) |
| `lgx verify` | Package structure is valid | Package structure is valid |
| the Rust static library, `libferrata_relayer_module.a` | sha256 `97d82aa4…30df`, 107,032,178 bytes | the same |
| the `.lgx` | sha256 `1ea46616…b1f7`, 17,734,602 bytes | the same |

The derivations, evaluated on the box for both systems (`nix derivation show -r .#packages.<system>.lgx-portable`):
the crate build (`ferrata_relayer_module-0.1.0`) has `RISC0_SKIP_BUILD_KERNELS = "1"` for `aarch64-darwin` and
`x86_64-linux` on this branch and none at base; its other differing inputs are the source and the vendored crates'
paths. The Linux outputs are byte-identical, so the variable and the `APPLE` link change nothing there.

The guests are unchanged too: `lez/`'s guests built on this tree as `scripts/reproduce-image-ids.sh build` builds
them (RISC Zero 3.0.5, guest Rust 1.91.1, `scripts/guest-rustc`; 503 s) give the attestation program `ferrata_lez`
`0x816d1fef…03df` (binary sha256 `ddfde69b…aad9`, 627,064 bytes) and `ferrata_lez_keccak_soft` `0xefe9c58c…cb3e`
(`727c5cda…1995`), the ids and bytes `docs/image-ids.md` records.

## Not verified until the next macOS run

- The relayer's Nix build on macOS past `risc0-sys`: `nvtx`'s C file under Nix's darwin stdenv (it includes
  `dlfcn.h` and `pthread.h`; stubbed here), and the plugin's link with Metal and libobjc from the SDK Nix provides.
- `lgx verify` of the relayer's macOS `.lgx`, and the five `.lgx` uploaded.
- The user-side job under Rust 1.93.0 on macOS (its commands pass under 1.93.0 on Linux and passed under 1.100.0 on
  macOS until Clippy).
- None of the six modules is loaded under Logos Core on macOS by this workflow, and `ferrata_updater_module` is not
  built for macOS at all (it vendors linux-x86_64 circuits and rapidsnark).
