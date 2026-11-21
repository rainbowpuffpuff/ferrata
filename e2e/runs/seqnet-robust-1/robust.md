# seqnet robust: U.03 and R.07 on a running LEZ sequencer

- PASS U.03: ferrata-relayer imported root(cid, 2) on the running sequencer (tx 660dd6bdc6c2366626dd55da960a755bde6d6451667f4c5d7acbf11cdb34fd78, recovered true); 2 rounds before it was applied
- PASS U.03: SIGTERM: a clean stop line ({"event":"stop","reason":"SIGTERM","rounds":3,"t":1795296246})
- PASS U.03: nothing left in flight in the relayer's state
- PASS U.05: the claim's dry run preselects the gas drop: fee payer: a fresh one-time private account, credited by the relayer (sealed) and swept in the claim's proof; the relaye
- PASS R.07 claim: the tool SIGKILLed after its proof, with the claim's sendTransaction held unsent ({"t": 1795296252.8727002, "fired": "trap sendTransaction"})
- PASS R.07 claim: nothing reached the zone (2 private transactions before and after) and recovery from the seed still finds the deposit claimable
- PASS R.07 claim + U.05: the claim again reused the one-time account and the credit the killed run had (no second job): gas drop: one-time account 0xa94c201bbb95f7e5770bf20cc4d09b48620066c5d32a6780d1d6b2564afc06f3 (recorded for this deposit by an earlier run); the relay
- PASS U.05: the relayer daemon sent 2 gas drops, one per deposit, each sealed to the claim's one-time account (0 jobs refused); both claims' dry runs claim that credit to their one-time fee payer, and both applied: gas drop: one-time account 0xc814a73facbb8f0e94fb439c2cad785f1792e95a8a86860e5eae78dc8c2f3213 (new); the relayer's credit of 50000000 native LEZ, seal
- PASS U.05: the gas-drop daemon stopped cleanly on SIGTERM ({"event":"stop","reason":"SIGTERM","rounds":37,"t":1795296321})
- PASS R.07 claim: after the interrupted claim, both deposits claimed once each (recover: 2 claimed)
- PASS R.07 burn: the tool SIGKILLed with its burn at the sequencer and the answer held; its record: {"i":0,"amount":"0.001","at":1795296342}
- PASS R.07 burn: the burn the killed tool sent was applied (the zone: 6 -> 7 private transactions)
- PASS R.07 burn: the same command again found that burn on the zone and burnt nothing more (an earlier run's burn (counter 0, 0.001 ETH to 0x071f193B51C29eefB6f04bb64ce1DFF680290179) was sent and is on the zone); the zone still 7 private transactions
- PASS the release through the relayer paid 997000000000000 wei to the fresh address

14 passed, 0 failed
