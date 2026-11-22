# ferrata_attestor_module on the zone's records (native, 2026-11-22T01:10:48Z)

- PASS version: the module names the attestor it was built from
- PASS tier: tip 30, four keys, the blocks per signer = the run's tier.json
- PASS base: blocks 3..=30; its deployment = the run's deployment.json
- PASS certify 0: the channel key's certificate = the run's cert-0.json
- PASS attest 0: tip 30 signed, the LEZ node's highest final block (a first read starts there); the signed attestation = the run's att-0.json (encoded)
- PASS certify 1: the channel key's certificate = the run's cert-1.json
- PASS attest 1: tip 30 signed, the LEZ node's highest final block (a first read starts there); the signed attestation = the run's att-1.json (encoded)
- PASS certify 2: the channel key's certificate = the run's cert-2.json
- PASS attest 2: tip 30 signed, the LEZ node's highest final block (a first read starts there); the signed attestation = the run's att-2.json (encoded)
- PASS certify 3: the channel key's certificate = the run's cert-3.json
- PASS attest 3: tip 30 signed, the LEZ node's highest final block (a first read starts there); the signed attestation = the run's att-3.json (encoded)
- PASS verify: the four attestations verify
- PASS collect 3 of 4 (sequencer 3 silent): accepted natively, n = 4, t = 3, the input = the one the checkpoint program proved (sha256 f376ad54cf979cefcd7996dd56b6aacabfca67bb9c74161cc66e71da1389c575)
- PASS collect 3 of 4: its journal = the run's (rbz-zone-n4-t3.json)
- PASS collect 4 of 4: accepted, its journal = the run's (rbz-zone-n4-all.json)
- PASS collect 2 of 4: refused (t(4) = 3)
- PASS assemble 2 of 4: the program's own rule refuses it (InsufficientAttestations { have: 2, need: 3 }), as the run's refusal input
- PASS refused: it signed tip 31 and its LEZ node holds no block 31 (the node's final tip, 30, is below it): it never signs a lower tip
- PASS refused: it signed another block 30 than the one its LEZ node holds final: it never signs two histories
- PASS refused: a certificate of another attestation key
- PASS refused: the same certificate for another vault (a certificate binds chain, vault, configuration and channel)
- PASS loop: 3 rounds against the node over HTTP: signed tip 30 once, then found it unchanged twice; nothing refused
- PASS loop: its published attestation = the one-shot attest's (att-30.json, latest.json)
- PASS loop: it read the node each round (info, channel, blocks) and asked its LEZ node (getBlock)

Checks failed: 0
