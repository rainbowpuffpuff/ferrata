# seqnet robust: U.03 and R.07 on a running LEZ sequencer

- PASS U.03: ferrata-relayer imported root(cid, 2) on the running sequencer (tx 7962b57fc0b6efa3c44bd36d60f721bfdf2644ce164f673a333fe0ad29adf125, recovered true); 2 rounds before it was applied
- PASS U.03: SIGTERM: a clean stop line ({"event":"stop","reason":"SIGTERM","rounds":3,"t":1795307798})
- PASS U.03: nothing left in flight in the relayer's state
- PASS U.05: the claim's dry run preselects the gas drop: fee payer: a fresh one-time private account, credited by the relayer (sealed) and swept in the claim's proof; the relaye
- PASS M3: a claim naming a relayer that never answers handed it a job (gas-drop-cd6efdc213344cb4-bbbbbbbb.json) and stopped after its 15 s wait, nothing sent (one-time account 0xa94c201bbb95f7e5770bf20cc4d09b48620066c5d32a6780d1d6b2564afc06f3)
- PASS R.06 gas drop: the drops daemon SIGKILLed inside its first credit's send (held unsent by the proxy); its record: {"sends":1,"settled":false,"tx":"pending"}
- PASS R.07 claim: the tool SIGKILLed after its proof, with the claim's sendTransaction held unsent ({"t": 1795307865.833941, "fired": "trap sendTransaction"})
- PASS R.07 claim: nothing reached the zone (2 private transactions before and after) and recovery from the seed still finds the deposit claimable
- PASS R.07 claim + U.05: the claim again reused the one-time account and the credit the killed run had (no second job): gas drop: one-time account 0xa94c201bbb95f7e5770bf20cc4d09b48620066c5d32a6780d1d6b2564afc06f3 (recorded for this deposit by an earlier run); the relay
- PASS M3: the same deposit's claim naming the real relayer reused the one-time account (0xa94c201bbb95f7e5770bf20cc4d09b48620066c5d32a6780d1d6b2564afc06f3) and handed that relayer a receipt of its own; the output names the silent one
- PASS R.06 gas drop: after the kill and a failed send (503) the restarted daemon sent the credit again on the same account state and booked it from LEZ: {"sends":3,"resolvedLater":true,"tx":"2b827bc6a6a315fd6dfb57edb60728e8d863985bb2cb2cfbc2774d44e354d1ef"}; the claim's dry run claims exactly 1 pending credit
- PASS R.06 gas drop: the second credit forwarded with its answer held and the daemon SIGKILLed (00:38:38); restarted, it booked the drop from LEZ (its drop account's nonce moved), nothing sent again: {"sends":1,"resolvedLater":true,"tx":"pending"}; the claim's dry run claims exactly 1 pending credit
- PASS U.05: the relayer daemon sent 2 gas drops, one per deposit, each sealed to the claim's one-time account (0 jobs refused); both claims' dry runs claim that credit to their one-time fee payer, and both applied: gas drop: one-time account 0xc814a73facbb8f0e94fb439c2cad785f1792e95a8a86860e5eae78dc8c2f3213 (new); the relayer's credit of 50000000 native LEZ, seal
- PASS U.05: the gas-drop daemon stopped cleanly on SIGTERM ({"event":"stop","reason":"SIGTERM","rounds":1,"t":1795307950})
- PASS R.07 claim: after the interrupted claim, both deposits claimed once each (recover: 2 claimed)
- PASS R.07 burn: the tool SIGKILLed with its burn at the sequencer and the answer held; its record: {"i":0,"amount":"0.001","at":1795307966}
- PASS R.07 burn: the burn the killed tool sent was applied (the zone: 6 -> 7 private transactions)
- PASS R.07 burn: the same command again found that burn on the zone and burnt nothing more (an earlier run's burn (counter 0, 0.001 ETH to 0x0f68ac1761DcEb07Ef1D4C3b08aE1c9A3d32Cc51) was sent and is on the zone); the zone still 7 private transactions
- PASS the release through the relayer paid 997000000000000 wei to the fresh address
- PASS R.07 burn: a burn SIGKILLed with its send trapped (it never left the proxy); its record: {"i":1,"amount":"0.001","recipient":"0x0f68ac1761dceb07ef1d4c3b08ae1c9a3d32cc51"}
- PASS R.07 burn: nothing reached the zone (8 -> 8 private transactions); the burn with another recipient took the interrupted burn's counter 1 and was applied once (8 -> 9); the record cleared

21 passed, 0 failed
