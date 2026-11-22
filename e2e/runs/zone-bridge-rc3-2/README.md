# zone-bridge-rc3-2: the rc3 return leg again, with the attestor checking every tip against the sequencer's LEZ node

22 November 2026, the Modal build box: [`zone-bridge-rc3`](../zone-bridge-rc3/README.md)'s run again (rows 129 and
130) at the branch `fix-attestor`, whose hook ([`lez/live/hooks/return-leg.sh`](../../../lez/live/hooks/return-leg.sh))
now passes the sequencer's own RPC to the attestor (`--lez`): the attestor signs a tip only when LEZ v0.4.0-rc3's
`sequencer_service` serves it `Finalized` and byte for byte as inscribed, its last round is taken with the zone up, and
the node's answers the checkpoint is built from are read separately (from the first slot read to the LIB). One LEZ
v0.4.0-rc3 sequencer on a Logos Blockchain 0.3.5-rc.1 node (one validator, dev-mode leader proofs); the later stages by
`zone-bridge.sh` with `ZB_CHECKPOINT_B=checkpoint_b_0_3_5 ZB_N_MIN=1 ZB_T_MIN=1 ZB_LEAVE_OUT=none`; the zone's own
records in [`lez/results/v0.4.0-rc3/return-leg-2/`](../../../lez/results/v0.4.0-rc3/return-leg-2/README.md). STAND-IN:
the beacon chain on LEZ (the driver's committee), the Ethereum side (a local anvil, Sepolia's chain id; W and the
release delay warped). Results: [`checkpoint/attestor/RESULTS.md`](../../../checkpoint/attestor/RESULTS.md) §10.1.

The layout is `zone-bridge-rc3`'s: `identity.json`, `params.json`, `crown.json`, `deploy.json`, `manifest.json`,
`vault-checks.json` (the Ethereum side); `lez/` (crown-driver's sessions, the two captures); `zone/` (the fork digest,
the attestor's certificate, every round in `rounds.tsv`, the final attestation, the node's answers
`node-answers.tar.zst`); `checkpoint/` (`zone-24-106.input` and meta, the blocks and inscriptions checked, `burns.json`,
the collector's and executor's records, the deployed guest's refusal, `prove.json`, `receipts/`); `releases/`; `net/`
(the network journal of the proxy in front of the local anvil); `logs/` (the hook's log `zone-bridge.log`,
crown-driver's, the box steps').

Re-check: `ZB_PROGRAMS=<R0BF of docs/image-ids.md, with the two 0.3.5-rc.1 guests> ZB_CHECKPOINT_B=checkpoint_b_0_3_5
ZB_N_MIN=1 ZB_T_MIN=1 ZB_LEAVE_OUT=none bash checkpoint/attestor/zone-bridge.sh verify e2e/runs/zone-bridge-rc3-2`, and
`cd vault && forge test --match-contract ZoneBridgeRc3LezCheckedTest` (fixture `zone-bridge-rc3-2.json`).
