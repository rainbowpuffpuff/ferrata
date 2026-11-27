# Security note, 21 November 2026: RISC Zero's Groth16 advisory and Ferrata

> **Corrected 23 November 2026**: the light client's public path is not independent of LEZ's check of
> privacy-preserving transactions, as "Not affected" below said; see [the correction](#correction-23-november-2026).
> The rest of the note stands as written on 21 November 2026, except that its "no live exposure" sentence stopped
> holding later on 23 November 2026, when the project's public test network started running the v0.4 programs: see
> "Corrected again" in [the correction](#correction-23-november-2026).

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

No vault is frozen or retired; the test tokens' balances are 0. **What was at stake at that block: 0.001 Sepolia ETH,
test ether of the deployer's own.** Deposits were open on all four until the owner's caps (Status, below), so whatever
anyone deposited was exposed: a forged release on a v5 vault needs no forged checkpoint, only the burn root it already
accepted and a fresh nullifier (the v3 vault has accepted none, so there a forged checkpoint comes first).

**What stops it.** No key-less control does: the freeze authority has no key holder, no stop has an owner with a key,
and no watcher is set. The deployer key, admin of all four vaults, does: `setGlobalCap(Release, 0, 0)` stops every
release on all four, and every refund on the three v5 vaults (the v3 vault's `refund` does not draw on the caps, but
it needs a retired vault, and only the keyless freeze authority can retire the v3 vault, so it never opens refunds),
and `setGlobalCap(Deposit, 0, 0)` stops every deposit; the admin can undo both (sent on 21 November 2026: Status,
below). Estimated on Sepolia
(`eth_estimateGas`, nothing sent, [`estimate-publicnode.tsv`](estimate-publicnode.tsv)): 66,941 and 63,396 gas per v5
vault, 59,005 and 58,982 for the v3 vault, 508,998 in all, about 10^-11 Sepolia ETH at the base fee of 16 wei of block
12,183,475, where the estimates ran. Slower: on the v5 vaults the admin can install a freeze authority and a watcher
with `proposeRoles`, applied 14 days later (369,833 gas each); the v3 vault has no such path. A stop can also be
tripped by anyone holding a receipt the verifier accepts for the all-zero claim digest (`estop(receipt)`); whether the
forgery reaches that digest is not established, and none was built. None is public: RISC Zero stopped its own 3.0.0
deployments with their owner's key, not with a receipt (`estop()` on Sepolia at block 12,171,230, 19 November 2026
22:11:12 UTC, transaction `0x6a1ca0f4c05d24f9a47331449989c4a5da04fb9a8e4cb7ecb86e0b3718e33286`; on mainnet at block
26,452,086, 22:21:11 UTC, `0xe3085eef2649076f40175f54e91e10f9390b565350971b8d0de52664ea6df31c`; no transaction sent to
either stop calls `estop(receipt)`). If one is ever published, anyone can trip our four stops with it: our verifier has
RISC Zero's 3.0.0 `SELECTOR()`, control root and BN254 control id, so it accepts the same receipts. A tripped stop
closes every release, checkpoint and refund of its vault for good, a forgery's included, but not its deposits, and the
escrow stays locked; the deposit cap is needed either way.

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

**Status, 21 November 2026: closed.** The owner sent the eight cap transactions from the deployer, release then deposit
on each vault (nonces 49–56, Sepolia blocks 12,184,549 to 12,184,573, 21:52:00 to 21:57:00 UTC, all status 1, 402,530
gas in all; listed under Plan and in [`caps-sent.tsv`](caps-sent.tsv)). Read back at block 12,184,587 (21:59:48 UTC,
the finalized head at the read) on both endpoints, with identical answers
([`reads-capped-publicnode.tsv`](reads-capped-publicnode.tsv),
[`reads-capped-ethpandaops.tsv`](reads-capped-ethpandaops.tsv)): `globalCap(Release)` and `globalCap(Deposit)` are
0, 0, 0 (capacity, refill per second, available) on all four vaults, and every other read is as at block 12,183,432
but the admin's balance, lower by the eight transactions' fees. No release or deposit goes through on the four vaults,
nor a refund on the v5 vaults, until the admin raises a cap. Caps do not gate checkpoints: on the fork, after the two
calls, a forged checkpoint was still submitted and finalised on `0x04eE…e44c` (part B), but nothing could be released
against it.

**Status, 23 November 2026: superseded by the RISC Zero 3.1 vault.** The owner deployed a new vault on Sepolia,
`0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787` (block 12,196,188), on RISC Zero's `RiscZeroGroth16Verifier` 3.1.0 behind RISC Zero's emergency stop, which
the advisories' fixed releases make (EVIDENCE rows 231, 232). The four vaults above stay capped at zero and are
retired as the plan says; they are not used again.

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
  **Qualified 23 November 2026** ([correction](#correction-23-november-2026)): the public transactions are not checked
  by proof, but on LEZ v0.4 a privacy-preserving transaction's calls to public actors are applied on its proof alone
  and can write the instance's store and records, so the light client's records rest on LEZ's receipt check as well.
  For the v0.3.1 instance of row 150 this was not examined (below).

## Correction, 23 November 2026

**What the note said.** Under "Not affected" (above, as written on 21 November 2026): "LEZ public transactions, which
the sequencer executes rather than verifies by proof (the light client's updates and records on its public path, …)".

**What holds.** A public transaction is executed by the sequencer, not checked by proof, so the public path's own steps
(the job's `Open`, `Miller` and `Finish`, and `Attest`) do not depend on RISC Zero. The state they read does. On LEZ v0.4
a privacy-preserving transaction acts on public actors through the calls its journal lists, and LEZ replays each on the
public side as the journal states it, sender, inherited authorizations and PDA seeds included, with no check of its own
(LEZ's `lee/state_machine/core/src/execution_state.rs:842-872`, `proven.with_from(Sender::Call(proven.envelope.from))`
at `:849`, at v0.4.0-rc1 and rc3); a receiving program cannot tell such a call from a call another actor makes
(`ReceiveInput`, `lee/state_machine/core/src/program/mod.rs:373-385`), though its own checks of the call's content still
run (`execution_state.rs:910-917`). Only the receipt check binds these calls to a real private run.
LEZ v0.4.0-rc1 and rc3 accept a transaction when its proof, decoded as any `InnerReceipt`, passes `Receipt::verify` for
the privacy circuit (`lee/state_machine/src/privacy_preserving_transaction/circuit/mod.rs:51-65`, the same in both).
With the versions both lock (risc0-zkvm 3.0.5, risc0-groth16 3.0.4, risc0-circuit-recursion 4.0.4), that accepts a
Groth16 receipt as well as a succinct one (risc0-zkvm 3.0.5 `src/receipt.rs:344-354`): the receipt kind
GHSA-g6qg-9j8p-7j87 is about, in its affected versions. So if LEZ accepts a forged receipt, which §3 above leaves to
Logos to confirm and which we have not tested (we built no forgery), it could, by our reading of LEZ's source, write any
instance's store and records: through `ApplyVerified`, which an instance accepts from any actor of the program
(`lez/guests/program/src/main.rs:288-305`), or through `Apply` and `Record` sent in a job's or the instance's name
(`:269-286`, `:405-425`). A job is applied after a check of its phase, its adoption and its starting store
(`crates/ferrata-core/src/steps.rs:605-623`) and the period rules of `apply_core_compact` (`:304-335`). The records
public consumers read follow from that store. GHSA-mm6c-35cm-g4x2 (the Solidity verifiers) does not reach LEZ. Every
Ferrata program's public state rests on the same check: the bridge program's (`issue` and the burn and transfer
deliveries, `lez/guests/bridge/src/main.rs:639`, `:775`, `:852`, among others), the reference consumer's
(`lez/guests/consumer/src/main.rs:185`), and that of the programs that accept a reply by its sender (`ferrata_holdings`,
`ferrata_statements`, the statements reader, `ferrata_rebate`), as `docs/trust.md` §5 lists. RFP-022's rows Sec.2, Sec.3
and Sec.6 state it for the bridge.

No live exposure follows: no v0.4 program of ours runs on a public network, and every v0.4 sequencer named in this
repository is one we ran. The experimental v0.3.1 port (row 150) is the one live LEZ deployment. On v0.3.1 a private
transaction's public effects go through the owning program's apply (LEZ v0.3.1
`lee/state_machine/src/validated_state_diff/mod.rs:556`), where the port runs every light-client check
(`lez-v03/guest/src/main.rs:16-19`); whether a forged privacy-preserving transaction could write its state there was not
examined.

**Corrected again, 23 November 2026.** The paragraph above stopped holding later the same day: since 23 November 2026
the project's own public test network (LEZ v0.4.0-rc3 on RISC Zero 3.0.5, its one sequencer ours, test value only) runs
the v0.4 programs, the attestation and bridge programs among them. Until that network and our programs move to LEZ v0.4.0 (released 27 November 2026 on RISC Zero 3.1; the port has not
started), that network's program state is exposed as this section describes, if LEZ accepts a forged receipt, which has not been tested. The
vault paired with it verifies with RISC Zero's verifier 3.1.0 (`vault/deployments/sepolia-31-B.verification.txt`), but a
release rests on burns the LEZ side recorded.

**Why it was missed.** The note judged the public path by the transactions it sends, not by everything that can
write the state it reads. Internal reviews of `main` at `8c38d723` (22 and 23 November 2026) found it.

**What changes.** `docs/trust.md` states it: a fourth assumption in §0, the provers' row of §2.4, §2.5 and §5. The
RFP-021 rows of `docs/requirements-status.md` that rest on an instance's store (F.1, F.2, F.12, S.6, VS.1, VS.2, VS.5,
VS.6) and RFP-022 Sec.6 give it as a condition. Nothing deployed changes. Nothing in our programs removes the dependence.
A program cannot tell a replayed call from a call another actor makes. A replayed call always names a sender
(`execution_state.rs:346-351`), so only handlers that take top-level calls alone are out of its reach, such as the
bridge's role actors (`lez/guests/bridge/src/main.rs:979`). Every Ferrata program accepts calls from other actors
somewhere, and no flag of ours separates them: an instance refusing `ApplyVerified` would still accept `Apply` and
`Record` in its own actors' names. The fix is LEZ's, a LEZ on risc0
3.1.0.

## Plan

1. Done by the owner on 21 November 2026: the two cap calls on each of the four vaults (below; read back in the Status
   line). Next: retire the v5 vaults for good when keyless retirement opens (`retireOpensAt`: 11, 12 and 18 December
   2026, if no checkpoint is submitted before), or through a freeze authority after the timelock. They will not be used
   again.
2. Done here on 21 November 2026: this note; the README, EVIDENCE row 149, [docs/sepolia.md](../../sepolia.md),
   [verify.md](../../guides/verify.md), the mainnet run's README, [lez-v0.4-findings.md](../../design/lez-v0.4-findings.md),
   [SECURITY.md](../../../SECURITY.md) and [e2e/README.md](../../../e2e/README.md) corrected.
3. Next, once the open branches that edit them have merged: [`trust.md`](../../trust.md), Security #9 in
   [`traceability.md`](../../traceability.md), [`bridge.md`](../../design/bridge.md) §14-15,
   [`vault/README.md`](../../../vault/README.md) and the operator guide say that the stop did not trip under this
   advisory (the paragraph "The emergency stop did not trip" above).
4. The move to risc0 3.1.0 and verifier 3.1.0, with new vaults: [risc0-3.1-migration.md](../../design/risc0-3.1-migration.md).
   Phase 1 (the Ethereum side) done on 23 November 2026: the 3.1 vault on Sepolia (EVIDENCE rows 231, 232).
5. LEZ: a LEZ tag on risc0 3.1.0 came with LEZ v0.4.0 (27 November 2026); our port to it (phase 2) has not started.

```sh
# The owner sent these calls with the deployer key (0x79A9…5C07) on 21 November 2026, in this order: the table below.
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

| Nonce | Vault | Call | Transaction | Block | Time (UTC) | Gas |
|---|---|---|---|---|---|---|
| 49 | `0x04eE…e44c` (crown-live-3) | `setGlobalCap(Release, 0, 0)` | `0x8b2aa9351110da81dde2a9f9981294992590450b2aef9ce3193bca577488ddaa` | 12,184,549 | 21:52:00 | 52,923 |
| 50 | `0x04eE…e44c` | `setGlobalCap(Deposit, 0, 0)` | `0x7d891ae6749cf14b617d2d3d3b1a98c5fceb8429edd540d4fcaffa4984006d98` | 12,184,550 | 21:52:12 | 50,034 |
| 51 | `0xfE31…9E54` (crown-live-1) | `setGlobalCap(Release, 0, 0)` | `0x2fce2e1c2e49d5fb4345be873672453bd1fbbcc9dda7f7b8601f0270b449ef76` | 12,184,551 | 21:52:24 | 52,923 |
| 52 | `0xfE31…9E54` | `setGlobalCap(Deposit, 0, 0)` | `0x10a5c7baa1e01124a97b9aad1e67eb55fbbe26fd782e8ea938eaecf015ca35dc` | 12,184,552 | 21:52:36 | 50,034 |
| 53 | `0x8055…3470` (crown-live-4) | `setGlobalCap(Release, 0, 0)` | `0xabc3ec8d3e988d04d96fc19dfc53d9e62002782a8ba6e96c8aaa0b909c31cb3e` | 12,184,553 | 21:52:48 | 52,923 |
| 54 | `0x8055…3470` | `setGlobalCap(Deposit, 0, 0)` | `0x30d6293da10beacfd0e8a355a19b61bdcbf3950ccc996c396af21bfc84fcda4b` | 12,184,554 | 21:53:00 | 50,034 |
| 55 | `0xd0B5…8AF7` (v3) | `setGlobalCap(Release, 0, 0)` | `0xddf006bc6aee1e12dd5ce4fa44d98f81d09efbfcb5f301ae749473632518e472` | 12,184,555 | 21:53:12 | 46,839 |
| 56 | `0xd0B5…8AF7` | `setGlobalCap(Deposit, 0, 0)` | `0x66e5e36025fe559ad05896dcebcb12f6b32623c37b7049f9edafaf9da0dc486b` | 12,184,573 | 21:57:00 | 46,820 |

On publicnode and on ethpandaops each is status 1 with one `GlobalCapSet(direction, 0, 0)` log from its vault. In all:
402,530 gas and 26,279,278,897,270 wei (about 0.000026 Sepolia ETH), by which the admin's balance fell between blocks
12,183,432 and 12,184,587. The read-back: the Status line above.

Re-check, sending nothing: `docs/security/risc0-groth16-advisory-2026-11-21/reads.sh URL BLOCK OUT` (at 12,183,432,
and at 12,184,587 after the caps) and `fork-rehearsal.sh URL BLOCK OUTDIR` (Foundry's `cast` and `anvil`; at block
12,183,432 it compares its transcript with [`fork-rehearsal.txt`](fork-rehearsal.txt), where only line 1 and
`releasesFrozenUntil` may differ, and exits 1 if anything else does). Both need an endpoint that serves historical
state at that block (an archive node, e.g. `https://rpc.sepolia.ethpandaops.io`); publicnode refuses reads at past
blocks (8 of 205 on a re-run of 21 November 2026), and `reads.sh` then exits 1. `estimate.sh URL OUT` estimates at the
latest block. Each transaction above: `cast receipt HASH --rpc-url URL`.
