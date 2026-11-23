# `ferrata_updater_module` on our public LEZ test network, its Ethereum data through Logos Core (23 November 2026)

One headless Logos Core (`logosctl` 66d098d) ran `ferrata_updater_module` with `daemon.ledger = "sequencer"` against
our public test network (LEZ v0.4.0-rc3 anchored to its own Logos Blockchain node; `https://<domain>/rpc`), keeping a
Sepolia instance of our own following Sepolia for 90 minutes, **both** of its light-client sources going through Logos
Core's `beacon_api_module`. Beside it in the same Core: `beacon_api_module`, `eth_rpc_module` (Sepolia's execution
endpoint) and `ferrata_attest_module`, whose calls read Ethereum through `eth_rpc_module`; `attest-monitor` watched the
same instance. Every process was sampled every 5 s and each daemon's traffic was counted by its own relay (RFP-021
P.6). The attestation CLI's journey and the day-one tool were measured in the same run, and RFP-022's two
Ethereum-side daemons ran as Logos modules on `bench/perf/daemons-anvil.sh`'s anvil fixture (RFP-022 P.6).

Transactions went to our test network only: the instance's `Init`, the updater's 49 transactions and one attestation.
Ethereum was only read. The network's own instance (`c741a1f1…`, which the host's updater follows) was not touched.

**Result: 27 of 27 checks** ([`summary.md`](summary.md), written by `modules/scripts/live_testnet.py` from this
record's files; `python3 -I modules/scripts/test_live_testnet.py` computes every figure and check again, offline).

## Where it ran

| part | what |
|---|---|
| the network | our public LEZ v0.4.0-rc3 test network (`sequencer_service` `bafd07be`, a Logos Blockchain node 0.3.5-rc.1, 10-s blocks, final on Bedrock about 20–30 s later), reached at its public JSON-RPC through its gateway; the attestation program at `6f9zuRR63bSN5AEeffqZeACYCwFmqfe5JJhPshuJfYNg`, image `0x816d1fef…03df`, immutable (read back by `ferrata-day1 inspect`, its seven segments followed from the header: [`instance/inspect-after-init.json`](instance/inspect-after-init.json)) |
| the box | the Modal build box (gVisor, x86-64; 2 cores requested, more allowed), one container for the whole session |
| the modules | built on the box with logos-module-builder `5f74cdb` from this branch at `fea8b8e5` ([`report-build/`](report-build/steps.md)): `ferrata_updater_module` from a copy of the tree with LEZ v0.4.0-rc3 selected (`lez/scripts/lez-select.sh`, plus the harness's wire crates and the module's calls crate, which the script leaves on rc1), its lockfile [`modules/ferrata_updater_module/rust-lib/lockfiles/v0.4.0-rc3.Cargo.lock`](../../ferrata_updater_module/rust-lib/lockfiles/v0.4.0-rc3.Cargo.lock), dev package 64,380,611 bytes; the guests it carries are the tree's own (`version()`: the attestation program declared and computed `0x816d1fef…03df`, the image the network runs); `ferrata_attest_module` from the tree as committed (rc1), 3,565,635 bytes; Logos's `beacon_api_module` (`aca138e`) and `eth_rpc_module` (`6965f32`), as `modules/scripts/verified-proxy.sh` builds them |
| the tools | `ferrata-day1` and `ferrata-updater` for rc3 (the same copy; `ferrata-day1` rebuilt at `0057fe4f` before the session), `attest-monitor` and the `ferrata-attest` CLI from the tree as committed; sha256 of each in [`report-build/artifacts.txt`](report-build/artifacts.txt) |
| Ethereum | light-client data: Lodestar and Nimbus (`beacon_api_module`'s providers `lodestar-counted`, `nimbus-counted`, added with its `add_provider`, each through a counting relay); execution: ethpandaops's Sepolia endpoint through `eth_rpc_module` (mode `off`: straight to the endpoint); attest-monitor: Lodestar and publicnode. Reads only, no API key |

## What happened (UTC, 23 Nov)

| when | what |
|---|---|
| 14:37–14:55 | `live-testnet.sh instance`: fresh keys (`ferrata-day1 keygen --program 6f9zuRR6…`: the network's program by its account, no program keys), two faucet claims of 10 LGO (each account and its public key only), a fresh finalized Sepolia checkpoint (slot 11,642,496, root `0x7954ad80…`; Lodestar's bootstrap, publicnode's header), **`Init` applied in block 1,698** (114,811 execution gas). Instance `0x3147ce50…ddbb` ([`instance/`](instance/)). Each `ferrata-day1` command followed the network from its genesis (sign 489 s, send 558 s) |
| 14:58:59 | Logos Core started; the four modules installed and loaded; `beacon_api_module`'s two providers added; `eth_rpc_module` given Sepolia's execution endpoint |
| 14:59:11–15:08:07 | the updater module's `start`: its set-up followed the network from genesis to block 1,722 (head 1,776) in 535.6 s, every block read through the relay and applied with LEZ's state transition, then answered `started` |
| 15:08–16:37 | the module kept the instance current: the initial committee adopted (blocks 1,779 and 1,784), the period-1421 update adopting the next committee (blocks 1,789–1,807), then 13 finality updates; **49 of 49 transactions applied, 15 jobs finished**, blocks 1,779–2,302 |
| 15:16:55 | right after an applied update, on the instance's newest record (slot 11,642,688, Sepolia block 12,196,417): `ferrata_attest_module`'s calls (below) and **one attestation of the Sepolia vault's slots 0 and 1, applied in block 1,831** (249,282 execution gas; `ferrata-day1`, the day-one kit's stage d) |
| 15:53:39 | the restart through the module: `stop` (0.20 s; the round in hand finished, the backend saved its follower), 15 s, `start` again (1.11 s: the follower restored, to block 2,049 in 1.0 s); 21 transactions applied after it |
| 16:38:07 | the final `stop` (0.13 s), attest-monitor stopped by SIGTERM (exit 0, 88 polls), `logosctl daemon stop` exit 0; then the CLI's journey |

The instance at the end, as the network holds it: active, 16 jobs applied, finalized slot **11,643,104** (root
`0xf1800d40…6194`), the next committee known ([`session/final-state.json`](session/final-state.json)). Every
finalized header the module's jobs moved it to (14) was confirmed canonical by attest-monitor's two providers
([`session/monitor/seen.json`](session/monitor/seen.json)). Each transaction was final on Bedrock 15.1–26.8 s after it
was sent (median 22.9 s; the module's own lines). `/health` answered `ok` to 87 of 88 probes (the first, as `start` answered, came before the
module's first round); `status` through Logos Core answered `running` 88 times of 88.

## RFP-021 U.3, clause by clause

| the RFP asks | shown here |
|---|---|
| a process that follows Ethereum through the Logos Ethereum core module | the updater's two light-client sources are `via = "beacon_api_module"` ([`session/req/updater.toml`](session/req/updater.toml)); `beacon_api_module` carried its 258 requests to Lodestar and 256 to Nimbus (the relays' counts), 7.5 and 7.3 MiB of light-client data, two required to agree (`min_agreeing = 2`). Logos's announced light-client API of its Ethereum module does not exist yet; `beacon_api_module` is Logos Core's module for the beacon API |
| detects when the tracked instance is behind, and submits the outstanding light-client updates to LEZ | the journal ([`session/journal.tsv`](session/journal.tsv)) and the backend's manifest ([`session/run.tsv`](session/run.tsv)): 15 jobs, 49 transactions, every one applied on the network; `/health`'s `last_plan` alternates between `up-to-date` and the next step |
| a Logos module accompanied by a Logos Core headless CLI/daemon, runnable standalone | the module in `logosctl daemon` (headless, nothing else running in that Core but Logos's own modules); its methods `start`, `status`, `metrics`, `stop` through `logosctl call` ([`session/calls.tsv`](session/calls.tsv)). The same daemon as a binary, `ferrata-updater`, is the network's own updater |
| configurable Ethereum consensus and execution RPC endpoints | consensus: the operator's providers added to `beacon_api_module` (`add_provider`), each source naming its provider (`provider = "lodestar-counted"`); execution: the endpoint given to `eth_rpc_module` (`set_chain_config`), through which `ferrata_attest_module` read the record's block, its receipts and a storage proof. The updater itself needs no execution endpoint: a light-client update carries the execution header the program records |
| structured logging | one line per transaction (step, result, gas, block, send-to-final time, hash: [`session/updater-module.log`](session/updater-module.log)), the journal (TSV), `/health` (JSON) and `/metrics` (Prometheus), and `status` through Logos Core (JSON, [`session/probes-u1/`](session/probes-u1/)) |
| a clean shutdown path | `stop`: the round in hand finishes and the backend saves its follower, then `logosctl daemon stop` exit 0; after the restart the module went on from the saved follower |
| the operator journey: install, configure, run, monitor | [operator guide](../../../docs/guides/operator-guide.md) §0 and §3; monitoring by `attest-monitor` on the same instance (88 polls, its alerts in [`session/monitor/`](session/monitor/)); this run's commands in `modules/scripts/live-testnet.sh` |

## RFP-021 P.6: CPU, memory and time of the off-chain components (measured)

Process samples every 5 s (`bench/perf/tree_sample.py`: each process with its children, the CPU of children it has
waited for included); the box's gVisor kernel does not report a process's peak (VmHWM), so a peak is the largest
sample. Over the updater's 5,928 s (from its `start` to its final `stop`):

| component | CPU s | share of one core | peak MiB | median MiB |
|---|---:|---:|---:|---:|
| `ferrata_updater_module` (its host process with the executor it starts) | 714.39 | 0.120 | 270.9 | 148.1 |
| — its start: the follower from the network's genesis (531.4 s) | 492.87 | 0.928 | 270.9 | 244.0 |
| — after the start (5,391.5 s: 15 jobs, a restart) | 216.72 | **0.040** | 268.4 | 148.1 |
| `beacon_api_module` | 2.24 | 0.0004 | 59.6 | 59.5 |
| `eth_rpc_module` | 0.67 | 0.0001 | 57.7 | 57.7 |
| `ferrata_attest_module` (idle but for its calls) | 1.32 | 0.0002 | 58.7 | 58.7 |
| — during its calls (60.1 s) | 0.89 | 0.015 | 58.7 | 54.7 |
| Logos Core's daemon (`logosctl`, its own process) | 0.47 | 0.0001 | 58.2 | 58.2 |
| Logos Core with every module host process (its tree: Logos's five own modules and these four) | 723.10 | 0.122 | 776.1 | 655.6 |
| `attest-monitor` (polling every 60 s, its `curl`s included; 5,391.5 s) | 17.99 | 0.0033 | 22.2 | 4.2 |

The module calls, through `logosctl` ([`session/calls.tsv`](session/calls.tsv)): `verify_checkpoint` 5.31 s (the
checkpoint's bootstrap against the program's `Init` checks, `KeyValidate` on all 512 keys, three beacon providers
agreeing, the execution block through `eth_rpc_module`: verdict `ok`), `fetch_log_proof` through `eth_rpc_module`
1.16 s (the record's block and its receipts, the trie rebuilt), `dry_run_attest` 0.07 s (a storage slot with its proof
given), 0.39 s (the proof read through `eth_rpc_module`) and 0.09 s (the log), `read_instance` 0.08 s,
`build_attest_tx` 0.08 s (the session's first call passed the program's account in base58 and was refused,
`invalid_request`; called again with its hex in the same Core, `A-build_attest_tx-hex`).

The one-shot tools (GNU time):

| tool | wall s | CPU s | peak MiB |
|---|---:|---:|---:|
| `ferrata-attest` CLI, its journey of `lez/ATTEST-CLI.md` (in-process chain, recorded Sepolia data, dev mode; exit 3, the refusal 130 its dry run predicted) | 119.15 | 121.61 | 137.1 |
| `ferrata-day1 send` (the `Init`, following the network from its genesis first) | 557.86 | 517.65 | 137.1 |
| `ferrata-day1 sign` (the attestation; the follower resumed) | 0.98 | 0.90 | 114.3 |
| `ferrata-day1 send` (the attestation, until final on Bedrock) | 24.58 | 1.19 | 116.8 |

Traffic (each daemon's own relay; the HTTP requests and answers, TLS not counted), after the updater's start, over
5,370 s (per hour *derived*):

| daemon → endpoint | requests | up | down | MB per hour up / down |
|---|---:|---:|---:|---:|
| updater module → the network's RPC | 8,952 | 2.12 MiB | 31.47 MiB | 1.49 / 22.12 |
| updater module → Lodestar (through `beacon_api_module`) | 257 | 0.03 MiB | 7.44 MiB | 0.02 / 5.23 |
| updater module → Nimbus (through `beacon_api_module`) | 256 | 0.03 MiB | 7.32 MiB | 0.02 / 5.15 |
| attest-monitor → the network's RPC | 88 | 0.02 MiB | 0.21 MiB | 0.01 / 0.15 |
| attest-monitor → Lodestar / publicnode | 261 / 261 | 0.03 / 0.03 MiB | 0.19 / 0.24 MiB | 0.02 / 0.14, 0.02 / 0.17 |

During its start the module's follower read about 1,700 requests and 7.6 MiB from the network (the relay's series
point 8 s before `start` answered: 1,699 requests, 7,957,117 bytes down); the whole session: 10,782 requests,
2.47 MiB up, 39.27 MiB down. Its follower applies every block of the network, the host updater's
transactions included, so its cost grows with the network's use, not only with its own instance.

## RFP-022 P.6: `monitor` and the relayer as Logos modules (measured)

`bench/perf/daemons-anvil-modules.sh`: the run of `bench/perf/daemons-anvil.sh` (anvil, a rule (B) FerrataVault v5,
ten idle minutes, an honest candidate the monitor checks and the relayer finalises, ten idle minutes) with the two
daemons started as `ferrata_monitor_module` and `ferrata_relayer_module` in one headless Logos Core (their `start`
with the binaries' own arguments), 14:51–15:13. The candidate was finalised (final state = wanted); every `start`
and `stop` answered without error; `logosctl daemon stop` exit 0.

| process (1,278.5 s) | CPU s | share of one core | peak MiB | JSON-RPC bytes up / down |
|---|---:|---:|---:|---:|
| `ferrata_monitor_module` | 0.33 | 0.0003 | 66.4 | 86,279 / 254,039 |
| `ferrata_relayer_module` | 0.70 | 0.0005 | 73.9 | 411,356 / 983,811 |
| Logos Core's daemon (its own process) | 0.00 | 0.0000 | 57.0 | — |
| Logos Core with every module host process | 1.84 | 0.0014 | 480.3 | — |

Against the binaries' run (`lez/results/perf-daemons.md`: `monitor` 0.53 CPU s and 14.0 MiB, the relayer 0.87 CPU s
and 20.3 MiB, the same traffic to within 1%): as modules the daemons cost the same CPU and traffic; each host
process holds about 55–60 MiB of its own (the idle `eth_rpc_module` host in the live run: 57.7 MiB).

## Not shown here

- RFP-022 P.6's other gaps: the importer as a daemon (no bridge configuration on this network: the vault's Sepolia
  step is the owner's), `ferrata_attestor` (it reads its Logos Blockchain node's API, which this network does not
  expose), `monitor`'s traffic to a LEZ node (it has no LEZ client: its node view comes from a file or a command).
- The proven path, the A3 gate's three daemons (RFP-021 R.9, R.10), and Logos's testnet 0.4 (this network is ours).
- The session's first `start` used the module's default (`quiet`): the daemon's own job lines (finalized slot, root,
  sources) are in the log only after the restart, which asked for them. `check.py`'s check that every applied job's
  update is in the log therefore fails for the first life; its checks of the journal against the manifest pass, and
  every job's root is checked by attest-monitor's providers instead (`summary.json`).
- An earlier attempt's container was lost before its call ended (its keys with it: two faucet claims, and an `Init`
  that may have been applied under an unknown instance id).

## Re-check

```sh
python3 -I modules/scripts/test_live_testnet.py        # every figure and check again from these files (offline)
python3 -I lez/updater/results/live-sepolia/check.py modules/results/live-testnet-2026-11-23/session   # journal vs manifest
# the run (build box; FERRATA_TESTNET_RPC = the network's RPC, FERRATA_TESTNET_SEND=yes; sends to that network only):
modules/scripts/live-testnet.sh build OUT && modules/scripts/live-testnet.sh instance OUT
modules/scripts/live-testnet.sh session OUT 90 && modules/scripts/live-testnet.sh build-bridge OUT
modules/scripts/live-testnet.sh bridge-modules OUT 10 && modules/scripts/live-testnet.sh collect OUT RECORD
```

## Files

| file | what |
|---|---|
| `summary.md`, `summary.json` | the figures and checks |
| `instance/` | the network's `programs.json`, the instance (`instance.json`), the accounts and their public keys, the two faucet answers, the `Init` transaction's record, `inspect` after it |
| `session/req/`, `session/rep/` | every request and reply of the module calls; `updater.toml` |
| `session/calls.tsv`, `session/marks.tsv` | each call's duration; the session's phase marks |
| `session/journal.tsv`, `session/run.tsv`, `session/updater-module.log` | the updater's journal, its backend's manifest, its own lines from Logos Core's log |
| `session/probes-u1/` | `/health` every minute (`check.py`'s layout), `status` through Logos Core |
| `session/samples.csv`, `session/relay/` | the process samples; each relay's totals, per method or path, and series |
| `session/monitor/`, `session/attest/`, `session/cli/` | attest-monitor's status, alerts, verdicts and log; the attestation (sign, send, verdict); the CLI's output and GNU time |
| `session/logos-logs/` | Logos Core's whole log (gzip) |
| `daemons-anvil-modules/` | RFP-022's modules on anvil: samples, phases, the proxies' counts, every reply |
| `report-*/` | each command's steps and the logs that identify what was built |
