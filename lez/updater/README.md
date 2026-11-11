# ferrata-updater: the light-client updater daemon

Keeps Ferrata light-client instances on LEZ following Ethereum (RFP-021 U.3, R.2, R.8, S.7; proposal
A3). Each round, per instance, it reads the instance from the chain, asks several beacon API
sources for light-client data, runs the attestation program's own checks on every candidate
against the on-chain store, and sends the next step of the public job (`Init`, `Open`, `Miller`,
`Adopt`, `Finish`) for the candidate that moves the instance furthest, or the next step of a job
another updater left open. It keeps no light-client store of its own: the chain is the state.

**Status (11 November 2026).** Built and tested against LEZ v0.4.0-rc1's state machine in process
(`HarnessLedger`: the harness's program image, LEZ's fee program and settlement, `RISC0_DEV_MODE`)
on the lab's recorded mainnet and Sepolia data (`../lab/results/record/net/`). **Not built:** the
backend that sends to a running LEZ sequencer (`daemon.ledger = "sequencer"` refuses to start); the
lab driver (`../lab/driver/`) is what sends the same messages to a sequencer through LEZ's wallet
today. Not run against live beacon endpoints (nothing is sent to a public network from this
branch); the HTTP path (`BeaconApi`, through `curl`) is tested against beacon nodes on loopback
serving the recorded data.

## Run

```sh
cd lez
cargo build -p ferrata-updater --features harness --locked      # dev mode: the in-process chain
RISC0_DEV_MODE=1 target/debug/ferrata-updater --config updater/dev/dev.toml [--rounds N] [--quiet]
updater/dev/demo.sh OUT_DIR    # 45 s of the above, /health and /metrics read, then SIGTERM
```

One file configures everything: `ferrata-updater.example.toml` lists every field with its
default. In short:

| section | fields |
|---|---|
| `[daemon]` | `name`, `state_dir` (committee cache and journal), `poll_secs`, `listen` (`/health`, `/metrics`), `ledger` (`harness`; `sequencer` not built) |
| `[keys]` | `payer_key_file`: a 32-byte secret key in hex. It signs and pays every transaction and needs no other role: no step of the program checks a sender |
| `[fees]` | `gas_limit` declared by every step (default 4,800,000; the largest step measured is 3,731,712), `max_fee_per_tx`, `budget` (the daemon stops sending at it and says so in `/health`) |
| `[policy]` | `defer_window`, `max_lag_slots`, `min_agreeing`, `retry_base_ms`, `retry_max_ms`, `max_attempts`, `demote_rounds`, `max_periods_per_request` |
| `[[chain]]` | `name`, `preset` (`mainnet`, `sepolia`), `checkpoint` (the instance's trusted block root), `adoption_parts`; `[[chain.source]]` with `url` (a beacon node's REST API) or `dir` (a recorded directory in the lab's layout) |

SIGTERM or SIGINT: the round in hand finishes, then the daemon exits 0. Nothing needs saving: a
transaction is either on chain or not, and the next start reads the chain.

## What a round does

1. **Read the instance** (`InstanceState`). None yet: fetch the checkpoint's bootstrap, accept the
   first one that passes `Init`'s own checks (`steps::initialize`), send `Init`, then the committee's
   `Adopt` parts. A source serving another bootstrap is demoted.
2. **Committees.** The store holds summaries; the signer witness needs the full 512 keys. They come
   from the cache (`state_dir/<chain>/committees/`), the bootstrap, or the sources' period updates,
   and are accepted only when they hash to the summaries on chain.
3. **Candidates.** From every usable source: the newest finality update and the period updates
   from the store's period to the source's head.
4. **Verdicts.** Every candidate goes through the program's steps natively (`plan_job`: step 1,
   Miller loops, final exponentiation, adoption, `apply`), on the on-chain store upgraded to the
   candidate's layout. *Useful* (it advances, rotates or adopts), *no change* (older than the
   store), *unverifiable* (signed by a committee the store does not hold yet), or *invalid*: the
   program would refuse it, so the source is demoted for `demote_rounds` and nothing is sent.
5. **Cross-check.** A source whose newest verified finalized slot is more than `max_lag_slots`
   behind the best is flagged lagging (the best source's update is used: fallback is per round). A
   source that does not answer is flagged unavailable. With `min_agreeing = k`, the updater sends
   only an update whose finalized header `k` sources serve (the best such one), else waits.
6. **Choice.** Adopting the next committee first when the store lacks it; then the furthest
   finalized header; then the most signers.
7. **The job's next step, from the chain.** A job's id hashes the instance, its starting store and
   the update, so every updater computes the same id for the same update. No job yet: `Open`. A job
   in `Miller`: `Miller`. Adoption pending: the next `Adopt` part. Otherwise `Finish`. The updater
   continues any open job, its own or another's, and after a restart its own.
8. **Deferral.** Each (job, step) is sent by the updater whose wait, `H(key, job, step) mod
   defer_window` rounds after it first saw the step due, runs out first; the others find the step
   done and skip it. Two honest updaters collide only when their waits are equal.
9. **Send.** Within `max_fee_per_tx` and `budget`; a transport failure is retried with
   exponential backoff (`retry_base_ms * 2^attempt`, at most `retry_max_ms`, `max_attempts`), and
   before every retry the chain is read again: a step another updater sent meanwhile is not sent.
   Every sent transaction is a line of `state_dir/<chain>/journal.tsv`.

## Health and metrics

`GET /health`: JSON per instance (`status` `ok`, `degraded` or `down`, the reasons, the finalized
slot and period, the last plan and error); HTTP 503 when an instance has no usable source.
`degraded`: a source demoted, lagging or unavailable, the budget reached, or the last round stuck.

`GET /metrics` (Prometheus text, labels `daemon`, `chain`, `source`): `ferrata_updater_rounds_total`,
`_tx_total{step,result=applied|refused|rejected}`, `_refused_by_code_total{code}`,
`_gas_total{result}`, `_fee_total{result}`, `_budget_remaining`, `_transport_errors_total`,
`_retries_total`, `_deferrals_total`, `_steps_done_by_others_total`, `_jobs_finished_total`,
`_finalized_slot`, `_period`, `_health` (0 ok, 1 degraded, 2 down), and per source
`_source_requests_total`, `_source_errors_total`, `_source_refused_total`, `_source_lagging`,
`_source_available`, `_source_demoted`, `_source_finalized_slot`. A sample is
`results/demo-metrics.txt`.

## Tested (measured; `cargo test -p ferrata-updater --features harness`, RISC0_DEV_MODE=1, on the mrun build box)

| test | what it shows | result |
|---|---|---|
| `catches_up_mainnet_and_sepolia_from_recorded_data` | from the checkpoint to the newest recorded update, two sources | mainnet: 4 jobs, 16 transactions, finalized slot 15,631,808, root `530599ec…2328`; Sepolia: 35 jobs (the activation, 33 period updates through the Gloas fork, the newest finality update), 171 transactions, 11,556,000, root `d31b7d34…f3cc`; both the lab's final roots (`../lab/README.md`); none refused |
| `lying_source_is_refused_without_gas` | a source with forged signatures and one with a wrong bootstrap, before an honest one | both demoted, no transaction refused (nothing they served was sent), `/health` degraded naming the liar, `source_refused_total` counts it |
| `stale_and_down_sources_fall_back` | a source 6 updates behind and one that is down, beside a fresh one; then `min_agreeing = 2` | the fresh source's newest update applied; stale flagged lagging; down counted; with two required, the updater holds at what both serve (11,550,720) and waits |
| `restart_mid_job_and_mid_period` | killed after `Open` and `Miller` of a finality update inside Sepolia's period 1410; restarted on the same state directory; two dropped connections; then a start with no state directory | the restarted updater sends `Finish` (not a second `Open`), two retries, applied; the one without state refills its committees from the sources and reaches the end; no transaction refused |
| `beacon_api_sources_over_http` | `BeaconApi` (curl, SSZ, the `updates` endpoint's length-prefixed chunks) against two beacon nodes on loopback: one serving the recorded mainnet chain, one answering 503 | catch-up to 15,631,808 through the first; the second flagged unavailable, `/health` degraded naming it |
| `fee_limits_are_declared_and_enforced` | a maximum fee below a step's cost; a budget of 250,000,000 with a maximum fee of 100,000,000 | the first is rejected by LEZ's settlement (`MaxFeeBelowReserve`: nothing charged, nothing applied); the second stops after 7 transactions (156,173,360 charged), `/health` says "budget reached" |
| `config.rs` (3 tests) | the shipped `ferrata-updater.example.toml` and `dev/dev.toml` parse; an unknown preset, a non-hex checkpoint, a zero deferral window, a source with both `url` and `dir`, an unknown field are refused; the `updates` stream splits under both length conventions | pass |
| `gate_two_honest_one_adversary` | proposal A3's done gate (below) | passes, under four block orders |

**The A3 gate** (`results/gate.txt`). Two honest updaters (one with a lying source, one with a
lagging one) and an adversarial updater follow mainnet and Sepolia on one chain while the record
replays one update at a time; each round is a block whose transactions settle in a shuffled order;
the adversary races due steps a third of the time without waiting and, at each new update, opens a
job for a forged update and replays an applied one.

| | |
|---|---|
| block order (`GATE_SEED`) | default | 1 | 2 | 3 |
|---|---:|---:|---:|---:|
| updates applied, each once (`InstanceState.applied` = 1 + updates, per instance) | 58 | 58 | 58 | 58 |
| honest gas on refused steps / on applied steps (gate ≤ 0.10) | 0.0698 | 0.0364 | 0.0384 | 0.0646 |
| honest-1 share of the honest fees (gate 0.35–0.65) | 0.6306 | 0.5465 | 0.5298 | 0.5377 |
| honest-2 share | 0.3694 | 0.4535 | 0.4702 | 0.4623 |
| honest-1 / honest-2 / adversary transactions applied | 62 / 38 / 150 | 59 / 52 / 139 | 56 / 52 / 142 | 58 / 50 / 142 |
| honest-1 / honest-2 / adversary transactions refused | 18 / 28 / 25 | 15 / 13 / 28 | 18 / 11 / 19 | 16 / 18 / 23 |
| rounds (blocks); ledger transactions | 447; 431 | 429; 416 | 437; 408 | 425; 417 |

Without the adversary (`GATE_ADVERSARY=0`, `results/gate-no-adversary.txt`), the two honest
updaters alone: 58 updates each applied once in 695 rounds, 132 and 118 transactions applied, 11 and
17 refused (collisions of equal waits), refused gas 0.0172 of applied, fee shares 0.5404 / 0.4596.

58 updates: 14 mainnet and 44 Sepolia, every update the lab recorded, not the gate's 100. The
adversary also sent 110 junk `Open`s in each run, at its own cost. Files: `results/gate.txt`,
`results/gate-seed-{1,2,3}.txt`. The default order's shares are near the band's edge: about 100
honest transactions are split by a hash, so a share of 0.63 is about two standard deviations out.

## Not built, not tested

- The sequencer backend (LEZ's wallet and `sequencer_service_rpc`, following blocks final on
  Bedrock, as `../live/driver/src/live.rs`); it would implement `ledger::Ledger` in a workspace of
  its own, as the lab driver does, since it links LEZ's node-side crates.
- Live beacon endpoints (the HTTP path ran against loopback nodes only); HTTPS without `curl`; JSON
  responses (only SSZ is read).
- The proven path (`VerifyAll`): the daemon drives the public job only.
- Running as a Logos module under `logosctl daemon` (the design: `docs/design/attestation.md` §8).
- Jobs whose `Finish` fails stay open (the program has no clean-up step); the updater moves on.
