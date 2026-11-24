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
| Sepolia fork | block 12200361 (2026-11-24T04:43:00Z), hash `0x56d10a8725f576d93fa1be580163fb8cd79eb1a523ddfa4f778edd57dadc749e`, read from a public Sepolia RPC |
| vault | `0x67aC447386F2ea14eF2Fbf55eF33Ce1d8F386516` (rule B), configId `0xa62fb073bd6f4022aab5bf1df9efd607c083283264aaf2ec8fe6eaf5bb46d2ab` |
| deposit | 1000000000000000 wei, Ethereum tx `0x361f55b2772b4101dbd8ca21def839021e6ef2ab6de886b42caa333210d09c1e` |
| claim | LEZ tx `73417d65fff98c84e969de481d220069b0739fbf03077ec3f4a79d1b195ed8ce`; holding 1000000000000000 |
| burn | LEZ tx `b9d3c862ca68d872fe55aca72711eef7cb7228550fe41a87deec6ec1a2a166c5` to `0xF74077965351A6fe64cA1124762ceA9D22d1cE2e` |
| checkpoint | burn root `0xedac98ed007261fa1a5af3ca961f89bd9bb71e02ae7013b52248b495607867dc`, 1 burn(s), tip 30 |
| release | Ethereum tx `0x05288fac33825493d194b91ee8794f777c1f41a436eee1b1fcbb060b871ef684`; the fresh address holds 997000000000000 wei |

| step | result | seconds | detail |
|---|---|---:|---|
| fork | ok | 1 | Sepolia block 12200361 (2026-11-24T04:43:00Z), hash 0x56d10a8725f576d93fa1be580163fb8cd79eb1a523ddfa4f778edd57dadc749e; anvil forks it, chain id 11155111 |
| zone: programs and the stand-in instance | ok | 57 | attestation program 0x816d1fef46… and bridge program 0x847a4336c2… deployed through LEZ's loader; stand-in instance 0x84b80e2777… (bootstrap slot 11640896); zone tip 20 |
| vault deployed on the fork | ok | 7 | vault 0x67aC447386F2ea14eF2Fbf55eF33Ce1d8F386516 (rule B), configId 0xa62fb073bd6f4022aab5bf1df9efd607c083283264aaf2ec8fe6eaf5bb46d2ab, genesisState 0x8e54c7d6a8de29730ff8874b4f6ab32653c3d6f85274cc44fa19057391a3efb8 (ci_zone's test committee); RISC Zero's Groth16 verifier 0x25Fa4286d401bdc4AC7657bc7a1ebdE1B737F4b1 (code hash 0x47dfdcd8969f76d298cd5c4bc1f2889a6bc042b8c0f1e428f526cce1316b9970) deployed by DeployVault, its code then replaced on the fork by MockRiscZeroVerifier's; checkpoint guest 0xdafa6ab9a26841681fd071b8e4cdfae514371f82902426dc9fb1e2c80d29cda0, release guest 0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd |
| deposit | ok | 1 | 0.001 ETH to the user's public /0, Ethereum tx 0x361f55b2772b4101dbd8ca21def839021e6ef2ab6de886b42caa333210d09c1e; the vault's tree holds 1 leaf |
| operator import | ok | 19 | the stand-in committee finalized the fork's block 12200370; bridge Init (pairing: configId in the vault's slot 2), bridge Register ETH (registry entry, schedules 0 and 1, leaf count), bridge ImportRoot n=1 (stage 1, block 12200370): 3 of 3 applied |
| fees | ok | 2 | native LEZ to the user's public /0 (0x8a7162a5acc0f2c0beab5a9d07dfab129e7dc9b18cd9c14b3090fe2cc40154b0): 3000000000000 |
| claim (plaintext) | ok | 4 | LEZ tx 73417d65fff98c84e969de481d220069b0739fbf03077ec3f4a79d1b195ed8ce applied; the user's public /0 holds 1000000000000000 (0.001 ETH) |
| claim again refused | ok | 1 | refused: error: refused: LEZ bridge program exit code 250 ALREADY_CLAIMED (checked before sending: the zone's history shows it claimed): the deposit's nullifier is spent; the holding unchanged |
| burn | ok | 4 | LEZ tx b9d3c862ca68d872fe55aca72711eef7cb7228550fe41a87deec6ec1a2a166c5 applied: 0.001 ETH burnt to 0xF74077965351A6fe64cA1124762ceA9D22d1cE2e; the holding is 0 |
| checkpoint journal | ok | 5 | 34 applied zone transactions in 28 blocks; the guest's journal equals the native one ({"executor_wall_s":0.434881854,"image_id":"0xdafa6ab9a26841681fd071b8e4cdfae514371f82902426dc9fb1e2c80d29cda0","input_by); 1 burn, root 0xedac98ed007261fa1a5af3ca961f89bd9bb71e02ae7013b52248b495607867dc |
| checkpoint accepted | ok | 1 | submitCandidate, Delta let pass, finalizeAttested by ferrata-relayer; finalState 0x75037d4d2d4080a62476f6ce8e0a4b09797310044c75bfa8bbf7cfb6e113b962; burn root accepted |
| release journal | ok | 1 | release guest 0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd: journal equal to the native one (RISC0_DEV_MODE: executed, receipt fake) |
| release paid | ok | 1 | Ethereum tx 0x05288fac33825493d194b91ee8794f777c1f41a436eee1b1fcbb060b871ef684: the Release event pays out the burnt 1000000000000000 wei: 997000000000000 to the fresh address, 2000000000000 relay fee to the relayer, 1000000000000 credited (OwedCredited: the checkpoint fee) |
| release again refused | ok | 0 | eth_call of the same release reverts with NullifierAlreadyUsed (0xcad2ae02); nullifierUsed(0x5f4aa3df33d51f50c0c852e7fac18a00fd10bc950aaae7a150a95dc1800342f1) = true |

- records: `run/` (steps.tsv, facts.json, the vault's identity, params and deploy record, `lez/` the zone's
  transactions, `eth/` the fork's, `user/` what the user ran and saw, `checkpoint/`), `zone/` (the sequencer's
  configuration, logs and environment), `sequencer-roundtrip.log`

## This record

The files are those of the last call of `bash scripts/ci/sequencer-roundtrip.sh e2e/runs/ci-sequencer-2026-11-24/roundtrip`
(its four stages in one call, as the CI job runs them one per step), on the repository's Modal build box (Linux x86-64;
it reports 2 CPUs, so the script used 4 build jobs), after `scripts/fetch-upstream.sh` and `scripts/ci/install-risc0.sh`
(as the job does: r0vm 3.1.0 in `~/.risc0` for `ferrata-e2e`, 3.0.5 in the LEZ home for LEZ), with the scripts of
commit `6b42bc2e` (branch `ci-sequencer`), on 24 November 2026, 04:56:54–04:59:13 UTC. Nothing was sent to Sepolia:
anvil kept every transaction (the deployer and the relayer are impersonated; the user's deposit is signed with anvil's
published dev key 2 and sent to anvil only); the public Sepolia RPC was read for the fork and nothing else.

`sequencer-roundtrip.log` keeps every call of that day, in order (`build.log` only the last two, warm, builds): the
cold build (41 min: the vault
1 min, `bridge/` 2 min, the rule (B) guest 2.5 min, `ci_zone` 8.5 min, the LEZ guests 1.3 min (their target directory
warm from the attestation job's run), LEZ v0.4.0-rc1's standalone sequencer 10.5 min, crown-driver 7 min, `cli/`
9.3 min); five run attempts that stopped, each fixed before the next (DeployVault refuses any verifier but RISC Zero's
3.1.0: the vault tests' mock is now put in its place on the fork, `5c0baae6`; `forge inspect` does not resolve test
contracts: the mock's code from forge's artifact, `5192c80c`; `ferrata-e2e` needs r0vm 3.1.0: `4f202764`; the release's
split was asserted wrongly twice: the recipient is paid the burnt amount less the relay fee and the credited checkpoint
fee, `da90369a`, `86033ad9`); then three passing calls (04:50, 04:53, 04:57 UTC), the last with the replay's refusal
held to `NullifierAlreadyUsed` (`6b42bc2e`). A passing call's run stage took 1 min 52 s.

Target directories after the build, measured on the box: `bridge/target` 1.1 GB, `checkpoint/target` 3.3 GB,
`lez/target` 3.2 GB (with the attestation job's guests), `tmp/ci-seq-roundtrip/target` (the sequencer, crown-driver
and `cli/`) 4.0 GB. mrun's ledger: the cold call 2,629 s, 2.62 core-hours, about 0.27 USD; the last 203 s, about
0.02 USD.
