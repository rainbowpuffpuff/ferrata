# Logos modules: results

**Two modules, built, packaged, loaded and called; a log attested end to end through them.** On
11 November 2026, `modules/scripts/check.sh` (repository at commit `8104361`) built
`ferrata_attest_module` (six methods) and `ferrata_bridge_module` (seven) with logos-module-builder,
packaged each as a dev and a portable `.lgx`, installed and loaded both together in a headless Logos
Core (`logosctl`, dev and portable builds), and called every method: 24 calls, each reply equal to the
native call and meeting its expectations, in both. Then `e2e.sh` ran through each daemon: logs and a
storage slot of a finalized mainnet block, fetched over JSON-RPC, attested on LEZ by transactions the
module built, 35 of 35 checks each. Step failures 0, check failures 0
([`results/check-2026-11-11/summary.md`](results/check-2026-11-11/summary.md)). The release action's
steps ran locally for both packages ([`catalog/`](catalog/README.md)).

## Where it ran

The project's Modal build box (workspace `ohyou`), a gVisor container (`Linux 4.19.0-gvisor x86_64`,
64 GiB; `nproc` reported 2, the box allows up to 32 threads), started cold for this worktree: Nix
2.28.3, cargo 1.93.0, Python 3.12.1, jq 1.7. Builder, SDK, `logosctl` and nixpkgs at the revisions of
the first run (below); Logos packages from source (the Logos binary cache was not used). On the laptop
only edits, `git`, and reads of public Logos repositories. Every number below is from the runs' own
output, files under `results/check-2026-11-11/`, `e2e/native/` and `catalog/dry-run/`.

## Methods

| Module | Methods (`logosctl module show`: the trait's methods with their descriptions) |
|---|---|
| `ferrata_attest_module` | `version()`, `dry_run_attest`, `verify_checkpoint`, `fetch_log_proof`, `build_attest_tx`, `read_attestation` (each `(QString)` but `version`) |
| `ferrata_bridge_module` | `version()`, `deposit`, `claim`, `burn`, `release`, `recover`, `status` |

Interfaces: [`README.md`](README.md).

## Calls through Logos Core (24, dev and portable)

| Requests | Source | Calls | Result |
|---|---|---:|---|
| `requests.py` | the attestation fixtures (mainnet block 26,384,851; the `lez/` harness's checkpoint) | 11 | the first run's 11, unchanged: attested, refused with 110 and 140, checkpoints ok and failed |
| `bridge_requests.py` | `bridge/vectors/*-v2.json`, the vectors the bridge core, program, guests and vault are tested against | 13 | `deposit` ×3 (commitment, leaf, deposit id, nullifier = the vectors'), `claim` (nullifier, root, path = the vector's) and a refusal (`RootMismatch`), `burn` ×3 (tag, memo, MAC, `C_out`, leaf, burn id, nullifier = the vectors'), `release` (nullifier, burn root, the 352-byte journal and its SHA-256 = the vector's) and a refusal, `recover` (finds the wallet's deposit, not the other wallet's), `status` (claimed, claimable, awaiting root; released, releasable, awaiting checkpoint) |

Every reply through the dev and the portable Logos Core equals the native reply (JSON, key order
ignored). Calls take 0.0–0.1 s (`logosctl call` wall clock, process start included).

## A log end to end (`e2e.sh`, 35 checks; native, dev and portable: each 35 of 35)

`recorded_rpc.py` serves the recorded block 26,384,851 (`eth_getBlockByNumber`,
`eth_getBlockReceipts`) on 127.0.0.1; the chain is the `lez/` harness's Stage 4 (the mainnet instance
`0xc112f292…23bfe0`, initialized from the fixture's bootstrap, its finality update verified through the
public job, in LEZ v0.4.0-rc1's state machine and fee settlement); `lez/harness/src/bin/module_e2e.rs`
reads the record the program holds at `header(id, slot)`, and decodes and executes what the module
built.

| Case | What the module did | What the program did |
|---|---|---|
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
answered 4 requests per run (3,301,898 bytes). The e2e took 52 s through each daemon; each module
call 0.04–0.10 s.

## Packages and the release action's steps

| Artefact | Bytes |
|---|---:|
| `ferrata_attest_module` dev / portable `.lgx` | 2,116,091 / 5,273,170 |
| `ferrata_bridge_module` dev / portable `.lgx` | 2,021,347 / 5,179,992 |
| Nix closure of each dev package | 667,804,688 / 667,481,848 |

`release-local.sh` (commit `d7e6dab`, the same module code): each module exported as its own
repository (39 and 20 files), `nix build .#lgx-portable` (53 s and 51 s, dependencies cached),
`lgx verify` (`logos-package` `4bdd904`, `lgx 0.1.0`): "Package structure is valid", unsigned; the
sidecars (sha256 `833b736b…` and `b90d8ecf…`, root hashes, manifests); `index.json` built by
`logos-modules-release-tool` `v1.3` (`94462e6`) `index.py`: schema 2, repository `ferrata-modules`,
2 packages; `index.py validate-repo` on `catalog/logos-repo.json`: OK. Nothing was published.

## Steps (final run, seconds)

Everything cached by the earlier runs on the same box: `build-lgx` (both modules) 91.8, `build-lgx-portable`
19.7, daemon start, install and load under 1 each, `dev-e2e` 52.0, `portable-e2e` 52.5; whole run 280 s.
From the cold box, the first command re-ran the first run's `check.sh` (the prototype at `0523f77`):
SDK source 53.7 s, `build-lgx` (the module and every Logos dependency) 511.5 s, `logosctl` (dev) 1,573.0 s,
portable package 11.2 s, portable `logosctl` 518.3 s; its 11 calls again equal to native in dev and
portable (its summary counts 2 failures only because `summary.py` changed under it to require the e2e,
which that script did not run). The harness's test build: 456 s.

## What failed on the way (all fixed, re-run)

1. `check.sh`'s default output `modules/out` was not ignored by git, so the box copied it back; now ignored.
2. With a tab `IFS`, `read` collapsed the empty request field of the `version` calls, which then got the
   wrong arguments (`INVALID_ARGS`); a call without a request is now written `-`.
3. Two `e2e.sh` runs on one box collided on the RPC server's port; each run now takes its own.
4. The dry run predicted only the slot's statement id for a storage request; the program also attests
   the account as a fact of its own. `dry_run_attest` now returns `ids`, the program's list in its order.
5. `release-local.sh` read Nix's progress lines as the out path, and `index.py` named the repository
   `unknown` outside the catalog's directory; both fixed.

## Not done

What `README.md` lists under "Left out": the modules do not go through Logos Core's Ethereum, key and
transaction modules (plain `http://` JSON-RPC; messages built, not signed or sent; the harness executes
them in-process, not a networked sequencer); no `advance` or configuration reads in the attestation
module; no private claim, `ImportRoot`, registry reads or release proof in the bridge module; the
catalog is laid out and dry-run, not published; Linux x86-64 only.

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