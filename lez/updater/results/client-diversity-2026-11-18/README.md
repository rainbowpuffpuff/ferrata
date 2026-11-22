# Client diversity for the updater (`policy.min_distinct_clients`), 18 November 2026

What [`../../README.md`](../../README.md#client-diversity-policymin_distinct_clients) describes, tested and run: the
updater sends an update only when sources of `k` different consensus clients agree on it (byte for byte, or on its
finalized header). Everything here ran on a Modal build box (16 cores, gVisor); the public beacon APIs were only read
(GET only), with the User-Agent `ferrata-updater` and nothing that identifies the caller; nothing was
sent to Ethereum, Sepolia or any LEZ network: the chain the updater wrote to is LEZ v0.4.0-rc1's state machine in
process (`ledger = "harness"`, `RISC0_DEV_MODE=1`, no proofs).

## 1. The probe: which client each public API runs, and whether two clients serve the same bytes

`probe.py`, 18 November 2026, 16:06–16:10Z, from a Modal build box: read-only GETs, User-Agent `ferrata-updater`,
nothing that identifies the caller. Twenty rounds 12 s apart; in each, every endpoint was asked for its finality
update at the same moment (threads); then the period updates of the last five periods. Records: `probe/<net>/probe.jsonl`
(every answer, decoded), `probe/<net>/summary.md` (the tables), `probe/<net>/answers/` (each endpoint's
`/eth/v1/node/version` answer, its first finality update, and the period updates, one file per period because every
endpoint served the same bytes).

| network | endpoint | `/eth/v1/node/version` | light-client data |
|---|---|---|---|
| mainnet | `http://testing.mainnet.beacon-api.nimbus.team` | `Nimbus/v26.10.2-0abc0d-stateofus` | SSZ (Electra layout) |
| mainnet | `http://unstable.mainnet.beacon-api.nimbus.team` | `Nimbus/v26.10.2-3a36af-stateofus` | SSZ |
| mainnet | `https://lodestar-mainnet.chainsafe.io` | `Lodestar/v1.51.0/7923280` | SSZ |
| mainnet | `https://ethereum-beacon-api.publicnode.com` | `Lighthouse/v8.3.3-57c2abc/x86_64-linux` | JSON only: asked for SSZ, it answered JSON (`..._finality-round0.json`); the updater reads SSZ |
| Sepolia | `http://unstable.sepolia.beacon-api.nimbus.team` | `Nimbus/v26.10.2-5628de-stateofus` | SSZ (Gloas layout) |
| Sepolia | `https://lodestar-sepolia.chainsafe.io` | `Lodestar/v1.51.0/7923280` | SSZ |
| Sepolia | `https://ethereum-sepolia-beacon-api.publicnode.com` | `Lighthouse/v8.3.3-57c2abc/x86_64-linux` | none (404) |

Also tried in a first, one-round pass (16:01Z): `https://www.lightclientdata.org` (HTTP 503) and
`http://testing.sepolia.beacon-api.nimbus.team` (no answer).

| | mainnet | Sepolia |
|---|---|---|
| rounds in which Nimbus and Lodestar served the same finality update | **0 of 20** | **1 of 20** |
| ... the two Nimbus nodes the same | 18 of 20 | — |
| finality updates Lodestar served (distinct) / of them served by Nimbus too, in any round | 2 / 1 (`789e4e83…`, Nimbus in round 18, Lodestar from round 19) | 2 / 1 (`0407a1aa…`, both in round 10) |
| period updates of the last five periods the same from every endpoint | **5 of 5** (1910–1914) | **5 of 5** (1412–1416) |
| disagreements (the same attested header and signature slot, other bytes) | 0 | 0 |

Why: the two implementations keep different finality updates. Lodestar served the first update of a newly finalized
checkpoint (attested at the epoch's first slot) for the whole epoch (mainnet: `be04b506…`, attested at 15,682,817, in
rounds 0–18); Nimbus served the newest each slot (the attested slot rose by one a round). Both are honest and both
verify; they serve the same bytes during one slot of each epoch, when Nimbus's newest is that first update. The best
update of a period (`is_better_update`) is the same in both, byte for byte.

So the strict rule, byte for byte from two clients, holds for period updates whenever asked, and for finality updates
only if the updater remembers what each client served (`served_by`, built after this probe): asked in the same round
only, it would almost never send a finality update.

## 2. The suite (recorded data, stub providers that lie)

`cargo test --locked -p ferrata-updater --features harness -- --nocapture` (`RISC0_DEV_MODE=1`), on the tree of this
change: **27 passed, 0 failed** (6 in the library, 8 in `config.rs`, 13 in `recorded.rs`; [`suite/tests.txt`](suite/tests.txt));
`RISC0_SKIP_BUILD=1 cargo clippy -p ferrata-updater --features harness --all-targets -- -D warnings` clean
([`suite/clippy-summary.txt`](suite/clippy-summary.txt)); `cargo fmt` applied. Steps and seconds:
[`suite/run-steps.txt`](suite/run-steps.txt); 17:45–18:01Z, the suite and the live runs of §3 at the same time. The box
keeps no git history (`suite/run-env.txt` shows it); the sources it ran are those of commit `7c1784bb` (`5aa2e701` and
the `cargo fmt` that ran first on the box, formatting only). A first run of the same suite at 16:13Z (all new tests
passed) found the failing restart test below; a second was lost with its container. The new tests:

| test | what it shows | result |
|---|---|---|
| `distinct_clients_must_serve_the_same_bytes` | the lab's mainnet record from two sources reporting the recorded Nimbus and Lodestar answers, `k = 2`; then two sources both reporting Lodestar | two clients: caught up to the lab's final root (finalized slot 15,676,800), nothing refused, `/health` ok, `client_info` metrics and `/health` clients as reported; one client twice: 27 of 30 rounds waiting after the initialization, "1 of 2 clients agree byte for byte (lodestar)" |
| `an_update_one_client_serves_is_held_back` | Sepolia record; a "Lodestar" source 3 entries ahead (valid updates no other client serves yet: the stand-in for a fork one implementation, or a captured committee's accomplice, serves) beside a "Nimbus" one | `k = 0`: followed the source ahead to 11,600,416 (the other serves up to 11,599,200); `k = 2`: held at 11,591,680 and waits; once the other client serves the same bytes, both reach the record's end (11,600,992); nothing refused |
| `a_lying_client_is_logged_and_does_not_count` | a source with forged signatures reporting Lodestar beside an honest "Nimbus"; then a third honest client | nothing after the initialization is sent; the liar demoted by the program's checks; 7 disagreements counted for each; `/health` degraded naming "lodestar(BadSignature) demoted" and "nimbus serves other bytes than lodestar(BadSignature) for the period-1912 update"; with a third client: caught up to 15,676,800, nothing refused |
| `clients_reported_over_http` | loopback beacon nodes answering `/eth/v1/node/version` with the recorded answers ([`../../tests/fixtures/node-version/`](../../tests/fixtures/node-version/SOURCE.md)): Nimbus and Lodestar (bytes); Lodestar and a Lighthouse `headers` node (finalized header); the `headers` node reporting Lodestar too | caught up to 15,676,800 in both; one client twice: waits, "1 of 2 clients agree its finalized header (lodestar)" |
| `client_diversity_configuration` (config) | off by default; 1 refused; too few sources able to count refused; `client` only for a recorded dir; `finalized-header` lets a `headers` source count | pass |
| `node_version_answers` (config) | the three recorded answers read as `nimbus`, `lodestar`, `lighthouse`; a hostile answer (quotes, newline) kept to printable ASCII; an empty one refused | pass |
| `distinct_clients_from_distinct_sources` (unit) | `k` clients must come from `k` different sources | pass |

Found on the way: the lab's record has grown since the updater's tests last ran (Sepolia 44 → 125 entries, mainnet
14 → 95, lab sessions of 14–17 November), and `restart_mid_job_and_mid_period` assumed the third entry from the end
was a finality update, no longer true (it fails on `main` as it is); it now picks the last finality update that moves
the store inside its period (entry 123 of 125, period 1416). On the grown record the A3 gate's default run (policy off)
replays **220 updates** (95 mainnet, 125 Sepolia), each applied once, refused gas / applied 0.0603, honest-1's share
of the fees 0.5027, 1,020 rounds, the same in both runs of the day ([`suite/gate-default.txt`](suite/gate-default.txt),
[`suite/gate-default-run1.txt`](suite/gate-default-run1.txt)); `../gate.txt` keeps the 58-update figures the README
and EVIDENCE row 80 cite.

## 3. Live: the updater with the policy, against public APIs

`live/live.py` (box script beside the repository: `live.sh`), 17:46–17:56Z, ten
minutes, two daemons at once, each from the checkpoint of mainnet period 1913's update (finalized slot 15,672,512,
root `0x76b355f9…6f62`) on an instance of its own, polling every 12 s:

| | `live/mainnet-bytes/`: `k = 2`, `bytes` | `live/mainnet-header/`: `k = 3`, `finalized-header` |
|---|---|---|
| sources | `nimbus` (testing.mainnet…nimbus.team), `lodestar` (lodestar-mainnet.chainsafe.io) | the same two, and `lighthouse` (publicnode, `headers`) |
| transactions | 16, all applied | 19, all applied |
| updates applied, in order (finalized slot, when final) | period 1913's update 15,672,512 (17:47:46Z; adopts the next committee); period 1914's 15,679,520 (17:48:53Z; rotates); the finality update 15,683,296 (17:53:22Z) | 15,672,512 (17:47:46Z); a finality update 15,683,264 (17:48:29Z; rotates); period 1914's update (17:49:36Z; adopts period 1915's committee); 15,683,296 (17:53:22Z) |
| each served or confirmed by | nimbus and lodestar, byte for byte | nimbus, lodestar and lighthouse |
| rounds the best update was not sent for want of clients | 14 ("nimbus only" or "lodestar only") | 1 ("lighthouse, lodestar only", then sent) |
| disagreements | 0 | 0 |
| `/health` (20 probes) | 19 ok, 1 degraded (nimbus unavailable once) | the same |
| end | up to date at 15,683,296 | the same |

So byte for byte from two clients followed mainnet live, at the cost of one finalized checkpoint: no update finalizing
15,683,264 reached the updater as the same bytes from both clients (its log names each such update as served by
"nimbus only" or "lodestar only"), so it was skipped, and the next checkpoint, 15,683,296, was applied at the same
second as by the `finalized-header` daemon, which had applied 15,683,264 at 17:48:29Z. Files per run: `summary.md`, `daemon.log`, `health.jsonl` (each 30 s),
`metrics-last.txt` (with `ferrata_updater_source_client_info`), `updater.toml` (paths shortened to `spike/`).

## 4. What did not run

- **Sepolia live**: `live.py` wants the checkpoint served as a bootstrap by every light-client source, and Lodestar's
  Sepolia API answered HTTP 500 for the finalized headers of periods 1415's and 1414's updates (Nimbus 200; checked
  again at 18:02Z; both serve period 1416's): no run. The updater itself needs the bootstrap from one source only (it
  is bound to the checkpoint by hashes); the strictness was the script's.
- Lighthouse as a light-client source: publicnode's node answers light-client requests in JSON only, and the updater
  reads SSZ (README, "Not built"); it counted as a `headers` source.
- A run of hours or on a LEZ sequencer: ten minutes, in process.

## 5. After the review (18 November, 18:45Z)

An independent review of the five commits above (its counterexamples are now `../../tests/diversity_limits.rs`) found
the code doing what the README says and the measured figures re-deriving, and asked for these changes, made on the
branch after merging `integration8`:

- **`finalized-header` is weaker than said** (its F1): the update sent is one source's bytes, and a `headers` answer is
  read for `canonical`, not `finalized`; an update that one light-client source alone serves, a forged next committee
  among them, is sent when its finalized header is canonical (`finalized_header_mode_sends_what_one_light_client_source_served`,
  `headers_answers_count_canonical_blocks_and_client_names_are_the_providers_text`). The README, trust.md §2.4 and
  EVIDENCE row 147 now say so and recommend `bytes`.
- **The EIP-7657 loss** (F2): about 536 ETH for the cheapest 342 seats and 12,095 ETH for all 512, with the source and
  target penalties a slashed validator pays until withdrawable (`../../../../docs/forgery-cost/README.md`).
- **One provider listed twice** (F4): under the policy the configuration refuses the same host twice
  (`one_provider_listed_twice_is_refused_by_the_configuration`); two hosts of one provider still count twice.
- **Waiting for clients is now an alarm** (F5): `/health` degraded and `ferrata_updater_client_wait_slots` when the
  sources serve updates more than `max_lag_slots` past the instance and none is sent
  (`waiting_for_clients_is_reported_in_health`).
- The Waiting text no longer reads "2 of 1 sources" (F7); running the suite rewrites the tracked `../gate.txt` with the
  grown record's figures: `git checkout` it afterwards (F10).

The suite after these changes (box, 18:55–19:14Z; the sources of the commit that records this): **31 passed, 0
failed** (6 in the library, 8 in `config.rs`, 4 in `diversity_limits.rs`, 13 in `recorded.rs`), clippy `-D warnings`
clean ([`suite/tests-after-review.txt`](suite/tests-after-review.txt), [`suite/run-after-review.txt`](suite/run-after-review.txt)).
The four: `finalized-header` opened 11,600,416 and the period-1416 update 11,599,872, which only the source ahead served,
while `bytes` held at 11,591,680; one backend listed twice under two names met `k = 2` in process (the updater cannot
tell) and the configuration refused it ("the same host"); a `headers` answer with `finalized: false` confirms, and
`Nimbus`, `Nimbus ` and `nimbus-eth2` are three client names; two sources of one client: 57 of 60 rounds waiting, `/health`
degraded ("waiting for clients: the sources serve updates 50880 slots past the instance, none sent ...").

## 6. Re-check

```sh
python3 -I lez/updater/results/client-diversity-2026-11-18/probe.py OUT --net mainnet=URL,URL --rounds 20
cd lez && RISC0_DEV_MODE=1 cargo test --locked -p ferrata-updater --features harness -- --nocapture
python3 -I lez/updater/results/client-diversity-2026-11-18/live/live.py OUT --bin lez/target/debug/ferrata-updater \
  --name live-bytes --preset mainnet --lc nimbus=URL --lc lodestar=URL --k 2 --key lez/updater/dev/payer.key
```
