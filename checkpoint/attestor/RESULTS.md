# `ferrata_attestor`: results

What ran on 11 November 2026, and where. Every number below comes from a command named here; the logs and
records are in `runs/`. **Where:** the Modal build box of this worktree (`spike/modal-build/mrun`, a gVisor
container on up to 16 cores), workspace `carl`, between about 16:15 and 16:46 UTC; the Groth16 prover step
in RISC Zero's own prover image (`risczero/risc0-groth16-prover:v2025-04-03.1`, the `groth16_wrap` function
of the same Modal app); the vault in Foundry's local EVM on the same box. Nothing ran on a public network.

## What is real and what stands in

The lab's LEZ v0.4.0-rc1 chain has one sequencer, so its channel holds one key (`n = 1`). Rule (B) with
`t > 1` needs `n >= 3` distinct channel keys in the live set `K` (`t(n) = max(1, min(ceil(2n/3), n-1))`, so
`t(3) = 2`). The run therefore uses a **stand-in key set** `K = {3b14e398…b0a2, e3db1118…6813, ee5875eb…df42}`:
the first is the lab sequencer's real channel key, the real producer of `T`; the other two are throwaway keys
derived from public labels (`ferrata/attestor/devnet-key/channel-1`, `-2`) for two sequencers that do not run.
The node's `/channel` answer is given the same three keys (`configuration_threshold` 2); everything else the
attestors read is the node's own answer. Real: the 17 LEZ blocks 186..=202 (51 transactions, 87,349 bytes),
`T = 202`'s inscription as the node holds it (node block `cfd9acd4…a1ee`, slot 8,856,117), the whole final tier
each attestor checks before it signs (blocks 1..=202, 202 inscriptions, all at or below the LIB slot
8,856,126), and the certificate the lab's channel key gave in the real-blocks run (`realblocks/out/attestor.json`).
A zone of three running LEZ sequencers is not shown here (last section).

## 1. Tests

`cargo +1.88 test -p ferrata-attestor -p attestor-core -p checkpoint-core` (box, `runs/devnet-n3/devnet.log`
was the same session): **12 of 12** `ferrata-attestor` tests pass (6 `attestor-core`, 28 `checkpoint-core`
unchanged); `cargo +1.88 clippy -p ferrata-attestor --all-targets` clean; `cargo +1.88 fmt --check` clean.

| Test | What it checks |
|---|---|
| `reads_the_lab_nodes_final_tier` | the lab node's answers give `K = {3b14…}`, tip 202 `774780c6…2bbb`, final message `cc5ae8ce…a79e` (the channel's own `tip_message`), 17 blocks linked |
| `a_tip_beyond_the_nodes_lib_is_not_final` | with the LIB one slot below `T`'s node block, the final tip is 201 |
| `the_lab_certificate_is_real_and_binds_chain_vault_configuration_and_channel` | the recorded real input, upgraded with `T`'s inscription, is accepted (n = 1, t = 1, producer the lab key); the lab certificate verifies for this deployment and not for another vault, chain or configuration |
| `three_attestors_sign_and_any_two_are_accepted` | three attestors sign; all three and each pair are accepted (n = 3, t = 2); the journal does not depend on who attested |
| `refused_too_few_signatures` | one and zero attestations: `InsufficientAttestations { have: 1 / 0, need: 2 }`; the collector refuses below t |
| `refused_an_uncertified_key` | an attestation key presenting another key's certificate: `BadCertificate`; a channel key outside `K`: `NotAccredited`; the attestor refuses to sign either |
| `refused_a_key_of_another_deployment` | certificates for another vault, chain id or configuration: `BadCertificate` (also when the whole attestation is for the other deployment); the attestor refuses to sign with them |
| `refused_a_repeated_key_and_a_signature_over_another_key_set` | the same attestation twice: `DuplicateAttestor`; a signature over the one-key set: `BadAttestation` |
| `the_attestor_never_signs_two_histories` | with its state, the attestor refuses another hash at a height it signed and a tip below the one it signed |
| `a_dropped_reason_for_each_bad_attestation` | the collector drops, with the reason, a bad certificate, an outsider, a repeat and a bad signature, and keeps the two valid ones |
| `a_rotation_by_two_of_three_and_attestations_under_the_new_set` | with a stand-in posting timeframe of 20, a rotation `{lab, k1, k2} -> {lab, k1, k3}` signed by k1 and k2 is applied in the proof and attestations under the new set are accepted; refused: k2's attestation (`NotAccredited`, removed by the rotation; its attestor refuses to sign), the rotation signed by k1 alone (`ConfigThresholdUnmet { have: 1, need: 2 }`), attestations over the old set (`BadAttestation`) |
| `attestations_of_a_descendant_of_t_count_and_an_attestor_behind_t_is_refused` | `T = 200` with attestations of 202 is accepted (path after `T` = 2); an attestation of 201 for `T = 202`: `NotADescendant { tip_id: 201 }`; the same one supports `T = 200` |

## 2. The devnet run (`attestor/devnet.sh`, `runs/devnet-n3/`)

`cargo +1.93.0 build --release -p host -p ferrata-attestor`, then `bash attestor/devnet.sh <lab node answers>
attestor/runs/devnet-n3` (answers: `realblocks/extract.sh`'s output of 10 November, `channel.json` sha256
`31feca6a…16b3`). Output: `devnet.log`.

| Step | Result |
|---|---|
| Base input | the recorded `real-lab-186-202.input` upgraded with `T`'s inscription from the node (`upgrade`), re-keyed to `K` with nMin 3, tMin 2 (`rekey`): parent state `b8efe5c6…4d13`, `lezConfigId` `da0edd60…9d40` (unchanged: the configuration id does not cover nMin and tMin) |
| Certificates | channel keys 1 and 2 certify attestation keys `a7a11c4a…369b` and `9b065ce4…917f` (`certify`); the lab's attestor keeps its real certificate of `b707f1ee…bc6a` |
| Attestations | three attestors read the node's final tier (blocks 1..=202 checked each) and sign tip 202 under `keySetHash(K)` = `54d8d8dc…` (`att-0/1/2.json`); a second round from each attestor's state signs the same tip again; attestor 1 asked with the node's own one-key answer refuses (its key is not in the live set) |
| Check | `verify`: all three verify |
| Collect | lab attestor + attestor 1 (sequencer 2 silent): **kept 2 of 2, n = 3, t = 2**, accepted natively (`rbm-lab-n3-t2`); all three: kept 3 of 3 (`rbm-lab-n3-all`); one alone: refused (`too few attestations: 1 valid, t(n = 3) = 2 needed`) |
| Descendant | `T = 200`, attestations of 202 (`retip`): accepted (`rbm-lab-n3-t200`); attestor 2 with its node behind (LIB below 202's slot) signs 201 (`att-2-behind.json`): refused for `T = 202` (`rbm-refuse-behind`) |
| Rotation | stand-in posting timeframe 20 (a rotation needs a non-zero turn, SPEC-v3 D3; the lab's channel has 0): parent state `8ef5578a…5194` over `{lab, k1, k2}`; `rotate` adds a configuration transaction (the core's `ChannelConfigOp` encoding) to `{lab, k1, k3}`, threshold 2, signed by k1 and k2; the node's channel answer gives the new set; the lab's attestor and attestor 1 sign under it (`att-rot-0/1.json`), attestor 2 refuses (its key left the set); collected: accepted (`rbm-lab-rot`); refusal inputs: k2's earlier attestation (`removed-key`), the rotation signed by k1 alone (`rotation-one-signer`), attestations over the old set (`old-set`) |
| Refusal inputs | `attestor-refusals`: too-few, none, uncertified, other-vault, other-chain, outsider, duplicate |

## 3. The checkpoint program in r0vm (`runs/devnet-n3/executor.log`, `executor.jsonl`)

`target/release/host bench <name>` (r0vm 3.0.5 executor; the guest's journal equals the native run's).

| Input | Blocks | Attestations | User cycles | Segments |
|---|---|---|---|---|
| `rbm-lab-n3-t2` (n = 3, t = 2) | 17 | 2 | 4,750,279 | 6 (1×2^17 + 5×2^20) |
| `rbm-lab-n3-all` | 17 | 3 | 6,444,846 | 7 (7×2^20) |
| `rbm-lab-n3-t200` (T = 200) | 17 | 2 | 4,918,159 | 6 (1×2^19 + 5×2^20) |
| `rbm-lab-rot` (a rotation, 2 of 3 under the new set) | 17 | 2 | 6,539,536 | 7 (7×2^20) |

Derived: an attestation costs about 1.69 M cycles (6,444,846 − 4,750,279), two `verify_strict` signatures.

Refused in the guest (`Guest panicked: checkpoint refused: …`, no receipt): `rbm-refuse-too-few`
`InsufficientAttestations { have: 1, need: 2 }`; `-none` `{ have: 0, need: 2 }`; `-uncertified` `BadCertificate`;
`-other-vault` `BadCertificate`; `-other-chain` `BadCertificate`; `-outsider` `NotAccredited`; `-duplicate`
`DuplicateAttestor`; `-behind` `NotADescendant { tip_id: 201 }`; `-removed-key` `NotAccredited`;
`-rotation-one-signer` `ConfigThresholdUnmet { have: 1, need: 2 }`; `-old-set` `BadAttestation`.

## 4. The proof (`runs/devnet-n3/prove.jsonl`, `proof/`)

| Step | Where | Result |
|---|---|---|
| `host prove rbm-lab-n3-t2` (succinct) | build box CPU | **347.48 s** wall, 6 segments, 4,750,279 user / 5,373,952 total cycles, receipt 224,314 B (sha256 `59835dd6…e876`); verified against the image id, journal equal to the native one |
| `ferrata-e2e p254` (identity_p254, bridge/) | build box, r0vm | 18.4 s, seal 222,668 B |
| Groth16 prover image (`groth16_wrap`, carl) | Modal | 20 s, about 0.01 USD |
| `ferrata-e2e groth16` | build box | Groth16 receipt verified; on-chain seal 260 B, selector `0x73c457ba`; journal sha256 `498dd53d…0f09` |
| the rotation checkpoint `rbm-lab-rot`: `host prove`, p254, Groth16, receipt | build box CPU; Modal | **517.77 s**, 7 segments, 6,539,536 user cycles, receipt sha256 `662306e0…be53`; p254 19.1 s; Groth16 16 s; journal sha256 `b399dc40…8ccf`, parent `8ef5578a…5194`, new state `926b9292…1d75` (`runs/devnet-n3/proof-rotation/`) |

Image ids of this build (`host ids`; guests unchanged on this branch, ids differ from other builds through
the paths they embed): `checkpoint_b` `0x82dec3b8c242297e236d6862f74c4b29428e961bb539c84058fd0922b188e464`,
`checkpoint` `0x743e97c82b0a7dc9d7ed53c3a63af80d7ee9a6d84611df423461c71ad6ee6f1c`, `segment`
`0x6f6987cf8e92ddf3d6435189cf50747c736bde3346c9f7e4bb5bfb7d1041c7a1`.

The journal: nMin 3, tMin 2, parent `b8efe5c6…4d13`, new state `bf12603d…da53`, tip 202 `774780c6…2bbb`,
n = 3, t = 2, no burns (no bridge program runs in the lab), `echoNonce` 0.

## 5. The vault (`vault/test/real/RuleBMulti.t.sol`, fixture `rule-b-multi.json`)

`python3 -I attestor/vault-fixture.py` then `forge test --match-path test/real/RuleBMulti.t.sol -vv` (Foundry,
local EVM; FerrataVault v5 in mode (B), nMin 3, tMin 2, genesis = the parent state over `K`, behind RISC Zero's
`RiscZeroGroth16Verifier` and emergency stop): **14 of 14 pass** (10 for the 2-of-3 checkpoint, 4 for the
rotation checkpoint, `RuleBRotationTest`, fixture `rule-b-rotation.json`).

| Test | Result |
|---|---|
| journal is two of three; candidate then final after the 1 h wait (and not before) | accepted; `finalState` = new state; burn root accepted |
| a journal claiming t = 1, or n = 2 | `KeySetBelowFloor` (the vault's floor check, before the verifier) |
| t = 3, n = 4, another tip or new state under the real seal | `VerificationFailed` |
| a flipped byte in each seal word | refused |
| a one-key vault, a two-key vault | `ParentNotFinal` |
| a vault with floors (1, 1) or (4, 3) | `WrongCommitteeFloor` |
| a vault pinning another image | `VerificationFailed` |
| gas (`RuleBMultiGas`) | `submitCandidate` **343,581**, `finalizeAttested` **161,437** |
| rotation: the parent is the old set, the journal 2 of 3; candidate then final; a vault whose genesis is the rotated set | accepted, `finalState` = the new state (its key set the rotated one); `ParentNotFinal`; gas 343,581 + 161,437 |

The whole vault suite on the box (`forge test`, before the rotation contract): 443 of 444; the one failure,
`test_privacySuite_ethereumHalf`, reads `../target/privacy/scenario.json`, which a separate privacy-suite run
writes (absent on this box), and is untouched by this branch.

The checkpoint's gas does not depend on n or t (the journal has a fixed size): the one-key real-blocks
checkpoint costs 347,832 + 195,149 (`vault/GAS.md`).

## 6. Against a running node (`attestor/live-node.sh`, `runs/live-node/`)

A copy of the lab's saved generation 3 (`spike/tmp/realblocks/labcopy/gen-3.tar.zst`) and the lab bundle's
`logos-blockchain-node`, started alone on the box as `realblocks/extract.sh` starts it (never the lab).
`attest --node http://127.0.0.1:18080 --save …` fetched `/cryptarchia/info` (LIB slot 8,856,126),
`/channel/<id>` and 27 block windows (7,400,280 bytes), checked blocks 1..=202 and signed tip 202 under the
node's one-key set (`keySetHash` `868b734d…8b14`); `verify` accepts it (`live-att-0.json`). Attestor 1 asked
the same node: refused, its channel key is not in the live set.

## What failed on the way

- The recorded `realblocks/out/real-lab-186-202.input` no longer decodes (`BadLength { what: "suffix" }`): it
  was written before rule (B) inputs carried `T`'s inscription (91c0df8). `upgrade` inserts it from the node.
- The first test expectation that a journal claiming t = 1 fails verification was wrong: the vault refuses it
  earlier (`KeySetBelowFloor`).
- The host needs Rust 1.93.0 (a dependency needs 1.90); the first build with 1.88 failed.

## Not done

- A zone of three running LEZ sequencers. Route without Docker (not tried): the lab bundle's
  `sequencer_service` has `setup` (joiners without `--create-channel`), `start --wait-for-channel
  --gossip-listen … --bootstrap-peers …` and `stake --sequencer-key … --from <account>` through a LEE wallet
  home; missing are a wallet home holding a genesis-funded account's key, and the committee update that
  accredits the joiners (LEZ proposes it itself; it needs the configuration threshold's signatures over
  gossip, `integration_tests/tests/channel_config_threshold.rs`). Founding stakes at genesis instead need
  `sequencer_core::sign_genesis_stake`.
- `ferrata_attestor` as a Logos module (README).
- Rotations proposed and inscribed by a running committee (the rotation here is built from the core's own
  encoding and signed with the stand-in keys); the monitor's watcher reading attestations.
