# LEZ v0.4.0-rc1 → v0.4.0-rc2: what changed for Ferrata

Ferrata's LEZ work (`../../lez/`: the light-client and attestation program, its consumer, the
bridge program, the harness, and `lez/live/` on a running sequencer) was built and measured
against LEZ `v0.4.0-rc1` (commit `1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21`). LEZ tagged
`v0.4.0-rc2` on 9 November 2026 (commit `e3edf47402a82557103ca719b30d0bd2684338d1`), 59 commits
later (`git log --oneline v0.4.0-rc1..v0.4.0-rc2`, 18 of them merges). This note lists every
change, says whether it touches what Ferrata uses, and how. It was written by reading the diff
(`git diff v0.4.0-rc1 v0.4.0-rc2`) before building anything; the last section records what the
build against rc2 then confirmed or added. Measurements are in `../../lez/RESULTS.md` (section
"LEZ v0.4.0-rc2") and `../../lez/live/README.md`.

## What did not change

Read in the diff as unchanged (`git diff --stat v0.4.0-rc1 v0.4.0-rc2 -- <path>` is empty):
the block application Ferrata's live driver uses (`lez/chain_state/src/apply.rs`:
`apply_block`, `settle_transaction`, `opening_fee_state`), the pre-genesis state
(`lez/testnet_initial_state`), the `programs` crate's sources and manifest, the manifests of
`lee` and `lee_core` (so their dependencies), `test_guest_core` (the `scripted` test program's
message), the sequencer's network configurations (`lez/sequencer/service/networks/`) and its
RPC (`lez/sequencer/service/rpc`). The host toolchain (Rust 1.98.1), RISC Zero (3.0.5) and the
Bedrock node LEZ pins (`logos-blockchain` at `d480efc6`, 62 references in each `Cargo.lock`)
are the same. In `lee_core`, everything a program sees (its input, transitions, messages,
actors, PDAs, the state-reply idiom) is unchanged: the only `lee_core` change is in the loader's
own types (below). The wallet's entry points the driver calls keep their signatures
(`send_pub_tx_paid_by`, `send_privacy_preserving_tx_with_pre_check`, `ProgramLoader::deploy`,
`set_dry_run`, `set_config`, `new_init_storage`, `new_update_chain`), and `DEFAULT_GAS_LIMIT` is
still 2,000,000.

## Summary

| area | commits | our programs | harness | live driver and scripts |
|---|---|---|---|---|
| program loader: tagged loader state, authorised segment writes, per-byte charge, genesis image check | `e3edf474` (#943) | source unchanged; images change (paths) | one call renamed (`ProgramHeader::from_bytes`) | two calls renamed; deployment now pays execution gas |
| privacy circuit: one image witness per account | `e3edf474` (#943) | none | the circuit image changes: no rc1 receipt verifies on rc2 | the real proof must be made with rc2's circuit |
| builtin program and circuit artifacts rebuilt | `e3edf474`, `a016949f`, `3d3bac3e` | none | builtin image ids change (genesis) | same |
| wallet: measured gas limits, refusal to send a reverting transaction, dry-run report, fee headroom renamed | `38456c72`, `acf6fba7`, `5dc61eb3`, `49542202`, `1f7afc01`, `df52f773`, `00746e09` (#1035) | none | none (the harness does not use the wallet) | the scripted refusals are refused by the wallet unless allowed; the declared gas limits change unless pinned; `DryRun` carries a report |
| fees: `fee_for` factored out | `df52f773` | none | none (same formula) | none |
| Bedrock test chain: 4 s slots, confirmation depth 3 | `9d80a89a`, `616108d5` (#1063) | none | none | the slot clock and epoch phase of `run-live.sh` and `phase.py`; finality latency |
| sequencer: bounded node requests | `b0c15a2c` (#1076), `7c59ef5d`, `c743e760` (#1071) | none | none | robustness only; no interface change |
| sequencer: metrics, dashboards, turns | `6dccdb7b`, `8f64cc82`, `54d2f0bd`, `4b0cdfee`, `06efce80` (#1036) | none | none | none (metrics port unchanged) |
| cross-zone messaging | `88f441f8` (#1072), `8b59da72` (#1066), `3d3bac3e` (#1074), `1e9a7184` (#1085) | none (Ferrata does not use LEZ cross-zone) | none | none |
| `core_ffi` | `38456c72`, `85075961`, `3fb42679` | none | none | none (not linked) |
| tests, fixtures, CI, CODEOWNERS | the other 21 | none | none | none |

## The program loader (`e3edf474`, "fix(program_loader): strengthen program deployment and execution", #943)

What changed (`lee/state_machine/core/src/program/mod.rs`, `lez/programs/program_loader/core/src/lib.rs`,
`lee/state_machine/src/validated_state_diff/{mod.rs,public_backend.rs}`, `lee/state_machine/src/state/mod.rs`,
`lee/privacy_preserving_circuit/src/main.rs`):

1. **Tagged loader state.** A loader actor state is now a borsh enum
   `LoaderEntry { Header(ProgramHeader), Segment(ProgramSegment) }` (one tag byte, append-only), so a
   header and a segment can no longer decode as each other. `ProgramHeader::to_bytes`/`from_bytes`
   become `to_loader_actor_state`/`from_loader_actor_state`; `ProgramSegment` gets the same pair;
   `LoaderEntry::to_bytes`/`from_bytes` take the old names. The immutable mirror commitment still
   hashes the untagged header (`immutable_mirror_commitment` keeps its signature and value).
   - *Programs*: none of the guests reads loader state; no source change.
   - *Harness*: `tests/private_consumer.rs` decodes the consumer's header to build its mirror
     membership (`ProgramHeader::from_bytes`): renamed call on rc2.
   - *Driver*: `scenario.rs` (is the program already deployed?) and `verify.rs` (the deployed image
     ids) decode headers: renamed calls. `blocklog.rs` decodes loader *messages*, whose enum
     (`WriteSegment`, `CreateHeader`, `UpdateHeader`) is unchanged.
   - Every account written by the loader is one byte longer on rc2 (the tag).
2. **A segment's target must be authorised.** `WriteSegment` now asserts `input.is_authorized`, as
   `CreateHeader` and `UpdateHeader` already did. The driver already signs every segment account
   with a key derived for it (`scenario.rs`, `deployment_keys`), so its deployment is unaffected;
   anyone else's segment can no longer be written into a squatted account.
3. **The loader is charged.** `run_program_loader` charges `LOADER_CYCLES_PER_BYTE = 1` cycle for
   the message's pre-state and for every loader actor state the loader reads, within the
   transaction's budget (`OutOfGas` past it). On rc1 a loader transaction was charged no execution
   gas at all. A `WriteSegment` reads the next segment (about 96 KiB), a `CreateHeader` reads the
   whole chain (the user ELF). The comment puts the rate at about 3.7 times below the measured cost,
   kept so the wallet's default gas limit covers any legal chain. The private path's simulator
   charges the same.
   - *Harness*: none: the harness deploys at genesis, which does not run the loader.
   - *Driver*: the deployment's execution gas and fees change (rc1: 6 transactions per 508 KB image,
     execution gas 0, storage gas 479,496, base fees 3,835,968). Measured on rc2 in `lez/live`.
4. **Segments built and checked before upload.** `program_loader_core::build_segments` and
   `segment_count` (the wallet refuses a chain over `MAX_PROGRAM_SEGMENTS` before paying for any
   segment); the wallet error `SegmentCountMismatch` becomes `SegmentChain(SegmentChainError)`. The
   chunk size (`MAX_SEGMENT_DATA_LEN`, 96 KiB) and the tail-to-head order are unchanged. The driver
   splits with the same constant: no change.
5. **Genesis checks a builtin's image id.** Deploying a program at genesis now computes the image id
   of its ELF and asserts that it equals the `Program`'s id (`Program::new_unchecked` documents that
   callers verify it). The harness deploys its programs at genesis with
   `Program::new_unchecked(methods::*_ID, ELF)`: the ids are the ones risc0-build computed, so the
   check holds; it costs one image-id computation per program per genesis.
6. **The privacy circuit refuses two program-image witnesses for one account** (before: the second
   silently replaced the first). Ferrata's private transactions name each program once: no effect
   on behaviour. The circuit's binary changed (`artifacts/lee/privacy_preserving_circuit/
   privacy_preserving_circuit.bin`, 853,552 → 857,716 bytes), so its image id changed: **a receipt
   proven with rc1's circuit does not verify on rc2**. The GPU-proven transactions in `lez/results/`
   (`stage2-private-tx.borsh`, `stage4-private-attest-tx.borsh`, `bridge-private-*-tx.borsh`) are
   rc1 evidence only; they were also bound to the harness's deployment (`lez/live/README.md`).

## The builtin programs (`e3edf474`, `a016949f`, `3d3bac3e`)

`artifacts/lez/programs/*.bin` were rebuilt (the bridge, bridge lock, clock, cross-zone inbox and
outbox, fee, ping, sequencer stake and wrapped token programs; e.g. `fee.bin` 389,660 → 389,676
bytes) after the `lee_core` change and the cross-zone refactor. Their image ids at genesis change;
the harness links them through `programs` (`artifacts` feature) and the live zone through the
sequencer. Ferrata calls none of them directly; whether they move any of its charges is what the
harness tables on rc2 show.

## The wallet (#1035 and its commits; `1f7afc01`, `49542202`, `5dc61eb3`, `acf6fba7`, `38456c72`, `df52f773`, `00746e09`)

rc1's wallet signed a public transaction with the configured `gas_limit` and
`max_fee_for(gas_limit)` and sent it. rc2's wallet (`lez/wallet/src/lib.rs`, `fees.rs`,
`config.rs`):

1. **Simulates every public send** against the fetched states of its accounts at the head fee
   state (`measure_public`), unless both `FeeParams::gas_limit` and `FeeParams::max_fee` are pinned
   (`set_fee_params`). For a loader message it also fetches the segments the loader reads without
   declaring them (`ProgramLoader::undeclared_reads`, `1f7afc01`).
2. **Declares the measured gas** plus `public_gas_margin_percent` (new config field, default 25),
   capped at the configured `gas_limit`, unless `FeeParams::gas_limit` pins it; `max_fee` is priced
   at the base fees after `fee_headroom_blocks` blocks of the steepest climb.
3. **Refuses to send a transaction whose simulation reverts** (`ExecutionFailureKind::SimulatedRevert`)
   unless `set_allow_revert(true)`: "a failed action is charged its whole gas limit".
4. Simulates a second time with the fee reserve held from the payer's balance when the transaction
   reads that balance (`df52f773`).
5. `ExecutionFailureKind::DryRun` carries a `DryRunReport` (`DryRun(Box<DryRunReport>)`) instead of
   printing; the private dry run reports the same figures.
6. The config field `private_fee_headroom_blocks` is renamed `fee_headroom_blocks` (the old name is a
   serde alias); `set_private_fee_headroom_blocks` becomes `set_fee_headroom_blocks`.

Effect on `lez/live`'s driver, which sends every transaction through the wallet:

- `private.rs` matches `ExecutionFailureKind::DryRun` (twice): the pattern takes the report on rc2.
- The scenario sends 19 transactions the program refuses with an exit code and 2 that LEZ reverts;
  on rc2 the wallet would not send them (item 3). The driver allows reverts on rc2: the refusals are
  the point of those steps.
- The driver declares each job's gas limit itself (4,000,000 by default, more for the largest; the
  wallet default 2,000,000 for the deployment and one deliberate under-declared adoption), and a
  refusal is charged its whole declared limit. On rc2 the wallet would declare its measurement plus
  25 % instead (item 2). The driver pins the limit it declared on rc1 (`FeeParams::gas_limit`), so
  the same steps are charged the same way; `max_fee` is left to the wallet (a fixed-width field:
  the transaction's bytes do not change).
- What rc2's defaults would do instead, read from the source and not run: the under-declared
  adoption (rc1: reverted, charged 2,000,000) would be simulated within 2,000,000 cycles, run out of
  gas in the simulation and not be sent; every applied transaction would declare about 1.25 times
  its cycles.

## Bedrock's test chain (`9d80a89a`, `616108d5`, #1063)

LEZ's `bedrock/deployment-settings.yaml`, which `lez/live` runs the node with, now has
`slot_duration_in_seconds: 4` (was 1) and `security_param: 3` (was 5). In the node's source
(`consensus/cryptarchia-engine`, `epoch_length` and `base_period_length`) an epoch is
(3 + 3 + 4) × ⌊k / f⌋ slots with f = 1/2: 100 slots of 1 s (100 s) on rc1, 60 slots of 4 s (240 s)
on rc2. The node binary is the same revision.

- `run-live.sh` reads the node's slot 0 as "the second the tip's slot changed, minus the slot":
  wrong by a factor of the slot length on rc2. It and `phase.py` assume a 100 s epoch. Both now take
  the slot length and the epoch length from the deployment settings of the LEZ tree they run.
- The block interval of 10 s still divides the epoch (240 s), so the alignment of the sequencer's
  blocks against epoch boundaries (`run-live.sh`, the rewind of the rc1 observations) still applies.
- Finality depth 3 at 4 s slots replaces depth 5 at 1 s slots: the time to `Finalized` changes;
  measured in `lez/live`.
- The node's storage gas price moves at epoch rotations; with 240 s epochs it moves less often.

## The sequencer

- `b0c15a2c` (#1076), `7c59ef5d`, `c743e760` (#1071): the Bedrock publisher's and the polling readers'
  node requests are bounded by timeouts (`chain_state::timeout_node`); a timed-out funding call may
  hold a note reservation until it expires. No interface change; the live run is its test.
- `6dccdb7b`: a block counts as "produced by us" (a metric) only once final.
- `8f64cc82`, `54d2f0bd`, `4b0cdfee`, `06efce80` (#1036): dashboards, logs, turn tracking for several
  sequencers. Not used (one sequencer, no Grafana).
- `8b59da72` (#1066): cross-zone dispatches drained round-robin per source zone. Not used.

## Cross-zone messaging (`88f441f8`, `3d3bac3e`, `1e9a7184`, `8b59da72`)

Outbound cross-zone sends are now charged like any user transaction (no longer system
injections: `common::transaction::is_system_injection`), the emission wire contract moved to its
own core crate, emissions of reverted transactions are skipped, and a peer's genesis hash is
pinned. Ferrata's bridge connects Ethereum and LEZ through the attestation program and the
checkpoint, not LEZ's zone-to-zone messaging: no effect.

## core_ffi, tests, CI

`core_ffi` exposes dry runs, declared fees and the fee state, and moves the dry run next to the
transaction hash in its send results (`38456c72`, `85075961`, `3fb42679`): not linked by Ferrata.
The other commits are tests (`6197dff0`, `e48e1b4c`, `787d56e5`, `206038a7`, `758d97db`, `bb637edd`,
`cd706bf3`, `c743e760`), CI (`9840912b`, `ef033eb1`, `4e0dea27`, `2bee011b`, `3de1d922`,
`9a629137`, `0fb43b63`, `49fe7d81`), `CODEOWNERS` (`1f6b8ff1`), the wallet docs (`00746e09`) and
merges.

## What Ferrata changes for rc2

Predicted from the diff, then confirmed by building and running (`../../lez/RESULTS.md` §9):

| where | change | why |
|---|---|---|
| `lez/` manifests | LEZ path dependencies point at `upstream/lez-<tag>`, switched by `lez/scripts/lez-select.sh <tag>` (default and committed: `v0.4.0-rc1`) | build against either tag; rc1 must still build to the same images |
| lockfiles | per-tag lockfile changes in `lez/lockfiles/` | rc2's crates add dependency edges (`program_loader_core` → `thiserror`, `chain_state` → `async-trait`; `common` drops `ping_core`) |
| harness | feature `lez-rc2`: the renamed header decoder; results of rc2 under `lez/results/v0.4.0-rc2/` | API rename; rc1's results stay as they are |
| driver | feature `lez-rc2`: renamed header decoder, `DryRun(_)`, the declared gas limit pinned, reverts allowed | API rename; the wallet's new simulation and refusal |
| `lez/live` scripts | the LEZ tag's commit and scratch directory; Bedrock's slot and epoch length from its deployment settings | rc2 runs 4 s slots and 240 s epochs |
| guests | none | `lee_core`'s program-facing API is unchanged |

What the build found besides: the lockfile changes are exactly LEZ's own path crates
(`cross_zone_emission_core` is new; `program_loader_core` → `thiserror`; `chain_state` →
`async-trait`; `common` drops `ping_core`), no external crate changes version, and the six guests'
lockfiles do not change. The harness's copy of LEZ's settlement (`lez/harness/src/settle.rs`) still
matches: the functions it copies are unchanged in rc2. The live driver had stopped compiling on
either tag for a reason that is not rc2's: the wire gained a `chain_id` with the attestation, after
the rc1 run. Every table row the harness measures (cycles, execution and storage gas, base fees,
results) is the same on rc2 as on rc1; only the image ids move.
