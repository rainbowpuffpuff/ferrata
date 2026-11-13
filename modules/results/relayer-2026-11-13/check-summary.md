# check.sh summary

## Steps

| step | result | seconds |
|---|---|---:|
| versions | ok | 0.3 |
| stage | ok | 0.7 |
| sdk-source | ok | 0.1 |
| cargo-lockfiles | ok | 0.0 |
| requests | ok | 0.1 |
| bridge-requests | ok | 0.1 |
| daemon-requests | ok | 0.1 |
| native-test | ok | 13.8 |
| native-calls | ok | 0.8 |
| native-daemons | ok | 90.2 |
| prepare-build | ok | 0.8 |
| build-lgx | ok | 179.3 |
| build-logosctl | ok | 49.7 |
| dev-daemon-start | ok | 0.4 |
| dev-install-ferrata_relayer_module | ok | 1.7 |
| dev-load-ferrata_relayer_module | ok | 0.1 |
| dev-show-ferrata_relayer_module | ok | 0.0 |
| dev-relayer-e2e | ok | 79.9 |
| dev-module-ls | ok | 0.0 |
| dev-daemon-stop | ok | 0.6 |
| build-lgx-portable | ok | 15.4 |
| build-logosctl-portable | ok | 0.6 |
| portable-daemon-start | ok | 0.3 |
| portable-install-ferrata_relayer_module | ok | 2.1 |
| portable-load-ferrata_relayer_module | ok | 0.1 |
| portable-show-ferrata_relayer_module | ok | 0.0 |
| portable-relayer-e2e | ok | 79.0 |
| portable-module-ls | ok | 0.0 |
| portable-daemon-stop | ok | 0.6 |
| sizes | ok | 1.4 |

## Calls

| call | module | method | native verdict | code | expectations | dev: equal to native | dev: seconds | portable: equal to native | portable: seconds |
|---|---|---|---|---:|---|---|---:|---|---:|
| relayer_version | ferrata_relayer_module | version | ferrata_relayer_module |  | met | yes | 0.0 | yes | 0.0 |
| relayer_run_unknown_option | ferrata_relayer_module | run | invalid_request |  | met | yes | 0.0 | yes | 0.0 |
| relayer_run_unbounded | ferrata_relayer_module | run | invalid_request |  | met | yes | 0.0 | yes | 0.0 |
| relayer_run_no_endpoint | ferrata_relayer_module | run | error |  | met | yes | 0.0 | yes | 0.0 |
| relayer_run_no_vault | ferrata_relayer_module | run | error |  | met | yes | 0.0 | yes | 0.0 |

## relayer (native): 10 passed, 0 failed

- PASS fork: relayer_fork.rs on an anvil fork of Sepolia with crown-live-1's proofs (65 s): 34 daemon lines; 1 checkpoint(s), stop: SIGTERM
- PASS roots: no light client on the zone yet: waiting (the zone holds no instance 0x1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d of the attestation program)
- PASS roots: a record of 2 deposits (block 5, slot 9000064): ImportRoot n = 2 applied ({"leafCount":2,"recordSlot":9000064,"block":5,"tx":"devnet block 4"})
- PASS roots: the relayer's ImportRoot is byte for byte ferrata_bridge_module's import_root for the same record (importable, 2151 bytes)
- PASS roots: up to date: no import line, nothing sent
- PASS roots: a third deposit not yet recorded: waiting (the newest record, header(id, 9000064) of block 5, holds 2 deposits, 2 are imported, the vault holds 3: waiting for a later record)
- PASS roots: a record the proofs do not match (state root changed): refused by the dry run, code 110 (ferrata_bridge_module's import_root: 110 UNKNOWN), nothing sent
- PASS roots: the next honest record: ImportRoot n = 3 applied ({"leafCount":3,"previous":2,"recordSlot":9000192})
- PASS roots: the cadence: a fourth deposit recorded within the hour of the last attempt (period 498487) is not imported (imported [2,3])
- PASS roots: the next hour of anvil's clock: the daemon imports n = 4; clean stop on SIGTERM (exit 0, {"rounds":8,"reason":"SIGTERM"})

## relayer (dev): 11 passed, 0 failed

- PASS the module loaded in a Logos Core of its own, version() {"module":"ferrata_relayer_module","version":"0.1.0","lez_wallet":false}
- PASS fork: relayer_fork.rs on an anvil fork of Sepolia with crown-live-1's proofs (54 s): 34 daemon lines; 1 checkpoint(s), stop: SIGTERM
- PASS roots: no light client on the zone yet: waiting (the zone holds no instance 0x1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d of the attestation program)
- PASS roots: a record of 2 deposits (block 7, slot 9000064): ImportRoot n = 2 applied ({"leafCount":2,"recordSlot":9000064,"block":7,"tx":"devnet block 4"})
- PASS roots: the relayer's ImportRoot is byte for byte ferrata_bridge_module's import_root for the same record (importable, 2183 bytes)
- PASS roots: up to date: no import line, nothing sent
- PASS roots: a third deposit not yet recorded: waiting (the newest record, header(id, 9000064) of block 7, holds 2 deposits, 2 are imported, the vault holds 3: waiting for a later record)
- PASS roots: a record the proofs do not match (state root changed): refused by the dry run, code 110 (ferrata_bridge_module's import_root: 110 UNKNOWN), nothing sent
- PASS roots: the next honest record: ImportRoot n = 3 applied ({"leafCount":3,"previous":2,"recordSlot":9000192})
- PASS roots: the cadence: a fourth deposit recorded within the hour of the last attempt (period 498487) is not imported (imported [2,3])
- PASS roots: the next hour of anvil's clock: the daemon imports n = 4; clean stop on SIGTERM (exit 0, {"rounds":8,"reason":"SIGTERM"})

## relayer (portable): 11 passed, 0 failed

- PASS the module loaded in a Logos Core of its own, version() {"module":"ferrata_relayer_module","version":"0.1.0","lez_wallet":false}
- PASS fork: relayer_fork.rs on an anvil fork of Sepolia with crown-live-1's proofs (53 s): 34 daemon lines; 1 checkpoint(s), stop: SIGTERM
- PASS roots: no light client on the zone yet: waiting (the zone holds no instance 0x1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d of the attestation program)
- PASS roots: a record of 2 deposits (block 6, slot 9000064): ImportRoot n = 2 applied ({"leafCount":2,"recordSlot":9000064,"block":6,"tx":"devnet block 4"})
- PASS roots: the relayer's ImportRoot is byte for byte ferrata_bridge_module's import_root for the same record (importable, 2183 bytes)
- PASS roots: up to date: no import line, nothing sent
- PASS roots: a third deposit not yet recorded: waiting (the newest record, header(id, 9000064) of block 6, holds 2 deposits, 2 are imported, the vault holds 3: waiting for a later record)
- PASS roots: a record the proofs do not match (state root changed): refused by the dry run, code 110 (ferrata_bridge_module's import_root: 110 UNKNOWN), nothing sent
- PASS roots: the next honest record: ImportRoot n = 3 applied ({"leafCount":3,"previous":2,"recordSlot":9000192})
- PASS roots: the cadence: a fourth deposit recorded within the hour of the last attempt (period 498487) is not imported (imported [2,3])
- PASS roots: the next hour of anvil's clock: the daemon imports n = 4; clean stop on SIGTERM (exit 0, {"rounds":8,"reason":"SIGTERM"})

## Replies (native)

- `relayer_version`: `{"lez_wallet": false, "methods": ["version", "run", "start", "status", "stop"], "module": "ferrata_relayer_module", "relayer": {"commit": "box", "crate": "ferrata-cli", "daemon": "ferrata_cli::relayerd (the ferrata-relayer binary's code)", "features": [], "lez": "v0.4.0-rc1 1806e87", "source_sha256": "cca42bd3930855a7830932158d1ded672fa63ec9053c13b0fd6c69f469d15bf8", "version": "0.1.0"}, "version": "0.1.0", "zones": ["devnet"]}`
- `relayer_run_no_endpoint`: `{"error": "--eth-rpc is required", "events": [], "verdict": "error"}`
- `relayer_run_no_vault`: `{"error": "--vault is required", "events": [], "verdict": "error"}`

Step failures: 0. Check failures: 0.
