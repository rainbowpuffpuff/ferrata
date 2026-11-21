# The local round: LEZ v0.3.1 standalone sequencer and anvil, real proofs

Run by `local.sh` on the Modal build box (18 Nov 2026; CPU request 2, limit 16 physical cores; 64 GiB), the program
after review-lez-v03-bridge's fixes (image id `d75d06ee1b7d2d5af561f2f6547463d0219590466e8afa51512a68696e137e10`): a LEZ
v0.3.1 standalone sequencer (tag v0.3.1, `1a003bf`; its debug configuration without the gossip listener and with one
more genesis supply account, the run's payer; Bedrock mocked; proofs verified, `RISC0_DEV_MODE` unset), a plain local
anvil (chain 31337), FerrataVault v5 deployed there by `vault/script/deploy.sh` (mode B, RISC Zero's Groth16 verifier
behind the emergency stop; vault `0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9`), two deposits, and the bridge driven by `testnet.sh run` through LEZ
v0.3.1's own wallet. The light client is the **stand-in** (`guests/records`): it wrote the record of the captured anvil
block that `Init` and `ImportRoot` read. Files: `local/` (`run.json` without keys, `state.json`, the capture, the
vault's record, the deposits' receipts, GNU time of the two proofs, `verify.py`'s output).

Bridge `6bqUQFL6dGtHfxGY97jtjFnXDxLeZZf7QMsB14EuVcim`, configuration `00e18245791caf3994730c95a59184600d3255c833596f4cf036d0c3f0187261` = the vault's slot 2. **19 LEZ transactions**: the two
deployments are 6 and 4 transactions (segments and header), then 9 others.

| block | transaction | bytes | execution gas | |
|---:|---|---:|---:|---|
| 13 | deploy the bridge (program_loader: 5 segments and the header: 6 transactions) | 0 | not read |  |
| 21 | deploy the STAND-IN light client (3 segments and the header: 4 transactions) | 0 | not read |  |
| 27 | the stand-in writes its record of the captured anvil block | 555 | 21,596 | fee 177,208 |
| 29 | `Init`: the vault's slot 2 = `configId` under the record | 2,839 | 158,848 | fee 1,293,496 |
| 31 | `ImportRoot` n = 2 | 3,792 | 221,270 | fee 1,800,496 |
| 33 | `Claim` of deposit 0 (public kind) to the public holder | 1,936 | 152,226 | fee 1,233,296 |
| 35 | the same claim again (a refusal: 250 `ALREADY_CLAIMED`, inferred) | 1,936 | 142,219 | fee 1,153,240 |
| 37 | deposit 0 to another destination (a refusal: 249 `CLAIM_INVALID`, inferred) | 1,936 | 99,634 | fee 812,560 |
| 51 | `ClaimPrivately` of deposit 1 (private kind): privacy-preserving, **real proof** | 247,826 | exempt | proven and sent in 178.0 s |
| 53 | `Burn` 0.001 ETH, the public holder (signed) | 976 | 277,684 | fee 2,229,280 |
| 66 | `Burn` 0.001 ETH, the private holder: privacy-preserving, **real proof** | 248,346 | exempt | proven and sent in 168.2 s |

Execution gas is derived from the fee the payer was charged (its balance before and after): fee = 8 x execution gas +
8 x bytes at this chain's base fees (8 and 8, the floor); a deployment's fee was not read (it charges storage gas only:
testnet-ops's notes, outside the repository). Privacy-preserving transactions are fee-exempt on v0.3.1.
**The two real proofs** (LEZ's wallet, `execute_and_prove` on the box's CPU, r0vm 3.0.5): the private claim
178.0 s to prove and send, 247,826 bytes, peak memory 9.26 GiB (GNU time of the whole command,
3:28.10 with the wait for its block); the private burn 168.2 s, 248,346 bytes, 9.26 GiB (3:18.29).
The sequencer verified both receipts (a dev-mode private transaction is 3,781 bytes: `synthetic.md`).

**Read back** (`verify.py --any-status`, while the chains ran: `local/verify.txt`): 24 checks, 0 fail.
9 check inclusion only (a v0.3.1 node keeps no per-transaction status: a refusal is an included transaction that
changed nothing); 10 compare every account the run touched, byte for byte, with the `state.json` the same run wrote
from the same node shortly before (the configuration, `root(cid, 2)`, both nullifiers, the public holder, both burn
notes, the burn tree, the stand-in's record): a consistency check; 5 are EVM checks (the vault's slots 0, 1 and 2 at
the captured block, both deposits' receipts). The two refusals' codes are inferred, not observed: they were included
and charged, and the state shows no second credit (the public holder was credited once and burned once: it holds
nothing). The burn tree at `burns(cid)`: 2 leaves, root `8dbe4d560291db01279fa3876445ebe73ea1fa94b83af284b90bb73b6f9a78c3`, which
`bridge03 burns` recomputes from the two notes.

Re-check: `./local.sh <run dir>` runs it all again (fresh chains, so new hashes and ids).
