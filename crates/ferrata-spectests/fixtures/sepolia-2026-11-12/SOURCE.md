# Sepolia: a Gloas bootstrap of period 1411 (slot 11,562,112) and the period updates from it

Fetched by `scripts/fetch-sepolia-checkpoint.py` between 2026-11-12T10:40:23Z and 2026-11-12T10:40:24Z (UTC). Every request went
to a public endpoint without an API key, with User-Agent `ferrata-fixture-fetch`. Responses are stored byte for byte,
except the `updates` stream, which is split into one file per period with its 8-byte length and 4-byte fork
digest removed (the digests are listed below). `SHA256SUMS` pins every file.

**Why a new checkpoint.** The attestation program bounds every transaction that creates or extends an
instance by the weak-subjectivity period of the instance's finalized header, by the sequencer's clock
(`crates/ferrata-core/src/ws.rs`); the crown run's instance is configured with `ws_period_epochs` 3,532
(`sepolia_config`). The period-1378 bootstrap of `sepolia-gloas-2026-11-10` (slot 11,288,576, 5 Oct 2026)
left that period at slot 11,401,632 (2026-10-21T03:26:24Z), so the crown run's `Init`
from it reverted on LEZ (`crown-live-2`, 12 Nov 2026). On 10 Nov Lodestar served no Gloas-era bootstrap
(that fixture's SOURCE.md). Now it serves one for its current finalized checkpoint and answers HTTP 500
"syncCommitteeWitness not available" for earlier ones (recorded below for the period's first checkpoint;
on 12 Nov 2026 around 10:40Z also for the first 24 epoch-start checkpoints of period 1411, by a first version
of this script, and for the 7 checkpoints before the then finalized one, by hand), so the bootstrap is the
checkpoint Lodestar had finalized at the fetch.

**The period's update is older than the bootstrap, and still taken.** Lodestar's best update of the
bootstrap's period finalizes that period's first checkpoint, before the bootstrap. A store fresh from the
bootstrap does not know the next sync committee, so the update is relevant all the same: it carries the next
committee of the store's own period (`validate_light_client_update`'s `update_has_next_sync_committee`;
`crates/ferrata-core/src/validate.rs`), and `apply_light_client_update` adopts that committee without moving
the finalized header. The store then follows the chain into the next period, as the old fixture's did.

**When this checkpoint leaves the period.** Slot 11,562,112 is in epoch 361,316; its period ends
at the start of epoch 364,849, slot 11,675,168, **2026-11-28T03:13:36Z**. An `Init` from
this checkpoint is valid only before then (each later transaction is bounded by the period of the store's
finalized header at that time, which the updates move forward).

| | |
|---|---|
| chain | Sepolia: chain id 11155111, `genesis_validators_root` 0xd8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078; Gloas since epoch 353,024 (version 0x90000076) |
| bootstrap (Gloas layout, 25,472 bytes) | slot 11,562,112 (2026-11-12T10:22:24Z), period 1411, block root 0x3b901f5e6853b5cf5eb04d7ccfe1272e66c38291cb3a11dd159cacd1117ccf7c (the second provider's, `beacon_header_11562112_second.json`) |
| its weak-subjectivity end (3,532 epochs) | slot 11,675,168, 2026-11-28T03:13:36Z |
| finality update (Gloas) | attested slot 11,562,176, signature slot 11,562,177, 499/512 signers; finalized slot 11,562,112, root 0x3b901f5e6853b5cf5eb04d7ccfe1272e66c38291cb3a11dd159cacd1117ccf7c |

| period | layout | fork digest | attested slot | signature slot | finalized slot | signers | relevant to the bootstrapped store as |
|---|---|---|---|---|---|---|---|
| 1411 | gloas | 0x669e6c11 | 11558976 | 11558977 | 11558912 | 509/512 | next committee of the store's period (finalized header older than the bootstrap) |

Cross-checks made by the script: the bootstrap is in the Gloas layout and its header hashes to the root the
second beacon provider (ethereum-sepolia-beacon-api.publicnode.com) reports as canonical at its slot; every
update is attested, signed and finalized in its own period, carries a next sync committee, and is relevant
to the bootstrapped store (last column); the second provider reports the root of every finalized header
named (`beacon_header_*_second.json`), the finality update's included. The bootstrap's committee branch and
the updates' signatures are verified by the
light client itself (`crown-driver check --chain sepolia`, the driver's tests, and the program's `Init`).

## Requests

| UTC | endpoint | request | file | note |
|---|---|---|---|---|
| 2026-11-12T10:40:23Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/finality_update` | finality_update.ssz | gloas |
| 2026-11-12T10:40:23Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11558912` |  |  |
| 2026-11-12T10:40:23Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/bootstrap/0xf54bd3575c05c7d83ee53dcef9bbfd98720b39e0be93aa9707a22dce8f2225d3` |  | HTTP 500: {"code":500,"message":"syncCommitteeWitness not available 0xf54bd3575c05c7d83ee53dcef9bbfd98720b39e0be93aa9707a22dce8f22 |
| 2026-11-12T10:40:23Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/bootstrap/0x3b901f5e6853b5cf5eb04d7ccfe1272e66c38291cb3a11dd159cacd1117ccf7c` |  | gloas |
| 2026-11-12T10:40:24Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11562112` |  |  |
| 2026-11-12T10:40:24Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/updates?start_period=1411&count=1` |  | gloas |
| 2026-11-12T10:40:24Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11558912` | beacon_header_11558912_second.json |  |
| 2026-11-12T10:40:24Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11562112` | beacon_header_11562112_second.json |  |
