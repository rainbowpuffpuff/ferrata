# The lab: Ferrata's light client following Sepolia and mainnet on a LEZ zone, unattended

RFP-021's first milestone asks for an instance that follows Sepolia for seven consecutive days
with no manual repair. This directory keeps such a thing running: LEZ v0.4.0-rc1's own
sequencer chain, with its Logos Blockchain (Bedrock) node, kept alive on Modal, and two
instances of the light-client program on it, one following Sepolia (through its Gloas fork) and
one following mainnet, each fed the newest light-client updates over the program's public job
path. Every transaction and every block is recorded, and the record replays offline.

`../live/` ran the program once on the same zone, for an afternoon; this is the same zone made
to persist and to be driven on a schedule. Nothing of LEZ or of the node is modified.

## What runs where, and how often

| | |
|---|---|
| where | Modal, the lab's own workspace (`LAB_PROFILE`): app `ferrata-lab`, one function `session`, a Volume `ferrata-lab` |
| how often | every two hours, at minute 17 (cron `17 */2 * * *`, UTC); at most one session at a time |
| a session | restores the newest saved state of the zone from the Volume, starts the node and the sequencer, fetches the newest light-client data, submits it to both instances, waits until every transaction is in a block final on Bedrock, stops both processes, saves a new state generation, writes the record (`session.sh`) |
| the zone | LEZ's `sequencer_service` at tag `v0.4.0-rc1` (`1806e87`), unmodified, with LEZ's `local.json` (sockets on loopback, 10 s blocks: as `../live/`); a Logos Blockchain node at `d480efc6` (the revision LEZ pins, built by LEZ's resolver) with LEZ's `node-config.yaml` and `deployment-settings.yaml`, `POL_PROOF_DEV_MODE=true` as LEZ's container sets it: **one validator, leader proofs in dev mode** |
| the program | the guest image built from this branch, `61b7b70dee5a5020af54b3107cef5298861fadd301c9dc0b063c80d88ca42f9a` (602,888 bytes), deployed once through LEZ's `program_loader` (7 transactions), immutable. It is the program of `../RESULTS.md` §6 (the store's upgrade in place, Gloas records) built in another directory, so its image id is not `9f3eb00f…` (`../RESULTS.md` §3: a guest embeds its build paths) |
| the driver | `driver/`: the live driver's chain modules, unchanged (LEZ's wallet sends every transaction; the follower applies only blocks `Finalized` on Bedrock and reads each charge with LEZ's own `chain_state` settlement), and the job path of `../harness/src/lib.rs` (`Instance::update_in`): `Open`, `Miller`, the two adoption parts when an update hands over a committee, `Finish`; `Init` and two adoption parts once per instance. Every job step declares a gas limit of 4,800,000 (the largest step measured 3,731,712) |
| keys | LEZ's published test accounts pay (genesis of `local.json`); the program's deployment keys are derived from public labels. Throwaway by design; nothing is signed for Ethereum |
| reads | Lodestar's public beacon APIs (`lodestar-sepolia.chainsafe.io`, `lodestar-mainnet.chainsafe.io`: the only public ones found serving light-client data) for bootstraps and updates; publicnode's beacon APIs as the second provider. User-Agent `ferrata-lab-fetch`, no key, nothing else; every request is listed in the session's `fetch-<chain>.json` |

## The two instances

| | Sepolia | mainnet |
|---|---|---|
| configuration | `harness::sepolia::sepolia_config(2)`: Electra store layout at the start, the schedule with Gloas at epoch 353,024 | `harness::mainnet_config(2)`: Electra (Fulu) layout, no Gloas in the schedule |
| bootstrap and checkpoint | the period-1378 bootstrap of `crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10` (pinned by its `SHA256SUMS`), block root `5f71d7f6…b20d`: the earliest the program can follow from, since no public node serves a Gloas-era bootstrap (the fixture's `SOURCE.md`) | the finalized checkpoint Lodestar reported at the first session, `4baa732e…abff` (slot 15,625,920, period 1,907), which publicnode reported canonical before the bootstrap was fetched |
| weak subjectivity | the checkpoint is the instance's trusted root, fixed at `Init` (part of its id); from it on, the program accepts only what its sync committees sign | the same |
| committee handoffs | as the program does them: an update whose signature falls in the store's next period rotates the committees, and its next committee is adopted in two `Adopt` transactions before `Finish` | the same |
| catch-up | the first session replayed every period's update from 1378 to 1409 (the store upgrading itself to the Gloas layout at 1379), then the newest finality update | `Init`, the current period's update (adopts the next committee), the newest finality update |

The driver keeps the bootstrap and the served bytes of every update it applied, rebuilds the
native light client from them at the start of every session, and drives the chain only if the
instance on the sequencer holds exactly that store; after every job it checks the instance's
store and its header record against the native client again. A session that cannot reach the
endpoints, or whose job is refused, changes nothing it cannot redo: the next session fetches
again and continues from what the chain holds.

## Survival

- **Restarts.** The zone survives being stopped: the node's genesis is fixed (slot 0 is unix
  second 1,785,493,027), a fresh node already starts millions of slots after it, and a gap of
  hours between sessions is more of the same. The first scheduled session restarted the zone
  after 54 minutes stopped; the sequencer resumed at its block 185 and its scheduler came up 37 s
  after its start. Hence the scheduled design, not a long-running function.
- **Bedrock's epoch seconds.** As `../live/` found, the sequencer must not make its blocks in the
  last seconds before an epoch. Each session reads the node's slot clock, starts the sequencer,
  reads from its log when its block scheduler started, and starts it again (up to four times,
  timed by the delay it saw) until its blocks fall 1 to 6 s after an epoch second.
- **A failed or missed session.** A session saves a new generation only when the sequencer and
  the node both stopped on SIGTERM within 60 s; otherwise the next session starts from the last
  good generation (the node's state, the sequencer's home and the driver's follower, wallet and
  kept updates travel together in one generation, so they never disagree). Three generations
  are kept. A session that does not run at all leaves the chain where it was; the next one
  catches up (every period's update since the instance's finalized period, then the newest
  finality update). Every session, failed or not, is a line of `results/sessions.tsv` and a
  directory of logs.

## Operate

From the repository, on a machine with the Modal client and `LAB_PROFILE` set to the Modal profile of the lab's workspace
(a local `verify`, with `LAB_VERIFY_BIN`, needs neither):

```sh
lez/lab/lab.sh deploy     # upload the bundle if it changed, deploy the app: the schedule starts
lez/lab/lab.sh run 60     # one session now (60: minutes it may submit jobs; default 20)
lez/lab/lab.sh status     # both instances against the chains' clocks, the sessions, failures
lez/lab/lab.sh fetch      # the record and the sessions' logs into results/
lez/lab/lab.sh verify     # offline replay of results/ (on the mrun build box)
lez/lab/lab.sh stop       # stop the app: no schedule, nothing running or billing
```

The bundle (`build.sh all`, then `build.sh pack`, both on the mrun build box) holds the
sequencer, the node, the lab driver, r0vm 3.0.5 (LEZ executes programs through it, from genesis
on), the node's circuit files and the program image, with their SHA-256 (`bundle-SHA256SUMS` in
the record). The Volume keeps the state after `stop`; `modal volume delete ferrata-lab` removes it.

## The record (`results/`)

| file | what |
|---|---|
| `record/run.tsv` | the manifest: one row per transaction (step, hash, block, position, status, exit code, execution and storage gas charged, gas limit, base fees, fee, the payer's debit, the block's totals) |
| `record/blocks.borsh`, `record/segments.tsv`, `record/chain.tsv` | every block as the sequencer served it, with the program bytecode removed (6 chunks, listed with their SHA-256); one line per block |
| `record/jobs.tsv` | one row per job: chain, kind (`init`, `period`, `finality`), period, attested, signature and finalized slots, layout, whether it rotated the committees, advanced the finalized header, adopted a committee; the job id, its transactions and blocks |
| `record/net/<chain>/` | the instance's bootstrap, checkpoint and the served bytes of every update it applied, in order |
| `record/instances.tsv`, `programs.tsv`, `summary.txt` | the instances and the program as the chain holds them at the end |
| `sessions.tsv`, `sessions/<id>/` | every session: times, status, the generation saved, the sequencer's starts and block phase; its logs (session, driver, fetch with every request, sequencer, an excerpt of the node's, the node's state at start and end) |

## Re-check

- `lab.sh verify` (the driver's `verify`, needs only `results/` and the program image): the
  blocks link and hash, with the bytecode put back; they replay through LEZ's
  `chain_state::apply_block` from the pre-genesis state; every manifest row equals what LEZ's
  settlement charges; and for each chain, the instance as the replay leaves it equals the native
  light client rebuilt from the kept bootstrap and updates, header record included.
- Against Ethereum: each session's `fetch-<chain>.json` holds the block root the second
  provider reports for the newest finalized slot; it must equal the instance's
  `finalized_root` in `record/instances.tsv` when that session ended there. The image: build
  `lez/methods` at this commit in a directory of the same path to get the same bytes.

## Results so far (measured)

| | |
|---|---|
| sessions | 10 Nov 18:49 UTC: failed to start (r0vm was not in the bundle; fixed, kept in the record). 18:51–19:22 UTC: the catch-up, 1,868 s, 184 transactions, all applied, blocks 1–185. 20:17 UTC (scheduled, after 54 min stopped): 123 s, 6 transactions, all applied, blocks 186–191. 22:17 UTC (scheduled): 133 s, 11 transactions, all applied, blocks 192–202, Sepolia's first live committee handoff (its period-1410 update). 11 Nov, 00:17 to 14:17 UTC, eight scheduled sessions: 83–134 s each, 6 transactions each but 11 at 10:17 (mainnet's live committee handoff into period 1,908) and 9 at 12:17, all applied, blocks 203–258 |
| transactions | 257, all applied, none refused, reverted, dropped or rejected; the program's transactions used 208,951 (`Init`) to 3,733,713 (`Finish` of mainnet's period-1908 update) execution gas, none over the 5M target; fees 6,288,435,520 in all (the payers held 1e13 and 2e13 at genesis) |
| re-check | `lab.sh verify` after each session, on the build box (exit 0 each time); after the third: 202 blocks replay, 201 manifest rows equal LEZ's charges; Sepolia at finalized slot 11,551,200 (period 1,410, Gloas layout), root `80088b24…9a30`, and mainnet at 15,627,008 (period 1,907), root `4b909f85…7bdb`, both equal to the native client; at every session both roots equal the second provider's. After the session of 11 Nov 14:17 UTC (the eight sessions of 11 Nov fetched together at 15:48 UTC; `lab.sh verify` once, on the build box of our Modal workspace): 258 blocks replay, 257 manifest rows equal LEZ's charges; Sepolia at finalized slot 11,556,000 (period 1,410), root `d31b7d34…f3cc`, and mainnet at 15,631,808 (period 1,908), root `530599ec…2328`, both equal to the native client; each session's applied finality update has the finalized header root that session's second provider reported (22 of 22, the root recomputed from the kept update) |
| 14 Nov fetch | the record to the session of 14 Nov 14:17 UTC (fetched the same afternoon): 48 sessions, 47 ok; the 46 scheduled since 10 Nov 20:17 UTC all ran (83–140 s each; 7 of them started the sequencer twice, the script's own phase alignment, `session.log`), none failed; 498 transactions, all applied, blocks 1–500; Sepolia's live handoffs into periods 1,411 (12 Nov 00:17), 1,412 (13 Nov 04:17), 1,413 (14 Nov 08:17), mainnet's into 1,909 (12 Nov 14:17) and 1,910 (13 Nov 16:17). `lab.sh verify` (the driver of 66f6bbbd, on the build box): 500 blocks replay, 498 manifest rows equal LEZ's charges; Sepolia at finalized slot 11,577,600 (period 1,413, Gloas layout), root `27375fd2…31b2`, mainnet at 15,653,408 (period 1,910), root `5fc50898…38db`, both equal to the native client after 83 and 52 kept updates; every session's applied finality update has the finalized header root that session's second provider reported (`python3 -I lez/lab/roots.py lez/lab/results`: 94 of 94, both chains) |
| 15 Nov fetch | the record to the session of 15 Nov 20:17 UTC (fetched at 21:20 UTC): 63 sessions, 62 ok; the 61 scheduled since 10 Nov 20:17 UTC all ran (83–141 s each; 17 of them started the sequencer twice, each logged as the script's own phase alignment, "sequencer start 2: scheduler after …" in `session.log`, nothing else), none failed; 598 transactions, all applied, blocks 1–600; since the 14 Nov fetch Sepolia's live handoff into period 1,414 (15 Nov 10:17) and mainnet's into 1,911 (14 Nov 20:17). `lab.sh verify` (the driver of 66f6bbbd, on the build box): 600 blocks replay, 598 manifest rows equal LEZ's charges; Sepolia at finalized slot 11,586,592 (period 1,414, Gloas layout), root `3d6c817b…f639`, mainnet at 15,662,400 (period 1,911), root `4df808cf…d143`, both equal to the native client after 99 and 68 kept updates; every session's applied finality update has the finalized header root that session's second provider reported (`python3 -I lez/lab/roots.py lez/lab/results`: 124 of 124, both chains) |
| 17 Nov fetch | the record to the session of 17 Nov 20:17 UTC (fetched at 20:22 UTC): 87 sessions, 86 ok; the 85 scheduled from 10 Nov 20:17 UTC to 17 Nov 20:17 UTC, seven days apart, all ran at their two-hour marks (83–142 s each; 30 of them started the sequencer twice, the script's own phase alignment, `session.log`), none missed, none failed; 765 transactions, all applied, blocks 1–768; since the 15 Nov fetch Sepolia's live handoffs into periods 1,415 (16 Nov 14:17) and 1,416 (17 Nov 18:17), mainnet's into 1,912 (16 Nov 00:17) and 1,913 (17 Nov 02:17). `lab.sh verify` (the driver of 66f6bbbd, on the build box): 768 blocks replay, 765 manifest rows equal LEZ's charges; Sepolia at finalized slot 11,600,992 (period 1,416, Gloas layout), root `a062b3c8…47fc`, mainnet at 15,676,800 (period 1,913), root `4429b5f5…d9e7`, both equal to the native client after 125 and 95 kept updates; every session's applied finality update has the finalized header root that session's second provider reported (`python3 -I lez/lab/roots.py lez/lab/results`: 172 of 172, both chains) |
| 18 Nov fetch | the record to the session of 18 Nov 20:17 UTC (fetched at 20:34 UTC): 99 sessions, 98 ok; the 97 scheduled from 10 Nov 20:17 UTC to 18 Nov 20:17 UTC, eight days apart, all ran at their two-hour marks (83–143 s each; 35 of them started the sequencer twice, the script's own phase alignment, `session.log`), none missed, none failed; 847 transactions, all applied, blocks 1–850; since the 17 Nov fetch Sepolia's live handoff into period 1,417 (18 Nov 20:17) and mainnet's into 1,914 (18 Nov 06:17). `lab.sh verify` (the driver of 66f6bbbd, on the build box): 850 blocks replay, 847 manifest rows equal LEZ's charges; Sepolia at finalized slot 11,608,192 (period 1,417, Gloas layout), root `23362d57…5466`, mainnet at 15,684,000 (period 1,914), root `c8e94b89…a8fc`, both equal to the native client after 138 and 108 kept updates; every session's applied finality update has the finalized header root that session's second provider reported (`python3 -I lez/lab/roots.py lez/lab/results`: 196 of 196, both chains) |
| state | generation 2: 20,402,011 bytes compressed (node 52M, sequencer 48M, driver 5.6M on disk); generation 3: 30,970,636 bytes. It grows with the node's and the sequencer's write-ahead logs; `sessions/<id>/session.log` lists the largest files at every save |
| cost | billed by Modal (`modal.billing`, hourly buckets, the lab's workspace): the build box 0.1967 USD in the hour of the builds (18:00 UTC), then 0.0802 and 0.0522 USD (packing, the first two `lab.sh verify` runs, and the 20 minutes the box stays warm after each call); the lab's failed start and catch-up session together 0.0642 USD (31 min, two physical cores requested); the scheduled session of 20:17 UTC 0.0064 USD (the whole 20:00 hour of the app). At twelve such sessions a day: about 0.08 USD a day, 0.54 USD to 17 November (derived) |

## The claim that can be made today

As of the session of 18 November 2026, 20:17 UTC (`results/`, re-checked by `lab.sh verify`):

> Two instances of Ferrata's light-client program have followed their chains on LEZ v0.4.0-rc1's
> own sequencer (one Bedrock validator, leader proofs in dev mode) since 10 November 2026,
> 19:22 UTC, with no manual repair: eight consecutive days. **Sepolia:** initialized from the
> period-1378 bootstrap, through the Gloas fork, 138 updates applied (40 period updates, 98
> finality updates), 39 committee handoffs (eight of them live, into periods 1,410 to 1,417);
> finalized slot 11,608,192. **Mainnet:** initialized at the finalized checkpoint of slot
> 15,625,920, 108 updates applied (10 period updates, 98 finality updates), 7 committee handoffs
> (all live, into periods 1,908 to 1,914); finalized slot 15,684,000. 99 sessions, of which 1
> failed to start before the first good one; 98 good, 97 of them scheduled (10 November 20:17
> UTC to 18 November 20:17 UTC, one every two hours), none missed. 847 transactions, all
> applied; 850 blocks, all final on Bedrock; the whole chain replays offline to the recorded
> charges and to both instances' stores, and both finalized roots are the ones a second beacon
> provider reports.

Eight consecutive days from 19:22 UTC on 10 November ended at 19:22 UTC on 18 November; the
record above reaches the scheduled session after it. The lab keeps running; `lab.sh status` and
`lab.sh fetch` give the numbers of the moment.

## What this is not

- One validator with leader proofs in dev mode, and one sequencer, on one container: a local
  zone, as in `../live/`, not a network. There is no public LEZ v0.4 network to join.
- The program image is a rebuild of `../RESULTS.md` §6's at another path (`61b7b70d…`, not
  `9f3eb00f…`); the same source, not the same bytes.
- Between sessions nothing runs: the instances lag their chains by up to the schedule's two
  hours plus finality; a light client tolerates that by construction (an update is accepted
  as long as the store knows the signing committee), and every session catches up.
- No proofs: the public job path only. The proven path and the bridge are out of the lab.
