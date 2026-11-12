# ferrata_updater_module on the in-process LEZ chain with the lab's records (logosctl, 2026-11-12T21:38:41Z)

- PASS version: the attestation program the module carries has the id of docs/image-ids.md (0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df), declared and computed from its bytes
- PASS check_config: two chains (mainnet, sepolia), two sources each, ledger harness
- PASS the sequencer backend is refused, as by the binary (it is not built)
- PASS start: the daemon runs in the module
- PASS status: polled 23 times while the daemon ran; then stop ended it
- PASS up to date after 213 rounds (118 s)
- PASS mainnet: finalized slot 15,631,808, root = the lab's (530599ec91a6ce70948d70d1066bb35a2a0cb999ad8e2cb6229452dcb7102328)
- PASS sepolia: finalized slot 11,556,000, root = the lab's (d31b7d34c2e6b6d8c3430dcdc763375d0ba9a9b201554164326133367aa3f3cc)
- PASS health: both instances ok
- PASS metrics: 187 transactions applied, none refused

Jobs applied (InstanceState.applied): mainnet 4, sepolia 35; rounds 213; 118 s.

Checks failed: 0
