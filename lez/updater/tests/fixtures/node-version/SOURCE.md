# `/eth/v1/node/version` answers, recorded

What three public beacon APIs answered to `GET /eth/v1/node/version` on 18 November 2026 at 16:06Z (read-only, User-Agent
`ferrata-updater`, from a Modal build box), byte for byte, as `../../results/client-diversity-2026-11-18/probe.py` saved
them (`../../results/client-diversity-2026-11-18/probe/mainnet/answers/`):

| file | endpoint |
|---|---|
| `nimbus.json` | `http://testing.mainnet.beacon-api.nimbus.team` |
| `lodestar.json` | `https://lodestar-mainnet.chainsafe.io` |
| `lighthouse.json` | `https://ethereum-beacon-api.publicnode.com` |

The tests (`tests/config.rs`, `tests/recorded.rs`) read the client family from them and serve them from loopback
beacon nodes. What a node reports here is its own, unauthenticated claim.
