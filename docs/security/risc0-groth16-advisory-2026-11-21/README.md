# Security note, 21 November 2026: RISC Zero's Groth16 advisory and Ferrata

RISC Zero published two critical advisories on 11 November 2026:
[GHSA-g6qg-9j8p-7j87](https://github.com/risc0/risc0/security/advisories/GHSA-g6qg-9j8p-7j87) (risc0-zkvm) and
[GHSA-mm6c-35cm-g4x2](https://github.com/risc0/risc0-ethereum/security/advisories/GHSA-mm6c-35cm-g4x2) (the Ethereum
verifier contracts). Their words: the Groth16 verifier "accepts a Groth16 receipt whose claim digest has been
substituted with the digest of a different, attacker-chosen `ReceiptClaim`. An attacker who can produce one valid
Groth16 receipt can therefore have an arbitrary claim, including its image ID and journal, accepted". Every
`RiscZeroGroth16Verifier` of version 3.0.0 or earlier accepts the forged proofs; risc0-zkvm before 3.1.0,
risc0-circuit-recursion before 4.1.0 and risc0-groth16 before 3.0.6 are affected. The fix is risc0-zkvm 3.1.0 with a
new trusted setup and verifier 3.1.0 (selector `0xe71e6620`); receipts made by earlier versions do not verify under
3.1.0. RISC Zero emergency-stopped its own deployments of verifier 3.0.0 (`0x73c457ba`) and earlier, and writes:
"Applications that call a `RiscZeroGroth16Verifier` address directly must switch to the 3.1.0 verifier."

Everything in this repository that proves is built on risc0-zkvm 3.0.5, and every Groth16 seal it records carries the
selector `0x73c457ba`. Links to both advisories were in public material this project saved on 19 November 2026; the
project did not act on them until a code review raised them on 21 November 2026.

## Affected

**1. The four vaults on Sepolia.** Each verifies through its own `RiscZeroVerifierEmergencyStop` wrapped around our own
deployment of verifier 3.0.0 (`0x8f0Af3Bf302C64d57f840127F94d9c123a8Fa2A0`, `VERSION()` "3.0.0", `SELECTOR()`
`0x73c457ba`), not through RISC Zero's router, so RISC Zero's stop does not reach them. Read on 21 November 2026 at
Sepolia block 12,183,432 (18:00:48 UTC), read-only, on two public endpoints with identical answers
([`reads-publicnode.tsv`](reads-publicnode.tsv), [`reads-ethpandaops.tsv`](reads-ethpandaops.tsv), [`reads.sh`](reads.sh)):

| Vault (run) | Stop | Freeze authority, watchers | Caps | Holds (wei) | A forgery can take now |
|---|---|---|---|---|---|
| [`0x04eE…e44c`](https://sepolia.etherscan.io/address/0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c) (crown-live-3) | `0x1dB3…8221`, owner renounced, not paused | `0xC820…129f` (no key), none | none | 6,000,000,000,000 (4·10^12 escrow, 2·10^12 owed fees) | nothing: the escrow is below the smallest denomination, 10^15 |
| [`0xfE31…9E54`](https://sepolia.etherscan.io/address/0xfE3199fa2634413CbAC317371076Cae3EB2d9E54) (crown-live-1) | `0x2Ea6…6Ea9`, the same | the same | none | the same | nothing, the same |
| [`0x8055…3470`](https://sepolia.etherscan.io/address/0x80555Fe53CCcEf46951B23A3c75643a4C9283470) (crown-live-4) | `0xB249…EfCA`, the same | the same | none | the same | nothing, the same |
| [`0xd0B5…8AF7`](https://sepolia.etherscan.io/address/0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7) (v3, end-to-end fixture) | `0xe483…1139`, owner `0xC820…` (no key), not paused | `0xC820…`, none | none | 10^15 (deposit 0) | 0.001 Sepolia ETH, after a forged rule (A) checkpoint: a wait of at least 1,500 s, then a 120 s release delay |

No vault is frozen or retired; the test tokens' balances are 0. **What is at stake today: 0.001 Sepolia ETH, test
ether of the deployer's own.** Deposits are open on all four, so whatever anyone deposits from now on is exposed: a
forged release on a v5 vault needs no forged checkpoint, only the burn root it already accepted and a fresh nullifier
(the v3 vault has accepted none, so there a forged checkpoint comes first).

**What stops it.** No key-less control does: the freeze authority has no key holder, no stop has an owner with a key,
and no watcher is set. The deployer key, admin of all four vaults, does: `setGlobalCap(Release, 0, 0)` stops every
release on all four, and every refund on the three v5 vaults (the v3 vault's `refund` does not draw on the caps, but
it needs a retired vault, and only the keyless freeze authority can retire the v3 vault, so it never opens refunds),
and `setGlobalCap(Deposit, 0, 0)` stops every deposit; the admin can undo both. Estimated on Sepolia
(`eth_estimateGas`, nothing sent, [`estimate-publicnode.tsv`](estimate-publicnode.tsv)): 66,941 and 63,396 gas per v5
vault, 59,005 and 58,982 for the v3 vault, 508,998 in all, about 10^-11 Sepolia ETH at the base fee of 16 wei of block
12,183,475, where the estimates ran. Slower: on the v5 vaults the admin can install a freeze authority and a watcher
with `proposeRoles`, applied 14 days later (369,833 gas each); the v3 vault has no such path. A stop can also be
tripped by anyone holding a receipt the verifier accepts for the all-zero claim digest (`estop(receipt)`); whether the
forgery reaches that digest is not established, and none was built.

**The emergency stop did not trip.** On 21 November 2026 our trust model ([`trust.md`](../../trust.md) §5 and §6,
[`bridge.md`](../../design/bridge.md) §15, Security #9 in [`traceability.md`](../../traceability.md)) named the stop
as the answer to an unsound proof system: whoever holds the proof of unsoundness trips it, and nothing more is
released. This advisory is that case, and none of the four stops tripped (`paused()` false on all four at block
12,183,432). The advisories include no proof of exploit. The forgery makes the verifier accept a chosen claim, not
every seal, so the model of [`vault/README.md`](../../../vault/README.md) ("Under a broken verifier", where a junk
receipt trips the stop) does not describe it, and whether it reaches the all-zero claim digest is not established
(above). No owner with a key can call `estop()`: the v5 vaults' stops have renounced owners, and the v3 vault's stop
is owned by the keyless `0xC820…`. On these four vaults only the admin's caps stop a forgery; decision D1 of the
[migration plan](../../design/risc0-3.1-migration.md) §4 changes the wiring for new vaults.

**Rehearsed on a local fork** ([`fork-rehearsal.txt`](fork-rehearsal.txt), [`fork-rehearsal.sh`](fork-rehearsal.sh)):
anvil, forked from Sepolia at the same block, nothing sent to Sepolia; the forgery stood in for by replacing the
verifier's code on the fork with one STOP opcode, which accepts every seal as the advisory says a forged proof is
accepted. On `0x04eE…e44c` as deployed: two 0.001 ETH deposits taken by a stranger, one by a release against the
vault's existing burn root, one against a forged checkpoint finalised after its 3,600 s wait. After the admin's two
calls: the same releases revert `GlobalReleaseCapExceeded`, a deposit `GlobalDepositCapExceeded`, the calls from any
other address `NotAdmin`; the four vaults accept both calls. Through the timelock: a new freeze authority freezes
releases and checkpoints and retires the vault, until the freeze lapses after 30 days unless renewed.

**Status, 21 November 2026:** the owner has not sent the cap transactions; the caps are unlimited, as read at block
12,183,432. This line changes when the caps read zero on chain.

**2. Our proofs as evidence.** Our provers made every seal and receipt in this tree from the recorded inputs, but a
3.0.x verifier accepting one no longer shows anyone else that its claim is true. EVIDENCE row 149 (our seals accepted
by RISC Zero's mainnet verifier 3.0.0, 18 November) shows that our seals are well-formed for the 3.0.0 circuits, no
more; the seal checks of `scripts/verify-sepolia.py` ([verify.md](../../guides/verify.md)) have the same limit. The
proofs are made again under 3.1.0 in the migration.

**3. LEZ.** LEZ v0.4.0-rc1, rc3 and v0.3.1 (the public LEZ testnet) lock risc0-zkvm 3.0.5 and verify the receipts of
privacy-preserving transactions with it; our private claim, burn and attestation are such transactions. Whether LEZ
accepts a forged one is for Logos to confirm and fix. LEZ `dev` already moves to risc0 3.1.0 (`6aaabe5`), so a LEZ
built from it refuses our 3.0.5 receipts until our guests are rebuilt.

## Not affected

- Ethereum mainnet: nothing of ours is deployed there. No one's funds: the vaults hold the deployer's Sepolia ether.
- LEZ public transactions, which the sequencer executes rather than verifies by proof (the light client's updates and
  records on its public path, the public LEZ testnet instance of EVIDENCE row 150), and the native light-client core.

## Plan

1. Now, the owner: the two cap calls on each of the four vaults (below). Retire the v5 vaults for good when keyless
   retirement opens (`retireOpensAt`: 11, 12 and 18 December 2026, if no checkpoint is submitted before), or through a
   freeze authority after the timelock. They will not be used again.
2. Done here on 21 November 2026: this note; the README, EVIDENCE row 149, [docs/sepolia.md](../../sepolia.md),
   [verify.md](../../guides/verify.md), the mainnet run's README, [lez-v0.4-findings.md](../../design/lez-v0.4-findings.md),
   [SECURITY.md](../../../SECURITY.md) and [e2e/README.md](../../../e2e/README.md) corrected.
3. The move to risc0 3.1.0 and verifier 3.1.0, with new vaults: [risc0-3.1-migration.md](../../design/risc0-3.1-migration.md).
4. LEZ: wait for a LEZ tag on risc0 3.1.0 (Logos).

```sh
# The owner runs this with the deployer key (0x79A9…5C07); nothing here has been sent.
export ETH_RPC_URL=https://ethereum-sepolia-rpc.publicnode.com
export ETH_KEYSTORE=/path/to/deployer-keystore      # cast prompts for its password, or add --password-file FILE
# --chain 11155111: with a wrong ETH_RPC_URL the node refuses the transaction rather than running it on another chain
for V in 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c 0xfE3199fa2634413CbAC317371076Cae3EB2d9E54 \
         0x80555Fe53CCcEf46951B23A3c75643a4C9283470 0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7; do
  cast send --chain 11155111 "$V" "setGlobalCap(uint8,uint256,uint256)" 1 0 0   # no release (and no refund on the v5 vaults)
  cast send --chain 11155111 "$V" "setGlobalCap(uint8,uint256,uint256)" 0 0 0   # no deposit
done
for V in 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c 0xfE3199fa2634413CbAC317371076Cae3EB2d9E54 \
         0x80555Fe53CCcEf46951B23A3c75643a4C9283470 0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7; do
  cast call "$V" "globalCap(uint8)(uint256,uint256,uint256)" 1; cast call "$V" "globalCap(uint8)(uint256,uint256,uint256)" 0
done                                                           # expect 0, 0, 0 for each
```

Re-check, sending nothing: `docs/security/risc0-groth16-advisory-2026-11-21/reads.sh URL BLOCK OUT` and
`fork-rehearsal.sh URL BLOCK OUTDIR` (Foundry's `cast` and `anvil`). Both need an endpoint that serves historical state
at that block (an archive node, e.g. `https://rpc.sepolia.ethpandaops.io`); publicnode refuses reads at past blocks
(8 of 205 on a re-run of 21 November 2026), and `reads.sh` then exits 1. `estimate.sh URL OUT` estimates at the
latest block.
