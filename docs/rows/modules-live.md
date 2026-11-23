# Rows: modules-live (the updater module on our public test network; the off-chain components' resources)

For the integrator. Nothing here edits `docs/EVIDENCE.md`, `docs/requirements-status.md`, `docs/traceability.md`,
`docs/performance.md`, the operator guide or the README's status lines: the texts to apply are below. The record:
[`modules/results/live-testnet-2026-11-23/`](../../modules/results/live-testnet-2026-11-23/README.md) (27 of 27
checks; `summary.md` written by `modules/scripts/live_testnet.py` from the record's files). Re-check, offline:
`python3 -I modules/scripts/test_live_testnet.py` (every figure and check computed again from the record, `check.py`
on the journal, no machine path). Branch `modules-live`; the run's tools at `fea8b8e5` (modules) and `0057fe4f`
(session); the record at `96d7b862`.

## Proposed status changes

| Row | Now | Proposed | Why |
|---|---|---|---|
| RFP-021 U.3 | in part | **shown** | The one clause the row lacked: the updater **module** on a running sequencer with its Ethereum data through Logos Core. On our public LEZ v0.4.0-rc3 network, `ferrata_updater_module` in a headless Logos Core (`logosctl daemon`), both light-client sources `via = "beacon_api_module"` (514 light-client requests through it, two providers required to agree), kept our own Sepolia instance current for 90 min: 15 jobs, 49 of 49 transactions applied (journal = manifest, `check.py`), the instance from slot 11,642,496 to 11,643,104 (every header confirmed canonical by attest-monitor's two providers), a restart through `stop`/`start`, a clean stop (`logosctl daemon stop` exit 0); the execution endpoint configured in `eth_rpc_module`, which the attestation module in the same Core read through; `/health` ok 87 of 88, `status` through Logos Core 88 of 88; monitored by attest-monitor (88 polls). Install, configure, run, monitor: operator guide §0, §3 (proposed addition below) and `modules/scripts/live-testnet.sh`. Condition stated, as today: Logos's announced light-client API of its Ethereum module does not exist; `beacon_api_module` is used until it does. The network is ours, not Logos's testnet 0.4 (the RFP's U.3 does not name it). |
| RFP-021 P.6 | in part | **shown** | What the row lacked is measured in one run on real code: the `ferrata-attest` CLI (its journey: 119 s, 121.6 CPU s, 137 MiB), `ferrata_attest_module` (0.0002 of a core, 58.7 MiB; its calls 0.07–5.31 s), attest-monitor (0.0033 of a core, 22.2 MiB peak, 0.15 MB/h from the network), the updater as a Logos module (0.040 of a core after its start, 148 MiB median, 270.9 MiB peak; its start from the network's genesis 493 CPU s in 531 s; 22 MB/h from the network and 5.2 MB/h from each light-client provider), Logos Core itself (its daemon 0.0001 of a core and 58 MiB; each module host process 57–60 MiB) and the day-one tool. With the updater daemon (EVIDENCE row 172) and the proven path's prover (row 123), every off-chain component of the RFP-021 design is measured. The documentation to apply: `docs/performance.md` RFP-021 §P.6 (below). |
| RFP-022 P.6 | in part | in part (narrower) | Now measured: the daemons as Logos modules: `ferrata_monitor_module` and `ferrata_relayer_module` on `daemons-anvil.sh`'s fixture (0.0003 and 0.0005 of a core, 66.4 and 73.9 MiB, the binaries' traffic to within 1%), and the updater module whose records the imports need (above). Still not measured: the importer as a daemon (our network's bridge B1 has no vault configured: the vault's Sepolia step is the owner's), `ferrata_attestor` (it reads its Logos Blockchain node's API, which our network does not expose), `monitor`'s traffic to a LEZ node (it has no LEZ client: RFP-022 U.3). |

## Proposed EVIDENCE row (the next number)

| # | What it shows | Numbers | Where it ran | Proofs | Re-check | Details |
|---|---|---|---|---|---|---|
| next | **The updater module on a public LEZ network with its Ethereum data through Logos Core (RFP-021 U.3), and the off-chain components' resources in the same run (RFP-021 P.6, RFP-022 P.6)**: `ferrata_updater_module` (sequencer backend, built for LEZ v0.4.0-rc3) in a headless Logos Core on our public LEZ v0.4.0-rc3 network, both light-client sources through `beacon_api_module`, keeping our own Sepolia instance (`0x3147ce50…`, `Init` block 1,698, on the network's published program `0x816d1fef…`, made with `ferrata-day1 keygen --program`) current; `ferrata_attest_module`'s calls reading Ethereum through `eth_rpc_module` in the same Core; attest-monitor on the instance; one attestation of the Sepolia vault's slots 0 and 1 applied (block 1,831); every process sampled, each daemon's traffic counted; `monitor` and the relayer as Logos modules on the anvil fixture | 90 min: 15 jobs, 49 of 49 transactions applied, finalized slot 11,642,496 → 11,643,104 (14 headers, each confirmed by two providers), each transaction final 15.1–26.8 s after sent; restart `stop` 0.20 s, `start` 1.11 s; updater module 0.040 of a core and 148 MiB median after a start of 535.8 s (493 CPU s: its follower from the network's genesis), 22.1 MB/h from the network, 5.2 MB/h from each provider; attest module 0.0002 of a core, 58.7 MiB, `verify_checkpoint` 5.31 s; attest-monitor 0.0033 of a core, 22.2 MiB; Logos Core's daemon 0.0001 of a core, 58.2 MiB; CLI journey 119.15 s, 121.61 CPU s, 137.1 MiB; `ferrata-day1 send` of `Init` 557.86 s (517.65 CPU s); monitor module 0.33 CPU s and 66.4 MiB, relayer module 0.70 CPU s and 73.9 MiB in 1,278.5 s; 27 of 27 checks | build box (Modal), public test network (ours: LEZ v0.4.0-rc3 with its Logos Blockchain node), public Sepolia endpoints (reads only), local EVM (anvil) | none (the public job path; the CLI in dev mode) | `python3 -I modules/scripts/test_live_testnet.py`; `check.py` on `session/` | [`modules/results/live-testnet-2026-11-23/`](../../modules/results/live-testnet-2026-11-23/README.md) |

## Texts to apply

Paths are given from the repository's root (as links, from the file they go into).

**`docs/requirements-status.md`**

- RFP-021 U.3 → shown: "The updater module in a headless Logos Core on a public LEZ v0.4.0-rc3 network we run, both
  light-client sources through `beacon_api_module`: 15 jobs, 49 of 49 transactions applied, a restart and a clean
  stop; the execution endpoint in `eth_rpc_module`; the operator journey documented and recorded (operator guide §0,
  §3; attest-monitor watching the instance). Condition: Logos's light-client API of its Ethereum module, announced, is
  replaced by `beacon_api_module`." Evidence: 79, 81, 97, 99, 104, 118, 179, 193, next.
- RFP-021 P.6 → shown: "Every off-chain component measured: the updater as a binary (0.043–0.092 of a core) and as a
  Logos module (0.040 after its start, 148 MiB median), the proven path's prover, the attestation CLI (119 s, 137 MiB),
  its module (0.0002 of a core, 59 MiB), attest-monitor (0.0033 of a core, 22 MiB) and Logos Core itself; the updater
  bounds what it reads and serves." Evidence: 21, 123, 172, 200, next; `docs/performance.md` RFP-021 §P.6.
- RFP-022 P.6 (in part): replace "the importer as a daemon, `ferrata_attestor`, `monitor`'s LEZ traffic and the
  daemons as Logos modules not measured" with "the daemons as Logos modules measured (monitor 0.0003 of a core and
  66 MiB, relayer 0.0005 and 74 MiB, the binaries' traffic); the importer as a daemon, `ferrata_attestor` and
  `monitor`'s LEZ traffic not measured". Evidence: add next.

**`docs/traceability.md`**

- RFP-021 U.3, evidence column: after "(row 118)." add "On a public LEZ v0.4.0-rc3 network we run, the module in a
  headless Logos Core with both its light-client sources through `beacon_api_module` (the operator's providers added
  with `add_provider`) kept our own Sepolia instance current for 90 min: 15 jobs, 49 of 49 transactions applied, a
  restart through `stop`/`start`, `logosctl daemon stop` exit 0; the attestation module in the same Core read the
  execution chain through `eth_rpc_module`; attest-monitor watched the instance (row next,
  record `modules/results/live-testnet-2026-11-23/`)." In "Remains", drop "and the updater module on a
  sequencer with both its Ethereum sources through Logos Core's `beacon_api_module`".
- RFP-021 P.6, evidence column: replace "the attestation CLI and module, the §8 `monitor` and the daemons as Logos
  modules: not measured" with "the attestation CLI's journey 119.15 s, 121.61 CPU s, 137.1 MiB; `ferrata_attest_module`
  0.0002 of a core, 58.7 MiB (its calls 0.07–5.31 s); attest-monitor 0.0033 of a core, 22.2 MiB; the updater as a
  Logos module on a public LEZ network 0.040 of a core and 148 MiB median after a start of 535.8 s (row next)".
- RFP-022 P.6, evidence column: replace "Not measured: the importer as a daemon, `ferrata_attestor`, `monitor`'s LEZ
  traffic and the daemons as Logos modules." with "As Logos modules (row next): `ferrata_monitor_module` 0.33 CPU s,
  66.4 MiB, `ferrata_relayer_module` 0.70 CPU s, 73.9 MiB in 1,278.5 s, their traffic the binaries' to within 1%. Not
  measured: the importer as a daemon, `ferrata_attestor`, `monitor`'s LEZ traffic."

**`docs/performance.md`, RFP-021 §P.6**: add three rows to the table, and replace the "**Not measured:**" sentence:

| Component | CPU | Memory | Time, bandwidth | Source |
|---|---|---|---|---|
| **the updater as a Logos module** (`ferrata_updater_module` in a headless Logos Core, sequencer backend, both light-client sources through `beacon_api_module`), on a public LEZ v0.4.0-rc3 network we run, 90 min | **0.040 of one core** after its start (216.7 CPU s in 5,391.5 s, 15 jobs); its start, the follower from the network's genesis: 492.9 CPU s in 531.4 s | **148.1 MiB median, 270.9 MiB peak** (its host process with the executor it starts) | the network's RPC 22.1 MB an hour down, 1.5 up (it applies every block of the network); 5.2 MB an hour from each of the two light-client providers; each transaction final 15.1–26.8 s after sent | measured (`modules/results/live-testnet-2026-11-23/`) |
| `ferrata_attest_module` and the `ferrata-attest` CLI | the module 0.0002 of a core (its calls 0.07–5.31 s: `verify_checkpoint` 5.31 s, `fetch_log_proof` through `eth_rpc_module` 1.16 s); the CLI's journey 121.6 CPU s | the module's host process 58.7 MiB; the CLI 137.1 MiB peak | the CLI's journey 119.2 s (in-process chain, recorded data); `ferrata-day1`'s attestation 0.98 s to sign, 24.6 s to final | measured (same record) |
| attest-monitor (the §8 `monitor`), polling every 60 s; Logos Core itself | attest-monitor 0.0033 of a core (18.0 CPU s in 5,391.5 s); Logos Core's daemon 0.0001 | attest-monitor 22.2 MiB peak, 4.2 MiB median; Logos Core's daemon 58.2 MiB, each module host process 57–60 MiB, the Core with Logos's own five modules and these four 656 MiB median | attest-monitor 0.15 MB an hour from the network, 0.14 and 0.17 MB from its two providers | measured (same record) |

"**Not measured:**" sentence, replaced by: "Measured as a Logos module on a public LEZ network we run, with the CLI, the
attestation module and attest-monitor in the same run (above); the gates' figures are the binary's."

**`docs/performance.md`, RFP-022 §P.6**: add a row, and in **Gaps** drop "`monitor` and the relayer as Logos modules
under `logosctl daemon` (`ferrata_monitor_module`, `ferrata_relayer_module`) are not measured: the binaries are.":

| `monitor` and the relayer as Logos modules (`ferrata_monitor_module`, `ferrata_relayer_module` in one headless Logos Core), the same anvil run | monitor 0.33 CPU s in 1,278.5 s (0.0003 of a core); relayer 0.70 (0.0005) | 66.4 and 73.9 MiB (each host process; about 57 MiB of it the host's own) | JSON-RPC 86,279 / 254,039 B and 411,356 / 983,811 B, the binaries' to within 1% | measured (`modules/results/live-testnet-2026-11-23/`, `bench/perf/daemons-anvil-modules.sh`) |

**Operator guide §3.6** (after "…a restart mid-job through `stop` and `start` (EVIDENCE row 104, …)."): "On our public
LEZ v0.4.0-rc3 network it ran with its Ethereum data through Logos Core: `beacon_api_module.init_defaults`, then the
operator's providers with `add_provider` (`{"endpoint": URL, "id": NAME}`), and in the updater's file one source per
provider with `via = "beacon_api_module"`, `chain_id` and `provider = NAME` (`min_agreeing = 2`); the execution
endpoint for the attestation module with `eth_rpc_module.set_chain_config`. `start` answers once its set-up is done,
which on a new state directory means following the network from its genesis (about 9 minutes there, 1,700 requests):
give the call a long timeout, and keep the state directory. Pass `"quiet": false` to `start` for the daemon's own job
lines in Logos Core's log (record: `modules/results/live-testnet-2026-11-23/`)." And in §9 add the three P.6 rows
above and drop "**Not measured:** the daemons as Logos modules … and the `ferrata-attest` CLI and its module".

**README, "Specified but not built"**: in **Daemons**, drop "the attestation side's monitor against public providers
(built on recorded answers: `monitor/attest/`)" (attest-monitor ran against Lodestar and publicnode on a public network:
row next; the network's host runs it too).

Box runs: the Modal build box.
