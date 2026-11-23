# live-testnet-2026-11-23: figures and checks (written by `modules/scripts/live_testnet.py summary`)

## Checks

- PASS ferrata_updater_module loaded in the headless Logos Core (session/rep/modules-loaded.json)
- PASS beacon_api_module loaded in the headless Logos Core (session/rep/modules-loaded.json)
- PASS eth_rpc_module loaded in the headless Logos Core (session/rep/modules-loaded.json)
- PASS ferrata_attest_module loaded in the headless Logos Core (session/rep/modules-loaded.json)
- PASS the updater module carries the sequencer backend, built on LEZ v0.4.0-rc3 (U-version)
- PASS every light-client source of the updater goes through beacon_api_module (updater.toml), two required to agree
- PASS beacon_api_module carried the updater's light-client requests: 258 to Lodestar, 256 to Nimbus (the relays' counts)
- PASS the module applied 49 of 49 transactions on the network, 15 jobs finished
- PASS every finalized header the module's jobs moved the instance to (14) was confirmed canonical by attest-monitor's two providers (session/monitor/seen.json: 14 confirmed)
- PASS /health answered ok to 87 of 88 probes (the module's own server)
- PASS status through Logos Core: running in 88 of 88 calls
- PASS the module's stop, start (a restart halfway) and the final stop answered (rep/U-stop-1, U-start-2, U-stop-final)
- PASS after the restart the module went on: 21 transactions applied
- PASS Logos Core stopped cleanly (logosctl daemon stop exit 0)
- PASS the instance is active and moved from its checkpoint (slot 11642496) to slot 11643104
- PASS an attestation of the Sepolia vault's slots 0 and 1 under the instance's record, applied on the network (attest-send.log)
- PASS ferrata_attest_module read_instance answered (rep/A-read_instance.json)
- PASS ferrata_attest_module verify_checkpoint answered (rep/A-verify_checkpoint.json)
- PASS ferrata_attest_module fetch_log_proof answered (rep/A-fetch_log_proof.json)
- PASS ferrata_attest_module dry_run_storage answered (rep/A-dry_run_storage.json)
- PASS ferrata_attest_module dry_run_storage_eth answered (rep/A-dry_run_storage_eth.json)
- PASS RFP-022: the relayer module finalised the honest candidate (phases.tsv: final state = wanted)
- PASS RFP-022: monitor-start answered through Logos Core, no error (daemons-anvil-modules/rep/monitor-start.json: verdict started)
- PASS RFP-022: relayer-start answered through Logos Core, no error (daemons-anvil-modules/rep/relayer-start.json: verdict started)
- PASS RFP-022: monitor-stop answered through Logos Core, no error (daemons-anvil-modules/rep/monitor-stop.json: verdict ok)
- PASS RFP-022: relayer-stop answered through Logos Core, no error (daemons-anvil-modules/rep/relayer-stop.json: verdict ok)
- PASS RFP-022: that Logos Core stopped cleanly
- PASS session 2: ferrata_updater_module, beacon_api_module and eth_rpc_module loaded in the headless Logos Core
- PASS session 2: the module reaches the network's RPC itself (a sequencer not on this machine, by its https URL), two light-client sources through beacon_api_module and an execution source through eth_rpc_module, three to agree
- PASS session 2: check_config accepted that file (rep/U-check_config.json)
- PASS session 2: the module applied 24 of 24 transactions on the network, 8 jobs finished
- PASS session 2: every finished job's update (8 of 8) was confirmed by the execution source before it was sent (the daemon's job line: served or confirmed by …, eth_rpc_module-sepolia among them)
- PASS session 2: eth_rpc_module carried the execution checks to the endpoint (8 eth_getBlockByHash, 8 eth_getBlockByNumber: the relay's counts)
- PASS session 2: /health ok to 44 of 45 probes
- PASS session 2: start, stop, start again (halfway) and the final stop answered through Logos Core
- PASS session 2: Logos Core stopped cleanly
- PASS session 2: ferrata attest cli/status-first exit 0 (session2/cli/status-first.*)
- PASS session 2: ferrata attest cli/status-again exit 0 (session2/cli/status-again.*)
- PASS session 2: ferrata attest cli/config exit 0 (session2/cli/config.*)
- PASS session 2: ferrata attest cli2/attest-dry exit 0 (session2/cli2/attest-dry.*)
- PASS session 2: ferrata attest cli2/attest-send exit 0 (session2/cli2/attest-send.*)
- PASS session 2: ferrata attest cli2/advance-dry exit 0 (session2/cli2/advance-dry.*)
- PASS session 2: ferrata attest cli2/advance-send exit 0 (session2/cli2/advance-send.*)
- PASS session 2: ferrata attest cli2/status-after exit 0 (session2/cli2/status-after.*)
- PASS session 2: ferrata attest cli/attest-dry refused with 215 HEADER_REQUIRED, as its dry run said (exit 3)
- PASS session 2: ferrata attest cli/attest-send refused with 215 HEADER_REQUIRED, as its dry run said (exit 3)
- PASS session 2: ferrata attest cli/advance-dry refused with 52 NotNewer, as its dry run said (exit 3)
- PASS session 2: ferrata attest sent the attestation (with the block's header) and the network applied it
- PASS session 2: ferrata attest submitted a finality update (its steps applied on the network)

Checks failed: 0

## Components (measured: process samples every 5 s)

| component | wall s | CPU s | CPU share of one core | peak RSS MiB | median RSS MiB | processes (max) |
|---|---:|---:|---:|---:|---:|---:|
| updater-module | 5927.9 | 714.39 | 0.1205 | 270.9 | 148.1 | 2 |
| beacon_api_module | 5927.9 | 2.24 | 0.0004 | 59.6 | 59.5 | 1 |
| eth_rpc_module | 5927.9 | 0.67 | 0.0001 | 57.7 | 57.7 | 1 |
| attest-module | 5927.9 | 1.32 | 0.0002 | 58.7 | 58.7 | 1 |
| attest-monitor | 5391.5 | 17.99 | 0.0033 | 22.2 | 4.2 | 2 |
| Logos Core daemon (logosctl, its own process) | 5927.9 | 0.47 | 0.0001 | 58.2 | 58.2 | 1 |
| Logos Core with every module host process (its tree) | 5927.9 | 723.1 | 0.122 | 776.1 | 655.6 | 11 |
| measurement: one counting relay (lez-updater) | 5927.9 | 11.28 | 0.0019 | 41.4 | 41.4 | 1 |
| updater-module (start: follower from genesis) | 531.4 | 492.87 | 0.9275 | 270.9 | 244.0 | 2 |
| updater-module (after the start) | 5391.5 | 216.72 | 0.0402 | 268.4 | 148.1 | 2 |
| attest-module (its calls) | 60.1 | 0.89 | 0.0148 | 58.7 | 54.7 | 1 |
| eth_rpc_module (its calls) | 60.1 | 0.19 | 0.0032 | 57.7 | 56.3 | 1 |

## One-shot tools (measured: GNU time)

| tool | wall s | CPU s | peak RSS MiB |
|---|---:|---:|---:|
| ferrata-day1 send (Init) | 557.86 | 517.65 | 137.1 |
| ferrata-day1 sign (Attest) | 0.98 | 0.9 | 114.3 |
| ferrata-day1 send (Attest) | 24.58 | 1.19 | 116.8 |
| ferrata-attest CLI journey | 119.15 | 121.61 | 137.1 |

## Traffic (measured: each daemon's own relay, HTTP level, TLS not counted)

| relay | requests | errors | up | down | seconds |
|---|---:|---:|---:|---:|---:|
| lez-updater | 10782 | 0 | 2.47 MiB | 39.27 MiB | 5955.9 |
| lodestar-updater | 258 | 0 | 0.03 MiB | 7.49 MiB | 5955.7 |
| nimbus-updater | 256 | 0 | 0.03 MiB | 7.32 MiB | 5956.0 |
| lez-monitor | 89 | 0 | 0.02 MiB | 0.21 MiB | 5955.7 |
| lodestar-monitor | 264 | 0 | 0.03 MiB | 0.20 MiB | 5955.7 |
| publicnode-monitor | 264 | 0 | 0.03 MiB | 0.24 MiB | 5955.7 |

After the updater's start (the relays' series; per hour *derived*):

| relay | seconds | requests | up | down | MB per hour up / down |
|---|---:|---:|---:|---:|---:|
| lez-updater | 5370.2 | 8952 | 2.12 MiB | 31.47 MiB | 1.49 / 22.12 |
| lodestar-updater | 5370.2 | 257 | 0.03 MiB | 7.44 MiB | 0.02 / 5.23 |
| nimbus-updater | 5370.2 | 256 | 0.03 MiB | 7.32 MiB | 0.02 / 5.15 |
| lez-monitor | 5370.2 | 88 | 0.02 MiB | 0.21 MiB | 0.01 / 0.15 |
| lodestar-monitor | 5370.2 | 261 | 0.03 MiB | 0.19 MiB | 0.02 / 0.14 |
| publicnode-monitor | 5370.2 | 261 | 0.03 MiB | 0.24 MiB | 0.02 / 0.17 |

## Module calls

| call | module | method | exit | seconds |
|---|---|---|---:|---:|
| U-version | ferrata_updater_module | version | 0 | 0.23 |
| A-version | ferrata_attest_module | version | 0 | 0.09 |
| bam-init_defaults | beacon_api_module | init_defaults | 0 | 0.08 |
| bam-add-lodestar | beacon_api_module | add_provider | 0 | 0.09 |
| bam-add-nimbus | beacon_api_module | add_provider | 0 | 0.09 |
| bam-providers | beacon_api_module | list_providers | 0 | 0.09 |
| rpc-scope | eth_rpc_module | set_network_scope | 0 | 0.1 |
| rpc-chain | eth_rpc_module | set_chain_config | 0 | 0.1 |
| rpc-mode | eth_rpc_module | set_verified_proxy_mode | 0 | 0.09 |
| rpc-chain-config | eth_rpc_module | get_chain_config | 0 | 0.08 |
| U-check_config | ferrata_updater_module | check_config | 0 | 0.1 |
| U-start-1 | ferrata_updater_module | start | 0 | 535.73 |
| A-read_instance | ferrata_attest_module | read_instance | 0 | 0.08 |
| A-verify_checkpoint | ferrata_attest_module | verify_checkpoint | 0 | 5.31 |
| A-fetch_log_proof | ferrata_attest_module | fetch_log_proof | 0 | 1.16 |
| A-dry_run_storage | ferrata_attest_module | dry_run_attest | 0 | 0.07 |
| A-dry_run_storage_eth | ferrata_attest_module | dry_run_attest | 0 | 0.39 |
| A-dry_run_log | ferrata_attest_module | dry_run_attest | 0 | 0.09 |
| A-build_attest_tx | ferrata_attest_module | build_attest_tx | 0 | 0.08 |
| A-build_attest_tx-hex | ferrata_attest_module | build_attest_tx | 0 | 0.08 |
| U-stop-1 | ferrata_updater_module | stop | 0 | 0.2 |
| U-start-2 | ferrata_updater_module | start | 0 | 1.11 |
| U-metrics-final | ferrata_updater_module | metrics | 0 | 0.08 |
| U-status-final | ferrata_updater_module | status | 0 | 0.07 |
| U-stop-final | ferrata_updater_module | stop | 0 | 0.13 |

## Journal

```
{
 "transactions": 49,
 "applied": 49,
 "results": {
  "applied": 49
 },
 "jobs_finished": 15,
 "jobs_with_adoption": 2,
 "fees": 1278889920,
 "gas_exec": 159563281,
 "blocks": [
  1779,
  2302
 ],
 "finalized_slots": [
  11640832,
  11642496,
  11642688,
  11642720,
  11642752,
  11642784,
  11642816,
  11642848,
  11642880,
  11642912,
  11642944,
  11642976,
  11643008,
  11643040,
  11643072,
  11643104
 ]
}
```

## Session 2: the module reaching the network directly, an execution source; `ferrata attest`

| component | wall s | CPU s | CPU share of one core | peak RSS MiB | median RSS MiB |
|---|---:|---:|---:|---:|---:|
| updater-module | 3265.8 | 432.76 | 0.1325 | 296.3 | 165.1 |
| beacon_api_module | 3265.8 | 1.28 | 0.0004 | 59.0 | 58.9 |
| eth_rpc_module | 3265.8 | 0.73 | 0.0002 | 56.8 | 56.8 |
| Logos Core daemon (its own process) | 3265.8 | 0.3 | 0.0001 | 57.8 | 57.6 |

| `ferrata attest` (release) | exit | wall s | CPU s | peak RSS MiB |
|---|---:|---:|---:|---:|
| cli/status-first | 0 | 1259.58 | 827.64 | 137.1 |
| cli/status-again | 0 | 44.06 | 30.87 | 136.8 |
| cli/config | 0 | 1.28 | 0.59 | 114.3 |
| cli/attest-dry | 3 | 30.92 | 21.26 | 136.6 |
| cli/attest-send | 3 | 18.41 | 1.38 | 116.7 |
| cli/advance-dry | 3 | 0.91 | 0.35 | 116.9 |
| cli2/attest-dry | 0 | 28.47 | 19.59 | 133.1 |
| cli2/attest-send | 0 | 23.22 | 1.32 | 116.8 |
| cli2/advance-dry | 0 | 1.63 | 1.01 | 133.0 |
| cli2/advance-send | 0 | 70.01 | 4.79 | 133.0 |
| cli2/status-after | 0 | 0.72 | 0.08 | 84.4 |

```
{
 "journal": {
  "transactions": 24,
  "applied": 24,
  "jobs_finished": 8,
  "jobs_confirmed_by_the_execution_source": 8,
  "blocks": [
   4193,
   4454
  ],
  "finalized_slots": [
   11644672,
   11644704,
   11644736,
   11644768,
   11644800,
   11644832,
   11644864,
   11644896
  ]
 },
 "execution_traffic": {
  "requests": 16,
  "up": 3512,
  "down": 225915,
  "by": {
   "eth_getBlockByHash": 8,
   "eth_getBlockByNumber": 8
  }
 },
 "execution_source_confirmations": 3,
 "instance_final": {
  "active": true,
  "applied": 27,
  "finalized_slot": 11644896,
  "finalized_root": "0x456751647fa3e13463addfa0883bd74d3063b9e33daa8246e87654bee9603542"
 },
 "updater_config": {
  "rpc": "https://<domain>/rpc",
  "min_agreeing": 3,
  "sources": [
   {
    "name": "beacon_api_module-lodestar",
    "kind": null,
    "via": "beacon_api_module",
    "provider": "lodestar-counted",
    "chain_id": 11155111
   },
   {
    "name": "beacon_api_module-nimbus",
    "kind": null,
    "via": "beacon_api_module",
    "provider": "nimbus-counted",
    "chain_id": 11155111
   },
   {
    "name": "eth_rpc_module-sepolia",
    "kind": "execution",
    "via": "eth_rpc_module",
    "provider": null,
    "chain_id": 11155111
   }
  ]
 }
}
```

## RFP-022: monitor and relayer as Logos modules (anvil fixture)

Phases: idle-1 start at +0 s; activity start at +600 s; activity end (final state 0x89a39679a46c1f7b…; wanted 0x89a39679a46c1f7b…) at +682 s; idle-2 end at +1283 s.

| process | wall s | CPU s | CPU share of one core | peak RSS MiB | median RSS MiB | JSON-RPC bytes up / down |
|---|---:|---:|---:|---:|---:|---:|
| monitor-module | 1278.5 | 0.33 | 0.0003 | 66.4 | 66.4 | 86279 / 254039 |
| relayer-module | 1278.5 | 0.7 | 0.0005 | 73.9 | 73.9 | 411356 / 983811 |
| Logos Core daemon (its own process) | 1278.5 | 0.0 | 0.0 | 57.0 | 57.0 | - / - |
| Logos Core with every module host process (its tree) | 1278.5 | 1.84 | 0.0014 | 480.3 | 480.3 | - / - |

CPU seconds per phase: monitor-module [0.15, 0.02, 0.14]; relayer-module [0.37, 0.08, 0.25]; Logos Core daemon (its own process) [0.0, 0.0, 0.0]

