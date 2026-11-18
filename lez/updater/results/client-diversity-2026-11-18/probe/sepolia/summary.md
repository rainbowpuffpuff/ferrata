# Client diversity of public light-client data: probe

Started 2026-11-18T16:06:09Z; 20 rounds, 12.0 s apart; User-Agent `ferrata-updater`; read-only GETs.

## sepolia

| endpoint | `/eth/v1/node/version` | client | `finality_update` | layout |
|---|---|---|---|---|
| `http://unstable.sepolia.beacon-api.nimbus.team` | 200: `Nimbus/v26.10.2-5628de-stateofus` | nimbus | 200 (1448 bytes) | gloas |
| `https://lodestar-sepolia.chainsafe.io` | 200: `Lodestar/v1.51.0/7923280` | lodestar | 200 (1448 bytes) | gloas |
| `https://ethereum-sepolia-beacon-api.publicnode.com` | 200: `Lighthouse/v8.3.3-57c2abc/x86_64-linux` | lighthouse | 404 |  |

Finality updates asked of every endpoint at the same moment:

| round | `unstable.sepolia.beacon-api.nimbus.team` (nimbus) | `lodestar-sepolia.chainsafe.io` (lodestar) | `ethereum-sepolia-beacon-api.publicnode.com` (lighthouse) | verdict |
|---|---|---|---|---|
| 0 | c2169dd6b63e att 11607029 sig 11607030 fin 11606944 (487) | 0c5207b68b48 att 11607020 sig 11607022 fin 11606944 (493) | HTTP 404 | different moment |
| 1 | cf0f65cda4cc att 11607030 sig 11607031 fin 11606944 (487) | 0c5207b68b48 att 11607020 sig 11607022 fin 11606944 (493) | HTTP 404 | different moment |
| 2 | cbab58505481 att 11607031 sig 11607032 fin 11606944 (487) | 0c5207b68b48 att 11607020 sig 11607022 fin 11606944 (493) | HTTP 404 | different moment |
| 3 | 94edb8518ecc att 11607033 sig 11607034 fin 11606944 (487) | 0c5207b68b48 att 11607020 sig 11607022 fin 11606944 (493) | HTTP 404 | different moment |
| 4 | 8bb626ecd8b5 att 11607034 sig 11607035 fin 11606944 (488) | 0c5207b68b48 att 11607020 sig 11607022 fin 11606944 (493) | HTTP 404 | different moment |
| 5 | 0bdacc5b6137 att 11607035 sig 11607036 fin 11606944 (472) | 0c5207b68b48 att 11607020 sig 11607022 fin 11606944 (493) | HTTP 404 | different moment |
| 6 | c5ba34b03773 att 11607036 sig 11607037 fin 11606944 (473) | 0c5207b68b48 att 11607020 sig 11607022 fin 11606944 (493) | HTTP 404 | different moment |
| 7 | 0acae79f6d8a att 11607037 sig 11607038 fin 11606944 (472) | 0c5207b68b48 att 11607020 sig 11607022 fin 11606944 (493) | HTTP 404 | different moment |
| 8 | a080e202e050 att 11607038 sig 11607039 fin 11606944 (487) | 0c5207b68b48 att 11607020 sig 11607022 fin 11606944 (493) | HTTP 404 | different moment |
| 9 | 8760720394a3 att 11607039 sig 11607040 fin 11606944 (487) | 0c5207b68b48 att 11607020 sig 11607022 fin 11606944 (493) | HTTP 404 | different moment |
| 10 | 0407a1aa34b0 att 11607040 sig 11607041 fin 11606976 (492) | 0407a1aa34b0 att 11607040 sig 11607041 fin 11606976 (492) | HTTP 404 | identical |
| 11 | a57c4ffff948 att 11607041 sig 11607042 fin 11606976 (492) | 0407a1aa34b0 att 11607040 sig 11607041 fin 11606976 (492) | HTTP 404 | different moment |
| 12 | 6e1002dbefa2 att 11607042 sig 11607043 fin 11606976 (476) | 0407a1aa34b0 att 11607040 sig 11607041 fin 11606976 (492) | HTTP 404 | different moment |
| 13 | 6e1002dbefa2 att 11607042 sig 11607043 fin 11606976 (476) | 0407a1aa34b0 att 11607040 sig 11607041 fin 11606976 (492) | HTTP 404 | different moment |
| 14 | ff76e4ee3d13 att 11607044 sig 11607045 fin 11606976 (491) | 0407a1aa34b0 att 11607040 sig 11607041 fin 11606976 (492) | HTTP 404 | different moment |
| 15 | e327aa4aa614 att 11607045 sig 11607046 fin 11606976 (491) | 0407a1aa34b0 att 11607040 sig 11607041 fin 11606976 (492) | HTTP 404 | different moment |
| 16 | cd0339fa8fea att 11607046 sig 11607047 fin 11606976 (491) | 0407a1aa34b0 att 11607040 sig 11607041 fin 11606976 (492) | HTTP 404 | different moment |
| 17 | dff9f49fc529 att 11607047 sig 11607048 fin 11606976 (491) | 0407a1aa34b0 att 11607040 sig 11607041 fin 11606976 (492) | HTTP 404 | different moment |
| 18 | 52be55bbb006 att 11607048 sig 11607049 fin 11606976 (491) | 0407a1aa34b0 att 11607040 sig 11607041 fin 11606976 (492) | HTTP 404 | different moment |
| 19 | 2814403ea2b1 att 11607049 sig 11607050 fin 11606976 (491) | 0407a1aa34b0 att 11607040 sig 11607041 fin 11606976 (492) | HTTP 404 | different moment |

Rounds: identical 1, different moment 19, DISAGREE 0, incomplete 0

Period updates `updates?start_period=1412&count=5` (head period 1416; periods below it are finished):

| period | `unstable.sepolia.beacon-api.nimbus.team` | `lodestar-sepolia.chainsafe.io` | `ethereum-sepolia-beacon-api.publicnode.com` | verdict |
|---|---|---|---|---|
| 1412 | bbb120067ead att 11567172 sig 11567173 (504) | bbb120067ead att 11567172 sig 11567173 (504) | — | identical |
| 1413 | 9605806e1184 att 11575363 sig 11575364 (507) | 9605806e1184 att 11575363 sig 11575364 (507) | — | identical |
| 1414 | c03b0581a700 att 11583552 sig 11583553 (511) | c03b0581a700 att 11583552 sig 11583553 (511) | — | identical |
| 1415 | bf7abe77b6ed att 11591744 sig 11591745 (509) | bf7abe77b6ed att 11591744 sig 11591745 (509) | — | identical |
| 1416 | dd6d4b2726d8 att 11599938 sig 11599939 (506) | dd6d4b2726d8 att 11599938 sig 11599939 (506) | — | identical |

Ended 2026-11-18T16:10:07Z.
