# ferrata_attestor_module beside each sequencer of a running zone of four (2026-11-12T23:15:33Z)

logosctl version pre-release-66d098d; 300 s of sequencing, polls every 15 s; zone.sh and the hooks took 347 s (exit 0).

- PASS the zone ran and its genesis is the one runs/zone's deployment was made for (deployment-settings sha256)
- PASS four Logos Cores loaded the module; each attestation key certified by its sequencer's channel key
- PASS the four loops started against the node's HTTP API
- PASS the node's final tier: tip 29, four keys, every sequencer inscribed
- PASS attestor 0: 22 rounds while the zone ran; signed 19 rising tips (3 4 6 7 8 10 11 13 14 16 17 19 20 22 24 25 27 28 29 ), the last the final tip 29; refused 1 rounds (no final block yet), errors 0
- PASS attestor 1: 22 rounds while the zone ran; signed 19 rising tips (3 4 6 7 8 10 11 13 14 16 17 19 20 22 24 25 27 28 29 ), the last the final tip 29; refused 1 rounds (no final block yet), errors 0
- PASS attestor 2: 22 rounds while the zone ran; signed 19 rising tips (3 4 6 7 8 10 11 13 14 16 17 19 21 22 24 25 27 28 29 ), the last the final tip 29; refused 1 rounds (no final block yet), errors 0
- PASS attestor 3: 22 rounds while the zone ran; signed 19 rising tips (3 4 6 7 8 10 11 13 14 16 17 19 21 22 24 25 27 28 29 ), the last the final tip 29; refused 1 rounds (no final block yet), errors 0
- PASS the base over the zone's final blocks has the deployment the attestors certified
- PASS verify: the four published attestations verify
- PASS collect 3 of 4 (attestor 2 left out): accepted by the checkpoint program's rule natively, n = 4, t = 3
- PASS collect 4 of 4: accepted
- PASS collect 2 of 4: refused

Collected input (3 of 4): 25323 bytes, sha256 0d7c33a8472a62340d7085b3886bb4c319d4fdcf749dd2cf7f4f4dab43ef9727; journal sha256 21fb67716047a2657c4cbc8fa2e53f9700b2920bccd48d19b859471ee181088c; accepted: n = 4, t = 3, 3 attestations counted, producer fce0d0fadf637a62981f6c8772a235d0e4533fcd572dc6ad9aa6a6bad9cc8096.

Checks failed: 0
