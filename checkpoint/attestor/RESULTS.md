# `ferrata_attestor`: results

What ran on 11 November 2026, and where. Every number below comes from a command named here; the logs and
records are in `runs/`. **Where:** the Modal build box of this worktree (`spike/modal-build/mrun`, a gVisor
container on up to 16 cores), workspace `ws-7`, between about 16:15 and 16:46 UTC; the Groth16 prover step
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
A zone of four running LEZ sequencers, with no stand-in key, is in §7.

## 1. Tests

`cargo +1.88 test -p ferrata-attestor -p attestor-core -p checkpoint-core` (box, last run 19:21 UTC): **12 of 12** `ferrata-attestor` tests pass (6 `attestor-core`, 28 `checkpoint-core`
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
| Groth16 prover image (`groth16_wrap`, ws-7) | Modal | 20 s, about 0.01 USD |
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
`RiscZeroGroth16Verifier` and emergency stop): **18 of 18 pass** (10 for the 2-of-3 checkpoint, 4 for the
rotation checkpoint, `RuleBRotationTest`, fixture `rule-b-rotation.json`, and 4 for the real zone's checkpoint,
`RuleBZoneTest`, §7).

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

The whole vault suite on the box (`forge test`, with every new contract): 451 of 452; the one failure,
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

## 7. A real zone of four sequencers (`attestor/zone.sh`, `attestor/zone-attest.sh`, `runs/zone/`)

Not a stand-in: upstream's own four-sequencer devnet (`lez/configs/docker-all-in-one/devnet` of LEZ v0.4.0-rc1
`1806e87`: one configuration whose genesis stakes the four channel keys `seq-0..3/channel_signing_key`;
posting timeframe 60 slots, timeout 25, a block every 10 s), run without Docker on the build box with the lab
bundle's binaries (`sequencer_service`, `logos-blockchain-node`, `r0vm`; the box needs `libpcsclite1`):
one Logos Blockchain node (upstream's `bedrock/node-config.yaml` and `deployment-settings.yaml`, sha256
`19383c41…d2ed`, leader proofs in dev mode), four `sequencer_service` processes on loopback, gossip over QUIC
with explicit bootstrap peers (upstream's compose uses mDNS). seq-0 creates the channel; the others wait for it.
Run of 11 November, 18:59:05–19:04:29 UTC, 300 s of sequencing (`zone.log`, the processes' last log lines in
`logs/`); a first run at 18:46–18:51 UTC behaved the same, but its answers were lost with its container.

| What | Result |
|---|---|
| Channel | four accredited keys from the first poll; `configuration_threshold` 3; the turn (`tip_sequencer`) went 0 → 1 → 2 → 3 → 0, about 60 slots each (the sequencers log `Turn update: our_turn=…`) |
| Chain | all four sequencers at the same height at every poll (7/7/7/7 … 28/28/28/28); final tip 30, LIB slot 8,930,837 after the sequencers stopped |
| Who inscribed (the attestor's `tier`, checked inscriptions) | `fce0d0fa…` (seq-0) 11 blocks (2–6, 25–30), `6462ecf1…` (seq-2) 6 (7–12), `90a548e0…` (seq-3) 6 (13–18), `99273f3e…` (seq-1) 6 (19–24); block 1 has no inscription |
| The node's answers | 358,770 bytes (`answers/`, the channel's final blocks) |
| Attestors | each sequencer's channel key (the staked devnet key) certifies an attestation key (`cert-0..3.json`); the four attestors check blocks 2..=30 and sign tip 30 `c7b81252…b01e` (final message `1cf04ac6…e33d`, the channel's own `tip_message`); all four verify |
| Input (`base`) | final blocks 3..=30 (28 blocks, 56 transactions, 23,492 bytes), parent block 2, `K` = the channel's four keys, posting timeframe 60, configuration tip `e502602b…224c`; nMin 4, tMin 3; bridge values of the configuration are stand-ins (no bridge program runs): `lezConfigId` `21effc46…523e`, parent state `60dfe488…e559` |
| Collected | sequencers 0, 1, 2 (sequencer 3 silent): **n = 4, t = 3**, accepted, 6,261,183 user cycles (7 segments); all four: accepted, 7,950,701 (9 segments); two of four: refused in the guest, `InsufficientAttestations { have: 2, need: 3 }` |
| Proof | `host prove rbz-zone-n4-t3` (box CPU): **513.54 s**, 7 segments, 6,261,183 user cycles, receipt sha256 `3293f702…0f6c`; identity_p254 18.2 s; Groth16 prover image (ws-7) 17 s; seal 260 bytes, journal sha256 `57d90e16…c985` (`runs/zone/proof/`, `prove.jsonl`) |
| Vault | `RuleBZoneTest` (FerrataVault v5 in mode (B), nMin 4, tMin 3, genesis = the parent state over the four keys, RISC Zero's real Groth16 verifier): the journal is 3 of 4; candidate, then final after the wait and not before; a journal claiming t = 2 refused (`KeySetBelowFloor`); a vault with floors (3, 2) refuses it (`WrongCommitteeFloor`); **343,581 + 161,437 gas** |

**A silent sequencer** (`SILENT=3 SILENT_AFTER=60 attestor/zone.sh …`, `runs/zone-silent/`, 19:22:41–19:28:05
UTC): seq-3 (`90a548e0…`) was stopped 60 s into the run, before its first turn. The others went on: when its
turn came the chain stood at height 12 for about 40 s (the turn's posting timeout), then the turn passed to the
next key and blocks continued to 25; the final tier has no block by seq-3's key (`fce0d0fa…` 11 blocks,
`6462ecf1…` 7, `99273f3e…` 6). The other three attestors' 3 of 4 over blocks 3..=25 (23 blocks) is accepted by
the checkpoint program in r0vm (6,214,564 user cycles, `T` = 25 inscribed by `6462ecf1…`); 2 of 4 refused.
(`zone-attest.sh` also signs with seq-3's attestor, which reads the same node; its 4-of-4 input is accepted too.)

## 8. The bridge program on the zone, and its checkpoint into the vault (`attestor/zone-bridge.sh`, `e2e/runs/zone-bridge-1/`)

What ran on 13 November 2026, on the Modal build box of the `zone-bridge` worktree (our Modal workspace, a gVisor
container on up to 16 cores, 32 threads), and in RISC Zero's Groth16 prover image (the `groth16_wrap` function of
the same app). Nothing ran on a public network. The zone stage ran from 07:01 to 07:23 UTC (1,379 s, one command);
proofs, wraps and the vault followed until 07:50 UTC. Records: [`e2e/runs/zone-bridge-1/`](../../e2e/runs/zone-bridge-1/)
(`logs/zone-bridge.log` is the run's own log; `zone/` the zone's records, `lez/` the sessions', `checkpoint/` and
`releases/` the proofs', `manifest.json` the anvil's transactions, `vault-checks.json` what the vault held after).

**Real:** the four `sequencer_service` processes (each its own home, staked channel key and RPC; gossip between
them over QUIC on loopback) and the Bedrock channel they share (four accredited keys, `configuration_threshold` 3,
posting timeframe 60 slots, the turn rotating among them); the programs, the final ones of
[`docs/image-ids.md`](../../docs/image-ids.md) (`ferrata_lez` `0x816d1fef…`, `ferrata_bridge` `0xccde6171…` from
the crown-live-3 LEZ bundle, sha256 `0013bab1…`, its `SHA256SUMS` checked; `checkpoint_b` `0xf8116722…` and
release `0xb64900bc…` as R0BF files whose sha256 are the table's); both private transactions proven by LEZ's
wallet; four `ferrata_attestor` processes, each key certified by its sequencer's channel key; the node's final
tier, each block of it checked by each attestor; `checkpoint_b`'s STARK proof and its Groth16 wrap; the release
proofs; FerrataVault v5 in mode (B) with RISC Zero's Groth16 verifier behind its emergency stop.
**STAND-IN:** the beacon chain on LEZ (crown-driver's stand-in committee, whose keys the driver holds, finalizes
the anvil's blocks: the light client's Ethereum is not Sepolia's); the Ethereum side (a local anvil 1.3.6, not a
fork, with Sepolia's chain id because crown-driver pins it; the deployer an impersonated address derived from a
label); the vault's wait `W` (3,600 s) and release delay (120 s), passed by `evm_increaseTime`; leader proofs in
dev mode on the one Bedrock node all four sequencers post to (as every devnet here); the channel keys are
upstream's published devnet keys; the zone's genesis is upstream's devnet genesis plus `local.json`'s two funded
public test accounts (the devnet's own two hold pre-fee balances, 10,000 and 20,000, below one fee reservation,
as LEZ's `testnet_initial_state` says), the only change to upstream's configuration besides loopback addresses;
the burns' recipient is crown-driver's default address.

| Step | Result |
|---|---|
| Zone | upstream's four-sequencer devnet (as §7) on the bundle's binaries; each sequencer restarted until its blocks fall 1–6 s after an epoch second (the crown sessions' rule; `zone/phases.txt`: 3, 1, 1 and 3 starts); the channel accredits the four keys; over the run the turn went seq-0 → seq-2 → seq-3 → seq-1 → seq-0 four times (the channel's key order; `zone/logs/poll.log`, 50 polls), six blocks a turn; heights equal on all four at 48 of 50 polls (two polls one block apart); no head rewind and no given-up inscription in any sequencer's log |
| Session A (`crown-driver crown-a`, this tree's build; transactions sent to seq-0's RPC) | the attestation program and the bridge deployed (15 transactions in blocks 17–31), the STAND-IN instance's `Init` (block 32) and its two adoptions (33, 34); T0 = 34 `0x0b678ced…` |
| Ethereum side (local anvil) | DeployVault: verifier, stop, registry, vault `0xE9bD0e95…` (nonce 3, predicted), test token, ETH and the token registered, the stop's owner renounced; `lezConfigId` `0x8c313ea4…` (`crown-driver config-id` over session A and that address), `genesisState` `0x958a2dcd…` (the state at T0 over the four keys, posting timeframe 60); nMin 4, tMin 3; deposits j0 (public) and j1 (private) of 0.001 ETH plus the 0.2 % relay fee, 200,572 and 151,394 gas; captures of anvil blocks 9 and 10 (headers rebuilt and checked against their hashes); `crown-driver check`: pairing, import and registration accepted natively under both records |
| Attestors | one per sequencer, keys from the OS's randomness (not recorded), each certified by its sequencer's channel key for this deployment (`zone/attestor-*/cert.json`); a round every 60 s from 07:09:50 UTC: each signed the same 14 rising final tips 35, 41, …, 113 and refused none (`rounds.tsv`) |
| Session B (`crown-driver crown-b`) | bridge `Init` (block 40), `Register` ETH (41), `ImportRoot` n=1 (42), `Claim` of deposit 0 (43), `ImportFees` (47), `ImportRoot` n=2 (48), `ClaimPrivately` of deposit 1 (proven in 281.4 s, 252,267 bytes; block 78), `Burn` public (79), `Burn` private (proven in 324.2 s, 255,031 bytes; block 113); T1 = 113. Every transaction went to seq-0's RPC; the blocks that hold them were made by seq-0 (40–43, 113), seq-2 (47, 48) and seq-3 (78, 79): the others had them by gossip |
| Final tier | the sequencers stopped at 07:23:20 UTC; once the node's LIB passed the channel's tip, all four attestors signed final tip 116 `0x4c37dbfc…` (final message `0x4c451c6e…`, inscribed by seq-2) after checking blocks 2..=116; `verify`: all four verify |
| Node answers (`zone/node-answers.tar.zst`) | those attestor 0 fetched and checked in its final round: 1,322 immutable node blocks, slots 9,059,955..=9,061,568, 4,944,924 bytes (1,325,397 compressed); final inscriptions of LEZ blocks 2..=116 all accepted by `realblocks-crown` (parse, re-hash, link, framing, BLAKE2b, `verify_strict`), by seq-0 36, seq-1 30, seq-2 25, seq-3 24 |
| Input `zone-34-116` (`realblocks-crown --att`, three of the four records, attestor 3 left out) | blocks 35..=116 (82 blocks, 179 transactions, 604,877 bytes), T = 116; n = 4, t = 3; two burns, burn root `0x64742ec9…`, imported leaf count 2; parent state `0x958a2dcd…` (= the vault's `genesisState`), new state `0x0d79f1d0…`; journal sha256 `a2aa83f3…` |
| r0vm | 3 of 4: **8,060,167** user cycles, 10 segments (9×2^20 + 2^19), journal equal to the native one; 4 of 4: 9,758,970, 12 segments; 2 of 4: refused by the collector (`too few attestations: 2 valid, t(n = 4) = 3 needed`) and by the guest (`InsufficientAttestations { have: 2, need: 3 }`) |
| Proof | `ferrata-e2e prove` on the box's CPU: **701.1 s**, 10 segments, 8,060,167 user / 9,961,472 total cycles, receipt 224,314 bytes sha256 `4e1ba0a4…`, verified against the image id; identity_p254 18.7 s; Groth16 prover image 14 s; seal 260 bytes, selector `0x73c457ba` |
| Releases (`ferrata-e2e crown-release`, the burn tree recomputed by the bridge core, equal to the checkpoint core's) | `release-public-0` and `release-private-1`: 77,736 user cycles, 1 segment, 14.6 s and 14.3 s on the CPU; identity_p254 18.8 s and 18.5 s; Groth16 16 s and 14 s; journals sha256 `0aec6465…` and `96b17e06…` |
| Vault (the anvil restored from its saved state; `e2e/crown/sepolia.sh`) | `submitCandidate` 375,269 gas; W passed (`evm_increaseTime` 3,601); `finalizeAttested` 218,385; `finalState` = the journal's new state; `burnRootStatus(0x64742ec9…)` accepted; the release delay passed (121 s); `release` 0 (420,584 gas) and 1 (361,396): each paid **999,000,000,000,000 wei** (0.001 ETH less the 0.1 % checkpoint fee) to the recipient each burn named, nullifier used; left in the vault: 6,000,000,000,000 wei (the relay and checkpoint fees) |
| `vault/test/real/ZoneBridge.t.sol` (fixture `zone-bridge.json`, the run's journal and seals in a vault built as DeployVault builds it, at the journal's address and chain id) | **4 of 4**: the journal is 3 of 4 over the zone's keys, 2 burns, 2 leaves; candidate, final after the hour and not before, burn root accepted, both burns released with their seals (each as the anvil paid), a second release refused; a flipped seal byte and t = 2 refused; gas 343,581 + 181,337 (calls; the anvil's figures are transactions). `RuleBZoneTest` 4 of 4 and `RuleBMultiTest` 5 of 5 unchanged |
| `zone-bridge.sh verify` | the input, the journal and `burns.json` re-derived from the recorded node answers and attestation records: equal; the six receipts (two of each proof) verify |

Re-check (from the worktree root, on the build box): `bash checkpoint/attestor/zone-bridge.sh build e2e/runs/zone-bridge-1`
then `ZB_PROGRAMS=<R0BF files of docs/image-ids.md> bash checkpoint/attestor/zone-bridge.sh verify e2e/runs/zone-bridge-1`
(the programs from `scripts/reproduce-image-ids.sh build`); `cd vault && forge test --match-contract ZoneBridgeTest`.
The whole run: the stages `zone`, `input`, `prove`, `releases`, then `spike/modal-build/mwrap` on each
`work/<proof>/input.json`, `groth16 <proof>` for each, `vault`, `fixture`, `verify` (the script's header).

## 9. A rotation the zone's committee proposed, in the checkpoint (`ZB_ROTATE=1 attestor/zone-bridge.sh`, `e2e/runs/zone-bridge-2/`)

The run of §8 again (same box, same programs, same stand-ins), 13 November 2026, zone stage 08:43–09:05 UTC (1,363 s),
proofs and vault until 09:25 UTC, with one step more between the attestors' start and session B: a fifth sequencer
joins the committee the way LEZ v0.4.0-rc1 provides for. Its key is staked on LEZ with upstream's own operator
command (`sequencer_service stake --sequencer-key … --from Public/…`, the minimum stake, 149, paid by a funded test
account through crown-driver's wallet); LEZ's state then names five keys, and the four running sequencers'
`channel_signing` actors do what they are written to do: the turn's sequencer drafts the `ChannelConfigOp`, its peers
sign that transaction, and Bedrock applies it once t(4) = 3 of the four keys have signed. Nothing of this is driven by
Ferrata's code; the run only stakes the key and watches the node's `/channel`. Records:
[`e2e/runs/zone-bridge-2/`](../../e2e/runs/zone-bridge-2/) (`zone/channel-before-rotation.json`,
`channel-after-rotation.json`; the rest as §8's).

| Step | Result |
|---|---|
| Before | session A on the four sequencers: T0 = 28 `0x82816c41…`; the vault (same address and `lezConfigId` as §8's: same deployer, nonces and zone settings) with `genesisState` `0x2f604a80…` over the four keys; deposits and captures as §8 |
| The stake | seq-4's home set up (a fresh channel key `98b684c5…`); `stake` of 149 from public test account 0 into a new ownership account, transaction `95b71382…`, included by seq-0 in its turn |
| The rotation | **15 s after the stake was sent the node's channel accredits five keys**: `configuration_threshold` 3 → 4, configuration tip `0xe502602b…` → `0xd070c2ab…`; the `ChannelConfigOp` (transaction `0x60095f74…`: the config op and its fee transfer) in node block `d9cd820d…`, slot 9,066,785, between the inscriptions of LEZ blocks 30 and 31; its `ChannelMultiSigProof` carries the signatures of key indices 0, 2 and 3 of the old set (seq-0, seq-3, seq-1); the new set is the five keys in ascending order |
| The fifth sequencer | started on the channel (three starts for its phase), it took its turn in the rotation: seq-0 → seq-2 → seq-3 → seq-4 → seq-1 (the new key order), six blocks a turn; it inscribed LEZ blocks 44–49, 73–78 and 102–107 (18 of the final tier's); heights equal on every running sequencer at 49 of the 55 polls (at four one sequencer was a block ahead or behind, at two seq-4's RPC was not up yet); no head rewind and no given-up inscription in any log |
| Attestors | four from before the rotation (each signed 16 rising tips, 29 … 117, refusing none: tip 29 under the four-key set, every later one under the five-key set, `attest.log`) and seq-4's, certified by its new channel key, started after it (14 rounds, 38 … 113) |
| Session B (on five sequencers) | as §8: `Init` (block 55, seq-1's), `Register` ETH (56), `ImportRoot` n=1 (57), `Claim` (58), `ImportFees` (62, seq-2's), `ImportRoot` n=2 (63), `ClaimPrivately` (proven in 227.6 s; block 87), `Burn` public (88), `Burn` private (proven in 274.8 s; block 116); T1 = 116 |
| Final tier | all five attestors sign final tip 119 `0xe6c7f705…` (final message `0x5322cb50…`) after checking blocks 2..=119, key set n = 5; all five verify |
| Node answers | attestor 0's final round: 1,307 immutable node blocks, slots 9,065,973..=9,067,693, 4,943,984 bytes; final inscriptions of LEZ blocks 2..=119 accepted, each signer checked against the key set in force at its node slot (seq-0 27, seq-1 24, seq-2 25, seq-3 24, seq-4 18) |
| Input `zone-28-119` (`realblocks-crown --att`, attestor 4 left out) | the rotation as it was inscribed: the transaction re-encoded from the node's JSON and checked against its hash, op index 0, the three signatures; parent = T0 over the four keys and session A's configuration tip; blocks 29..=119 (91 blocks, 198 transactions, 613,341 bytes); four of the five attestations, over the new set; n = 5, t = 4; the same two burns (burn root `0x64742ec9…`: the leaves are §8's, from the same configuration, notes and amounts); parent state `0x2f604a80…`, new state `0xa2a12e2c…` (its key set the five), journal sha256 `0c916209…` |
| r0vm | 4 of 5: **12,544,642** user cycles, 15 segments; 5 of 5: 14,231,687, 16 segments; 3 of 5: refused by the collector (`too few attestations: 3 valid, t(n = 5) = 4 needed`) and by the guest (`InsufficientAttestations { have: 3, need: 4 }`) |
| Proof | the box's CPU: **921.1 s**, 15 segments, 12,544,642 user / 14,696,448 total cycles, receipt sha256 `4db4bd52…`; identity_p254 14.9 s; Groth16 14 s |
| Releases | the same two statements as §8 (inputs byte for byte): 11.1 s and 11.2 s on the CPU, receipts `18de1d11…` and `2dc50451…`, Groth16 14 s and 15 s |
| Vault (local anvil) | `submitCandidate` 375,293 gas; W warped; `finalizeAttested` 218,385; `finalState` = the five-key new state; the burn root accepted; both releases (420,596 and 361,396 gas) paid 999,000,000,000,000 wei each, nullifiers used |
| `vault/test/real/ZoneBridgeRotation.t.sol` (fixture `zone-bridge-rotation.json`) | **6 of 6**: the journal is 4 of 5 (n = 5, t = 4) over a parent of four keys; the proof's set is the committee's five, which keep the four; candidate, final after the hour, both releases with their seals; a flipped seal byte and t = 2 refused; a vault whose genesis is the new set refuses it (`ParentNotFinal`); gas 343,581 + 181,337 (a fixed-size journal: as §8's) |
| `zone-bridge.sh verify` | input, journal and burns re-derived from the recorded answers and attestations: equal; six receipts verify (and §8's run again, equal, with this `realblocks-crown`) |

What the rotation here is not: the key set grew (no key was removed, so "a key removed by a rotation the proof
applied" is still shown only on the lab's chain with stand-in keys, §2); the new sequencer's stake came from a test
account through a CLI, not from an operator's own funds.

## 10. Review fixes of 21 November: the LEZ check, withdrawals, garbage, the state (branch `fix-attestor`)

The code reviews of 21 November (H-2, M-3, M-4 of the bridge review; M7, L7, L8 of the modules review)
found that the attestor signed the Bedrock node's view alone, stopped on withdrawals and on one garbage
inscription, re-read the whole chain each round, and lost its two-histories guard when the read was
narrowed. All on the build box (`mrun`), measured:

| Run | Result |
|---|---|
| Probe of the unchanged code (`5cf1e8e7`, a test file copied in for one run) | **H-2**: attestor 2 signed `F` = block 201 `59041fd0…205e`, a well-formed block on 200 by k1 that no LEZ node applied; **M-3**: `opcode 0x14 … not one LEZ sends`; **M-4**: a 5-byte payload and a second "block 200" each refuse the whole tier; **M7**: with the read starting at 201, it signed 202 although the signed 200 (another hash) is not its ancestor |
| `cargo test -p ferrata-attestor --locked` (after) | 24 passed: `attestor.rs` 12 (ported), `lez_check.rs` 7, `withdrawals.rs` 3, `io.rs` 2; `cargo clippy --all-targets -- -D warnings` clean |
| The forged turn (`h2_a_block_that_lez_parked_is_never_signed_and_the_forgery_needs_it`) | with F final on Bedrock and parked by LEZ, every attestor signs 200; once the honest 201 and 202 are final, 202 (LEZ decides at 201); the program accepts T = F with two forced signatures of F (so the attestor is the gate) and refuses it with the honest ones (`NotADescendant { tip_id: 200 }`) |
| Random rounds with restarts (`m7_never_two_histories_across_restarts`, 4 seeds x 150 rounds) | 117-127 rounds signed per seed, 23-33 refused (outages, compromised LEZ nodes), 19-26 forged turns, 22-30 compromised views; never two blocks signed at one height, every signed tip descends from the last, every honest round signs the LEZ node's final tip; signed up to 313-334 |
| Mutants (one guard removed at a time) | fork taken without the LEZ node: 2 tests fail; end of the run unconfirmed: 4; guard partial: 1; tripwire off: 1. None of these four survives (the review of this branch found two other guards no test covered: §10.1) |
| Rounds against a local node stub over curl (`io.rs`) | round 1 asks 15 windows from slot 8,848,857; round 2 one window from the signed tip's slot 8,856,117, same signature; the saved answers replay to the same tier; a reused directory and a LEZ node that does not answer are node errors |
| `devnet.sh`, `zone-attest.sh` re-run (outputs in scratch; recorded runs have no LEZ answers, stood in) | both exit 0 with their refusals; the zone's four attestations sign the same bytes as `runs/zone/att-*.json` (the 264-byte `encoded` field, equal), while the files differ in their `node` facts: `first_block` 30 and `blocks_checked` 1 (a first read starts at the LEZ node's highest final block; the records, of 11 November, read 2 and 29) and `fork_digest` (null, a field the records predate); the 3-of-4 input is identical to the proven `rbz-zone-n4-t3.input`. (An earlier text said the attestations were byte-identical: only `encoded` was compared.) `runs/zone/` keeps the 11 November records; `zone-attest.sh` now writes a replay's records to a directory of its own |
| `ferrata_attestor_module` calls (`the_zone_replayed_gives_the_proven_input`, in a scratch workspace laid out like the module) | 2 passed; without LEZ answers the module's `attest` refuses |

Not re-run: `zone.sh`, `zone-bridge.sh`, `live-node.sh` (live nodes; `live-node.sh` now needs a LEZ node), the
module's `attestor-e2e.sh` and `attestor-zone-live.sh` (they must now pass a LEZ node or a stand-in: open).

## What failed on the way

- The recorded `realblocks/out/real-lab-186-202.input` no longer decodes (`BadLength { what: "suffix" }`): it
  was written before rule (B) inputs carried `T`'s inscription (490b5a4). `upgrade` inserts it from the node.
- The first test expectation that a journal claiming t = 1 fails verification was wrong: the vault refuses it
  earlier (`KeySetBelowFloor`).
- The host needs Rust 1.93.0 (a dependency needs 1.90); the first build with 1.88 failed.
- The zone: the bundle's `sequencer_service` needs `libpcsclite1`, absent from the box (first attempt stopped
  at `setup`); the first full run (18:46–18:51 UTC) worked but its answers were lost (fetched from slot 0, about
  17,800 windows, while the local `mrun` was stopped by a 10-minute limit and the container recycled); the next
  runs fetch from the channel's first slot. The zone's block 1 has no inscription, so its spans start at 3.
- One proof call returned nothing when its container was recycled mid-proof; it was redone on a new container.
- §8: crown-driver refused a channel of four keys (`ensure!(keys.len() == 1)`: the crown run's devnet has one); it now
  takes the channel's key set as it is. The first box build of crown-driver stopped at `pcsc-sys` (the box had
  `libpcsclite1` but not its headers). The zone stage's own dump of the node's answers read `lib_slot` at the top of
  `/cryptarchia/info`, where the node nests it (`cryptarchia_info.lib_slot`), and saved no window; the answers are
  the ones attestor 0 fetched and checked in its final round (`zone-bridge.sh answers`), which the stage now packs
  itself.
- §9: the first start of the rotation run never reached the box (the laptop's Modal client timed out, "Deadline
  exceeded", and the box's container was recycled); the second start ran it.

## Not done

- On the zone of §8 and §9: a dropped inscription (B1's done gate; the silent sequencer is shown, §7); a rotation that
  removes a key; the attestor as the Logos module beside the bridge zone's sequencers (the binary ran there); the
  zone's checkpoint on Sepolia.
- The monitor's watcher reading attestations. (Rotations proposed and inscribed by a running committee: §9.)
