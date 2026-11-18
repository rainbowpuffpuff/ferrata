# testnet-return-fork-2: the stand-in return leg, rehearsed again with the attestor's checks (18 Nov 2026)

The second rehearsal of the leg back from the public LEZ testnet (LEZ v0.3.1), run by
[`e2e/testnet-return.sh fork`](../../testnet-return.sh) on a local anvil fork of Sepolia at block 12,162,297, after an
independent review asked that the attestor sign only what it has checked (findings A1 to A6,
[`lez-v03-return/README.md`](../../../lez-v03-return/README.md)). **Nothing was sent to Sepolia, and nothing was read
from or sent to the LEZ testnet.** The arrangement is a testnet stand-in, not Ferrata's design
([`vault/src/standin/README.md`](../../../vault/src/standin/README.md), [`docs/trust.md`](../../../docs/trust.md) §3.6).
[`testnet-return-fork-1`](../testnet-return-fork-1/README.md) is kept as it was; what is the same is described there.

**Real, as in fork-1:** the unchanged FerrataVault (rule (B)) and FerrataRegistry; RISC Zero's Groth16 verifier and
emergency stop, Sepolia's own contracts on the fork; two deposits; two proofs of the unchanged `release` program (image
`0xb64900bc…29e9`), each proven on a CPU, wrapped to Groth16 and accepted by that verifier; both payments.

**Stand-in, as in fork-1:** the burn root's attestation (two of three keys we hold), the burns (a labelled test wallet,
computed natively), `lezConfigId` (a labelled placeholder), the deposits' destinations, the chain (a fork; the deployer
`0x79A9…5C07` impersonated at its real nonce 36).

## What is new: the attestor checked the bridge's state, at a Finalized tip below the head

The LEZ side was a **local stub node** ([`lez-v03-return/stub_node.py`](../../../lez-v03-return/stub_node.py)), a
stand-in in its own right:

- it served **real recorded blocks** of the public LEZ testnet, 83000 to 83003 (read on 18 November; 83000 and 83001
  Finalized, 83002 and 83003 Pending; `lez-v03-return/fixtures/`);
- it served a **placed bridge state** (`checkpoint/stub-node-state.json`): lez-v03-bridge's `burns(cid)` holding the two
  native burns and `root(cid, 2)` holding the forked vault's deposit root, under a labelled program id
  (`2Xw45ovqWvSzamTWNhc2t7ovad3MLoJFsqSeRjzsya4r`, the sha256 of a label). No block it served holds those burns.

Against it, `standin.py checkpoint` ran every check the review asked for (`checkpoint/candidate.json`, `source`):

| Check | Result |
|---|---|
| the burns compared with the bridge's accumulator on the node (count, frontier, configuration), layout derived from the program id (`checkpoint/layout.json`) | equal: `accumulator_checked_against_node: true` |
| the deposit root LEZ imported against the vault's `depositRoot()` | equal: `imported_root_is_the_vaults: true` |
| the head block read before and after the accumulator, same id and hash | block 83003 both times |
| the tip | block **83001**, `Finalized` on the node, its hash recomputed, linked to 83000: `final_on_node: true` |
| the blocks between the tip and the read (83002 and 83003, both Pending), read in full | 2 and 9 public transactions, none naming the bridge program or an accumulator account (`blocks_between_tip_and_read`) |

So the checkpoint was cut at a Finalized block **below** the head, which fork-1's attestor could not do (it needed the
head itself Finalized). `standin.py sign` then signed (two of the rehearsal's keys), and would have refused had either
check been missing from `source`; `verify.py` re-checked both flags.

The stand-in keys are the rehearsal's (`0x18db…288a`, `0xb1b4…b5c3`, `0xdd8d…fe21`, as in fork-1). The live run takes
three fresh keys, which have signed nothing; `e2e/testnet-return.sh live` refuses a plan whose signers signed for
another run of this leg.

## The transactions and their gas

Ten transactions, all by the deployer, **11,458,629 gas** as anvil counts it (`manifest.json`, `gas.tsv`); the same
addresses as fork-1 (the deployer's nonce was still 36): stand-in `0xB24905b767c1b50dDe6b1B33aDb119CA7E2dEfCA`, registry
`0x837F715d57299799C64dcF5ba84525319D5EeAfe`, vault `0x80555Fe53CCcEf46951B23A3c75643a4C9283470`.

| # | Stage | Gas on the fork |
|---|---|---:|
| 1-4 | deploy: the stand-in, the registry, the vault, `registerToken(ETH)` | 803,966; 2,757,365; 5,689,775; 709,314 |
| 5-6 | deposit-0, deposit-1 (each through the deposit guard: no pending transaction, a marker before the send) | 200,560; 151,406 |
| 7 | submit: the journal with a 130-byte seal of two signatures | 165,500 |
| 8 | finalizeAttested, after 120 s | 221,505 |
| 9-10 | release-0, release-1, each with a real Groth16 seal, after the 120 s release delay | 396,713; 362,525 |

Sepolia's own figures for the same transactions (EIP-8037) are in fork-1's table: about 71 M gas in all. From the fork
up to the last check: 15:20:29Z to 15:22:09Z.

## The release proofs

Made before the fork, by `e2e/testnet-return.sh prove` (CPU proofs on the Modal build box, RISC Zero's Groth16 prover
image through the build box app):

| Step | public-0 | private-1 | Where |
|---|---:|---:|---|
| executor: user cycles, segments | 77,736, 1 | 77,736, 1 | `releases/*.exec.json` |
| succinct proof, `r0vm` on a CPU (32 threads) | 12.4 s | 12.5 s | `releases/*.prove.json` |
| `identity_p254` | 15.9 s | 15.7 s | `releases/*.p254.json` |
| RISC Zero's Groth16 prover, its proving step | 7.3 s | 7.7 s | the prover's own log (not committed) |
| seal | 260 bytes | 260 bytes | `releases/receipts/*/seal.hex` |

## What was refused, and the check

`refusals.json`: the same 28 `eth_call`s as fork-1, each answered as expected (below threshold, a duplicated signer,
another journal, releases before and during the candidate, finalisation before the wait, replays, an unsigned root,
signatures as a proof, a proof for another journal). `verify-fork.txt`: `lez-v03-return/verify.py check --fork`,
**10 of 10**.

## How it was run

```sh
# reads only: the plan, with the stub node serving the recorded blocks (genesis at block 83001)
FERRATA_LEZ_RPC=http://127.0.0.1:<port> FERRATA_STANDIN_KEYS=<rehearsal keys> e2e/testnet-return.sh plan testnet-return-fork-2
python3 -I lez-v03-return/standin.py native-burns --identity e2e/runs/testnet-return-fork-2/identity.json \
  --recipient 0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07 --out e2e/runs/testnet-return-fork-2/checkpoint
FERRATA_MRUN=<build box> FERRATA_MWRAP=<prover image> e2e/testnet-return.sh prove testnet-return-fork-2
FERRATA_STANDIN_KEYS=<rehearsal keys> e2e/testnet-return.sh fork testnet-return-fork-2   # on the build box
```

Files as in fork-1, plus `checkpoint/layout.json` and `checkpoint/stub-node-state.json`. Not committed: the stand-in
keys, the wallet's burn notes and the release inputs, the receipts' binaries, the logs.
