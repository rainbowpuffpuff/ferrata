# check.sh summary

## Steps

| step | result | seconds |
|---|---|---:|
| versions | ok | 0.1 |
| guests | ok | 4.3 |
| stage | ok | 1.1 |
| sdk-source | ok | 0.1 |
| cargo-lockfiles | ok | 0.0 |
| requests | ok | 0.1 |
| bridge-requests | ok | 0.0 |
| daemon-requests | ok | 0.1 |
| native-test | ok | 42.9 |
| native-calls | ok | 1.2 |
| native-daemons | ok | 112.9 |
| prepare-build | ok | 2.1 |
| build-lgx | ok | 622.5 |
| build-logosctl | ok | 0.7 |
| dev-daemon-start | ok | 0.3 |
| dev-install-ferrata_updater_module | ok | 5.7 |
| dev-load-ferrata_updater_module | ok | 0.1 |
| dev-show-ferrata_updater_module | ok | 0.0 |
| dev-updater-e2e | ok | 116.9 |
| dev-module-ls | ok | 0.0 |
| dev-daemon-stop | ok | 0.6 |
| build-lgx-portable | ok | 129.7 |
| build-logosctl-portable | ok | 0.6 |
| portable-daemon-start | ok | 0.3 |
| portable-install-ferrata_updater_module | ok | 6.1 |
| portable-load-ferrata_updater_module | ok | 0.1 |
| portable-show-ferrata_updater_module | ok | 0.0 |
| portable-updater-e2e | ok | 116.9 |
| portable-module-ls | ok | 0.0 |
| portable-daemon-stop | ok | 0.6 |
| sizes | ok | 3.5 |

## Calls

| call | module | method | native verdict | code | expectations | dev: equal to native | dev: seconds | portable: equal to native | portable: seconds |
|---|---|---|---|---:|---|---|---:|---|---:|
| updater_version | ferrata_updater_module | version | ferrata_updater_module |  | met | yes | 0.2 | yes | 0.1 |
| updater_config | ferrata_updater_module | check_config | ok |  | met | yes | 0.0 | yes | 0.0 |
| updater_config_binary | ferrata_updater_module | check_config | ok |  | met | yes | 0.0 | yes | 0.0 |
| updater_config_unknown_field | ferrata_updater_module | check_config | invalid_request |  | met | yes | 0.0 | yes | 0.0 |

## updater (native): 8 passed, 0 failed

- PASS version: the attestation program the module carries has the id of docs/image-ids.md (0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df), declared and computed from its bytes
- PASS check_config: two chains (mainnet, sepolia), two sources each, ledger harness
- PASS the sequencer backend is carried (lez/updater/sequencer's library): with no sequencer at 127.0.0.1:3040, the set-up fails on the connection, as the binary's does
- PASS up to date after 172 rounds (113 s)
- PASS mainnet: finalized slot 15,631,808, root = the lab's (530599ec91a6ce70948d70d1066bb35a2a0cb999ad8e2cb6229452dcb7102328)
- PASS sepolia: finalized slot 11,556,000, root = the lab's (d31b7d34c2e6b6d8c3430dcdc763375d0ba9a9b201554164326133367aa3f3cc)
- PASS health: both instances ok
- PASS metrics: 187 transactions applied, none refused

## updater (dev): 10 passed, 0 failed

- PASS version: the attestation program the module carries has the id of docs/image-ids.md (0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df), declared and computed from its bytes
- PASS check_config: two chains (mainnet, sepolia), two sources each, ledger harness
- PASS the sequencer backend is carried (lez/updater/sequencer's library): with no sequencer at 127.0.0.1:3040, the set-up fails on the connection, as the binary's does
- PASS start: the daemon runs in the module
- PASS status: polled 23 times while the daemon ran; then stop ended it
- PASS up to date after 246 rounds (116 s)
- PASS mainnet: finalized slot 15,631,808, root = the lab's (530599ec91a6ce70948d70d1066bb35a2a0cb999ad8e2cb6229452dcb7102328)
- PASS sepolia: finalized slot 11,556,000, root = the lab's (d31b7d34c2e6b6d8c3430dcdc763375d0ba9a9b201554164326133367aa3f3cc)
- PASS health: both instances ok
- PASS metrics: 187 transactions applied, none refused

## updater (portable): 10 passed, 0 failed

- PASS version: the attestation program the module carries has the id of docs/image-ids.md (0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df), declared and computed from its bytes
- PASS check_config: two chains (mainnet, sepolia), two sources each, ledger harness
- PASS the sequencer backend is carried (lez/updater/sequencer's library): with no sequencer at 127.0.0.1:3040, the set-up fails on the connection, as the binary's does
- PASS start: the daemon runs in the module
- PASS status: polled 23 times while the daemon ran; then stop ended it
- PASS up to date after 254 rounds (117 s)
- PASS mainnet: finalized slot 15,631,808, root = the lab's (530599ec91a6ce70948d70d1066bb35a2a0cb999ad8e2cb6229452dcb7102328)
- PASS sepolia: finalized slot 11,556,000, root = the lab's (d31b7d34c2e6b6d8c3430dcdc763375d0ba9a9b201554164326133367aa3f3cc)
- PASS health: both instances ok
- PASS metrics: 187 transactions applied, none refused

## Replies (native)

- `updater_version`: `{"dev_mode": "1", "ledgers": ["harness", "sequencer"], "methods": ["version", "check_config", "run", "start", "status", "metrics", "stop"], "module": "ferrata_updater_module", "program": {"bytes": 627064, "computed_id": "0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df", "declared_id": "0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df", "name": "ferrata_lez"}, "updater": {"circuits": {"artifact": "logos-blockchain-circuits-v0.5.7-linux-x86_64", "files_sha256": "d9f24f8912aca76dbab149292dc1ba156b9882ef5563b239c8e6025904884325"}, "commit": "e613fb4", "guests": …`
- `updater_config`: `{"chains": [{"checkpoint": "4baa732e5dbdbe97932abcf62c7caccc1cba8e9c14118d153aea629e25a4abff", "layout": null, "name": "mainnet", "preset": "mainnet", "sources": [{"dir": "../../../lez/lab/results/record/net/mainnet", "kind": "light-client", "name": "record-a", "url": null}, {"dir": "../../../lez/lab/results/record/net/mainnet", "kind": "light-client", "name": "record-b", "url": null}]}, {"checkpoint": "5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d", "layout": null, "name": "sepolia", "preset": "sepolia", "sources": [{"dir": "../../../lez/lab/results/record/net/sepolia", "ki …`
- `updater_config_binary`: `{"chains": [{"checkpoint": "3b901f5e6853b5cf5eb04d7ccfe1272e66c38291cb3a11dd159cacd1117ccf7c", "layout": "gloas", "name": "sepolia", "preset": "sepolia", "sources": [{"dir": null, "kind": "light-client", "name": "lodestar", "url": "https://lodestar-sepolia.chainsafe.io"}, {"dir": null, "kind": "headers", "name": "publicnode", "url": "https://ethereum-sepolia-beacon-api.publicnode.com"}]}, {"checkpoint": "4baa732e5dbdbe97932abcf62c7caccc1cba8e9c14118d153aea629e25a4abff", "layout": null, "name": "mainnet", "preset": "mainnet", "sources": [{"dir": null, "kind": "light-client", "name": "lodestar", …`

Step failures: 0. Check failures: 0.
