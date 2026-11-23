# Rows: modules-live (the updater module on a LEZ network we run; the off-chain components' resources)

For the integrator. Nothing here edits `docs/EVIDENCE.md`, `docs/requirements-status.md`, `docs/traceability.md`,
`docs/performance.md`, `docs/sepolia.md`, the operator guide or the README's status lines: the texts to apply are
below. The record: [`modules/results/live-testnet-2026-11-23/`](../../modules/results/live-testnet-2026-11-23/README.md)
(49 of 49 checks in its `summary.md`, written by `modules/scripts/live_testnet.py` from the record's files). Re-check,
offline: `python3 -I modules/scripts/test_live_testnet.py` (every figure and check computed again from the record;
`check.py` on both sessions' journals; no machine path).

The network: a LEZ v0.4.0-rc3 network we run, reachable publicly (not Logos's: LEZ testnet 0.4 does not exist; its
address is not published here). The header of `docs/requirements-status.md` accepts "one of their sequencers" (LEZ v0.4
RC's own `sequencer_service`, here rc3); RFP-021 U.3 and P.6 do not name testnet 0.4.

## Proposed status changes

| Row | Now | Proposed | Why |
|---|---|---|---|
| RFP-021 U.3 | in part | **shown** | Each clause, as the RFP words it, in the record (README, "RFP-021 U.3, clause by clause"): `ferrata_updater_module` in a headless Logos Core (`logosctl daemon`) followed Sepolia for our own instance on the network, its light-client data through `beacon_api_module` (two sources, each its own provider), and, since the review, **an execution endpoint through `eth_rpc_module`** as a source of the updater itself (`kind = "execution"`, `via = "eth_rpc_module"`): with three sources required to agree, no update was sent before the execution endpoint held its block canonical (8 jobs, each job line naming `eth_rpc_module-sepolia` among its confirmers; 16 execution requests through the module). It detected the instance behind and submitted the updates (session 1: 15 jobs, 49 of 49 transactions; session 2: 8 jobs, 24 of 24 transactions), reached the network both through a relay and directly as a remote sequencer (`FERRATA_SEQUENCER_REMOTE`, `FERRATA_TESTNET_SEND=yes`: session 2), restarted through `stop`/`start` from its saved follower, stopped cleanly (`logosctl daemon stop` exit 0); `/health`, `/metrics`, the journal and its log lines structured; attest-monitor watched the instance (88 polls). The operator journey: operator guide §0, §3 with the addition below (both runs' commands in `modules/scripts/live-testnet.sh`). Condition (unchanged): Logos's announced light-client API of its Ethereum module does not exist; `beacon_api_module` is used until it does. |
| RFP-021 P.6 | in part | **shown** | What the row lacked, measured on the network: `ferrata attest` (cli/, release, the CLI RFP-021 U.2 is shown on), its first command from a new state (it follows the network from genesis) 1,259.58 s, 827.64 CPU s, 137.1 MiB peak, the same command again 44.06 s and 30.87 CPU s, `config` 1.28 s, an attestation dry run 28.47 s (19.59 CPU s, 133.1 MiB) and sent 23.22 s (applied, equal to its dry run), a finality update as a dry run 1.63 s and sent 70.01 s (its three steps applied); `ferrata_attest_module` (0.0002 of a core, 58.7 MiB; its calls 0.07–5.31 s); attest-monitor (0.0033 of a core, 22.2 MiB peak); the updater as a Logos module (0.040 of a core after its start, 148.1 MiB median, 270.9 MiB peak; its start from the network's genesis 492.9 CPU s in 531.4 s; 22.1 MB/h from the network, 5.2 MB/h from each light-client provider; session 2 with its execution source : 0.031 of a core after its start, 165.1 MiB median, 296.3 MiB peak); Logos Core itself (its daemon 0.0001 of a core and 58 MiB; each module host process 57–60 MiB resident, shared pages counted in each). With the updater daemon (EVIDENCE row 172) and the proven path's prover (row 123), every off-chain component of the RFP-021 design is measured. The documentation to apply: `docs/performance.md` RFP-021 §P.6 (below). |
| RFP-022 P.6 | in part | in part (narrower) | Measured now: the daemons as Logos modules: `ferrata_monitor_module` 0.33 CPU s and 66.4 MiB, `ferrata_relayer_module` 0.70 CPU s and 73.9 MiB in 1,278.5 s on `daemons-anvil.sh`'s fixture, the binaries' traffic to within 1% and no more CPU; the updater module whose records the root imports need (above). Not measured: the importer as a daemon (the network's bridge B1 has no vault configured: the vault's Sepolia step is the owner's), `ferrata_attestor` (it reads its Logos Blockchain node's API, which the network does not expose), `monitor`'s traffic to a LEZ node (it has no LEZ client: RFP-022 U.3). |

## Proposed EVIDENCE row (the next number)

| # | What it shows | Numbers | Where it ran | Proofs | Re-check | Details |
|---|---|---|---|---|---|---|
| next | **The updater module on a LEZ v0.4.0-rc3 network we run, reachable publicly, with its Ethereum data through Logos Core (RFP-021 U.3), and the off-chain components' resources (RFP-021 P.6, RFP-022 P.6)**: `ferrata_updater_module` (sequencer backend, built for v0.4.0-rc3) in a headless Logos Core kept our own Sepolia instance (`0x3147ce50…`, `Init` block 1,698, on the network's published program `0x816d1fef…`, made with `ferrata-day1 keygen --program`) current: session 1 its light-client sources through `beacon_api_module`, reaching the network through a counting relay; session 2 reaching it directly as a remote sequencer, with an execution source through `eth_rpc_module` (`kind = "execution"`) and three sources to agree; `ferrata_attest_module`'s calls reading Ethereum through `eth_rpc_module`; attest-monitor on the instance; `ferrata attest` (release, v0.4.0-rc3) on the network; `monitor` and the relayer as Logos modules on the anvil fixture | session 1, 90 min: 15 jobs, 49 of 49 transactions applied, finalized slot 11,642,496 → 11,643,104 (each header confirmed by two providers), a restart; the module 0.040 of a core after a start of 535.8 s (492.9 CPU s: its follower from genesis), 148.1 MiB median, 22.1 MB/h from the network; session 2, 46 min: 8 jobs, 24 of 24 transactions, each update confirmed by the execution endpoint before it was sent (16 requests through `eth_rpc_module`), the module 0.031 of a core after its start; `ferrata attest`: `status` from genesis 1,259.6 s, 827.6 CPU s, 137.1 MiB; again 44.1 s; an attestation sent and applied in 23.2 s (dry run 28.5 s), a finality update sent and applied in 70.0 s; attest module 0.0002 of a core, 58.7 MiB; attest-monitor 0.0033 of a core, 22.2 MiB; monitor module 0.33 CPU s and 66.4 MiB, relayer module 0.70 CPU s and 73.9 MiB in 1,278.5 s; 49 of 49 checks | build box (Modal); a LEZ v0.4.0-rc3 network we run (its own Logos Blockchain node; reachable publicly); public Sepolia endpoints (reads only); local EVM (anvil) | none (the public job path) | `python3 -I modules/scripts/test_live_testnet.py`; `check.py` on `session2/` (all pass) and on `session/` (one check needs log lines the first start did not print: README) | [`modules/results/live-testnet-2026-11-23/`](../../modules/results/live-testnet-2026-11-23/README.md) |

## Texts to apply

Paths are given from the repository's root (as links, from the file they go into).

**No public LEZ network runs our v0.4 code: five statements on main** (review M2). Each distinguishes a network
Logos runs (none for v0.4) from the LEZ v0.4.0-rc3 network we run, reachable publicly:

- `docs/requirements-status.md` (header): "Every sequencer named here is one we ran; none is a public LEZ network." →
  "Every sequencer named here is one we ran; none is a LEZ network Logos runs (LEZ testnet 0.4 does not exist). One of
  ours is reachable publicly: a LEZ v0.4.0-rc3 network with its own Logos Blockchain node (its address is not
  published here), named where it is used."
- `docs/guides/operator-guide.md` (Status): "None has run on a public LEZ network (none exists for v0.4); each has run
  on zones we ran, on anvil and on forks of Sepolia, as §0 lists." → "None has run on a LEZ network Logos runs (none
  exists for v0.4); each has run on zones we ran, on anvil and on forks of Sepolia, as §0 lists, and the updater, as
  a binary and as a Logos module, on a LEZ v0.4.0-rc3 network we run that is reachable publicly
  (`modules/results/live-testnet-2026-11-23/`)."
- `README.md` (the live round trip): "The LEZ side was a devnet we ran (one validator): no public LEZ network runs any
  of it." → "The LEZ side was a devnet we ran (one validator). No LEZ network Logos runs carries any of it; the
  attestation program and the bridge program at B1 also run on a LEZ v0.4.0-rc3 network we run that is reachable
  publicly (EVIDENCE row next)."
- `README.md` ("Not audited"): "On a public LEZ network only the light client's experimental v0.3.1 port is deployed,
  on the public LEZ testnet (row 150); none of the proposal's v0.4 programs is: LEZ testnet 0.4 does not exist." → "On
  a LEZ network Logos runs only the light client's experimental v0.3.1 port is deployed, on the public LEZ testnet
  (row 150); none of the proposal's v0.4 programs is: LEZ testnet 0.4 does not exist. They run on a LEZ v0.4.0-rc3
  network we run that is reachable publicly (row next)."
- `docs/EVIDENCE.md` (introduction) and `docs/sepolia.md` (crown-live-1): "On a public LEZ network only the light
  client's experimental v0.3.1 port is deployed (row 150); none of the proposal's v0.4 programs is: the round trip's
  LEZ side is a devnet we ran (LEZ testnet 0.4 does not exist)." and "no public LEZ network runs any of it" → the same
  distinction: "on a LEZ network Logos runs …; the proposal's v0.4 programs run on a LEZ v0.4.0-rc3 network we run that
  is reachable publicly (row next)" and "no LEZ network Logos runs carries any of it".

**`docs/requirements-status.md`**

- RFP-021 U.3 → shown: "The updater module in a headless Logos Core on a LEZ v0.4.0-rc3 network we run, reachable
  publicly: its light-client data through `beacon_api_module`, an execution endpoint through `eth_rpc_module`
  confirming each update's block before it is sent; 23 jobs, every transaction applied; directly as a remote
  sequencer and through a relay; a restart and a clean stop; the operator journey documented (operator guide §0, §3)
  and recorded, attest-monitor watching the instance. Condition: Logos's announced light-client API of its Ethereum
  module, replaced by `beacon_api_module`." Evidence: 79, 81, 97, 99, 104, 118, 179, 193, next.
- RFP-021 P.6 → shown: "Every off-chain component measured: the updater as a binary (0.043–0.092 of a core) and as a
  Logos module (0.040 after its start, 148 MiB median), the proven path's prover, `ferrata attest` on a network
  (its first command, from the network's genesis, 1,260 s and 137 MiB; then 1–70 s a command), its module (0.0002 of a core, 59 MiB), attest-monitor (0.0033 of a core, 22 MiB) and Logos Core
  itself; the updater bounds what it reads and serves." Evidence: 21, 123, 172, 200, next; `docs/performance.md`
  RFP-021 §P.6.
- RFP-022 P.6 (in part): replace "the importer as a daemon, `ferrata_attestor`, `monitor`'s LEZ traffic and the
  daemons as Logos modules not measured" with "the daemons as Logos modules measured (monitor 0.33 CPU s and 66 MiB,
  relayer 0.70 and 74 MiB in 1,278.5 s, the binaries' traffic); the importer as a daemon, `ferrata_attestor` and
  `monitor`'s LEZ traffic not measured". Evidence: add next.

**`docs/traceability.md`**

- RFP-021 U.3, evidence: after "(row 118)." add "On a LEZ v0.4.0-rc3 network we run, reachable publicly, the module in
  a headless Logos Core with its light-client sources through `beacon_api_module` (the operator's providers, added
  with `add_provider`) kept our own Sepolia instance current: 90 min through a counting relay (15 jobs, 49 of 49
  transactions, a restart through `stop`/`start`), then 46 min reaching the network itself as a remote sequencer
  with an execution source through `eth_rpc_module` (`kind = "execution"`), three sources to agree (8 jobs, each
  confirmed by the execution endpoint before it was sent); `logosctl daemon stop` exit 0 both times; attest-monitor
  watched the instance (row next, record `modules/results/live-testnet-2026-11-23/`)." In "Remains", drop "and the
  updater module on a sequencer with both its Ethereum sources through Logos Core's `beacon_api_module`"; and move
  "the gate's three daemons on a sequencer at 100 updates" to R.9 and R.10's "Remains" (it is the A3 gate those rows
  rest on, not a clause of U.3; review L9).
- RFP-021 U.3, plan column: after "Light-client data comes through `beacon_api_module.lc_request` until the API
  announced for the Ethereum module exists." add "An execution endpoint (a JSON-RPC URL, or `eth_rpc_module` in Logos
  Core) can confirm each update's execution block before it is sent (`kind = "execution"`)."
- RFP-021 P.6, evidence: replace "the attestation CLI and module, the §8 `monitor` and the daemons as Logos modules:
  not measured" with "`ferrata attest` (release) on a network: `status` from a new state (it follows the network from genesis) 1,259.58 s, 827.64 CPU s, 137.1 MiB; again 44.06 s; `config` 1.28 s; an attestation 28.47 s as a dry run and 23.22 s sent; a finality update 1.63 s and 70.01 s; `ferrata_attest_module` 0.0002 of a core,
  58.7 MiB (its calls 0.07–5.31 s); attest-monitor 0.0033 of a core, 22.2 MiB; the updater as a Logos module 0.040 of a
  core and 148 MiB median after a start of 535.8 s (row next)".
- RFP-022 P.6, evidence: replace "Not measured: the importer as a daemon, `ferrata_attestor`, `monitor`'s LEZ traffic
  and the daemons as Logos modules." with "As Logos modules (row next): `ferrata_monitor_module` 0.33 CPU s, 66.4 MiB,
  `ferrata_relayer_module` 0.70 CPU s, 73.9 MiB in 1,278.5 s, their traffic the binaries' to within 1%. Not measured:
  the importer as a daemon, `ferrata_attestor`, `monitor`'s LEZ traffic."

**`docs/performance.md`, RFP-021 §P.6**: add these rows, and replace the "**Not measured:**" sentence with "Measured as
a Logos module on a LEZ v0.4.0-rc3 network we run, with `ferrata attest`, the attestation module and attest-monitor on
that network (above); the gates' figures are the binary's.":

| Component | CPU | Memory | Time, bandwidth | Source |
|---|---|---|---|---|
| **the updater as a Logos module** (`ferrata_updater_module` in a headless Logos Core, sequencer backend, its light-client sources through `beacon_api_module`), on a LEZ v0.4.0-rc3 network we run, 90 min | **0.040 of one core** after its start (216.7 CPU s in 5,391.5 s, 15 jobs); its start, the follower from the network's genesis: 492.9 CPU s in 531.4 s | **148.1 MiB median, 270.9 MiB peak** (its host process with the executor it starts) | the network's RPC 22.1 MB an hour down, 1.5 up (it applies every block of the network), 5.2 MB an hour from each of the two light-client providers (uncompressed, TLS not counted); each transaction final 15.1–26.8 s after sent | measured (`modules/results/live-testnet-2026-11-23/`, session 1) |
| the same with an execution source through `eth_rpc_module`, reaching the network directly, 46 min | 0.031 of one core after its start (84.1 CPU s in 2,734.9 s, 8 jobs); its start from session 1's state 345.6 CPU s in 525.9 s; 165.1 MiB median, 296.3 MiB peak | | the execution checks through `eth_rpc_module`: 16 requests, 3,512 B up, 225,915 B down | measured (same record, session 2) |
| `ferrata attest` (cli/, release), on the network | `status` from a new state (following the network from genesis, about 4,190 blocks) 1,259.6 s wall, 827.6 CPU s, 137.1 MiB peak; with its state kept 44.1 s; `config` 1.3 s; an attestation 28.5 s as a dry run, 23.2 s sent (until final); a finality update 1.6 s as a dry run, 70.0 s sent (three steps) | | | measured (same record, session 2) |
| `ferrata_attest_module` | 0.0002 of a core (its calls 0.07–5.31 s: `verify_checkpoint` 5.31 s, `fetch_log_proof` through `eth_rpc_module` 1.16 s) | its host process 58.7 MiB resident (shared pages counted) | | measured (session 1) |
| attest-monitor (the §8 `monitor`), polling every 60 s; Logos Core itself | attest-monitor 0.0033 of a core (18.0 CPU s in 5,391.5 s); Logos Core's daemon 0.0001 | attest-monitor 22.2 MiB peak, 4.2 MiB median; Logos Core's daemon 58.2 MiB; each module host process 57–60 MiB resident, shared pages counted in each | attest-monitor 0.15 MB an hour from the network, 0.14 and 0.17 MB from its two providers | measured (session 1) |

**`docs/performance.md`, RFP-022 §P.6**: add a row, and in **Gaps** drop "`monitor` and the relayer as Logos modules
under `logosctl daemon` (`ferrata_monitor_module`, `ferrata_relayer_module`) are not measured: the binaries are.":

| `monitor` and the relayer as Logos modules (`ferrata_monitor_module`, `ferrata_relayer_module` in one headless Logos Core), the same anvil run | monitor 0.33 CPU s in 1,278.5 s (0.0003 of a core); relayer 0.70 (0.0005) | 66.4 and 73.9 MiB resident (each host process, shared pages counted; about 57 MiB of it a host process's own) | JSON-RPC 86,279 / 254,039 B and 411,356 / 983,811 B, the binaries' to within 1% | measured (`modules/results/live-testnet-2026-11-23/`, `bench/perf/daemons-anvil-modules.sh`) |

**Operator guide §3.6**, after "…a restart mid-job through `stop` and `start` (EVIDENCE row 104, …).": "With its
Ethereum data through Logos Core: `logosctl call beacon_api_module init_defaults`, then the operator's providers,
`logosctl call beacon_api_module add_provider 11155111 '{"endpoint":"https://…","id":"NAME"}'`, and in the updater's
file one source per provider with `via = "beacon_api_module"`, `chain_id = 11155111` and `provider = "NAME"` (an answer
from another provider, after a failover, is not taken). For an execution endpoint: `logosctl call eth_rpc_module
set_chain_config 11155111 '{"endpoint":"https://…","name":"sepolia","testnet":true,"timeoutSecs":60}'` and a source
with `kind = "execution"`, `via = "eth_rpc_module"`, `chain_id = 11155111`; with `min_agreeing` equal to the number of
sources, no update is sent before the endpoint holds its block canonical. For a sequencer not on this machine, start
Logos Core with `FERRATA_SEQUENCER_REMOTE` set to the `[sequencer] rpc` URL exactly and `FERRATA_TESTNET_SEND=yes` in
its environment (the module refuses another address). `start` answers once its set-up is done, which on a new state
directory means following the network from its genesis (about 9 minutes and 1,700 requests on our network on
23 November): give the call a long timeout and keep the state directory. Pass `"quiet": false` to `start` for the
daemon's own job lines in Logos Core's log (record: `modules/results/live-testnet-2026-11-23/`, both sessions)." In §9,
add the rows above and drop "**Not measured:** the daemons as Logos modules … and the `ferrata-attest` CLI and its
module".

**README, "Specified but not built"**: in **Daemons**, drop "the attestation side's monitor against public providers
(built on recorded answers: `monitor/attest/`)" (attest-monitor ran against Lodestar and publicnode on the network we
run: row next).

**CI**: add `python3 -I modules/scripts/test_live_testnet.py` to the job that runs `scripts/test_perf_records.py`
(offline, standard library only, under a second). The rc3 lockfiles (`modules/ferrata_updater_module/rust-lib/lockfiles/
Cargo.lock.v0.4.0-rc3`, `cli/lockfiles/Cargo.lock.v0.4.0-rc3`) are named so that `scripts/deny_all.py` (which takes
every `*Cargo.lock` for a workspace) skips them; cargo-deny therefore does not scan the rc3 graphs (review L6): a
follow-up could scan each in a copy of its workspace, as the build swaps it in.

**The code changes' tests on the build box**: `ferrata-updater`'s `config` 9 of 9, `execution` 2 of 2, `bounds` 3 of 3
(a source naming its provider refuses another provider's answer), `recorded`'s `execution_source_confirms_or_holds_back`
and `headers_source_confirms_or_holds_back` (an honest endpoint: 9 confirmations, 10 jobs applied; a lying one: 27
rounds waiting, health degraded, nothing refused); `attest-monitor`'s suite 32 of 32; `lez/updater/sequencer`'s library
tests 4 of 4 at v0.4.0-rc1 and at v0.4.0-rc3 (`day1::tests::a_program_known_by_its_account_has_no_keys_to_sign_with`
among them); `ferrata attest` built at v0.4.0-rc3 with `--features lez-rc3` (the one wallet call whose API differs).

Box runs: the Modal build box.
