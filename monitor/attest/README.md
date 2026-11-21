# `attest-monitor`: the attestation side's monitor

**Prototype, unaudited.** RFP-021 S.7 (how to detect that an instance has stalled) and VS.4 (what a consumer can do
about a captured sync committee); the design's `monitor` duties of `docs/design/attestation.md` §3.6, §8 and §11.3.
The watchers' `monitor` of the bridge's return leg is the other program in [`../`](../README.md).

It watches one instance of the attestation program on LEZ and compares what the instance holds with what several
beacon providers report. The program cannot see any of this; the monitor is how an operator or a consumer does.

| Alert | Level | When |
|---|---|---|
| `instance-diverged` | critical | The instance's finalized header is not the canonical block at its slot by the account of at least `--diverge-at` providers (default 1: any provider), whatever the others report: they have another root there, or no block. This is what a captured or colluding sync committee leaves (a forged handoff or finality the program accepts on the signature alone, `docs/trust.md` §2). **Wire it to the consumer's freeze authority** (`--alert-cmd`, `--alert-cmd-level critical`), and size caps to what passes before the freeze. A provider that reports the instance's root does not cancel the alarm: it may collude with the forger (the alert names it under `agreeing`). The rule, with `n` providers answering: `f` lying providers raise it when `f` ≥ `--diverge-at`; `f` providers colluding with a forged instance hide it only when `f` > `n` − `--diverge-at` (by default, only when every provider that answers colludes). So by default one provider can raise it (a false freeze costs a pause; a missed one, what the caps let through); with `--diverge-at 2` and three or more providers, one provider can neither raise it nor hide it. |
| `providers-disagree` | warn | Fewer than `--diverge-at` providers disagree while others agree (only with `--diverge-at` 2 or more): no verdict yet; page on it. |
| `instance-unconfirmed` | warn | Too few usable answers to confirm the header. |
| `instance-stalled` | warn | The instance's finalized header lags the providers' finalized header (their median, so that one provider cannot raise or hide it) by more than `--stall-after` (default 15,300 s: two missed sessions of a two-hour cadence, plus finality). Updates are outstanding; anyone may submit them, and the instance resumes (R.7). Nobody is obliged to. |
| `ws-half` | critical | By the chain's clock, the finalized header is past half its weak-subjectivity period (`Config::ws_end_slot`): submit the outstanding updates now. |
| `ws-expired` | critical | Past the period: the program refuses every update; the instance must be replaced by a fresh one (F.10) and consumers must migrate. |
| `provider-unavailable`, `provider-wrong-chain`, `provider-behind` | warn | A provider gave no usable answer (its finalized slot included), answers for another chain (its genesis is not the instance's), or has not finalized the instance's slot (behind or syncing: its answer there is not final, and a 404 may be a block it has not seen); never counted. A provider past the slot that no longer serves it (pruned history, or a node checkpoint-synced after the slot that has not backfilled) answers 404 like an empty slot and counts as disagreeing; the alert names it. |
| `instance-inactive`, `instance-unreadable` | warn | The instance's committee is not adopted yet; its state could not be read. |
| `recovered`, `started`, `stopped` | info | An alert cleared; the daemon's start and clean stop. |

```sh
cd monitor/attest && cargo build --release --locked          # Rust 1.93; HTTP through curl
# one assessment (JSON on stdout; exit 0 healthy, 10 a warning, 20 a critical alert, 1 an error)
attest-monitor check --state lez:http://127.0.0.1:3040 --program 0x<program account> --instance 0x<instance id> \
  --provider lodestar=https://<beacon> --provider nimbus=https://<beacon> --provider publicnode=https://<beacon>
# the daemon: every --interval seconds; alerts on change, repeated every --repeat while they hold; SIGTERM stops it
attest-monitor watch --state lez:http://127.0.0.1:3040 --program 0x... --instance 0x... --provider ... \
  --interval 300 --alert-log alerts.jsonl --alert-cmd 'my-freeze-tool' --alert-cmd-level critical \
  --status-file status.json --state-file seen.json
```

- `--state`: `lez:URL` (a LEZ v0.4 sequencer's `getAccount` of `instance(id)`, decoded with LEZ's own `Account` type),
  `file:PATH` or `cmd:COMMAND` (hex of the state's Borsh encoding, or JSON with `instance_state`, from anything that
  reads LEZ: the `ferrata` tool, `ferrata_attest_module` through Logos Core's `lez_core`).
- `--provider NAME=URL`: a beacon node's REST API: `/eth/v1/beacon/genesis`, `/eth/v1/beacon/headers/{slot}`,
  `/eth/v1/beacon/headers/finalized`. `--views FILE` instead: the providers' answers as JSON (offline).
- Alerts go, one JSON line each, to standard error, `--alert-log` and `--alert-cmd` (`sh -c`, the JSON on standard
  input, at or above `--alert-cmd-level`), as the watchers' `monitor`'s do. Nothing else is sent anywhere.
- `--state-file` keeps every finalized header the instance has shown with the providers' verdict, across restarts;
  `--status-file` is the latest assessment.

## Tested

`cargo test --locked` (`tests/monitor.rs`): 10 tests, all pass (Modal build box, 21 November 2026). They run the binary
against providers on loopback that serve what Sepolia's public providers served (publicnode's header at slot
11,549,664 from the attestation fixture, Lodestar's genesis from `verify_checkpoint`'s recorded run, publicnode's root at
11,608,192 from the lab's session of 18 November), on the instance states the R.7 test wrote with LEZ's state machine
(`tests/fixtures/stall.json`, [`lez/results/r7-stall.md`](../../lez/results/r7-stall.md)):

| Test | Measured |
|---|---|
| `healthy_when_three_providers_confirm_the_finalized_header` | 3 agree on `0xeed90328…6657` at 11,549,664; no alert, exit 0 |
| `the_r7_stall_raises_the_stall_and_half_weak_subjectivity_alarms_and_the_resume_clears_them` | the stalled instance at the resume's time: lag 702,336 s, `instance-stalled` and `ws-half` (half at slot 11,606,192, end 11,662,720), exit 20, divergence `confirmed`; the resumed instance: its root is publicnode's `0x23362d57…5466`, no alert |
| `past_the_weak_subjectivity_period_the_instance_must_be_replaced` | `ws-expired` |
| `a_forged_finalized_header_is_called_diverged_and_runs_the_consumers_freeze_hook` | the instance's finalized header altered (as a captured committee would leave it): `instance-diverged` with all 3 providers named; the alert command (the consumer's freeze hook) ran once, with the instance id |
| `one_colluding_provider_cannot_hide_a_divergence_from_the_freeze_hook` | the forged header, 2 honest providers + 1 colluding (reporting the forged root), the freeze tool at `--alert-cmd-level critical`: `instance-diverged`, exit 20, the freeze tool ran once, the honest providers named under `providers` and the colluder under `agreeing`; 1 honest + 2 colluding: still `diverged`; only colluding providers answering: `confirmed` (the stated limit) |
| `the_divergence_rule_one_liar_raises_it_by_default_and_not_with_diverge_at_2` | a healthy instance, 2 honest + 1 liar: `diverged` (exit 20) by default, the liar under `providers`; with `--diverge-at 2`: `providers-disagree` (exit 10); the forged header with `--diverge-at 2`: 1 colluding of 3 still `diverged`, 2 of 3 `contested`; `--diverge-at 0` refused; a provider claiming a far finalized slot does not raise the stall |
| `a_provider_behind_the_instance_is_not_counted` | 1 honest provider + 2 finalized 32 slots before the instance's header (404 at its slot) + 1 without a finalized answer: `provider-behind` ×2, `provider-unavailable`, `unconfirmed`, no divergence (counted, the two would have raised a false critical); the two once finalized past the slot, still no block there: `diverged` |
| `unreachable_and_wrong_chain_providers_are_not_counted` | a closed port and an HTTP 503: `provider-unavailable` ×2; mainnet's genesis: `provider-wrong-chain`; one honest left: `instance-unconfirmed` |
| `reads_the_instance_from_a_lez_sequencers_get_account` | `lez:`: one `getAccount`, the account built with LEZ's own types as the sequencer serves it; an instance the account does not hold: exit 1 |
| `the_daemon_alerts_on_change_recovers_and_stops_on_sigterm` | `watch`: `instance-stalled` and `ws-half` once each (not repeated within `--repeat`), `recovered` for both when the instance resumes, the status and state files, SIGTERM: `stopped`, exit 0 |

Not done: a run against a running LEZ sequencer and public beacon providers (the `lez:` reader is tested against
LEZ's own `Account` serialization on loopback); the execution side (a record's block hash against execution endpoints);
a Logos module of it (the instance can come from `ferrata_attest_module` through `cmd:`).
