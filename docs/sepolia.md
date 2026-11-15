# Live on Sepolia: the runs, the deployed vaults, and how to check them

The README's two sections on the Sepolia runs and deployments, moved here unchanged on 16 November 2026 (only the
links are rewritten for this folder).

## Live on Sepolia, and how to check it yourself

On 12 November 2026 (EVIDENCE rows 91–92) the whole round trip ran on the final programs of
[`docs/image-ids.md`](image-ids.md), Sepolia to LEZ and back, as `crown-live-3`, the second live round trip:
FerrataVault v5 deployed in mode (B) at [`0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c`](https://sepolia.etherscan.io/address/0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c),
the vault reviewers should check; LEZ's light client started from a fresh Sepolia checkpoint, inside the programs'
weak-subjectivity bound; a public and a private deposit, final on Sepolia, imported by the bridge program on a LEZ
v0.4.0-rc1 devnet from Sepolia's own finality updates and claimed, the private one with a real proof; a public and a
private burn; a rule (B) checkpoint over the devnet's blocks proven on a GPU, its Groth16 seal verified on Sepolia;
both releases paid. 13 transactions, 74.9M gas, two starts (one stop on the RPC endpoint's lagging reads, before the
vault, resumed), verify 14 of 14 ([`e2e/runs/crown-live-3/`](../e2e/runs/crown-live-3/README.md)). A start before it,
`crown-live-2`, stopped at its first stage with nothing sent: the bound refused its 38-day-old checkpoint
([`e2e/runs/crown-live-2/`](../e2e/runs/crown-live-2/README.md)). The first, `crown-live-1` (11 November, EVIDENCE rows
41–42), made the same round trip on earlier versions of these programs (`integration5`'s, built before guest builds
were path-independent) through the vault
[`0xfE3199fa2634413CbAC317371076Cae3EB2d9E54`](https://sepolia.etherscan.io/address/0xfE3199fa2634413CbAC317371076Cae3EB2d9E54)
([`e2e/runs/crown-live-1/`](../e2e/runs/crown-live-1/README.md)). The LEZ side was a devnet we ran (one validator): no
public LEZ network runs any of it ("Not audited. Not for production.", below).

Anyone can re-check it from the committed records against a JSON-RPC endpoint of their choice:
`python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-3` (13 passed on two public endpoints; `e2e/runs/crown-live-1`
the same). It checks every recorded transaction, the deployment's CREATE addresses, the vault's immutables (image ids,
configId, genesisState, rule (B), verifier, linked registry), the deposits' commitments, the vault storage LEZ imported
(Merkle proofs against Sepolia's final headers), the checkpoint journal against the LEZ records and its seal through
the vault's own verifier, finalization and releases (EVIDENCE row 92). The vault's programs rebuild from this tree:
`scripts/reproduce-image-ids.sh build`, then `compare --vault 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c` gives all
of its ids (6 of 6 crown programs, the vault's two immutables among them, and 14 of 14 guests).
[docs/guides/verify.md](guides/verify.md) says what each check proves and what it still trusts, and how to
rebuild the programs to their image ids.

## Not audited. Not for production.

No part of this repository has been audited. Three vaults are deployed on a public network, all on Sepolia.
FerrataVault v5 of `vault/` (rule (B)) with the final programs is at
[`0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c`](https://sepolia.etherscan.io/address/0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c)
(created by transaction `0xa67bfa471825f0dad32e8243444700635a08f39d85b328672f5959b122c12822`, block 12,119,409), with
its emergency stop (owner renounced) and library: the live round trip of 12 November 2026 made two deposits, a
checkpoint and two releases through it ([`e2e/runs/crown-live-3/`](../e2e/runs/crown-live-3/README.md), EVIDENCE row
91). The same v5 vault with the first run's programs is at
[`0xfE3199fa2634413CbAC317371076Cae3EB2d9E54`](https://sepolia.etherscan.io/address/0xfE3199fa2634413CbAC317371076Cae3EB2d9E54)
(transaction `0xee91c261265ac3ada50b525edb35a1faa32bb5bb4ffa2b1d433f5a278d9808a3`, block 12,111,048): the live round
trip of 11 November 2026 ([`e2e/runs/crown-live-1/`](../e2e/runs/crown-live-1/README.md), EVIDENCE row 41). Both have
testnet parameters (a 1-day revocation window, a 120 s release delay, no caps, no watchers, a freeze authority without
a key holder). Before them, the v3 vault of the end-to-end fixture was deployed on Sepolia at
[`0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7`](https://sepolia.etherscan.io/address/0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7) (created by
transaction `0x0a5c3a64b6357c76e273f3ecd91ad58078f9c872f3ab90e68dba521de0589f12`, block 12,107,894), with one deposit, deposit 0
(transaction `0xf3235de7115886851c2a92b6471e1cdce9c8aa9fa3c39558d60cade17a5789a0`, block 12,107,901, 0.001 ETH).
Deposit 0 was claimed on LEZ by the plaintext path: on a LEZ v0.4.0-rc1 sequencer we run, from a capture
of Sepolia's finalized state, with the frozen bridge program, not this tree's
([`lez/live/inbound/README.md`](../lez/live/inbound/README.md), "The real session"). No public LEZ network runs
any of it: LEZ testnet 0.4 does not exist, and the round trip's LEZ side was a devnet we ran (one validator). The prototypes exist to measure and to test the design; the audit
is planned for the code the proposal would deliver. A security review by an agent in a separate context
(11 November 2026) is not an audit; its fixes are in this tree (EVIDENCE row 30), and the frozen bridge program
of the real session predates them and the time fix. Slither, Aderyn, Halmos, cargo-audit, cargo-deny and fuzzing
(EVIDENCE rows 35, 36, 40 and 78) are tooling, not an audit.
