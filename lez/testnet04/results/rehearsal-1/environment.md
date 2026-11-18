# The rehearsal's box

Read on the box after the run (18 November 2026, 19:2x UTC), by `sha256sum`, `r0vm --version`, `/proc`.

- Machine: a Modal build box (CPU container under gVisor, kernel `4.19.0-gvisor`): `nproc` 2, 32 processors listed, 64 GiB.
- `sequencer_service`, LEZ v0.4.0-rc3 (`bafd07be`, the unmodified tag), built on the box by `lez/live/run.sh build-sequencer`:
  sha256 `be2ac354f4268d7c021519735667d5d5b723e5901a65e4dcd47cd686dc42ca34`, rustc 1.98.1 (48a229cea 2026-09-01).
- Logos Blockchain node: release `0.3.5-rc.1` (commit `f6d4267fe7d1ff8ab4cb30e9a18154bc66e10c30`), asset
  `logos-blockchain-node-linux-x86_64-0.3.5-rc.1.tar.gz`, taken by LEZ's resolver: sha256
  `2f30172903a36175606f466fa6a9d5503a26cb8f95eb9b9444677ee06520c6e2`. Its log: 53,127 lines, 51,559 of them
  gossipsub "failed to broadcast" errors (one validator, no peers).
- `ferrata-day1`, as used from 17:53 UTC (the tool reading chain state as of the last final block, commit `36cd3f5a`):
  sha256 `29817811fd55aceab18c218a5d9f7198ad8dce85cd7c6f9215494f1b492e9a59`; the build used before it (17:48-17:53)
  differed only in `inspect`'s reads (commit `bb7b7696`). `ferrata-updater` (the daemon, commit `bb7b7696`): sha256
  `597da31c9f2a095398f9abc13a35c60cbd9a1b978e3533b7704cf6a275309981`. Both built against rc3 by
  `cargo build --release --locked` in `lez/updater/sequencer` of a copy of the tree with `lez-select.sh v0.4.0-rc3`.
- The programs deployed: `ferrata_lez.bin` sha256 `ddfde69bea89a5b882f55266fcb4b7c887df7debc1c9d4972d8ebea5a72eaad9`,
  `ferrata_bridge.bin` sha256 `11d832482ff0a26c8f283edda1a35b262274c066734895c62f696394dcab6070`: built on the box from the
  tree as committed (LEZ v0.4.0-rc1, `cargo build -p methods --locked` with `scripts/guest-rustc`), equal to
  `docs/image-ids.md`; `r0vm` (risc0-r0vm 3.0.5) gave `816d1fef…` and `ccde6171…`.
