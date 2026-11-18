# `standin/`: a testnet stand-in for the checkpoint proof

**Not part of Ferrata's design. Not in the proposal. Never for production.** This directory holds one contract,
[`StandInCheckpointVerifier.sol`](StandInCheckpointVerifier.sol), used only by a test deployment that shows a burn on
the **public LEZ testnet (LEZ v0.3.1)** released on Sepolia. In that deployment the vault, the deposits and every
release proof are real; the burn root is accepted on the signatures of keys that the operator of the run holds.

## Why it exists

Ferrata's vault accepts a LEZ burn root only with a zero-knowledge proof of its checkpoint program
([`docs/trust.md`](../../../docs/trust.md) §3, [`docs/design/bridge.md`](../../../docs/design/bridge.md) §7–§11). Under
rule (B) that program, `checkpoint_b`, reads LEZ v0.4 blocks and their inscriptions on Logos Blockchain and checks the
signatures of `t` of the zone's sequencers over the final tip, made by `ferrata_attestor` with keys their channel keys
certified. It ran live on Sepolia against a v0.4 zone we started ([`docs/sepolia.md`](../../../docs/sepolia.md)).

On the public LEZ testnet none of that can run as built:

- its blocks are v0.3.1, and the checkpoint programs read v0.4 blocks;
- its sequencer is Logos's; it runs no attestor, and its channel key has certified nothing for us.

So no checkpoint proof can exist for that zone today. To show the rest of the return leg on public data anyway, this
contract takes the place of that one proof and of nothing else.

## What the contract does

It is what the vault names as its `verifier` (an `IRiscZeroVerifier` that also answers `paused()`):

| Image id asked about | Who decides |
|---|---|
| `STAND_IN_IMAGE_ID` = `sha256("ferrata/stand-in/checkpoint-attestation/v1")` = `0x66ff80f9…dc38` | **Stand-in.** The seal must be `k` concatenated 65-byte ECDSA signatures, `threshold ≤ k ≤ n`, each by a key of the fixed set, in strictly ascending order of signer address, over the EIP-712 digest of `CheckpointAttestation(imageId, journalDigest)` under the domain (name, version, **chain id**, **this contract's address**). |
| every other id, the `release` program's among them | **Real.** The call goes unchanged to RISC Zero's `RiscZeroVerifierEmergencyStop` around RISC Zero's Groth16 verifier. |

`paused()` is RISC Zero's stop; once it is tripped the stand-in id is refused too.

The stand-in id is a constant, the hash of a label. It is no program's image id, so a vault's `checkpointImageId()`
shows at a glance that the vault does not take `checkpoint_b` proofs, and the contract can never be set up to answer
for a real program. There is no owner, no setter and no upgrade. The constructor refuses every chain but anvil,
Sepolia, Hoodi and Holesky.

The journal digest the keys sign is the vault's own (`FerrataVault._checkpointDigest`): it binds the vault's chain id,
its address and its `lezConfigId`. The EIP-712 domain adds the verifier's chain id and address. A signature is
therefore good for one journal, one vault, one verifier, one chain.

## What stays real, and what the stand-in does not establish

Real in a vault that uses it: the vault's bytecode ([`FerrataVault.sol`](../FerrataVault.sol) and
[`FerrataRegistry.sol`](../FerrataRegistry.sol), unchanged) and all its rules (the candidate's public wait,
finalisation, the release delay, nullifiers, caps, freezes, revocation); the deposits; and **every release, which
still needs a zero-knowledge proof of the unchanged `release` program** that a burn of this configuration, token,
amount and recipient is under the accepted root.

Not established: that the accepted root is a root of burns in valid, final LEZ blocks. The signers are not the zone's
sequencers. Nothing in the contract looks at LEZ blocks or at Logos Blockchain. **`threshold` of the keys can make the
vault accept any burn root, and with a root of their own making they can produce release proofs for burns that never
happened and take the whole escrow.** That is a bridge with a trusted committee for the outbound root, which is what
Ferrata's design exists to avoid. The three keys of the testnet run are held by one operator, so the threshold
protects against a signing mistake, not against the operator.

What the keys cannot do: stand in for a release proof (signatures sent as a release seal reach RISC Zero's verifier and
fail), reuse a signature on another vault, verifier, chain or journal, or sign after RISC Zero's stop.

## The deployment that uses it

Chosen for the testnet run ([`e2e/testnet-return.sh`](../../../e2e/testnet-return.sh),
[`script/standin/DeployStandIn.s.sol`](../../script/standin/DeployStandIn.s.sol)):

- **Rule (B)**, with `submitCandidate` and then `finalizeAttested`. Rule (B) is "one proof of `t` attestations of the
  final tip, final after the wait unless cancelled": the stand-in replaces exactly that one object. Rule (A) would
  need a second seal that echoes a beacon recorded after the wait, and its support inference from later inscriptions
  means nothing for signatures by keys we hold: one more transaction, one more signing round, nothing shown.
  `finalize` (rule (A) only) is not used.
- `checkpointImageId` = the stand-in id; `releaseImageId` = the final `release` program's id
  ([`docs/image-ids.md`](../../../docs/image-ids.md)); `lezConfigId` = the LEZ v0.3.1 bridge configuration's id, a
  parameter (it is also in every burn leaf, so a release proof is bound to it).
- `nMin` = `n` = the stand-in set's size (3), `tMin` = `t` = its threshold (2): the journal's key-set fields describe
  the keys that actually sign. The key set in the checkpoint state is the signer addresses, left-padded to 32 bytes.
- `genesisState`: the checkpoint state at a block the public LEZ node reports Finalized, with no burns and no
  imports.
- `delta` = `deltaFloor` = 120 s and a release delay of 120 s, so a release can be paid about five minutes after the
  attestation is submitted. The vault's bound `revokeWindow / deltaFloor ≤ 1000` puts the floor at 87 s for a one-day
  revocation window. crown-live-3 used 3,600 s and 120 s.
- Everything else as crown-live-3's test deployment: a one-day revocation window, a 30-day maximum freeze, no caps, no
  watchers, a freeze authority nobody holds a key for, ETH with denominations 0.001 / 0.01 / 0.1 ETH, a 0.2 % relay
  fee and a 0.1 % checkpoint fee.
- RISC Zero's stop is the one the live vaults already use on Sepolia
  (`0x1dB3fa8041E8178f1608c2F7dF627d7EC16C8221`, owner renounced, around the Groth16 verifier
  `0x8f0Af3Bf302C64d57f840127F94d9c123a8Fa2A0`).

**One vault for both directions.** Deposits can only leave the vault they went into, and a vault's verifier is
immutable. A testnet round trip therefore makes its deposits into this deployment's vault; a vault behind RISC Zero's
verifier alone keeps them until a checkpoint proof for the zone exists
([`lez-v03-return/INTERFACE.md`](../../../lez-v03-return/INTERFACE.md) §4).

## Tests

[`test/standin/`](../../test/standin/), 40 tests; the vault's own suites are untouched (504 passed, 1 skipped, with
these).

- `StandInCheckpointVerifier.t.sol` (30): a threshold is accepted (every pair, all three); refused are a seal below
  threshold, an empty or misaligned seal, more signatures than keys, a wrong signer, a duplicated signer, signers out
  of order, the other encoding of a signature (high `s`) and a raw `v`, a signature for another journal, another
  image, another verifier, another chain (in both directions), and a signature without the domain. Every other image
  id is forwarded: a **real release seal** of the live run crown-live-3 verifies through the stand-in exactly as in
  RISC Zero's contracts, fails for another journal or image, and fails when tampered; signatures sent for the release
  id fail; real seals sent for the stand-in id fail. After RISC Zero's stop nothing verifies. The constructor's
  refusals.
- `StandInVault.t.sol` (10): the unchanged vault with the stand-in as verifier, at the address, chain id and
  configuration that crown-live-3's two real release proofs bind. Their burn root is accepted on two signatures, final
  after the wait, and **both releases are paid with their real Groth16 seals**; refused are a replay, a release under
  an unsigned root, a root signed below threshold, a root signed for another vault, chain or configuration, a signed
  journal with a changed field, crown-live-3's real `checkpoint_b` proof, signatures as a release proof, and a
  candidate after RISC Zero's stop.

```sh
cd vault && forge test --match-path "test/standin/*"
```
