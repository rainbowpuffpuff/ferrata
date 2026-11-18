# live-bytes: live, mainnet, min_distinct_clients = 2, client_match = bytes

Started 2026-11-18T17:46:13Z, 10.0 min, poll 12 s; daemon exit 0.
Checkpoint: finalized header of period 1913's update, slot 15672512, root 0x76b355f991b2bf9bb8c2c29256d19c3a97dee49b954ecc424a1e7c80f03e6f62.
Sources: `nimbus` http://testing.mainnet.beacon-api.nimbus.team, `lodestar` https://lodestar-mainnet.chainsafe.io

- transactions: 16 (16 applied, 0 refused, 0 rejected)
- jobs opened (each update chosen): 3
- rounds an update was not sent for lack of clients: 14
- disagreements logged: 0

Jobs:

    job 0x526a816d829b0390: finalized slot 15672512 root 0x76b355f991b2bf9bb8c2c29256d19c3a97dee49b954ecc424a1e7c80f03e6f62 (attested slot 15672590, signature slot 15672591, 511 of 512 signers; adopts the next committee), served or confirmed by nimbus, lodestar; clients lodestar, nimbus
    job 0x675592d0b0f2117e: finalized slot 15679520 root 0xcd6fec0789ceab089ea24dbed3d6d32a2b106de6a6404e799d1b7b0ef4cf2796 (attested slot 15679593, signature slot 15679594, 511 of 512 signers; advances; rotates the committees; adopts the next committee), served or confirmed by nimbus, lodestar; clients lodestar, nimbus
    job 0xe6fd5ba0c085f55e: finalized slot 15683296 root 0xab78f2a4b08aa51c585b0716a67d5fa6be6aafae87907c84f2deb5b97bb52711 (attested slot 15683361, signature slot 15683362, 510 of 512 signers; advances), served or confirmed by nimbus, lodestar; clients lodestar, nimbus

Disagreements (first 5):


Not sent (first 3, last 3):

    2026-11-18T17:48:01Z [live-bytes mainnet] update finalizing slot 15683264 (attested slot 15683336): nimbus only, 1 of the 2 clients policy.min_distinct_clients asks; not sent
    2026-11-18T17:49:08Z [live-bytes mainnet] update finalizing slot 15683264 (attested slot 15683328): lodestar only, 1 of the 2 clients policy.min_distinct_clients asks; not sent
    2026-11-18T17:49:25Z [live-bytes mainnet] update finalizing slot 15683264 (attested slot 15683343): nimbus only, 1 of the 2 clients policy.min_distinct_clients asks; not sent
    2026-11-18T17:52:00Z [live-bytes mainnet] update finalizing slot 15683264 (attested slot 15683356): nimbus only, 1 of the 2 clients policy.min_distinct_clients asks; not sent
    2026-11-18T17:52:15Z [live-bytes mainnet] update finalizing slot 15683264 (attested slot 15683358): nimbus only, 1 of the 2 clients policy.min_distinct_clients asks; not sent
    2026-11-18T17:52:41Z [live-bytes mainnet] update finalizing slot 15683296 (attested slot 15683360): lodestar only, 1 of the 2 clients policy.min_distinct_clients asks; not sent
