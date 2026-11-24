# The run's environment

- started 2026-11-24T03:25:40Z, finished 2026-11-24T03:51:37Z (UTC); exit status of the phases: 0
- phases: info deploy fresh-follow fresh-scenarios fresh-consume fresh-private export
- LEZ: v0.4.0-rc3, commit bafd07be0701398f9bc435f73ab05e9a7936c301, source tree unmodified
- sequencer: sequencer_service_standalone, sha256 9c4b6d5633a0089654f6b48800144af82fce21de197a40ed543dddc78ca328ef, built with rustc 1.98.1 (48a229cea 2026-09-01) cargo 1.98.1 (797e8a9bc 2026-08-05) 
- Bedrock: none; the sequencer's mock Bedrock actor (LEZ's `standalone` feature)
- sequencer configuration: LEZ's lez/sequencer/service/networks/local.json with the changes of config/sequencer_config.diff
- RISC0_* variables in the sequencer's environment: RISC0_HOME=<risc0-home> 
- head rewinds in the sequencer's log: 0; inscriptions it gave up on: 0
- driver: live-driver, built from lez/live/driver at HEAD (uncommitted paths: 36)
- r0vm: risc0-r0vm 3.0.5
- guest binaries: program sha256 ddfde69bea89a5b882f55266fcb4b7c887df7debc1c9d4972d8ebea5a72eaad9, scripted sha256 29249a3ac807ad3268ff5499ccee70a75b687d5fe5517fa5843b333c2c6c1bc0
- machine: 2 CPUs visible, unknown, 64 GiB, Linux 4.19.0-gvisor
- ports: sequencer RPC 127.0.0.1:3040, metrics 127.0.0.1:9000, Bedrock API 127.0.0.1:18080
- last block: 519; blocks the sequencer published to Bedrock: 0; its last channel update: 
