# ferrata-updater on a running LEZ sequencer, following Sepolia live (12–13 November 2026)

The updater daemon with its sequencer backend (`../../sequencer/`, `daemon.ledger = "sequencer"`) kept the crown
run's Sepolia instance following Sepolia on a running LEZ v0.4.0-rc1 sequencer and Logos Blockchain (Bedrock) node,
from Lodestar's public light-client data with publicnode's public beacon API as the second source, for
eight hours over two zones, 74 updates applied end to end, through Sepolia's sync-committee period boundary (1411 → 1412: the committees rotated and period 1413's was adopted, on the live chain). Nothing was sent to Sepolia or
any public network: the beacon APIs were only read, every transaction went to the zone's own sequencer.

| part | what ran | mode |
|---|---|---|
| zone | LEZ v0.4.0-rc1 `sequencer_service` (unmodified tag, `1806e87`) and a Logos Blockchain node (`d480efc6`), LEZ's local configuration with 10 s blocks, as the crown run's LEZ sessions run them: `lez/crown/session.sh` from the crown bundle `0013bab1…` (the one of `e2e/runs/crown-sessiona-sepolia-1` and `crown-live-3`), on Modal (our Modal workspace, app `ferrata-crown`, function `updater_session`: 2 cores requested, up to 8; 8 GiB) | the sequencer executes and verifies every transaction (`RISC0_DEV_MODE` unset); Bedrock leader proofs in dev mode, one validator (LEZ's local setup) |
| programs | deployed by session A through LEZ's `program_loader`: the attestation program `816d1fef…` and the bridge `ccde6171…`, the image ids the sequencer's loader computed ([`../../../../docs/image-ids.md`](../../../../docs/image-ids.md)) | final programs |
| instance | `crown-driver crown-a`'s Sepolia instance `0xa5fb2591…b2b5`: `Init` from [`crates/ferrata-spectests/fixtures/sepolia-2026-11-12`](../../../../crates/ferrata-spectests/fixtures/sepolia-2026-11-12/SOURCE.md) (checkpoint `0x3b901f5e…ccf7c`, slot 11,562,112, period 1411) and that period's update (the next committee) | session A, as `crown-sessiona-sepolia-1` |
| daemon | `ferrata-updater`, the build of `lez/updater/sequencer/` at commit `6152bfe` (binary SHA-256 `912208fc8fd7637207721ec66789fe1fbeea51a840d4c9b25cac0bf52942525a`, built on the Modal build box), with `zone-*/updater.toml`: one daemon, `layout = "gloas"` (so it computes the crown's instance id), `min_agreeing = 2`, `poll_secs = 24`, `defer_window = 1`, payer LEZ's published test key of `local.json`'s genesis | the same daemon as the harness build, another `Ledger` |
| sources | Lodestar's public Sepolia beacon API `https://lodestar-sepolia.chainsafe.io` (light-client data, SSZ: finality updates, period updates, bootstrap); publicnode's `https://ethereum-sepolia-beacon-api.publicnode.com` as `kind = "headers"` (it serves no light-client data: 404): each update's finalized header must be the block it holds canonical at that slot | read only, User-Agent `ferrata-updater`, no key |

## What happened

Two zones, each a run of `e2e/crown.sh updater` on the crown app: session A (a new zone: the programs deployed, the
instance initialised), then sessions U of the daemon, each in a container of its own; between two sessions the
zone (the sequencer's and the node's state, and the daemon's state directory) is saved as a generation on the Volume
and restored (as the crown run's sessions A and B do).

**Zone 1** (`crown-updater-live-1`, `zone-1/`):

| session | UTC (12–13 Nov) | blocks | what |
|---|---|---|---|
| A | 19:15:47–19:21:18 | 1–24 | a new zone; both programs deployed; `Init` and the period-1411 update applied (23 transactions, all applied) |
| U1 | 19:21:55–20:47:46 | 25–537 | 14 updates applied (finalized slot 11,564,736 → 11,565,152); a restart of the daemon at 20:41:55 (below); stopped on request at 20:45:26, the zone saved as generation 2 (head block 537) |
| U2 | 20:49:32–about 22:43 | 538–1,200, lost | from generation 2: 18 updates applied (→ 11,565,728, block 1,200 at 22:40:09), a restart mid-job at 21:15:39; then Modal preempted the container. Its records were mirrored to the Volume every minute and are kept (`zone-1/u2-lost/`); its blocks were only in the container, so they are not on zone 1's final chain |
| U3 | 22:43:32–01:19:49 | 538–1,468 | Modal's re-run of the same call, from generation 2 again: the daemon's state was generation 2's (U1's end), it caught up in one job (11,565,152 → 11,565,728) and applied 25 updates in all (→ 11,566,496), a restart mid-job at 23:11:07; stopped on request at 01:15:20 (to save a generation: generation 3, head block 1,468) |
| U4 | 01:20:28–01:35:09 | none | from generation 3: the sequencer could not publish block 1,469, Bedrock's wallet out of funds (Findings); the daemon's first `Open` never settled, `/health` 503 (no round finished); the daemon killed (exit 137), nothing saved |

On zone 1's final chain (blocks 1–1,468, every one `Finalized` on Bedrock at the export) the daemon applied 39
updates (117 transactions, all applied); with U2's 18 on the lost blocks, 57.

**Zone 2** (`crown-updater-live-2`, `zone-2/`), a new zone once zone 1's Bedrock funds were spent:

| session | UTC (13 Nov) | blocks | what |
|---|---|---|---|
| A | 01:35:32–01:39:47 | 1–24 | as zone 1's: the programs (the same image ids), the same instance `0xa5fb2591…` |
| U1 | 01:40:31–03:32:12 | 25–691 | 17 updates applied: the first from the checkpoint (11,562,112) to 11,566,624 in one job; through the period boundary (below); stopped on request at 03:29:33, the zone saved |

On zone 2's chain (blocks 1–691, every one `Finalized` on Bedrock at the export) the daemon applied 17 updates (53
transactions, all applied).

**The period boundary.** Period 1412 began at slot 11,567,104 (03:00:48Z). By 03:10Z both providers reported slot
11,567,072 finalized, but Lodestar's light-client data could not move the instance yet: its finality update stayed
at finalized slot 11,567,008 (attested 11,567,072, the last epoch-start block of period 1411), and its best update of
period 1412 had no finality (finalized slot 0, 504 signers). The daemon judged both, sent nothing and stayed
`up-to-date`, `/health` ok. At 03:13:58Z Lodestar served a finality update attested at 11,567,168 and signed at
11,567,169 by period 1412's committee (480 of 512), finalizing slot 11,567,104, the first of period 1412 (root
`f665af39…a945`); publicnode held that block canonical. Its job (`Open` block 585, `Miller` 589, `Finish` 593,
03:15:27Z, 9,901,372 gas) **rotated the committees**. At 03:15:51Z Lodestar's period-1412 update had finality
(attested 11,567,172, 504 signers, the same finalized header) and carried period 1413's committee: the job with
adoption (`Open` 597, `Miller` 601, `Adopt` 1/2 605, `Adopt` 2/2 609, `Finish` 613, 03:18:47Z, 16,117,928 gas, five
transactions) **adopted the next committee**. The daemon then followed period 1412 under the new current
committee: 11,567,136 at 03:21:57Z, 11,567,168 at 03:28:27Z.

## Every update applied

`zone-1/summary.md`, `zone-1/u2-lost/summary.md` and `zone-2/summary.md` list every applied update: finalized slot,
root, attested and signature slots, signers, its effect (advances, rotates the committees, adopts the next one), the
blocks of its `Open` and `Finish`, the time, its gas and fee. The daemon's own line for each job
(`updater-u*.log`, "job 0x…: finalized slot … root … served or confirmed by lodestar, publicnode") names the two
sources that agreed on it.

| | updates applied | transactions | of them applied | gas | fee charged (= payer debit) | final instance (crown-driver's export) |
|---|---:|---:|---:|---:|---:|---|
| zone 1, final chain (U1, U3) | 39 | 117 | 117 | 356,226,897 | 2,849,815,176 | finalized slot 11,566,496, root `26aceb3c…`, 41 jobs (2 of session A) |
| zone 1, U2 (lost blocks) | 18 | 54 | 54 | | | (its journal also holds U1's 42) |
| zone 2 | 17 | 53 | 53 | 162,883,026 | 1,303,064,208 | finalized slot 11,567,168, root `8e84a344…`, 19 jobs |
| **in all** | **74** | **224** | **224** | | | |

No transaction was refused, reverted, dropped or rejected; no job was left open (each was finished, also across the
restarts mid-job). A finality update is one job of three transactions: 9.11M gas (`Open` 2,078,027, `Miller`
3,305,773, `Finish` 3,722,050 with storage gas; fee 72.85M at the base fee of 8, which never moved: one transaction
per block, under the 5M target); one that rotates the committees 9.90M (its `Open` checks the signature against the
next committee: 2,874,121); one that adopts a committee 16.12M in five. Each transaction was final on Bedrock and read
by the follower 6.9–16.4 s after the sequencer took it (median 15.7 s, zone 2); with `poll_secs = 24`, an update was
applied about 80 s after the daemon first saw it. Lodestar served a new finality update once an epoch (6.4 min); the
applied updates follow one another epoch by epoch, except where the instance caught up in one job (zone 1 after the
preemption: 11,565,152 → 11,565,728; zone 2's first job from the checkpoint) and at the boundary, where Lodestar
skipped slots 11,567,040 and 11,567,072 (above).

## Health and metrics

`probes-*/health.jsonl`: `/health` and the main `/metrics` counters read every minute (one JSON line per probe);
`probes-*/metrics-*.txt` and `health-*.json`: the full texts every tenth probe; `zone-*/sessions/*/metrics-last.txt`:
at each session's end. 483 probes in the five sessions: 464 `ok` (HTTP 200); the first of each session (5) before
the daemon listened; 14 in U4, HTTP 503 with no body: its first round never finished (its transaction could not
settle, Findings), and the daemon serves `/health` from the snapshot of its last finished round.

## Restarts from its state

| restart | what it shows |
|---|---|
| U1, 20:41:55: SIGTERM between jobs (the session's planned mid-job trigger compared `Open`, the journal writes `open`: fixed for U2, and its 20-minute fallback fired) | exit 0 two seconds later, after the round in hand; the new process followed the chain from its saved follower (block 505 in 0.4 s, against 12.8 s from LEZ's genesis at the first start) and applied the next update (11,565,152) at 20:45:01 |
| U2, 21:15:39: SIGTERM right after the daemon's `Open` of finalized slot 11,565,312 was applied (block 693) | exit 0; the new process found its open job in `state/sepolia/open/` ("job 0xeae47b7b… is open: its miller first") and sent `Miller` (block 696) and `Finish` (block 700): no second `Open` |
| U3, 23:11:07: the same, mid-job | the same: the kept job finished first |
| U1 → U2, 20:45–20:49: the zone saved and restored, all three processes restarted in a new container | the daemon's state came back with the zone; the next update applied at 20:51:38 |
| U2 → U3, about 22:43: the container preempted, the zone restored from the generation before U2 | the daemon's state was that generation's; the instance on the restored chain was at 11,565,152 and the daemon caught up in one job |
| U3 → U4, 01:15–01:21: a planned save and restore | the daemon resumed (block 1,468 in 3.9 s) and sent its next `Open`; the zone could not include it (Findings) |

## Re-check

From the repository root, with Python 3 only:

```sh
python3 -I lez/updater/results/live-sepolia/check.py lez/updater/results/live-sepolia/zone-1
python3 -I lez/updater/results/live-sepolia/check.py lez/updater/results/live-sepolia/zone-1/u2-lost
python3 -I lez/updater/results/live-sepolia/check.py lez/updater/results/live-sepolia/zone-2
# and every applied finalized root against a Sepolia beacon node of your choice (read only):
python3 -I lez/updater/results/live-sepolia/check.py lez/updater/results/live-sepolia/zone-2 --beacon https://ethereum-sepolia-beacon-api.publicnode.com
```

It checks that every journal line has its manifest row (same block, result, execution gas; the fee recorded is what
the payer's balance fell by, read from the sequencer before and after), that a transaction alone in its block carries
the block's own gas totals (they come from the block's fee transaction, which the sequencer made), that every applied
job is `Open`, `Miller`, [`Adopt` 1/2, 2/2,] `Finish` in increasing blocks, that the finalized slots of the advancing
jobs increase, that crown-driver's own export after the run (`export/instances.tsv`, its own follower of the chain)
holds the last advancing job's finalized slot and root, and with `--beacon` that each applied root is canonical at its
slot. Output of the runs of 13 November 2026: `check-output.txt`.

`export/blocks.borsh` holds every block of a zone's final chain as the sequencer served it (program bytecode taken
out, as `lez/live/results/`); `export/chain.tsv` one line per block (all `Finalized` on Bedrock at the export).

To run it again (about 6 minutes of session A, then the daemon; nothing is sent to any public network):

```sh
# the binary, on the build box (lez/crown/build.sh seed brings the crown build's target directory)
../modal-build/mrun --profile <yours> -- 'bash lez/crown/build.sh seed && cd lez/updater/sequencer && CARGO_TARGET_DIR=$(cd ../../../.. && pwd)/tmp/crown/live/target cargo build --release'
e2e/crown.sh updater crown-NAME --bin PATH/ferrata-updater --minutes 120 [--restart-min 25]
e2e/crown.sh updater-status crown-NAME; e2e/crown.sh updater-fetch crown-NAME DIR
```

## Findings

- **A zone's Bedrock funds run out.** LEZ's local configuration funds the sequencer's inscriptions on Bedrock for
  about 1,468 blocks: at 01:19:19Z, after 4.1 hours of 10 s blocks on zone 1 (its sessions A, U1 and U3; U2's blocks
  and their inscriptions were lost with its container), the node refused the next one ("Wallet does not have enough
  funds, available=54625", every 10 s from then: 82 times in U4's `sequencer.log.gz`). The zone cannot move after that;
  a client sees its transaction never settle. A long-running zone needs its channel's Bedrock wallet topped up, an
  operator concern for every client of a LEZ zone, not only the updater.
- **The daemon says too little while its round waits.** In U4 it waited on its `Open` for 14 minutes (its settlement
  timeout is LEZ's 1,500 s) while `/health` answered 503 with no body. A round that waits on the chain should report
  it (the transaction, since when, the chain's last final block): not built.
- **A preempted container loses the zone since its last generation.** Modal preempted U2's container after 1.9 hours
  and re-ran the call; the zone came back from generation 2 and U2's blocks (538 to at least 1,200) were lost. The daemon lost nothing it
  needed: it reads the chain, and on the restored chain it caught up in one job. A zone that must keep its blocks needs
  a machine that is not preempted or frequent generations (U3 → U4 was one: about six minutes without the daemon).
- **One light-client provider sets the pace at a period boundary.** For about 13 minutes after period 1412 began,
  Lodestar's light-client data could not move the instance although Sepolia had finalized two more checkpoints (above).
  Lodestar is the only public provider of light-client data found (`../../../lab/README.md`); a second one would be a
  second `light-client` source.
- **Lodestar's bootstrap of the checkpoint answers HTTP 500** (as the fixture's `SOURCE.md` found). The daemon did not
  need it: session A had initialised the instance, and the committees came from Lodestar's period updates.
- **`finish_own_jobs` matters on live data.** A beacon node serves only its newest finality update, and a job takes
  about 80 s, so the update a job was opened for can be gone from the source before its `Finish`. The daemon finished
  every job it opened, from `state/sepolia/open/`, also after the two restarts mid-job (U2, U3).

## Not shown

- One daemon. The A3 gate's two honest daemons and an adversary (deferral, refused duplicates, fee shares) ran in
  process only (`../gate*.txt`); on this zone nothing competed with the daemon.
- A local zone (one sequencer, one Bedrock validator with dev-mode leader proofs), not a network; no public LEZ v0.4
  network exists. Hours, not proposal A4's seven days on testnet 0.4.
- The proven path (`VerifyAll`) and the daemon as a Logos module.

## Files

| file | what |
|---|---|
| `check.py` | the re-check above |
| `zone-N/journal.tsv` | the daemon's journal on the zone's final chain: per transaction its round, step, job, finalized slot, result, exit code, execution and storage gas, fee, block, time, label (cumulative over the zone's sessions: the journal is in the daemon's state, saved with the zone) |
| `zone-N/run.tsv` | the sequencer backend's manifest of the same transactions (hash, block, position, status, gas, base fees, fee, payer debit, the block's own totals) |
| `zone-N/updater-uK.log` | the daemon's log of session U*K* (with the backend's line per transaction) |
| `zone-N/probes-uK/` | `/health` and `/metrics` of session U*K* |
| `zone-N/updater.toml` | the configuration as it ran (written by `lez/crown/session.sh u`) |
| `zone-N/export/` | crown-driver's export of the zone's final chain after the daemon stopped: `blocks.borsh`, `chain.tsv`, `instances.tsv`, `programs.tsv`, `segments.tsv`, `summary.txt` |
| `zone-N/sessions/` | per session: `session.log`, the node's and the sequencer's logs (`bedrock-excerpt.log`, `sequencer.log.gz`), the sequencer's phase, the container's resources, the state directory at the restart and at the end; `sessions/a/`: session A (`session-a.json`, its driver log, its export); `session-u.out`: the cloud function's log of every session |
| `zone-1/u2-lost/` | session U2's own records: its blocks are not on zone 1's final chain (below) |
| `zone-N/summary.md` | `check.py --write`: the table of applied updates |
