# e2e/seqnet: results

What ran, where and with what, for the user's private round trip on a running LEZ sequencer ([`README.md`](README.md)).
Every number below is from the run's own records under `e2e/runs/seqnet-NAME/` (each has a `SUMMARY.md`: every
transaction on both chains with who sent it, and what the user saw).

**Where:** the Modal build box (`spike/modal-build/mrun`, profile ws-5: gVisor, Ubuntu 24.04, up to 16 physical
cores), 13 November 2026. **Chains:** a LEZ v0.4.0-rc1 devnet started there (the sequencer and the Logos Blockchain
node of the crown-live-3 bundle, LEZ's local configuration, dev-mode leader proofs) with the final programs of
[`docs/image-ids.md`](../../docs/image-ids.md) (attestation `0x816d1fef…`, bridge `0xccde6171…`, read back from the
zone after their deployment), and a local anvil (chain id 11155111) with FerrataVault v5 in mode (B). Nothing was
sent to any public network.

## 1. Through the `ferrata` tool: `seqnet-cli-1` (dev mode)

[`e2e/runs/seqnet-cli-1/`](../runs/seqnet-cli-1/SUMMARY.md): `run.sh cli cli-1`, 13 Nov 09:05–09:12 UTC (session A
4.5 min; the round trip, from the deposit to the release, 3 min).

| Step | What the user ran | Result |
|---|---|---|
| deposit | `ferrata deposit --denomination 0` (dry run, then `--keystore … --send --yes`; private `/0` by default) | `depositETH` in anvil block 9, leaf 0, 0.001 ETH |
| import (operator) | — | `crown-b --operator-only`: bridge `Init` (LEZ block 23), `Register` ETH (24), `ImportRoot` n=1 (25) |
| fees (stand-in) | — | `seqnet-fund`: 3·10¹² native LEZ to public `/0` (block 26), then 2·10¹² to private `/0` by LEZ's wallet (block 27, private; the private account holds 1,999,994,929,384 after the 5,070,616 fee its proof paid) |
| claim held | `ferrata claim` | refused: `held by your claim delay: … from Ethereum block 609` (the seed's 2 h delay) |
| delay | — | 601 anvil blocks mined (the tool counts 12 s per block); not overridden |
| private claim | `ferrata claim --bridge-program … --send --yes` (defaults) | `ClaimPrivately` executed and proven by LEZ's wallet, applied in LEZ block 28 (transaction `148ad854…`), a privacy-preserving transaction whose visible bridge calls are `Read` and `Issue`; `recover` then shows the deposit **claimed** (from the nullifier account the private transaction named) and **private `/0` holding 0.001 ETH**, decrypted by LEZ's wallet after its sync |
| private burn | `ferrata burn --amount 0.001 --recipient 0xa015…1dAf` (a fresh address; private `/0` by default) | applied in LEZ block 29 (`a4ea74ad…`), private; its note reaches the zone in the clear as a `Burn` call to `token(cid, ETH)` (`lez::visible_calls`), from which `recover` finds the burn from the seed |
| checkpoint (prover, relayer) | — | `realblocks-crown`: the node's final inscriptions of LEZ blocks 1..=30 re-hashed, linked and signature-checked, all 30 accepted; the input over blocks 20..=29 (10 blocks, 30 transactions, 119,217 B); `checkpoint_b` (`0xf8116722…`) executed: journal (544 B) equal to the native one, 5,505,024 padded cycles; **stand-in**: the mock verifier accepts that journal; `ferrata-relayer`: `submitCandidate` (anvil block 612), Δ = 3600 s let pass on anvil's clock, `finalizeAttested` (614) |
| release held | `ferrata release` | refused: `ReleaseDelayNotElapsed()` (the vault's 120 s after `finalizeAttested`) |
| release | `ferrata release --seal … --job-out job.json` (any relayer, the registered relay fee) | the wallet's rule picked the root of checkpoint 0 (1 burn); **stand-in**: the mock accepts the release journal; `ferrata-relayer` sends `release` (anvil block 617): **0.000997 ETH to the fresh address** (0.001 less the checkpoint fee 0.000001 and the relay fee 0.000002, paid to the relayer, who collects its fees with `claimOwed`, 618) |
| recover | `ferrata recover` | `1 deposits … claimed`, `1 burn … released`, `0 holdings` |

On the zone: 97 transactions in blocks 1..=31, none reverted (63 of session A: the two programs' deployment and the
instance's `Init` and adoptions; the operator's 3; the stand-in's 2; the user's 2 private ones). On anvil: 15
transactions, none reverted (the vault's deployment 8, the user's deposit, the stand-in prover's 2 `accept`, the
relayer's `submitCandidate`, `finalizeAttested`, `release` and `claimOwed`).

Dev mode (`SEQNET_PROOFS=dev`): the sequencer was started with `RISC0_DEV_MODE=1` and LEZ's wallet made fake receipts
for the three private transactions (the shield, the claim, the burn).
