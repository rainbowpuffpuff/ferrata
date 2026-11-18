# live-header: live, mainnet, min_distinct_clients = 3, client_match = finalized-header

Started 2026-11-18T17:46:13Z, 10.0 min, poll 12 s; daemon exit 0.
Checkpoint: finalized header of period 1913's update, slot 15672512, root 0x76b355f991b2bf9bb8c2c29256d19c3a97dee49b954ecc424a1e7c80f03e6f62.
Sources: `nimbus` http://testing.mainnet.beacon-api.nimbus.team, `lodestar` https://lodestar-mainnet.chainsafe.io; headers: `lighthouse` https://ethereum-beacon-api.publicnode.com

- transactions: 19 (19 applied, 0 refused, 0 rejected)
- jobs opened (each update chosen): 4
- rounds an update was not sent for lack of clients: 1
- disagreements logged: 0

Jobs:

    job 0x526a816d829b0390: finalized slot 15672512 root 0x76b355f991b2bf9bb8c2c29256d19c3a97dee49b954ecc424a1e7c80f03e6f62 (attested slot 15672590, signature slot 15672591, 511 of 512 signers; adopts the next committee), served or confirmed by nimbus, lodestar, lighthouse; clients lighthouse, lodestar, nimbus
    job 0xcc0005aefee93be7: finalized slot 15683264 root 0x9f2153b308017a362483cf43db745be5df484c89f3b404c16487212e66422499 (attested slot 15683337, signature slot 15683338, 511 of 512 signers; advances; rotates the committees), served or confirmed by nimbus, lodestar, lighthouse; clients lighthouse, lodestar, nimbus
    job 0x51618dac72c42d2a: finalized slot 15679520 root 0xcd6fec0789ceab089ea24dbed3d6d32a2b106de6a6404e799d1b7b0ef4cf2796 (attested slot 15679593, signature slot 15679594, 511 of 512 signers; adopts the next committee), served or confirmed by nimbus, lodestar, lighthouse; clients lighthouse, lodestar, nimbus
    job 0xb9d432ca03094656: finalized slot 15683296 root 0xab78f2a4b08aa51c585b0716a67d5fa6be6aafae87907c84f2deb5b97bb52711 (attested slot 15683361, signature slot 15683362, 510 of 512 signers; advances), served or confirmed by nimbus, lodestar, lighthouse; clients lighthouse, lodestar, nimbus

Disagreements (first 5):


Not sent (first 3, last 3):

    2026-11-18T17:52:42Z [live-header mainnet] update finalizing slot 15683296 (attested slot 15683360): lighthouse, lodestar only, 2 of the 3 clients policy.min_distinct_clients asks; not sent
    2026-11-18T17:52:42Z [live-header mainnet] update finalizing slot 15683296 (attested slot 15683360): lighthouse, lodestar only, 2 of the 3 clients policy.min_distinct_clients asks; not sent
