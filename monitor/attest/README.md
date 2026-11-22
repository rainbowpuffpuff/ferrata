# `attest-monitor`: the attestation side's monitor

**Prototype, unaudited.** RFP-021 S.7 (how to detect that an instance has stalled) and VS.4 (what a consumer can do
about a captured sync committee); the design's `monitor` duties of `docs/design/attestation.md` §3.6, §8 and §11.3.
The watchers' `monitor` of the bridge's return leg is the other program in [`../`](../README.md).

It watches one instance of the attestation program on LEZ and compares what the instance holds with what several
beacon providers report. The program cannot see any of this; the monitor is how an operator or a consumer does.

| Alert | Level | When |
|---|---|---|
| `instance-diverged` | critical | The instance's finalized header is not the canonical block at its slot by the account of at least `--diverge-at` providers (default 1: any provider), whatever the others report: they hold another canonical block at the slot (final for them or not yet), or no block at a slot they have finalized. This is what a captured or colluding sync committee leaves (a forged handoff or finality the program accepts on the signature alone, `docs/trust.md` §2). **Wire it to the consumer's freeze authority** (`--alert-cmd`, `--alert-cmd-level critical`), and size caps to what passes before the freeze. A provider that reports the instance's root does not cancel the alarm: it may collude with the forger (the alert names it under `agreeing`). So by default one provider can raise it (a false freeze costs a pause; a missed one, what the caps let through); with `--diverge-at 2` and three or more providers, one provider can neither raise it nor hide it. The exact rule, with the next alarm: [below](#the-rule). |
| `instance-ahead` | critical | The instance claims a finality the chain cannot have, or that no provider has: `why: too-new`, its finalized header is younger, by the chain's clock, than the chain's finality delay (two epochs, 768 s on Ethereum), which no honest instance can hold, whatever the providers say; `why: ahead-of-providers`, no provider that answers has finalized the header's slot or holds the header there although it is older than the finality delay plus `--ahead-grace` (default 60 s): the instance is not on their chain, or every provider lags. **Wire it to the freeze authority too.** |
| `providers-disagree` | warn | Fewer than `--diverge-at` providers disagree while others agree (only with `--diverge-at` 2 or more): no verdict yet; page on it. |
| `instance-unconfirmed` | warn | Too few usable answers to confirm the header. |
| `instance-stalled` | warn | The instance's finalized header lags the providers' finalized header (their median: with three or more providers one provider can neither raise nor hide it; with two the lower counts, so either can hide a stall) by more than `--stall-after` (default 15,300 s: two missed sessions of a two-hour cadence, plus finality). Updates are outstanding; anyone may submit them, and the instance resumes (R.7). Nobody is obliged to. |
| `ws-half` | critical | By the chain's clock, the finalized header is past half its weak-subjectivity period (`Config::ws_end_slot`): submit the outstanding updates now. |
| `ws-expired` | critical | Past the period: the program refuses every update; the instance must be replaced by a fresh one (F.10) and consumers must migrate. |
| `provider-unavailable`, `provider-wrong-chain`, `provider-behind` | warn | A provider gave no usable answer (its finalized slot included), answers for another chain (its genesis is not the instance's), or has not finalized the instance's slot (behind or syncing). The first two are never counted. One behind the slot is counted only when it already holds another canonical block there (a disagreement): its 404 there may be a block it has not seen, and its agreement is not final, so neither counts. A provider past the slot that no longer serves it (pruned history, or a node checkpoint-synced after the slot that has not backfilled) answers 404 like an empty slot and counts as disagreeing; the alert names it. |
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
  input, at or above `--alert-cmd-level`), as the watchers' `monitor`'s do. Nothing else is sent anywhere. In `watch`,
  an alert the command did not take (it exited non-zero) is emitted again at the next poll, not only after `--repeat`;
  and an outage is not a recovery: a poll that cannot read the instance, has too few providers for a verdict, or has
  no provider's finalized slot holds what was raised (no `recovered`) until a poll can judge it.
- `--state-file` keeps every finalized header the instance has shown with the providers' verdict, across restarts;
  `--status-file` is the latest assessment. `watch` judges each header on record again (up to 8 a poll) until its
  verdict settles (confirmed, or contested, with no provider behind its slot), whether or not the instance has moved
  on. `instance-diverged` and `instance-ahead` are one alert per header (its slot): one found diverged, or too new,
  stays raised (repeated every `--repeat`) when the instance moves on or providers drop out; one diverged, or ahead of
  every provider, clears (`recovered`) only when a later poll confirms that header (`--min-agreeing` agree, none
  disagrees), not when the verdict becomes `contested` or `unconfirmed`. An operator who finds such an alarm false (a
  lying provider) removes the provider and, for a header the instance has left, its entry in the state file. A state
  file written before this version has no `settled`: its confirmed and contested headers count as settled, its
  unconfirmed and diverged ones are judged again.
- The alert command gets one JSON line per alert, newline-terminated.

## The rule

On the instance's finalized header at slot `S` with root `R`, `D` the chain's finality delay (two epochs: 768 s on
Ethereum; Casper FFG finalizes a checkpoint two epochs after it at the earliest, so an honest light client's finalized
header is always at least that old: the mainnet finality update of the attestation fixtures finalizes slot 15,624,480
with signature slot 15,624,545, 65 slots later) and `G` the `--ahead-grace` (60 s):

1. `instance-ahead` (`too-new`) when the header is younger than `D` by the chain's clock. No provider can hide it.
2. `instance-diverged` when at least `--diverge-at` providers are counted against it: a provider that has finalized `S`
   counts with another root there or no block; one that has not finalized `S` counts only with another canonical block
   there. A provider behind `S` with no block there (it may not have seen the slot) or with `R` there is not counted
   either way; nor is one unreachable or on another chain.
3. `instance-ahead` (`ahead-of-providers`) when at least one provider answers, none of them has finalized `S` or holds
   `R` there, and the header is older than `D + G`: the instance is not on their chain, or every provider lags by more
   than `G` (a false alarm then costs a pause). One provider behind among others that have finalized `S` never raises it.

With `n` providers counted, `f` lying providers raise `instance-diverged` on a healthy instance when `f` ≥
`--diverge-at`; `f` providers colluding with a forged instance hide it only when fewer than `--diverge-at` honest
providers are counted, that is `f` > `n` − `--diverge-at` (by default, only when every provider counted colludes).

What one `check` cannot see: a forged header that no honest provider can be counted against yet (each has not finalized
`S` and holds no block there: an empty slot of its chain, or one it has not reached; or it is down), older than `D`,
and either within the grace or vouched for by a colluding provider (claiming `S` final, or holding `R`). `watch` judges
it again once the honest providers finalize `S`; on a chain that finalizes normally they finalize any slot within
three epochs of it (19.2 min on Ethereum; one epoch more when every slot from `S` to the next epoch boundary is empty),
so the forged header raises `instance-diverged` at the latest at the first poll after that, whether or not the
instance has moved on: that time, plus a poll interval and the freeze's own, is what a consumer's caps must cover. The
limit that stays: every provider counted colludes with the forger.

## Tested

`cargo test --locked` (`tests/monitor.rs`): 28 tests, all pass, and 4 unit tests (Modal build box, 22 November 2026). They run the binary
against providers on loopback that serve what Sepolia's public providers served (publicnode's header at slot
11,549,664 from the attestation fixture, Lodestar's genesis from `verify_checkpoint`'s recorded run, publicnode's root at
11,608,192 from the lab's session of 18 November), on the instance states the R.7 test wrote with LEZ's state machine
(`tests/fixtures/stall.json`, [`lez/results/r7-stall.md`](../../lez/results/r7-stall.md)). At the tests' clock (the
stalled state's time plus 60 s) the header at 11,549,664 is 852 s old.

| Test | Measured |
|---|---|
| `healthy_when_three_providers_confirm_the_finalized_header` | 3 agree on `0xeed90328…6657` at 11,549,664; no alert, exit 0 |
| `the_r7_stall_raises_the_stall_and_half_weak_subjectivity_alarms_and_the_resume_clears_them` | the stalled instance at the resume's time: lag 702,336 s, `instance-stalled` and `ws-half` (half at slot 11,606,192, end 11,662,720), exit 20, divergence `confirmed`; the resumed instance: its root is publicnode's `0x23362d57…5466`, no alert |
| `past_the_weak_subjectivity_period_the_instance_must_be_replaced` | `ws-expired` |
| `a_forged_finalized_header_is_called_diverged_and_runs_the_consumers_freeze_hook` | the instance's finalized header altered (as a captured committee would leave it): `instance-diverged` with all 3 providers named; the alert command (the consumer's freeze hook) ran once, with the instance id |
| `one_colluding_provider_cannot_hide_a_divergence_from_the_freeze_hook` | the forged header, 2 honest providers + 1 colluding (reporting the forged root), the freeze tool at `--alert-cmd-level critical`: `instance-diverged`, exit 20, the freeze tool ran once, the honest providers named under `providers` and the colluder under `agreeing`; 1 honest + 2 colluding: still `diverged`; only colluding providers answering: `confirmed` (the stated limit) |
| `the_divergence_rule_one_liar_raises_it_by_default_and_not_with_diverge_at_2` | a healthy instance, 2 honest + 1 liar: `diverged` (exit 20) by default, the liar under `providers`; with `--diverge-at 2`: `providers-disagree` (exit 10); the forged header with `--diverge-at 2`: 1 colluding of 3 still `diverged`, 2 of 3 `contested`; `--diverge-at 0` and `--min-agreeing 0` refused (exit 2); a provider claiming a far finalized slot does not raise the stall |
| `a_provider_behind_the_instance_is_not_counted` | 1 honest provider + 2 finalized 32 slots before the instance's header (404 at its slot) + 1 without a finalized answer: `provider-behind` ×2, `provider-unavailable`, `unconfirmed`, no divergence (counted, the two would have raised a false critical); the two once finalized past the slot, still no block there: `diverged` |
| `b2_forged_instance_ahead_of_every_providers_finality_raises_a_critical_alert` | the review's test, inverted: the forged header, 3 honest providers finalized 64 slots (then 1 slot) before it, each with the real block at its slot: exit 20, `instance-diverged` (all 3 named) and `instance-ahead` (`ahead-of-providers`), the freeze tool ran for both, each time |
| `b2_daemon_keeps_a_forged_header_raised_once_the_instance_has_moved_ahead_again` | the review's daemon test, inverted: forged header 1 (ahead of the providers' finality, 1,596 s old) raised `instance-diverged`; the forger's next header (60 s old) `instance-ahead` (`too-new`); the providers then finalize the first slot: the first still raised, no `recovered`; the freeze tool ran for both |
| `a_header_younger_than_the_finality_delay_is_critical_whatever_the_providers_say` | the real header, 3 providers confirming: at 768 s old no alert (exit 0), at 756 s `instance-ahead` (`too-new`, exit 20); a forged header 360 s old with every provider colluding: `confirmed`, and `instance-ahead` all the same, the freeze tool ran |
| `a_header_ahead_of_every_providers_finality_is_critical_after_the_grace` | a forged header at a slot where 3 honest providers, finalized 32 slots before it, have no block: at 829 s `instance-ahead` (`ahead-of-providers`, exit 20); at 828 s (within the grace) warnings only (exit 10); `--ahead-grace 0` at 769 s: critical; the real header with all 3 behind but holding it there: warnings only, no `instance-ahead` |
| `the_daemon_judges_an_earlier_header_again_once_the_providers_finalize_its_slot` | a forged header vouched for by one colluder (finality claimed, forged root), 2 honest providers behind it with no block there: one `check` gives a warning (exit 10, the stated limit); `watch`: the forger moves on to a second forged header; the honest providers finalize the first slot: `instance-diverged` for the first (`current: false`), then for the second when they finalize its slot; the freeze tool ran once for each; both `diverged` in the state file |
| `a_state_file_from_before_settled_judges_its_unconfirmed_headers_again_not_its_confirmed_ones` | a state file written before this version: its unconfirmed (forged) header is judged again and raised; its header confirmed long ago, where the providers now have no block, is not judged again |
| `with_diverge_at_2_an_honest_provider_dropping_out_is_not_a_recovery` | `--diverge-at 2`, the forged header, 2 honest + 1 colluder: `instance-diverged`; one honest provider down (`contested`, `providers-disagree`): held, no `recovered`, not emitted again |
| `the_stall_resists_one_lying_provider_with_three_providers_not_with_two` | the R.7 stall with 2 honest providers + 1 claiming the instance's own slot as finalized: `instance-stalled`; with 1 honest + that one: no stall (the lower of two counts), the stated limit |
| `unreachable_and_wrong_chain_providers_are_not_counted` | a closed port and an HTTP 503: `provider-unavailable` ×2; mainnet's genesis: `provider-wrong-chain`; one honest left: `instance-unconfirmed` |
| `reads_the_instance_from_a_lez_sequencers_get_account` | `lez:`: one `getAccount`, the account built with LEZ's own types as the sequencer serves it; an instance the account does not hold: exit 1 |
| `the_daemon_holds_a_divergence_through_an_outage_and_retries_a_failed_freeze_hook` | `watch` on the forged header with a freeze tool that fails until it is "up": `instance-diverged` emitted again at each poll while the tool fails, taken once the tool is up; then the instance unreadable (`instance-unreadable`) and every provider down (`provider-unavailable` ×3): the divergence held, no `recovered`, not emitted again; providers back and the honest header: `recovered` |
| `the_daemon_alerts_on_change_recovers_and_stops_on_sigterm` | `watch`: `instance-stalled` and `ws-half` once each (not repeated within `--repeat`), `recovered` for both when the instance resumes, the status and state files, SIGTERM: `stopped`, exit 0 |

Not done: a run against a running LEZ sequencer and public beacon providers (the `lez:` reader is tested against
LEZ's own `Account` serialization on loopback); the execution side (a record's block hash against execution endpoints);
a Logos module of it (the instance can come from `ferrata_attest_module` through `cmd:`).
