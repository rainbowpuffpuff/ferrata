# `lez-v03-return/`: the leg back from the public LEZ testnet (a stand-in attestor)

**A testnet stand-in, not Ferrata's design, never for production.** This directory holds the off-chain half of a test
arrangement that shows a burn on the public LEZ testnet (LEZ v0.3.1, `https://testnet.lez.logos.co`) released by a
FerrataVault on Sepolia. In that arrangement the vault, the deposits and every release proof are real (each release is
a zero-knowledge proof of the unchanged `release` program, checked by RISC Zero's Groth16 verifier); the burn root is
attested by keys that the operator of the run holds, because no checkpoint proof can exist for that zone today. The
on-chain half and the reasons are in [`vault/src/standin/README.md`](../vault/src/standin/README.md); the trust
statement is [`docs/trust.md`](../docs/trust.md) §3.6.

| File | What it is |
|---|---|
| [`standin.py`](standin.py) | the attestor: reads a LEZ v0.3.1 node (reads only), recomputes the burn root from the listed burns, compares it with the bridge's accumulator on the node, builds the vault's 17-word checkpoint journal, signs it with the stand-in keys (EIP-712, the stand-in verifier's domain) |
| [`verify.py`](verify.py) | re-reads a run's verdicts from an Ethereum endpoint: every transaction, the deployment, the signed checkpoint, both release proofs through RISC Zero's verifier, the payments, the refusals |
| [`INTERFACE.md`](INTERFACE.md) | what the attestor reads from the v0.3.1 bridge: the accumulator's layout, the burns, the tip |
| [`test_standin.py`](test_standin.py) | 25 tests against a local stub of the node's RPC that serves the real testnet blocks below and an accumulator computed natively |
| [`test_attestor_checks.py`](test_attestor_checks.py) | an independent review's tests (18 Nov 2026), turned into refusals: nothing unchecked is signed, no tip is given by hand; and the derived layout read against lez-v03-bridge's own recorded run |
| [`stub_node.py`](stub_node.py) | a local stub of the node for the fork rehearsal: the recorded real blocks, and a bridge state placed from the rehearsal's burns (a stand-in in its own right) |
| [`fixtures/`](fixtures/) | four real blocks of the public LEZ testnet (83000, 83001 Finalized; 83002, 83003 Pending), read on 18 November 2026, the transactions that inscribed them on the public Logos Blockchain testnet, and the channel's configuration |
| [`STAGE2.md`](STAGE2.md), [`stage2.py`](stage2.py), [`test_stage2.py`](test_stage2.py) | a measurement, not part of the stand-in: how far a real checkpoint gets on that network's public data (the inscriptions' hashes, signatures and message chain recomputed natively), what a v0.3.1 checkpoint program would need, and how long finality takes there |

Standard library only (`python3 -I`): SHA-256 and HMAC from `hashlib`, Keccak-256 and secp256k1 signing written out
here (RFC 6979 nonces, low `s`; not constant-time, for throwaway testnet keys). Everything it computes is recomputed
by other code before anything depends on it: the burn leaves and the root by the Rust bridge core
(`ferrata-e2e crown-release`, which makes the release inputs), the journal digest by the vault, the signed digest by
the stand-in verifier (`attestationDigest`).

## What "final" means on LEZ v0.3.1, and how the attestor checks it

A LEZ block becomes final when its inscription on the zone's channel is in a block of Logos Blockchain at or below
that chain's last irreversible block. In v0.3.1 only the sequencer knows this; it records it per block as
`bedrock_status` (`Pending`, `Safe`, `Finalized`: `lez/common/src/block.rs:65-76` at tag `v0.3.1`):

- the sequencer's Logos Blockchain client turns each finalized channel operation into a finalized entry
  (`lez/sequencer/actors/bedrock/src/actor.rs:207-225`);
- the core takes the final tip's block id as `finalized_up_to` (`lez/sequencer/core/src/lib.rs:765`, `:995`);
- the storage actor marks every stored block up to that id `Finalized`, and a block keeps that status once it has it
  (`lez/sequencer/actors/storage/src/actor.rs:247-284`). No code of the sequencer sets `Safe`.

`getBlock` returns the block in borsh with that status as its last byte
(`lez/sequencer/service/rpc/src/lib.rs:56`). A v0.3.1 block carries no signature and no state root; its hash is
`sha256("/LEE/v0.3/Message/Block/" ‖ 8 zero bytes ‖ borsh(block_id, prev_block_hash, timestamp, transactions))`
(`lez/common/src/block.rs:112-131`).

`standin.py checkpoint` takes the node (`--rpc`) and the bridge's layout (`--layout`, or one derived by `standin.py
layout` from the program id) as required arguments; there is no way to give it a tip or a state by hand. It:

1. reads the bridge's accumulator between two reads of the head block that agree (same id, same hash), so that it is
   the state after that block (the *read head*);
2. recomputes every burn leaf, the root and the frontier from the listed burns and requires the node's count, root
   (or frontier) and configuration to equal them (and, with `--deposit-root`, the deposit root the bridge imported to
   equal the vault's);
3. takes as the tip the block given with `--block`, or else the newest block the node reports `Finalized`, and
   requires the node to report it `Finalized`. A `Pending` or `Safe` tip is refused, and no option lifts the refusal:
   on this network such blocks are replaced. Block 83006 (11 transactions, timestamp 18 November 2026, 08:50:23 UTC)
   was the node's `Pending` head at 08:54 UTC and was gone at 10:28 UTC, when the head was 83005 again;
4. recomputes the tip's hash from the block's own bytes and requires it to link to its parent's hash;
5. if the tip is not the read head, reads every block between them and refuses unless the accumulator cannot have
   changed there (next section).

The checkpoint file records both checks in its `source` (`final_on_node`, `accumulator_checked_against_node`, the read
head, the blocks between). `standin.py sign` refuses a checkpoint whose `source` does not record both as true, or
whose journal names another tip, and `verify.py` fails a run whose candidate lacks either.

## A tip below the head

The accumulator can only be read as it is now, after the head; the head is rarely `Finalized` on this network (on 18
November blocks came four to ten hours apart, and a block became `Finalized` about a day and a half after it was
made), so a tip at the head alone would almost never be signed. The attestor therefore takes a `Finalized` tip `T`
below the read head `h` when no block in `(T, h]` can have changed the accumulator, and records those blocks. Why the
accumulator after `T` is then the one read after `h`:

- the accumulator is the bridge program's shard of two accounts, `burns(cid)` and `root(cid, n)`. In v0.3.1 an
  account's shard of a program is written only by that program's own `apply`
  (the v0.3.1 model as [`lez-v03-bridge/core/src/lib.rs`](../lez-v03-bridge/core/src/lib.rs) lines 9-13 states it:
  "on another program's shard it can only check"), and only when a transaction runs the program;
- the program runs either as a transaction's root call (a public transaction's `program_account_id`), as a call from
  another program (a *chained call*, which carries its own shard selectors, so the outer transaction need not name the
  bridge), or inside a privacy-preserving transaction;
- lez-v03-bridge writes `burns(cid)` only in `Burn` and `root(cid, n)` only in `ImportRoot`, and both refuse a caller
  (`lez-v03-bridge/guests/bridge/src/main.rs:125` and `:237`, "only a top-level call ... is visible to a
  checkpoint"); the claims only guard `root(cid, n)` (`RootIs`). So a chained call cannot change the accumulator;
- the attestor reads every block in `(T, h]`, recomputes each hash, links each to the one before (from `T`'s hash to
  the read head's), and parses every transaction: a public transaction whose root program is the bridge, or whose
  shard selectors name the bridge program or an account of the accumulator, refuses; so does any transaction it does
  not parse in full (a privacy-preserving one, which could hold a private burn; v0.3.1 blocks also carry no failed
  marker, so a failed bridge transaction refuses too).

So the state after `h` equals the state after `T`. The condition is the bridge's, so a layout must say it: `standin.py
layout` writes `changes_only_in_root_calls` (with the source lines above); a layout without it gets a tip at the read
head only. The same reading applies to a tip above the read head (`--block` later than `h`). Tests: the real blocks
83002 and 83003 (2 and 9 public transactions of other programs) between the `Finalized` 83001 and the read head 83003
are accepted; a bridge transaction at the root or in a shard selector, a privacy-preserving transaction, and a head
replaced during the read are refused or read again (`test_standin.py`). The rehearsal
[`testnet-return-fork-2`](../e2e/runs/testnet-return-fork-2/README.md) was cut this way.

What it does **not** check, and why the result is a stand-in:

- **the inscription itself.** It trusts one node's flag, the node Logos runs. The inscription can be read and checked
  from public Logos Blockchain data ([`STAGE2.md`](STAGE2.md) §1); its finality cannot (§3);
- **validity.** It executes nothing: the accumulator is what that node says the state is;
- **the node.** One RPC endpoint, over TLS, no second source.

On 18 November 2026 the public testnet's newest `Finalized` block was 83001 (17 November, 03:13 UTC); the blocks
after it were `Pending`, produced four to ten hours apart, and one of them was rolled back (above). A burn made there
is attested once a block at or after the burns' block is `Finalized` and no later block touches the bridge; until then
the attestor signs nothing. The Ethereum side adds about five minutes after the attestation is submitted.

## The keys

`standin.py keygen --dir DIR` writes fresh secp256k1 keys, one file each, mode 600, and prints only their addresses.
The keys of the testnet run are outside the repository; `e2e/testnet-return.sh` refuses a key directory inside it.
**The live run uses fresh keys**, generated for it and used for nothing else: the rehearsals' seals are committed and
were signed for the same chain, verifier and vault addresses a live run gets at the same deployer nonce, so their keys
are the rehearsals' only. `e2e/testnet-return.sh live` refuses a plan whose signers signed for any other run of this
leg (`e2e/runs/testnet-return-*/plan.json`). The owner's live keys are in a directory of their own, mode 600, outside
the repository, and have signed nothing. The plan reads the signers from that directory, and the deployment takes them
from the plan (`params.json`, read by `vault/script/standin/DeployStandIn.s.sol`):

```sh
python3 -I lez-v03-return/standin.py keygen --dir <live keys>             # once, for the live run only
FERRATA_STANDIN_KEYS=<live keys> e2e/testnet-return.sh plan <run>         # then live <run> deploy, as the script says
```
The key set (ascending addresses) and the threshold are fixed in the stand-in verifier at its deployment; in the
checkpoint state the key set is those addresses, left-padded to 32 bytes.

## Commands

The whole flow is [`e2e/testnet-return.sh`](../e2e/testnet-return.sh) (`plan`, `fork`, `live`, `prove`, `verify`).
The attestor on its own:

```sh
python3 -I lez-v03-return/standin.py selftest
python3 -I lez-v03-return/standin.py tip --rpc https://testnet.lez.logos.co            # the newest Finalized block
python3 -I lez-v03-return/standin.py layout --program <bridge program id> --config-id 0x<cid> --imported 2 --out layout.json
python3 -I lez-v03-return/standin.py wallet-burns --identity identity.json --seed-text "<the wallet's seed text>" \
    --recipient 0x<recipient> --check <the bridge driver's record> --out .       # burns.json and burn-notes.json
python3 -I lez-v03-return/standin.py checkpoint --identity identity.json --burns burns.json \
    --parent-state 0x<the vault's finalState> --signers 0x..,0x..,0x.. --threshold 2 \
    --rpc https://testnet.lez.logos.co --layout layout.json --out checkpoint.json
python3 -I lez-v03-return/standin.py sign --checkpoint checkpoint.json --verifier 0x<stand-in verifier> --keys DIR --out candidate.json
python3 -I lez-v03-return/test_standin.py                                               # 25 tests, no network
python3 -I lez-v03-return/test_attestor_checks.py                                       # 5 tests, no network
```

It cannot send anything: the LEZ client refuses every RPC method that is not a `get*` read, and no Ethereum
transaction is made here (the candidate is sent by `e2e/crown/sepolia.sh`, as in the crown run).
