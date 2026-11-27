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
| Sepolia fork | block 12220225 (2026-11-27T01:24:36Z, 1003 s old when forked), hash `0x41406f5730fa07a92e7a1a0c813be88939cf125384be90e446086517dc936601`, read from https://ethereum-sepolia-rpc.publicnode.com |
| vault | `0x67aC447386F2ea14eF2Fbf55eF33Ce1d8F386516` (rule B), configId `0x3be8f71ad9d2b61c67e99d045de817fc67e91023e7af45c68693a7eaca844c96` |
| deposit | 1000000000000000 wei, Ethereum tx `0x2705d2c4bb064e645133f15858403cf4ee483184b0601a329a0db56137c89419` |
| claim | LEZ tx `ec31a6ad3cce52d57ee2cd3bd74b297108ca7a097b5e093dce879063767011d2`; holding 1000000000000000 |
| burn | LEZ tx `610d32757de5a313f31499c4e98fd3ab7e6f2ad94a93d50a48e53e62c45efdf4` to `0x28E77c15720bD320D1879B5F86eF86D526565c01` |
| checkpoint | burn root `0xd56c121cb25ccd49feda76e95e1228d5727a116a968ea3d78317da28024ccfee`, 1 burn(s), tip 35 |
| release | Ethereum tx `0xa85bf2c9354abe648509eefb193a57263b661d6213a6905d9ab2fb89950041af`; the fresh address holds 997000000000000 wei |

| step | result | seconds | detail |
|---|---|---:|---|
| fork | ok | 1 | Sepolia block 12220225 (2026-11-27T01:24:36Z, 1003 s old), hash 0x41406f5730fa07a92e7a1a0c813be88939cf125384be90e446086517dc936601, from https://ethereum-sepolia-rpc.publicnode.com; anvil forks it, chain id 11155111 |
| zone: programs and the stand-in instance | ok | 64 | attestation program 0x816d1fef46… and bridge program 0xffb47f6eae… deployed through LEZ's loader; stand-in instance 0xa5000dc547… (bootstrap slot 11665472); zone tip 22 |
| vault deployed on the fork | ok | 12 | vault 0x67aC447386F2ea14eF2Fbf55eF33Ce1d8F386516 (rule B), configId 0x3be8f71ad9d2b61c67e99d045de817fc67e91023e7af45c68693a7eaca844c96, genesisState 0x8e54c7d6a8de29730ff8874b4f6ab32653c3d6f85274cc44fa19057391a3efb8 (ci_zone's test committee); RISC Zero's Groth16 verifier 0x25Fa4286d401bdc4AC7657bc7a1ebdE1B737F4b1 (code hash 0x47dfdcd8969f76d298cd5c4bc1f2889a6bc042b8c0f1e428f526cce1316b9970) deployed by DeployVault, its code then replaced on the fork by MockRiscZeroVerifier's; checkpoint guest 0xdafa6ab9a26841681fd071b8e4cdfae514371f82902426dc9fb1e2c80d29cda0, release guest 0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd |
| deposit under schedule 0 | ok | 2 | Ethereum tx 0x2705d2c4bb064e645133f15858403cf4ee483184b0601a329a0db56137c89419: 0.001 ETH to the user's public /0; escrow +1002000000000000 wei (the denomination and the relay fee 2000000000000), the fee recipient's credit +0; the vault captured |
| fee change | ok | 1 | announceFees (mint fees 5000000000000 50000000000000 500000000000000 wei; burn 0 0 0, checkpoint 1000000000000 10000000000000 100000000000000 unchanged); the seven days passed on the fork's clock; schedule 1 in force: denomination 0 pays mint fee 5000000000000 and relay fee 2000000000000 |
| deposit under schedule 1 | ok | 1 | Ethereum tx 0x67313d6ef3c63b6fe44c792172bffaa04f4cb18359b44d237bd6ec3266dcf514: escrow +1002000000000000 wei (the denomination and the relay fee, as under schedule 0); the fee recipient 0x000000000000000000000000000000005e9de901 credited 5000000000000 wei at the deposit (OwedCredited); the vault captured |
| operator import | ok | 47 | the stand-in committee finalized the fork's blocks 12220234 and 12220238; bridge Init (pairing: configId in the vault's slot 2), bridge Register ETH (registry entry, schedules 0 and 1, leaf count), bridge ImportRoot n=1 (stage 1, block 12220234), bridge ImportRoot n=2 (stage 2, block 12220238): 4 of 4 applied |
| fees | ok | 4 | native LEZ to the user's public /0 (0x8a7162a5acc0f2c0beab5a9d07dfab129e7dc9b18cd9c14b3090fe2cc40154b0): 3000000000000 |
| claim of leaf 0 (schedule 0) | ok | 8 | LEZ tx ec31a6ad3cce52d57ee2cd3bd74b297108ca7a097b5e093dce879063767011d2 applied: the user's public /0 holds 1000000000000000 (the denomination), the relayer /1 2000000000000 (the relay fee) |
| claim of leaf 1 (schedule 1) | ok | 5 | LEZ tx f3bee10ece2911dc24b6c3278394a3101790063d2877ed1473c188335a7869eb applied: /0 +1000000000000000 (the denomination), the relayer /1 +2000000000000 (the relay fee): the same amounts as the claim under schedule 0 |
| claim again refused | ok | 1 | refused: error: refused: LEZ bridge program exit code 250 ALREADY_CLAIMED (checked before sending: the zone's history shows it claimed): the deposit's nullifier is spent; the holding unchanged |
| burn | ok | 7 | LEZ tx 610d32757de5a313f31499c4e98fd3ab7e6f2ad94a93d50a48e53e62c45efdf4 applied: 0.001 ETH burnt to 0x28E77c15720bD320D1879B5F86eF86D526565c01; the holding is 1000000000000000 (the other claim's denomination) |
| checkpoint journal | ok | 8 | 39 applied zone transactions in 33 blocks; the guest's journal equals the native one ({"executor_wall_s":0.500734015,"image_id":"0xdafa6ab9a26841681fd071b8e4cdfae514371f82902426dc9fb1e2c80d29cda0","input_by); 1 burn, root 0xd56c121cb25ccd49feda76e95e1228d5727a116a968ea3d78317da28024ccfee |
| checkpoint accepted | ok | 0 | submitCandidate, Delta let pass, finalizeAttested by ferrata-relayer; finalState 0xb90413c6219d23fc13eb8c59d64692e1bc951d7c71cd90d8dc48fe071645a057; burn root accepted |
| release journal | ok | 1 | release guest 0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd: journal equal to the native one (RISC0_DEV_MODE: executed, receipt fake) |
| release paid | ok | 1 | Ethereum tx 0xa85bf2c9354abe648509eefb193a57263b661d6213a6905d9ab2fb89950041af: the Release event pays out the burnt 1000000000000000 wei: 997000000000000 to the fresh address, 2000000000000 relay fee to the relayer, 1000000000000 credited (OwedCredited: the checkpoint fee) |
| release again refused | ok | 0 | eth_call of the same release reverts with NullifierAlreadyUsed (0xcad2ae02); nullifierUsed(0x13bae80c15b269ee50e54aaf9767e62842b646f7a514fb14719426e0301bce79) = true |

- records: `run/` (steps.tsv, facts.json, the vault's identity, params and deploy record, `lez/` the zone's
  transactions, `eth/` the fork's, `user/` what the user ran and saw, `checkpoint/`), `zone/` (the sequencer's
  configuration, logs and environment), `sequencer-roundtrip.log`

