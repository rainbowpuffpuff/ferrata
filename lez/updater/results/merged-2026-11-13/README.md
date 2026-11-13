# ferrata-updater after the merge of `updater-live` (13 November 2026)

The updater's tests, the A3 gate's eight runs, the harness binary as a process, the sequencer build and the
re-check of the live Sepolia records, on the tree that merges `updater-live` (the sequencer backend, `headers`
sources, `layout`, `finish_own_jobs`) into the daemon refactored for the Logos module, with the update planning in
`lez/advance`. Modal build box (workspace ws-4, gVisor, 2 CPUs reported, up to 16 cores, 64 GiB), commit
`6dcf6af`, 13 Nov 2026 03:57–04:52 UTC, `RISC0_DEV_MODE=1` (no proof is made by any of it). The script:
`spike/tmp/merge-updater/box/run1.sh` (beside the repository); the modules' run of the same script is in
[`../../../../modules/results/merged-updater-2026-11-13/`](../../../../modules/results/merged-updater-2026-11-13/).

| File | What it holds |
|---|---|
| `run-steps.txt` | every step, its result and seconds; all `ok` but `sequencer-smoke`, whose exit 1 is its last line (a `git diff \| grep` that found nothing on the box), not a check: its output is `sequencer-build.txt` |
| `run-env.txt` | the commit, the box, the toolchains, the release guest's id (`0xb64900bc…29e9`, as `docs/image-ids.md`) |
| `tests.txt` | `cargo test --locked -p ferrata-advance` (no tests of its own) and `-p ferrata-updater --features harness`: 5 config tests and 9 recorded tests, **14 of 14 pass** (the default gate run's figures inside) |
| `gate-default.txt`, `gate-runs.txt` | the A3 gate: the default run and the seven others (`GATE_SEED` 1–3, `GATE_SALT` a–c, `GATE_ADVERSARY=0`): **all eight figures equal those of `updater-live`** (`../gate*.txt`, which the runs rewrote byte for byte) |
| `native-tests.txt` | the test result lines of `lez/` and `cli/` (ferrata-wallet 3, the CLI's lib 10, every test compiled, `miniapp_devnet` built) and the ferrata tool's e2e (1 passed) |
| `demo.txt` | `updater/dev/demo.sh` (the harness binary as a process, row 81): 36 rounds in 45 s, mainnet to 15,631,808, 16 transactions applied, `/health` 200 `ok`, an unknown path 404, SIGTERM: exit 0 |
| `sequencer-build.txt` | the sequencer build of the merged daemon (`lez/updater/sequencer`, `cargo build --release`): SHA-256 `6d5347cc…f408`, 25,664,240 bytes; without a sequencer: usage (exit 2), `ledger = "harness"` refused, the example configuration's sequencer at 127.0.0.1:3040 not there ("Connection refused", exit 1), a sequencer not on this machine refused; the lock's one addition (`ferrata-advance`) |
| `check-live.txt` | `live-sepolia/check.py` on the committed records with `--beacon https://ethereum-sepolia-beacon-api.publicnode.com`: **ALL PASS** for `zone-2` (17 roots canonical), `zone-1` (39) and `zone-1/u2-lost` (32, 14 of them U1's, also in `zone-1`): the 74 updates of the live run |

Not run here: the merged daemon's sequencer build against a running sequencer (the live run of 12–13 November used
the binary of `updater-live`'s tree, SHA-256 `912208fc…525a`, [`../live-sepolia/`](../live-sepolia/README.md)).

Re-run: `spike/modal-build/mrun --put ../tmp/merge-updater/box -- 'bash ../tmp/merge-updater/box/run1.sh'` from
the worktree (the crown snapshot on the workspace's Volume seeds the sequencer build and `vault/lib`).
