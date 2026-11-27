# zone-bridge-rc3-erc20: an ERC-20 over the return leg of four LEZ v0.4.0-rc3 sequencers, three of four keys (RFP-022 F.7)

27 November 2026, the Modal build box: the same run as [`zone-bridge-rc3-multi`](../zone-bridge-rc3-multi/README.md)
(read it first: the zone, the attestors, the stand-ins and the stages are the same), with the deployment's 6-decimal
test ERC-20 (`TestToken`, "TUSD", `vault/script/DeployVault.s.sol`'s `_tokenParams`: denominations 100, 1,000 and
10,000 TUSD, relay fees 0.25 %, checkpoint fees 0.1 %) in place of ETH: `checkpoint/attestor/zone-bridge.sh` with
`ZB_TOKEN=test-token`, `ZB_LEZ` = LEZ v0.4.0-rc3, `ZB_CHECKPOINT_A=checkpoint_0_3_5 ZB_CHECKPOINT_B=checkpoint_b_0_3_5
ZB_N_MIN=4 ZB_T_MIN=3`, the RISC Zero 3.1 programs of [`docs/image-ids.md`](../../../docs/image-ids.md)
(`checkpoint_b_0_3_5_31` `0x8917f58e…`, `release_31` `0xf279af41…`), the bridge program `ferrata_bridge_b1`.

**The ERC-20 on both chains.** The vault (v5, mode (B), floors nMin 4, tMin 3, RISC Zero's verifier 3.1.0 behind its
emergency stop, on a local anvil with Sepolia's chain id) registers ETH and the test token; the two deposits of the run
are the token's (`crown.json`, `deploy.json`: the token at the deployer's creation 4, `0x5f35…f540`), each minted,
approved for exactly its total and deposited (`Crown.s.sol`). crown-driver (`--token test-token`) registers the token on
LEZ from the vault's own registry slot and schedule 0 (`Register test-token`, its entry hash the vault's), imports both
roots, claims deposit 0 publicly and deposit 1 privately (LEZ's real proof, 455.6 s), and burns 100 TUSD from the
public and from the private holding (the private burn proven for real, 533.7 s): `lez/session-b/`.

**The checkpoint and the proofs.** `zone-25-143` (118 blocks, 251 transactions; two burns, two imported leaves) takes
three of the four attestations of tip 143 (`checkpoint/attestor.json`: n = 4, t = 3); RISC Zero 3.1's executor matches
the native journal (22,449,087 user cycles: `checkpoint/exec.json`); two of four, an outsider, a repeated attestor and
an earlier round are refused natively and by the guest (`checkpoint/refusals/`). The checkpoint and both releases
proven on Modal's GPU through the bridge's prover client (`prover/`: killed while it waited for the checkpoint's proof,
resumed on the same call; 90.4 s, 232.4 s and 27.6 s; PROVER KILL-RESUME OK), each wrapped to a 260-byte Groth16 seal
with RISC Zero 3.1's selector `0xe71e6620`, each journal the statement's.

**The release of the ERC-20** (`vault-checks.json`, `logs/zone-bridge.log`): the checkpoint submitted and, after W,
final (the vault's final state the journal's, its burn root `0x851c…8653` accepted); both releases paid in the token to
the recipient their burns name, **99,900,000 base units each** (100 TUSD less the 0.1 % checkpoint fee; the relay fee
0, the release relayer being the recipient), checked by `balanceOf` before and after; each nullifier then used; each
release's and the checkpoint's mined calldata called again: refused (`NullifierAlreadyUsed` twice, `ParentNotFinal`).
The same seals in forge, `vault/test/real/ZoneBridgeRc3Erc20.t.sol` (fixture `zone-bridge-rc3-erc20.json`): the
checkpoint, then both real releases paying exact amounts and the escrow falling by them; another registered token and
another denomination refused by the real verifier (2 of 2). `verify`: the checkpoint input, journal and burns
re-derived from the recorded node answers and attestations, equal; every receipt verified.

`vault-attempt-1/`: the vault stage's first attempt, which stopped before its checks on a parsing error of the
script (an ERC-20 balance read as `cast` prints it, the number followed by its scientific notation; fixed); its records
are kept. **Stand-ins** as in every `zone-bridge.sh` run: the beacon chain on LEZ (a committee crown-driver holds),
the Ethereum side (anvil; W and the release delay warped), leader proofs in dev mode. Validity rests on t of n
accredited sequencer keys (rule (B)).
