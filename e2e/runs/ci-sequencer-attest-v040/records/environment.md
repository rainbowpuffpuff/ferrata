# The run's environment

- started 2026-11-28T07:28:16Z, finished 2026-11-28T07:36:45Z (UTC); exit status of the phases: 1
- phases: info deploy fresh-follow fresh-scenarios fresh-consume fresh-private export
- LEZ: v0.4.0, commit a255b42c0fc80d1cb219454a3a1f5ec3b5d92115, source tree unmodified
- sequencer: sequencer_service_standalone, sha256 8dce25a8ad4d1ba9a442d6435981c6c332ca061dedde0aa78f1aecbbf729ef3c, built with rustc 1.98.1 (48a229cea 2026-09-01) cargo 1.98.1 (797e8a9bc 2026-08-05) 
- Bedrock: none; the sequencer's mock Bedrock actor (LEZ's `standalone` feature)
- sequencer configuration: LEZ's lez/sequencer/service/networks/local.json with the changes of config/sequencer_config.diff
- RISC0_* variables in the sequencer's environment: RISC0_HOME=<risc0-home> RISC0_HOME_LEZ=<risc0-home> RISC0_SERVER_PATH=<risc0-home>/extensions/v3.1.0-cargo-risczero-x86_64-unknown-linux-gnu/r0vm 
- head rewinds in the sequencer's log: 0; inscriptions it gave up on: 0
- driver: live-driver, built from lez/live/driver at HEAD (uncommitted paths: 39)
- r0vm: risc0-r0vm 3.1.0
- guest binaries: program sha256 66466e83abdfecfca097690b05c5e45eb8ed50bb4f9f5536ca8a02bd9550e761, scripted sha256 acf5aad4ea41d112a8025dcdbe4ddfd5aabc00e3b98141c2e843e5de935c35e1
- machine: 2 CPUs visible, unknown, 64 GiB, Linux 4.19.0-gvisor
- ports: sequencer RPC 127.0.0.1:3040, metrics 127.0.0.1:9000, Bedrock API 127.0.0.1:18080
- last block: 169; blocks the sequencer published to Bedrock: 0; its last channel update: 
