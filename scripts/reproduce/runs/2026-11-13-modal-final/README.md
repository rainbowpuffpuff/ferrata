# The one-command check on the final tree, 13 Nov 2026

`MODAL_PROFILE=… modal run --detach scripts/reproduce/modal_app.py` from a checkout of commit `0f31e30` (branch
`final-merge`: the build's last merge, `miniapp-sequencer` into `integration8`, with its checks and documents; the
commits after it record this run and change documents only): **5 of 5 PASS in 31:27** on 16 CPUs, no committed file
changed: both live runs' records 13 passed each, the vault 464 passed and 1 skipped (the zone runs' suites
`ZoneBridge` and `ZoneBridgeRotation` added since the run before; 71 tests with real proofs in 17 suites), the core
163, 14 of 14 guest ids and 6 of 6 crown programs, the LEZ harness 56. Modal preempted the first container 12 minutes
in, during the LEZ harness (its first four checks had passed, with the same results), and ran the function again from
the start: 13:44–13:56 UTC, then 13:58–14:29 UTC, 2,745 s from the call to the logs back.
[`summary.txt`](summary.txt) is its verdict; [`environment.txt`](environment.txt) and [`packages.txt`](packages.txt)
its toolchains and packages (the same as the run before but the commit and the date); `guest-ids/` the rebuilt ids
(the same as the run before). The run before, at the published tree `a5fd69f`:
[`../2026-11-13-modal/`](../2026-11-13-modal/README.md); the same check under Docker on an x86-64 machine:
[`../2026-11-12-docker-aws/`](../2026-11-12-docker-aws/README.md).
