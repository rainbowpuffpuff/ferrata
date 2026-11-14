# check.sh summary

## Steps

| step | result | seconds |
|---|---|---:|
| versions | ok | 0.1 |
| stage | ok | 0.8 |
| sdk-source | ok | 0.2 |
| cargo-lockfiles | ok | 0.0 |
| requests | ok | 0.1 |
| bridge-requests | ok | 0.0 |
| daemon-requests | ok | 0.1 |
| native-test | ok | 22.6 |
| native-calls | ok | 1.3 |
| prepare-build | ok | 0.8 |
| build-lgx | ok | 284.2 |
| build-logosctl | ok | 0.6 |
| dev-daemon-start | ok | 0.3 |
| dev-install-ferrata_monitor_module | ok | 1.0 |
| dev-load-ferrata_monitor_module | ok | 0.1 |
| dev-show-ferrata_monitor_module | ok | 0.0 |
| dev-install-ferrata_relayer_module | ok | 1.6 |
| dev-load-ferrata_relayer_module | ok | 0.1 |
| dev-show-ferrata_relayer_module | ok | 0.1 |
| dev-module-ls | ok | 0.0 |
| dev-daemon-stop | ok | 0.7 |
| sizes | ok | 1.1 |

## Calls

| call | module | method | native verdict | code | expectations | dev: equal to native | dev: seconds |
|---|---|---|---|---:|---|---|---:|
| monitor_version | ferrata_monitor_module | version | ferrata_monitor_module |  | met | yes | 0.0 |
| monitor_check_pass | ferrata_monitor_module | check | pass |  | met | yes | 0.0 |
| monitor_check_conflict | ferrata_monitor_module | check | cancel |  | met | yes | 0.0 |
| monitor_check_stale_set | ferrata_monitor_module | check | cancel |  | met | yes | 0.0 |
| monitor_check_wait | ferrata_monitor_module | check | wait |  | met | yes | 0.0 |
| monitor_node_view_lab | ferrata_monitor_module | node_view | ok |  | met | yes | 0.0 |
| monitor_run_unbounded | ferrata_monitor_module | run | invalid_request |  | met | yes | 0.0 |
| monitor_invalid | ferrata_monitor_module | check | invalid_request |  | met | yes | 0.0 |
| relayer_version | ferrata_relayer_module | version | ferrata_relayer_module |  | met | yes | 0.0 |
| relayer_run_unknown_option | ferrata_relayer_module | run | invalid_request |  | met | yes | 0.0 |
| relayer_run_unbounded | ferrata_relayer_module | run | invalid_request |  | met | yes | 0.0 |
| relayer_run_no_endpoint | ferrata_relayer_module | run | error |  | met | yes | 0.0 |
| relayer_run_no_vault | ferrata_relayer_module | run | error |  | met | yes | 0.0 |

## Replies (native)

- `monitor_version`: `{"methods": ["version", "check", "node_view", "run", "start", "status", "stop", "custody"], "module": "ferrata_monitor_module", "monitor": {"commit": "unknown", "source_sha256": "6c59a99e337a6c562dbae06a699efba2f7c6b520ed1c090045f2d393975c8cd1", "version": "0.1.0"}, "version": "0.1.0"}`
- `monitor_check_pass`: `{"verdict": "pass"}`
- `monitor_check_conflict`: `{"reason": {"finalHash": "0x3333333333333333333333333333333333333333333333333333333333333333", "kind": "tipConflict", "node": "synthetic", "tipId": 3}, "verdict": "cancel"}`
- `monitor_check_stale_set`: `{"reason": {"candidateN": 3, "candidateT": 2, "kind": "keySetNotLive", "liveN": 4, "liveT": 3, "node": "synthetic"}, "verdict": "cancel"}`
- `monitor_check_wait`: `{"until": 1300, "verdict": "wait", "why": "tip 5 not yet final in synthetic (final height 3)"}`
- `monitor_node_view_lab`: `{"verdict": "ok", "view": {"attestations": [], "finalBlocks": {"1": "0x19e0b86b3a6abf2b6587c89d5f7e08027e694861905aca9c3f83b58e5807407f", "10": "0xca477453ec9d7218203ee5e693f6fffb51138bcbf20a0e95f7e2a33599b83a6c", "100": "0xa124229c3aa4269c0c977ebc2f30f975193cd609ed9867b72fbda30873b62120", "101": "0x2392024053cef4827c98040b14220751279b0d9d6b09eaac34fa25868bc0b86f", "102": "0x2f733c7d2527727474620239fa4161048ece69aab3f573da18d0edd102ea26f5", "103": "0x38f6bf1bf0c75e40e77ddf3829f702f13175733ab96b158543ad27795201434c", "104": "0x7e3fa352ee0c0430cd2926bbb1e2d1f90a9aae24a2953332c4278bc70260315d", " …`
- `relayer_version`: `{"lez_wallet": false, "methods": ["version", "run", "start", "status", "stop", "custody"], "module": "ferrata_relayer_module", "relayer": {"commit": "unknown", "crate": "ferrata-cli", "daemon": "ferrata_cli::relayerd (the ferrata-relayer binary's code)", "features": [], "lez": "v0.4.0-rc1 1806e87", "source_sha256": "2d0a4e738aca7d0988b1da3b762cf3b562f82a23dc948a7843a454eb4b6b3716", "version": "0.1.0"}, "version": "0.1.0", "zones": ["devnet"]}`
- `relayer_run_no_endpoint`: `{"error": "--eth-rpc is required", "events": [], "verdict": "error"}`
- `relayer_run_no_vault`: `{"error": "--vault is required", "events": [], "verdict": "error"}`

Step failures: 0. Check failures: 0.
