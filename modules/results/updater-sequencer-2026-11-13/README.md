# `ferrata_updater_module` on a running LEZ sequencer, inside a headless Logos Core (13 November 2026)

The updater module now carries both of the daemon's backends. `ledger = "sequencer"` opens
`lez/updater/sequencer/`'s library (`open_ledger`), the same code the `ferrata-updater` binary of that directory runs
(EVIDENCE row 99): LEZ's wallet client sends each transaction, and a follower applies every block final on Bedrock
with LEZ's block application and reads the transaction's result. The module links LEZ v0.4.0-rc1's node-side crates
for it (`modules/scripts/stage-updater.sh` stages them together with the prebuilt circuit and rapidsnark libraries
they embed). It was built with logos-module-builder, checked through the dev and the portable Logos Core, and then run
in the portable headless Logos Core on a crown zone: a LEZ v0.4.0-rc1 sequencer and a Logos Blockchain node, keeping
the crown run's Sepolia instance following Sepolia from Lodestar's light-client data, with publicnode's headers as
the second source. Nothing was sent to Sepolia or any public network: the beacon APIs were only read, and every
transaction went to the zone's own sequencer.

## Where it ran

| part | what |
|---|---|
| tree | commit `2d86798`; the module's `version()` reports `staged: 2d86798`, `ledgers: ["harness", "sequencer"]`, the attestation program `0x816d1fef…03df` declared and computed from its bytes, the circuits `v0.5.7` and rapidsnark `v0.0.8` (PIC) libraries it embeds |
| build and checks | Modal build box (workspace ws-3, app `ferrata-build`): `modules/scripts/check.sh` with `MODULES=ferrata_updater_module` ([`check/`](check/summary.md)) |
| packages | `nix build .#lgx-portable`: `logos-ferrata_updater_module-module-lib.lgx`, 61,016,822 bytes, SHA-256 `962304cc…29c2` (16 MB before, with the harness alone); the portable `logosctl` (`logos-logoscore-cli#ctl-bundle-dir` at `66d098d`), 353 MB, packed as `logos-portable.tar.zst` (108,744,582 bytes, SHA-256 `80f57e19…b45a`), with the system loader `/lib64/ld-linux-x86-64.so.2` and no reference into `/nix` |
| zone | the crown run's session machinery on Modal: workspace ws-3, app `ferrata-crown`, function `updater_session` (2 cores requested, up to 8; 8 GiB), the crown bundle `0013bab1…` (LEZ v0.4.0-rc1 `sequencer_service`, a Logos Blockchain node `d480efc6`, LEZ's local configuration with 10 s blocks, one validator, dev-mode leader proofs); session A deployed the final programs and initialised the Sepolia instance `0xa5fb2591…b2b5` from `crates/ferrata-spectests/fixtures/sepolia-2026-11-12` (checkpoint `0x3b901f5e…ccf7c`, slot 11,562,112, period 1411) and that period's update |
| Logos Core | the portable `logosctl` in the zone's container (Ubuntu 24.04, no Nix): `daemon start`, `install` of the portable package (unsigned, as every local package), `module load`, then the module's methods with `logosctl call` (`lez/crown/session.sh u`, module mode; `e2e/crown.sh updater RUN --module DIR`) |
| daemon | `start` with the configuration `session.sh` writes ([`zone/updater.toml`](zone/updater.toml)): `ledger = "sequencer"`, `layout = "gloas"`, `min_agreeing = 2`, `poll_secs = 24`, `finish_own_jobs`, payer LEZ's published test key; `/health` and `/metrics` served by the module on `127.0.0.1:9464`, read every minute, and the module's `status` through Logos Core at every probe |
| mode | the sequencer executes and verifies every transaction (no `RISC0_DEV_MODE` in the zone or in Logos Core); no proof is made: the public job path |

## What happened

Modal preempted the zone's container twice. Both times it re-ran the call from the zone's last generation, which is
how the crown machinery recovers (as in row 99):

| session | UTC (13 Nov) | blocks | what |
|---|---|---|---|
| A | 09:08:09–09:12:33 | 1–24 | a new zone; both programs deployed (the image ids of `docs/image-ids.md`); `Init` and the period-1411 update applied; generation 1 saved |
| U (first) | 09:12:33–about 09:14:40 | lost | the module installed, loaded and started (its follower at block 25 in 12.1 s); preempted before any job. Records: `zone/sessions/session-u-preempted/` |
| U2 | 09:15:54–about 10:05 | 25–283, lost | from generation 1: **9 updates applied** (31 transactions), a **restart mid-job through the module's `stop` and `start`**; preempted 40 s before its planned end, so no export and no generation. Its records were mirrored every minute: [`zone/u2-preempted/`](zone/u2-preempted/summary.md) |
| U3 | 10:06:39–10:30:17 | 25–171 | from generation 1 again: **5 updates applied** (19 transactions), the restart through `stop` and `start` again; stopped on request after 24 minutes (to limit the exposure to preemption); crown-driver's export; generation 2 saved (head block 171). [`zone/`](zone/summary.md) |

On the zone's kept chain (blocks 1–171), the module applied 5 updates in 19 transactions, all applied: one finality
update that rotates the committees (period 1412's committee signed it), the period-1412 update that **adopts the next
committee** (1413's, five transactions, 16,117,935 gas), and three finality updates (about 9.24M gas each, three
transactions). Each transaction was final on Bedrock about 15 s after the sequencer took it. With U2's 9 updates on
blocks that were lost with its container, the module applied **14 updates in 50 transactions** (12 finality updates,
2 of them rotating the committees, and 2 adoptions). None was refused, reverted, dropped or rejected.

| # (U3) | finalized slot | effect | blocks (Open–Finish) | Finish (UTC) | gas |
|---:|---:|---|---|---|---:|
| 1 | 11,569,152 | advances, rotates the committees | 27–35 | 10:08:17 | 9,242,611 |
| 2 | 11,567,104 | adopts the next committee | 39–55 | 10:11:37 | 16,117,935 |
| 3 | 11,569,184 | advances | 59–67 | 10:13:37 | 9,243,108 |
| 4 | 11,569,216 | advances | 86–94 | 10:18:07 | 9,242,982 |
| 5 | 11,569,248 | advances (its job across the restart) | 123–130 | 10:24:07 | 9,243,334 |

**The restart through the module.** In both sessions the restart was armed 15 minutes after `start`. It fired as soon
as the journal's last line was an applied `Open`, that is, with a job half done. Then `session.sh` called `stop` and,
15 s later, `start` again:

- U2, 09:31:41: `stop` returned at once (round 30, the round in hand being over), and the backend saved its follower.
  `start` at 09:31:57 restored the follower (block 121 in 0.2 s, against 12.1 s from LEZ's genesis at the first
  start). The daemon found its job in the state directory ("job 0x85c396fa… (finalized slot 11568992) is open: its
  miller first") and sent `Miller` (block 123) and `Finish` (block 127). There was no second `Open`.
- U3, 10:22:57: the same. `Open` of slot 11,569,248 was in block 123 before `stop` (round 31); after `start` at 10:23:12
  (follower at block 124 in 0.2 s), `Miller` went into block 126 and `Finish` into block 130.

**Health.** U3 answered 25 of its 26 `/health` probes `ok` (HTTP 200), and U2 50 of 51. The first probe of each
session came before the module listened. Every `status` call through Logos Core reported `running: true`, healthy,
and the instance as the chain holds it (`probes-u1/status.jsonl`).

## Re-check

From the repository root, with Python 3 only:

```sh
python3 -I lez/updater/results/live-sepolia/check.py modules/results/updater-sequencer-2026-11-13/zone
python3 -I lez/updater/results/live-sepolia/check.py modules/results/updater-sequencer-2026-11-13/zone/u2-preempted
# and every applied finalized root against a Sepolia beacon node of your choice (read only):
python3 -I lez/updater/results/live-sepolia/check.py modules/results/updater-sequencer-2026-11-13/zone --beacon https://ethereum-sepolia-beacon-api.publicnode.com
```

[`check-output.txt`](check-output.txt) holds the output of 13 November: **ALL PASS** for both. For U3: 19 of 19
journal lines have their manifest row, each fee charged equals the payer's debit, each block's own gas totals are
right, the jobs have the shape Open, Miller, [Adopt 1/2, 2/2,] Finish, the 4 advancing jobs each go further, the
export holds finalized slot 11,569,248 (`1e34457e…`) and 7 jobs (session A's 2 and the module's 5), and the 5 roots
are canonical on publicnode. For U2: 31 of 31 transactions and 9 jobs, the roots canonical, no export.

To run it again (about 5 minutes of session A, then the module; nothing is sent to any public network):

```sh
# on the build box: the module's packages and the portable logosctl (check.sh builds both)
MODULES=ferrata_updater_module modules/scripts/check.sh OUT
mkdir DIR && cp OUT/lgx-portable-ferrata_updater_module/*.lgx DIR/ && tar -C OUT/logos-portable/ -hcf - . | zstd -o DIR/logos-portable.tar.zst
e2e/crown.sh updater crown-NAME --module DIR --minutes 50 --restart-min 15
e2e/crown.sh updater-status crown-NAME; e2e/crown.sh updater-fetch crown-NAME FETCHED
```

## The checks on the build box ([`check/`](check/summary.md))

`check.sh` with the updater module, dev and portable, `E2E=1`, at `2d86798`: **step failures 0, check failures 0**.
`updater-e2e.sh` passed 8 of 8 natively and 10 of 10 through each Logos Core. The in-process chain, as in row 97: the
lab's records from two sources each; mainnet to slot 15,631,808 (`530599ec…2328`) and Sepolia to 11,556,000
(`d31b7d34…f3cc`); 187 transactions applied and none refused; 117 s in each Core. The check that used to see
`ledger = "sequencer"` refused now sees the backend carried: with no sequencer at its address, the set-up fails on
the connection, as the binary's does. The calls crate's own tests passed 3 of 3, among them
`the_sequencer_backend_is_carried`.

## Found on the way

- **Nix needs the build-time downloads staged.** In a normal cargo build, LEZ's node-side crates download two things:
  the circuits' prebuilt artifact (`logos-blockchain-circuits-*-sys`, into the user's cache) and rapidsnark's static
  libraries (`rust-rapidsnark`, after reading a version file of its git checkout). Nix vendors each crate on its own,
  without network access or that file. `stage-updater.sh` now stages both beside the crate, and
  `rust-lib/.cargo/config.toml` names them (`LBC_ROOT_DIR`, `RAPIDSNARK_LIB_DIR`).
- **The builder at the pinned revision (`5f74cdb`) reads `nix.cmake.extra_link_libraries` and does not use it.** The
  plugin first failed to load (`undefined symbol: SCardEndTransaction`: LEZ's wallet links PC/SC for keycards). It now
  links `pcsclite` through `logos_module(... LINK_LIBRARIES pcsclite)`.
- **The two lockfiles disagree on `keccak` 0.2.** LEZ patches it from logos-blockchain's sponges; the module's old
  lock pinned the registry's 0.2.2. The merged lock takes LEZ's.
- **`updater_session` is preemptible.** Modal preempted it twice in 80 minutes; the zone came back from its last
  generation each time, and the daemon lost nothing it needed. The blocks since that generation were lost.
- **The package is larger: 61 MB portable**, against 16 MB. Its host needs `curl` (the beacon sources) and
  `libpcsclite` at run time.

## Not shown

- The A3 gate's three daemons (two honest, one adversarial) on a sequencer, and 100 updates. One module ran, on a
  one-validator devnet. Sessions of minutes, not proposal A4's seven days on testnet 0.4.
- Light-client data through Logos Core's own modules. The module reads the beacon APIs with `curl`, as the binary
  does.
- The proven path (`VerifyAll`).

## Files

| file | what |
|---|---|
| `zone/` | session U3's record in `check.py`'s layout: `journal.tsv`, `run.tsv`, `updater-u1.log` (the daemon's own lines, taken from the Logos Core log), `probes-u1/` (`health.jsonl`, `status.jsonl` through Logos Core), `export/` (crown-driver's export after the run), `updater.toml`, `summary.md` (`check.py --write`), `module/` (every `logosctl call` request and reply: `version`, `check_config`, `start-1`, `stop-1`, `start-2`, `stop-2`, `metrics-final`, `show.json`, `install.txt`), `sessions/` (`session-u.out`, the Logos Core logs, the session's logs, session A's records, the first session U's) |
| `zone/u2-preempted/` | session U2's record in the same layout, without the export |
| `check/` | `check.sh`'s summary, steps and the updater runs (native, dev, portable) |
| `check-output.txt` | `check.py`'s output for both records |
