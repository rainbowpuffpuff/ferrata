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

## The operator's journey

| Step | How | Shown by |
|---|---|---|
| Install | `cd lez && cargo build -p ferrata-updater --features harness --locked` (the in-process chain), or `cd lez/updater/sequencer && cargo build --release` (a running sequencer); or the Logos module `ferrata_updater_module` | the builds of the runs below |
| Configure | one TOML file; [`ferrata-updater.example.toml`](ferrata-updater.example.toml) lists every field with its default (the table below) | `tests/config.rs` parses the shipped files and refuses bad ones |
| Run | `ferrata-updater --config FILE`; no permission, no registration: any account with LEZ gas advances any instance | [`dev/demo.sh`](dev/demo.sh) at this commit ([`docs/guides/runs/2026-11-21-lez/`](../../docs/guides/runs/2026-11-21-lez/README.md): 29 transactions applied in 45 s in that run; how many depends on the machine's speed); 8 h following Sepolia live on a devnet ([`results/live-sepolia/`](results/live-sepolia/)) |
| Monitor | `GET /health` (`ok`, `degraded` with reasons, `down` with HTTP 503) and `GET /metrics` (Prometheus): [Health and metrics](#health-and-metrics) | the demo read `/health` (HTTP 200, `ok`) and `/metrics` while it ran |
| Stop | SIGTERM or SIGINT: the round in hand finishes, exit 0; a restart reads the chain | the demo's exit 0 after SIGTERM; `restart_mid_job_and_mid_period` |

What every other off-chain party can do to an attestation, and why none of them can stop one:
[`docs/trust.md`](../../docs/trust.md) §2.4, each party walked by `tests/every_party.rs`. The whole operator role
(cost, cadence, concurrency, a stalled instance): [`docs/guides/operator-guide.md`](../../docs/guides/operator-guide.md) §3.

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
| `[sequencer]` | `rpc` (the sequencer's JSON-RPC; loopback only, or the one remote sequencer `FERRATA_SEQUENCER_REMOTE` names, sent to only with `FERRATA_TESTNET_SEND=yes`), `program` (the attestation program's account, base58 or 0x-hex), `wait_final` (default `true`: read the chain and settle transactions only in blocks final on Bedrock), `run_dir` (the backend's follower and manifest; default `state_dir/ledger`), `log` (the sequencer's log, for the reason of a dropped transaction) |
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
   Before the verdicts, a candidate signed in the future is refused: its signature slot starts more than 60 s after
   the chain's clock (`Ledger::now_ms`; `FUTURE_MARGIN_MS`, the skew the updater tolerates, as for its windows). No
   honest node serves one (a sync aggregate exists only once its slot's block does) and the program refuses its
   `Open` until then; its source is demoted and nothing else it served in the round is used (internal reviews' findings
   M4 and M1). The in-process chain without a clock (`HarnessLedger::now` unset) checks nothing here.
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
6. **Choice.** An update that can be sent now first: its `Open` window is open by the chain's clock (its signature slot
   has started); then adopting the next committee when the store lacks it; then the furthest finalized header; then the
   earliest signature slot (it decides between copies of one update that can both be sent); then the most signers. The
   signature does not cover `signature_slot` (only its fork version), so a source can serve a real update, or another
   real update of the same finalized header with more signers, re-dated to another slot of its period: to a later one,
   whose `Open` window has not opened, or to one that has started. Re-dated more than 60 s past the chain's clock it was
   refused in item 4. Within that margin it is not refused (a few seconds of clock skew make an honest update look the
   same), but it ranks after every update that can be sent: a source re-dating so, even again every round, holds back
   only an update no other source serves, and meanwhile the instance advances to the furthest update the other sources
   serve. (Before an internal review's M1 such an update ranked first when it adopted the next committee, and every
   round waited for its window, so that nothing was sent until the period ended: 24.2 h of chain time in the review's
   run, a period being about 27 h.) Re-dated to a slot that has started, a copy can be sent and can rank first (adopting
   the next committee, or finalizing furthest). Each copy is a new job, and anyone may open it and send its steps; the
   steps this updater sends of a job it does not carry wait their turns by the instance's state, not by the job (item
   8), so new copies, opened by the source itself or not, delay the instance by a bounded number of rounds on each state
   of the instance and do not hold it (a party applying older real updates itself starts new states: item 8). (Before
   two internal reviews' findings M1, every copy's `Open` was deferred anew, and then every opened copy's next step: a
   source that knew the updater's key held every update back, nothing sent in the 113 and 115 rounds the reviews ran
   after the rotation.) The comparison is the program's own start check, exact: a tolerance would let a source re-date
   to a slot about to start. The signer count does not change the store an update leaves (every update the program
   applies has two thirds of the committee) and breaks the remaining ties (`tests/refusals.rs`: copies of one update, an
   update only the liar serves, two real updates of one finalized header from the client-diversity probe, Lodestar's
   with 511 signers re-dated against Nimbus's with 510, and an update adopting the next committee, or one only the liar
   serves, re-dated 48 s past the clock every round; `tests/redating.rs`: an update adopting the next committee re-dated
   each round to a new started slot, the one whose `Open` the updater defers longest; the same with the source opening
   each copy itself, or opening it and sending its `Miller`; the strongest attacker known against the turns of item 8;
   and the earlier attacks' variants).
7. **The job's next step, from the chain.** A job's id hashes the instance, its starting store and
   the update, so every updater computes the same id for the same update. No job yet: `Open`. A job
   in `Miller`: `Miller`. Adoption pending: the next `Adopt` part. Otherwise `Finish`. The updater
   continues the open job of the update it chose, its own or another's, and after a restart its own. A job it sent a step
   for (its own `Open`, or a later step of a job another party opened) comes first, before any other
   update is chosen (`finish_own_jobs`, default): a beacon node serves only its newest finality
   update, so the update a job was opened for can be gone from every source before the job's last
   step, and the steps already paid would be lost; and a party that opens the job of every new update
   and leaves it would otherwise have the updater pay a `Miller` each round for a job it then left
   (an internal code review's UP-M3, `tests/jobs.rs`). The update is kept in `state_dir/<chain>/open/` until
   the job is finished or moot (the store moved past it), so a restarted updater finishes it too.
   A kept job whose next step is held after a refusal (item 10) stays kept while the hold will end,
   until that step has been refused three times; then the updater leaves it to another party (a
   refusal for good, by the program or out of gas, leaves it at once).
   With `finish_own_jobs = false` each round takes the best update, and a job left behind keeps the
   steps already paid for it.
8. **Deferral.** Each (job, step) is sent by the updater whose wait, `H(key, job, step) mod defer_window` rounds after
   it first saw the step due, runs out first; the others find the step done and skip it. Two honest updaters collide on
   a job's step only when their turns for it end in the same round. A step of a job this updater carries (it sent a step
   of it, item 7) counts from the first round that step was due. An `Open`, and a step of a job it does not carry, count
   from the first round a step of that kind was due on the instance's state, whatever job it is for; after it sends a
   step there, the next of that kind waits `defer_window - 1` rounds. A job's id hashes `signature_slot`, which a source
   can change at will; any beacon node can build another real update of the period from each of its blocks; and anyone
   may open a job and send its steps. Counted by the job, a source serving a new copy each round, or opening each copy
   itself (two internal reviews' findings M1), had this updater count again each round. Now each kind of
   step has one turn per state, at most `defer_window - 1` rounds, whatever the sources serve and others send; a source
   can only present jobs at another step, each kind's turn running once. So, on each state of the instance, this
   updater's first step goes out within `K × (defer_window - 1)` rounds of the first round one was due there (`K = 3 +
   adoption_parts` kinds: `Open`, `Miller`, the adoption's parts, `Finish`), and the job it then carries takes each of
   its other steps within `defer_window` rounds: an update adopting the next committee is applied at most `K ×
   (defer_window - 1) + 1 + (K - 1) × defer_window` rounds after the first round it was due on that state (68 at the
   defaults, `defer_window` 8 and two parts; measured 66 rounds with the strongest attacker known in
   `tests/redating.rs`, and 67 with every slot of the period searched in an independent review run, not reproduced by a
   committed test; 16 to 26 with the openers,
   30 with the started slots). The turns start again whenever the state changes, and anyone may change it by applying a
   real update: a party that applies older real updates itself starts a new state with each, and new turns ("Not built,
   not tested"). The key is the updater's name and its payer's account, both public: a source can pick copies whose
   steps wait longest, but not past these turns. After a step is sent, a fresh turn whose wait is drawn from each
   round's job would end at the first round whose draw had run out: with `finish_own_jobs = false` and sources whose
   best update changes every round, an `Open` went out every third round or so (an internal review's L2). With sources
   whose best update changes every slot (honest beacon nodes serving their newest finality update), a second updater
   opens its own job for the newer update once its `Open` turn on the state has run. Measured in an independent review
   run, not reproduced by a committed test, over four updates against the code before the per-state turns: two updaters
   in one slot paid 51% more per applied update (2 `Open`s per update against 1) and three 28% more (2.25 against 1.5),
   about the same with one updater or two a slot apart; with two or three updaters every update was applied within its
   15 rounds (before them, 2 to 4 of 4), and no transaction was refused.
9. **Send.** Within `max_fee_per_tx` and `budget`; a transport failure is retried with
   exponential backoff (`retry_base_ms * 2^(attempt-1)` after attempt 1, 2, ..., at most
   `retry_max_ms`, `max_attempts` attempts), and just before sending and before every retry the
   chain is read again: a step another party sent, or a store that moved, since the round read the
   instance is not sent (UP-L5). A read that fails is tried again at a later round; after a refusal, a step whose chain cannot be read again is held, and judged at the next round that reads the chain: a race (the hold dropped, nothing counted) or a step that stayed due (a failing send and a strike, as any; `tests/refusals.rs`).
   Every sent transaction is a line of `state_dir/<chain>/journal.tsv` (round, step, job, finalized
   slot, result, exit code, execution and storage gas, fee, block, the time it settled, its label, the
   chain's note: a refusal's reason).
10. **Windows and refusals** (an internal code review's UP-H1, `tests/refusals.rs`). The program gives `Init` a
   timestamp window ending with its checkpoint's weak-subjectivity period and `Open` one from its
   update's signature slot to the end of the store's period (`ferrata-advance`'s `init_window`,
   `open_window`), and LEZ charges a transaction executed outside its window the whole gas limit. So
   neither is sent unless its window stays open 60 s past the chain's clock (`Ledger::now_ms`: for the
   sequencer backend this machine's clock: a sequencer on this machine stamps its blocks with the same
   clock; the one remote sequencer the owner may name for the day-one deployment
   (`FERRATA_SEQUENCER_REMOTE`, sending only with `FERRATA_TESTNET_SEND=yes`) stamps them with its own,
   so the 60 s margin covers a small skew and a refusal by a larger one is held like any window
   refusal below; the in-process harness takes each transaction's own time, `HarnessLedger::now` sets it);
   outside it the plan is stuck with what to do ("initialise from a newer checkpoint", "initialise a
   new instance"), and `/health` says `down`. After a refusal the chain is read again (up to
   `max_attempts` times): a step another party's transaction made done or moot was a race and nothing
   is held; a step still due would meet the same state, so it is held: for good if the program refused
   it or it ran out of gas (out of gas: until `fees.gas_limit` is raised), and for a window or an
   unknown reason 4 rounds, then 8, 16, 32, then every 64 rounds, sent again after each hold, never
   given up (review of the fixes, L3); if the chain cannot be read after the refusal, the step is held
   4 rounds (doubling if that happens again to the same step) whatever the cause, and judged at the next round that
   can read the chain: done or moot, a race (the hold dropped, no strike, not a failing send: on the sequencer backend a
   read fails while another party's transaction touching the instance is in flight, which is when a race is lost; L4);
   still due, a send that did not land and a strike, as any refusal of a step that stayed due (9c L-U3: it used to be
   neither, whatever followed). A hold for good is kept while the instance holds the state its step was refused on
   (a job commits to the store it starts from), across restarts: `state_dir/<chain>/held.tsv` (9c L-U5: it used to
   end after 4,096 rounds, about 13.7 h at 12 s rounds, and at every restart, and the step was refused and charged
   again); a hold that ends is forgotten 4,096 rounds after its last refusal. Three refusals in a row of steps that stayed due pause
   sending for 8 rounds, then 16, ... up to 512, with one step tried after each pause: this bounds what
   a refusal nothing here foresees can cost, also when every round brings a new job. The reason is in
   the log line, the journal, `/health` and the metrics.

## An execution endpoint (`kind = "execution"`)

A source of `kind = "execution"` is an Ethereum execution endpoint: a JSON-RPC `url` (read with `curl`, as the beacon
sources), or inside Logos Core `via = "eth_rpc_module"` with `chain_id` (the endpoint, proxy and verified-proxy
policy that module holds for that chain). It serves no light-client data. It confirms an update's finalized header by
the execution block that header binds (the payload header's block hash; in the Gloas layout the block hash itself): the
endpoint must hold that block canonical at its number (`eth_getBlockByHash`, then `eth_getBlockByNumber`), and then it
counts towards `policy.min_agreeing` for the update, as a `headers` source does for the beacon block. With
`min_agreeing` equal to the number of sources, no update is sent before the execution endpoint holds its block; an
endpoint that does not is named in `/health` (`degraded`) and the updater waits. It reads nothing else: the program
checks the update itself, and an execution endpoint cannot make it accept one it would refuse
(`execution_source_confirms_or_holds_back`, `tests/execution.rs`).

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
slot and period, the last plan and error, the steps held after a refusal, the pause, the sends that
did not land, the rounds that ended in an error, the last applied transaction's time, how far the
sources are past the instance, the rounds without progress, and the time it was written); HTTP 503
when an instance is `down` or when no round has refreshed the answer for ten polls (at least 600 s: a
round hung on a settle wait or a source). It reflects what lands on the chain (an internal code review's UP-M1,
`tests/health.rs`):
`degraded`: a source demoted, lagging or unavailable, a `headers` source holding another block at a
finalized header's slot, two sources serving other bytes for one update or waiting for clients beyond `max_lag_slots`
(`min_distinct_clients`), a transaction it sent that did not land (rejected, refused while its step
stayed due, failed at transport after every retry; refused with the chain unreadable after it: from the round that
reads the step still due), a round that ended in an error, the last round
stuck, sending paused, or no progress: the instance's finalized slot unmoved for
`6 × defer_window + 10` rounds while the sources serve updates more than `max_lag_slots` past it.
`down` (HTTP 503), whenever this updater moves nothing once the instance is active (before that, a step held for good, such as the initialization's `Adopt` out of gas, is `degraded`, naming what to change): no light-client source usable and answering;
five sends in a row that did not land; five rounds in a row that ended in an error (a chain that
answers neither reads nor sends fails each round before anything is sent; so does a round that cannot fetch the
committees the store summarises, 9c L-U2: it used to stay `degraded`); the instance, or before
`Init` its checkpoint, past its weak-subjectivity period by the chain's clock (a person must
initialise a new instance, or start from a newer checkpoint); no progress for twice the rounds above,
`2 × (6 × defer_window + 10)` (a step held, sending paused, too few sources or clients agreeing, round
after round); or the budget reached (the review of the fixes, M1: an unreachable chain and an
instance past its period stayed `degraded`, HTTP 200, in 200 rounds of 200). After an outage longer
than that, `/health` stays `down` until the instance moves again (measured: three rounds, one job).

The listener serves each connection on a thread of its own: at most eight at once with two seconds
to send the request head, of those at most four from one host other than loopback (an IPv4 address, or an IPv6
address's /64 network, from any address of which one host may answer; 9c L-U4f: each IPv6 address used to count as a
host; on loopback every client is this machine, one address: they share the eight); a connection beyond those is still
served, on at most eight more threads, if its request head arrives within 250 ms, as a monitor's does
(the request goes out with the connection); beyond those it is closed at once. A head is at most 8 KiB
(`400` beyond, and the client receives it: what it still sends is read for a moment before the close).
A connection's place is given back before it is closed, so a client that connects again at once
finds it. So one idle or slow client neither blocks another nor grows the daemon (an internal operations review's M3),
and clients that keep re-opening idle connections no longer keep a monitor out (the review of the
fixes, L1: eight such clients used to deny `/health` while they kept at it). To keep a prompt request
out, clients must hold the eight places and fill the eight quick ones again at least every 250 ms; one
other host gets at most four of the eight, so it cannot; several hosts, or a process on this machine,
could: keep `listen` on loopback (`tests/serve.rs`). Every answer
a source gives through `curl` is read to a bound (an `updates` answer 8 MiB, a bootstrap or finality
update 1 MiB, a JSON answer 64 KiB), with `curl -q`, HTTP(S) only, no redirect, the URL passed with
`--url`; a `beacon_api_module` reply is refused past 16 MiB once received (the module hands its reply
over whole: that bound limits parsing, not memory); a provider's message reaches the log and
`/health` as at most 200 printable characters (UP-M2, `tests/bounds.rs`).

`GET /metrics` (Prometheus text, labels `daemon`, `chain`, `source`): `ferrata_updater_rounds_total`,
`_tx_total{step,result=applied|refused|rejected}`, `_refused_by_code_total{code}`,
`_refusals_total{cause=window|gas|program|other,after=race|due|unread}`, `_held_steps`, `_sending_paused`,
`_failing_sends`, `_failed_rounds`, `_past_weak_subjectivity` (1: the instance or its checkpoint is
past its weak-subjectivity period), `_sources_head_slot`, `_rounds_without_progress`,
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
has grown since 13 November (mainnet 108, Sepolia 138 updates): `restart_mid_job_and_mid_period` now picks its stop
point from the data (it had assumed the third entry from the end was a finality update), and the A3 gate's default run
replays all **246** updates, each applied once, refused gas / applied 0.0752, honest-1's share 0.4841, 1,101 rounds
(`results/gate.txt`, which CI's `lez/updater/tests/recorded.rs` writes; the table below keeps the 58-update runs).
Running the suite rewrites the tracked `results/gate.txt` with the record's figures.

**Review fixes** (22 November 2026: the findings of the code reviews of 21 November, then those of the review of
these fixes): **64 tests pass** (10 in the library, 8 in `config.rs`, 4 in `diversity_limits.rs`, 13 in
`recorded.rs`, and the files below), clippy `-D warnings` clean with and without `harness`; the A3 gate's default run
on the grown record (246 updates, 1,101 rounds) passes: honest refused / applied gas 0.0752, shares 0.4841 / 0.5159.

**Liveness fixes** (internal reviews of 22 November 2026: one lying source holding every
update back, and the Lows L-U2, L-U3, L-U4f, L-U5, L-U6 and L-U7; then a source re-dating within the margin every
round, and that review's Lows and nits; then a source re-dating to slots that have started, a new job each round,
opened by itself or not, and an `Open` every third round or so with `finish_own_jobs = false`): **92
tests pass** (11 in the library, 3 `bounds`, 8 `config`, 1 `daemon_clock`, 4 `diversity_limits`, 8 `every_party`, 8
`health`, 2 `jobs`, 13 `recorded`, 10 `redating`, 16 `refusals`, 1 `replacement`, 7 `serve`), clippy `-D warnings` clean
with and without `harness`, fmt clean; the A3 gate's default run writes `results/gate.txt` byte for byte as before (with
one candidate per state every wait is as it was). Each new test fails with its fix undone: `python3 -I
lez/updater/dev/mutants.py NAME` (from the root) undoes one fix in the sources, runs the tests it added and puts the
sources back, also when stopped by Ctrl-C, SIGTERM or SIGHUP (`--check`: every mutant still applies;
`dev/test_mutants.py` checks the restoring). The table gives the files as they stand.

| test | what it shows | result |
|---|---|---|
| `refusals.rs` (16) | `Init` and `Open` with the chain's clock (`HarnessLedger::now`) at and past the window's end; out of gas; a new refused job every round; a race; a store moved while planning; a refusal of unknown cause three times; races each followed by failed reads; a refusal of unknown cause three times with every read after it failing; a source re-dating updates: a copy, an update only it serves, another real update of one finalized header with more signers (600 slots, or 24, 60 or 72 s past the chain's clock), and, again every round 48 s past the clock, an update adopting the next committee or one only it serves; a hold for good across 4,106 rounds and a restart | at the end minus 60 s or past it nothing is sent, 0 charged, `/health` `down` naming the action; 1 ms inside, sent and applied, `ok`. `gas_limit` 1,000,000: `Adopt` refused once in ten rounds, nothing charged after, released when the limit is raised. 5 refusals in 40 rounds where each round brings a new job (the breaker). A refusal lost to another updater (exit 207) not held. A step made moot while planning is not sent (sent anyway: refused, fee 1,706,408). A `Miller` refused three times for an unknown cause: sent again after 4, 8 and 16 rounds, lands, the instance moves. 9 races each followed by one failed read: all races, no strike; with every read failing: held, judged a race the next round (the hold dropped), no strike, no failing send. The same `Miller` refusals with every read after each failing and no other party: each judged due the round after (failing sends 1, 2, 3; strikes 1, 2, 3; `/health` "did not land" from round 3), the third strike pauses sending (rounds 15-23), the fourth lands in round 30, `ok`. A source re-dating every finality update 600 slots, listed first: 4 rounds, 0 waiting, the update applied, the liar demoted (`/health` names the 7,188 s); a re-dated update only the liar serves, the other source one update behind: 4 rounds, 0 waiting, the other's update applied (15,626,400 → 15,627,008); Lodestar's real update of the probe (511 signers) re-dated against Nimbus's of the same finalized header (510): Nimbus's applied in 4 rounds whether the re-dated slot starts 24 s or 60 s (the margin itself) past the chain's clock, the liar not demoted, or 72 s or two hours past it, the liar demoted; with the fix undone, 30 of 30 rounds waiting and the store unmoved. A real update adopting the next committee (the record's entry 12) re-dated 48 s past the chain's clock every time it is asked, the store without its next committee after a rotation: 0 of 60 rounds waiting, the honest period update (entry 10) adopted at once, entry 20 applied (15,635,392) once served, the liar not demoted, `ok` (with the order of `81bbea61`: 27 of 30 rounds waiting, the committee not adopted, then 30 of 30 with entry 20 unsent, `down`); an update only the liar serves, re-dated so: the other source's applied in round 3, then 27 rounds waiting for the liar's window alone, `/health` `degraded` naming no progress (with that order: 30 of 30 waiting, the store unmoved). The initialization's `Adopt` out of gas held for good: 1 refusal in 4,106 rounds, `held.tsv` one line; restarted on the same directory: still 1 refusal ten rounds later; raised: applied, `held.tsv` empty (undone: 2 refusals in 4,106 rounds) |
| `redating.rs` (10) | `defer_window` 8 unless named. The store lacking its next committee: a source re-dates entry 12 (it adopts it) each round to a started slot it has not used, the one whose `Open` the updater would defer longest (its key computed from its name and payer account); the same against `finish_own_jobs = false`; the source opening each copy itself, a new copy every 7 rounds or every round, or opening it and sending its `Miller`, copies chosen by the updater's wait for the step then due; the source using up each kind of step's turn in turn, then leaving a job to carry, its later steps chosen to wait longest. `finish_own_jobs = false` with two honest sources serving their newest finality update each slot. The earlier attacks' variants (re-dating within the margin): re-dated a few slots past the clock, varying each round (the review's schedule: 1 to 5 slots; 0 to 5 by SplitMix64; `defer_window` 1 and 8); two sources re-dating alike beside one or two honest sources (`min_agreeing` 2; two clients, `bytes`; `defer_window` 1); a source re-dating 600 slots on every other answer | the started slots: rotated in round 5, the copy's `Open` deferred 7 rounds, the next committee adopted in round 35, entry 20 applied (`m4`: 55 `Open` deferrals, never adopted); `finish_own_jobs = false`: 6 `Open`s of the copies in the 52 rounds after the rotation (`m4reset`: 45); the opener: adopted 16, 26 and 23 rounds after the rotation, entry 20 applied, nothing refused, the source not demoted, `/health` `ok` (`m6`, and the code with only an `Open`'s turn counted by the state: never adopted, nothing sent after the rotation); the cycling source: adopted 66 rounds after the rotation, the bound on one state being 68; newest-only sources with `finish_own_jobs = false`: 8 `Open`s in 60 rounds (`m7`, a fresh turn after each send, and the code before the whole turn after a send: 16; `m4reset`, no new turn: 56); the variants: adopted in round 8 with `defer_window` 1 and in the two-liar cases, in rounds 33 and 28 (rotated in 15 and 10) with `defer_window` 8, entry 20 applied, no round waiting (`m1roll`: 57 of 60 rounds waiting, then 30 of 30, never adopted, `down`); the 600-slot liar demoted 5 times, every update applied (`m1`: never demoted) |
| `health.rs` (8) | every send rejected (no funds), failing at transport, the only source down, the instance not moving, the chain unreachable for 200 rounds, the instance past its weak-subjectivity period for 200 rounds, a daemon cut off from its chain, a restart without the committee cache whose source serves only the bootstrap | degraded in rounds 1-4, `down` in round 5, `ok` once one lands; `down`; degraded after 16 rounds without progress, `down` after 32 (`defer_window` 1); unreachable: degraded in rounds 1-4, `down` in 5-200, `ok` once it answers and the instance moves; past its period: `down` in 200 of 200, nothing sent; the daemon's `/health`: 200 in rounds 1-4, 503 from round 5; the committees not served (store in period 1908): degraded in rounds 1-4, `down` from round 5, naming them (undone: the round ends `stuck`), the next round without an error once a source serves them |
| `jobs.rs` (2) | ten rounds, a new finality update each, another party opening its job first and leaving it; a kept job whose `Miller` is refused once while the sources move on | `Miller` then `Finish` of each job continued: 5 updates applied, 5 `Miller`s and 5 `Finish`es (asserted exactly), nothing refused; the kept job kept through its 3 held rounds and finished: 2 `Open`s for 2 applied updates |
| `bounds.rs` (3) | answers at and one byte past their bound, with and without a length; 256 MiB; module replies; provider messages | exact bound read, one more refused; 256 MiB cut after 1 MiB: the test server pushed a few MiB of it before curl was stopped (4,194,304, 6,291,456 and 6,291,456 bytes in three runs: socket buffering decides; asserted under 64 MiB: a reader that took the answer whole would have taken all 256 MiB; with curl's limit and the reader's bound both removed, the test fails), and where the kernel reports `VmHWM` the peak RSS is checked too (the box's gVisor does not); `file://` refused; messages kept to 200 printable characters |
| `daemon_clock.rs` (1) | the clock the windows are checked against | a `sequencer` backend gets this machine's clock; the harness, and a wrapper of it that says nothing, each transaction's own time |
| `serve.rs` (7) | an idle connection, a request with no end, a trickling client, 24 connections, eight clients re-opening idle connections for 8 s, 400 connections at once, a stale snapshot | `/health` in 0.8 ms beside idle connections; 8,192-byte head answered, 8,193 answered `400`; 64 MiB offered without a newline: about 1.5 to 1.7 MiB got through before the server ended the connection (1,769,472, 1,572,864 and 1,572,864 bytes in three runs; asserted under 32 MiB; the peak RSS checked where `VmHWM` is reported); 24 connections: `/health` at once (0.6 ms); the eight clients: 80 of 80 tries answered; 400 at once: 8 kept open; `503` when stale |
| `diversity_limits.rs` (4) | as above, and waiting for clients | degraded from the first round waiting, `down` once the instance has not moved for 32 rounds |

**The A3 gate** (the runs below are of 12 November 2026, over the 58 updates recorded then; `results/gate.txt`
holds the default run over the grown record's 246 updates, as the paragraph above gives it). Two honest updaters (one
with a lying source, one with a lagging one) and an adversarial updater follow mainnet and Sepolia on one chain while the record
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
- A chain clock (`Ledger::now_ms`) more than 60 s behind Ethereum's: honest sources' newest updates look signed in the
  future, are refused and their sources demoted (`/health` names the gap in seconds). A clock behind Ethereum's by more
  than the age of the newest update when it is read (up to a slot and a few seconds) keeps every source that serves only
  its newest update waiting for its window: nothing is sent and no source is demoted until the clock is fixed (`/health`
  raises the stall alarm). An independent review run measured, on the in-process chain with two sources serving their
  newest update each slot, every update applied with the clock up to 13 s behind and none from 15 s (not reproduced by a
  committed test). Behind by less, the newest
  honest update can rank after an older one that can be sent (item 6): the older may be applied first (one more job and
  its fees).
- A source re-dating within the 60 s, alone in serving the newest finalized header, holds it back until another source
  serves it or a later one, and is not demoted (`/health` names the instance not moving once that lasts the stall
  alarm's rounds).
- One lying light-client source can still delay this updater, not hold it. Re-dating a real update to slots that have
  started, a new copy each round, opening the copies itself and sending their steps (anyone may), and choosing them with
  the updater's key (computed from its name and payer account, both public), it can use up the turn of each kind of step
  on the instance's state before the updater sends one, then have the job the updater carries wait its full turn at each
  step (item 8): on each state of the instance at most `K × (defer_window - 1) + 1 + (K - 1) × defer_window` rounds (68
  at the defaults; measured 66), each update it delays a real one, applied. It is not demoted (a started slot is what an
  honest update carries), and the stall alarm (`6 × defer_window + 10` rounds, 58 at the defaults) can fire before the
  update lands. The turns start again with each new state. A party that also applies real updates itself, older than the
  newest the sources serve (paying their jobs), starts a new state with each, and new turns: it can postpone the
  adoption of the next committee, and keep the instance behind the newest update, for as long as it has such updates to
  apply. The instance moves at each one, so the stall alarm need not fire. Measured in an independent review run, not
  reproduced by a committed test, the party applying the record's entries 14 to 20 one after another: with the lying
  source using up the turns on each state, the next committee adopted 475 rounds after the rotation (58 to 60 rounds per
  state; `/health` not `ok` in 4 rounds), or 317 with each reset right after this updater's first step on the state
  (`ok` throughout); with no lying source at all, 165 rounds, the same before the per-state turns. Such a party needs no
  lying source and pays for every update it applies, and each update it applies must move the finalized header to a
  later checkpoint (about an epoch, 32 slots). Alone, it is limited by the older real updates it has: without a lying
  source the states last less than an epoch (165 rounds for the seven updates of the run above; at the default
  `poll_secs` 12 a round is a slot), so the store gains on the chain and the party soon has none left. With a source
  using up the turns, states last 58 to 60 rounds, longer than an epoch: the store falls further behind the chain at
  each reset, so the party can hold each adoption of the next committee until the store reaches the end of its period,
  and again in the next, for as long as it pays and the source is kept (derived, not measured: the store then crosses a
  period in about 1.8 periods of real time). If the instance keeps falling behind, remove or replace a source that
  serves such copies (the journal and the per-source metrics show which one served the updates applied); another updater
  whose sources exclude it is not delayed by it.
- A sequencer whose clock lags the updater's (with the sequencer backend the updater takes this machine's clock for the
  chain's; a remote sequencer stamps its blocks with its own) by more than the time from a slot's start to the round: a
  source re-dating to the slot that has just started makes `Open`s the updater sends and the chain refuses, each charged
  the whole gas limit, three in a row pausing sending (item 10), for as long as the source does so and the clocks differ
  (an internal review's L1, measured with that review's test, not committed: with the sequencer 2 s behind and rounds
  half a second into a slot, 6 refused and 71 of 80 rounds paused with `defer_window` 1; at this commit with
  `defer_window` 8, 5 refused, three pauses and the next committee not adopted in 80 rounds, as before the turns of item
  8 were counted by the state). Keep the sequencer's clock synchronised with the updater's, or run the updater beside
  it.
- With `finish_own_jobs = false` and sources whose best update changes every round (honest beacon nodes serving their
  newest finality update, or a source re-dating its copies), the updater takes the newest update each round and leaves
  the job it opened: it opens a job at most once per `defer_window` rounds and finishes none (`tests/redating.rs`: 8
  `Open`s in 60 rounds with two honest sources serving their newest update each slot; 16 without the whole turn after a
  send). Do not use it with such sources; the default finishes the job it opened.
- Tested on the in-process chain with its clock set (`HarnessLedger::now`), not on a sequencer.
- The bound tests' peak-RSS checks need `VmHWM` in `/proc/self/status` (and `/proc/self/clear_refs` to reset it); the
  build box's gVisor reports neither, so there the byte counts are the check.
- The review fixes on a sequencer: every measurement of them ran in the in-process chain. A sequencer's real
  refusals (the note the backend passes on when a receipt has no exit code), a clock that differs from the
  sequencer's near a window's end, `/health` under a real settle wait, and reads that fail while another daemon's
  transaction touching the instance is in flight (R.9 with several daemons) are untested there.
- Client diversity ran ten minutes live on mainnet, on the in-process chain (two daemons; Sepolia's run did not start:
  Lodestar's API answered HTTP 500 for the bootstraps the run script asked both clients for), not on a sequencer and
  not for days. A provider's reported client is not authenticated. Lighthouse's public node serves light-client data
  as JSON only, which `BeaconApi` does not read (it counts as a `headers` source).
