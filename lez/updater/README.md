# ferrata-updater: the light-client updater daemon

Keeps Ferrata light-client instances on LEZ following Ethereum (RFP-021 U.3, R.2, R.8, S.7; proposal
A3). Each round, per instance, it reads the instance from the chain, asks several beacon API
sources for light-client data, runs the attestation program's own checks on every candidate
against the on-chain store, and sends the next step of the public job (`Init`, `Open`, `Miller`,
`Adopt`, `Finish`) for the candidate that moves the instance furthest, or the next step of a job
another updater left open. It keeps no light-client store of its own: the chain is the state.

**Status (13 November 2026).** Built and tested against LEZ v0.4.0-rc1's state machine in process
(`HarnessLedger`: the harness's program image, LEZ's fee program and settlement, `RISC0_DEV_MODE`)
on the lab's recorded mainnet and Sepolia data (`../lab/results/record/net/`), and **run on a running
LEZ v0.4.0-rc1 sequencer and Logos Blockchain node** with its sequencer backend (`sequencer/`,
`daemon.ledger = "sequencer"`): the crown run's zone and final programs (`../../docs/image-ids.md`),
following Sepolia live from Lodestar's light-client data with publicnode's headers as the second
source ([On a running LEZ sequencer](#on-a-running-lez-sequencer-with-live-sepolia-data), record in
[`results/live-sepolia/`](results/live-sepolia/)). The beacon APIs are only read; nothing is sent to
Sepolia or any public network.

## Run

```sh
cd lez
cargo build -p ferrata-updater --features harness --locked      # dev mode: the in-process chain
RISC0_DEV_MODE=1 target/debug/ferrata-updater --config updater/dev/dev.toml [--rounds N] [--quiet]
updater/dev/demo.sh OUT_DIR    # 45 s of the above, /health and /metrics read, then SIGTERM

# On a running LEZ sequencer: the build of sequencer/ (a workspace of its own: it links LEZ's
# node-side crates, as ../crown/driver does; LEZ's tree at upstream/lez-v0.4.0-rc1 beside the
# repository, libpcsclite-dev for LEZ's wallet)
cd updater/sequencer && cargo build --release
target/release/ferrata-updater --config FILE   # daemon.ledger = "sequencer", [sequencer] rpc and program
# ... or on the crown run's cloud app, a zone of its own (lez/crown/session.sh a, then u), from the repository root:
e2e/crown.sh updater crown-NAME --bin PATH/ferrata-updater --minutes N [--restart-min M] [--poll-s S]
e2e/crown.sh updater-status crown-NAME; e2e/crown.sh updater-stop crown-NAME; e2e/crown.sh updater-fetch crown-NAME DIR
```

One file configures everything: `ferrata-updater.example.toml` lists every field with its
default. In short:

| section | fields |
|---|---|
| `[daemon]` | `name`, `state_dir` (committee cache, journal, the jobs it opened), `poll_secs`, `listen` (`/health`, `/metrics`), `ledger` (`harness`: the build of this crate with `--features harness`; `sequencer`: the build of `sequencer/`) |
| `[sequencer]` | `rpc` (the sequencer's JSON-RPC; loopback only), `program` (the attestation program's account, base58 or 0x-hex), `wait_final` (default `true`: read the chain and settle transactions only in blocks final on Bedrock), `run_dir` (the backend's follower and manifest; default `state_dir/ledger`), `log` (the sequencer's log, for the reason of a dropped transaction) |
| `[keys]` | `payer_key_file`: a 32-byte secret key in hex. It signs and pays every transaction and needs no other role: no step of the program checks a sender |
| `[fees]` | `gas_limit` declared by every step (default 4,800,000; the largest step measured is 3,731,712), `max_fee_per_tx`, `budget` (the daemon stops sending at it and says so in `/health`) |
| `[policy]` | `defer_window`, `max_lag_slots`, `min_agreeing`, `retry_base_ms`, `retry_max_ms`, `max_attempts`, `demote_rounds`, `max_periods_per_request`, `finish_own_jobs` (default `true`), `min_distinct_clients` (default 0, off) and `client_match` (`bytes` or `finalized-header`): [client diversity](#client-diversity-policymin_distinct_clients) |
| `[[chain]]` | `name`, `preset` (`mainnet`, `sepolia`), `checkpoint` (the instance's trusted block root), `layout` (the checkpoint bootstrap's light-client layout, `electra` by default, `gloas` for a Gloas-era bootstrap; the instance id hashes it), `adoption_parts`; `[[chain.source]]` with `url` (a beacon node's REST API) or `dir` (a recorded directory in the lab's layout), and `kind`: `light-client` (default) or `headers` (a node that serves no light-client data, only `/eth/v1/beacon/headers/{slot}`: it confirms the finalized header, below); `client`: a recorded `dir`'s client, as a node would report it (a node's is what it reports) |

SIGTERM or SIGINT: the round in hand finishes, the backend saves what it keeps (the sequencer
backend: its follower of the chain; the harness: nothing), then the daemon exits 0. The updater itself
needs nothing saved: a transaction is either on chain or not, and the next start reads the chain.

## What a round does

1. **Read the instance** (`InstanceState`). None yet: fetch the checkpoint's bootstrap, accept the
   first one that passes `Init`'s own checks (`steps::initialize`), send `Init`, then the committee's
   `Adopt` parts. A source serving another bootstrap is demoted.
2. **Committees.** The store holds summaries; the signer witness needs the full 512 keys. They come
   from the cache (`state_dir/<chain>/committees/`), the bootstrap, or the sources' period updates,
   and are accepted only when they hash to the summaries on chain.
3. **Candidates.** From every usable source: the newest finality update and the period updates
   from the store's period to the source's head.
4. **Verdicts.** Every candidate goes through the program's steps natively (`ferrata-advance`,
   `../advance`, shared with `ferrata_attest_module`'s `advance`; `plan_job`: step 1,
   Miller loops, final exponentiation, adoption, `apply`), on the on-chain store upgraded to the
   candidate's layout. *Useful* (it advances, rotates or adopts), *no change* (older than the
   store), *unverifiable* (signed by a committee the store does not hold yet), or *invalid*: the
   program would refuse it, so the source is demoted for `demote_rounds` and nothing is sent.
5. **Cross-check.** A source whose newest verified finalized slot is more than `max_lag_slots`
   behind the best is flagged lagging (the best source's update is used: fallback is per round). A
   source that does not answer is flagged unavailable. With `min_agreeing = k`, the updater sends
   only an update whose finalized header `k` sources serve or confirm (the best such one), else
   waits: a light-client source by serving an update with that finalized header, a `headers` source
   by holding that block canonical at its slot (`GET /eth/v1/beacon/headers/{slot}`; a confirmation
   is kept, a finalized block stays canonical). Most public beacon APIs serve no light-client data
   (publicnode's answers 404 to `light_client/finality_update`), so a second, independent provider
   is a `headers` source; one that holds another block there is counted (`mismatches`) and named in
   `/health`, and nothing is sent until enough sources agree.
   With `min_distinct_clients = k` the update must also come from `k` different consensus clients
   ([below](#client-diversity-policymin_distinct_clients)); otherwise the updater waits.
6. **Choice.** Adopting the next committee first when the store lacks it; then the furthest
   finalized header; then the most signers.
7. **The job's next step, from the chain.** A job's id hashes the instance, its starting store and
   the update, so every updater computes the same id for the same update. No job yet: `Open`. A job
   in `Miller`: `Miller`. Adoption pending: the next `Adopt` part. Otherwise `Finish`. The updater
   continues any open job, its own or another's, and after a restart its own. A job it opened comes
   first, before any other update is chosen (`finish_own_jobs`, default): a beacon node serves only
   its newest finality update, so the update a job was opened for can be gone from every source
   before the job's last step, and the steps already paid would be lost. The update is kept in
   `state_dir/<chain>/open/` until the job is finished or moot (the store moved past it), so a
   restarted updater finishes it too.
8. **Deferral.** Each (job, step) is sent by the updater whose wait, `H(key, job, step) mod
   defer_window` rounds after it first saw the step due, runs out first; the others find the step
   done and skip it. Two honest updaters collide only when their waits are equal.
9. **Send.** Within `max_fee_per_tx` and `budget`; a transport failure is retried with
   exponential backoff (`retry_base_ms * 2^attempt`, at most `retry_max_ms`, `max_attempts`), and
   before every retry the chain is read again: a step another updater sent meanwhile is not sent.
   Every sent transaction is a line of `state_dir/<chain>/journal.tsv` (round, step, job, finalized
   slot, result, exit code, execution and storage gas, fee, block, the time it settled, its label).

## Client diversity (`policy.min_distinct_clients`)

Built 18 November 2026 (EVIDENCE row 147). The program's checks (step 4) refuse what the sync committee did not
sign, not what a captured committee signs: a finality update for a block that was never finalized, signed by 342 of
the 512 members, passes them, and the signers lose no stake for it ([`../../docs/trust.md`](../../docs/trust.md)
§2.2). An honest beacon node derives light-client data only from blocks its own fork choice imported, so it does not
serve such an update; two implementations with the same bug are less likely than one. `min_distinct_clients = k` (0,
the default: off; 1 is refused) makes the updater send an update only when sources of `k` different consensus clients
agree on it. Use it with `client_match = "bytes"`: only then is an update that one source alone serves never sent
(while an accomplice holds fewer than `k` of the configured sources); `"finalized-header"` sends such an update when
its finalized header is canonical (below).

- **The client** is what a source reports at `GET /eth/v1/node/version`, asked every round, by its first word
  (`Nimbus/v26.10.2-0abc0d-stateofus`: `nimbus`), kept to printable ASCII. A source that does not answer counts under
  none that round; a change of answer is counted and logged. It is the provider's own claim: nothing authenticates
  it, and a provider behind a load balancer can answer from nodes of several clients. A recorded `dir` has `client`
  in the configuration; `beacon_api_module` reports none (it fails over between providers).
- **`client_match = "bytes"`** (default): the update's SSZ, byte for byte, as each light-client source served it (a
  finality update with a finality update, a period update with a period update), from `k` clients on `k` different
  sources. The updater remembers which (client, source) pairs served which bytes (at most 4,096 updates), because
  implementations keep different finality updates: on 18 November Lodestar served the first update of a newly
  finalized checkpoint for the whole epoch, Nimbus the newest one each slot, so the two served the same bytes during
  one slot of each epoch ([`results/client-diversity-2026-11-18/`](results/client-diversity-2026-11-18/README.md)).
- **`client_match = "finalized-header"`**: the update's finalized header, served by a light-client source or held
  canonical at its slot by a `headers` source (the agreement `min_agreeing` counts), from `k` clients. Weaker and
  faster: it compares the header, not the signed update, and a `headers` answer is read for `canonical`, not
  `finalized`, so **the update sent is one source's bytes**: one that a single light-client source serves is sent when
  its finalized header is canonical, a period update with a next committee nobody else served among them (a captured
  committee's forged handoff; `tests/diversity_limits.rs`, `finalized_header_mode_sends_what_one_light_client_source_served`).
- **Refused otherwise:** no transaction; the plan is `Waiting` ("1 of 2 clients agree byte for byte (lodestar)"), and
  the log names the best update not sent and the clients that served it. While it waits for clients and the sources serve
  updates more than `max_lag_slots` past the instance (any the program would not refuse), `/health` is degraded ("waiting for clients: ...") and
  `ferrata_updater_client_wait_slots` gives the distance: a policy the configured sources cannot meet (two of one
  client, a node that does not answer `/eth/v1/node/version`) is an alarm, not a silent stall.
- **Sources, not providers:** `k` clients from `k` configured sources. The updater cannot tell whether two sources are
  one provider (its version answer and its data may come from different backends); under the policy the configuration
  refuses the same host twice, not two hosts of one provider.
- **Disagreements:** two sources serving different bytes for the same finality update (the same attested header and
  signature slot: one block's sync aggregate over its parent, which honest nodes derive alike) or for the same finished
  period are logged with both digests, counted once per update and source, and named in `/health` for that round. The
  updater does not decide who is right; an update the program would refuse demotes its source as before.
- **Not covered:** the initialization (the bootstrap is bound to the checkpoint root by hashes) and the committees
  (bound to the summaries on chain); a job this updater opened continues (`finish_own_jobs`).
- **What it does not give:** it binds this updater only. The program accepts the same update from anyone on its
  signatures (VS.6): a forger submits it. The policy (with `bytes`) keeps an honest operator from paying for and relaying
  a forgery or a buggy client's update, and its `/health` is the first alarm when its sources disagree.
- **Off by default:** with `min_distinct_clients = 0` nothing changes: no version request, no new metric, the same
  plans (the A3 gate below ran with it off).

## The sequencer backend (`sequencer/`)

`SequencerLedger` (`sequencer/src/ledger.rs`) is the `Ledger` for a running LEZ sequencer; the
daemon is the same (`src/daemon.rs`, which the Logos module drives too). It reuses the live driver's modules by path
(`../live/driver/src`: the wallet connection, the follower, the manifest), as the crown driver does.

- **Send.** A step's transaction is built exactly as `HarnessLedger` builds it: the step's actors
  and admission evidence, the payer's nonce (read from the sequencer), a fee declaration with the
  configured gas limit and maximum fee, the payer's signature. It goes to the sequencer's
  `sendTransaction` through LEZ's wallet client. An error the sequencer answers is a rejection
  (nothing included, nothing charged: `rejected`); no answer is a transport failure, which the
  updater retries after reading the chain again.
- **Settle.** The sequencer serves no receipts. The follower applies every block the sequencer
  serves with LEZ's own block application (`chain_state::apply`), and only once the block is
  `Finalized` on Bedrock (a head block can still be replaced); a transaction's status, exit code,
  execution and storage gas come from LEZ's settlement of it; its fee is what the payer's balance
  fell by, read from the sequencer before and after. A transaction never included within 4 blocks
  is `rejected` (dropped). Every one is a row of `state_dir/ledger/run.tsv` (the live driver's
  manifest: hash, block, position, status, gas, base fees, fee, payer debit, the block's own totals).
- **Read.** The actor states the updater reads are the follower's (blocks final on Bedrock),
  checked against what the sequencer serves.
- **Keep.** The follower is saved in `state_dir/ledger/` every 5 minutes and when the daemon stops,
  so a restart follows on from there rather than from LEZ's genesis.

## Health and metrics

`GET /health`: JSON per instance (`status` `ok`, `degraded` or `down`, the reasons, the finalized
slot and period, the last plan and error); HTTP 503 when an instance has no usable source.
`degraded`: a source demoted, lagging or unavailable, a `headers` source holding another block at a
finalized header's slot, two sources serving other bytes for one update or waiting for clients beyond `max_lag_slots`
(`min_distinct_clients`), the budget reached, or the last round stuck.

`GET /metrics` (Prometheus text, labels `daemon`, `chain`, `source`): `ferrata_updater_rounds_total`,
`_tx_total{step,result=applied|refused|rejected}`, `_refused_by_code_total{code}`,
`_gas_total{result}`, `_fee_total{result}`, `_budget_remaining`, `_transport_errors_total`,
`_retries_total`, `_deferrals_total`, `_steps_done_by_others_total`, `_jobs_finished_total`,
`_finalized_slot`, `_period`, `_health` (0 ok, 1 degraded, 2 down), and per source
`_source_requests_total`, `_source_errors_total`, `_source_refused_total`, `_source_lagging`,
`_source_available`, `_source_demoted`, `_source_finalized_slot`, `_source_confirmed_total` and
`_source_mismatches_total` (a `headers` source's confirmations and disagreements); with `min_distinct_clients`,
`_source_client_info{client,version}`, `_source_client_changes_total`, `_source_disagreements_total` and
`_client_wait_slots`, and `/health` lists every source's reported client. Samples:
`results/demo-metrics.txt` (dev mode), `results/live-sepolia/metrics-last.txt` (the live run).

## Tested (measured; `cargo test -p ferrata-updater --features harness`, RISC0_DEV_MODE=1, on the mrun build box)

Last run 13 November 2026 on the tree that merges this work with the daemon the Logos module drives and the
planning in `../advance` (commit `b099490`): 14 tests (5 in `config.rs`, 9 in `recorded.rs`), all pass; the seven
other gate runs pass too, every figure as on 12 November
([`results/merged-2026-11-13/`](results/merged-2026-11-13/README.md); the run of 12 November:
[`results/tests.txt`](results/tests.txt)). The guests build with LEZ's
guest toolchain (Rust 1.91.1 in `RISC0_HOME`, `scripts/ci/install-risc0.sh`).

| test | what it shows | result |
|---|---|---|
| `catches_up_mainnet_and_sepolia_from_recorded_data` | from the checkpoint to the newest recorded update, two sources | mainnet: 4 jobs, 16 transactions, finalized slot 15,631,808, root `530599ec…2328`; Sepolia: 35 jobs (the activation, 33 period updates through the Gloas fork, the newest finality update), 171 transactions, 11,556,000, root `d31b7d34…f3cc`; both the lab's final roots (`../lab/README.md`); none refused |
| `lying_source_is_refused_without_gas` | a source with forged signatures and one with a wrong bootstrap, before an honest one | both demoted, no transaction refused (nothing they served was sent), `/health` degraded naming the liar, `source_refused_total` counts it |
| `stale_and_down_sources_fall_back` | a source 6 updates behind and one that is down, beside a fresh one; then `min_agreeing = 2` | the fresh source's newest update applied; stale flagged lagging; down counted; with two required, the updater holds at what both serve (11,550,720) and waits |
| `restart_mid_job_and_mid_period` | killed after `Open` and `Miller` of a finality update inside Sepolia's period 1410; restarted on the same state directory; two dropped connections; then a start with no state directory | the restarted updater sends `Finish` (not a second `Open`), two retries, applied; the one without state refills its committees from the sources and reaches the end; no transaction refused |
| `beacon_api_sources_over_http` | `BeaconApi` (curl, SSZ, the `updates` endpoint's length-prefixed chunks) against two beacon nodes on loopback: one serving the recorded mainnet chain, one answering 503 | catch-up to 15,631,808 through the first; the second flagged unavailable, `/health` degraded naming it |
| `fee_limits_are_declared_and_enforced` | a maximum fee below a step's cost; a budget of 250,000,000 with a maximum fee of 100,000,000 | the first is rejected by LEZ's settlement (`MaxFeeBelowReserve`: nothing charged, nothing applied); the second stops after 7 transactions (156,173,360 charged), `/health` says "budget reached" |
| `headers_source_confirms_or_holds_back` | one recorded light-client source and a `headers` source over HTTP on loopback (`/eth/v1/beacon/headers/{slot}`, JSON, as publicnode's), `min_agreeing = 2`; then the same with a `headers` source that holds another block at every slot | honest: catch-up to 15,631,808, 3 confirmations, `/health` ok; lying: after `Init`, 27 of 30 rounds waiting, nothing sent, 27 mismatches, `/health` degraded naming it, nothing refused |
| `own_job_is_finished_when_the_source_moves_on` | the source moves on to a newer finality update after this updater's `Open` (as a beacon node serves only its newest); with `finish_own_jobs`, without, and with it after a restart on the same state directory | with it: `Miller` of its own job next (also after the restart, from `state_dir/mainnet/open/`), both updates applied, 14 transactions; without: the newer update's `Open`, the first job left open, 12 transactions; nothing refused, no kept job left at the end |
| `config.rs` (5 tests) | the shipped `ferrata-updater.example.toml` and `dev/dev.toml` parse; an unknown preset, a non-hex checkpoint, a zero deferral window, a source with both `url` and `dir`, an unknown field, `ledger = "sequencer"` without `[sequencer]` or with a sequencer not on loopback, an unknown ledger, an unknown layout, a `headers` source without `url` are refused; the example's Sepolia chain (checkpoint `3b901f5e…`, layout `gloas`) is the crown run's instance `a5fb2591…`; a beacon node's header answer is read only when canonical at that slot; the `updates` stream splits under both length conventions | pass |
| `gate_two_honest_one_adversary` | proposal A3's done gate (below) | passes in all eight runs (seven with the adversary, one without), 12 November 2026 |

**Client diversity** (18 November 2026, [`results/client-diversity-2026-11-18/`](results/client-diversity-2026-11-18/README.md)
§2, §5): on the tree of that change **27 tests pass** (6 in the library, 8 in `config.rs`, 13 in `recorded.rs`), clippy
`-D warnings` clean; after an independent review the same day **31** (with `tests/diversity_limits.rs`: its
counterexamples, and its fixes: one host refused twice, `/health` degraded while waiting for clients). New: two clients serving the same bytes catch up as before and one client twice waits
(`distinct_clients_must_serve_the_same_bytes`); a "Lodestar" source serving valid updates three entries ahead of the
other client is followed without the policy (to 11,600,416) and held back with it (at 11,591,680) until the other
serves the same bytes (`an_update_one_client_serves_is_held_back`); a forged-signature source claiming a second client
sends nothing, is demoted, and both sides' disagreements (7 each) are logged and named in `/health`
(`a_lying_client_is_logged_and_does_not_count`); the recorded `/eth/v1/node/version` answers of Nimbus, Lodestar and
Lighthouse over HTTP, in both match modes (`clients_reported_over_http`); the configuration's checks. The lab's record
has grown since 13 November (mainnet 95, Sepolia 125 updates): `restart_mid_job_and_mid_period` now picks its stop
point from the data (it had assumed the third entry from the end was a finality update), and the A3 gate's default
run replays all **220** updates, each applied once, refused gas / applied 0.0603, honest-1's share 0.5027, 1,020
rounds (`results/client-diversity-2026-11-18/suite/gate-default.txt`; the table below keeps the 58-update runs). Running
the suite rewrites the tracked `results/gate.txt` with the grown record's figures; `git checkout` it afterwards to keep
the 58-update file the README and EVIDENCE row 80 cite.

**The A3 gate** (`results/gate.txt`). Two honest updaters (one with a lying source, one with a
lagging one) and an adversarial updater follow mainnet and Sepolia on one chain while the record
replays one update at a time; each round is a block whose transactions settle in a shuffled order;
the adversary races due steps a third of the time without waiting and, at each new update, opens a
job for a forged update and replays an applied one.

| run (`GATE_SEED`: block order; `GATE_SALT`: names, so deferral waits) | default | seed 1 | seed 2 | seed 3 | salt a | salt b | salt c |
|---|---:|---:|---:|---:|---:|---:|---:|
| updates applied, each once (`InstanceState.applied` = 1 + updates, per instance) | 58 | 58 | 58 | 58 | 58 | 58 | 58 |
| honest gas on refused steps / on applied steps (gate ≤ 0.10) | 0.0820 | 0.0871 | 0.0566 | 0.0798 | 0.0655 | 0.0613 | 0.0715 |
| honest-1 share of the honest fees (gate 0.35–0.65) | 0.4428 | 0.4375 | 0.4612 | 0.4477 | 0.4743 | 0.3933 | 0.3945 |
| honest-2 share | 0.5572 | 0.5625 | 0.5388 | 0.5523 | 0.5257 | 0.6067 | 0.6055 |
| honest-1 / honest-2 / adversary transactions applied | 45 / 56 / 149 | 41 / 58 / 151 | 46 / 51 / 153 | 46 / 52 / 152 | 53 / 54 / 143 | 40 / 60 / 150 | 41 / 62 / 147 |
| honest-1 / honest-2 / adversary transactions refused | 18 / 14 / 17 | 22 / 16 / 12 | 16 / 14 / 15 | 20 / 18 / 15 | 15 / 16 / 15 | 17 / 23 / 24 | 18 / 27 / 26 |
| rounds (blocks); ledger transactions | 437; 409 | 444; 410 | 446; 405 | 416; 413 | 419; 406 | 407; 424 | 423; 431 |

Measured on 12 November 2026 with `finish_own_jobs` (the default since then: an updater finishes the
job it opened before it opens another). Without the adversary (`GATE_ADVERSARY=0`,
`results/gate-no-adversary.txt`), the two honest updaters alone: 58 updates each applied once in 737
rounds, 119 and 131 transactions applied, 19 and 15 refused (collisions of equal waits), refused gas
0.0284 of applied, fee shares 0.4762 / 0.5238. The runs of 11 November, on an earlier tree (the
same gate, the same 58 updates each applied once; the default run gives today's figures without
`finish_own_jobs` too, at `2dbad7c`): refused gas 0.0364–0.0932 of applied, honest-1's
share 0.4926–0.6306; without the adversary 0.0172 and 0.5404 / 0.4596 in 695 rounds.

58 updates: 14 mainnet and 44 Sepolia, every update the lab recorded, not the gate's 100. The
adversary also sent 110 junk `Open`s in each run, at its own cost. Files: `results/gate*.txt`.
A wait is a hash of the updater's name and key, the job and the step, so the four `GATE_SEED` runs
share one draw of waits; other names draw others (`GATE_SALT`). The refused share ranges
0.057–0.087 over the seven runs with the adversary, within the gate in all; honest-1's share of
the fees 0.39–0.47. It comes from same-round collisions: two honest updaters with equal waits (1 in
8 per step), and an honest updater sending in the round the adversary races; a refused duplicate
`Open` costs about 2.2M (step 1 runs before the job is found to exist), a refused later step at
most about 24k.

## On a running LEZ sequencer, with live Sepolia data

12–13 November 2026, 19:21Z to 03:29Z (about eight hours, five sessions on two zones): the build of `sequencer/`
kept the crown run's Sepolia instance (`0xa5fb2591…`, from `crates/ferrata-spectests/fixtures/sepolia-2026-11-12`)
following Sepolia on LEZ v0.4.0-rc1 devnets made by the crown run's own session machinery (`lez/crown/session.sh a`,
then `u`; the final programs `816d1fef…`, `ccde6171…`), on Modal (`e2e/crown.sh updater`), from Lodestar's public
light-client data with publicnode's public beacon API as the second source (`kind = "headers"`, `min_agreeing = 2`).
Record, tables and the re-check: [`results/live-sepolia/`](results/live-sepolia/README.md).

| | |
|---|---|
| updates applied end to end | **74** (224 transactions, all applied; none refused, reverted, dropped or rejected; no job left open): 39 on zone 1's final chain, 18 on blocks a preempted container lost (records kept), 17 on zone 2 |
| the period boundary | Sepolia's period 1411 → 1412 (03:00:48Z): the update finalizing slot 11,567,104 rotated the committees (blocks 585–593, 9,901,372 gas), the period-1412 update adopted period 1413's committee (blocks 597–613, five transactions, 16,117,928 gas), then two more updates under the new committee |
| each update | checked by the program's own steps natively before paying, its finalized header confirmed canonical by publicnode; a finality update is three transactions, 9.11M gas, each final on Bedrock 6.9–16.4 s after sending |
| `/health`, `/metrics` | read every minute: 464 of 483 probes `ok`; 5 before the daemon listened; 14 at HTTP 503 while a round waited on a zone out of Bedrock funds |
| restarts from its state | SIGTERM between jobs and twice mid-job (the kept job finished: `Miller`, `Finish`, no second `Open`); the zone (sequencer, node, daemon) saved and restored twice; a container preempted and the zone restored from its last generation (the daemon caught up in one job) |
| re-check | `python3 -I lez/updater/results/live-sepolia/check.py lez/updater/results/live-sepolia/zone-2 --beacon URL`: journal against manifest, fees against the payer's balance, the blocks' own gas totals, the jobs' steps, the instance in crown-driver's export, every root against a beacon node |
| found | a LEZ zone's Bedrock funds for inscriptions ran out after 1,468 blocks (4.1 h), and its sequencer stopped; Lodestar's light-client data lagged the boundary by about 13 minutes; a preempted container loses the zone since its last generation (results README, Findings) |

## Not built, not tested

- HTTPS without `curl`; light-client data as JSON (only SSZ is read).
- A sequencer other updaters also write to: the live run had one daemon; the A3 gate's three daemons
  (and its 100 updates) ran in process only. No public LEZ v0.4 network exists to run on.
- Several days unattended (proposal A4's seven days on testnet 0.4): the live run lasted hours.
- The proven path (`VerifyAll`): the daemon drives the public job only.
- As a Logos module (`modules/ferrata_updater_module`, `logosctl daemon`) it drives the same daemon with either
  ledger: the harness (the in-process chain lives in the module's process) or the sequencer backend above, whose
  library (`sequencer/src/lib.rs`, `open_ledger`) the module links (`modules/scripts/stage-updater.sh` stages it
  with LEZ's node-side crates and their prebuilt circuit and rapidsnark libraries). It ran inside a headless Logos
  Core on a crown zone following Sepolia (`modules/results/updater-sequencer-2026-11-13/`, EVIDENCE row 104).
- Jobs whose `Finish` fails stay open (the program has no clean-up step); the updater moves on.
- Client diversity ran ten minutes live on mainnet, on the in-process chain (two daemons; Sepolia's run did not start:
  Lodestar's API answered HTTP 500 for the bootstraps the run script asked both clients for), not on a sequencer and
  not for days. A provider's reported client is not authenticated. Lighthouse's public node serves light-client data
  as JSON only, which `BeaconApi` does not read (it counts as a `headers` source).
