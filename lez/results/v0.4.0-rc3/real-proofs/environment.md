# Where the real proofs ran (16 November 2026)

- Machine: one Modal build box (a CPU container: 16 physical cores, 32 threads, 64 GiB at most, under gVisor; `nproc`
  reports 2, the CPU request, so every prover ran with `RAYON_NUM_THREADS=32`); no GPU. The same box built everything.
- LEZ: `v0.4.0-rc3` = `bafd07be0701398f9bc435f73ab05e9a7936c301`, cloned on the box, commit and clean tree checked;
  its `sequencer_service` built from it (`lez/live/run.sh build`), its node resolved by its own resolver: Logos
  Blockchain `0.3.5-rc.1` (release, `f6d4267fe7d1ff8ab4cb30e9a18154bc66e10c30`).
- The sequencer's environment: no `RISC0_DEV_MODE` (each session's `config/sequencer-process.txt`); the node with
  `POL_PROOF_DEV_MODE=true` (leader proofs in dev mode), as LEZ's container entrypoint runs it.
- Provers: LEZ v0.4.0-rc3's wallet library linked into `live-driver` and `crown-driver` (this tree's `lez/live/driver`,
  `lez/crown/driver`, with `lez/scripts/lez-select.sh v0.4.0-rc3`), r0vm 3.0.5, risc0-zkvm 3.0.5; nice 10, below the
  node and the sequencer.
- Programs: the final rc1 builds of `docs/image-ids.md`, built on the box from this tree on v0.4.0-rc1 and checked by
  sha256 and image id (attestation program `0x816d1fef…`, bridge `0xccde6171…`, reference consumer `0x8464603b…`,
  `scripted` `0x8cb0e3f5…`).
- Toolchains: rustc 1.98.1 (host), RISC Zero Rust 1.88.0 (guests), Foundry 1.3.6 (the local anvil and the vault).
- Each session's own `environment.md` (in `update/`, `attest-claim-burn/`, `attest-claim-burn-try1/`) has its times,
  the binaries' sha256 and the node's answers at the end.
