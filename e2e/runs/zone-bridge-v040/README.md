# zone-bridge-v040: the bridge round trip on LEZ v0.4.0, partial: both deposits claimed (one with a real proof) and both burns applied; no attestation, so no release

**Run:** `checkpoint/attestor/zone-bridge.sh zone e2e/runs/zone-bridge-v040` on a Modal build box, 28 November 2026,
03:59–04:18 UTC ([`logs/zone-bridge.log`](logs/zone-bridge.log)). The inputs:
- `ZB_LEZ=<spike>/upstream/lez-v0.4.0`: LEZ v0.4.0 `a255b42c`, unmodified.
- `ZB_BUNDLE`: the bundle of `zone-v040-standalone-1/`, plus this tree's v0.4.0 programs added by
  `checkpoint/attestor/zone-bundle.sh add-programs`. `ferrata_lez.bin` has sha256 `66466e83…` and `ferrata_bridge.bin`
  `6c943d7f…`, as `lez-040-core` builds them with `lez/scripts/lez-select.sh v0.4.0`; both were checked against their
  `ids.jsonl`.
- `ZB_DRIVER`: crown-driver built from `lez-040-core` `41614cfe`, sha256 `4dee0f3a…`. It speaks the v0.4.0 bridge's image
  id, and before a private claim it receives the private test account's pending genesis credit into its note.
- `ZB_PROGRAMS`: the RISC Zero 3.1 rule (A)/(B) and release programs of `docs/image-ids.md`
  (`ZB_CHECKPOINT_A=checkpoint_0_3_5`, `ZB_CHECKPOINT_B=checkpoint_b_0_3_5`). These are rc3's, which the vault pins in
  `params.json`.
- `ZB_FINAL_TRIES=3`.

The zone is upstream's four-key devnet at v0.4.0 on a Logos Blockchain 0.4.0-rc.2 node (V2 deployment settings,
dev-mode leader proofs), set up as in `zone-v040-four-keys-1/`. (`zone/logs/anvil.log` lists anvil's well-known
development accounts, from Foundry's public test mnemonic, as the base's `e2e/runs/endpoints-*` records do.) As in every zone-bridge run, two parts are STAND-INs: the
beacon chain on LEZ (the driver's committee) and the Ethereum side (a local anvil with Sepolia's chain id). Nothing was
sent to any public network.

**What applied on the v0.4.0 zone** ([`zone/logs/driver-a.log`](zone/logs/driver-a.log),
[`zone/logs/driver-b.log`](zone/logs/driver-b.log)):

| Step | Where | Result |
|---|---|---|
| The four sequencers started in phase with the node's epochs (`seq_aligned` reads v0.4.0's tracing-format logs) | LEZ | four keys accredited, heights 2/2/2/2 |
| The node's fork digest (`GET /chain/fork`) as session A's `bedrockGenesisId` | node 0.4.0-rc.2 | [`zone/chain-fork.json`](zone/chain-fork.json) |
| crown-a: the attestation and bridge programs deployed (program loader); the attestation program's STAND-IN Init, adopted in two steps | LEZ | applied; T0 = block 25 ([`lez/`](lez/)) |
| The v5 vault over the zone's four keys; deposits j0 (public) and j1 (private) and their captures | anvil | deployed and mined ([`deploy.json`](deploy.json), [`crown.json`](crown.json), [`lez/inputs/`](lez/inputs/)); `crown-driver check` accepted both captures |
| Four attestors, each certified by its sequencer's channel key for this deployment | | certified ([`zone/attestor-*/cert.json`](zone/)) |
| crown-b: the light client's stage-1 finality update (Open, Miller, Finish) | LEZ | applied, blocks 29–34 |
| bridge Init (pairing: the configId in the vault's slot 2); Register ETH; ImportRoot n=1 | LEZ | applied, blocks 36, 38, 40 |
| **bridge Claim of deposit 0 (public)** | LEZ | **applied**, block 42; the public test account's holding credited |
| The stage-2 finality update; ImportFees; ImportRoot n=2 | LEZ | applied, blocks 44–52 |
| The private test account receives its pending genesis credit into its note (real proof, 118 s) | LEZ | applied, block 66 |
| **bridge ClaimPrivately of deposit 1** (real proof by LEZ's wallet, 179 s, 238,416 bytes) | LEZ | **applied**, block 85 |
| **bridge Burn (public)**, 0.001 ETH of the public test account | LEZ | **applied**, block 87 |
| **bridge Burn (private)**, 0.001 ETH of the private test account (real proof, 151 s) | LEZ | **applied**, block 104 = T1; two burn notes written |
| The attestors' final rounds: a tip at or above T1 signed by the threshold | LEZ → attestors | **none signed** in 3 rounds (heights 105–107, all four sequencers level): `STOP: the attestors did not sign one tip at or above T1 = 104 in 3 rounds` |

**Not reached:** the checkpoint (rule (B)) and its proof, the vault's `submitCandidate`/`finalizeAttested`, and the
releases. This tree's attestor still reads LEZ blocks with the release candidates' decoder. On v0.4.0's blocks it
refuses at the first LEZ block, with `NotStrictlyOrdered { what: "actors" }` (shown in
`zone-v040-four-keys-1/attest-run.txt`), so it signs no tip. Its loop logs here
([`zone/logs/attest-loop-*.log`](zone/logs/)) are empty: the loop does not log a round's refusal. `lez-040-core` now has
checkpoint-core's v0.4.0 LEZ decoder (feature `lez-0-4-0`, which parses, re-hashes and links all 107 blocks of
`zone-v040-four-keys-1/`). The attestor (`ferrata_attestor tier`) and a v0.4.0 rule (B) program are not yet ported to it.
The vault here pins rc3's rule programs, which could not verify a v0.4.0 checkpoint in any case.

**What this does show:** on LEZ v0.4.0, with a zone of four sequencers on node 0.4.0-rc.2, the deposit-to-burn half of
the round trip works end to end. Ferrata's attestation program (its v0.4.0 build) takes its STAND-IN Init and two
light-client finality updates. The bridge program (its v0.4.0 build) takes the pairing, the ETH registration, two root
imports, the fee import, a public claim, a private claim proven for real by LEZ's wallet and verified by the sequencers,
and both a public and a private burn. The return half (attest → checkpoint → release) waits on the attestor's port.

**Earlier attempt (commit `f7764d9a`, replaced by this record):** the same stage with crown-driver `7fed77f6`. It stopped
before sending ClaimPrivately (`Fee payer 44KVvzvh… holds 0`). At v0.4.0 the private test account starts with an empty
note and is credited through publications, so the claim could not pay its fee from the note. The driver of `41614cfe`
receives that credit first: the block 66 transaction above.
