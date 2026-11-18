# zone-bridge-rc3: the return leg over a zone on Logos Blockchain 0.3.5-rc.1 (LEZ v0.4.0-rc3)

The run of [`lez/results/v0.4.0-rc3/return-leg/`](../../../lez/results/v0.4.0-rc3/return-leg/README.md) (what ran,
where, what is real and what stands in, the re-checks), in the layout of
[`checkpoint/attestor/zone-bridge.sh`](../../../checkpoint/attestor/zone-bridge.sh)'s runs: one LEZ v0.4.0-rc3
sequencer on a 0.3.5-rc.1 node (one validator, dev-mode leader proofs), its zone stage made by
[`lez/live/hooks/return-leg.sh`](../../../lez/live/hooks/return-leg.sh), the later stages by `zone-bridge.sh` with
`ZB_CHECKPOINT_B=checkpoint_b_0_3_5 ZB_N_MIN=1 ZB_T_MIN=1 ZB_LEAVE_OUT=none`. STAND-IN: the beacon chain on LEZ (the
driver's committee), the Ethereum side (a local anvil, Sepolia's chain id; W and the release delay warped).

- `identity.json`, `params.json` (the vault's ids: `checkpoint_b_0_3_5`, `release`), `crown.json` (the deposits and,
  after the vault stage, the candidate and the releases), `deploy.json`, `manifest.json`: the Ethereum side.
- `lez/session-a/`, `lez/session-b/`: crown-driver's sessions (the private claim's and burn's proven transactions,
  the burn notes), `lez/inputs/`: the two captures.
- `zone/`: the fork digest (`chain-fork.json`), the attestor's certificate, rounds and final attestation, the node's
  answers it checked (`node-answers.tar.zst`: the checkpoint's input is built from these alone).
- `checkpoint/`: `realblocks-crown`'s input `zone-24-125.input` and meta, the blocks and inscriptions it checked,
  `burns.json`, the collector's and the executor's records (`exec.json`), the deployed guest's refusal.
- `logs/`: the hook's log (`zone-bridge.log`), crown-driver's, the box steps'.
