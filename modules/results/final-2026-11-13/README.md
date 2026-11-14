# The final tree's checks, 13 November 2026 (branch `final-merge`)

After the last merge of the build waves (`miniapp-sequencer` into `integration8`'s tip `402626e`: merge `2bddffa`)
and a rustfmt pass (`6a19ec9`, layout only), on the Modal build box (our Modal workspace; gVisor, 2 CPUs reported, up
to 16 cores, 64 GiB; Nix 2.28.3 with the modules-more box's store restored; cargo 1.93.0, 1.98.1 and 1.88.0, r0vm
3.0.5, Foundry 1.3.6), 13 Nov 2026 11:53–13:41 UTC. Dev mode throughout: no proof was made. The native part ran at
`2bddffa`, the rest at `6a19ec9`; every step and its seconds: [`run-steps.txt`](run-steps.txt); the environment:
[`run-env.txt`](run-env.txt).

| What | Command | Result |
|---|---|---|
| The guests | `cd lez && cargo build --locked -p methods`; `cd bridge && cargo build --release --locked -p methods` | built; `release.bin` sha256 `7be1349c…11bf`, id `0xb64900bc…29e9`, as [`docs/image-ids.md`](../../../docs/image-ids.md) |
| `cli/` | `cargo test -p ferrata-wallet`; `cargo test --lib`; `cargo test --no-run`; `cargo build --example miniapp_devnet`; `cargo build --features seqnet --bins`; `cargo check --lib --no-default-features` | 3 and 11 passed; every test target, the devnet example, the three `seqnet` tools and the library without `lez-wallet` build |
| The `ferrata` tool's e2e | `cd cli && bash tests/e2e.sh` | 1 passed: the private round trip on the defaults and the recovery from the seed, the lines `cli/README.md` shows |
| The relayer's suite | `bash cli/tests/relayer.sh` | 9 passed: 6 unit tests, `gasdrop` 1, `relayer_fork` 1, the harness e2e with the gas-drop guest 1; on the fork the candidate 375,281 gas, `finalizeAttested` 218,385, the release 358,908, the paid release 407,052, each equal to [`e2e/runs/relayer-fork/results.json`](../../../e2e/runs/relayer-fork/results.json) ([`relayer-fork-results.json`](relayer-fork-results.json)) |
| `lez/` | `cargo test --locked -p ferrata-advance`; `cargo test --locked -p ferrata-updater --features harness` | 14 passed; the A3 gate's default run 0.0820, `lez/updater/results/gate.txt` the same after the run |
| `lez/updater/sequencer` | `cargo build --locked --release`; `cargo test --locked --release` (its own lock) | builds; it has no tests of its own |
| `monitor/` | `cargo test --release --locked` | 15 passed |
| `checkpoint/attestor` | `cd checkpoint && cargo +1.88 test --locked -p ferrata-attestor` | 12 passed |
| rustfmt | `cargo fmt --all --check` in `cli/` and in `lez/` | 12 files had drifted; formatted in `6a19ec9` (the only token changes are match arms' braces), then the builds above and `cli/`'s unit tests again: clean |
| The six modules | `BRIDGE_E2E=1 RELEASE_ELF=… modules/scripts/check.sh OUT` (dev and portable, `E2E=1`, the daemons' jobs in both flavours) | **0 step failures in 71 steps, 0 check failures** (2,827 s): 53 calls equal to the native ones in both Logos Cores and meeting their expectations (attest 11, bridge 13, attestor 12, monitor 8, updater 4, relayer 5); `e2e.sh` 43/43 and `bridge_e2e.sh` 24/24 in each Core; the attestor's job 23/23 and the monitor's 46/46, natively and in each Core; the updater's 8/8 natively and 10/10 in each Core; the relayer's 10/10 natively and 11/11 in each Core ([`check/`](check/summary.md)) |
| Logos Core's own modules beside ours | `RELEASE_ELF=… LOGOS_TMPDIR=/var/tmp/lio modules/scripts/logos-io.sh OUT` | `eth_rpc_module`, `beacon_api_module`, `signer_manager_module`, `fee_module` and `tx_sender_module` built from their sources; `keystore_module` stops at install, so `tx_sender_module` does not load (EVIDENCE row 110's findings, unchanged); the attestation module's 4 reads through `eth_rpc_module` (route `direct`), each equal to the plain-HTTP read; `import_root`'s read through it equal to the proofs given; `e2e.sh` 43/43 and `bridge_e2e.sh` 24/24 through that Logos Core; its `check.sh` of the two modules 0 and 0 ([`logos-io/`](logos-io/summary.md)) |
| The `e2e/seqnet` records (no sequencer started) | `python3 -I e2e/seqnet/summary.py` on a copy of each run | `summary.json` re-derived equal for `seqnet-cli-1`, `-miniapp-1`, `-2` and `-3`; `SUMMARY.md` equal for the three mini-app runs, and for `cli-1` but for two more lines at its end (the run log's count and `DONE`, written after the committed summary); in all four, `programs.txt`'s `checkpoint_b` and `release` sha256 and ids equal `docs/image-ids.md`, and the attestation and bridge programs' ids are those of the final programs |

Packages (bytes, dev / portable): attest 3,525,111 / 6,685,698; bridge 6,467,973 / 9,624,252; attestor 2,342,280 /
5,502,290; monitor 9,029,959 / 12,187,556; updater 57,842,481 / 61,007,846; relayer 14,317,062 / 17,477,490
([`check/sizes.log`](check/sizes.log)).

Not re-run here: the catalog (`release-local.sh`, `catalog-install.sh`), the mini-app's Qt Quick and Basecamp tests,
the live zone of four, any sequencer run, any proof.
