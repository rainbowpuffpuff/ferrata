# testnet-return-fork-1: the stand-in return leg, rehearsed on a fork of Sepolia (18 Nov 2026)

A rehearsal of the Ethereum side of the leg back from the public LEZ testnet (LEZ v0.3.1), run by
[`e2e/testnet-return.sh`](../../testnet-return.sh) on a local anvil fork of Sepolia at block 12,160,815.
**Nothing was sent to Sepolia or to the LEZ testnet.** The arrangement is a testnet stand-in, not Ferrata's design
([`vault/src/standin/README.md`](../../../vault/src/standin/README.md), [`docs/trust.md`](../../../docs/trust.md) §3.6).

**Real in this run:** the unchanged FerrataVault (rule (B)) and FerrataRegistry; RISC Zero's Groth16 verifier and
emergency stop, which are Sepolia's own contracts on the fork (`0x8f0A…2A0` behind `0x1dB3…8221`); two deposits; two
proofs of the unchanged `release` program (image `0xb64900bc…29e9`, [`docs/image-ids.md`](../../../docs/image-ids.md)),
each proven on a CPU, wrapped to Groth16 and accepted by that verifier; both payments.

**Stand-in in this run:**

- **the burn root's attestation:** two of three keys we hold signed the checkpoint journal (no checkpoint proof);
- **the burns:** two burn notes of a labelled test wallet, computed natively (`standin.py native-burns`), one named
  public and one private. No LEZ transaction made them. The Rust bridge core recomputed every leaf and the root
  (`ferrata-e2e crown-release`: "bridge core = checkpoint core, leaf by leaf");
- **`lezConfigId`:** a labelled placeholder (`plan.json`, `lezConfigIdIsPlaceholder: true`), because the v0.3.1 bridge
  configuration did not exist yet. A live run takes the real one and makes its proofs again;
- **the deposits' destinations:** the crown run's labelled test accounts and test bridge seed;
- **the chain:** a fork. The sender (the deployer `0x79A9…5C07`, at its real nonce 36) was impersonated; no key was read.

What was read from the public LEZ testnet, and is real: block 83001 (17 November, 03:13 UTC), which Logos's node
reported `Finalized`, its hash recomputed from its contents. It is the tip of the vault's `genesisState` and of the
checkpoint. The bridge's accumulator was **not** read from the node (`checkpoint/candidate.json`,
`accumulator_checked_against_node: false`): no bridge is deployed there.

## The transactions and their gas

Ten transactions, all by the deployer, **11,458,605 gas** as anvil counts it (`manifest.json`, `gas.tsv`).
Anvil does not price Sepolia's state gas (EIP-8037, in force there since 6 October 2026), so the last column gives
what the same transaction cost or was estimated at on Sepolia itself.

| # | Stage | Transaction | Gas on the fork | On Sepolia (EIP-8037) |
|---|---|---|---:|---:|
| 1 | deploy | `StandInCheckpointVerifier` at `0xB24905b767c1b50dDe6b1B33aDb119CA7E2dEfCA` (nonce 36): 2 of 3 keys, in front of RISC Zero's stop | 803,966 | 5,080,177 (two public nodes' `eth_estimateGas` for this creation, 18 Nov) |
| 2 | deploy | `FerrataRegistry` at `0x837F715d57299799C64dcF5ba84525319D5EeAfe` (nonce 37) | 2,757,365 | 19,700,914 (the same estimate, and crown-live-3's) |
| 3 | deploy | `FerrataVault` at `0x80555Fe53CCcEf46951B23A3c75643a4C9283470` (nonce 38): verifier = the stand-in, `checkpointImageId` = `0x66ff80f9…dc38` | 5,689,775 | 39,559,884 (crown-live-3's estimate for the same bytecode) |
| 4 | deploy | `registerToken(ETH)` | 709,314 | 3,218,360 (crown-live-3's estimate) |
| 5 | deposit-0 | `depositETH`, 0.001 ETH + 0.000002 relay fee, public destination, leaf 0 | 200,560 | 559,652 (crown-live-3, used) |
| 6 | deposit-1 | `depositETH`, the same, private destination, leaf 1 | 151,406 | 268,026 (crown-live-3, used) |
| 7 | submit | `submitCandidate`: the journal with a **130-byte seal of two signatures** | 165,500 | below 729,349 (crown-live-3's, which verified a Groth16 seal) |
| 8 | finalizeAttested | after the wait of 120 s | 221,505 | 844,349 (crown-live-3, used) |
| 9 | release-0 | `release` with a **real Groth16 seal**, after the 120 s release delay: 0.000999 ETH paid | 396,725 | 661,632 (crown-live-3, used) |
| 10 | release-1 | the same for the second burn | 362,489 | 465,768 (crown-live-3, used) |

A live run would therefore use about 71 M gas. The deployed library of crown-live-3
(`0xa75e653BdaD23d198Ea357277b715aD8A9687673`) is byte for byte this tree's `FerrataRegistry` (compared on 18 Nov), so
`FERRATA_REGISTRY` could save transaction 2; that path moves the vault to nonce 37 and was not rehearsed.

On the fork the Ethereum side took 96 s from the first transaction to the last check (10:16:22Z to 10:17:58Z); the two
waits were passed by moving the fork's clock.

## The release proofs

| Step | public-0 | private-1 | Where |
|---|---:|---:|---|
| executor: user cycles, segments | 77,736, 1 | 77,736, 1 | `releases/*.exec.json` (the same count for both witnesses) |
| succinct proof, `r0vm` 3.0.5 on a CPU (32 threads) | 12.5 s | 12.5 s | `releases/*.prove.json` |
| `identity_p254` | 12.6 s | 12.8 s | `releases/*.p254.json` |
| RISC Zero's Groth16 prover image (`risc0-groth16-prover:v2025-04-03.1`) | 20 s | 18 s | the prover's own log |
| seal | 260 bytes, selector `0x73c457ba` | the same | `releases/receipts/*/seal.hex`, `groth16.json` |

About 45 s of computing per release, on a 32-thread CPU; no GPU. The journal each proof committed is the vault's
352-byte release journal for this vault, chain id and `lezConfigId` (`releases/receipts/*/journal.hex`,
`releases/release-*.json`).

## What was refused

`refusals.json`: 28 `eth_call`s at three points of the run, each answered as expected.

| When | Call | Answer |
|---|---|---|
| before the candidate | the candidate with one signature (a root signed below threshold) | `BelowThreshold()` |
| | the candidate with one signature twice | `SignersNotAscending()` |
| | the signatures with another burn root | `NotASigner()` |
| | each release, with its real proof, before its root was submitted | `BurnRootNotAccepted()` |
| candidate submitted, not final | finalisation before the wait was over | `WaitNotOver()` |
| | each release, with its real proof, under a root that was signed but not final | `BurnRootNotAccepted()` |
| after the releases | each release again (a replay) | `NullifierAlreadyUsed()` |
| | each release under a root nobody signed | `BurnRootNotAccepted()` |
| | the attestation for another journal; one signature; one signature twice (at the stand-in) | `NotASigner()`, `BelowThreshold()`, `SignersNotAscending()` |
| | the attestation sent as a `checkpoint_b` proof; the signatures sent as a release proof | reverted in RISC Zero's verifier (`SelectorMismatch`) |
| | each release proof for another journal | `VerificationFailed()` |

Accepted, as they must be: the signed candidate (the stand-in), and each release proof at the stand-in, at RISC Zero's
stop and at RISC Zero's Groth16 verifier directly.

## The check

`verify-fork.txt`: `lez-v03-return/verify.py check` against the fork, **10 of 10**: every transaction as recorded; the
CREATE addresses; the stand-in's key set, threshold and the verifier behind it; the vault's immutables (the stand-in,
the stand-in id, the `release` id of `docs/image-ids.md`, rule (B)); `genesisState` recomputed from the LEZ block and
the key set; the deposits and the vault's root recomputed; the signed journal, its signers recovered, the burn root
recomputed from `checkpoint/burns.json`, the finalisation; both proofs in RISC Zero's verifier and both payments; the
permanent refusals. A fork's transactions exist nowhere else, so this record cannot be re-checked against public
Sepolia; a live run's can (`e2e/testnet-return.sh verify <run>`).

## Files

`plan.json` (the plan and the deployment's parameters), `identity.json`, `params.json`, `deploy.json`,
`manifest.json`, `gas.tsv`, `crown.json` (the deposits, the candidate, the releases with their seals, as
`vault/script/Crown.s.sol` reads them), `checkpoint/` (the burns, the journal, the signed candidate),
`releases/` (statements, prover records, seals, journals), `refusals.json`, `verify-fork.txt`. Not committed: the
stand-in keys, the wallet's burn notes and the release inputs (they hold the release secrets; here a test wallet's),
the receipts' binaries, the logs (they name local paths).
