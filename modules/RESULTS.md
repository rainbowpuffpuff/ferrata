# Logos modules: results

Runs, newest first: Ferrata's Ethereum transactions through Logos Core's send stack (14 November), the six modules on the final tree (13 November, after the last merge), Ethereum reads through Logos Core's own `eth_rpc_module` (13 November), the updater module with its sequencer backend, on a running LEZ sequencer inside a headless
Logos Core (13 November), the relayer as the sixth module (13 November: root imports in `ferrata-relayer` and
`ferrata_relayer_module`), the five modules after the merge of `updater-live` (13 November: the updater's
sequencer backend, `headers` sources and `finish_own_jobs` on the daemon the updater module drives), the
five modules after the merge of the two branches of 12 November (12–13 November, at the merged commit), the three daemons as modules and completing the attestation and bridge
modules (those two branches, each run on its own), the attestation and bridge modules (11 November), the
attestation module's prototype (10 November).

## Ferrata's Ethereum transactions through Logos Core's send stack, 14 November 2026

[`results/send-stack-2026-11-14/`](results/send-stack-2026-11-14/README.md) (Modal build box, `scripts/send-stack.sh`;
a local anvil chain only). `eth_rpc_module` 6965f32 with our 6-line glue patch for the clients builder 5f74cdb
generates (`patches/eth_rpc_module-6965f32-generated-clients.patch`), `fee_module`, `keystore_module`,
`signer_manager_module`, `tx_sender_module` (each with the builder's own locks), `verified_proxy_module` and
`beacon_api_module` (their own lock) and `ferrata_monitor_module` and `ferrata_relayer_module` (`check.sh`: step and
check failures 0) loaded in one headless Logos Core (logosctl `66d098d`, 14 modules). The two Ferrata modules named
custodians by the keystore's and signer manager's `configure` (ungated upstream); each module's `custody` imported its
daemon's key and unlocked it for `tx_sender_module` without confirmation (74 ms, 68 ms). From one anvil snapshot per
step, each daemon transaction sent the plain way (alloy) and with `--send-via logos` (`tx_sender_module`, signed by
`keystore_module` through `signer_manager_module`, broadcast through `eth_rpc_module`, 29–66 ms after `send`): the
relayer's `submitCandidate`, `finalizeAttested`, `release`, `claimOwed`, the monitor's `cancelCandidate` (twice) and
`pauseCheckpoints`: **7 of 7 equal** (calldata, sender, nonce, status, gas used, gas limit, logs, block, the vault's
state after). The fallback (tx_sender_module unloaded, the key held) sent the plain way and said so. Findings: the
glue patch (draft upstream PR); a module method that blocks while its own thread calls another module deadlocks (the
SDK's worker-thread calls wait for the Qt thread), so the daemons run with `start` and `run` refuses `--send-via logos`.

## The six modules on the final tree, 13 November 2026

**After the last merge (`miniapp-sequencer` into `integration8`, branch `final-merge`), every module check again.**
Modal build box (gVisor, 2 CPUs reported, up to 16 cores, 64 GiB; Nix 2.28.3, the store of the
modules-more box restored), commit `14838c3`, 13 Nov 2026 12:28–13:37 UTC, dev mode: `check.sh` with the six modules,
dev and portable, `E2E=1 BRIDGE_E2E=1`, **step failures 0 (71 steps), check failures 0** (2,827 s): 53 calls, each
equal to native in both Cores and meeting its expectations (attest 11, bridge 13, attestor 12, monitor 8, updater 4,
relayer 5); `e2e.sh` 43/43 and `bridge_e2e.sh` 24/24 in each Core; the attestor's job 23/23 and the monitor's 46/46,
natively and in each Core; the updater's 8/8 natively and 10/10 in each Core; the relayer's 10/10 natively and 11/11
in each Core. Then `logos-io.sh` (1,144 s): Logos Core's own modules built from their sources with the same findings
as in the section below (`keystore_module` stops at install, `tx_sender_module` does not load), the attestation module's four reads
and `import_root`'s read through `eth_rpc_module` each equal to the plain read or the proofs given, `e2e.sh` 43/43
and `bridge_e2e.sh` 24/24 through that Logos Core. In the same session: `cli/`'s tests, its e2e and the relayer's
suite, `lez/`'s updater tests, `monitor/` and `checkpoint/attestor`. Files and every figure:
[`results/final-2026-11-13/`](results/final-2026-11-13/README.md).

## Verified reads through Logos Core's `verified_proxy_module`, on Sepolia, 14 November 2026

[`results/verified-proxy-2026-11-14/`](results/verified-proxy-2026-11-14/README.md) (Modal build box,
`scripts/verified-proxy.sh`; public Sepolia, reads only). Logos Core's `verified_proxy_module` (`fa98479`, with
nimbus-eth1 v0.4.4's `libverifproxy` built from source) and `beacon_api_module` (`aca138e`), each with its own lock,
loaded in logosctl `66d098d` beside `eth_rpc_module`, `ferrata_attest_module` and `ferrata_updater_module`. From a
finalized root Lodestar, publicnode and Nimbus agreed on, the proxy's light client synced (verified head two or three
blocks behind the endpoint's). With Sepolia in `required` mode: `fetch_log_proof` of the four deposits of
`crown-live-3` and `crown-live-1`, the four capture blocks' headers and the vault's 24 storage values (route
`verified`), and `verify_checkpoint`'s execution check, each equal to the runs' records and to the same read with the
mode `off`; `eth_getProof` refused (`unknown method`: nimbus's library passes it through unverified and does not
dispatch it). The updater's light-client data through `beacon_api_module.lc_request`, turned into SSZ equal byte for
byte to the providers' SSZ and the 12 November fixture; `ferrata_updater_module` on those sources followed the crown
runs' checkpoint to Sepolia's finalized header on the in-process LEZ chain (21 transactions applied, none refused).
48 of 48 checks. Not shown: the proxy on a devnet (it serves public networks only), any other module's reads through
it, the updater through it on a sequencer, sends.

## LEZ through Logos Core's `lez_core` (its merged v0.4 state), 14 November 2026

[`results/lez-core-v04-2026-11-14/`](results/lez-core-v04-2026-11-14/README.md) (Modal build box). `lez_core` at
`cc73ebf` (`main` after #72 and #74, merged 13 November; LEZ v0.4.0-rc1 `1806e87`, builder `5f74cdb`) built from its
source in 1,289 s, and with [`patches/lez_core-cc73ebf-authorizes-public-pda.patch`](patches/lez_core-cc73ebf-authorizes-public-pda.patch);
its unit tests 60/60 as merged, 64/64 patched; `check.sh` for `ferrata_bridge_module` 0 step and 0 check failures. On a
LEZ v0.4.0-rc1 devnet with the final programs (`e2e/runs/seqnet-lezcore-1/`, dev mode), the module's wallet session
through `lez_core`: 85 of 85 bridge-state reads (`get_account_public`) and 9 of 9 private-holding reads equal to the
plain path's, the private claim and the private burn executed, proven and sent by LEZ's wallet inside Logos Core and
applied (LEZ blocks 30, 31), the release paid; with real proofs (`e2e/runs/seqnet-lezcore-2/`, the sequencer verifying)
the claim and the burn proven inside Logos Core in 248.7 s and 228.8 s and applied (blocks 74, 98). As merged, `lez_core` refuses the bridge's PDA accounts (no
`public_pda` kind) and, with no account authorizing, the bridge program refuses the claim (exit 252).

## Ethereum reads through Logos Core's `eth_rpc_module`, 13 November 2026

[`results/logos-core-io-2026-11-13/`](results/logos-core-io-2026-11-13/) (Modal build box, `scripts/logos-io.sh`;
what the Logos modules are: [LOGOS-CORE-MODULES.md](LOGOS-CORE-MODULES.md)). Logos Core's modules built from
their sources with our builder pin: `eth_rpc_module` (its two optional dependencies called by name),
`beacon_api_module`, `fee_module`, `signer_manager_module`, `tx_sender_module`; `keystore_module` does not
build with that pin (its install stops on its interface contract), so the send stack does not load. In one
headless Logos Core with `eth_rpc_module` beside them, the attestation and bridge modules read Ethereum
through it (`raw_rpc`, called by name from inside the module; plain HTTP kept as the fallback):
`fetch_log_proof` twice and the storage proof twice, each equal to the same read over plain HTTP, then `e2e.sh`
43/43 on those replies; `import_root` reading the vault's `eth_getProof` through it, equal to the reply to the
proofs given (`importable`, n = 3), `bridge_e2e.sh` 24/24. `check.sh` for the two modules: 0 step and 0
check failures. Not shown here: verified reads (`verified_proxy_module`: shown on Sepolia on 14 November, above),
sends (`keystore_module`), the updater's beacon data through `beacon_api_module` (14 November, above), LEZ through
`lez_core` (v0.3.1).

## The updater module on a running LEZ sequencer, 13 November 2026

`ferrata_updater_module` carries both of the daemon's backends: with `ledger = "sequencer"` it opens
`lez/updater/sequencer/`'s library, the code of the binary of row 99 (LEZ's wallet client, a follower of blocks final
on Bedrock), linking LEZ v0.4.0-rc1's node-side crates. Built with logos-module-builder at commit `e613fb4` on the
Modal build box: `check.sh` for this module, dev and portable, **step failures 0, check
failures 0**, `updater-e2e.sh` 8/8 natively and 10/10 through each Logos Core on the in-process chain (187
transactions applied, none refused; with no sequencer, the sequencer backend's set-up fails on the connection, as the
binary's does). Then the portable package (61 MB) in the portable headless Logos Core on a crown zone (Modal,
`ferrata-crown`'s `updater_session`, LEZ v0.4.0-rc1 and a Logos Blockchain node, the Sepolia instance of the crown
run): `install`, `load`, `version`, `check_config`, `start`, `status` at every probe, `stop` and `start` mid-job,
`metrics`, `stop`. Lodestar's light-client data and publicnode's headers, read only. Session U3: **5 updates applied
end to end** (19 transactions: a rotation of the committees, the adoption of period 1413's committee, three finality
updates), the restart through the module's `stop` and `start` with a job half done (the kept job finished, no second
`Open`), export and generation 2 saved; session U2, preempted 40 s before its end: 9 updates (31 transactions) and the
same restart. `check.py` ALL PASS on both records, every applied root canonical on publicnode.
[`results/updater-sequencer-2026-11-13/`](results/updater-sequencer-2026-11-13/README.md).

## The relayer as a module, and its root imports, 13 November 2026

**`ferrata-relayer` imports the vault's deposit roots, and runs as `ferrata_relayer_module`.** The daemon moved from
`cli/src/bin/relayer.rs` into `cli/`'s library (`cli/src/relayerd.rs`): the binary parses its arguments, prints the
daemon's lines and turns SIGTERM or SIGINT into a stop; the module runs the same code in a thread of its host
process, keeps the lines and stops when asked. New in the daemon: root imports (`--import-roots`; once per
`--import-every` period of Ethereum time, default 3,600 s) with the code `ferrata_bridge_module`'s `import_root`
runs (`ferrata_wallet::import`: `import_proof`, `import_call`, `dry_run_import`), and `--lez-devnet`, the mini-app
devnet's zone in place of a sequencer (a stand-in). `cli/` gained the feature `lez-wallet` (default): the module
builds `cli/`'s library without LEZ's wallet library, which links LEZ's node-side crates (RocksDB, libp2p).

Where: the Modal build box (gVisor; Nix 2.28.3; Logos Core from source), the sources of commit `f3dc299`.
`MODULES=ferrata_relayer_module modules/scripts/check.sh`: **0 step failures, 0 check failures**; the dev package
14,318,774 bytes (built in 179 s), the portable 17,477,514; five one-shot calls (`daemon_requests.py`) equal to the
native replies in both Logos Cores; `relayer-e2e.sh` 10 of 10 with the binary, 11 of 11 through the module in the dev
and in the portable Logos Core (in a Logos Core of its own, without `RISC0_DEV_MODE`):

- `cli/tests/relayer_fork.rs` through the module (`FERRATA_RELAYER_CMD=modules/scripts/relayer-module.sh`): an anvil
  fork of Sepolia with `crown-live-1`'s real proofs; candidate 375,281 gas, `finalizeAttested` 218,385, release
  358,908, the paid release wrapped by the module from a succinct receipt 407,052 (2 × 10¹² wei to the relayer): the
  committed binary run's transactions and gas (EVIDENCE row 46); refusals, `status`, a clean stop on SIGTERM.
- Root imports on the mini-app's devnet, the light client's record of anvil's own block written by the devnet
  (`lc.finalize`, stand-in; its state root is anvil's header's) and the proofs anvil's `eth_getProof`: waiting
  without a light client; n = 2 applied (273,511–275,460 execution gas), the message byte for byte
  `ferrata_bridge_module`'s `import_root` for the same record and proofs; nothing sent when up to date; waiting for a
  later record; a record the proofs do not match refused before sending with code 110 (`ProofNodeMismatch`), the
  bridge module's code for it; n = 3; on the default cadence, no import within the hour and n = 4 in the next; a
  clean stop on SIGTERM.
- The catalog, for the relayer alone (`DRY_RUN_DIR`, `INSTALL_TEST_DIR`: the five's records are kept): exported,
  `nix build .#lgx-portable` (204 s), `lgx verify` valid, unsigned, `index.json` with 1 package, `validate-repo` OK;
  installed by name from the catalog over HTTPS on 127.0.0.1, loaded, called, and the installed module's root
  imports 10 of 10 (`catalog/dry-run-relayer/`, `catalog/install-test-relayer/`).

Records: [`results/relayer-2026-11-13/`](results/relayer-2026-11-13/README.md). The box runs before the final one:
`cargo check --no-default-features` of `cli/` and its default build, unit tests and test targets (ok); the binary's
`relayer-e2e.sh` 8 of 8 (before the cadence check and the comparison existed); a first `check.sh` whose only failure
was the comparison with the bridge module, because the devnet no longer compiled (a closure named `word` hid the
function `word`) and the earlier devnet binary was used: fixed, and `relayer-e2e.sh` now stops on a build error.
Spend: about 0.32 USD of box time (estimated by `mrun`), our Modal workspace.

Not done: the relayer's root imports on a running LEZ sequencer (the binary's `--sequencer` path has not run with
`--import-roots`; the module has no LEZ wallet); the devnet's records are not written by a sync committee; Gloas
records are waited on, not imported; the five other modules were not rebuilt with the shared scripts' new entries.

## The five modules after the merge of `updater-live`, 13 November 2026

**`updater-live` merged into the daemon the updater module drives, and check.sh re-run.** `updater-live` gave
`ferrata-updater` a backend for a running LEZ sequencer (`lez/updater/sequencer/`, a binary of its own),
`headers` sources, the store's `layout` and `finish_own_jobs` (an updater finishes the job it opened first, also
after a restart). They now sit in the daemon of `lez/updater/src/daemon.rs` that this module drives
(`Daemon::open` with the harness ledger; the sequencer build passes its own ledger to the same daemon), with the
update planning in `lez/advance`. The module carried the in-process chain only at this merge: it refused `ledger =
"sequencer"` (the check's configuration now has the `[sequencer]` section the configuration requires; the module
carries that backend since, above), and `check_config`
reports the chains' `layout`, the sources' `kind`, `[sequencer]` and `finish_own_jobs`.

Modal build box (gVisor, 2 CPUs reported, up to 16 cores, 64 GiB; Nix 2.28.3, the store of
the modules-more box restored), commit `b099490`, 13 Nov 2026 04:23–04:52 UTC: `check.sh` with the five modules,
dev and portable, `E2E=1 BRIDGE_E2E=1`, **step failures 0, check failures 0** (1,708 s): 48 calls, each equal to
native in both Cores and meeting its expectations (attest 11, bridge 13, attestor 12, monitor 8, updater 4);
`e2e.sh` 43/43 and `bridge_e2e.sh` 24/24 in each Core; the attestor's job 23/23 and the monitor's 46/46, natively
and in each Core; **the updater's job 8/8 natively (109 s) and 10/10 in each Core** (dev 117 s, portable 113 s):
mainnet to 15,631,808 and Sepolia to 11,556,000, the lab's roots, 4 and 35 jobs, 187 transactions applied, none
refused. Packages (dev/portable bytes): attest 3,483,190/6,641,640; bridge 6,383,644/9,543,692; attestor
2,342,281/5,502,285; monitor 9,030,000/12,187,798; updater 12,723,920/15,878,719. Files:
[`results/merged-updater-2026-11-13/`](results/merged-updater-2026-11-13/). In the same run, the updater's own
tests (14 of 14), the A3 gate's eight runs (every figure as before the merge), the sequencer build and the
re-check of the live Sepolia records: [`../lez/updater/results/merged-2026-11-13/`](../lez/updater/results/merged-2026-11-13/README.md).
Dev mode throughout; the catalog and the live zone of four were not re-run.

## The five modules after the merge, 12–13 November 2026

**The two branches of 12 November merged, and every result re-run at the merged commit.** The attestation
and bridge modules completed on the tools' own code (`advance`, the private claim, `import_root`,
`registry`, the release rule, `prove_release`; "Completing the two modules" below) and the three daemons as
modules ("The daemons as modules" below) were built on separate branches from the same commit and ran
separately. Merged (`50b13d3`), one `stage.sh` stages all five (the attestation and bridge modules through
`stage.py`, the attestor and monitor modules as before, the updater through `stage-updater.sh`), one
`check.sh` builds and checks them, with `BRIDGE_E2E=1` and the daemons' jobs. The updater now plans with
`ferrata-advance` (`lez/advance`), the code the attestation module's `advance` runs, so the updater
module's staged tree carries it too (`2dbad7c`). Then, on the Modal build box (a
gVisor container: 2 CPUs reported, up to 16 cores and 64 GiB; Nix 2.28.3 with the Nix store the daemons'
run had built, Logos from source; cargo 1.93.0 and 1.98.1, r0vm 3.0.5, Foundry 1.3.6), at commit `2dbad7c`
([`results/merged-2026-11-13/`](results/merged-2026-11-13/summary.md)):

| What | Natively | Dev Logos Core | Portable | From the catalog |
|---|---|---|---|---|
| 48 one-shot calls (the attestation module 11 with `version`, the bridge module 13, the attestor 12, the monitor 8, the updater 4), each reply equal to the native call and meeting its expectations | — | 48 of 48 | 48 of 48 | the five installed by name, loaded, `version` called |
| `e2e.sh`: two logs attested end to end and two refusals, `advance` (an update's `Open`, `Miller`, `Finish` planned by the module and executed by the program in order), the bridge module's `Burn` and `Claim` decoded by the program's types | — | 43 of 43 | 43 of 43 | 43 of 43 |
| `bridge_e2e.sh`: the mini-app's devnet (`import_root`, `registry`, two private claims, the release rule, `prove_release`, both releases accepted by the vault) | — | 24 of 24 | 24 of 24 | 24 of 24 |
| the attestor on the zone of four's records (`attestor-e2e.sh`): the collector's 3 of 4 = the input the checkpoint program proved, byte for byte | 23 of 23 | 23 of 23 | 23 of 23 | 23 of 23 |
| the attestor beside each sequencer of a running zone of four (`attestor-zone-live.sh`) | — | — | 13 of 13 | — |
| the monitor (`monitor-e2e.sh`, `anvil-e2e.sh` with the module in place of the binary) | 46 of 46 (the binary) | 46 of 46 | 46 of 46 | 46 of 46 |
| the updater (`updater-e2e.sh`): mainnet and Sepolia to the lab's final roots, 187 transactions applied, none refused | 8 of 8 | 10 of 10 | 10 of 10 | 10 of 10 |

`check.sh` (`BRIDGE_E2E=1`, dev and portable): step failures 0, check failures 0, 1,531 s (the Logos
packages and the daemons' run's builds cached in the Nix store; the five modules' dev packages built in
507 s). Each job's figures are those of the branches' own runs, timings aside: the updater's mainnet instance at slot
15,631,808 (root `530599ec…2328`) and Sepolia's at 11,556,000 (`d31b7d34…f3cc`), 4 and 35 jobs, in 107 s
through each Core and 106 s natively; the monitor's cancellations at 41,477 gas (41,468 in S5), the smallest
margin before a deadline 2,992 s through the module (3,000 s for the binary), the module's host process at
70,404 kB (dev) and 74,632 kB (portable) resident when S5 killed it; the release guest built here has the id
of [`docs/image-ids.md`](../docs/image-ids.md) (`0xb64900bc…29e9`, binary sha256 `7be1349c…11bf`); the
updater module carries the attestation program `0x816d1fef…03df`, declared and computed from its bytes.

The catalog's steps ran next, on the same module sources (tree `905731a`: it adds run 1's results and has
`catalog-install.sh` log `e2e.sh`'s and `bridge_e2e.sh`'s check counts; `version()` reports `2dbad7c`)
(`release-local.sh`, 576 s: the five exported as their
own repositories and built with `nix build .#lgx-portable`, `lgx verify` valid and unsigned, the sidecars,
`index.json` with five packages, `validate-repo` OK; [`catalog/dry-run/`](catalog/dry-run/index.json)), then
the install from that catalog (`catalog-install.sh` over HTTPS on 127.0.0.1, 292 s): the five found,
installed by name, loaded and called, then every job through the installed modules (the last column: the
daemons' 79 checks, `e2e.sh` 43, `bridge_e2e.sh` 24); one failure, the known one of EVIDENCE row 61,
refreshing the built-in official catalog under the throwaway CA bundle
([`catalog/install-test/log.txt`](catalog/install-test/log.txt)).

In the same run, beside each sequencer of a second running zone of four (`attestor-zone-live.sh`: four LEZ v0.4.0-rc1
sequencers from the lab bundle, 300 s of sequencing; four portable Logos Cores, each module's attestation
key certified by its sequencer's channel key), each module's loop read the node every 15 s: 22 rounds and
19 rising tips signed each, up to the final tip 29, one round refused (no final block yet), no error; the
collector's 3 of 4 accepted by the checkpoint rule natively (n = 4, t = 3; the input 25,323 bytes, sha256
`0d7c33a8…9727`), 4 of 4 accepted, 2 of 4 refused: 13 of 13
([`results/merged-2026-11-13/attestor-zone-live/`](results/merged-2026-11-13/attestor-zone-live/summary.md);
the zone's timing varies between runs: the daemons' run ended at tip 30).

Packages (bytes, dev / portable): `ferrata_attest_module` 3,483,216 / 6,641,682, `ferrata_bridge_module`
6,383,603 / 9,543,619, `ferrata_attestor_module` 2,342,266 / 5,502,240, `ferrata_monitor_module`
9,029,942 / 12,187,606, `ferrata_updater_module` 12,641,535 / 15,801,312 ([`sizes.log`](results/merged-2026-11-13/sizes.log)).

The native tests the two branches ran, at the same commit on the same box
([`native-tests.txt`](results/merged-2026-11-13/native-tests.txt)): `cli/` (`ferrata-wallet` 3, the
`ferrata` tool's library 10, every test target built, the devnet example built, the tool's end-to-end test
`cli/tests/e2e.sh` passed), `lez/` (`ferrata-advance` built; `ferrata-updater` with the harness: its
configuration tests 3 and its recorded-data tests 7, the A3 gate among them held: honest gas on refused
steps 0.0820 of the applied, ≤ 0.10, honest fee shares 0.4428 / 0.5572), `monitor/` (15), and
`checkpoint/attestor` (12, Rust 1.88). The first `cli/` step failed because the run's script did not point
`RISC0_HOME` at LEZ's guest toolchain (its guests then built with RISC Zero's Rust 1.88 and stopped at
packages that need 1.89 and 1.90); with it set, as `cli/tests/e2e.sh` sets it, it passed in the same box.

What the merge resolved (both branches had changed the module scripts and the docs):
- `stage.sh`: one entry point; `stage.py` now takes module names (it staged both of its modules at once).
- `check.sh`: the daemons' branch's five-module check, with the completing branch's `BRIDGE_E2E=1` step
  and `stage.py`'s `staged/` and `upstream/` trees in each module's build copy; `summary.py` reports both.
- `requests.py`: both branches had fixed the same stale instance id to the same value.
- The updater module: `stage-updater.sh` vendors `lez/advance`, and its `Cargo.lock` gains
  `ferrata-advance` exactly as `lez/Cargo.lock` did; `export.sh` (which the catalog's steps call) commits
  `stage.py`'s trees.

Re-run (Linux x86-64 with Nix, the RISC Zero toolchains of `lez/` and `bridge/`, r0vm, Foundry with
`vault/lib`; `LOGOS_TMPDIR` of at most 62 bytes):

```sh
(cd bridge && cargo build --release --locked -p methods)        # the release guest, for RELEASE_ELF
BRIDGE_E2E=1 RELEASE_ELF=$(find bridge/target/riscv-guest -name release.bin | head -1) \
  RISC0_SERVER_PATH=$(command -v r0vm) modules/scripts/check.sh OUT
modules/scripts/release-local.sh REL
CATALOG_E2E=1 CATALOG_E2E_MORE="updater-e2e.sh monitor-e2e.sh e2e.sh bridge_e2e.sh" \
  modules/scripts/catalog-install.sh REL OUT/logos-portable/bin/logosctl WORK
LOGOSCTL=OUT/logos-portable/bin/logosctl LGX=$(ls OUT/lgx-portable-ferrata_attestor_module/*.lgx) \
  modules/scripts/attestor-zone-live.sh spike/tmp/lab/bundle.tar.zst LIVE 300 15
```

Not done at the merge: what each branch left stands (below). The completing branch's one real release
proof through Logos Core was not repeated (`prove_release` is unchanged by the merge); everything here is
in dev mode.

## The daemons as modules, 12 November 2026

**Five modules built, packaged, loaded and called together; the three daemons doing their jobs as
modules.** `modules/scripts/check.sh` built `ferrata_attestor_module` (rule (B)'s attestor and collector,
`checkpoint/attestor`), `ferrata_monitor_module` (the watchers' `monitor`) and `ferrata_updater_module`
(`ferrata-updater`, `lez/updater`) with logos-module-builder, beside the two earlier modules, packaged all
five as dev and portable `.lgx`, installed and loaded them together in a headless Logos Core (`logosctl`,
dev and portable builds) and called every one-shot method: 48 calls (24 of them the three new modules'), each reply equal to the
native call and meeting its expectations, in both. Then each daemon did its job through each Logos Core, on the
devnet its own tests use, against the same job run natively:

| Job | Devnet | Natively | Dev Logos Core | Portable | From the catalog |
|---|---|---|---|---|---|
| the attestor on the zone of four's records: tier, base, four certificates and attestations equal to the run's, the collector's 3 of 4 = the input the checkpoint program proved (sha256 `f376ad54…c575`), 4 of 4, 2 of 4 refused, three refusals to sign, the loop against a node over HTTP | `checkpoint/attestor/runs/zone` | 23 of 23 | 23 of 23 | 23 of 23 | 23 of 23 |
| the attestor beside each sequencer of a **running** zone of four: four Logos Cores, each module's loop reading the node while the zone runs | `checkpoint/attestor/zone.sh` (the lab bundle) | — | — | 13 of 13 | — |
| the monitor: `monitor/tests/anvil-e2e.sh`, scenarios S1–S11, A0–A4, R1–R4 and the alerting check, the module in place of the binary | anvil, a v5 vault | 46 of 46 (the binary) | 46 of 46 | 46 of 46 | 46 of 46 |
| the updater: mainnet and Sepolia from their checkpoints to the lab's final roots, none refused; the sequencer backend refused | the in-process LEZ v0.4.0-rc1 chain, the lab's records | 8 of 8 (`run`) | 10 of 10 | 10 of 10 | 10 of 10 |

The native, dev and portable columns and the calls are `check.sh` at commit `dcf7502` (step failures 0,
check failures 0); the catalog column and the running zone are the run before it, at `a161df1`, with the
same packages (two expectation fixes came between, see 8 below and "Calls"). `e2e.sh` (the earlier
modules' log end to end) passed 36 of 36 in both flavours. All five modules were
installed from the local catalog by name and loaded, and the three jobs passed through the installed
modules (the last column). Results: [`results/check-2026-11-12/`](results/check-2026-11-12/summary.md),
[`results/attestor-zone-live/`](results/attestor-zone-live/summary.md), [`catalog/`](catalog/README.md).

### Where the daemons' modules ran

The Modal build box of this worktree (our Modal workspace), a gVisor container (`Linux 4.19.0-gvisor
x86_64`, 16 cores, 64 GiB), started cold: Nix 2.28.3 with the set-up of the first run, cargo 1.93.0 (and
1.98.1 for `lez/`), Foundry 1.3.6 (anvil, forge, cast), r0vm 3.0.5; builder, SDK, `logosctl` and nixpkgs
at the revisions of the first run; Logos packages built from source again (the dev `logosctl` took
1,848.6 s). Logos Core with `LOGOS_TMPDIR=/var/tmp/lgm` (see 4 below). On the laptop only edits, `git` and
reads. `version()` of the new modules reports commit `723fa5b`, the commit their staged crates were
copied from: after it the modules changed only in `metadata.json`'s descriptions and the test scripts (the
sources hashes in `version()` identify the copies). Every number here is from the runs' own output.

### The three modules

| Module | Wraps | Methods | Rust | Dev / portable `.lgx` bytes | Dev closure bytes |
|---|---|---|---|---:|---:|
| `ferrata_attestor_module` | `checkpoint/attestor` (`ferrata-attestor`) with `checkpoint-core`, `attestor-core` | `version`, `keygen`, `pubkey`, `deployment`, `certify`, `tier`, `attest`, `verify`, `collect`, `assemble`, `base`; `start`, `status`, `stop` | builder's nixpkgs | 2,342,262 / 5,502,245 | 668,735,576 |
| `ferrata_monitor_module` | `monitor/` (its library, alloy 1.8.3, tokio) with `attestor-core` | `version`, `check`, `node_view`, `run`; `start`, `status`, `stop` | 1.93.0 | 9,029,901 / 12,187,598 | 693,801,688 |
| `ferrata_updater_module` | `lez/updater` with the `lez/` harness, LEZ v0.4.0-rc1's state machine, the 7 guests of `lez/methods` | `version`, `check_config`, `run`; `start`, `status`, `metrics`, `stop` | 1.98.0 | 12,620,856 / 15,780,219 | 707,729,872 |

The contracts the builder derived from the traits (`.lidl`): 2,045, 1,009 and 979 bytes. `module show`
lists each method with its description (`show-*.json`). The updater module carries the attestation
program it executes: `version()` reports the image id declared for it and the one computed from its
bytes, both `0x816d1fef…03df`, the id of [`docs/image-ids.md`](../docs/image-ids.md) (checked by
`updater-e2e.sh`). The daemons' own code changed only where a module needed it: the monitor's run
arguments and start moved into its library (`monitor/src/cli.rs`), its loop ends on a stop channel instead
of a signal when one is given (`daemon::run_with`: the module's process belongs to the host), and it takes
`--key-file`; the updater's set-up and rounds moved into its library (`lez/updater/src/daemon.rs`). Both
binaries run on the same code (the monitor's 15 tests and its anvil run, the updater's configuration tests
and catch-up, below).

### The attestor beside each sequencer of a running zone

`modules/scripts/attestor-zone-live.sh` (12 November, 21:24–21:30 UTC, commit `a161df1`): `checkpoint/attestor/zone.sh` ran a zone of four LEZ
v0.4.0-rc1 sequencers on one Logos Blockchain node (upstream's devnet configuration, the lab bundle's
binaries, 300 s of sequencing, genesis sha256 equal to the earlier zone run's), and four portable Logos
Cores, one per sequencer, each installed the attestor's package, loaded it, made its attestation key (OS
randomness) and had that sequencer's channel key certify it. While the zone ran, each module's loop
(`start`, poll 15 s) read the node over HTTP: 22 rounds each, the first refused (the node's final tier held no block yet), then 19 rising tips signed (3, 4, 6, 8, 9, 11, 13, 14, 16, 17, 19, 20, 22, 23, 25, 26, 28, 29, 30), each published as `att-<tip>.json`, none refused for a lower tip or another hash, no error. After the sequencers stopped and the LIB had
passed the channel's tip, each signed the final tip 30 (`2e0dca72…2374`); the four were stopped, and the module's
collector took 3 of 4 (attestor 3 left out; the checkpoint rule natively: accepted, n = 4, t = 3, producer `fce0d0fa…8096`; 26,166 bytes, journal sha256 `a58d6a4d…103e`), 4 of 4 (accepted) and refused 2 of 4. The node's final tier: the four channel keys, blocks 2..=30 checked (29), 11, 6, 6 and 6 blocks inscribed by the four sequencers.
Records: [`results/attestor-zone-live/`](results/attestor-zone-live/summary.md) (each attestor's rounds,
status, certificate and last attestation).

### The monitor through the module

`monitor/tests/anvil-e2e.sh` with `MONITOR_CMD=modules/scripts/monitor-module.sh`: every `monitor run`
the test makes goes to the module, bounded runs as `run`, the daemons of S5 and S10 as `start` with the
script waiting on `status`. S5 kills the module's host process (`logos_host --name ferrata_monitor_module`)
with SIGKILL while the daemon waits on a candidate, loads the module again, and the restarted daemon
cancels from the state file; a second daemon on the same state file is refused while the first runs (the
file lock holds inside one process); S10's SIGTERM becomes `stop`: exit 0, a `stopped` alert, the state
saved, the lock free. Every critical alert reached the pager through `--alert-cmd`, run by the module.
The cancellations used 41,477 gas (41,468 in S5), as with the binary; the smallest margin left before a deadline was 2,991 s (S5, the restart). The module's host process held 73,940 kB (dev) and 49,780 kB (portable) resident when S5 killed it; the binary's daemon, in the same run natively, 78,376 kB.

### The updater through the module

`updater-e2e.sh`: `start` with the module's configuration (`ferrata_updater_module/dev/recorded.toml`: the
harness ledger, mainnet and Sepolia, two recorded sources each), `status` every 5 s until both instances
were up to date, `metrics`, `stop`. Both flavours: mainnet at slot 15,631,808 with root `530599ec…2328` and Sepolia at 11,556,000 with root `d31b7d34…f3cc`, the lab's final roots (`lez/lab/README.md`), 4 and 35 jobs applied, 187 transactions applied and none refused (`metrics`), both instances healthy, in 118 s (the daemon ran 213 and 666 rounds by the time `status` saw it up to date: with `poll_secs = 0` it goes on reading the chain and the sources while up to date). Natively, `run` until up to date: the same roots in 172 rounds, 119 s. `start` with `ledger = "sequencer"` returns the binary's refusal. `version()` through the module reports `RISC0_DEV_MODE=1` and the program id above.

### Calls through Logos Core, five modules

| Module | Calls | Requests from |
|---|---:|---|
| `ferrata_attest_module` | 11 | the attestation fixtures (as before) |
| `ferrata_bridge_module` | 13 | `bridge/vectors/*-v2.json` (as before) |
| `ferrata_attestor_module` | 12 | the zone run's records: `tier` (= its `tier.json`), `deployment`, `pubkey` (the devnet seq-0 key = its channel key), `certify` (= its `cert-0.json`), `attest` (= its `att-0.json`), `verify` (four), `collect` 3 of 4 (= the proven input) and 2 of 4 (refused), `assemble` (the program's refusal), a certificate of another key (refused), an unreadable request |
| `ferrata_monitor_module` | 8 | `check` on a node view: pass, `tipConflict`, `keySetNotLive`, wait; `node_view` of the lab's record (final height 258); an unbounded `run` refused; an unreadable request |
| `ferrata_updater_module` | 4 | `version`, `check_config` of the module's and the binary's example configuration, one with an unknown field refused |

Every reply through the dev and the portable Logos Core equals the native reply and meets its
expectations; calls take at most 0.2 s (`logosctl call`, process start included). One expectation of the
earlier module was stale and was corrected first: `requests.py` still expected integration6's instance id
for the p1899 checkpoint after lc-a1 added `ws_period_epochs` to the configuration (51d8d80 changed the
configuration and the calls crate's own test, `0x39860f39…b79c`, but not this request).

### Steps (12 November, the final run)

`check.sh` at commit `dcf7502` (the Logos packages cached by the earlier runs on the same box): 765 s in
all; native tests 27.6 s, native calls 1.9 s, the three daemons' jobs natively 153.2 s, `build-lgx` (five)
39.2 s, `build-lgx-portable` 42.5 s; through the dev Logos Core: `e2e.sh` 68.0 s, attestor 4.3 s, updater
118.5 s, monitor 40.0 s; through the portable one: 71.1, 4.6, 118.2 and 38.4 s. From the cold container: the
dev `logosctl` 1,848.6 s (the warm-up run), the portable one 461.0 s; the first complete Nix builds of the
three new crates (their whole dependency trees: alloy with aws-lc-sys, LEZ v0.4.0-rc1 with risc0-zkvm and
blst) were inside one `build-lgx` of 355.9 s for all five modules (it stopped later, at 3 below);
`release-local.sh` for five 45 s once cached (318 s the first time), `catalog-install.sh` with the three jobs
148 s, the live zone 351 s. Steps: [`results/check-2026-11-12/steps.md`](results/check-2026-11-12/steps.md).

### What failed on the way (12 November)

1. **The socket path.** With this worktree's `TMPDIR` (68 bytes), every module's host process died at load:
   Logos Core puts each module's socket in `TMPDIR` (`logos_<module>_<id>`), a Unix socket path holds 107
   bytes, `logos_host` logged "failed to listen" and then crashed on SIGSEGV ("the module process died on
   signal 11 before it reported that it had loaded"). The cold warm-up run lost its portable half to it.
   The scripts that start a Logos Core now refuse a `TMPDIR` (or `LOGOS_TMPDIR`) over 62 bytes.
2. **The scaffold's imports.** The generated `provider_gen.rs` imports `std::sync::Mutex`; the three crates
   imported it too (E0252). Every dependency (alloy with aws-lc-sys, LEZ, risc0-zkvm, blst) had built.
3. **`;` in `metadata.json`.** `LogosModule.cmake` reads the file into an unquoted CMake string; the new
   descriptions' semicolons split it ("logos_module: NAME is required"). `check.sh` now refuses one.
4. **Rust 1.98.1.** The builder's rust-overlay (locked 26 August 2026) stops at 1.98.0; LEZ pins 1.98.1.
   The updater module builds with 1.98.0 (LEZ's crates deny warnings; none was raised).
5. **An empty git history.** The box's copy of the tree has no commits; `git rev-parse HEAD` printed `HEAD`
   before failing, which put a newline into `STAGED.json`. The staging scripts use `--verify -q`.
6. **`Result::err`.** A trait method named `err` on `Result` is shadowed by the inherent one (compile error,
   caught natively).
7. **anvil.** In one run through the module, forge sent the rule (A) vault's two deploy transactions at
   once; anvil 1.3.6 mined the first and kept the vault's pending for good, even through `evm_mine`. That
   run's monitor job was stopped by hand and failed; the test now deploys with `forge script --slow`, and
   the final run passed in both flavours.
8. **Checks of mine.** The calls' module filter in `check.sh` (jq) selected nothing in the first full run,
   and the live zone script's tier check had a jq precedence error (fixed before run 6); in run 6 the
   native calls ran without `RISC0_DEV_MODE`, which the updater module's `version()` reports, so its reply
   differed from the daemon's, and `requests.py`'s p1899 id was stale (both fixed for run 7, `dcf7502`).

### Not done (12 November)

What `README.md` lists under "Left out" for the three modules: the daemons use no Logos Core module for
network or keys (the attestor reads its node over plain HTTP and keeps keys in files; the monitor sends its
own Ethereum transactions with a key file; the updater reads beacon APIs or recorded directories);
attestations are published as files; the monitor still reads node views through its adapter, with no client
of the LEZ or Logos Blockchain nodes' RPCs; the updater module runs only on the in-process chain (no
sequencer backend), so two of them cannot share a chain and the A3 gate remains the binary's test; the
live zone had one Logos Blockchain node for the four Cores and no bridge program, and its collected input
was checked by the rule natively, not proven; nothing published; Linux x86-64 only.

## Completing the two modules (12 November 2026)

**What the modules lacked is built, on the tools' own code, and runs through Logos Core.**
`ferrata_attest_module` gains `advance` (an update's job planned the way the updater plans it);
`ferrata_bridge_module` gains the private claim (`ClaimPrivately`, built and checked from LEZ's state),
`import_root`, `registry`, the release by the wallet's rule (the newest accepted root whose release delay
has passed) and `prove_release` (the release guest through r0vm). No logic is written twice: the `ferrata`
tool's claim builder, release rule, release plan and release guest run moved into `ferrata-wallet`
(`cli/wallet`), the updater's job planning into `ferrata-advance` (`lez/advance`); the tool, the updater
and the modules all link them, and the modules now link the programs' own wire crates and LEZ's
`lee_core` (staged by `scripts/stage.py`), so every message and state is the programs' own type. All of it
ran on the Modal build box (a gVisor container, 2 CPUs reported), repository at commit
`3be4e37` (results in [`results/complete-2026-11-12/`](results/complete-2026-11-12/)):

| What | Where it ran | Result |
|---|---|---|
| unit tests | native | `ferrata-wallet` 3 (the release rule at its edges, the import a claim goes under), the attestation module's calls 8, the bridge module's 6 (one: the rule through the JSON interface, against the explicit root); `ferrata-advance` is the updater's code moved, tested by the updater's tests (7, with the harness, its fee gate within bounds, first round, same code) and by `advance` below |
| `e2e.sh` with `advance` | native and through Logos Core (dev `logosctl`) | 43 of 43 checks each ([native](results/complete-2026-11-12/e2e-native/e2e.md), [Logos Core](results/complete-2026-11-12/logos-dev/e2e/e2e.md)) |
| `bridge_e2e.sh` (the mini-app's devnet) | native and through Logos Core | 24 of 24 each ([native](results/complete-2026-11-12/bridge-e2e-native/bridge_e2e.md), [Logos Core](results/complete-2026-11-12/logos-dev/bridge-e2e/bridge_e2e.md)) |
| `check.sh` (`PORTABLE=0 E2E=1 BRIDGE_E2E=1`) | both modules built and packaged by logos-module-builder, installed and loaded in a headless Logos Core | step failures 0, check failures 0; every static call (25) equal to native ([summary](results/complete-2026-11-12/logos-dev/summary.md)) |
| one release proven for real | through Logos Core: a daemon without `RISC0_DEV_MODE`, `prove_release` | **real** succinct receipt of the release guest `0xb64900bc…29e9` (the image id of [`docs/image-ids.md`](../docs/image-ids.md), built here: binary sha256 `7be1349c…11bf`, 230,360 bytes), verified against it by the module (`dev_mode` false, `image_id_matches` true): 15.0 s of proving on the box's CPU (the `logosctl call` 15.1 s), receipt 584,508 bytes of JSON, sha256 `038d07a5…c14f`, in the form `ferrata-relayer` wraps ([`real-proof/`](results/complete-2026-11-12/real-proof/)) |
| the tools on the shared code | native | the `ferrata` tool's end-to-end test (`cli/tests/e2e.sh`: deposit, scan, claim, burn, release and recovery against anvil and the harness, its release rule included) passes |

Proofs: dev mode (`RISC0_DEV_MODE=1`) everywhere but the one release above. The devnet's private claims
are proven by its harness in dev mode (`FERRATA_DEVNET_PROOFS=real` would prove them for real, as EVIDENCE
row 86 did for the mini-app).

### `advance` (`e2e.sh`, the `lez/` harness's Stage 4 chain)

`module_e2e advance-info` gives the mainnet instance initialized from the attestation fixture's bootstrap,
before its finality update. `advance` takes its state, the fixture's finality update and bootstrap (SSZ,
as a beacon node serves them) and plans the update with `ferrata-advance` (the program's step code run
natively with blst first): useful, finalized slot 15,622,144 → 15,624,480, 505 signers, three steps:
`Open` (4,990 bytes), `Miller`, `Finish` (1 byte each). `module_e2e advance` executes them in order in LEZ
v0.4.0-rc1's state machine: each decoded canonically by the program and applied (execution gas 2,330,188,
3,308,048 and 3,723,960), the instance's finalized slot moves, its record is written, and the instance's
state and the record are byte-equal to what the harness's own job for the same update leaves (the same
job id). `read_instance` reads the advanced state back; the same update again is `no_change`.

### The bridge module against the mini-app's devnet (`bridge_e2e.sh`)

The devnet is `cli/examples/miniapp_devnet.rs` (the v5 vault on anvil, chain id 1, local only, its
verifier the vault tests' mock; the zone in the LEZ harness with the bridge program). It gained reads and
a submission for this test (`module.*`: the zone's states, the vault's leaves and accepted roots, a
synthetic recorded block's proofs, submitting what the module built) and a checkpoint that can leave the
release delay running; it plans nothing.

| Step | What the module did | What the chains did |
|---|---|---|
| two deposits (the tool's) | — | 0.1 ETH each to the user's private account `/0` |
| `import_root` | `ImportRoot` (1,231 bytes) from the block's proofs in `eth_getProof`'s shape, dry-run with the programs' functions: importable, `root(cid, 2)` = the vault's | applied (231,020 execution gas); a wrong count: refused by the dry run and by the program, both 247 (`IMPORT_MISMATCH`) |
| `registry` | `token(cid, ETH)`, `config(cid)`, `root(cid, 2)` decoded; the claim range of leaf 0 (0 to 2) and its fees; `configId` recomputed = the devnet's | — |
| `claim`, private, ×2 | the tool's claim: `ClaimPrivately` (1,239 bytes, 6 accounts, the root the private account's own actor) | proven (dev mode) and settled privately; the private account holds 0.2 ETH; a second claim of deposit 0: refused by the module from the nullifier's state (250, `ALREADY_CLAIMED`) |
| `release` by the rule | burn 0: the older root (its delay passed), not the newer (in its delay); burn 1: `ReleaseDelayNotElapsed` with the time the newer opens; 200 s later: the newer | two private burns (the tool's), two checkpoints (the mock verifier on the exact journals) |
| `prove_release` | the release guest on the module's input, journal = native | — |
| the vault | — | both releases accepted (the mock verifier on the exact journals), 0.1 ETH paid each; `status`: both released |

The devnet's vault pins a placeholder release image id (its verifier is the mock); the release guest the
module ran is the real one.

### What changed to make it build in Logos Core

- The modules link LEZ's `lee_core` (edition 2024) and, the bridge module, the RISC Zero client; both
  build with logos-module-builder's toolchain (rustc 1.89.0) once `ruint` is held at 1.17.2 (1.20 needs
  1.90; the modules' locks were seeded from `cli/Cargo.lock`).
- A daemon whose `TMPDIR` path is long starts but cannot listen (its socket path passes the 107-byte limit
  of a Unix socket): `check.sh` and `real_release_proof.sh` need a short `TMPDIR`.
- `requests.py`'s expected id of `checkpoint_p1899` was stale since lc-a1 added `ws_period_epochs` to the
  configuration (`51d8d80`); it is the id the calls crate's own test gives (`0x39860f39…b79c`).

### Re-run

```sh
cd bridge && cargo build --release --locked -p methods     # the release guest (image id 0xb64900bc…)
RELEASE_ELF=$PWD/target/riscv-guest/methods/ferrata-guests/riscv32im-risc0-zkvm-elf/release/release.bin
cd .. && modules/scripts/e2e.sh OUT1 && RELEASE_ELF=$RELEASE_ELF modules/scripts/bridge_e2e.sh OUT2
TMPDIR=<short dir> PORTABLE=0 E2E=1 BRIDGE_E2E=1 RELEASE_ELF=$RELEASE_ELF modules/scripts/check.sh OUT3
TMPDIR=<short dir> modules/scripts/real_release_proof.sh OUT3/logos/bin/logosctl OUT3/lgx-ferrata_bridge_module $RELEASE_ELF OUT4
```

### Not done

The modules still read and send outside Logos Core's own modules (no `eth_rpc_module`, `keystore_module`,
`tx_sender_module`, `lez_core`): the private claim's private execution is the LEZ wallet's (on the devnet
the harness runs it with the wallet's keys), the import's proofs and every LEZ state come in the request,
and the receipt `prove_release` gives is succinct (the vault takes the Groth16 wrap, the relayer's). The
portable build was not re-run this time; Linux x86-64 only.

## The attestation and bridge modules, 11 November 2026

**Two modules, built, packaged, loaded and called; a log attested end to end through them.** On
11 November 2026, `modules/scripts/check.sh` (repository at commit `8a55838`) built
`ferrata_attest_module` (seven methods) and `ferrata_bridge_module` (seven) with logos-module-builder,
packaged each as a dev and a portable `.lgx`, installed and loaded both together in a headless Logos
Core (`logosctl`, dev and portable builds), and called every method: 24 calls, each reply equal to the
native call and meeting its expectations, in both. Then `e2e.sh` ran through each daemon: logs and a
storage slot of a finalized mainnet block, fetched over JSON-RPC, attested on LEZ by transactions the
module built, and the instance's state read back, 36 of 36 checks each. Step failures 0, check failures 0
([`results/check-2026-11-11/summary.md`](results/check-2026-11-11/summary.md)). The release action's
steps ran locally for both packages ([`catalog/`](catalog/README.md)).

### Where it ran

The project's Modal build box (workspace `ws-12`), a gVisor container (`Linux 4.19.0-gvisor x86_64`,
64 GiB; `nproc` reported 2, the box allows up to 32 threads), started cold for this worktree: Nix
2.28.3, cargo 1.93.0, Python 3.12.1, jq 1.7. Builder, SDK, `logosctl` and nixpkgs at the revisions of
the first run (below); Logos packages from source (the Logos binary cache was not used). On the laptop
only edits, `git`, and reads of public Logos repositories. Every number below is from the runs' own
output, files under `results/check-2026-11-11/`, `e2e/native/` and `catalog/dry-run/`.

### Methods

| Module | Methods (`logosctl module show`: the trait's methods with their descriptions) |
|---|---|
| `ferrata_attest_module` | `version()`, `dry_run_attest`, `verify_checkpoint`, `fetch_log_proof`, `build_attest_tx`, `read_attestation`, `read_instance` (each `(QString)` but `version`) |
| `ferrata_bridge_module` | `version()`, `deposit`, `claim`, `burn`, `release`, `recover`, `status` |

Interfaces: [`README.md`](README.md).

### Calls through Logos Core (24, dev and portable)

| Requests | Source | Calls | Result |
|---|---|---:|---|
| `requests.py` | the attestation fixtures (mainnet block 26,384,851; the `lez/` harness's checkpoint) | 11 | the first run's 11, unchanged: attested, refused with 110 and 140, checkpoints ok and failed |
| `bridge_requests.py` | `bridge/vectors/*-v2.json`, the vectors the bridge core, program, guests and vault are tested against | 13 | `deposit` ×3 (commitment, leaf, deposit id, nullifier = the vectors'), `claim` (nullifier, root, path = the vector's) and a refusal (`RootMismatch`), `burn` ×3 (tag, memo, MAC, `C_out`, leaf, burn id, nullifier = the vectors'), `release` (nullifier, burn root, the 352-byte journal and its SHA-256 = the vector's) and a refusal, `recover` (finds the wallet's deposit, not the other wallet's), `status` (claimed, claimable, awaiting root; released, releasable, awaiting checkpoint) |

Every reply through the dev and the portable Logos Core equals the native reply (JSON, key order
ignored). Calls take 0.0–0.1 s (`logosctl call` wall clock, process start included).

### A log end to end (`e2e.sh`, 36 checks; native, dev and portable: each 36 of 36)

`recorded_rpc.py` serves the recorded block 26,384,851 (`eth_getBlockByNumber`,
`eth_getBlockReceipts`) on 127.0.0.1; the chain is the `lez/` harness's Stage 4 (the mainnet instance
`0xc112f292…23bfe0`, initialized from the fixture's bootstrap, its finality update verified through the
public job, in LEZ v0.4.0-rc1's state machine and fee settlement); `lez/harness/src/bin/module_e2e.rs`
reads the record the program holds at `header(id, slot)`, and decodes and executes what the module
built.

| Case | What the module did | What the program did |
|---|---|---|
| the instance's state, read at `instance(id)` | `read_instance`: chain 1, store `Electra`, finalized slot 15,624,480, 2 jobs applied (activation and the update); the id recomputed from the decoded `Config` and the checkpoint equals the instance's | — |
| USDC transfer (tx 0, log 0) | 2 JSON-RPC calls; the 531 receipts rebuild the block's `receiptsRoot`; 3-node proof; dry run on the record read from the chain: attested, code 0; `Attest` message 2,121 bytes at `header(id, slot)` = `0xc9d33e28…cce5` | decoded the bytes (canonical Borsh), applied: 92,295 execution gas, 2,519 storage gas; its event = its native evaluation; statement id = the dry run's |
| WETH transfer in the block's largest receipt (306 logs; by block-wide `logIndex` 7) | as above; message 58,310 bytes | applied: 1,614,747 execution gas, 58,708 storage gas; ids equal |
| WETH balance slot (the fixture's `eth_getProof`) | dry run: attested, the provider's value; the program's two statement ids (the account, then the slot); message 6,906 bytes | applied: 273,546 execution gas, 7,304 storage gas; ids equal |
| tampered receipt proof | dry run refuses with 110 (`ProofNodeMismatch`) | refuses with exit 110 (67,401 execution gas) |
| a request naming another instance | (a check only the program makes) | refuses with exit 211 (`WRONG_INSTANCE`) |
| bridge `Burn` (outbound vector case 0) | note = the vector's; 185-byte message | the bridge program's `BridgeMessage` decodes it canonically to the same note |
| bridge `Claim` (the inbound claim vector, public destination) | claimable; nullifier, root, path = the vector's; 1,271-byte message | decodes canonically to the same witness; the fee range covers the leaf |

`read_attestation` decodes each event and matches it with the dry run's ids and the instance.
The harness's instance id equals the id `verify_checkpoint` computes for the attestation fixture's
checkpoint (`checkpoint_attest_fixture`), which the first run could not confirm. The recorded server
answered 4 requests per run (3,301,898 bytes). The e2e took 55.7 s (dev) and 51.9 s (portable), most
of it the harness rebuilding the Stage 4 chain for each transaction; each module call 0.04–0.10 s.

### Packages and the release action's steps

| Artefact | Bytes |
|---|---:|
| `ferrata_attest_module` dev / portable `.lgx` | 2,123,682 / 5,280,230 |
| `ferrata_bridge_module` dev / portable `.lgx` | 2,021,508 / 5,180,044 |
| Nix closure of each dev package | 667,834,808 / 667,482,016 |
| Nix closure of the dev `logosctl` | 907,405,600 |

`release-local.sh` (commit `8a55838`): each module exported as its own repository (39 and 20 files),
`nix build .#lgx-portable` (47 s and 49 s, dependencies cached; the same packages as `check.sh`'s, 5,280,230
and 5,180,044 bytes),
`lgx verify` (`logos-package` `4bdd904`, `lgx 0.1.0`): "Package structure is valid", unsigned; the
sidecars (sha256 `dce2086a…` and `35ed5eb1…`, root hashes, manifests); `index.json` built by
`logos-modules-release-tool` `v1.3` (`94462e6`) `index.py`: schema 2, repository `ferrata-modules`,
2 packages; `index.py validate-repo` on `catalog/logos-repo.json`: OK. Nothing was published.

### Installed from the catalog (`catalog-install.sh`)

A headless Logos Core installed both modules *from the catalog* as a client does: the dry run's
`index.json` and packages and `catalog/logos-repo.json` served on 127.0.0.1 over HTTPS (`logosctl`
refuses a plain `http://` catalog: "unsupported URL scheme (https required in v1)") with a throwaway
self-signed certificate given to the daemon as its CA bundle; portable `logosctl` of the final run.
`catalog add` the catalog's `logos-repo.json`, `catalog source http`, `search ferrata` (both found),
`install` each by name (downloaded over HTTPS, root hashes from the index), `module load`, `call version`
(both answer, staged from commit `8a55838`). The server logged the index and the two packages fetched.
One command failed, for a reason outside this catalog: `catalog refresh` also fetches the daemon's
built-in official catalog (GitHub), whose certificate cannot be verified with the throwaway CA bundle
(curl error 60), even after `catalog disable`. Log: `catalog/install-test/log.txt`.

### Steps (final run, seconds)

Everything cached by the earlier runs on the same box: `build-lgx` (both modules) 94.7, `build-lgx-portable`
21.2, daemon start, install and load under 1 each, `dev-e2e` 55.7, `portable-e2e` 51.9; whole run 286 s.
From the cold box, the first command re-ran the first run's `check.sh` (the prototype at `17030d4`):
SDK source 53.7 s, `build-lgx` (the module and every Logos dependency) 511.5 s, `logosctl` (dev) 1,573.0 s,
portable package 11.2 s, portable `logosctl` 518.3 s; its 11 calls again equal to native in dev and
portable (its summary counts 2 failures only because `summary.py` changed under it to require the e2e,
which that script did not run). The harness's test build: 456 s.

### What failed on the way (all fixed, re-run)

1. `check.sh`'s default output `modules/out` was not ignored by git, so the box copied it back; now ignored.
2. With a tab `IFS`, `read` collapsed the empty request field of the `version` calls, which then got the
   wrong arguments (`INVALID_ARGS`); a call without a request is now written `-`.
3. Two `e2e.sh` runs on one box collided on the RPC server's port; each run now takes its own.
4. The dry run predicted only the slot's statement id for a storage request; the program also attests
   the account as a fact of its own. `dry_run_attest` now returns `ids`, the program's list in its order.
5. `release-local.sh` read Nix's progress lines as the out path, and `index.py` named the repository
   `unknown` outside the catalog's directory; both fixed.

### Not done

What `README.md` lists under "Left out": the modules do not go through Logos Core's Ethereum, key and
transaction modules (plain `http://` JSON-RPC; messages built, not signed or sent; the harness executes
them in-process, not a networked sequencer); no `advance` in the attestation module; no private claim, `ImportRoot`, registry reads or release proof in the bridge module (all built on 12 November, above); the
catalog is laid out, dry-run and installed from locally, not published; Linux x86-64 only.

## First run, 10 November 2026: the prototype (`ferrata_attest_module`, three methods)

**Built, loaded and answering.** On 10 November 2026, `modules/scripts/check.sh` (repository at commit
`c65aa32`) built the module with logos-module-builder as a Rust-first module, packaged it as a dev and
a portable `.lgx`, installed each into a headless Logos Core (`logosctl` daemon, dev and portable
builds), loaded it, and called its three methods with 11 requests. Every reply through Logos Core
equals the reply of the same call made natively (the methods' crate without Logos Core), and meets
what the request must produce. Step failures 0, check failures 0.

### Where it ran

A Modal cloud container (the project's build box), started fresh: a gVisor sandbox that reports `Linux 4.19.0-gvisor x86_64`, up to 32 cores and 64 GiB,
Ubuntu 24.04 image. Nix 2.28.3 (pinned, see [What failed](#what-failed-on-the-way)), cargo 1.93.0,
Python 3.12.1, jq 1.7. On the laptop only `requests.py` ran (to check its output), and one request to
the Logos cache. Every number below
comes from that run's report (`summary.md`, `steps.md`, the step logs), except where marked.

| Input | Revision |
|---|---|
| logos-module-builder (the module's flake input) | `5f74cdb3cecacde054c68d72fcc0d2cfe2195900` |
| logos-rust-sdk (from the builder's lock) | `040e94e83873249dc5f27da8c2a01c3dcaaa630d` |
| logos-logoscore-cli (`logosctl`; it reports `pre-release-66d098d`) | `66d098d3af1367cf831b69c13a5aa8345edeb682` |
| nixpkgs (from the module's `flake.lock`) | `e9f00bd893984bc8ce46c895c3bf7cac95331127` |
| ferrata-core, staged by `stage.sh` (`version()` reports it) | 0.0.1, sources SHA-256 `36f83f06…bb6c5b` |

The module's `flake.lock` and `rust-lib/Cargo.lock` are committed; both were written on the box by the
first runs.

### Steps (seconds, wall clock)

| Step | Seconds |
|---|---:|
| stage ferrata-core; SDK source from the builder; requests | 0.1; 19.6; 0.1 |
| native unit tests (5 passed: Borsh length, instance id, hex, unreadable requests, chain pairs) | 2.9 |
| native calls (11) | 0.2 |
| `nix build .#lgx` (module and every Logos dependency from source; Qt and the rest from cache.nixos.org) | 482.1 |
| `nix build logos-logoscore-cli#ctl` (from source) | 1632.0 |
| dev: daemon start, install, load, `module show`, 11 calls, `module ls`, daemon stop | 0.4, 0.4, 0.1, 0.1, ≈0.1 each, 0.1, 0.6 |
| `nix build .#lgx-portable` (after the dev build) | 13.8 |
| `nix build logos-logoscore-cli#ctl-bundle-dir` (after the dev build) | 616.6 |
| portable: daemon start, install, load, `module show`, 11 calls, `module ls`, daemon stop | 0.4, 0.7, 0.1, 0.1, ≤0.1 each, 0.1, 0.6 |

Whole run: 2,796 s, 9.45 core-hours, about 0.59 USD (the build box's estimate). Call times are `logosctl call`
wall clock, process start included, at 0.1 s resolution.

`logosctl module show` lists `version()`, `dry_run_attest(QString)` and `verify_checkpoint(QString)`
with the descriptions written on the Rust trait: the builder derived the contract from the trait and
the host sees it.

### Calls

Each request is a file that `requests.py` writes from the repository's fixtures, passed as
`logosctl call ferrata_attest_module METHOD @FILE`. "Equal" is the reply through Logos Core compared
with the native reply (JSON, key order ignored), for the dev and the portable package.

| Call | Method | Verdict | Code | Equal (dev, portable) |
|---|---|---|---:|---|
| `version` | `version` | `ferrata_attest_module` 0.1.0, ferrata-core 0.0.1, no BLS backend | | yes, yes |
| `attest_weth_balance` | `dry_run_attest` | attested | 0 | yes, yes |
| `attest_empty_slot` | `dry_run_attest` | attested | 0 | yes, yes |
| `attest_absent_account` | `dry_run_attest` | attested | 0 | yes, yes |
| `refuse_tampered_proof` | `dry_run_attest` | refused, `ProofNodeMismatch` | 110 | yes, yes |
| `refuse_other_block` | `dry_run_attest` | refused, `ProofNodeMismatch` | 110 | yes, yes |
| `refuse_wrong_chain` | `dry_run_attest` | refused, `ChainMismatch` | 140 | yes, yes |
| `checkpoint_p1899` | `verify_checkpoint` | ok | 0 | yes, yes |
| `checkpoint_attest_fixture` | `verify_checkpoint` | ok | 0 | yes, yes |
| `checkpoint_wrong_root` | `verify_checkpoint` | failed: bootstrap `CheckpointMismatch`, observed root disagrees | 10 | yes, yes |
| `checkpoint_chain_mismatch` | `verify_checkpoint` | failed: chain id and `genesis_validators_root` do not belong together | | yes, yes |

What the replies say:

- **Storage, mainnet block 26,384,851** (hash `0x3e02e5ba…cba7bce`), against the record the program
  writes for the fixture's finalized header (anchor slot 15,624,480): WETH's balance slot for the
  holder is `0xf22ca9b4cc5d0f3d61`, as the provider said (`provider_value_matches: true`), statement
  id `0xabc1114e…1dfa53`. An empty slot of WETH and slot 0 of an absent account are attested as zero.
- **Refusals** carry the core's codes, which the program returns unchanged
  (`lez/wire/src/lib.rs`, `refusal`): one nibble changed in the account's leaf node, and the same
  slot's proof from block 26,384,835 against block 26,384,851's record, are refused with 110
  (`ProofNodeMismatch`); a consumer configured for Sepolia's chain id against a mainnet record with
  140 (`ChainMismatch`).
- **Instance id.** For the `lez/` harness's mainnet instance (`mainnet_config(2)`, checkpoint
  `0x64b56a37…7d91ac`) the module computes `0xce624027489c721108da1cbaeb5eb16990111f6908282949e05108e7cb2afdfb`,
  the id the program wrote into its own transactions (`lez/results/stage3-read-tx.hex`; prefix
  `ce624027489c` in `lez/results/prove-private-dry-run.log`). The module encodes `Config` itself
  (the program's crate links LEZ); this is its one check against the program. For the attestation
  fixture's checkpoint it computes `0xc112f292…23bfe0`, which nothing recorded can confirm.
- **Bootstrap.** For both checkpoints, the checks of the program's `Init` (`StoreCore::from_bootstrap`:
  layout, header validity, header root against the checkpoint, committee branch) accept the
  bootstrap: slot 15,556,608 and slot 15,622,144. With the update's finalized root given as the
  checkpoint instead, they refuse it with 10 (`CheckpointMismatch`).
- **Observed roots.** The root a second beacon provider reported for each checkpoint's slot when the
  fixtures were fetched (publicnode for the attestation fixture; `EXPECTED_FINALIZED` in
  `lez/harness/src/lib.rs` for the other) agrees with the checkpoint; the wrong checkpoint disagrees.

### Sizes

| Artefact | Bytes |
|---|---:|
| dev package `logos-ferrata_attest_module-module-lib.lgx` | 2,034,156 |
| its plugin, `variants/linux-amd64-dev/ferrata_attest_module_plugin.so` (uncompressed) | 5,680,168 |
| its contract, `assets/lidl/ferrata_attest_module.lidl` | 721 |
| portable package (`.#lgx-portable`) | 5,192,816 |
| Nix closure of the dev package | 667,516,992 |
| Nix closure of the dev `logosctl` | 907,397,392 |

### What failed on the way

All of these came from the build box's container, not from the module; none was tried on another
Linux machine. The workarounds for 3, 4 and 6 are in the box's set-up, which is not part of the
repository; `check.sh` holds those for 2 and 5.

1. **The Logos binary cache** (`cache.nix.logos.co`, named by `logos-co/setup-nix-cache-action`)
   timed out from the box and from the laptop (curl exit 28 after 15 s). Every Logos package was
   built from source, hence the 482 s and 1,632 s builds.
2. **Evaluation stack.** With the container's `unlimited` stack limit, Nix's evaluation of the
   module's flake stopped with `stack overflow (possible infinite recursion)`; with a 1 GiB limit it
   evaluates. `check.sh` sets that limit.
3. **Nix 2.35.6**, the current release, failed every local build (`unexpected EOF reading a line`):
   after the builder exits it calls `fchmodat2`, which gVisor does not implement (`ENOSYS`, seen with
   strace). Nix 2.28.3 does not call it.
4. **Builder output.** Nix reads each builder's output through a pseudoterminal; in this container
   it often never received the builder's first line (`reading a line: Input/output error`): 6 of 6
   trivial builders failed, and one package build failed 7 times before passing. An `LD_PRELOAD`
   library that gives Nix a pipe instead of a pseudoterminal fixed it (8 of 8 trivial builders).
5. **Builds as root.** With Nix build users every local build failed too, so builds ran as root, and
   a cargo build then leaves `/homeless-shelter`, after which Nix refuses to start builds.
   `check.sh` removes it and retries, only on this failure and on 4's.
6. **Memory.** One run's container died while building `logosctl` with 8 parallel builds of 16 cores
   each; with 4 builds of 8 cores the run completed.

Modal spend for all of this work, including the failed runs, probes and a clean-up: 23 calls,
3,442 s, about 0.82 USD (the build box's estimates).

### Not done

- **The log path end to end.** `dry_run_attest` takes a log and its receipt proof
  (`attest_log`), but the fixture holds the block's receipts, not a proof, so no log was attested.
- What `modules/README.md` lists as left out: no network, no transactions, no program-only checks
  (`WRONG_INSTANCE`), no batches, hops or accumulator leaves, no `KeyValidate`, no catalog release,
  Linux x86-64 only.
- `version()` reports the commit `check.sh` was given (`FERRATA_COMMIT`, else `git rev-parse`); the
  sources hash is what identifies the ferrata-core that was built.

### Reproduce

On Linux x86-64 with Nix (flakes), cargo, python3 and jq: `modules/scripts/check.sh [OUT_DIR]`
(`PORTABLE=0` skips the portable package). In a gVisor container like the one above, Nix needs the
set-up described under [What failed](#what-failed-on-the-way). The report is `OUT_DIR/report/summary.md`.