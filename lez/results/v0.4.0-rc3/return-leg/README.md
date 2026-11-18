# The return leg on LEZ v0.4.0-rc3: a checkpoint over a zone whose Bedrock runs Logos Blockchain 0.3.5-rc.1

LEZ v0.4.0-rc3 pins Logos Blockchain 0.3.5-rc.1 (`f6d4267f`, the release Logos's devnet runs), which puts a 32-byte
fork digest in front of every Mantle transaction's operations and hashes it into the transaction hash
([`../README.md`](../README.md), "Logos Blockchain 0.3.5-rc.1 (LEZ #1095) and the checkpoint guests"). The deployed
checkpoint guests read the `d480efc6` layout and refuse such transactions. This directory records the return leg over
such a zone, in stages: a second build of the two checkpoint guests that reads the new layout; a rule (B) checkpoint
over the real blocks of an rc3 zone; the checkpoint proven, wrapped and paid out by FerrataVault v5 on a local anvil.
Unaudited, not for production, as the rest of the repository.

Everything ran on 18 November 2026 on the Modal build box of this worktree (workspace `carl`; a gVisor container
listing 32 processors and 64 GiB; `nproc` reports 2, so the provers ran with `RAYON_NUM_THREADS=32`). Nothing was sent
to a public network; the one network read was the Sepolia vault's immutables over public JSON-RPC (`compare --vault`).

## Stage 1: checkpoint-core reads the 0.3.5-rc.1 layout (a second build)

- **Where the deployment pins the digest: `lezConfigId`.** For a zone on 0.3.5-rc.1 the configuration's
  `bedrockGenesisId` is the zone's fork digest (what `GET /chain/fork` of its node answers), so `lezConfigId`, which
  the vault pins and the journal carries, commits to it; vault v5 and its 17-word journal serve the new build
  unchanged, and so does the final bridge program, which binds burns and messages to `lezConfigId` computed with the
  same v1 preimage. A journal word was rejected (a new vault); a new preimage field or tag too (a `lezConfigId` the
  bridge's deployment tools do not compute). The digest is `BLAKE2b-256("FORK_DIGEST_V1" ‖ genesis id ‖ era
  digests)`: unlike a genesis id it is checked, on every Bedrock transaction the program reads. One digest is pinned:
  after a Bedrock era change the program refuses the zone's new inscriptions (fails closed; the retirement plan
  applies). Reasons in full: [`checkpoint-core/src/bedrock_0_3_5/mod.rs`](../../../../checkpoint/crates/checkpoint-core/src/bedrock_0_3_5/mod.rs).
- **What the build changes.** Each Mantle transaction the program reads (`T`'s inscription, rule (A)'s support
  inscriptions, channel configuration changes) is `fork_digest ‖ ops`, hashed as `BLAKE2b-256("MANTLE_TXHASH_V1" ‖
  fork_digest ‖ ops)`, its digest required to be the deployment's; a channel configuration whose threshold exceeds its
  key count is refused, as 0.3.5-rc.1 refuses to decode it (logos-blockchain #3781). Rules (A) and (B) over that
  layout are copies of `rule.rs` and `rule_b.rs` (whose panic locations are compiled into the deployed guests).
- **How it is built.** A cargo feature of checkpoint-core, `bedrock-0-3-5`, off by default, in new files only
  (`src/bedrock_0_3_5/`; `lib.rs` gained two lines at its end), used by new guest crates in their own workspace
  (`checkpoint/guests-0-3-5/`, lockfile = `checkpoint/guests/`' but the members) built by `checkpoint/methods-0-3-5`:
  `checkpoint_0_3_5` `0x259cd89e…9b0f` and `checkpoint_b_0_3_5` `0xafbf4023…3bcf`
  ([`docs/image-ids.md`](../../../../docs/image-ids.md), their own heading; not deployed on Sepolia).
- **The default build did not move.** `scripts/reproduce-image-ids.sh build`, then `compare --vault 0x04eE…e44c`, at
  this branch on the box: **14 of 14** guests equal `docs/image-ids.md` and **6 of 6** the crown run's ids, the Sepolia
  vault's immutables read back equal ([`checkpoint/results/bedrock-0-3-5/box1-compare.log`](../../../../checkpoint/results/bedrock-0-3-5/box1-compare.log)).
- **Tests** ([`tests.log`](../../../../checkpoint/results/bedrock-0-3-5/tests.log)): the default build's three tests that
  record today's refusal still pass, with and without the feature; the second build's counterparts on upstream's own
  codec fixtures at `f6d4267f` (`tests/bedrock_0_3_5.rs`, `new_build`: it frames the 0.3.5-rc.1 transaction and finds
  its inscription; its hash is what a 0.3.5-rc.1 signer signs, written out with the `blake2` crate, and a signature
  over it verifies while one over the operations alone does not; another digest is refused; the `d480efc6` layout is
  refused; upstream's channel configuration fixture decodes, and the `d480efc6` one, threshold 21 over two keys, is
  refused) and over its rules (`tests/bedrock_0_3_5_rules.rs`: rule (A) and rule (B) over 0.3.5-rc.1 transactions
  give, word for word, the default rules' journals over the same blocks, two burns and a rotation among them; the
  `d480efc6` layout, another digest and a rotation to an unreachable threshold are refused, the last applied by the
  default build). `clippy -D warnings` clean on both builds. The first run of the new tests failed two of them on a
  mistyped fixture constant (kept: `box1-tests-first-run.log`).

## Stage 2: a rule (B) checkpoint over the real blocks of an rc3 zone

| | |
|---|---|
| The zone (records: [`zone/`](zone/environment.md)) | LEZ v0.4.0-rc3's own `sequencer_service` (built from the tag `bafd07be`, sha256 `be2ac354…`), **one sequencer**, and its Logos Blockchain node **0.3.5-rc.1** (release asset of `f6d4267f`, sha256 `2f301729…`), **one validator, leader proofs in dev mode**, as the sessions of [`../real-proofs/`](../real-proofs/README.md); run with `lez/live/run.sh` (phases `info hook export`) and the hook [`lez/live/hooks/return-leg.sh`](../../../live/hooks/return-leg.sh), 10:12–10:35 UTC; 129 blocks exported, all final on Bedrock; no head rewind, no given-up inscription |
| The fork digest | `GET /chain/fork`: `c025c6d2…58ac`; every one of the channel's 125 final inscriptions carries it; session A's `bedrockGenesisId` is it, so `lezConfigId` `0xfe93d41d…4458` commits to it |
| The programs | the final ones of `docs/image-ids.md` (rc1 builds, checked by sha256 and `r0vm --id`): attestation program `0x816d1fef…`, bridge `0xccde6171…`, deployed through rc3's loader |
| Session A (crown-driver `crown-a --fork-digest`) | the attestation program, the bridge and the STAND-IN beacon instance deployed; T0 = block 24 |
| Ethereum side (local anvil) | FerrataVault v5 in mode (B) behind **RISC Zero's Groth16 verifier** and its emergency stop (DeployVault), pinning `checkpoint_b_0_3_5` and `release`, nMin 1, tMin 1, genesis = the state at T0 over the zone's one key; deposits j0 (public) and j1 (private) and their captures |
| Session B (crown-driver `crown-b --private real`) | `Init` (block 37), `Register` ETH (39), `ImportRoot` n=1 (41), `Claim` of deposit 0 (44), `ImportFees` (54), `ImportRoot` n=2 (56), **`ClaimPrivately`** of deposit 1 (proven for real by LEZ's wallet in 306.3 s, 252,267 B; block 89), **`Burn` public** (91), **`Burn` private** (proven in 317.5 s, 255,031 B; block 125); T1 = 125 |
| The attestor | `ferrata_attestor` beside the sequencer, its key (OS randomness, not recorded) certified by the sequencer's channel key for this deployment; 17 rounds, none refused, rising final tips; the final round: tip 125 (= T1), blocks 1..=125 checked, each inscription in the 0.3.5-rc.1 layout with digest `c025c6d2…` |
| Node answers | the attestor's final round: slots 2,375,341..=2,376,251, 4,081,638 bytes ([`zone-bridge-rc3/zone/node-answers.tar.zst`](../../../../e2e/runs/zone-bridge-rc3/zone/node-answers.tar.zst)) |
| The input (`realblocks-crown`, from the node's answers and the attestation alone) | blocks 25..=125 (101 blocks, 217 transactions, 608,482 B), T = 125, n = 1, t = 1; all 125 final inscriptions parsed, re-hashed, linked, framed, BLAKE2b-hashed with their digest and `verify_strict`-checked by checkpoint-core's second build; **2 burns, burn root `0x78aae803…7759`**, imported leaf count 2; parent state `0x8805ccd3…` (= the vault's `genesisState`), new state `0x27848dc8…`; journal sha256 `2d3b8566…` |
| r0vm executor (`checkpoint_b_0_3_5`) | **18,971,326 user cycles**, 21 segments; **journal equal to the native one**; 0 of 1 attestations: refused by the collector and by the guest (`Core(InsufficientAttestations { have: 0, need: 1 })`) |
| The burn root is the bridge program's | `ferrata-e2e crown-release` recomputes the burn tree with the bridge core from the bridge program's two burns: each leaf and the root `0x78aae803…` equal the checkpoint's |
| The deployed `checkpoint_b` on the same input | refused in the executor: `UnsupportedOpcode { opcode: 37 }` (the digest's first byte read as the operation count, its second as an opcode) |

One sequencer, as the brief allows: the key set is one key (n = 1, t = 1) and its one attestor; a zone of several
sequencers, their attestors and a committee's rotation ran on rc1's node ([`checkpoint/attestor/RESULTS.md`](../../../../checkpoint/attestor/RESULTS.md) §8, §9).

## What this does not show

- Logos's devnet: one local validator with leader proofs in dev mode, a zone of one sequencer, a stand-in beacon chain
  (the driver's committee finalizes the anvil's blocks) and a local anvil with Sepolia's chain id; W and the release
  delay are warped. Not a public network, not testnet 0.4.
- A Bedrock era change: the zone ran in one era, one digest throughout; what happens at an era change is reasoned (the
  program refuses the new digest), not run.
- Rule (A) over this zone (built and tested in the second build, not run here), a rotation on 0.3.5-rc.1 (the
  threshold check is tested on hand-built and upstream fixtures only), several sequencers on 0.3.5-rc.1.

## Files and re-checks

- [`e2e/runs/zone-bridge-rc3/`](../../../../e2e/runs/zone-bridge-rc3/README.md): the run (`identity.json`,
  `params.json`, `crown.json`, `deploy.json`; `lez/` the sessions; `zone/` the attestor's records and the node's
  answers; `checkpoint/` the input, its meta, the executor runs; `logs/`). [`zone/`](zone/environment.md) here: the
  zone's own records (`environment.md`, the blocks, `logs/`, the configuration diffs).
- Offline re-check (no node, no chain; from the worktree root, after `scripts/reproduce-image-ids.sh build --out OUT`
  and `cd checkpoint && cargo build --release -p methods-0-3-5 --locked`, with the R0BF programs and their `ids.jsonl`
  in one directory `P`, `ferrata-e2e program-binary` making the two new ones): `ZB_PROGRAMS=P
  ZB_CHECKPOINT_B=checkpoint_b_0_3_5 ZB_N_MIN=1 ZB_T_MIN=1 ZB_LEAVE_OUT=none bash checkpoint/attestor/zone-bridge.sh
  verify e2e/runs/zone-bridge-rc3` re-derives the input, the journal and `burns.json` from the recorded node answers
  and attestation and compares them; `cargo +1.88 test -p checkpoint-core --features bedrock-0-3-5` in `checkpoint/`.
- The box scripts that ran it: `spike/tmp/rc3-return/box*.sh` beside the repository (not in it).
