# R3: the suites the job API touches, run again on the module and the view of commit `48a3b13d`

The module's and the view's code did not change after `48a3b13d` (later commits touched the Basecamp driver and the
docs only). Every suite below ran on the Modal build box on 17 November 2026 on that code.

| Suite | Before (16 Nov) | Now | Records |
|---|---|---|---|
| `ferrata_bridge_module`'s own checks (`modules/scripts/check.sh`, `MODULES=ferrata_bridge_module PORTABLE=0 E2E=0`): native tests, native calls, the dev `.lgx`, loaded in a headless Logos Core | 11 native tests (the crate's tests before the job API) | **18 of 18 native tests**, 7 of them the job API's (`jobs::tests`: an unknown id, a job answered at once that settles in its thread, two starts of one step, a failed job's error and a panic, `wait: true`; `session::tests`: the jobs through `wallet`, the calls refused while a job runs); 0 step failures, 0 check failures, 154 s | [`module-tests/`](module-tests/) |
| The round trip inside Basecamp in dev mode (`run.sh basecamp`, `SEQNET_PROOFS=dev`) | 13 of 13 (`seqnet-basecamp-2`) | **13 of 13 steps, 58 checks**; the claim and burn as jobs (6.9 s and 2.8 s, settled); 33 calls of the view, the longest 3,141 ms, none cut | [`basecamp-dev/`](basecamp-dev/) |
| Basecamp's `nix run .#ui-tests` (`scripts/basecamp-ui-tests.sh`, Basecamp `b642a9a`) | 10 of 10 | **10 of 10** | [`ui-tests/basecamp-ui-tests.txt`](ui-tests/basecamp-ui-tests.txt) |
| The devnet Qt Quick Test (`scripts/box-test.sh`: `cli/examples/miniapp_devnet` and qmltestrunner, Ubuntu's Qt 6.4) | 17 of 17 | **17 of 17** (the devnet backend answers claim and burn itself, no job: the view's other path) | [`box-test/`](box-test/) |
