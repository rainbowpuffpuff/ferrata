# live-roundtrip-2026-11-24: the first full round trip on the Ferrata LEZ test network

ETH from Sepolia to the LEZ v0.4.0-rc3 network we run and back: a deposit into the vault
`0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787`, its root imported on the zone, the deposit claimed and burned there, the
zone's first checkpoint (rule (B)) proven, submitted and finalized on Sepolia, and the burn released, 23-24 November
2026. Public chain data only: [`record.json`](record.json) holds every fact below as both chains give it, and
[`scripts/verify-live-roundtrip.py`](../../../scripts/verify-live-roundtrip.py) reads each one again, rebuilds the record
from the chains' answers and compares it field by field, then checks the links between the legs on those answers;
[`proofs.json`](proofs.json) is the prover client's record of the two GPU proofs, checked against the chain too;
[`lez-blocks.json`](lez-blocks.json) holds the zone's answers for the blocks the LEZ transactions are in (and two
others), which the offline test replays:

```sh
python3 -I scripts/verify-live-roundtrip.py --check     # online, read-only: 24 checks, every field (Sepolia's public RPCs, the zone's /rpc)
python3 -I scripts/test_verify_live_roundtrip.py        # offline (CI): the links on the record, chains that do not link, each field
                                                        # altered, the zone's answers replayed and planted ones refused
```

The zone is ours (one validator, Logos Blockchain node 0.3.5-rc.1 with dev-mode leader proofs; docs/EVIDENCE.md row
248), not LEZ testnet 0.4, which does not exist. The deposit and the release are to and from the same Ethereum address,
`0x81781fFEB24BfF62AE0046f59869b1Cc534C2080` (chain data); the claim credited, and the burn debited, the LEZ public account
`4APuasjMfEjHzXXNui7QmYZ6t2ABR946mCa4Jf4bpFoo`.

## The legs

| Leg | Chain, block (time, UTC) | Transaction | What the chain shows |
|---|---|---|---|
| Deposit | Sepolia 12,196,757 (23 Nov 16:11:00) | `0x44709f7c31561f1682b67b95a1a01b774246fcdcbdb4f049d482e8284857e9e1` | status 1, gas 614,222; `Deposit` leaf 0, 0.001 ETH, commitment `0xb42a61d1…2c3a`; `depositRoot` `0x782d35b1…` -> `0x0d0c28da…8b23`, `escrowed(ETH)` 0 -> 0.001002 |
| Fee import | LEZ 2,276 (16:31:22) | `50f1d085b23fe47a6b1b4a6a12463311f272a7ea52df24b55a0a0dd62bc63ec3` | applied; `import_fees` (ETH's fees known to leaf 1) under the attested Sepolia record slot 11,643,072 |
| Root import | LEZ 2,278 (16:31:42) | `f2fb461e534c17fbcca0de06ff8091542744bbfb7aee27d641e72bfc1213b1e3` | applied; `import_root` root(cid, 1) = `0x0d0c28da…8b23`, the same record slot. A second sender's import of the same root, `dd159aec…7075`, came second in that block and reverted |
| Claim | LEZ 3,746 (20:36:22) | `28771fd510b26758f39ee2b1b5e921568a6b0c14857267d58e91ffb8ef1c2e2f` | applied; `claim` of leaf 0 under root(cid, 1), signed by the account it credits |
| Burn | LEZ 3,749 (20:36:52) | `5c89d209852ee257bad4191cdd176e11dfea6b83b35a12f8a9487c6fa2e727cf` | applied; `burn` of 0.001 ETH (a public burn), its note `COut` `0xbc2f4eea…2664` |
| Checkpoint submitted | Sepolia 12,198,162 (21:03:24) | `0xcaa00efa1ec41cf7dfe7ebc20e35368784535fc0e6ec49934644148e13aea0d1` | status 1, gas 729,373; `CandidateSubmitted` newState `0x66ce08ea…4f48`, burnRoot `0x0478a044…0a4d`, 1 burn, 1 imported leaf, tip 3,767; the candidate's `journalDigest` `0xd71cefb9…b0c6` |
| Checkpoint finalized | Sepolia 12,201,456 (24 Nov 08:30:12) | `0x643d0c687b782aad00618d0d50bbaa3cd6aaa01908e0fcf2f2e724b09c5390f8` | status 1, gas 844,349; `CheckpointFinalized` index 0; `checkpointCount` 0 -> 1, `finalState` genesis -> `0x66ce08ea…`, `burnRootCheckpoint(burnRoot)` 0 -> 1 |
| Release | Sepolia 12,201,646 (09:09:24) | `0x48b78ad406eae9c05eb665a1ce06e205cacc45f5905769425b4abbc700886f65` | status 1, gas 732,686; `Release` 0.001 ETH, relay fee 0.000002 to the relayer, `OwedCredited` 0.000001 (the checkpoint fee) to the relayer, which submitted the checkpoint (the vault's `prover` of the candidate); `nullifierUsed(NOut)` false -> true, `escrowed(ETH)` 0.001002 -> 0.000002; the recipient's balance **+0.000997 ETH** in that block |
| Fee claimed | Sepolia 12,201,647 (09:09:36) | `0xe2c264e63ec2ab2e2e9960405125c6359f711b3eaa16f8b534db41149c6e1239` | status 1, gas 44,728; `OwedClaimed` 0.000001; `owed(relayer, ETH)` and `totalOwed(ETH)` back to 0 |

Each Sepolia transaction is the vault's only event in its block, so the vault's state read by `eth_call` at the end of
the block before and of its own block (in the record, for every leg) is that transaction's doing. Each LEZ transaction
is taken from the block `getTransaction` names, as that block's bytes hold it (the block's hash recomputed from them):
its bytes, the ones `getTransaction` serves, hash to its id, a block that does not hold it is refused, and it applied or
reverted as that block's reverted list says; its bridge message decodes with `idl/ferrata_bridge.idl.json`
(idl/check.py's decoder) with every byte used.

## What links the legs (checked on the chains' answers, not on the record's copy of them)

1. **The import carries the deposit.** The root `import_root` wrote as root(cid, 1) is the vault's `depositRoot` at the
   end of the deposit's block. The import applied in its block; the second sender's import of the same root, under the
   same record (Sepolia slot 11,643,072), is a later transaction of that block and reverted.
2. **The claim is this deposit's.** The claim's witness opens the deposit: SHA-256("ferrata/C/v1" ‖ s ‖ kind ‖ dest)
   is the `Deposit` event's commitment, dest (kind 2, a public destination) is the account that signed the claim, and
   the vault's leaf (`FerrataVault._leafHash`: leaf 0, the depositor, ETH, 0.001 ETH, that commitment) folded with the
   claim's 32-sibling path is the same `depositRoot`. This is how the claim was identified: from its own bytes on the
   zone, not from timing. (A plaintext claim publishes its opening; this deposit took that path, so the zone shows
   which deposit it claimed. The private path, which hides that link, is not part of this run.)
3. **The checkpoint holds this burn and no other.** The burn leaf (`ferrata-bridge-core` encoding: burn index 0, the
   configuration, ETH, 0.001 ETH, the burn's `COut` and `r`, all from the burn's own message) folded with an empty
   path is the burnRoot of the candidate, of the finalized checkpoint and of the release; the checkpoint counts 1 burn.
4. **The proofs are the ones the vault verified.** Each proof record's image id is the vault's (`checkpointImageId`,
   `releaseImageId`) and its seal is the one in the transaction that carried it (`submitCandidate`, `release`). The
   candidate's `journalDigest` on the vault is the SHA-256 of the checkpoint proof's journal, and the release journal
   (`FerrataVault._releaseJournalDigest`, rebuilt from the release's calldata and amount) is the release proof's, byte
   for byte (SHA-256 `0x54ee2772…47e4`). The release proof's nullifier is the one the vault marked used.
5. **The claim and the burn are in the checkpoint's history.** The zone's blocks 3,746 to 3,767 are read, each hash
   recomputed from the block's bytes and each linked to the block before; block 3,767's hash is the candidate's
   `tipHash` (the tip the checkpoint proof covers), and the claim's and the burn's blocks, which hold them (each taken
   from its block's own bytes) as applied transactions, are on that chain. The
   imports' blocks (2,276, 2,278) are not walked (1,490 more blocks); the claim, applied under root(cid, 1), rests on
   the import.

## What ran where (not on chain)

The checkpoint and the release were proven on Modal GPUs by the host's prover client
(`deploy/testnet/host/bin/ferrata-bridge-prover`); its own records, [`proofs.json`](proofs.json) (their journals, seals
and image ids checked against the chain above), give two times each: from picking the request up to the proof's
return, Modal's start included, and the prover's own run on the GPU (the proof and its Groth16 wrap, `prover.wall_s`):
**checkpoint `checkpoint_b_0_3_5`** (image `0x8917f58e…af04`) over LEZ blocks 1,226-3,767 (2,542 blocks, input
3,064,586 bytes) in **1,307.0 s**, of which the prover **87.1 s**; **release `release_31`** (image `0xf279af41…a6fd`,
Groth16) in **425.6 s**, of which the prover **9.9 s**. The host's checkpoint watcher built the checkpoint's input from the zone's blocks; its relayer
(`0x1862d98b06B0aB2e201B2C224232225deaD441ba`) submitted and finalized the checkpoint, sent the release and claimed its
fee. The release was requested by the holder with the `ferrata` CLI (`ferrata release`), whose input the prover saw.

## What this run does not show

- **Latency.** The finalisation came about 10.5 hours after the 1-hour window had passed (22:03:24 on 23 November): the relayer had stalled on a
  fault of the host's deployment (fixed in `deploy/testnet/`), so the outbound timings here are not a measurement.
- **The private path.** The claim was a plaintext claim and the burn a public burn; the private claim and burn, and the
  privacy rows, rest on other records.
- **A network Logos runs.** Everything is on the zone we run (above), with one sequencer key.
