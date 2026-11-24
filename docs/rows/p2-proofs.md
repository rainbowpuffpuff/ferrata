# Stream p2-proofs: RFP-021 P.2, every action and statement type proven on this tree's images

For the integrator of `docs/requirements-status.md`, `docs/traceability.md`, `docs/EVIDENCE.md` and the README's
status lines. Branch `p2-proofs` from `8ac83197`: `2eddbc61` (the dump holds every message and statement type; the
summary checks every record against its image id), `04b6767f` (the records, `bench/perf/results/p2-c/`), `e4dfa4e0`
(`lez/results/perf-proofs.md`), `531462d9` (`docs/performance.md` RFP-021 §P.2), `badae931` (this file); after review
round 1 (`spike`-side review-p2): `dc624cfe` (the summary refuses a misfiled log and a proof on a sibling image, with
tests in CI), `58d2a711` (`docs/design/attestation.md`), `53385806` (host spread and receipt measure in the documents),
`ee007509` (the real-data leaf's charges pinned in `tests/vault.rs`), then this revision. No program, guest or
image id changed: the dumped guest binaries' sha256 equal `docs/image-ids.md`'s. Runs of 24 November 2026: the stage
on the Modal build box (481 s), the eleven proofs on Modal containers held to 16 CPUs and 32 GiB, r0vm 3.0.5 (CPU,
the binary of the earlier rows, sha256 `36c016a5…`), 22:19–22:41Z. Nothing was pushed or sent to any network.

## What the row lacked, and what is shown now

The status table's reason for P.2 (in part): "the leaf's only real proof is of an earlier image, and the public
`VerifyAll`, `ApplyVerified` and `Read` are not proven". Proven now, on the attestation program `0x816d1fef…` and the
reference consumer `0x8464603b…` (measured; composite s / compress s / succinct receipt B, the InnerReceipt, the
measure of the per-action tables):

| execution (transition of `perf_prove --dump`) | user cycles = execution gas | segments | proof | peak GiB | host |
|---|---:|---:|---|---:|---|
| accumulator leaf: FerrataVault v5's slots 0 and 1 and deposit leaf 0, real Sepolia state (49) | 327,016 | 1 | 108.5 / 20.8 / 230,936 | 4.62 | AMD Zen 3 |
| accumulator leaf: the same with both deposit leaves (50; row 71's request, proven before on image `0x8fe18f74…`) | 392,358 | 1 | 109.1 / 20.8 / 232,040 | 4.62 | AMD Zen 3 |
| `VerifyAll`, the finality update in one public transaction (40) | 9,049,362 | 10 | 1,079.3 / 189.7 / 227,408 | 9.20 | AMD Zen 3 |
| `ApplyVerified`, sent by it to the instance (41) | 58,184 | 1 | 13.7 / 9.2 / 225,429 | 1.44 | AMD Zen 3 |
| `Record`, sent by the instance to the header (42) | 17,350 | 1 | 21.4 / 16.1 / 224,041 | 1.44 | AMD Zen 2 |
| `Read`, in a public read of a record: `ReadRecord` → `Read` → reply (44) | 12,268 | 1 | 7.8 / 10.5 / 224,105 | 1.44 | AMD Zen 3 |
| `ReadPadded`, the same read with 1,024 bytes of padding (47) | 18,377 | 1 | 7.8 / 10.3 / 225,133 | 1.44 | AMD Zen 3 |
| the consumer's `ReadRecord`, `ReadPadded` and replies (43, 45, 46, 48) | 12,061; 15,081; 27,561; 17,381 | 1 each | 6.6–12.3 / 8.4–9.8 / 223,462–226,106 | 1.44 | AMD Zen 3, Zen 4 |

Per transaction (execution / storage gas): the public update 9,124,896 / 4,105 (proofs 1,114.4 s composite, 214.9 s
compress, three receipts 227,408 + 225,429 + 224,041 B inner); the read 39,410 / 604; the padded read 63,319 / 1,628;
the leaf 327,016 / 7,659 and with both leaves 392,358 / 8,731. These equal `lez/results/stage1-one-transaction.md`,
`stage5-vault-v5.md` rows 1 and 3, and `perf-state-actions.md`, and the charges CI pins (`tests/costs_attestation.rs`:
`VERIFY_ALL_TX`, `READS`; `tests/vault.rs`: `REAL_LEAVES`).

With them, every message of the attestation program (`Init`, `JobOpen`, `Adopt`, `Apply`, `Record`, `Open`, `Miller`,
`Finish`, `VerifyAll`, `ApplyVerified`, `Attest`, `Read`, `ReadPadded`, `Succeed`, `Spawn`) and every type of statement
(account, absent account, storage slot, absent slot, event log, a slot behind an EIP-2935 hop, several facts with a
relation, accumulator leaf) has its cycles, gas, proof time and proof size: 50 guest executions (28 actions), every one
proven by r0vm on a 16-CPU, 32-GiB container, plus the finality update and a committee handoff each proven in one
private transaction and settled (unchanged, `p2-a`, `p2-b`).

**Each record checked against the image id.** `python3 -I bench/perf/summarize_proofs.py bench/perf/results/p2-c
bench/perf/results/p2-b bench/perf/results/p2-a` (which writes `lez/results/perf-proofs.md`) refuses, printing nothing
and exiting 1, unless: each proof log's RESULT line carries its own transition's number and user cycles, and its
`image <id> (<handler>)` line carries index.tsv's id and handler (perf_prove computes the id from the binary it proves and
verifies both receipts against it); the id is the row of the first table of `docs/image-ids.md` (this tree's guests)
of the guest the handler runs in, `ferrata_consumer` for the consumer's handlers and `ferrata_lez` for every other and
for each private run's program; a stage's SHA256SUMS, where kept (`p2-a`, `p2-c`), lists the guest with that row's
sha256; and a later directory's index rows equal the first's for the transitions it proved (the new dump's rows 1–39
are byte-identical to `p2-b/index.tsv`). `bench/perf/test_summarize_proofs.py` (in CI beside `scripts/test_perf_records.py`)
holds the results file to the summary byte for byte and refuses a misfiled log, a proof on a sibling image, a guest
missing from a stage's sums and another program's private run; its four refusal tests fail against the summary of
`badae931`. The summary's last section, as run on this branch: `0x816d1fef…03df` `ferrata_lez`, sha256 equal, `p2-a`
1–27, 29, 32, 35 and private-1, `p2-b` 38–39 and private-handoff-1, `p2-c` 40–42, 44, 47, 49–50; `0x8464603b…4b96`
`ferrata_consumer`, sha256 equal, `p2-a` 28, 30–31, 33–34, 36–37, `p2-c` 43, 45–46, 48. In every row of the
per-action table the execution gas equals the user cycles proven (28 of 28).

## Row

| Row | Now | Proposed | Evidence |
|---|---|---|---|
| RFP-021 P.2 | in part (EVIDENCE 5, 32, 49, 123, 166): the leaf proven only on an earlier image; the public `VerifyAll`, `ApplyVerified` and `Read` not proven | **shown** | Every clause below; EVIDENCE 166 revised (text below); `lez/results/perf-proofs.md`; `docs/performance.md` RFP-021 §P.2. |

Clause by clause, as RFP-021 words it:

1. *"a breakdown by program action: advancing the tracked Ethereum state"*: `Init` and its two adoption transactions,
   the three-step finality update (`Open`, `Miller`, `Finish`, with `JobOpen`, `Apply`, `Record`), the
   five-transaction committee handoff (with `Adopt`), the finality update in one public transaction (`VerifyAll`,
   `ApplyVerified`, `Record`: new), the finality update and the committee handoff each in one private transaction
   (the proven path, settled), and instance succession (`Succeed`, `Spawn`): all proven, `perf-proofs.md` per-action
   and per-transition tables.
2. *"statement verification by type of statement"*: the eight types above, the accumulator leaf new on this image,
   each as the root of a transaction, and through the reference consumer (`CallAttest`, `Claim`): all proven; the
   read idiom a consumer's private transaction runs publicly (`Read`, `ReadPadded`): proven (new).
3. *"Report cycle count, gas cost, proof time, and proof size for each"*: every row: execution gas (= user cycles
   proven), storage gas, segments, composite and compression time, composite and succinct sizes, peak memory; the
   host per transition.
4. *"extending the measurement methodology from RFP-020"*: RISC Zero proof time and proof size of each guest
   execution, as for the 39 earlier rows (EVIDENCE 166).
5. *"Numbers must be reproducible from the test suite"*: `perf_prove --dump` runs the test suite's own scenario
   functions (`scenarios::one_transaction_update` of `tests/public_path.rs`, `accumulated::read_record` of
   `tests/costs_attestation.rs`, the requests of `vault::real` (`tests/vault.rs`), and the stage 1 and stage 4
   scenarios as before), whose tests write or pin the cycles and gas: `tests/costs_attestation.rs` pins the update
   steps, `Init`, succession, the public and private update, the statements, the consumer's claims and the reads, and of
   the event-log call its `Attest` (100,517; the call itself, 164,872, is written to `stage4-attest-consumer.md`), and `tests/vault.rs` (`ee007509`) the real-data leaf's; `bash bench/perf/prove-actions.sh OUT` proves all 50 on
   one machine (or `bench/perf/p2-stage.sh` and `bench/perf/prove_modal.py` on containers); the summary command above
   rewrites the results file and refuses a record of another image.

What stays as it was: LEZ v0.4 does not prove public transactions (a sequencer re-executes them), so the proofs of
public executions are the ones a prover of the same execution would make, RFP-020's method (already the basis of
EVIDENCE 166); proof times vary with the host, each host named: compression up to 2.5 times (the same `Record`:
6.4 s on Intel, 8.4 s on Zen 4, 16.1 s on Zen 2), proving more than three times (6.4 s on Zen 4 to 21.4 s on Zen 2).
Nothing here is audited.

## Proposed texts (review-p2's corrected versions)

**requirements-status.md, P.2's reason** (Evidence: 5, 32, 49, 123, 166; `lez/results/perf-proofs.md`):

> Cycles, gas, proof time and size for every action that advances the tracked state and every statement type, on
> this tree's images of the attestation program and its reference consumer: the 50 guest executions of the test
> suite's scenarios, every message of the attestation program and every type of statement (the accumulator leaf, the
> public `VerifyAll`, `ApplyVerified`, `Read` and `ReadPadded` among them), proven for real on 16-CPU containers, each
> record checked against the image id; a finality update and a committee handoff each also proven in one private
> transaction and settled.

Counts if accepted: RFP-021 hard requirements 49 shown, 6 in part, 1 not yet, 3 waits on Logos (from 48 / 7 / 1 / 3).

**EVIDENCE row 166, revised** (claim | figures | where | kind | command | file). Receipt sizes are the InnerReceipt's,
as in the per-action tables, except where a whole `Receipt` is named:

> RFP-021 P.2: proof time and size for every action that advances the tracked state and every statement type, on
> this tree's images: the 50 guest executions of the test suite's scenarios (37 of 21 November, `Succeed`/`Spawn`
> added after review, and on 24 November the finality update in one public transaction (`VerifyAll`,
> `ApplyVerified`, `Record`), `Read`, `ReadPadded` and the accumulator-leaf statement on real Sepolia data), each with
> the input LEZ v0.4.0-rc1 gives it, proven for real (composite, then succinct, both verified, journals equal to the
> executor's), every record checked against `docs/image-ids.md` (`0x816d1fef…`, `0x8464603b…`); and the whole
> finality update and a committee handoff each proven in one private transaction and settled
> | per action and statement type: execution gas = user cycles proven; e.g. a finality update's `Open` / `Miller` /
> `Finish` 307.9 / 410.1 / 514.0 s composite on 16 CPUs; the same update's public `VerifyAll` 9,049,362 user cycles
> in 10 segments, 1,079.3 s composite and 189.7 s compression, succinct receipt 227,408 B (InnerReceipt; 231,713 B as
> a whole `Receipt`), 9.20 GiB; `Succeed` and `Spawn` 14.6 s each; `Read` 12,268 cycles, 7.8 s, 224,105 B; succinct
> receipts (InnerReceipt) 223,462–282,105 B; a storage-slot statement 81.0 s, 230,663 B; the accumulator leaf (slots
> 0 and 1 and one leaf) 327,016 cycles, 108.5 s, 230,936 B; the 306-log receipt's log 277.1 s, 282,105 B; the proven
> update path 9,049,362 user cycles, 1,673.6 s, 9.21 GiB, receipt 227,120 B, 485,298 + 224,063 gas; the proven
> committee handoff 15,796,512 user cycles, 2,254.2 s, 9.20 GiB, receipt 227,347 B, 451,895 + 224,063 gas
> | cloud containers held to 16 CPUs and 32 GiB (Modal), r0vm 3.0.5
> | real
> | `bash bench/perf/prove-actions.sh OUT` (one machine) or `bench/perf/p2-stage.sh` then `bench/perf/prove_modal.py`;
> `python3 -I bench/perf/summarize_proofs.py bench/perf/results/p2-c bench/perf/results/p2-b bench/perf/results/p2-a`;
> `python3 -I bench/perf/test_summarize_proofs.py`
> | `lez/results/perf-proofs.md`

**traceability.md, P.2's evidence cell (`docs/traceability.md` line 103).** The cell's [lez] part gives an earlier
image's figures as this tree's program's (9,224,206 / 482,524 / 16,087,430, `lez/RESULTS.md` lines 93–94) and the
one real proof of an earlier image (3,240.5 s, laptop CPU). Replace the [lez] part with:

> [lez] this tree's program (`0x816d1fef…`): a finality update 9,228,905 execution gas in three transactions or
> 9,124,896 in one public transaction, or 485,298 execution plus 224,063 storage gas proven in one private
> transaction; a handoff 16,092,355 in five, or 451,895 + 224,063 proven. Per action and per type of statement, the
> cycles, gas, proof time and proof size of all 50 guest executions of the test suite's scenarios, proven on this
> tree's images on 16-CPU containers, each record checked against the image id (row 166; `lez/results/perf-proofs.md`).

then keep the per-attestation sentences on rows 32 and 123 unchanged, and drop "the one real proof 3,240.5 s …" and
the row-71 sentence ("By type of statement, proof time and size exist for the log attestation only; …"). Add
`results/perf-proofs.md`, `results/stage1-one-transaction.md`, `results/stage5-vault-v5.md` and
`harness/src/bin/perf_prove.rs` to the cell's file column. (9,228,905 and 16,092,355 are the charges
`lez/harness/tests/costs_attestation.rs` pins: `FIN_TOTAL_GAS`, `UPDATE_TX_GAS[1]`.)

## Changed in another stream's file

- `docs/design/attestation.md` (`58d2a711`): line 133's list of the current programs' charges, which CI pins, now
  gives the finality update in one public transaction, 9,124,896 execution gas (4,105 storage). The table above it,
  of the programs measured on mainnet periods 1899–1903, is unchanged.
- `.github/workflows/ci.yml` (`dc624cfe`): one step, `python3 -I bench/perf/test_summarize_proofs.py`, beside the
  RFP-022 records' test.
