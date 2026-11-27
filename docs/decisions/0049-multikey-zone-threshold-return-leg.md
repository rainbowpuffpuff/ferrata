# ADR 0049: Grow the test zone to several sequencer keys and require t-of-n attestations, without claiming independent operators

- **Date:** 26 November 2026
- **Status:** accepted; built and run on local four-sequencer zones; the test-network key-growth runbook not executed
- **Requirements:** RFP-022 F.7, R.10

## Context

This record was written on 27 November 2026 from the commits, after the fact. Commit `765d0b4a` took the deployment
decision; commits `51320732` and `ea1722cc` built and recorded the four-key local return leg.

The test network began with one accredited sequencer key and 1-of-1 rule (B) checkpoints. That demonstrated the
checkpoint guest and vault path, but not threshold support over a running multi-sequencer zone. The question was
whether to retain that simple deployment or change its key set and services while keeping the same checkpoint and
vault programs.

## Options considered

1. Keep the public zone at one key and leave threshold coverage to standalone tests. *Recorded:*
   deploy/testnet/MULTIKEY.md records both that starting state and the existing local four-key run. It avoids a key-set
   change on the public zone, but its live return leg remains 1 of 1.
2. Use only a fresh zone with several keys already in genesis. *Recorded:*
   e2e/runs/zone-bridge-rc3-multi/README.md records such a local zone. It proves threshold checkpoints, but does not
   exercise staking more keys and changing accreditation on a zone already running.
3. Stake additional sequencer keys, wait for the committee's configuration change, run one sequencer and attestor per
   key, and collect t-of-n attestations of one tip for every later checkpoint.

## Decision

The third is the test-network deployment policy. The owner's runbook plans the new keys and staged configuration,
stakes them, waits for accreditation and only then starts the additional sequencer and attestor units. The checkpoint
service gathers t(n) signatures of one tip from the attestor directories; the existing checkpoint journal carries n
and t and the vault accepts values above its configured minimum floors.

The local implementation ran four LEZ v0.4.0-rc3 sequencers with 3-of-4 checkpoints and real RISC Zero 3.1 proofs,
including checkpoint refusal cases and releases through the vault verifier. The procedure for growing the public test
network's existing one-key zone is recorded but expressly not executed. Returning to one key would require another
configuration change signed by the new committee.

## Rationale and trade-offs

Growing the key set exercises the threshold mechanism without replacing the immutable checkpoint guest or vault. It
also separates a successful threshold proof from a claim of independent finality: all keys and services in this design
remain with one operator, so the design demonstrates t-of-n mechanics, not t independent parties.

The costs are liveness and operational exposure. A stopped key loses its posting turn and delays block production;
the runbook records an unresolved rc3 stall at a turn boundary and a procedure to stop the additional sequencers if
the tip stops progressing. Staking, rotating and later reducing the set each need the committee's own signed change.
The local genesis-based run does not prove that the unexecuted live key-growth steps will avoid those failures.

## Sources

- Commit `765d0b4a`; deploy/testnet/MULTIKEY.md (the chosen runbook, one-operator limit and unexecuted status)
- `deploy/testnet/host/bin/ferrata-zone-keys` and deploy/testnet/host/bin/ferrata-bridge-checkpoint (key growth and
  threshold collection)
- Commits `51320732` and `ea1722cc`; checkpoint/attestor/zone-bridge.sh (the four-sequencer run)
- e2e/runs/zone-bridge-rc3-multi/README.md and `vault/test/real/ZoneBridgeRc3Multi.t.sol` (3-of-4 records and real
  verifier tests)
- checkpoint/attestor/RESULTS.md §11 and docs/EVIDENCE.md row 266 (the run and its scope)
