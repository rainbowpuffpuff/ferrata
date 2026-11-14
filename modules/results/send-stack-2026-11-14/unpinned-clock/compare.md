| step | call | equal: to, input, value, from, nonce, status, gasUsed, logs, block; state after | events | gas used | plain: gas limit, effective gas price | Logos (tx_sender_module): gas limit, effective gas price | path reported |
|---|---|---|---|---|---|---|---|
| M1-cancelCandidate | `cancelCandidate` (36 B) | all equal | CandidateCancelled | 41,477 | 52,163, 148,930,902 wei | 52,163, 130,366,017 wei | plain yes, logos yes |
| M2-cancel-and-pause | `cancelCandidate` (36 B) | all equal | CandidateCancelled | 41,477 | 52,163, 99,985,253 wei | 52,163, 76,615,919 wei | plain yes, logos yes |
| M2-cancel-and-pause | `pauseCheckpoints` (4 B) | all equal | CheckpointsPaused | 47,260 | 47,260, 87,521,656 wei | 47,260, 67,065,411 wei | plain yes, logos yes |
| R1-submitCandidate | `submitCandidate` (484 B) | DIFFERENT: gasUsed, state | CandidateSubmitted | 138,471 | 141,271, 559,306,105 wei | 138,471, 490,051,290 wei | plain yes, logos yes |
| R2-finalizeAttested | `finalizeAttested` (420 B) | all equal | CheckpointFinalized | 216,167 | 235,367, 375,690,317 wei | 235,367, 329,405,793 wei | plain yes, logos yes |
| R3-release | `release` (292 B) | all equal | Release, OwedCredited | 173,273 | 175,312, 288,823,458 wei | 175,312, 221,570,865 wei | plain yes, logos yes |
| R3-release | `claimOwed` (36 B) | all equal | OwedClaimed | 35,786 | 47,107, 253,137,571 wei | 47,107, 194,194,443 wei | plain yes, logos yes |

Fallback (tx_sender_module unloaded, the watcher key held): 1 transaction(s) cancelCandidate, status [1], reported as [{"fallback": true, "path": "plain", "reason": "tx_sender_module.send did not answer: tx_sender_module.send did not answer: PluginCallFailed { plugin: \"tx_sender_module\", method: \"send\", message: \"{\\\"code\\\":\\\"object_unavailable\\\",\\\"message\\\":\\\"failed to acquire remote object 'tx_sender_module' (module not loaded, not published, or transport failure)\\\",\\\"origin\\\":\\\"tx_sender_module\\\"}\" }"}].

4 of 5 steps equal on both paths.
