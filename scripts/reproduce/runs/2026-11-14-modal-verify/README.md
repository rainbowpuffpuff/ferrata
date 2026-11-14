# The one-command check on the tree to publish, 14 Nov 2026

`MODAL_PROFILE=… modal run --detach scripts/reproduce/modal_app.py` from a checkout of commit `4a58137` (branch
`final-verify`: `integration8`'s `6b7ea86`, with the merges of `lez-core-v04` and `lez-rc3` and EVIDENCE rows 111–115,
plus one rustfmt line in `cli/src/bin/seqnet_chain.rs`; the commits after it record this run and the build box's
checks: documents and records only): **5 of 5 PASS in 36:10** on 16 CPUs, no committed file changed: both live runs' records 13
passed each (the second public endpoint answered for both; the first failed), the vault 464 passed and 1 skipped, the
core 163, 14 of 14 guest ids and 6 of 6 crown programs, the LEZ harness in dev mode 56 (LEZ v0.4.0-rc1, the default).
14 Nov 2026 11:36–12:12 UTC, 2,185 s from the call to the logs back; no preemption.
[`summary.txt`](summary.txt) is its verdict; [`environment.txt`](environment.txt) and [`packages.txt`](packages.txt)
its toolchains and packages (the same as the run before but the commit and the date); `guest-ids/` the rebuilt ids
(the same as the run before). The run before, at `0f31e30` (13 Nov, before the merges of `lez-core-v04` and
`lez-rc3`): [`../2026-11-13-modal-final/`](../2026-11-13-modal-final/README.md), 5 of 5 in 31:27 with the same counts.
The same tree's other checks on the build box (the modules, the tool, the relayer, the daemons' crates):
[`modules/results/final-verify-2026-11-14/`](../../../../modules/results/final-verify-2026-11-14/README.md).
