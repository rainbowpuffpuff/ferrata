# Stream ci-sequencer: RFP-021 S.2 and RFP-022 R.10 in CI against a standalone LEZ sequencer, proposed rows

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. Branch
`ci-sequencer` from `ebbee2aa`. **GitHub: run 45333224944 of `sequencer.yml` on `ci-sequencer` at `d5778a56`
(workflow_dispatch, 24 November 2026, 05:25–06:57 UTC), both jobs green**: `sequencer-attest` in 92.2 min,
`sequencer-roundtrip` in 72.7 min (`gh run view 45333224944 --json jobs`); the records are the run's artifacts
`ci-sequencer-attest-records` and `ci-sequencer-roundtrip-records`, the numbers below from their job summaries,
`records/summary.txt`, `run/steps.tsv` and `run/facts.json`. Box runs: the Modal build box, the same day, by the same
scripts (committed records under [`e2e/runs/ci-sequencer-2026-11-24/`](../../e2e/runs/ci-sequencer-2026-11-24/attest/README.md)).
Run 45331030912 (`c65933cc`) stopped first, in both jobs' first build step: the scripts required `r0vm` on PATH, where
a runner has none (fixed in `d5778a56`). After run 45333224944, `0123e717` changes configuration only (caches, the
proof's cap; section "The GitHub run against the box"). Nothing was sent to any public network: public Sepolia
endpoints were read (beacon and execution), as the jobs do.

## What changed

- **S.2 (`sequencer-attest`).** The job's only run (21 November) stopped at `twin Init`, "Execution outside of the
  validity window": it replayed mainnet data of 31 October, and the attestation program bounds every `Init` and update
  by the weak-subjectivity period of the instance's finalized header (3,532 epochs, about 15.7 days) by the
  sequencer's wall clock. The job now fetches its Ethereum data when it runs:
  [`scripts/fetch-sepolia-checkpoint.py`](../../scripts/fetch-sepolia-checkpoint.py) first (Lodestar's bootstrap of a
  recent finalized Sepolia checkpoint, the root publicnode's beacon API reports at its slot, the period updates), then,
  after the build, [`lez/scripts/fetch-attest-fixture.py`](../../lez/scripts/fetch-attest-fixture.py)` --network sepolia
  --same-address none` (the newest finality update; its block's RLP header and receipts and `eth_getProof` of Sepolia
  USDC's `totalSupply_` from a public Sepolia RPC), checked natively by `live-driver fresh-check` and fetched again
  until its finality update advances the store. New driver phases ([`lez/live/driver/src/fresh.rs`](../../lez/live/driver/src/fresh.rs)):
  `fresh-follow` (production), `fresh-scenarios` (the refusals on the same data), `fresh-consume` and `fresh-private`
  (consumption). The job moved to **LEZ v0.4.0-rc3** (the test network's tag, `lez/testnet04/`): the guests are built
  from the committed tree (the images of [`docs/image-ids.md`](../image-ids.md)), the sequencer and the driver against
  rc3 (`lez/scripts/lez-select.sh`, the manifests restored afterwards). A fetch that fails stops the job: nothing
  recorded is used in its place.
- **R.10 (`sequencer-roundtrip`, new).** [`scripts/ci/sequencer-roundtrip.sh`](../../scripts/ci/sequencer-roundtrip.sh)
  and its hook [`scripts/ci/sequencer-roundtrip-hook.sh`](../../scripts/ci/sequencer-roundtrip-hook.sh), run by
  `lez/live/run.sh`'s phase `hook` while LEZ v0.4.0-rc1's standalone sequencer runs: e2e/seqnet's operator and user
  tooling (crown-driver, the `ferrata` tool, `ferrata-relayer`) and the `roundtrip` job's checkpoint (`ci_zone`) and
  mock verifier, with anvil forking Sepolia. **rc1, not rc3, here:** the `ferrata` tool (`cli/`) pins LEZ v0.4.0-rc1's
  crates by path and is not in `lez-select.sh`'s manifests, and `ci_zone` writes rc1 blocks.
- **The workflow** ([`.github/workflows/sequencer.yml`](../../.github/workflows/sequencer.yml)): both jobs on pushes to
  `main`, daily at 05:43 UTC and by hand (a newer push to the same ref cancels the run in progress); every action pinned
  by commit (the SHAs of `ci.yml`; the TODO markers are gone); RISC Zero's toolchains, the upstream sources (rc1, rc2,
  rc3, Logos Blockchain), the vault's libraries, the native archives and the slow Rust builds cached; one step per
  stage, each with its own timeout; a job summary (`$GITHUB_STEP_SUMMARY`) naming the checkpoint and block used (S.2)
  or the fork point (R.10) and every phase's or step's outcome; the records uploaded whether the job passed or not.
- **Files of other streams, changed minimally** (each additive, the default behaviour kept):
  `lez/crown/driver/src/crown.rs` (the stand-in's bootstrap slot from `CROWN_STAND_IN_SLOT` if set: the fixed period-1409
  slot leaves the weak-subjectivity window on 26 November 2026), `lez/live/run-live.sh` (a `hook`-only run needs no
  live driver; `fresh-private` gets `attest-private`'s threads and priority), `cli/src/bin/seqnet_txs.rs`
  (`--txs-json`: the applied transactions in `ci_zone`'s format), `lez/scripts/fetch-attest-fixture.py`
  (`--same-address none`: no mainnet request), `lez/live/driver/src/main.rs` (the `fresh-*` commands),
  [`docs/guides/ci.md`](../guides/ci.md) (the two jobs), `scripts/ci/test_ci_model.py` (both jobs are push CI now; the
  checker's manual-job example is `lez-harness-full`), `scripts/ci/row-tests.toml` (S.2 and R.10).
- **No program, guest or image id changed.** The guests the jobs build and deploy are those of `docs/image-ids.md`
  (checked by the jobs: `ferrata_lez` `0x816d1fef…`, `scripted` `0x8cb0e3f5…`, `ferrata_consumer` `0x8464603b…`,
  `ferrata_bridge_b1` `0x847a4336…`, `checkpoint_b_31` `0xdafa6ab9…`, `release_31` `0xf279af41…`).

## Rows

| Row | Now | Proposed | Evidence |
|---|---|---|---|
| RFP-021 S.2 | in part (CI runs the state transition; the sequencer script stopped on aged recorded data; manual only) | **shown** (GitHub run 45333224944, job `sequencer-attest` green) | proposed EVIDENCE row A; [`e2e/runs/ci-sequencer-2026-11-24/attest/`](../../e2e/runs/ci-sequencer-2026-11-24/attest/README.md); `scripts/ci/sequencer-attest.sh` |
| RFP-022 R.10 | in part (CI runs the round trip over the state transition; sequencer round trips by hand) | **shown** (GitHub run 45333224944, job `sequencer-roundtrip` green) | proposed EVIDENCE row B; [`e2e/runs/ci-sequencer-2026-11-24/roundtrip/`](../../e2e/runs/ci-sequencer-2026-11-24/roundtrip/README.md); `scripts/ci/sequencer-roundtrip.sh` |

## RFP-021 S.2, clause by clause

"End-to-end integration tests exercise attestation production and consumption against a LEZ sequencer (standalone
mode) and an Ethereum test network or local fork, and are included in CI."

| Clause | Shown by | GitHub run 45333224944 (box record of the same day) |
|---|---|---|
| attestation **production** | phase `fresh-follow`: instance A of the deployed attestation program `Init`s from the fetched checkpoint (bootstrap and two adoption parts), applies the period update (the next committee) and the newest finality update (`Open`, `Miller`, `Finish`, every step on the sequencer); its store equals the native light client's after every job, its finalized root is the one the second beacon provider reports, and the record of the finalized block exists | checkpoint slot 11,640,832 (period 1421, root `0x49cd05dd…`, the second provider's; `Init` valid until 2026-12-09T01:37:36Z); finality update of slot 11,647,136 (05:47:12Z, root `0xd35f67ed…`, 486/512 signers), execution block 12,200,667; 11 applied (box: the same checkpoint, finality update of slot 11,646,336, block 12,199,909) |
| production's refusals | phase `fresh-scenarios` on the same data: a signature that is not a point (41), another update's signature (steps 1, 2 applied, 3 refused 40), five steps sent by the wrong actor (202), a job at another deployment (205), a second `Init` (203), an update for another chain's instance (40), an `Open` before its signature slot (LEZ's validity-window revert), duplicated and replayed steps (207, 81, 81, 205, 52), the wallet's default gas limit (out of gas); the finality update then applies | 41 sent: 23 applied, 16 refused with the designed codes, 2 designed reverts (box: the same) |
| attestation **consumption** | phase `fresh-consume`: `Attest` as the root of a transaction under A's record (a storage slot, a log, both at once), each applied, and refused without the block header (215), with a changed header (120), as a mainnet statement (140) and for a false value (130), each code equal to the native evaluation's; the reference consumer deployed through the loader, its `CallAttest` keeping the program's whole reply (equal to the native evaluation), its `Claim` of the block's ERC-20 `Transfer` crediting the sender the amount once, the same claim refused (223); phase `fresh-private`: the consumer's `AttestPrivately` at a private account, proven for real by LEZ's wallet and verified by the sequencer (`RISC0_DEV_MODE` unset) | 17 sent: 12 applied, 5 refused; the facts: USDC `totalSupply_` at block 12,200,667, the block's ERC-20 `Transfer` (tx 14, log 7); the private attestation applied, proven in 2,927.1 s with 4 threads (box: 1,222.1 s) |
| **a LEZ sequencer (standalone mode)** | LEZ v0.4.0-rc3's `sequencer_service` built from the unmodified tag with its `standalone` feature (its mock Bedrock actor; no Logos Blockchain node); every transaction built, signed and sent by LEZ's wallet over its RPC, read back with LEZ's own block application | 1,106 blocks; 90 transactions: 67 applied, 21 refused, 2 reverted, none dropped, rejected or unfinished (box: 519 blocks, the same counts) |
| **an Ethereum test network** | Sepolia, read when the job runs from public endpoints: Lodestar's beacon API (the bootstrap, the updates), publicnode's beacon API (the canonical roots), a public Sepolia RPC (the block, its receipts, `eth_getProof`, `eth_call`); `fresh/*/SOURCE.md` lists every request | each fetch answered at its first attempt; `fresh/checkpoint/`, `fresh/attest/` in the artifact (pinned by their `SHA256SUMS`) |
| **included in CI** | job `sequencer-attest` of `sequencer.yml`, on pushes to `main` (so `scripts/ci/check-row-tests.py` counts it as CI), daily and by hand | run 45333224944, job `sequencer-attest` green in 92.2 min: checkpoint 2 s, build 34.8 min, data 1 s, run 55.3 min, check and summary under 1 min |

Real and stand-in, exactly: **no stand-in** in this job; the private attestation's proof is real and verified by the
sequencer; the public attestations need no proof (LEZ executes public transactions itself).

## RFP-022 R.10, clause by clause

"End-to-end integration tests exercise the full deposit and redemption round trip against a LEZ sequencer
(standalone mode) and an Ethereum test network or local fork, and are included in CI."

| Clause | Shown by (each step a line of `run/steps.tsv`, the run stopping at the first that fails) |
|---|---|
| **deposit** | `ferrata deposit` (the user's tool) on the fork: 0.001 ETH to the user's public account `/0`; the vault holds 1 leaf |
| (import) | crown-driver `crown-b --operator-only`: the vault captured at the deposit's block, the committee finalizing that block, the bridge's `Init` (pairing), `Register ETH` and `ImportRoot n=1`, each reading the vault's storage through the attestation program on the sequencer: 3 of 3 applied |
| (claim) | `ferrata claim --plaintext`: held by its delay, the delay let pass on the fork's clock, sent; the user's public `/0` holds exactly 0.001 ETH; the same claim again refused (250 `ALREADY_CLAIMED`, the zone's history), the holding unchanged |
| **redemption** | `ferrata burn --from public` of the whole holding to a fresh address (the holding 0); a rule (B) checkpoint over every transaction the sequencer applied (34 in 28 blocks, `ci_zone`), the guest's journal equal to the native one, one burn; `submitCandidate`, the wait, `finalizeAttested` by `ferrata-relayer`; the vault's final state the journal's, its burn root accepted; `ferrata release --prove`: the release guest's journal equal to the native one; `ferrata-relayer` sends the release; the `Release` event pays out the burnt 0.001 ETH: 0.000997 to the fresh address, 0.000002 relay fee, 0.000001 credited checkpoint fee; the same release again refused (`NullifierAlreadyUsed`) |
| **a LEZ sequencer (standalone mode)** | LEZ v0.4.0-rc1's `sequencer_service` built from the unmodified tag with its `standalone` feature (no Logos Blockchain node); every LEZ transaction built, signed and sent by LEZ's wallet library over its RPC |
| **a local fork** | anvil forking Sepolia at Sepolia's finalized block when the job runs (read from a public Sepolia RPC; the fork's copy of that block has Sepolia's hash); the vault deployed on it by `vault/script/deploy.sh` (DeployVault), as on Sepolia |
| **included in CI** | job `sequencer-roundtrip` of `sequencer.yml`, on pushes to `main`, daily and by hand: run 45333224944, job green in 72.7 min (build 67.6 min, run 1.9 min); its 14 steps all ok: the fork (Sepolia block 12,200,821, 06:19:00Z), the programs and the stand-in instance, the vault, the deposit, the operator's import (3 of 3 applied), the fees, the plaintext claim, the claim again refused (250), the burn, the checkpoint journal (34 applied zone transactions in 28 blocks, 1 burn), the checkpoint accepted, the release journal, the release paid (997e12 + 2e12 relay fee + 1e12 checkpoint fee = 1e15 wei), the release again refused (`NullifierAlreadyUsed`) |

Stand-ins, exactly (the `roundtrip` job of `ci.yml` has the same three, and the fee one): **the vault's verifier**:
DeployVault deploys RISC Zero's Groth16 verifier 3.1.0, and the fork then replaces its code with the vault tests'
`MockRiscZeroVerifier` (`anvil_setCode`, as `vault/script/fork-rehearsal-31.sh` does), which accepts exactly the
(image id, journal digest) pairs it is told to: the journals the rule (B) checkpoint guest and the release guest
computed, each executed (RISC Zero executor; the release in dev mode) and equal to the native journal; no Groth16 proof
is made. **Ethereum consensus**: anvil has no beacon chain, so a 512-key committee the run holds finalizes the fork's
block for LEZ's light client (crown-driver's stand-in; its slots are recent, so `Init` is inside the window). **The
checkpoint's chain**: rule (B) needs blocks inscribed on Logos Blockchain, which a standalone sequencer does not make;
`ci_zone` puts the sequencer's applied transactions into LEZ v0.4.0-rc1 blocks inscribed by four test sequencer keys and
attested by three test attestation keys. **The gas drop**: `seqnet-fund` pays the user's LEZ fees from a funded
genesis account of LEZ's local network. Real: the sequencer and its state transition, the attestation and bridge
programs, the vault and its library and emergency stop, the user's tool, the relayer, the guests' executions, Sepolia's
state under the fork.

## Proposed EVIDENCE rows

Numbers to be assigned by the integrator (A and B below); the Details column to be linked as `docs/EVIDENCE.md` links (relative to `docs/`).

> | A | **Attestation production and consumption against a standalone LEZ sequencer on fresh Sepolia data, in CI
> (RFP-021 S.2)**: `scripts/ci/sequencer-attest.sh` (job `sequencer-attest`, `sequencer.yml`: pushes to `main`, daily,
> by hand) fetches a recent finalized Sepolia checkpoint and, after building LEZ v0.4.0-rc3's standalone sequencer and
> the live driver from source, the newest finality update with the facts under its block, from public endpoints; on
> the sequencer: the programs deployed through its loader (the images of `docs/image-ids.md`), instance A follows
> Sepolia (its root the second provider's), the twin's designed refusals, `Attest` at the record and its refusals, the
> reference consumer's `CallAttest`, `Claim` (credited once, the replay refused) and `AttestPrivately` with a real proof
> the sequencer verifies | GitHub run 45333224944 (job green in 92.2 min): checkpoint slot 11,640,832 (period 1421),
> finality update of slot 11,647,136 (block 12,200,667); 90 transactions: 67 applied, 21 refused with the designed
> codes, 2 designed reverts, none dropped; the private proof 2,927.1 s with 4 threads; build 34.8 min, run 55.3 min.
> Box run: the same checkpoint, finality update of slot 11,646,336, the same counts, the proof 1,222.1 s, build 17 min,
> run 26 min | GitHub Actions (`ubuntu-latest`, 4 vCPUs, AMD EPYC 7763, 16 GiB), 24 Nov 2026; Modal build box, the same
> day | the private attestation's real (the sequencer verifies);
> the rest none needed | `scripts/ci/sequencer-attest.sh RESULTS-DIR` | `e2e/runs/ci-sequencer-2026-11-24/attest/README.md` |

> | B | **The full deposit and redemption round trip through a standalone LEZ sequencer and a local fork of Sepolia, in
> CI (RFP-022 R.10)**: `scripts/ci/sequencer-roundtrip.sh` (job `sequencer-roundtrip`, `sequencer.yml`) builds LEZ
> v0.4.0-rc1's standalone sequencer, crown-driver, the `ferrata` tool, the relayer, `ci_zone` and the guests from source;
> while the sequencer runs, anvil forks Sepolia at its finalized block, DeployVault deploys the vault (rule B), and the
> user's deposit, the operator's import (Init, Register, ImportRoot), a plaintext claim and its refused replay, a public
> burn, the rule (B) checkpoint over every transaction the sequencer applied, the release and its refused replay run,
> each step asserted; stand-ins: the vault tests' mock verifier in place of RISC Zero's (accepting only the journals the
> guests computed, each equal to the native one), the light client's committee, `ci_zone`'s test committee, the gas
> drop | 14 steps ok; 34 applied zone transactions in the checkpoint, 1 burn;
> released 0.000997 ETH + 0.000002 relay fee + 0.000001 checkpoint fee = 0.001 ETH; the replay refused with
> `NullifierAlreadyUsed`. GitHub run 45333224944 (job green in 72.7 min): fork of block 12,200,821, the same 14 steps
> and amounts, build 67.6 min, run 1.9 min. Box run: fork of block 12,200,332, cold build 41 min, run 1 min 52 s |
> GitHub Actions (`ubuntu-latest`), 24 Nov 2026; Modal build box, the same day | none (dev mode; mock verifier) |
> `scripts/ci/sequencer-roundtrip.sh RESULTS-DIR` (under `e2e/runs/`) | `e2e/runs/ci-sequencer-2026-11-24/roundtrip/README.md` |

Amendment: EVIDENCE row 164's "Details" or "What it shows" gains "superseded in CI by row A (the job on fresh data,
LEZ v0.4.0-rc3, on pushes to `main`)".

## Proposed reasons (`docs/requirements-status.md`)

- **RFP-021 S.2, shown**: "CI runs attestation production and consumption against LEZ v0.4.0-rc3's standalone
  sequencer built from source, on Sepolia data fetched when the job runs (job `sequencer-attest`, on pushes to `main`,
  daily and by hand; green in GitHub run 45333224944): instance A follows Sepolia from a recent checkpoint, the twin's refusals
  on that data, the attestation consumed publicly and, with a real proof the sequencer verifies, privately. The round
  trip over LEZ's state transition stays in `roundtrip`." Evidence: A, 76, 164; `scripts/ci/sequencer-attest.sh`;
  `e2e/runs/ci-sequencer-2026-11-24/attest/`.
- **RFP-022 R.10, shown**: "CI runs the full deposit and redemption round trip through LEZ v0.4.0-rc1's standalone
  sequencer built from source and anvil forking Sepolia (job `sequencer-roundtrip`, on pushes to `main`, daily and by
  hand; green in GitHub run 45333224944), every step asserted; the stand-ins are the `roundtrip` job's (the mock verifier
  accepting the guests' journals, the light client's committee, the checkpoint's test committee, the gas drop). The
  round trips live on Sepolia (41, 91) and on zones (100–103, 125, 126) are by hand." Evidence: B, 76, 41, 91, 100–103.

## Proposed traceability text

- l.117 (S.2), "How" column: replace the sentence "Sequencer-based CI: a CI script builds LEZ v0.4.0-rc1's standalone
  sequencer ... older than the `Init` validity window" with: "Sequencer-based CI: job `sequencer-attest` builds LEZ
  v0.4.0-rc3's standalone sequencer and the live driver from source and runs, on Sepolia data fetched when the job
  runs, production (instance A from a recent checkpoint and the newest finality update), the refusals, and consumption
  (`Attest` at the record, the reference consumer's `CallAttest`, `Claim` and `AttestPrivately` with a real proof);
  on pushes to `main`, daily and by hand (EVIDENCE row A)." Test column: add `lez/live/driver/src/fresh.rs`.
- l.203 (R.10), "How" column: before "Through running sequencers, by hand, not in CI", add: "In CI through a
  standalone sequencer: job `sequencer-roundtrip` runs the round trip through LEZ v0.4.0-rc1's standalone sequencer and
  anvil forking Sepolia at its finalized block, every step asserted, with the `roundtrip` job's stand-ins (EVIDENCE row
  B)." Test column: add `scripts/ci/sequencer-roundtrip.sh`.

## The GitHub run against the box

| | GitHub run 45333224944 | Modal build box |
|---|---|---|
| machine | `ubuntu-latest`: 4 vCPUs (AMD EPYC 7763), 16 GiB | 16 cores, 64 GiB; the scripts used 4 build jobs and 4 prover threads |
| the private proof (4 threads) | 2,927.1 s | 1,222.1 s |
| `sequencer-attest` | build 34.8 min (guests 9.5, rc3 sequencer 15.3, driver 10.0), run 55.3 min (the proof's phase 50.7 min), job 92.2 min | build 17 min, run 26 min |
| `sequencer-roundtrip` | build 67.6 min (vault 0.8, `bridge/` 2.6, rule (B) guest 4.0, `ci_zone` 12.5, LEZ guests 9.5, rc1 sequencer 15.0, crown-driver 9.9, `cli/` 13.4), run 1.9 min, job 72.7 min | cold build 41 min, run 1 min 52 s |
| chain | 1,106 blocks (the proof's 50 minutes at 3 s a block) | 519 blocks |
| data | the same checkpoint (Lodestar again served the period's first one), a later finality update; every fetch answered at its first attempt | |
| caches | RISC Zero's toolchains (1.95 GB), the vault's libraries and the native archives restored; the upstream sources not (actions/cache refuses to save a path with `..`, so they are cloned, 5 s); rust-cache restored the key that run 45331030912 had saved before building anything and, finding it "up-to-date", stored nothing: no build was cached | none |

Fixed after the run, configuration only, in `0123e717` (run 45333224944 tested `d5778a56`): new rust-cache keys saved
by passing runs only (the next green run stores the targets; the following runs restore them), the upstream sources'
cache path without `..` (`runner.workspace`), and the per-phase cap of the attestation run raised from 3,600 to
5,400 s (the proof used 81% of the old cap on the runner). `ci.yml` caches the upstream sources at the same `..` path
and so never saves them either; not changed here (not this stream's file).

## Merging with main

`git merge-tree --write-tree origin/main ci-sequencer` (main at `98375f82`, base `ebbee2aa`): no conflict. Both sides
changed `docs/guides/ci.md`, `scripts/ci/row-tests.toml` and `scripts/ci/test_ci_model.py`. On a trial merge of
`97394076` with `98375f82` (a detached worktree, removed after) the `links` job's checks pass: `test_ci_model.py` 113
passed, `check-row-tests.py` consistent, `check-links.py` 0 broken of 2,529, `check-guides.py` 0 findings,
`check-actions-pinned.py` 0 unpinned, `test-sequencer-records.sh` 17 passed; re-run them on the real merge.

## Risks

- **Public endpoints.** Lodestar is the only public Sepolia beacon API found serving light-client data; when it or the
  Sepolia RPC does not answer, the step fails (by design). The daily run will show how often.
- **The proof on a runner** took 2,927 s against the box's 1,222 s; a slower runner gets the 5,400 s cap.
- **GitHub's cache budget** (10 GB per repository): the two jobs cache their slowest builds (rc3: 3.0 GB, rc1 and the
  tools: 4.0 GB, `checkpoint/` 3.3 GB on the box, uncompressed); with `ci.yml`'s caches GitHub evicts the least recently
  used.
- **The round trip's claim refusal** is the tool's (it reads the zone's history before sending: 250); the program's own
  refusal of a second claim is shown in the harness (row 76) and in the crown runs.
