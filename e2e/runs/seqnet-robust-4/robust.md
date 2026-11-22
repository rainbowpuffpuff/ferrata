# seqnet robust: U.03 and R.07 on a running LEZ sequencer

- PASS U.03: ferrata-relayer imported root(cid, 2) on the running sequencer (tx 013c8aa31ba9978a3e0a92e43e2b897c846a9e392eafe1a78040ff5504e452fe, recovered true); 2 rounds before it was applied
- PASS U.03: SIGTERM: a clean stop line ({"event":"stop","reason":"SIGTERM","rounds":3,"t":1795323584})
- PASS U.03: nothing left in flight in the relayer's state
- PASS U.05: the claim's dry run preselects the gas drop: fee payer: a fresh one-time private account, credited by the relayer (sealed) and swept in the claim's proof; the relaye
- PASS M3: a claim naming a relayer that never answers handed it a job (gas-drop-cd6efdc213344cb4-bbbbbbbb.json) and stopped after its 15 s wait, nothing sent (one-time account 0xa94c201bbb95f7e5770bf20cc4d09b48620066c5d32a6780d1d6b2564afc06f3)
- PASS R.06 gas drop: the drops daemon SIGKILLed inside its first credit's send (held unsent by the proxy); its record: {"sends":1,"settled":false,"tx":"pending"}
- PASS R.07 claim: the tool SIGKILLed after its proof, with the claim's sendTransaction held unsent ({"t": 1795323651.5468624, "fired": "trap sendTransaction"})
- PASS R.07 claim: nothing reached the zone (2 private transactions before and after) and recovery from the seed still finds the deposit claimable
- PASS R.07 claim + U.05: the claim again reused the one-time account and the credit the killed run had (no second job): gas drop: one-time account 0xa94c201bbb95f7e5770bf20cc4d09b48620066c5d32a6780d1d6b2564afc06f3 (recorded for this deposit by an earlier run); the relay
- PASS M3: the same deposit's claim naming the real relayer reused the one-time account (0xa94c201bbb95f7e5770bf20cc4d09b48620066c5d32a6780d1d6b2564afc06f3) and handed that relayer a receipt of its own; the output names the silent one
- PASS R.06 gas drop: after the kill and a send answered 503 the restarted daemon sent the credit again on the same account state and booked it from LEZ: {"sends":3,"resolvedLater":true,"tx":"d0cce718509b604de1ba8986d5d413e3f6fe3c2015403404f86d93f8267be304"}; the claim's dry run claims exactly 1 pending credit
- PASS R.06 gas drop (the second review's M1): two credits of the first drop reached the sequencer and were accepted: a later send (3 sends; the drop booked with d0cce718509b604de1ba8986d5d413e3f6fe3c2015403404f86d93f8267be304) and the send answered 503 to the daemon, forwarded late (2a5d9970ec3d36184527338dd4e957852cf36884023d3c4940686697316aea4f); the claim's dry run claims exactly 1 pending credit (a credit sent again is built only on the drop's recorded state: of the two, one applied)
- PASS R.06 gas drop: the second credit forwarded with its answer held and the daemon SIGKILLed (05:01:47); restarted, it booked the drop from LEZ (its drop account's nonce moved), nothing sent again: {"sends":1,"resolvedLater":true,"tx":"pending"}; the claim's dry run claims exactly 1 pending credit
- PASS U.05: the relayer daemon sent 2 gas drops, one per deposit, each sealed to the claim's one-time account (0 jobs refused); both claims' dry runs claim that credit to their one-time fee payer, and both applied: gas drop: one-time account 0xc814a73facbb8f0e94fb439c2cad785f1792e95a8a86860e5eae78dc8c2f3213 (new); the relayer's credit of 50000000 native LEZ, seal
- PASS U.05: the gas-drop daemon stopped cleanly on SIGTERM ({"event":"stop","reason":"SIGTERM","rounds":1,"t":1795323739})
- PASS R.07 claim: after the interrupted claim, both deposits claimed once each (recover: 2 claimed)
- PASS R.07 burn: the tool SIGKILLed with its burn at the sequencer and the answer held; its record: {"i":0,"amount":"0.001","at":1795323760}
- PASS R.07 burn: the burn the killed tool sent was applied (the zone: 6 -> 7 private transactions)
- PASS R.07 burn: the same command again found that burn on the zone and burnt nothing more (an earlier run's burn (counter 0, 0.001 ETH to 0x7b1bDf63516be912eE92393D3fA2e736Dd58Bc47) was sent and is on the zone); the zone still 7 private transactions
- PASS the release through the relayer paid 997000000000000 wei to the fresh address
- PASS R.07 burn: a burn SIGKILLed with its send trapped (it never left the proxy); its record: {"i":1,"amount":"0.001","recipient":"0x7b1bdf63516be912ee92393d3fa2e736dd58bc47"}
- PASS R.07 burn: nothing reached the zone (8 -> 8 private transactions); the burn with another recipient took the interrupted burn's counter 1 and was applied once (8 -> 9); the record cleared

22 passed, 0 failed
