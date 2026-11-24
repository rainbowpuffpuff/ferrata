# Sepolia: a Gloas bootstrap of period 1421 (slot 11,640,832) and the period updates from it

Fetched by `scripts/fetch-sepolia-checkpoint.py` between 2026-11-24T03:08:23Z and 2026-11-24T03:08:25Z (UTC). Every request went
to a public endpoint without an API key, with User-Agent `curl/8`. Responses are stored byte for byte,
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

**When this checkpoint leaves the period.** Slot 11,640,832 is in epoch 363,776; its period ends
at the start of epoch 367,309, slot 11,753,888, **2026-12-09T01:37:36Z**. An `Init` from
this checkpoint is valid only before then (each later transaction is bounded by the period of the store's
finalized header at that time, which the updates move forward).

| | |
|---|---|
| chain | Sepolia: chain id 11155111, `genesis_validators_root` 0xd8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078; Gloas since epoch 353,024 (version 0x90000076) |
| bootstrap (Gloas layout, 25,472 bytes) | slot 11,640,832 (2026-11-23T08:46:24Z), period 1421, block root 0x49cd05dd2796f3e0539e53797dfd2a9f76c4887b5124eb9bbbf2b7fe8b3efaec (the second provider's, `beacon_header_11640832_second.json`) |
| its weak-subjectivity end (3,532 epochs) | slot 11,753,888, 2026-12-09T01:37:36Z |
| finality update (Gloas) | attested slot 11,646,336, signature slot 11,646,337, 486/512 signers; finalized slot 11,646,272, root 0xe21bcae194ebafef59e7ccabfde987483f24fc400fd34e1defb711d9c808ce6c |

| period | layout | fork digest | attested slot | signature slot | finalized slot | signers | relevant to the bootstrapped store as |
|---|---|---|---|---|---|---|---|
| 1421 | gloas | 0x669e6c11 | 11640896 | 11640897 | 11640832 | 486/512 | newer than the bootstrap |

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
| 2026-11-24T03:08:23Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/finality_update` | finality_update.ssz | gloas |
| 2026-11-24T03:08:23Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11640832` |  |  |
| 2026-11-24T03:08:23Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/bootstrap/0x49cd05dd2796f3e0539e53797dfd2a9f76c4887b5124eb9bbbf2b7fe8b3efaec` |  | gloas |
| 2026-11-24T03:08:24Z | https://lodestar-sepolia.chainsafe.io | `GET /eth/v1/beacon/light_client/updates?start_period=1421&count=1` |  | gloas |
| 2026-11-24T03:08:24Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11640832` | beacon_header_11640832_second.json |  |
| 2026-11-24T03:08:24Z | https://ethereum-sepolia-beacon-api.publicnode.com | `GET /eth/v1/beacon/headers/11646272` | beacon_header_11646272_second.json |  |
