# Stream macos-fix: `macos.yml` made to pass on `macos-15`, proposed rows

For the integrator of `docs/requirements-status.md`, `docs/traceability.md`, `docs/EVIDENCE.md` and
`scripts/ci/row-tests.toml`. Branch `macos-fix` from `3d984678`. Box runs on the Modal build box (16 cores), 23 November
2026; laptop: the `links` job's checks. Nothing was pushed or sent. No program, guest or image id changed: the one
line added to code a guest compiles is a lint attribute, inserted so that every later line of `steps.rs` is
byte-identical, and `lez/`'s guests built on this tree give the attestation program `0x816d1fef…` with the bytes
`docs/image-ids.md` records. No dependency changed. The record: [`modules/results/macos-2026-11-23/`](../../modules/results/macos-2026-11-23/README.md).

## What changed

- **user-side** (failed at Clippy: unpinned stable, Rust 1.100.0, `clippy::unnecessary_map_or` at
  `crates/ferrata-core/src/steps.rs:390`): the job takes Rust 1.93.0 (`dtolnay/rust-toolchain` at `ci.yml`'s commit
  for `1.93.0`); `Job::is_verified` allows the lint with a reason (the attestation program is built from it), line
  for line. The workspace is Clippy-clean under 1.100.0 too.
- **every third-party action** in `macos.yml` pinned by commit: `checkout`, `cache` and `rust-toolchain` at `ci.yml`'s
  commits, `cachix/install-nix-action` (`v31`) and `actions/upload-artifact` (`v4`) at the commits their tags had on 23
  November 2026, the ones run 45110242235 used (`scripts/ci/pin-actions.py`, `git ls-remote`).
- **modules** (failed in `ferrata_relayer_module`'s `risc0-sys 1.5.1`: `risc0-build-kernel 2.0.1` creates RISC Zero's
  kernel cache under `$HOME`, read-only in the Nix build on macOS, then would run `xcrun metal`): the relayer's flake
  sets `RISC0_SKIP_BUILD_KERNELS=1`, `risc0-build-kernel`'s own switch (an empty kernel library, read only by
  `risc0-zkp`'s Metal prover, which the relayer never starts: it proves through `r0vm`); the relayer's `CMakeLists.txt`
  links `-framework Metal` and `objc` on macOS, which `risc0-zkp`'s verifier objects need on `aarch64-apple-darwin`
  (found by cross-building them; the link would otherwise have been the next failure).

## Rows

| ID | Proposed | Clause by clause | Evidence |
|---|---|---|---|
| RFP-022 S.2 | **in part (unchanged)** | *The modules work on both Linux and macOS*: Linux as before (EVIDENCE 59–62, 94–98). macOS: `macos.yml`'s first run (45110242235, `907736e8`) built and `lgx verify`ed four modules (`ferrata_attest_module`, `ferrata_bridge_module`, `ferrata_attestor_module`, `ferrata_monitor_module`) and stopped in the relayer's `risc0-sys`; the user-side job's tests passed and its Clippy failed. Both causes fixed on `macos-fix` and checked on Linux: the relayer's macOS crate build reproduced by cross-compiling `risc0-zkp` for `aarch64-apple-darwin` (fails without the switch, passes with it), its macOS link requirement read off the objects, and its Linux build byte-identical with and without the changes (static library and `.lgx`); Clippy 1.100.0 and 1.93.0, the tests (168 + 60) and fmt pass. **Not yet shown:** a green macOS run (the push to `main`); then still in part: no module is loaded under Logos Core on macOS by the workflow, and `ferrata_updater_module` is built for x86-64 Linux only (it vendors linux-x86_64 circuits and rapidsnark). | `.github/workflows/macos.yml`; `modules/results/macos-2026-11-23/`; EVIDENCE 165 (corrected below) and the row proposed below |

Proposed reason for `docs/requirements-status.md` (S.2), until the macOS run at `main` is green: *User-side proofs built
and proven on macOS; a macOS workflow builds the user-side crates and five modules: its first run, on `macos-15` at
`907736e8`, built four modules and failed on the relayer's Metal kernels and on Clippy (row 165); both fixed and checked
on Linux (row NEW: the relayer's macOS crate build cross-compiled, its link requirement added); not yet green on macOS;
the modules and the catalog run on Linux x86-64 only.* After a green run: *...a macOS workflow builds and verifies five
modules' `.lgx` on `macos-15` (run N) and runs the user-side crates' tests; none is loaded under Logos Core on macOS,
and the updater's module is x86-64 Linux only.*

## EVIDENCE

Row 165, its cause corrected (the build did not write into the vendored source): *... the modules job in the Nix build of
`risc0-sys 1.5.1`, whose build script (`risc0-build-kernel 2.0.1`, `src/lib.rs:303`) creates RISC Zero's Metal kernel
cache under `$HOME`, on macOS's read-only system volume in a Nix build (`Read-only file system`), after four modules had
built and passed `lgx verify` ...*

Proposed new row: **The macOS workflow's two failures reproduced and fixed without a Mac (RFP-022 S.2, in part)** |
`macos.yml` takes Rust 1.93.0 and pins every action by commit; `Job::is_verified` allows `unnecessary_map_or` line for
line (`steps.rs` from line 389 on byte-identical: no guest changes). Box, 23 November 2026: Clippy 1.100.0 on
`3d984678`'s `steps.rs` fails with that one lint, as on macOS, and passes on this tree, as under 1.93.0; tests 26
suites 168 passed, 1 ignored; bridge core 60 passed; fmt, `+1.85.0 check`, the `riscv32im` build pass. The relayer's
flake sets `RISC0_SKIP_BUILD_KERNELS=1`: `risc0-zkp 3.0.4 [prove,std]` cross-compiled for `aarch64-apple-darwin` (Rust
1.98.0, the relayer's lockfile; `nvtx`'s C stubbed, no macOS SDK) fails in `risc0-sys` without it and builds with it
(an empty kernel library); on Linux the five `risc0_*` rlibs are byte-identical with and without it. The cross-built
objects need libobjc (the verifier's codegen unit holds the Metal prover; every unit reaches it): the relayer's
`CMakeLists.txt` links Metal and `objc` on macOS. On Linux the relayer module, exported and built as the workflow
builds it (`export.sh`, `nix build .#lgx-portable`, `lgx verify`: valid), gives a byte-identical Rust static library
(`97d82aa4…`) and `.lgx` (`1ea46616…`, 17,734,602 bytes) with and without these changes; the darwin crate derivation
carries the variable. The attestation program built on this tree is `0x816d1fef…` with the recorded bytes. Not yet
run on macOS. | `modules/results/macos-2026-11-23/`; `.github/workflows/macos.yml`;
`modules/ferrata_relayer_module/{flake.nix,CMakeLists.txt}`; commits `e7264e02`, `9163d4a3`, `13b551fe` and the record's

## Other files that now read stale (not edited here)

- `scripts/ci/row-tests.toml`, RFP-022 S.2's note: "never run (no push)" — it ran (45110242235) and failed; suggest
  "first run at `907736e8` failed (four modules built; the relayer's `risc0-sys` and Clippy), fixed on `macos-fix`, not
  yet green".
- `scripts/ci/README.md` (check-actions-pinned.py): the counts of 21 November. Now: 0 unpinned, 6 TODO-marked (all in
  `sequencer.yml`), so it passes and can join the `links` job; `pin-actions.py --write .github/workflows/sequencer.yml`
  would pin the rest.
- `ci.yml`'s `test` job keeps `-A clippy::unnecessary_map_or`, now redundant (the attribute is in the code); harmless.
