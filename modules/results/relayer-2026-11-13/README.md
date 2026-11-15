# The relayer's root imports and its Logos module (13 November 2026)

What ran: `ferrata-relayer` with root imports (`--import-roots`, `cli/src/relayer.rs` `plan_root_import`, the
daemon `cli/src/relayerd.rs`) and `ferrata_relayer_module` (`modules/ferrata_relayer_module/`, the same daemon
code inside Logos Core), on the devnets the relayer's and the modules' tests use. Where: the Modal build box
(gVisor, Linux x86-64, Nix 2.28.3, Logos Core built from source at the pins of `modules/scripts/check.sh`), the
sources of commit `f3dc299` (synced to the box; the box's own git has no commits, so the staged `STAGED.json`
says `box`). Measured, except where a line says otherwise. Nothing was sent to a public network: the fork reads
public Sepolia state through anvil.

## Re-check

```sh
MODULES=ferrata_relayer_module modules/scripts/check.sh OUT      # Linux x86-64, Nix, Foundry, r0vm, cargo 1.98.1
modules/scripts/relayer-e2e.sh OUT                               # the binary only: any Linux with cargo and Foundry
```

## `check.sh` with `MODULES=ferrata_relayer_module`: 0 step failures, 0 check failures

[`check-summary.md`](check-summary.md), [`check-steps.md`](check-steps.md):

| Step | Result |
|---|---|
| staging (`stage-relayer.sh`), the lockfile from `cli/Cargo.lock` (617 packages: no RocksDB, LEZ wallet or libp2p) | ok |
| the calls crate's unit tests, natively (`cargo +1.98.1 test --release --locked`) | ok, 4 tests |
| five one-shot calls (`daemon_requests.py`: `version`, the binary's parser's refusals, an endless `run` refused, `status` without `--eth-rpc` or `--vault`), natively and through each Logos Core | each equal to the native reply, each meeting its expectation |
| `relayer-e2e.sh` with the binary (the reference) | 10 passed, 0 failed ([`native/`](native/summary.md)) |
| logos-module-builder: dev `.lgx` 14,318,774 bytes (179 s), portable 17,477,514 bytes ([`packages.txt`](packages.txt)) | ok |
| Logos Core dev and portable (`logosctl`): install, load, the calls, `relayer-e2e.sh` through the module in a Logos Core of its own | 11 passed, 0 failed in each ([`dev/`](dev/summary.md), [`portable/`](portable/summary.md)) |

## The relayer's job, binary and module

`relayer-e2e.sh` (the binary, then `ferrata_relayer_module` through `scripts/relayer-module.sh`, which hands the
binary's arguments to the module and prints its lines):

- **The Sepolia fork** (`cli/tests/relayer_fork.rs`, unchanged but for `FERRATA_RELAYER_CMD`): an anvil fork at
  the block before `crown-live-1`'s `submitCandidate`, that run's real proofs. The same transactions and gas in all
  three runs and in the committed binary run (`e2e/runs/relayer-fork/results.json`, EVIDENCE row 46): candidate
  375,281 gas, `finalizeAttested` 218,385 after W, release 358,908, the paid release wrapped by the relayer from the
  holder's succinct receipt 407,052 (2 × 10¹² wei received; the wrap 17.2–18.8 s on the box's CPU in these three runs, the Groth16
  prover image stood in by `cli/tests/wrap-stub.sh` with the proof it made on Modal), the refusals, `status`
  (1 checkpoint), a clean stop on SIGTERM. Through the module: 34 daemon lines, as the binary's.
- **Root imports on the mini-app's devnet** (`cli/examples/miniapp_devnet.rs`: the v5 vault on anvil, chain id 1;
  the zone in the LEZ harness with the bridge program, dev mode). The relayer reads the zone through the devnet's
  `zone.applied` and `zone.actor_state` and sends through `module.submit` (`--lez-devnet`, a stand-in for a
  sequencer). The light client's finality update is the devnet's `lc.finalize` (stand-in): anvil's newest block
  recorded at `header(id, slot)` with its own number, hash, parent hash, receipts root and time, and its header's
  state root, which equals the root of the trie anvil's `eth_getProof` answers from in every record
  (`devnet-calls.jsonl`: `proofs_root_is_header_root`), and `instance(id)` moved to that slot. The proofs the
  relayer imports with are anvil's own `eth_getProof`.
  - no light client on the zone: `root-import-waiting`, nothing sent;
  - a record of 2 deposits: `ImportRoot` n = 2 applied (273,511 execution gas, 2,677 storage gas natively; 275,460
    and 2,709 through the modules: the proofs are the anvil chain's at that block); its message is byte for byte
    `ferrata_bridge_module`'s `import_root` for the same record and proofs (2,151 to 2,183 bytes);
  - up to date: no line, nothing sent; a third deposit not yet recorded: `root-import-waiting`;
  - a record whose state root the proofs do not match: `root-import-refused` with exit code 110 (the core's
    `ProofNodeMismatch`), the code `ferrata_bridge_module`'s `import_root` gives for it, and nothing sent;
  - the next honest record: n = 3 (`previous` 2);
  - the daemon on the default cadence (`--import-every 3600`): its period done, a fourth deposit and its record are
    not imported within the same hour of anvil's clock; once anvil's clock moves into the next hour, n = 4 is
    imported; SIGTERM stops it cleanly (`stop`, reason `SIGTERM`).

## The relayer's own suite after the move of its daemon

`cli/tests/relayer.sh` in full on the same box after these changes ([`relayer-suite/`](relayer-suite/test-results.txt),
154 s): `cli/`'s 11 unit tests (6 of them the relayer's), the gas-drop checks on the real receipt
(`tests/gasdrop.rs`), `relayer_fork.rs` with the binary, and the harness end to end with the gas-drop guest: the
claim naming the relayer mints it 10¹⁶ wei, a release through the relayer 134,973 gas paying it 10¹⁴ wei, the
redirected copy refused, the private round trip (EVIDENCE rows 46–48's figures). Two fresh-box faults of the script
fixed on the way (`9cb3212`, `da3627c`): the guest lookup failed under `pipefail` when `bridge/target` did not exist,
and the guests were built with `RISC0_HOME` empty instead of unset.

## The catalog

`release-local.sh` for the relayer alone (`DRY_RUN_DIR`): exported as its own repository, `nix build
.#lgx-portable` (204 s), `lgx verify` valid and unsigned, the sidecar, `index.json` by `logos-modules-release-tool`
(1 package), `validate-repo` OK: [`../../catalog/dry-run-relayer/`](../../catalog/dry-run-relayer/index.json).
`catalog-install.sh` (`INSTALL_TEST_DIR`): a portable Logos Core added the catalog served over HTTPS on 127.0.0.1,
found the module (`search ferrata`), installed it by name, loaded and called it, and the installed module ran the
root imports above, 10 passed, 0 failed:
[`../../catalog/install-test-relayer/log.txt`](../../catalog/install-test-relayer/log.txt). Its one failed step,
`catalog refresh`, is the default upstream catalog's certificate under the local CA bundle, as in the five modules'
install (`../../catalog/install-test/log.txt`).

## Not shown here

The relayer's LEZ side on a running sequencer: the binary has it (`--sequencer`, LEZ's wallet) but it has not run
with root imports; the module has no LEZ wallet (its LEZ side is the devnet's zone). On the devnet no sync committee
writes the records. A Gloas record is waited on, not imported. The five other modules were not rebuilt in this run
(the shared scripts' changes only add the sixth module's entries).
