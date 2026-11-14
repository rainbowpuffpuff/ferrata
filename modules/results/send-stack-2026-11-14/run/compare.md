| step | call | equal: to, input, value, from, nonce, status, gasUsed, logs, block; state after | events | gas used | plain: gas limit, effective gas price | Logos (tx_sender_module): gas limit, effective gas price | path reported |
|---|---|---|---|---|---|---|---|
| M1-cancelCandidate | `cancelCandidate` (36 B) | all equal | CandidateCancelled | 41,477 | 52,163, 148,934,854 wei | 52,163, 130,369,476 wei | plain yes, logos yes |
| M2-cancel-and-pause | `cancelCandidate` (36 B) | all equal | CandidateCancelled | 41,477 | 52,163, 99,987,906 wei | 52,163, 76,617,951 wei | plain yes, logos yes |
| M2-cancel-and-pause | `pauseCheckpoints` (4 B) | all equal | CheckpointsPaused | 47,260 | 47,260, 87,523,978 wei | 47,260, 67,067,190 wei | plain yes, logos yes |
| R1-submitCandidate | `submitCandidate` (484 B) | all equal | CandidateSubmitted | 141,271 | 141,271, 559,306,044 wei | 141,271, 490,051,237 wei | plain yes, logos yes |
| R2-finalizeAttested | `finalizeAttested` (420 B) | all equal | CheckpointFinalized | 216,167 | 235,367, 375,700,282 wei | 235,367, 329,414,531 wei | plain yes, logos yes |
| R3-release | `release` (292 B) | all equal | Release, OwedCredited | 173,273 | 175,312, 288,831,120 wei | 175,312, 221,576,743 wei | plain yes, logos yes |
| R3-release | `claimOwed` (36 B) | all equal | OwedClaimed | 35,786 | 47,107, 253,144,286 wei | 47,107, 194,199,595 wei | plain yes, logos yes |

Fallback (tx_sender_module unloaded, the watcher key held): 1 transaction(s) cancelCandidate, status [1], reported as [{"fallback": true, "path": "plain", "reason": "tx_sender_module.send did not answer: tx_sender_module.send did not answer: PluginCallFailed { plugin: \"tx_sender_module\", method: \"send\", message: \"{\\\"code\\\":\\\"object_unavailable\\\",\\\"message\\\":\\\"failed to acquire remote object 'tx_sender_module' (module not loaded, not published, or transport failure)\\\",\\\"origin\\\":\\\"tx_sender_module\\\"}\" }"}].

5 of 5 steps equal on both paths.
