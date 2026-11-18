# The local round: LEZ v0.3.1 standalone sequencer and anvil, real proofs

Run by `local.sh` on the Modal build box (18 Nov 2026, 10:54-11:11Z; CPU request 2, limit 16 physical cores; 64 GiB): a LEZ v0.3.1
standalone sequencer (tag v0.3.1, `1a003bf`, its debug configuration without the gossip listener and with one more
genesis supply account, the run's payer; Bedrock mocked; proofs verified: `RISC0_DEV_MODE` unset), a plain local
anvil (chain 31337), FerrataVault v5 deployed there by `vault/script/deploy.sh` (mode B, RISC Zero's Groth16
verifier behind the emergency stop; vault `0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9`), two deposits, and the bridge driven by `testnet.sh run`
through LEZ v0.3.1's own wallet. The light client is the **stand-in** (`guests/records`): it wrote the record of anvil
block 10 that `Init` and `ImportRoot` read. Files: `local/` (`run.json` without keys, `state.json`, the capture,
the vault's record, the deposits' receipts, GNU time of the two proofs, `verify.py`'s output).

Bridge `CQy8Ng4kqejocniJQ4UuBibjuZ5AnbacxbPEtHvPTZeK` (image id `eaab4152…4bb4`), configuration `f1059265e28569fe39749b63ee979e987af59cc6ea07d0e8f9536fd555fa7947` = the vault's slot 2.

| block | transaction | bytes | execution gas | |
|---:|---|---:|---:|---|
| 13 | deploy the bridge (program_loader: 5 segments and the header) | 0 | storage only |  |
| 21 | deploy the STAND-IN light client (3 segments and the header) | 0 | storage only |  |
| 28 | the stand-in writes its record of anvil block 10 | 555 | 21,596 | fee 177,208 |
| 30 | `Init`: the vault's slot 2 = `configId` under the record | 2,840 | 158,755 | fee 1,292,760 |
| 32 | `ImportRoot` n = 2 | 3,792 | 221,214 | fee 1,800,048 |
| 34 | `Claim` of deposit 0 (public kind) to the public holder | 1,936 | 151,575 | fee 1,228,088 |
| 36 | the same claim again (refused: 250 `ALREADY_CLAIMED`) | 1,936 | 141,607 | fee 1,148,344 |
| 38 | deposit 0 to another destination (refused: 249 `CLAIM_INVALID`) | 1,936 | 99,636 | fee 812,576 |
| 51 | `ClaimPrivately` of deposit 1 (private kind): privacy-preserving, **real proof** | 247,826 | exempt | proven and sent in 169.5 s |
| 53 | `Burn` 0.001 ETH, the public holder (signed) | 976 | 277,497 | fee 2,227,784 |
| 66 | `Burn` 0.001 ETH, the private holder: privacy-preserving, **real proof** | 248,346 | exempt | proven and sent in 157.8 s |

Execution gas is derived from the fee the payer was charged (its balance before and after): fee = 8 x execution gas +
8 x bytes at this chain's base fees (8 and 8, the floor). Deployment's fee was not read here (deployment charges storage gas only: testnet-ops, `spike/tmp/testnet-ops/FOR-SIBLINGS.md`). Privacy-preserving
transactions are fee-exempt on v0.3.1. **The two real proofs** (LEZ's wallet, `execute_and_prove` on the box's CPU,
r0vm 3.0.5): the private claim 169.5 s to prove and send, 247,826 bytes, peak memory
9.26 GiB (GNU time of the whole command, 3:19.60 including the wait for its block); the private burn
157.8 s, 248,346 bytes, 9.25 GiB (3:07.82). The sequencer verified both receipts.

**Read back over RPC** (`verify.py --any-status`, run while the chains were up: `local/verify.txt`): 24 checks, 0 fail:
every transaction in the block recorded; every account the run touched holds exactly the recorded bytes: the
configuration, `root(cid, 2)`, both nullifiers, the public holder (empty: claimed one denomination, burned it), both
burn notes, the burn tree (`burns(cid)`: 2 leaves, root `9e304b8e793a90b8efa8f34b082b553fc385993b9bf8cfce0417a80d18d4e8b2`, which
`bridge03 burns` recomputes from the two notes), the stand-in's record; on anvil, the vault's slots 0, 1 and 2 at the
captured block and both deposits' receipts. A v0.3.1 node keeps no per-transaction status: the two refusals show as
transactions included in blocks 36 and 38, charged, with no effect (the public holder was credited once).

Re-check: `./local.sh <run dir>` runs it all again (fresh chains, so new hashes); the record's own bytes are compared
by `verify.py` while its chains run.
