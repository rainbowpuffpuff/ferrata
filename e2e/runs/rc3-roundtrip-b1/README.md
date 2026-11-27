# CI: the bridge round trip through a LEZ sequencer in standalone mode and a local fork of Sepolia

RFP-022 Reliability #10 (job `sequencer-roundtrip` of `.github/workflows/sequencer.yml`, script
`scripts/ci/sequencer-roundtrip.sh`): LEZ v0.4.0-rc3's `sequencer_service` built with its `standalone` feature (its
mock Bedrock actor; no Logos Blockchain node), blocks every 3 s; anvil forking Sepolia at its finalized block of
the moment. Stand-ins, exactly: the light client's committee (anvil has no beacon chain), the vault's verifier
(the vault tests' `MockRiscZeroVerifier`, accepting only the journals the checkpoint and release guests computed
in RISC Zero dev mode, each equal to the native one), the checkpoint's chain (the sequencer's applied
transactions inscribed by `ci_zone`'s test keys), the gas drop (`seqnet-fund`).

| | |
|---|---|
| Sepolia fork | block 12222587 (2026-11-27T09:50:12Z, 1141 s old when forked), hash `0xcbd44c6a4aa9e03fc219c69b8126ead8f1f11d098ad3a330f82760aab733bc61`, read from https://ethereum-sepolia-rpc.publicnode.com |
| vault | `0x67aC447386F2ea14eF2Fbf55eF33Ce1d8F386516` (rule B), configId `0xe7aab60e0bae94ca508d32b4a0f2c240ad549bc22b6232c30583a805fe021e0e` |
| deposit | 1000000000000000 wei, Ethereum tx `0xa1475029f09c37b5247d71902f78f1154c5ed77aa34376ea5b935a117fec40b3` |
| claim | LEZ tx `f20711077f69e30b80018c9ca9b2f5d1480669becc07bf9425ae6bdae97d7010`; holding 1000000000000000 |
| burn | LEZ tx `811b81ed2d195cc87e54e0237c8b0fac4128828dee18dae0041cebbb30d7a66b` to `0x5540889c6F8c7b7eD633BCF0857B5803F0b90785` |
| checkpoint | burn root `0x5d436b2eadb7ae56c9a124240cb43902df0a59134aae8d51f4e76f55088a294b`, 1 burn(s), tip 30 |
| release | Ethereum tx `0xaf0a125868f09a4d95521ac75af151b1564b0b8592980af8dc78efff64d497a7`; the fresh address holds 997000000000000 wei |

| step | result | seconds | detail |
|---|---|---:|---|
| fork | ok | 2 | Sepolia block 12222587 (2026-11-27T09:50:12Z, 1141 s old), hash 0xcbd44c6a4aa9e03fc219c69b8126ead8f1f11d098ad3a330f82760aab733bc61, from https://ethereum-sepolia-rpc.publicnode.com; anvil forks it, chain id 11155111 |
| zone: programs and the stand-in instance | ok | 56 | attestation program 0x816d1fef46… and bridge program 0x847a4336c2… deployed through LEZ's loader; stand-in instance 0xa5000dc547… (bootstrap slot 11665472); zone tip 20 |
| vault deployed on the fork | ok | 9 | vault 0x67aC447386F2ea14eF2Fbf55eF33Ce1d8F386516 (rule B), configId 0xe7aab60e0bae94ca508d32b4a0f2c240ad549bc22b6232c30583a805fe021e0e, genesisState 0x8e54c7d6a8de29730ff8874b4f6ab32653c3d6f85274cc44fa19057391a3efb8 (ci_zone's test committee); RISC Zero's Groth16 verifier 0x25Fa4286d401bdc4AC7657bc7a1ebdE1B737F4b1 (code hash 0x47dfdcd8969f76d298cd5c4bc1f2889a6bc042b8c0f1e428f526cce1316b9970) deployed by DeployVault, its code then replaced on the fork by MockRiscZeroVerifier's; checkpoint guest 0xdafa6ab9a26841681fd071b8e4cdfae514371f82902426dc9fb1e2c80d29cda0, release guest 0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd |
| deposit | ok | 1 | 0.001 ETH to the user's public /0, Ethereum tx 0xa1475029f09c37b5247d71902f78f1154c5ed77aa34376ea5b935a117fec40b3; the vault's tree holds 1 leaf |
| operator import | ok | 20 | the stand-in committee finalized the fork's block 12222596; bridge Init (pairing: configId in the vault's slot 2), bridge Register ETH (registry entry, schedules 0 and 1, leaf count), bridge ImportRoot n=1 (stage 1, block 12222596): 3 of 3 applied |
| fees | ok | 2 | native LEZ to the user's public /0 (0x8a7162a5acc0f2c0beab5a9d07dfab129e7dc9b18cd9c14b3090fe2cc40154b0): 3000000000000 |
| claim (plaintext) | ok | 6 | LEZ tx f20711077f69e30b80018c9ca9b2f5d1480669becc07bf9425ae6bdae97d7010 applied; the user's public /0 holds 1000000000000000 (0.001 ETH) |
| claim again refused | ok | 0 | refused: error: refused: LEZ bridge program exit code 250 ALREADY_CLAIMED (checked before sending: the zone's history shows it claimed): the deposit's nullifier is spent; the holding unchanged |
| burn | ok | 4 | LEZ tx 811b81ed2d195cc87e54e0237c8b0fac4128828dee18dae0041cebbb30d7a66b applied: 0.001 ETH burnt to 0x5540889c6F8c7b7eD633BCF0857B5803F0b90785; the holding is 0 |
| checkpoint journal | ok | 5 | 34 applied zone transactions in 28 blocks; the guest's journal equals the native one ({"executor_wall_s":0.414694191,"image_id":"0xdafa6ab9a26841681fd071b8e4cdfae514371f82902426dc9fb1e2c80d29cda0","input_by); 1 burn, root 0x5d436b2eadb7ae56c9a124240cb43902df0a59134aae8d51f4e76f55088a294b |
| checkpoint accepted | ok | 1 | submitCandidate, Delta let pass, finalizeAttested by ferrata-relayer; finalState 0x977c666a35577854ab991f2c1b1efbe9f5ce07a9ca864dc09bdce01926751b2a; burn root accepted |
| release journal | ok | 1 | release guest 0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd: journal equal to the native one (RISC0_DEV_MODE: executed, receipt fake) |
| release paid | ok | 1 | Ethereum tx 0xaf0a125868f09a4d95521ac75af151b1564b0b8592980af8dc78efff64d497a7: the Release event pays out the burnt 1000000000000000 wei: 997000000000000 to the fresh address, 2000000000000 relay fee to the relayer, 1000000000000 credited (OwedCredited: the checkpoint fee) |
| release again refused | ok | 0 | eth_call of the same release reverts with NullifierAlreadyUsed (0xcad2ae02); nullifierUsed(0x35d949b175073069474e3c8810e4619f848a58f62056c69dc3963d54ed00d248) = true |

- records: `run/` (steps.tsv, facts.json, the vault's identity, params and deploy record, `lez/` the zone's
  transactions, `eth/` the fork's, `user/` what the user ran and saw, `checkpoint/`), `zone/` (the sequencer's
  configuration, logs and environment), `sequencer-roundtrip.log`

