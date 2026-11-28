# The ferrata CLI's end-to-end tests on LEZ v0.4.0 (28 Nov 2026)

**Run:** on a Modal build box, 28 November 2026, from copies of the `lez-040-clients` working tree (03:46Z, between
d46656bc and cdc43666, for the first run; 03:54Z, at 88c8ed73, for the second), each prepared by `cli/v040/build.sh`'s
rules (LEZ v0.4.0 `a255b42c` selected with `lez/scripts/lez-select.sh v0.4.0`, the cli's LEZ paths and RISC Zero 3.1.0,
feature `lez-040`, lockfile `cli/lockfiles/Cargo.lock.v0.4.0`). The harness and guests are lez-040-core c36a60c4's (its
v0.4.0 port), the guests built with RISC Zero 3.1.0 from `scripts/ci/install-risc0.sh` (guest Rust 1.91.1); dev mode
(`RISC0_DEV_MODE=1`), as CI runs these tests. Paths of the box are written `<box>/` and `~/`.

| Run | Command (on the copy SRC) | Result |
|---|---|---|
| 1 | `RISC0_HOME=<lez home> RISC0_DEV_MODE=1 cli/v040/build.sh SRC test --test e2e --test both_halves --test roundtrip_b --test attest_faults -- --test-threads 4` | `attest_faults` 3 passed in 52.8 s (`ferrata attest` against a stand-in zone of LEZ v0.4.0's own block application: a failing Ethereum endpoint and interruptions send nothing twice, the journal guards, a stale checkpoint shown as code 53). The other three files passed in 0.01–0.05 s because they **skipped**: they run only with `FERRATA_E2E=1` and their own setups ([`attest-faults.txt`](attest-faults.txt)) |
| 2 | as `cli/tests/e2e.sh` does: the vault's Foundry build (`forge install` at its pins, `forge build`), the native archives (`scripts/fetch_native.py env`), then `FERRATA_E2E=1 RISC0_HOME=<lez home> RISC0_DEV_MODE=1 cli/v040/build.sh SRC test --test e2e -- --nocapture` | `deposit_scan_claim_burn_release_and_recovery` passed in 87 s, and `names_nothing_catches_every_form`: deposits to the vault on anvil (public and private destinations), scan, claims on the LEZ v0.4.0 zone (public, and private proven without naming the deposit), burns (public and from a private account), releases (one through a relayer), recovery from the seed, the burn journal's refusals, the chain-pairing refusal ([`e2e.txt`](e2e.txt): the test's own lines) |
| 3 | after review round 2's deposit check (`ferrata deposit` reads the network's LEZ first; lez-040-clients 2cb8b03c, the tree copied at 04:53Z), as run 2 | `deposit_scan_claim_burn_release_and_recovery` and `names_nothing_catches_every_form` passed (2 in 50.6 s); every deposit through the binary first read the version from a `getSequencerStatus` stand-in of the build's LEZ (`FERRATA_SEQUENCER`, tests/e2e.rs `lez_status_stand_in`) ([`e2e-3.txt`](e2e-3.txt): the test's output, box paths written `<box>/`) |

**Not in these runs:** `both_halves` and `roundtrip_b` with their setups (`e2e/both-halves/run.sh`, `e2e/ci/run.sh`:
anvil, the Ethereum-side checkpoint and release guests); real proofs (dev mode only); a live sequencer (the live run is
`cli/results/v040-standalone-1`); the committed rc1 build's e2e after the deposit check (CI's `roundtrip` job runs it;
the rc1 build's deposit check itself passed `tests/lez_version.rs` on the box).
