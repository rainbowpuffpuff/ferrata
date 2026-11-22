# The sequencer CI job's first run, on the build box (21 November 2026)

`scripts/ci/sequencer-attest.sh e2e/runs/ci-sequencer-attest-2026-11-21`, the exact script of the manual CI job
`sequencer-attest` ([`.github/workflows/sequencer.yml`](../../../.github/workflows/sequencer.yml)), run once on the
Modal build box (Linux x86-64) from this branch. RFP-021 Supportability #2.

## Result (measured): stopped in the third phase

- **Build, 16 min:** LEZ v0.4.0-rc1's `sequencer_service` with its `standalone` feature (LEZ's mock Bedrock actor, no
  Logos Blockchain node), the live driver, the guests.
- **`info`, `deploy`: applied.** A standalone sequencer started on 127.0.0.1; the attestation program (main and twin)
  and LEZ's `scripted` program deployed through the sequencer's own loader, immutable; the program's image id computed
  by the loader is `816d1fef…`, the final program of [`docs/image-ids.md`](../../../docs/image-ids.md).
  ([`records/summary.txt`](records/summary.txt): 21 transactions sent, 20 applied, blocks 1 to 22.)
- **`scenarios`: stopped.** `twin Init` reverted: "Execution outside of the validity window"
  ([`sequencer-attest.log`](sequencer-attest.log)). The phase replays the recorded mainnet light-client data of
  periods 1899–1903 (finalized slot 15,556,608, about 31 October 2026), and a standalone sequencer stamps its blocks
  with the wall clock: three weeks later the `Init` falls outside the validity window its transaction carries (the
  likely cause; not traced further). [`lez/live/README.md`](../../../lez/live/README.md) reports a clean rehearsal of
  these phases on the same mock before its recorded runs, when the data was younger.
- Box cost (estimate by `mrun`): 1,135 s, 1.90 core-hours, about 0.15 USD.

## What this means for the CI job

A job that replays recorded Ethereum data against a sequencer running on the wall clock ages out. The job is
therefore manual-only until its phases run on fresh data: a beacon chain made by the run with a committee whose keys
it holds, as the live driver's `inbound --chain stand-in` phase and the crown's stand-in already make
([`lez/live/driver/src/inbound.rs`](../../../lez/live/driver/src/inbound.rs)), so that every `Init` and update is
inside its window whenever CI runs. Not done here.

Machine paths in the logs are replaced by `<spike>`; the cargo build lines are cut from the log. (The script then
scrubbed paths only on success; since then it scrubs its whole results directory on every exit,
`scripts/ci/sequencer-records.sh`.)

`records/programs.tsv` lists a `consumer` row although the consumer was never deployed in this run (the phase that
deploys it, `attest-private`, did not start): the driver writes that row whenever it is given `--consumer-bin`. The
script's check that the consumer was deployed now reads `run.tsv` instead (its `CreateHeader` and every segment
applied), and says no for this run (`scripts/ci/test-sequencer-records.sh`).
