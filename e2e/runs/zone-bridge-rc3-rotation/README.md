# zone-bridge-rc3-rotation: a key rotation by the zone's committee, on four LEZ v0.4.0-rc3 sequencers, under RISC Zero 3.1

27 November 2026, the Modal build box: the run of [`zone-bridge-rc3-multi`](../zone-bridge-rc3-multi/README.md) (read
it first: the zone, the attestors, the stand-ins and the stages are the same; ETH, the bridge program
`ferrata_bridge_b1`) with `ZB_ROTATE=1`, as [`zone-bridge-2`](../zone-bridge-2/) did on LEZ v0.4.0-rc1 under RISC Zero
3.0 (EVIDENCE row 101): `checkpoint/attestor/zone-bridge.sh` with `ZB_LEZ` = LEZ v0.4.0-rc3,
`ZB_CHECKPOINT_A=checkpoint_0_3_5 ZB_CHECKPOINT_B=checkpoint_b_0_3_5 ZB_N_MIN=4 ZB_T_MIN=3`, the RISC Zero 3.1
programs of [`docs/image-ids.md`](../../../docs/image-ids.md) (`checkpoint_b_0_3_5_31` `0x8917f58e…`, `release_31`
`0xf279af41…`).

**The rotation** (`logs/zone-bridge.log`, `zone/channel-before-rotation.json`, `zone/channel-after-rotation.json`):
after session A, a fifth sequencer's key was staked on LEZ (upstream's `sequencer_service stake`); the zone's committee
proposed the channel's five-key set, and the channel accredited five keys 32 s after the stake was sent; the fifth
sequencer and its attestor started, and session B ran on five. In the checkpoint's input the configuration change is
as it was inscribed (`checkpoint/realblocks-crown.txt`: node block `6620f7e1…`, 5 keys, threshold 4, signed by key
indices 0, 1 and 2 of the set before it), so the proof applies it to the parent's four keys.

**The checkpoint** `zone-32-176` (144 blocks, 304 transactions; two burns, two imported leaves): four of the five
attestations of tip 176 (`checkpoint/attestor.json`: n = 5, t = 4; the attestor left out is the first, an original
key, so the stale-tip refusal below has a real earlier-round attestation by a key of the parent set: the fifth key
joined with the rotation); RISC Zero 3.1's executor matches the native journal (27,022,153 user cycles:
`checkpoint/exec.json`); all five accepted too (`exec-all.json`); three of five refused by the collector and the
guest; t − 1 attestations, an outsider, a repeated attestor and an earlier round (tip 33) refused natively and by the
guest (`checkpoint/refusals/`). The checkpoint and both releases proven on Modal's GPU through the bridge's prover
client (`prover/`: killed while it waited for the checkpoint's proof, resumed on the same call; 193.8 s, 14.4 s and
18.9 s; PROVER KILL-RESUME OK), each wrapped to a 260-byte Groth16 seal with RISC Zero 3.1's selector `0xe71e6620`.

**The vault** (`vault-checks.json`, `logs/`): v5 in mode (B) on a local anvil with Sepolia's chain id, its genesis over
the four keys at T0, floors nMin 4 and tMin 3, RISC Zero's verifier 3.1.0 behind its emergency stop: the checkpoint
submitted and, after W, final (its burn root accepted); both releases paid 999,000,000,000,000 wei to the recipient
their burns name, once; each release's and the checkpoint's mined calldata called again: refused
(`NullifierAlreadyUsed` twice, `ParentNotFinal`). The same seals in forge,
`vault/test/real/ZoneBridgeRc3Rotation.t.sol` (fixture `zone-bridge-rc3-rotation.json`): the proof's key set is the
committee's five, keeping the four; a vault whose genesis is the new set refuses it; and the tests of the rc3 fixture
(the checkpoint, both releases once each, the floors, a flipped seal byte, redirected and replayed releases): 9 of 9.
`verify` (`logs/verify.txt`): the checkpoint input, journal and burns re-derived from the recorded node answers and
attestations, equal; the GPU journals the native one and the statements'; it does not verify the seals itself (the
vault stage and the forge test do).

**Stand-ins** as in every `zone-bridge.sh` run: the beacon chain on LEZ (a committee crown-driver holds), the Ethereum
side (anvil; W and the release delay warped), leader proofs in dev mode; all five sequencers on one box. Validity rests
on t of n accredited sequencer keys (rule (B)). The first attempt of the input stage stopped on the stale-tip refusal's
key check (the default left-out attestor was the fifth key, accredited only by the rotation); `zone-bridge.sh` now
leaves out the first attestor after a rotation, and the stage ran again from the same zone records.
