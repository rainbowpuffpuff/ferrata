# monitor on a local anvil chain: 2026-11-11T18:32:20Z

anvil Version: 1.3.6-v1.3.6, chain id 31337; vault 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0 (rule B, wait 1 h), watcher 0x4e1b0Ab982D1A8bdd8E59D07C0E883CA01B39F3a, freeze authority 0xc284F4C031c0D3CCCA69AD4877d43768bB9b2bfB.

- PASS S1 honest candidate still pending after 3 polls
- PASS S1 monitor passed it
- PASS S1 no cancellation of it
- PASS S1 finalised after the wait
- PASS S2 conflicting candidate cancelled
- PASS S2 cancelled before its deadline
- PASS S2 cannot be finalised after the wait
- PASS S3 stale key set cancelled (keySetNotLive)
- PASS S4 wrong one cancelled
- PASS S4 honest one untouched
- PASS S4 honest one finalised
- PASS S5 a second monitor on the same state file is refused while the first runs
- PASS S5 monitor saw it and was killed while waiting (node lag allowance)
- PASS S5 its state file survived the kill and names the candidate
- PASS S5 restarted monitor cancelled it before the deadline
- PASS S5 the reason is tipNotFinal
- PASS S6 fresh monitor cancelled the pending wrong candidate
- PASS S6 and nothing else (one cancellation in that run)
- PASS S6 it audited the two finalised checkpoints as good
- PASS S7 wrong checkpoint finalised with nobody watching
- PASS S7 watcher raised finalized-wrong with the revocation deadline
- PASS S7 the freeze authority's monitor revoked checkpoint 2
- PASS S8 first cancellation
- PASS S8 second cancellation and the watcher's pause (about 12 h)
- PASS S8 a submission is refused during the pause
- PASS A0 the second vault is rule (A) (mode 0), turnSeconds 900
- PASS A1 honest candidate passed and untouched
- PASS A2 stale key set cancelled at once (keySetNotLive)
- PASS A3 tip not final: waiting until submittedAt + delta_g
- PASS A3 cancelled after delta_g, before its deadline (tipNotFinal)
- PASS A1 finalised with a beacon recorded after its wait
- PASS A1 audited as good after finalisation
- PASS A4 final tier stalled: alert and the watcher's 12 h pause (rule A)
- PASS R1 the lab record's final tier: 258 blocks
- PASS R2 a candidate at the real tip 258 passes
- PASS R3 the same id with another hash is cancelled
- PASS R4 a tip above the final tier waits
- PASS alerts: all 16 critical alerts reached the pager command

## Cancellations and the revocation (chain time, seconds)

| Scenario | Reason | After submission | Before deadline | cancelCandidate gas |
|---|---|---|---|---|
| S2 | tipConflict | 0 | 3600 | 41477 |
| S3 | keySetNotLive | 0 | 3600 | 41477 |
| S4 | tipConflict | 0 | 3600 | 41477 |
| S5 | tipNotFinal | 601 | 2999 | 41468 |
| S6 | tipConflict | 0 | 3600 | 41477 |
| S8 | tipConflict | 0 | 3600 | 41477 |
| A2 | keySetNotLive | 0 | 11760 | 41477 |
| A3 | tipNotFinal | 7202 | 5458 | 41477 |

S7: checkpoint 2 (tipConflict) finalised at 1794436953, revoked at 1794436954, revocable until 1794523353.

Alerts: "cancelled 9, candidate-passed 3, candidate-resubmitted 1, candidate-waiting 2, checkpoints-revoked 2, final-tier-stalled 1, finalized-wrong 2, paused 2, persistent-candidate 1, revoked 1, started 17, stopped 16".

Failures: 0
