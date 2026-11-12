# ferrata_attestor_module beside each sequencer of a running zone of four (2026-11-12T21:30:01Z)

logosctl version pre-release-66d098d; 300 s of sequencing, polls every 15 s; zone.sh and the hooks took 344 s (exit 0).

- PASS the zone ran and its genesis is the one runs/zone's deployment was made for (deployment-settings sha256)
- PASS four Logos Cores loaded the module; each attestation key certified by its sequencer's channel key
- PASS the four loops started against the node's HTTP API
- PASS the node's final tier: tip 30, four keys, every sequencer inscribed
- PASS attestor 0: 22 rounds while the zone ran; signed 19 rising tips (3 4 6 8 9 11 13 14 16 17 19 20 22 23 25 26 28 29 30 ), the last the final tip 30; refused 1 rounds (no final block yet), errors 0
- PASS attestor 1: 22 rounds while the zone ran; signed 19 rising tips (3 4 6 8 9 11 13 14 16 17 19 20 22 23 25 26 28 29 30 ), the last the final tip 30; refused 1 rounds (no final block yet), errors 0
- PASS attestor 2: 22 rounds while the zone ran; signed 19 rising tips (3 4 6 8 9 11 13 14 16 17 19 20 22 23 25 26 28 29 30 ), the last the final tip 30; refused 1 rounds (no final block yet), errors 0
- PASS attestor 3: 22 rounds while the zone ran; signed 19 rising tips (3 4 6 8 9 11 13 14 16 17 19 20 22 23 25 26 28 29 30 ), the last the final tip 30; refused 1 rounds (no final block yet), errors 0
- PASS the base over the zone's final blocks has the deployment the attestors certified
- PASS verify: the four published attestations verify
- PASS collect 3 of 4 (attestor 3 left out): accepted by the checkpoint program's rule natively, n = 4, t = 3
- PASS collect 4 of 4: accepted
- PASS collect 2 of 4: refused

Collected input (3 of 4): 26166 bytes, sha256 78febbe4792b33ea13f1294e4223f2fbad9518938b4b0bb375362418fc5e75d1; journal sha256 a58d6a4de2254c2fd9227e9b56f81723632b6d14bbb30ac8a5b577009093103e; accepted: n = 4, t = 3, 3 attestations counted, producer fce0d0fadf637a62981f6c8772a235d0e4533fcd572dc6ad9aa6a6bad9cc8096.

Checks failed: 0
