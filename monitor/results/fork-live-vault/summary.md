# monitor on an anvil fork of Sepolia at the live vault: 2026-11-11T18:44:35Z

Fork of chain 11155111 at block 12113579; vault 0xfE3199fa2634413CbAC317371076Cae3EB2d9E54 (deployed at block 12111048).

- PASS the fork serves the live vault (rule B)
- PASS fork only: watchers[0] (slot 73) is the monitor's key
- PASS F0 the monitor runs as a watcher of the live vault
- PASS F0 every live checkpoint (1) audited, all ok against the devnet's final tip
- PASS F0 the real attestation (certificate, final-tier message v2) is good: no finding
- PASS F0 no poll failed
- PASS F1 the forged candidate is cancelled (tipConflict) before its deadline
- PASS F2 the candidate on the real final tip passes untouched, attested by the live key

Live checkpoints audited: 1; cancellation of F1: {"secondsAfterSubmission":1,"secondsBeforeDeadline":3599,"deadline":1794426270,"tx":"0x03e291651f918d3943b78602586977ec952b50456e56d480c0f5af1210d67b3a"}.

Alerts: "cancelled 1, candidate-passed 1, started 2, stopped 2".

Failures: 0
