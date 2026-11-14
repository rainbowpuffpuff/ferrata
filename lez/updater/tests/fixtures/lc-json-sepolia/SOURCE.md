# Sepolia light-client data as JSON and as SSZ, for `src/lc_json.rs`'s tests

Fetched 2026-11-14 around 12:48–12:50Z (UTC) with `curl`, User-Agent `ferrata-probe`, from public endpoints without
an API key, read only. Stored byte for byte; `SHA256SUMS` pins every file.

| file | request | `Accept` | provider |
|---|---|---|---|
| `bootstrap.json` | `GET /eth/v1/beacon/light_client/bootstrap/0x3b901f5e6853b5cf5eb04d7ccfe1272e66c38291cb3a11dd159cacd1117ccf7c` | `application/json` | `unstable.sepolia.beacon-api.nimbus.team` (Status) |
| `bootstrap.ssz` | the same, 12 Nov: a copy of `crates/ferrata-spectests/fixtures/sepolia-2026-11-12/bootstrap.ssz` | `application/octet-stream` | `lodestar-sepolia.chainsafe.io` (ChainSafe) |
| `updates_1411.json` | `GET /eth/v1/beacon/light_client/updates?start_period=1411&count=1` | `application/json` | Lodestar |
| `updates_1411.stream` | the same | `application/octet-stream` (the stream: 8-byte length, 4-byte fork digest, the update) | Lodestar |
| `update_1411.ssz` | the same, 12 Nov, split: a copy of that fixture's `update_1411.ssz` (equal to the stream's update today) | | Lodestar |
| `finality_update.json` | `GET /eth/v1/beacon/light_client/finality_update` (attested slot 11,577,248, finalized 11,577,184, signature slot 11,577,249) | `application/json` | Lodestar |
| `finality_update.ssz` | the same, seconds later, with the same signature slot | `application/octet-stream` | Lodestar |

On 14 Nov Lodestar answered the bootstrap request with HTTP 500 and publicnode with 404; Nimbus's node served it,
byte for byte the 12 Nov fixture's SSZ. Every object is in the Gloas layout (Sepolia's since epoch 353,024).
