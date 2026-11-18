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
| [`test_standin.py`](test_standin.py) | 20 tests against a local stub of the node's RPC that serves three real testnet blocks and an accumulator computed natively |
| [`fixtures/`](fixtures/) | three real blocks of the public LEZ testnet (83000, 83001 Finalized; 83002 Pending), read on 18 November 2026 |

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

Before it signs, `standin.py checkpoint`:

1. reads the bridge's accumulator between two reads of the head block id that agree, so that it is the state after
   that block, and takes that block (or a later one given with `--block`) as the tip;
2. requires the node to report the tip `Finalized`. A `Pending` or `Safe` tip is refused; `--allow-pending` records it
   anyway and writes `final_on_node: false` into the record, which every later document of the run then shows;
3. recomputes the tip's hash from the block's own bytes and requires it to link to its parent's hash;
4. recomputes every burn leaf, the root and the frontier from the listed burns and requires the node's count, root
   (or frontier) and configuration to equal them (and, with `--deposit-root`, the deposit root the bridge imported to
   equal the vault's).

What it does **not** check, and why the result is a stand-in:

- **the inscription itself.** It trusts one node's flag, the node Logos runs;
- **validity.** It executes nothing: the accumulator is what that node says the state is;
- **the node.** One RPC endpoint, over TLS, no second source.

On 18 November 2026 the public testnet's newest `Finalized` block was 83001 (17 November, 03:13 UTC), with five
`Pending` blocks after it produced four to ten hours apart. A burn made there is final when that network makes it so;
the Ethereum side adds about five minutes after the attestation is submitted.

## The keys

`standin.py keygen --dir DIR` writes fresh secp256k1 keys, one file each, mode 600, and prints only their addresses.
The keys of the testnet run are outside the repository; `e2e/testnet-return.sh` refuses a key directory inside it.
The key set (ascending addresses) and the threshold are fixed in the stand-in verifier at its deployment; in the
checkpoint state the key set is those addresses, left-padded to 32 bytes.

## Commands

The whole flow is [`e2e/testnet-return.sh`](../e2e/testnet-return.sh) (`plan`, `fork`, `live`, `prove`, `verify`).
The attestor on its own:

```sh
python3 -I lez-v03-return/standin.py selftest
python3 -I lez-v03-return/standin.py tip --rpc https://testnet.lez.logos.co            # the newest Finalized block
python3 -I lez-v03-return/standin.py layout --program <bridge program id> --config-id 0x<cid> --imported 2 --out layout.json
python3 -I lez-v03-return/standin.py burns-from-notes --identity identity.json --notes burn-notes.json --out burns.json
python3 -I lez-v03-return/standin.py checkpoint --identity identity.json --burns burns.json \
    --parent-state 0x<the vault's finalState> --signers 0x..,0x..,0x.. --threshold 2 \
    --rpc https://testnet.lez.logos.co --layout layout.json --out checkpoint.json
python3 -I lez-v03-return/standin.py sign --checkpoint checkpoint.json --verifier 0x<stand-in verifier> --keys DIR --out candidate.json
python3 -I lez-v03-return/test_standin.py                                               # 20 tests, no network
```

It cannot send anything: the LEZ client refuses every RPC method that is not a `get*` read, and no Ethereum
transaction is made here (the candidate is sent by `e2e/crown/sepolia.sh`, as in the crown run).
