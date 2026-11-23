# Live on Sepolia: the runs, the deployed vaults, and how to check them

The README's two sections on the Sepolia runs and deployments, moved here unchanged on 16 November 2026 (only the
links are rewritten for this folder).

**Security note, 21 November 2026.** The four vaults below verify through our own deployment of RISC Zero's Groth16
verifier 3.0.0 (`0x8f0A…A2A0`), which accepts forged claims (RISC Zero's advisory GHSA-mm6c-35cm-g4x2, 11 November
2026). RISC Zero's emergency stop does not cover our deployment; the vaults' own stops have no owner with a key, their
freeze authority no key, and they had no caps. Read at block 12,183,432: a forgery could take 0.001 Sepolia ETH (the v3
vault's deposit 0) and nothing from the three v5 vaults, whose escrow is below their smallest denomination. Later that
day the deployer's admin key set every release and deposit cap of the four to zero (the deployer's nonces 49–56, read
back at block 12,184,587), which closes their releases and deposits and the v5 vaults' refunds. The runs below
happened and their records stand, but a seal's acceptance by these verifiers shows only that it is well-formed: [security note](security/risc0-groth16-advisory-2026-11-21/README.md).

**The RISC Zero 3.1 vault, 23 November 2026.** Vault [`0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787`](https://sepolia.etherscan.io/address/0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787) (rule (B),
for our test network's zone), on RISC Zero's `RiscZeroGroth16Verifier` 3.1.0 `0x43C858f02f209c6C10c06B64B4ab2Cefd8B8dc96` behind RISC Zero's emergency
stop `0xC15916BDC7b32951B0310e0E24F98764EbA4b314` (owner renounced); registry `0x03953f8888f2666f5DF572551a3141b4B94e0C13`; test token `0x5d4DdB1863d2EE85765499dd850a9AeedecA03bC`. Its record, read-only check (47 of 47) and
Sourcify verification: EVIDENCE row 232. It supersedes the four vaults below, which stay capped at zero.

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

On 18 November 2026 (EVIDENCE rows 141–142) the same round trip ran live with the LEZ side on LEZ v0.4.0-rc3, as
`crown-live-4`: rc3's own `sequencer_service` and its Logos Blockchain node 0.3.5-rc.1 (one sequencer, one validator,
dev-mode leader proofs, a zone we ran), the same LEZ programs, LEZ's light client started from the fresh checkpoint
`sepolia-2026-11-18`, and a fourth vault, FerrataVault v5 in mode (B) at
[`0x80555Fe53CCcEf46951B23A3c75643a4C9283470`](https://sepolia.etherscan.io/address/0x80555Fe53CCcEf46951B23A3c75643a4C9283470),
pinning the second build of the checkpoint guests (`checkpoint_b_0_3_5` `0xafbf4023…3bcf`,
[`image-ids.md`](image-ids.md)), which reads the fork digest every transaction of that node carries; the earlier vaults'
checkpoint ids cannot follow such a zone. 13 transactions (nonces 36–48), 74.9M gas, four starts (three stops on the
public endpoint's lagging reads, each resumed), both private LEZ transactions and the checkpoint proven for real, both
releases paid, verify 14 of 14 ([`e2e/runs/crown-live-4/`](../e2e/runs/crown-live-4/README.md)), rehearsed first on a
fork of Sepolia ([`e2e/runs/crown-rc3-rehearsal-1/`](../e2e/runs/crown-rc3-rehearsal-1/README.md)).
`python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-4`: 13 passed on two public endpoints.

Anyone can re-check it from the committed records against a JSON-RPC endpoint of their choice:
`python3 -I scripts/verify-sepolia.py e2e/runs/crown-live-3` (13 passed on two public endpoints; `e2e/runs/crown-live-1`
the same). It checks every recorded transaction, the deployment's CREATE addresses, the vault's immutables (image ids,
configId, genesisState, rule (B), verifier, linked registry), the deposits' commitments, the vault storage LEZ imported
(Merkle proofs against Sepolia's final headers), the checkpoint journal against the LEZ records and its seal through
the vault's own verifier, finalization and releases (EVIDENCE row 92). The vault's programs rebuild from this tree:
`scripts/reproduce-image-ids.sh build`, then `compare --vault 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c` gives all
of its ids (6 of 6 crown programs, the vault's two immutables among them, and 14 of 14 guests) on a checkout of
`f13faee2`; since milestone B1 this tree's bridge program is B1's (`docs/image-ids.md` `ferrata_bridge_b1`), so on a
later tree `compare` gives the other five and the vault's two immutables equal, the bridge equal to its own record, and
14 of 14 guests ([docs/guides/verify.md](guides/verify.md) §2).
[docs/guides/verify.md](guides/verify.md) says what each check proves and what it still trusts, and how to
rebuild the programs to their image ids.

**The same seals on Ethereum mainnet, read-only (EVIDENCE row 149; superseded as evidence on 21 November 2026:
verifier 3.0.0 accepts forged claims and RISC Zero has emergency-stopped it, so this shows that our seals are
well-formed for the 3.0.0 circuits, not that a forged seal would be refused).** On Sepolia the vaults check seals with our own
deployment of RISC Zero's Groth16 verifier (`0x8f0A…`). On 18 November 2026 the crown-live-3 checkpoint, its two releases
and every other Groth16 seal the tree records with its journal (66 in all) were also checked against RISC Zero's own
contracts on Ethereum mainnet, the ones risc0-ethereum v3.0.0's `deployment.toml` names: the router
`0x8EaB2D97Dfce405A1692a21b3ff3A172d593D319` and the Groth16 verifier 3.0.0 `0x2a098988600d87650Fb061FfAff08B97149Fa84D`.
The checks were `eth_call` at block 26,443,464 on two public endpoints. All 66 verify, and each seal, image id or
journal with one byte changed reverts. Nothing is deployed on mainnet and nothing was sent:
`python3 -I scripts/verify-mainnet-risc0.py` ([`e2e/runs/mainnet-risc0-verify/`](../e2e/runs/mainnet-risc0-verify/README.md)).

## Not audited. Not for production.

No part of this repository has been audited. Four vaults are deployed on a public network, all on Sepolia.
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
a key holder); with verifier 3.0.0 known to accept forged claims, that leaves the admin key as the only control (the
security note above). Before them, the v3 vault of the end-to-end fixture was deployed on Sepolia at
[`0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7`](https://sepolia.etherscan.io/address/0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7) (created by
transaction `0x0a5c3a64b6357c76e273f3ecd91ad58078f9c872f3ab90e68dba521de0589f12`, block 12,107,894), with one deposit, deposit 0
(transaction `0xf3235de7115886851c2a92b6471e1cdce9c8aa9fa3c39558d60cade17a5789a0`, block 12,107,901, 0.001 ETH).
Deposit 0 was claimed on LEZ by the plaintext path: on a LEZ v0.4.0-rc1 sequencer we run, from a capture
of Sepolia's finalized state, with the frozen bridge program, not this tree's
([`lez/live/inbound/README.md`](../lez/live/inbound/README.md), "The real session"). On a public LEZ network only the
light client's experimental v0.3.1 port is deployed, on the public LEZ testnet, where it attested slots 0 and 1 of the
v5 vault `0x04eE…e44c` above (EVIDENCE row 150); none of the proposal's v0.4 programs is: LEZ testnet 0.4 does not
exist, and the round trip's LEZ side was a devnet we ran (one validator). The prototypes exist to measure and to test
the design; the audit is planned for the code the proposal would deliver. A security review by an agent in a separate
context (11 November 2026) is not an audit; its fixes are in this tree (EVIDENCE row 30), and the frozen bridge
program of the real session predates them and the time fix. Slither, Aderyn, Halmos, cargo-audit, cargo-deny and
fuzzing (EVIDENCE rows 35, 36, 40 and 78) are tooling, not an audit.

**Source verification.** Since 16 November 2026 every contract of the three deployments is verified on Sourcify as an
exact match, compiler metadata hash included, and Blockscout shows each as fully verified: the two v5 vaults
([`0x04eE…`](https://repo.sourcify.dev/11155111/0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c),
[`0xfE31…`](https://repo.sourcify.dev/11155111/0xfE3199fa2634413CbAC317371076Cae3EB2d9E54)) with the registry library each is linked to
([`0xa75e…`](https://repo.sourcify.dev/11155111/0xa75e653BdaD23d198Ea357277b715aD8A9687673),
[`0xe5D7…`](https://repo.sourcify.dev/11155111/0xe5D797C9f585a9c1b5e530814f9F25bED24a4B15)), the v3 vault
([`0xd0B5…`](https://repo.sourcify.dev/11155111/0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7), from the tree it was deployed from, `c6aee464`), the
three test tokens, the three emergency stops and the Groth16 verifier
([`0x8f0A…`](https://repo.sourcify.dev/11155111/0x8f0Af3Bf302C64d57f840127F94d9c123a8Fa2A0)): 12 of 12. Re-check, sending nothing:
`vault/script/sourcify.sh status`, and the same with `FERRATA_FIXTURE=../e2e/runs/crown-live-1/deploy.json` (EVIDENCE
row 122). Etherscan keeps its own records and shows the same twelve as verified since that day, the three vaults as
exact matches (eight contracts by their source, four by its match to an identical contract among them). Its page for a
v5 vault adds "Heuristics suggest this may be a proxy contract": the vault calls its linked library by DELEGATECALL at
an address fixed in its bytecode. It is not a proxy and the address cannot be changed.
The fourth deployment (`crown-live-4`, 18 November 2026), read on Sourcify the same day
(`FERRATA_FIXTURE=../e2e/runs/crown-live-4/deploy.json vault/script/sourcify.sh status`, read-only, 20:48Z;
[`e2e/runs/crown-live-4/sourcify-status.txt`](../e2e/runs/crown-live-4/sourcify-status.txt)): its vault
[`0x8055…`](https://repo.sourcify.dev/11155111/0x80555Fe53CCcEf46951B23A3c75643a4C9283470), its registry library
`0x837F…` and its emergency stop `0xB249…` are exact matches, and its test token `0xD599…` a match under another
contract's name (`MockUSDT`: Sourcify's own match to identical code somebody verified); Etherscan not read.
