# The crown run: the whole round trip on the final programs (design)

Status (12 Nov 2026, 16:23Z): **run live on Sepolia to DONE on the final programs** as `crown-live-3`
([`e2e/runs/crown-live-3/`](../runs/crown-live-3/README.md): the records, the two starts and what each of the 13
transactions shows; EVIDENCE rows 91 and 92): FerrataVault v5 `0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c`, the
programs of the first table below, verify 14 of 14 on the runner and its 12 chain checks again from the laptop; one
stop on the RPC endpoint's lagging reads, before the vault, resumed by the same `start` from 148ff07 (the hardened
retries; none fired again). Its light client started from the fresh checkpoint
[`crates/ferrata-spectests/fixtures/sepolia-2026-11-12`](../../crates/ferrata-spectests/fixtures/sepolia-2026-11-12/SOURCE.md)
(Gloas bootstrap at slot 11,562,112, period 1411; an `Init` from it is valid until 28 Nov 2026 03:13 UTC), which
`crown-driver` reads from the bundle's fixture; session A ran on it first through this cloud run in fork mode
([`e2e/runs/crown-sessiona-sepolia-1/`](../runs/crown-sessiona-sepolia-1/README.md): `Init` and the catch-up applied,
23 of 23 transactions). `crown-live-2` ([`e2e/runs/crown-live-2/`](../runs/crown-live-2/README.md)) had stopped at its
first stage, before anything was sent to Sepolia: the programs' weak-subjectivity bound refused its 38-day-old
checkpoint (`crown sepolia Init` reverted, "Execution outside of the validity window"); a new run name, because a
resumed `crown-live-2` would have kept its own copy of the old bundle (`prepare_inputs` copies a run's bundle once). The
final programs (branch `integration7`, the guests as of 4abfbf5 (no later commit changes a guest): the thirteen merged
branches, guest builds independent of the build path) are those of the first table below, which any checkout rebuilds
to the same ids (`scripts/reproduce-image-ids.sh`, [`docs/image-ids.md`](../../docs/image-ids.md); `compare --vault
0x04eE8FF1…` after a `build`: 6 of 6, `e2e/runs/crown-live-3/compare-vault.txt`); rehearsed on an anvil fork of Sepolia
to DONE as `crown-rehearsal-5` (the stand-in chain on LEZ; 19 stages in 23 min 20 s, verify 14 of 14,
[`e2e/runs/crown-rehearsal-5/`](../runs/crown-rehearsal-5/README.md); `crown-rehearsal-4` before it stopped at verify on
stale pinned ids; `verify.py` now takes them from the run's own records, `params.json` and `programs/ids.jsonl`).

The first live run, `crown-live-1` (11 Nov 2026, DONE at 15:14Z;
[`e2e/runs/crown-live-1/`](../runs/crown-live-1/README.md): the records, the four starts and what each of the 13
transactions shows; EVIDENCE rows 41 and 42): FerrataVault v5 `0xfE3199fa2634413CbAC317371076Cae3EB2d9E54`, the
programs of crown-live-1 (the second table below), verify 14 of 14 on the runner and its 12 chain checks again from the
laptop; three stops on the RPC endpoint's lagging reads, each resumed by the same `start`. Built on branch
`integration5`, whose programs carry the time fix (LEZ-side time from attested records; rule (B) checks the tip's
producer inscription) and the security fixes (`secfix-lez`, `secfix-vault`); the guests were rebuilt from that tree at
the crown build path and rehearsed to DONE on them first (`e2e/runs/crown-rehearsal-3/`, on a fork, verify 14 of 14);
rehearsals 1 and 2 ran the earlier programs. Commands: "Commands" below. One cloud run, resumed by the same `start`
after a stop, that crosses Sepolia -> LEZ -> Sepolia with the final programs and real proofs everywhere, and a verify
script that re-checks every step from public data and the recorded blocks.

## Programs (built on the build box; since `integration7` the ids do not depend on the build path)

| Program | Source | Role |
|---|---|---|
| attestation program | `lez/guests/program` (as `integration` 5a94634) | Sepolia light client on LEZ (Gloas) |
| merged bridge program | `lez/guests/bridge`, `lez/bridge-wire` (as `integration5`: the time fix and the security fixes; `0x0e9dcd79…bbb6` at the crown build path) | registry, fees, caps and freezes on attested time, Sunset, Gloas header, holder PDAs |
| `checkpoint_b` | `checkpoint/` (after the `ImportRoot` fix 12d17e3) | rule (B) checkpoint over the session's blocks |
| release | `bridge/` | one release per burn |
| FerrataVault v5 + FerrataRegistry | `vault/src` | mode (B), RISC Zero's Groth16 verifier behind an emergency stop |

**The programs of `crown-live-2` and `crown-live-3`** (the final programs: `integration7`, guest sources as of 4abfbf5, built on the Modal build box with
`scripts/reproduce-image-ids.sh build`; the same commit built at two other paths with two other `$HOME`s gives
byte-identical binaries and these ids, [`docs/image-ids.md`](../../docs/image-ids.md); pinned in `cloud/ctl.py` and
`cloud/drive.py`; on the Modal Volume since `setup` on 12 Nov 2026; the LEZ bundle of these programs for
`crown-live-3`: 138,872,352 bytes, sha256 `0013bab125141c9d…`, uploaded by `setup` at 10:56Z: `crown-live-2`'s bundle
(139,304,302 bytes, sha256 `4dd813e28261ee96…`) repacked by `lez/crown/build.sh repack` with crown-driver `44024e7e…`
from this tree and the fixture `sepolia-2026-11-12`, its programs byte for byte):

| Program | Image id |
|---|---|
| attestation program `ferrata_lez.bin` (lc-a1: successors, root id, weak-subjectivity bound; attest-a2's wire) | `0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df` |
| bridge program `ferrata_bridge.bin` (bridge-controls: B2 controls and roles; the merged wire) | `0xccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd` |
| `checkpoint_b` (rule (B) with the tip's producer inscription; sources as crown-live-1's) | `0xf8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e` |
| `checkpoint` (rule (A), the vault's other immutable id) | `0xb9c1e6a834879f6be79d09d728523565846018629b82f66986eb58fab4f52c84` |
| `segment` | `0x11e888e94b9520b08477ba5c5c29cc46bdffc7b1e7c5fe822eb0776f40c54592` |
| release (`NOut` v2; sources as crown-live-1's) | `0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9` |

**The programs of `crown-live-1`** (`integration5`, built on the Modal build box at the maintainer's checkout path,
before guest builds were path-independent, from commit db9f034; their ids reproduce only at that path and commit,
[`docs/guides/verify.md`](../../docs/guides/verify.md) §2; `scripts/verify-sepolia.py` takes them from the run's own records):

| Program | Image id (crown-live-1) |
|---|---|
| attestation program `ferrata_lez.bin` (unchanged: byte-identical to the earlier crown build's) | `0x2228bb462b83032ade1c5de62d381b8dcffe9d10d9e977dce9e9fc8da31467cf` |
| bridge program `ferrata_bridge.bin` (time fix + C-1, H-1, M-3, I-4) | `0x0e9dcd79482f9e0b83762b965d7e1d4319ffeba8a4bdfbdff148bbeaba79bbb6` |
| `checkpoint_b` (rule (B) with the tip's producer inscription) | `0xa475e8f9aaa41d0ae3bf9a20ae7d937ebc637a961808765f0ff175158760d3b2` |
| `checkpoint` (rule (A), the vault's other immutable id) | `0xcf38b945a70b66d9132fb493b4baf1db308de73ccb8b1108e1c1f8a4363525e6` |
| `segment` | `0xd2b30667292418ccf3cf68ee4fa2dc9ed7873de4034fcf1fb2ed3a6305a61cca` |
| release (`NOut` v2) | `0x17257f91bca70abf7a5476b498d687e95b6fb4d59e75b6bca1d46877ac9bf5cd` |

The LEZ bundle of these programs: 139,311,009 bytes, sha256 `7cc85db0266aa1ee…` (`spike/tmp/integ5/crown/lez/bundle.tar.zst`
beside the repository; the R0BF programs and `ids.jsonl` in `spike/tmp/integ5/crown/r0bf/`). The checkpoint decoder reads bridge variants 0-3 (Burn, ImportRoot, Sunset, Echo), byte-identical
in the merged wire (`checkpoint/crates/checkpoint-core/src/wire.rs`, `lez/harness/tests/bridge_burn.rs`).

## Stages and where each runs

our Modal workspace, app `ferrata-crown` (CPU runner, GPU session, wrap), Volume `ferrata-crown`. Builds on
the build box (`mrun --profile <yours>). The laptop only starts, reads status and fetches.

| # | Stage | Machine | Persists (Volume) |
|---|---|---|---|
| 0 | pre-flight: deployer nonce N (pending = latest), balance, gas price, node-priced plan; the owner types yes | runner (CPU) | `run.json` |
| 1 | LEZ session A: fresh devnet (LEZ v0.4.0-rc1 sequencer + Logos Blockchain node, LEZ's local config, dev-mode leader proofs), `--create-channel`, deploy attest + bridge, Sepolia instance Init + 2 adopts from the bundle fixture's checkpoint (period 1411), its period update(s); stop cleanly; T0 = tip | CPU function | `lez/gen-1.tar.zst`, `lez/session-a/` (blocks, channel key's public part, deployment settings, T0) |
| 2 | identity: bedrockGenesisId = sha256(deployment-settings), channelId 0x01..01, keys [channel pk], postingTimeframe 0, configTipHash 0 (from `/channel`), genesisState(T0), vault = CREATE(deployer, N+2), configId | runner | `identity.json`, `params.json` |
| 3 | Sepolia deploy (script/deploy.sh, FERRATA_MODE=B, eip8037, verifier reused): stop N, library N+1, vault N+2, token N+3, register ETH N+4 (relay 0.2 %, checkpoint 0.1 %), register token N+5, renounce N+6 | runner | `deploy.json`, manifest |
| 4 | deposit j0: 0.001 ETH (+ 0.000002 relay fee) public to LEZ public test account 0; wait finality; capture 1 (vault slots 0,1,2 + the ETH registry and fee slots, deposits, updates) | runner | `lez-capture-1/` |
| 5 | deposit j1: 0.001 ETH private to LEZ private test account 0; wait finality; capture 2 | runner | `lez-capture-2/` |
| 6 | LEZ session B: restore gen-1; catch-up + finality updates of captures 1, 2; bridge Init (pairing: slot 2), Register ETH, ImportRoot n=1, n=2; Claim j0 (public), ClaimPrivately j1 (real proof); Burn public (account 0), Burn private (private account 0, real proof); wait Bedrock finality of the tip T1; stop; save | CPU function (`lez_session` in `cloud/app.py`: 16 cores reserved, up to 32; 16 to 64 GiB): LEZ's wallet proves both private transactions with the bundle's `r0vm` 3.0.5 on the CPU against the live sequencer (`CROWN_R0VM` may name a CUDA r0vm; the runner sets none) | `lez/gen-2.tar.zst`, `lez/session-b/` (driver export: blocks, txs, receipts) |
| 7 | attestor + checkpoint: the channel key certifies the attestation key (attestor-core), which signs FinalTip(T1); `checkpoint_b` input over T0+1..=T1 from the node's blocks and the driver's record; real proof (GPU), identity_p254 (runner CPU), Groth16 (prover image) | GPU function + runner + wrap function | `checkpoint/` |
| 8 | submitCandidate (N+9); wait W = 3600 s; finalizeAttested (N+10) | runner | manifest |
| 9 | release proofs for both burns (release program, CPU, during W) + wraps; after the release delay (120 s) release ETH twice (N+11, N+12) | runner + wrap | `releases/` |
| 10 | verify (both chains, from public data and the recorded blocks), DONE | runner (re-runnable on the box) | `verify.txt` |

**The vault's deployment parameters in this run** are `vault/script/deploy.sh`'s testnet defaults, except Δ (from
`identity.json`): the runner (`e2e/crown/sepolia.sh deploy`) sets no other: revocation window 1 day, release delay
120 s (minimum 0), maximum freeze 30 days, the freeze authority an address nobody holds a key for, no watchers, and
no cap set. The proposal's production values differ (a 7-day window, caps, a multisig freeze authority and three
watchers); this run shows the mechanism, not that configuration.

Why the devnet comes first: the vault's `genesisState` is the checkpoint state at a real LEZ tip (tip id, tip hash,
key set of the channel, configTipHash; vault/test/real/RealBlocks.t.sol `_state`), and `setup --create-channel`
draws a random channel key (7 setups, 7 keys). So the devnet exists before the vault and survives the deposits and
their finality waits as a saved generation on the Volume (as lez/lab does: a restart after 54 min worked).

## Identity of the configuration (merged wire, `lez/bridge-wire` `BridgeConfig::id`)

configId = sha256("ferrata/config/v1" || bedrockGenesisId || channelId || bridgeHeader || bridgeImageId ||
evmChainId (u256) || vault || attestationRootId || salt), 261 bytes, where
- bridgeHeader `0x8face982…e68d`, attestation program account `0x86ce4d7f…a04c` (derived keys of the labels
  `program/bridge/header`, `program/attest/header`; independent of the images);
- attestationRootId = sha256("ferrata-lez/v1/bridge/attestation-root" || attestation account || instance id);
  instance id `0xa5fb2591…2b5` (sepolia_config(2) in the Gloas layout, the checkpoint of `sepolia-2026-11-12`: slot
  11,562,112, period 1411; `crown-driver check --chain sepolia` prints it) unless the crown build says otherwise
  (`crown-live-2`'s `0x88df8428…c358`, from the period-1378 bootstrap, never existed: the program's weak-subjectivity
  bound refused that 38-day-old checkpoint);
- salt = sha256("ferrata-lez/v1/bridge/controls" || borsh(controls)); controls = LEZ admin, freeze authority,
  fee recipient (throwaway test accounts), floor and caps (unlimited);
- bedrockGenesisId and channelId: the devnet's real values (stage 2), not the inbound plan's tags;
- vault: CREATE(deployer, N+2), computed at stage 0 from the deployer's nonce and checked after stage 3.
The vault takes configId as `lezConfigId`; the LEZ Init proves slot 2 (`storedLezConfigId`) at a finalized block.

## The deployer, its nonces and the money

Same throwaway deployer as sepolia-live-2 (`0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07`, keystore in the Modal Secret
`ferrata-sepolia-key`). RISC Zero's Groth16 verifier of that run (`0x8f0Af3Bf302C64d57f840127F94d9c123a8Fa2A0`,
nonce 1, SELECTOR 0x73c457ba, read 10 Nov 23:43Z) is reused (FERRATA_GROTH16_VERIFIER). The plan assumed nonce 17
(vault `0x57b4…Ece6E`); the live run found nonce 10 (sepolia-live-2 had stopped after deposit 0), so N = 10: stop
`0x2Ea6457503024bB8dD99764F1bB5C7aFf9726Ea9`, library `0xe5D797C9f585a9c1b5e530814f9F25bED24a4B15`, vault
`0xfE3199fa2634413CbAC317371076Cae3EB2d9E54`, then nonces 13-22 (13 transactions; crown-live-1 left the deployer at
nonce 23). crown-live-3 found nonce 23, so N = 23: stop `0x1dB3fa8041E8178f1608c2F7dF627d7EC16C8221`, library
`0xa75e653BdaD23d198Ea357277b715aD8A9687673`, vault `0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c`, then nonces 26-35
(13 transactions; the deployer is now at nonce 36).
The run refuses to start unless latest = pending nonce and the CREATE addresses have no code. Ether: 2 x
1,002,000,000,000,000 wei of deposits; each release paid back 0.000999 ETH (the 0.1 % checkpoint fee is credited to
the prover's `owed`, the 0.2 % relay fees stay escrowed); gas 74,947,322 (measured; planned ~50-100 M), 0.0000165 ETH
at the prices paid (65-72 wei per gas, release 0 25,000,044); crown-live-3: gas 74,947,202, 0.0000248635 ETH (five
of the deployment's seven at 140,711 to 640,703 wei per gas, the other eight transactions at 24 to 543). Modal (cap 20 USD; ~2.4 spent before this work):
runner ~4 h of 2 cores (~1 USD at the nonpreemptible rate), GPU session ~1 h (~1 USD), wraps cents.

## Built and measured so far (build box = Modal, our Modal workspace, through `mrun`)

| What | Result |
|---|---|
| Guests at this path | attestation program `ferrata_lez.bin` image `0x2228bb46…67cf` (602,904 B); merged bridge `ferrata_bridge.bin` image `0x0be48090…25df` (507,088 B); `checkpoint_b` `0x7e51065f…7754`; release `0x20595dc5…2625` |
| Fresh devnet (LEZ v0.4.0-rc1 sequencer + Logos Blockchain node, dev-mode leader proofs) | deployed both programs in 14 blocks, all Finalized on Bedrock, 147 s; channel key, tip, deployment settings readable (the vault's identity inputs) |
| `identity.py` | genesisState reproduces the real-blocks proof's parent state; commitments reproduce sepolia-live-2's |
| Sepolia stages on a fork (stand-in identity, accept-all verifier on the fork only) | `deploy.sh` mode B, 2 deposits with fees, submitCandidate, finalizeAttested after 3,601 s, 2 releases: `e2e/runs/crown-fork-3r/README.md` |

## What remained after piece 2 (all done: crown-lez, crown-proofs, crown-cloud; the merged programs: integration5)

1. A crown driver on the merged wire (`lez/live/crown/`: the live driver's wire-free modules by `#[path]`, a new
   `crown.rs`): session A (deploy, instance Init, period updates of the repository fixture) and session B (the
   captures' updates and finality updates, bridge Init, Register ETH, ImportRoot n=1, n=2, Claim, ClaimPrivately,
   Burn public, Burn private), porting `lez/harness/src/bridge.rs`'s builders to the live sequencer.
2. Session save/restore between A and B (lez/lab/session.sh's generations on a Volume).
3. Attestor + `checkpoint_b` input from the session's own blocks (checkpoint/realblocks with the run's configuration
   instead of labels and chain id 31337).
4. The GPU session: LEZ proves through r0vm (risc0-zkvm `client` only), so r0vm 3.0.5 with CUDA as
   RISC0_SERVER_PATH makes both private proofs GPU proofs against the live sequencer.
5. Release inputs from real burns (bridge/e2e `release` uses the v1 burn MAC; the merged notes use v2).
6. The capture with the ETH registry and fee slots (register_slots) beside slots 0-2.
7. The cloud app (`ferrata-crown` on our Modal workspace), `e2e/crown.sh`, the verify script, one full rehearsal to DONE.

## Commands (for the owner)

The cloud app (`e2e/crown/cloud/`, Modal app `ferrata-crown` on our Modal workspace) runs the whole round trip from
one `start`; it needs this machine only to start, to look and to fetch. Rehearsed to DONE on an anvil fork of Sepolia
three times: `e2e/runs/crown-rehearsal-1/` and `-2/` on the earlier programs (branch `crown-rehearsal`; 2 stopped on
purpose and resumed), and **`e2e/runs/crown-rehearsal-3/` on the programs of this tree** (branch `integration5`, commit
fecd9c7; 19 stages in 24 min 19 s, no stop, verify 14 of 14).

Before the live start of `crown-live-3` (as it stood after the fresh checkpoint, 12 Nov 2026, 11:10Z; the run is done):
- this tree (branch `integration7`, from its worktree); `vault/lib` present in it (forge-std 1.10.0, OpenZeppelin
  5.4.0, risc0-ethereum 3.0.0; git ignores it and `start` packs it);
- the LEZ bundle with the fresh checkpoint (sha256 `0013bab125141c9d…`) and the R0BF programs of the final programs
  are on the Modal Volume (`setup` on 12 Nov 2026 at 10:56Z; `crown-sessiona-sepolia-1` ran session A from that
  bundle); `setup` again only if they were replaced since: it checks every program's sha256 and image id against the
  ids pinned in `cloud/ctl.py`; `crown-driver check --chain sepolia` says until when the checkpoint is valid (28 Nov
  2026 03:13 UTC for this one; after it, a fresh fixture: `scripts/fetch-sepolia-checkpoint.py`, then
  `lez/crown/build.sh repack` and `setup`);
- the Secret `ferrata-sepolia-key` on our Modal workspace (it exists; the run signs with the same throwaway deployer, then at
  nonce 23 after crown-live-1's 13 transactions, now at 36 after crown-live-3's: the gate prints the nonce, the vault
  address and the balance; on a resume, the vault the identity fixed at the first start);
- no ferrata-sepolia runner active (`e2e/sepolia-cloud.sh status`): both runs use the deployer's nonces, and `start`
  refuses while one is active.

From the tree's root (`e2e/sepolia-live.env`, if present, is read for SEPOLIA_RPC_URL and ETH_FROM; without it the
public endpoint and the known deployer are used, read-only, by `start`):

```
cd spike/ferrata-wt-integ7
export CROWN_MODAL_PROFILE=<your Modal profile>  # every command needs it (the run's Modal workspace)
e2e/crown.sh start crown-live-3                 # the live run: prints sender, nonce, vault address, the 13
                                                #   transactions, the gas price; type yes
e2e/crown.sh status crown-live-3                # any time: the stage, the stages done, the last lines of the log
e2e/crown.sh fetch crown-live-3                 # e2e/runs/crown-live-3/ (identity, transactions, captures, the LEZ
                                                #   sessions' records, checkpoint, releases, verify.txt)
e2e/crown.sh verify crown-live-3                # the chain checks again from this machine (read-only)
e2e/crown.sh stop                               # cancels the runner (and a queued GPU proof); `start` resumes
```

The inputs, if they ever need uploading again (from the same root; nothing is sent to any chain):
`e2e/crown.sh setup --bundle ../tmp/crown-fresh/lez/bundle.tar.zst --programs ../tmp/final-build/repro-a/r0bf`
(`scripts/reproduce-image-ids.sh build --out DIR` makes `DIR/r0bf` anywhere; the bundle is `spike/tmp/integ5`'s
with the final `ferrata_lez.bin`, `ferrata_bridge.bin` (`spike/tmp/final-build/crown/bundle.tar.zst`), repacked with
this tree's `crown-driver`, `session.sh` and checkpoint fixture: `CROWN_LEZ_TMP=DIR lez/crown/build.sh repack FROM`,
with the driver built by `lez/crown/build.sh driver` in `DIR/bin`).

A stop, a timeout or a crash anywhere is resumed by `start` with the same run name: every stage asks the chain or the
Volume first (deploy.sh resumes from its record; a deposit, submit, finalize or release already on chain is not sent
again; a LEZ session whose generation is saved is not run again, and once the vault's deployment has begun no new
devnet can be made). A stop during a LEZ session loses that session's work: it runs again from the last saved
generation (session B: ~15 min). `--halt-after <stage>` stops cleanly after a stage.

Expected duration of the live run (derived: the rehearsals' measured stage times plus Sepolia's own clock): build ~2
min; session A ~6 min (`crown-sessiona-sepolia-1`: 335 s, 23 transactions, from the period-1411 checkpoint; it was ~30 min from the period-1378 one: crown-lez's rehearsal 2 on the real Sepolia path: 176 transactions, 29.5 min); deployment ~3 min
(7 transactions, `--slow`); each deposit then waits for its block's finality (~13-20 min) and a light-client update that
covers it (up to ~7 min more): ~40 min for both; session B ~15 min (857 s in crown-rehearsal-3: the private claim
proven and sent in 276.8 s, the private burn in 336.0 s); the checkpoint ~2 min (crown-rehearsal-3: 18.5M user cycles,
33.3 s on an L40S, 41 s with the queue; GPU queue permitting: up to 2 h, then the CPU here); W = 60 min (the release
proofs, ~3 min, run in it); finalize, the 120 s release delay and both releases ~4 min; verify ~1 min. About **2 h 40
min to 3 h 10 min** in all, longer if no GPU is free. Modal: about 1-2 USD on our Modal workspace (estimated; the app `ferrata-crown`
had been billed 1.08 USD in all, a selftest and three rehearsals, by 06:52 CET; `mcost Modal; 3.38 USD by
16:41 CET, so about 2.3 USD for the live run, derived). Sepolia: 13 transactions, ~50 M gas (the vault ~39.5 M under
EIP-8037; the secured vault's `submitCandidate` costs ~24.7 k more), 0.002004 ETH escrowed by the deposits and paid
back by the releases less the fees. With the secured vault the checkpoint fee goes to the sender of
`submitCandidate` (the deployer here), and anyone may retire the vault 30 days plus one wait after the last
checkpoint progress (`retireOpensAt()`).

Measured in crown-live-1: 2 h 33 min of runner time over four starts (9 h 4 min of wall time with the owner's
restarts): session A 30.1 min (177 transactions); the deployment's seven transactions about 1.5 min of sending in
three parts; each deposit to its capture 19.7 and 19.1 min; session B 14.6 min (the private claim proven and sent in
270.7 s, the private burn in 317.5 s, on the session's CPUs); the checkpoint 1.5 min (21 segments, 32.1 s on an L40S,
no queue); W 58 min of runner time; finalize, the 120 s delay and the releases about 3 min; verify 3 s.

Measured in crown-live-3: about 2 h 9 min of runner time over two starts (2 h 20 min of wall time; the 11 min between
them were the retry fix and the owner's restart): session A 4.5 min (23 transactions, from the period-1411
checkpoint); the deployment's seven transactions in two parts (the stop and the library in start 1, the other five in
143 s); each deposit to its capture 14.7 and 25.2 min (the second waited for one more light-client update); session B
12.7 min (the private claim proven and sent in 231.9 s, the private burn in 282.8 s, on the session's CPUs); the
checkpoint 2.8 min (20 segments, 31.9 s on an L40S, 119 s with the queue); W 58 min of runner time; finalize, the
120 s delay and the releases about 3.3 min; verify 5 s.

Not exercised by the fork rehearsal (exercised elsewhere, or simple): the live captures with their finality waits
(`e2e/crown/capture.py`, tested read-only against Sepolia: block 12,108,569, 6 slots proven), signing with the
keystore in the cloud (as ferrata-sepolia's runner does), LEZ's real Sepolia instance (crown-lez's rehearsal), the
node's gas pricing on Sepolia (deploy.sh), a real hour of W. All of them ran in crown-live-1 and in crown-live-3.
