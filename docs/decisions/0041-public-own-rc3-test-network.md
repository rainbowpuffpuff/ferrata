# ADR 0041: Run a public LEZ v0.4.0-rc3 test network with its own Logos Blockchain node while testnet 0.4 is absent

- **Date:** 24 November 2026
- **Status:** accepted; deployed on the test network
- **Requirements:** RFP-021 S.1; RFP-022 S.1, R.10

## Context

LEZ testnet 0.4 does not exist, so a deployment on it cannot yet be demonstrated and both RFPs' S.1 rows still wait on
Logos. The existing public LEZ testnet runs v0.3.1, where only the experimental light-client port is deployed; it does
not host Ferrata's v0.4 programs. Standalone zones had already rehearsed the day-one kit and the rc3 bridge, but they
were not a public always-on network. The decision was how to make the current v0.4 attestation and bridge available
for public testing without representing rc3 as the missing Logos testnet.

## Options considered

1. Wait for Logos to open LEZ testnet 0.4 before offering a public v0.4 deployment. *Recorded:*
   docs/requirements-status.md, RFP-021 and RFP-022 S.1, which retain "waits on Logos" even after the own-network
   deployment. The day-one kit remains ready for that network.
2. Treat the existing public LEZ v0.3.1 testnet as the deployment target for the v0.4 programs. *Recorded:*
   docs/EVIDENCE.md row 150 scopes that deployment to the experimental v0.3.1 port and expressly does not claim the
   proposal's v0.4 deliverable. docs/design/risc0-3.1-migration.md likewise leaves the v0.3.1 port unchanged.
3. Operate our own public, always-on LEZ v0.4.0-rc3 zone on one AWS host, anchored to its own Logos Blockchain
   0.3.5-rc.1 node, with one validator and dev-mode leader proofs, and deploy the current programs and bridge services
   there. Keep its status explicitly ours, not Logos's testnet 0.4.

## Decision

The third. `deploy/testnet/` defines the network as code: the rc3 sequencer and its 0.3.5-rc.1 node, an indexer and
explorer, an HTTPS allowlisted gateway, a faucet, the attestation updater and monitor, and the Sepolia bridge's watcher,
prover client, relayer and rule (B) attestor. The day-one kit deploys the attestation and B1 bridge programs. A RISC
Zero 3.1 Sepolia vault pairs with the zone. The network began operating on 23 November and its infrastructure entered
the repository in commit `0208be1a` on 24 November; its first full public round trip completed on 24 November.

## Rationale and trade-offs

Running the unmodified rc3 sequencer and the node release it pins makes public testing possible now and exercises the
actual v0.4 services rather than only an in-process harness. One host carries the whole zone and its supporting services,
so the infrastructure, program binaries and redeployment steps can be published and checked. The full round trip linked
Sepolia's deposit and release transactions to the zone's imported root, public burn and rule (B) checkpoint.

This remains a prototype test network: one validator, one sequencer, dev-mode leader proofs, resettable state and test
value only. It does not satisfy the clauses naming Logos's testnet 0.4, demonstrate independent operators, or remove
the receipt-verification dependency of LEZ rc3's RISC Zero 3.0.5. It also incurs continuing host operation, public
endpoint limits and bridge-service faults; the first round trip's finalisation was delayed by a relayer deployment fault,
which the committed replay and mount-layout fix address rather than conceal.

## Sources

- Commit `0208be1a`; deploy/testnet/README.md, "What runs", "The Sepolia bridge demo" and "Operating notes"
- Commit `05ddb446` and deploy/testnet/results/2026-11-24-relayer-exdev-replay.txt (the mount-layout fix; the replay)
- docs/requirements-status.md, RFP-021 S.1 and RFP-022 S.1 (the unavailable Logos testnet and retained status)
- docs/EVIDENCE.md row 150 (the separate v0.3.1 public-testnet deployment)
- docs/EVIDENCE.md rows 242, 248 and 250 (the public zone, verifier and full round trip)
- e2e/runs/live-roundtrip-2026-11-24/README.md (the end-to-end chain links and limits)
