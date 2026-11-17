# R3: the suites the job API touches, run again on the final module and view (`3b7ffe1d`)

Every suite below ran on the Modal build box on 17 November 2026. **On the final code** (`3b7ffe1d`: the module as in
`48a3b13d`, its `.lgx` byte for byte the same; the view with the reopening fix), in one box job, 16:30–17:09 UTC,
after the real-proof run in [`../final/`](../final/README.md):

| Suite | Before the job API (16 Nov) | On `3b7ffe1d` | Records |
|---|---|---|---|
| `ferrata_bridge_module`'s own checks (`modules/scripts/check.sh`, `MODULES=ferrata_bridge_module PORTABLE=0 E2E=0`): native tests, native calls, the dev `.lgx`, loaded in a headless Logos Core | 11 native tests (the crate's tests before the job API) | **18 of 18 native tests**, 7 of them the job API's (`jobs::tests`: an unknown id, a job answered at once that settles in its thread, two starts of one step, a failed job's error and a panic, `wait: true`; `session::tests`: the jobs through `wallet`, the calls refused while a job runs); 0 step failures, 0 check failures | [`final/module-tests/`](final/module-tests/) |
| The round trip inside Basecamp in dev mode (`run.sh basecamp`, `SEQNET_PROOFS=dev`, `seqnet-basecamp-final-dev`) | 13 of 13 (`seqnet-basecamp-2`) | **13 of 13 steps, 58 checks**; the claim and burn as jobs (settled after 6.7 s and 2.9 s); 33 calls of the view, the longest 3,179 ms, none cut | [`final/basecamp-dev/`](final/basecamp-dev/) |
| The view in a headless Logos Core on a LEZ devnet (`run.sh miniapp`, the `qmltest-logos` runner, dev mode) | 9 of 9 | **10 of 10**, with the new `test_08_jobs`: a view created afresh finds the claim's and the burn's finished jobs and applies them (the release held from the burn's end for its own delay); an unknown job id reaches the view as "Unknown job"; a failed job's error ("not a deposit of this wallet") reaches the view's banner ([picture](final/seqnet-miniapp/shots/19-a-failed-job-reaches-the-view.png)); 29 calls, the longest 84 ms | [`final/seqnet-miniapp/`](final/seqnet-miniapp/) |
| Basecamp's `nix run .#ui-tests` (`scripts/basecamp-ui-tests.sh`, Basecamp `b642a9a`) | 10 of 10 | **10 of 10** | [`final/ui-tests/`](final/ui-tests/) |
| The devnet Qt Quick Test (`scripts/box-test.sh`: `cli/examples/miniapp_devnet` and qmltestrunner, Ubuntu's Qt 6.4) | 17 of 17 | **17 of 17** (the devnet backend answers claim and burn itself, no job: the view's other path) | [`final/box-test/`](final/box-test/) |

The job API's four cases asked for: an unknown id (`jobs::tests::unknown_id_and_missing_id`, `test_08_jobs`); a failed
job's error reaching the view (`jobs::tests::a_failed_job_keeps_its_error_and_a_panic_fails_it`, `test_08_jobs`, its
picture); two starts of one step (`jobs::tests::two_starts_of_one_step_give_one_job_and_another_step_is_refused`, and
inside Basecamp during each real proof: a second start answered the running job, [`../final/`](../final/README.md));
a job surviving a reopened view (`test_08_jobs` with a view created afresh; inside Basecamp with the view unloaded and
loaded again while the burn was proven, [`../final/`](../final/README.md)).

**Also**: the module beside `lez_core` in a headless Logos Core through logosctl (`run.sh lezcore`, dev mode,
`seqnet-lezcore-jobs-dev`, the module's code of `48a3b13d` = `3b7ffe1d`): the claim and burn with `"wait": true`, the
job run in the call itself (job 1, LEZ block 29; job 2, block 30), the round trip DONE ([`lezcore-wait/`](lezcore-wait/)).

**Earlier, on `48a3b13d`** (before the view's reopening fix; the module the same): the module's checks 18 of 18
([`module-tests/`](module-tests/)), the dev-mode round trip inside Basecamp 13 of 13, 58 checks
([`basecamp-dev/`](basecamp-dev/)), Basecamp's ui-tests 10 of 10 ([`ui-tests/`](ui-tests/)), the devnet Qt Quick Test
17 of 17 ([`box-test/`](box-test/)). The view in a headless Logos Core did not run on `48a3b13d`: its first attempt
stopped in the devnet's setup ("the Bedrock node did not answer": the node ran but its API never answered, on a box
where the devnet Qt Quick Test had just run), its second on a replaced container without Ubuntu's Qt 6 (the qmltest
runner not built); both before the view's test.
