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

A copy of the lab's saved generation 3 (`gen-3.tar.zst`, outside the repository) and the lab bundle's
`logos-blockchain-node`, started alone on the box as `realblocks/extract.sh` starts it (never the lab).
`attest --node http://127.0.0.1:18080 --save …` fetched `/cryptarchia/info` (LIB slot 8,856,126),
`/channel/<id>` and 27 block windows (7,400,280 bytes), checked blocks 1..=202 and signed tip 202 under the
node's one-key set (`keySetHash` `868b734d…8b14`); `verify` accepts it (`live-att-0.json`). Attestor 1 asked
the same node: refused, its channel key is not in the live set.

This run predates the LEZ check (§10): the attestor then signed the Bedrock node's view alone. `live-node.sh` now
also needs `LEZ`, a LEZ node that follows the recorded lab channel, and none exists for that lab generation (the
lab's sequencer state was not saved with it), so the run cannot be repeated as recorded; the LEZ check against
running LEZ nodes is shown by the zone runs of §10.1 instead.

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

Not re-run at this point: `zone.sh`, `zone-bridge.sh`, `live-node.sh` (live nodes; `live-node.sh` now needs a LEZ
node), the module's `attestor-e2e.sh` and `attestor-zone-live.sh` (they must now pass a LEZ node or a stand-in). §10.1
re-ran `zone-bridge.sh`, `return-leg.sh` and `attestor-e2e.sh` after the next review; §10.2 re-ran `devnet.sh` as the
README now says.

### 10.1 The branch's own review (22 November): the scripts around the attestor, two untested guards, the Lows

The review of this branch found the attestor's safety logic sound and its tests checking what they claim, and
four Medium findings outside that logic: two scripts that run the attestor beside live sequencers (`zone-bridge.sh`
asked stopped LEZ nodes for its final round; `lez/live/hooks/return-leg.sh` passed no LEZ node), the module's e2e
scripts (not moved to the new interface: 9 of 23 checks passed), and the "byte-identical" claim above (corrected
there). Its Lows: two guards no test covered, #1053 missing from the README, a first read that 16 junk ids could
stop, a fixed temporary file, the module's one-shot `attest` classifying a silent LEZ node as a refusal and working
without a state file, and undocumented operator steps. All on the build box (`mrun`), measured:

| Finding | What changed | Shown by |
|---|---|---|
| L2: two guards no test covered | tests: a first read never roots at a sibling its LEZ node does not hold; a tip by a key outside `K` is passed over (a first read whose run starts there refuses) | `lez_check.rs` `l2_a_first_read_never_roots_at_a_sibling_the_lez_node_does_not_hold`, `l2_a_tip_inscribed_by_a_key_outside_k_is_passed_over`; the review's surviving mutants now fail 2 and 1 tests |
| L5: 16 junk ids could stop a first read | the first read walks down every inscribed id (each asked once) | `l5_junk_ids_above_the_chain_never_stop_a_first_read` (28 junk ids, the LEZ node ahead of the LIB) |
| L6: `write_atomic` used `<path>.tmp` with `File::create` | a temporary file of its own (process, time, counter), opened `create_new` | the unique name: `files.rs` `l6_write_atomic_never_writes_through_a_planted_link`, `l6_two_writers_never_share_a_temporary_file`; `create_new` only since §10.2 (`l6_create_new_never_opens_what_is_at_the_temporary_name`) |
| L3: keygen's mode and refusal untested; L7's order shown only in part | tests through the CLI | `files.rs` `l3_keygen_writes_an_owner_only_key_and_never_overwrites`, `l7_the_state_holds_the_tip_before_the_attestation_is_written` |
| L4: the module's `attest` | a silent LEZ node is an `error`; `node` requires `state_file` (`is_node_fault`, shared with `round`) | calls crate `a_live_attest_keeps_a_state_and_a_silent_lez_node_is_an_error` (3 of 3) |
| L1, L7, L8 | README: #1053 and the LIB filter; "Operating it" (the attestor's own LEZ node; a node that lost the signed tip stops it until the operator acts; a lost state file loses the guard; `getBlock` marked for removal upstream); §6: `live-node.sh`'s run cannot be repeated as recorded | |
| M3: the module's e2e scripts | `recorded_node.py` stands the zone's inscribed blocks in as a LEZ node (`getBlock` over HTTP, or saved for `lez_dir`); `attestor-e2e.sh`, `daemon_requests.py`, `attestor-zone-live.sh` (the last signatures before the sequencers stop: `zone.sh`'s `ZONE_BEFORE_STOP`), `modules/README.md` | `attestor-e2e.sh` natively: **24 of 24** (one check added: another history at the signed height); `daemon_requests.py`'s 12 attestor requests: every expectation met ([`modules/RESULTS.md`](../../modules/RESULTS.md)) |
| M4: "byte-identical" | the text above; `zone-attest.sh` writes a replay's records to a directory of its own | replay into scratch: `encoded` equal x4, `node` differs as stated, the 3-of-4 input identical |
| M1, M2: the scripts beside live sequencers | the attestors' last round while the zone is up (each asks its own sequencer's LEZ node), every attestor on one tip at or above T1; the loops end after the round in hand; the node's answers read separately (a round now reads only from its signed tip's slot); `return-leg.sh` passes `--lez "$ZONE_RPC"` | the re-runs below |

`cargo test -p ferrata-attestor --locked`: **31 passed** (`attestor.rs` 12, `files.rs` 4, `io.rs` 2, `lez_check.rs` 10,
`withdrawals.rs` 3); `cargo clippy --all-targets -- -D warnings` clean; rustfmt clean. Mutants, one guard removed at
a time (the review's 14, `first-root-unchecked` following the new walk, and two that undo L5 and L6), each on a copy:
**16 of 16 fail at least one test**: `bytes-unchecked` 5, `end-unconfirmed` 4, `fork-status-only` 3, `fork-unchecked`
3, `lib-filter-off` 3, `first-root-unchecked` 2, `status-ignored` 2, `write-atomic-fixed-tmp` 2, and 1 each for
`end-status-only`, `first-root-cap16`, `guard-partial`, `lez-checked-off`, `lower-ok`, `signer-not-in-k-ok`,
`tripwire-off`, `tripwire-status-off`.

**The bridge program on a zone of four, with LEZ-checked attestors** (review M1's re-run: `zone-bridge.sh` at this
branch, [`e2e/runs/zone-bridge-3/`](../../e2e/runs/zone-bridge-3/README.md); §8's run again, everything else as there):

| Step | Result |
|---|---|
| The attestors | four, each beside its sequencer and asking that sequencer's LEZ node (`getBlock` on its JSON-RPC) every round: 13 rounds each (12 while the zone ran, one final), **13 rising tips signed (30, 35, 41, … 88, 93, 96), none refused**, the four on the same tip at every round; the first real LEZ nodes the check met: each signed tip was served `Finalized` and byte for byte as inscribed |
| The final round | taken while the sequencers ran, side by side: all four signed **T1 = 96** at the first try (heights 96/96/96/96); each final read started at the tip signed before (93: blocks 93..=96 checked); `verify` 4 of 4; then the sequencers stopped, the LIB passed the channel's tip, and the node's answers were read from the first slot the attestors read to the LIB |
| Sessions | A: T0 = 29; B: `Init` (block 35), `Register` ETH (36), `ImportRoot` n=1 (37), `Claim` (38), `ImportFees` (42), `ImportRoot` n=2 (43), `ClaimPrivately` (proven by LEZ's wallet in 241.1 s; block 67), `Burn` public (68), `Burn` private (282.4 s; block 96) |
| Input `zone-29-96` (`realblocks-crown --att`, attestor 3 left out) | blocks 30..=96 (67 blocks, 149 transactions, 592,292 bytes), n = 4, t = 3, 2 burns (burn root `0x64742ec9…`, the leaves §8's), imported leaf count 2; journal sha256 `e73ee6bb…`; parent state `0xb271bd0d…`, new state `0xe8330733…` |
| r0vm | 3 of 4: **22,251,410** user cycles, 24 segments; 4 of 4: 23,940,403, 26 segments; 2 of 4: refused by the collector and by the guest (`InsufficientAttestations { have: 2, need: 3 }`) |
| Proof | the box's CPU (32 threads, beside other builds): **1,426.4 s**, receipt 224,314 B, journal = native; identity_p254 16.3 s; Groth16 16 s (seal 260 B) |
| Releases | 77,736 cycles each, 11.1 s and 11.2 s; Groth16 12 s and 17 s |
| Vault (the run's anvil) | `submitCandidate` 375,305 gas; W warped; `finalizeAttested` 218,385; `finalState` = the journal's new state; burn root accepted; releases 420,596 and 361,372 gas, each paid 999,000,000,000,000 wei |
| `vault/test/real/ZoneBridge.t.sol` | `ZoneBridgeLezCheckedTest` (fixture `zone-bridge-3.json`) **4 of 4**; `ZoneBridgeTest` (zone-bridge-1's) still 4 of 4 |
| `zone-bridge.sh verify` | input, journal and burns re-derived from the recorded answers and attestations: equal; the receipts verify |

The box was preempted during a first attempt, whose zone stage had reached T1 = 111 with all four attestors signing
it at the first final round; Modal ran the call again in a new container, and the run above is that second one (the
first left only its live log lines).

**The rotation run again** (`ZB_ROTATE=1`, [`e2e/runs/zone-bridge-4/`](../../e2e/runs/zone-bridge-4/README.md), §9's run):
five keys accredited 20 s after the stake; attestors 0-3 21 rounds each, 21 rising tips (31 … 148), attestor 4 (the
fifth sequencer's, started after the rotation) 20 rounds (37 … 148), none refused; the final round: all five signed
T1 = 148 at the first try; input `zone-30-148` (T0 = 30): the rotation as inscribed, blocks 31..=148 (118 blocks, 252
transactions), n = 5, t = 4; r0vm 4 of 5: 26,952,174 user cycles, 29 segments (5 of 5: 28,633,873; 3 of 5 refused by
the collector and the guest); CPU proof 1,739.1 s, identity_p254 14.9 s, Groth16 15 s; releases 11.2 s and 11.1 s;
vault: `submitCandidate` 375,305 gas, `finalizeAttested` 218,385, releases 420,608 and 361,396, each paid
999,000,000,000,000 wei; `ZoneBridgeRotationLezCheckedTest` 6 of 6; `verify` equal.

**The rc3 return leg again** (review M2's re-run, [`e2e/runs/zone-bridge-rc3-2/`](../../e2e/runs/zone-bridge-rc3-2/README.md),
[`lez/results/v0.4.0-rc3/return-leg-2/`](../../lez/results/v0.4.0-rc3/return-leg-2/README.md)): rc3's
`sequencer_service` on a 0.3.5-rc.1 node; the attestor asked the sequencer's own `getBlock` every round: 14 rounds,
rising tips 27 … 105, none refused, the final round (zone up) signed T1 = 106 (fork digest `c025c6d2…`); input
`zone-24-106` (T0 = 24): blocks 25..=106 (82 blocks, 179 transactions), all 106 final inscriptions checked by the
second build, burn root `0x78aae803…` (the first rc3 run's leaves); `checkpoint_b_0_3_5` (rebuilt here, the same id
`0xafbf4023…`): 18,486,594 user cycles, 20 segments, CPU proof 1,543.9 s, identity_p254 22.7 s, Groth16 16 s; 0 of 1
refused; the deployed `checkpoint_b` refuses the input (`UnsupportedOpcode { opcode: 37 }`); releases 16.3 s each;
vault: `submitCandidate` 375,269 gas, `finalizeAttested` 218,373, releases 420,572 and 361,396, each paid
999,000,000,000,000 wei; on that anvil the seal is refused under the deployed `checkpoint_b`'s id at RISC Zero's
verifier and a replayed release with `NullifierAlreadyUsed()`; `ZoneBridgeRc3LezCheckedTest` 5 of 5; `verify` equal.
The six zone-bridge suites together: 30 of 30.

### 10.2 The second review (22 November): a machine path in a record, the Lows

The second review found the fixes and re-runs of §10.1 as stated, and one Medium: a record of `zone-bridge-4`
named this machine's absolute path (pubnet.py's traceback in `net/proxy.log`; `zone-bridge.sh` scrubbed only
`logs/`). Its Lows: the README's `devnet.sh` command rewrote tracked records; L6's `create_new` was code, not a test;
a refusal expectation of `daemon_requests.py` that a refusal of the read also met; text for the integrator
(`docs/rows/fix-attestor.md`). On the build box (`mrun`) unless said, measured:

| Finding | What changed | Shown by |
|---|---|---|
| M1: an absolute path in `e2e/runs/zone-bridge-4/net/proxy.log` | the line scrubbed in the commit that added the record; `scripts/scrub_records.py`: every file of a run scrubbed (text, at any depth) and checked (binary files and what archives hold, too), refused if a home directory, this machine's `spike/` or an agent's folder is left; `zone-bridge.sh` ends every stage but `build` and `verify` with it (a `records` stage does only that), `return-leg.sh` its hook; the worktree's paths become repository-relative, a scratch directory's `tmp/`; the three runs of §10.1 rewritten so (path prefixes only) | `scripts/test_scrub_records.py` (laptop): 28 checks, among them `zone-bridge.sh records` on a planted run holding the review's `net/proxy.log` line; 15 mutants of the helper and the stage, all killed, the review's cause (`logs/` only) among them; `zone-bridge.sh records` over `zone-bridge-3`, `-4`, `-rc3-2`: nothing left, nothing rewritten |
| L1: the README's `devnet.sh` command rewrote `runs/devnet-n3` | it writes to `$TMPDIR/devnet-n3` (a replay; `runs/devnet-n3/` keeps its records) | in a copy: rc 0, no file outside `target/` and `fixtures/` changed; the six attestations sign the same bytes as `runs/devnet-n3`'s (`encoded` equal), and their `node` facts read `first_block` 202 (201 for `att-2-behind`) and `blocks_checked` 1, where the records read 1 and 202 (201), and `fork_digest` null (a field the records predate) |
| L3: `create_new` untested | `write_atomic_via(path, tmp, bytes)`, the half of `write_atomic` that opens the temporary file; a failed open no longer removes what is at the name (not this call's file) | `files.rs` `l6_create_new_never_opens_what_is_at_the_temporary_name` (a link to a file, a dangling link and another writer's file at the exact name); the review's `write-atomic-no-create-new` fails 1 test, `cleanup-removes-foreign` 1 |
| L4: `attestor_refuse_cert` met by a refusal of the read | the expectation includes the reply's `error`, word for word | natively: the 12 attestor requests met; the same request with an empty LEZ node, or without `lez_dir`: `refused`, the old expectation met, the new one missed |
| N5: the recorded tip's signer guard untested (liveness only) | test | `lez_check.rs` `n5_the_signed_tip_is_not_signed_again_once_its_signer_left_k`; `recorded-tip-signer-unchecked` fails 1 test |
| N1-N4 | the calls crate's `attest` doc (`state_file` once); the README's indexer `Block` fields (`bedrock_status`); what §10.1 re-ran; `zone-bridge.sh answers` (its header, and "What failed on the way") | |

`cargo test -p ferrata-attestor --locked`: **33 passed** (`attestor.rs` 12, `files.rs` 5, `io.rs` 2, `lez_check.rs` 11,
`withdrawals.rs` 3), and 33 with `cargo +1.88`; clippy `-D warnings` and rustfmt clean. Mutants, each on a copy:
**25 of 25 fail at least one test**: the branch's 16 with the counts of §10.1, except `write-atomic-fixed-tmp` 3 and
`signer-not-in-k-ok` 2 (a new test fails too); the second review's 8: `state-after-publish`, `lez-fault-refused`,
`keygen-mode-0644`, `write-atomic-no-create-new`, `recorded-tip-signer-unchecked`, `state-channel-unchecked` 1 each,
`cert-unchecked` and `own-key-not-in-k-ok` 2 each; and `cleanup-removes-foreign` 1. The module natively: the calls
crate 3 of 3 (none skipped), `attestor-e2e.sh` 24 of 24. Not re-run: the live stages (`zone-bridge.sh zone`,
`return-leg.sh`, `attestor-zone-live.sh`) and the module through Logos Core; their record check is the helper's,
shown by its test.

## 11. Four LEZ v0.4.0-rc3 sequencers, three of four keys, under RISC Zero 3.1 (RFP-022 F.7; 26 November)

[`e2e/runs/zone-bridge-rc3-multi/`](../../e2e/runs/zone-bridge-rc3-multi/README.md): `zone-bridge.sh` with `ZB_LEZ` =
LEZ v0.4.0-rc3 (upstream's four-sequencer devnet on one Logos Blockchain 0.3.5-rc.1 node), `ZB_N_MIN=4 ZB_T_MIN=3`,
the RISC Zero 3.1 programs. Four keys accredited, threshold 3; the 165 final inscriptions of LEZ blocks 2..=166 by all
four keys (46, 46, 38, 35); four attestors, 25 rounds each (`resources.py`: 0.17 CPU s, 0.2 s wall, 18.0 MiB and
35,714 bytes read a round, medians). Input `zone-28-164`: 136 blocks, 287 transactions, two burns, two imported
leaves, three of the four attestations of tip 164; 22,149,862 user cycles, journal equal to the native one. Refused
natively and by `checkpoint_b_0_3_5` in the executor: two of four, a key outside the set, one attestor twice, a stale
round of a kept key (`attestor-refusals --real`). Proofs on Modal's GPU through the bridge's prover client, killed once
while it waited and resumed on the same call: the prover's own wall time 92.7 s for the checkpoint, 5.8 s and 8.1 s for
the releases; the client's wait for each answer 249.7 s (the checkpoint's, from its restart), 569.1 s and 179.9 s;
seals of RISC Zero 3.1. Vault (floors 4, 3) behind verifier 3.1.0 on anvil: final, both releases paid 999,000,000,000,000 wei once, each
release's and the checkpoint's mined calldata called again refused (`NullifierAlreadyUsed`, `ParentNotFinal`);
`ZoneBridgeRc3MultiTest` 7 of 7 on the run's seals; on a fork of Sepolia at block 12,216,620: FORK REHEARSAL 31 OK.
A rotation to five keys on rc3 under the same programs worked at the zone (five keys accredited 30 s after the stake)
but its run was lost twice with its box (the box's client went away), so key-set changes stay shown on rc1 (§9).

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
  itself. (Since §10.1 a round reads only from its signed tip's slot, so an attestor's answers no longer span the
  run: `zone-bridge.sh answers` now only re-packs what the zone stage saved and recovers nothing after a failed zone
  stage.)
- §10.1: the first start of `zone-bridge-3` lost its local `mrun` client at ten minutes (a tool's default limit for a
  background command); the box kept running it until its container was recycled, and nothing came back (records made
  on the box return only through the client). The second start ran with its client kept for two hours; its container
  was preempted at about 02:02 UTC with the zone stage done (T1 = 111) and Modal ran the whole call again in a new
  container: the records are that run's. Box-made records are now fetched with `--no-sync --get` before any synced call.
- §9: the first start of the rotation run never reached the box (the laptop's Modal client timed out, "Deadline
  exceeded", and the box's container was recycled); the second start ran it.

## Not done

- On the zone of §8 and §9: a dropped inscription (B1's done gate; the silent sequencer is shown, §7); a rotation that
  removes a key; the attestor as the Logos module beside the bridge zone's sequencers (the binary ran there); the
  zone's checkpoint on Sepolia.
- The monitor's watcher reading attestations. (Rotations proposed and inscribed by a running committee: §9.)
