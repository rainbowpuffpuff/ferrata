# What forging a light-client header puts at stake on Ethereum (18 November 2026)

The numbers behind [`../trust.md`](../trust.md) §2.2, "What a forger loses". Measured on mainnet from a public beacon
node's standard API, read-only, from a Modal build box (User-Agent `ferrata-updater`, nothing that identifies the
caller); derived with the specifications' own formulas. Nothing here is a Ferrata component: it is the cost side of
the light client's trust assumption.

## Measured

`python3 -I measure.py mainnet-lighthouse-publicnode.json --beacon https://ethereum-beacon-api.publicnode.com`, 16:24:56Z,
against `Lighthouse/v8.3.3-57c2abc/x86_64-linux` at slot 15,682,922 (epoch 490,091, sync-committee period 1914):
[`mainnet-lighthouse-publicnode.json`](mainnet-lighthouse-publicnode.json).

| | current committee (period 1914) | next committee (period 1915) |
|---|---:|---:|
| seats; distinct members (no member holds two seats) | 512; 512 | 512; 512 |
| members of 32 ETH / 33–1,023 / 1,024–2,047 / 2,048 ETH effective balance | 276 / 32 / 161 / 43 | 289 / 25 / 156 / 42 |
| effective balance of all members | 409,755 ETH | 390,013 ETH |
| the cheapest 342 seats (members by effective balance per seat) | 76,714 ETH | 58,397 ETH |
| members exiting / exited / withdrawable; slashed | 4 / 2 / 1; 0 | 5 / 1 / 0; 0 |

The active stake: 44,343,611 ETH (`/lighthouse/validator_inclusion/490090/global`, `current_epoch_active_gwei`). Check:
the sync-committee reward per seat in the block measured, 25,627 Gwei (`POST /eth/v1/beacon/rewards/sync_committee`, a
query), maps through Altair's `process_sync_aggregate` to two intervals of active stake, 44,197,134–44,198,858 and
44,342,999–44,344,728 ETH (the reward is not monotone in the stake: the base reward per increment is floored); the
second holds Lighthouse's count.

Failed: the same measurement against `https://lodestar-mainnet.chainsafe.io` (`Lodestar/v1.51.0/7923280`) was refused
with HTTP 429 (too many requests) at the committee's validator lookups
([`mainnet-lodestar-chainsafe.json`](mainnet-lodestar-chainsafe.json)); it was not repeated.

## Derived

| | how | value |
|---|---|---:|
| seats a forgery needs | `sum(bits) * 3 >= 512 * 2` (Ferrata's step 1 refuses below it) | 342 (342 × 3 = 1,026 ≥ 1,024; 341 × 3 = 1,023) |
| nominal stake of 342 seats | 342 × 32 ETH | 10,944 ETH |
| EIP-7657's own threshold | 512 × 32 ETH (its Motivation) | 16,384 ETH |
| two thirds / one third of the active stake | 44,343,611 × 2/3, × 1/3 | 29,562,407 / 14,781,204 ETH |
| loss if the cheapest 342 seats were slashed together (EIP-7657, not live) | Electra: `effective_balance // 4096` at once, plus `effective_balance × min(3 × slashed, total) / total` | 18.7 + 398.1 ≈ 417 ETH |
| ... all 512 together | the same | 100.0 + 11,359.0 ≈ 11,459 ETH |
| ... a third of the stake (conflicting FFG finality) | the same; `3 × slashed ≥ total`, so the whole balance | ≥ 14,781,204 ETH |

## Sources (fetched read-only on 18 November 2026)

- Consensus specifications, release `v1.6.1` (the latest release GitHub names; commit
  `5fa6edcca8ab4cf548653e6680b17b9d3e04d225`), under `https://github.com/ethereum/consensus-specs/blob/v1.6.1/specs/`:
  `phase0/beacon-chain.md` (`is_slashable_attestation_data`: double vote, surround vote; `process_proposer_slashing`,
  `process_attester_slashing`, `slash_validator`); `altair/beacon-chain.md` (`BeaconBlockBody`: `proposer_slashings`,
  `attester_slashings`, `sync_aggregate`, no sync-committee slashing; `process_sync_aggregate`: the participant reward
  and the penalty for a missing signature, nothing else); `altair/light-client/sync-protocol.md` (`is_better_update`,
  `process_light_client_update`: `sum(sync_committee_bits) * 3 >= len(sync_committee_bits) * 2`);
  `bellatrix/beacon-chain.md` (`PROPORTIONAL_SLASHING_MULTIPLIER_BELLATRIX = 3`); `electra/beacon-chain.md`
  (`MIN_SLASHING_PENALTY_QUOTIENT_ELECTRA = 4096`, `process_slashings` per increment, `get_next_sync_committee_indices`
  drawing in proportion to effective balance up to `MAX_EFFECTIVE_BALANCE_ELECTRA`); `gloas/beacon-chain.md` (its
  `BeaconBlockBody` still has only `proposer_slashings` and `attester_slashings`).
- EIP-7657, "Sync committee slashings" (`https://eips.ethereum.org/EIPS/eip-7657`; the source
  `https://github.com/ethereum/EIPs/blob/master/EIPS/eip-7657.md`, last changed by commit `35cf1b76cb`, 15 January 2026):
  `status: Stagnant`, `created: 2024-03-21`, Standards Track, Core; Test Cases, Reference Implementation and Security
  Considerations "TBD"; it would `slash_validator` each signer of two conflicting sync-committee messages.
- Accountable safety (a third of the stake slashable when two conflicting checkpoints are finalized): V. Buterin,
  V. Griffith, "Casper the Friendly Finality Gadget", arXiv:1710.09437; the two conditions are the specifications'
  `is_slashable_attestation_data`.

## Re-check

`python3 -I docs/forgery-cost/measure.py OUT.json --beacon URL` against any mainnet beacon node (the active stake
directly only from a Lighthouse node; from any node through the sync reward). The numbers move with every period
(27.3 h) and with the stake.
