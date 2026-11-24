# CI: the bridge round trip through a LEZ sequencer in standalone mode and a local fork of Sepolia

RFP-022 Reliability #10 (job `sequencer-roundtrip` of `.github/workflows/sequencer.yml`, script
`scripts/ci/sequencer-roundtrip.sh`): LEZ v0.4.0-rc1's `sequencer_service` built with its `standalone` feature (its
mock Bedrock actor; no Logos Blockchain node), blocks every 3 s; anvil forking Sepolia at its finalized block of
the moment. Stand-ins, exactly: the light client's committee (anvil has no beacon chain), the vault's verifier
(the vault tests' `MockRiscZeroVerifier`, accepting only the journals the checkpoint and release guests computed
in RISC Zero dev mode, each equal to the native one), the checkpoint's chain (the sequencer's applied
transactions inscribed by `ci_zone`'s test keys), the gas drop (`seqnet-fund`).

| | |
|---|---|
| Sepolia fork | block 12203095 (2026-11-24T14:12:24Z, 957 s old when forked), hash `0x0a100fa0511b0abca17d306e02a437ff3499fddfd0ae05ba7fff02c04c308731`, read from https://ethereum-sepolia-rpc.publicnode.com |
| vault | `0x67aC447386F2ea14eF2Fbf55eF33Ce1d8F386516` (rule B), configId `0xd38bff1d95d9441377a6f2ff257f4b5f060614214d5af3f196b8a39f0b8311e6` |
| deposit | 1000000000000000 wei, Ethereum tx `0x5bc73d2a3ae9dc9a0fae4d10525801ae2d42741b19be27658254ef73ca7ca200` |
| claim | LEZ tx `ad02e144dede462189601665e2bbab5e02ef6a3315636e5c9a53fdfdc1775ea2`; holding 1000000000000000 |
| burn | LEZ tx `5f854d592a8e5e1dba1b86272c1db93d10ed081921e35970773be3e747b795e7` to `0xC5318b5f799EDDec797e4fF8502EF6f69a2398bC` |
| checkpoint | burn root `0x5ec74bb264f97bbc0dc38c1dc5fef1db6d143b7682043bcb6c73d70a5286fecc`, 1 burn(s), tip 30 |
| release | Ethereum tx `0x83961a9bea86f95704ad048fd749dc4bfff3eef44e4fe36d972ffce7fafa5041`; the fresh address holds 997000000000000 wei |

| step | result | seconds | detail |
|---|---|---:|---|
| fork | ok | 3 | Sepolia block 12203095 (2026-11-24T14:12:24Z, 957 s old), hash 0x0a100fa0511b0abca17d306e02a437ff3499fddfd0ae05ba7fff02c04c308731, from https://ethereum-sepolia-rpc.publicnode.com; anvil forks it, chain id 11155111 |
| zone: programs and the stand-in instance | ok | 58 | attestation program 0x816d1fef46… and bridge program 0x847a4336c2… deployed through LEZ's loader; stand-in instance 0x7725154af4… (bootstrap slot 11649088); zone tip 21 |
| vault deployed on the fork | ok | 10 | vault 0x67aC447386F2ea14eF2Fbf55eF33Ce1d8F386516 (rule B), configId 0xd38bff1d95d9441377a6f2ff257f4b5f060614214d5af3f196b8a39f0b8311e6, genesisState 0x8e54c7d6a8de29730ff8874b4f6ab32653c3d6f85274cc44fa19057391a3efb8 (ci_zone's test committee); RISC Zero's Groth16 verifier 0x25Fa4286d401bdc4AC7657bc7a1ebdE1B737F4b1 (code hash 0x47dfdcd8969f76d298cd5c4bc1f2889a6bc042b8c0f1e428f526cce1316b9970) deployed by DeployVault, its code then replaced on the fork by MockRiscZeroVerifier's; checkpoint guest 0xdafa6ab9a26841681fd071b8e4cdfae514371f82902426dc9fb1e2c80d29cda0, release guest 0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd |
| deposit | ok | 1 | 0.001 ETH to the user's public /0, Ethereum tx 0x5bc73d2a3ae9dc9a0fae4d10525801ae2d42741b19be27658254ef73ca7ca200; the vault's tree holds 1 leaf |
| operator import | ok | 22 | the stand-in committee finalized the fork's block 12203104; bridge Init (pairing: configId in the vault's slot 2), bridge Register ETH (registry entry, schedules 0 and 1, leaf count), bridge ImportRoot n=1 (stage 1, block 12203104): 3 of 3 applied |
| fees | ok | 2 | native LEZ to the user's public /0 (0x8a7162a5acc0f2c0beab5a9d07dfab129e7dc9b18cd9c14b3090fe2cc40154b0): 3000000000000 |
| claim (plaintext) | ok | 6 | LEZ tx ad02e144dede462189601665e2bbab5e02ef6a3315636e5c9a53fdfdc1775ea2 applied; the user's public /0 holds 1000000000000000 (0.001 ETH) |
| claim again refused | ok | 1 | refused: error: refused: LEZ bridge program exit code 250 ALREADY_CLAIMED (checked before sending: the zone's history shows it claimed): the deposit's nullifier is spent; the holding unchanged |
| burn | ok | 5 | LEZ tx 5f854d592a8e5e1dba1b86272c1db93d10ed081921e35970773be3e747b795e7 applied: 0.001 ETH burnt to 0xC5318b5f799EDDec797e4fF8502EF6f69a2398bC; the holding is 0 |
| checkpoint journal | ok | 6 | 34 applied zone transactions in 28 blocks; the guest's journal equals the native one ({"executor_wall_s":0.482167856,"image_id":"0xdafa6ab9a26841681fd071b8e4cdfae514371f82902426dc9fb1e2c80d29cda0","input_by); 1 burn, root 0x5ec74bb264f97bbc0dc38c1dc5fef1db6d143b7682043bcb6c73d70a5286fecc |
| checkpoint accepted | ok | 1 | submitCandidate, Delta let pass, finalizeAttested by ferrata-relayer; finalState 0x30bd250244169c2c23da06e6f29f79c2e1036acf843eef6dcbe8cb152d6162e4; burn root accepted |
| release journal | ok | 1 | release guest 0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd: journal equal to the native one (RISC0_DEV_MODE: executed, receipt fake) |
| release paid | ok | 1 | Ethereum tx 0x83961a9bea86f95704ad048fd749dc4bfff3eef44e4fe36d972ffce7fafa5041: the Release event pays out the burnt 1000000000000000 wei: 997000000000000 to the fresh address, 2000000000000 relay fee to the relayer, 1000000000000 credited (OwedCredited: the checkpoint fee) |
| release again refused | ok | 0 | eth_call of the same release reverts with NullifierAlreadyUsed (0xcad2ae02); nullifierUsed(0x7c3fea11dd42d7a42f76c0614c661606aa58fb09a8f8cafff2c271907c0068c0) = true |

- records: `run/` (steps.tsv, facts.json, the vault's identity, params and deploy record, `lez/` the zone's
  transactions, `eth/` the fork's, `user/` what the user ran and saw, `checkpoint/`), `zone/` (the sequencer's
  configuration, logs and environment), `sequencer-roundtrip.log`

