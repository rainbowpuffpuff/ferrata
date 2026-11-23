# attest-seq-rf2: run rf1 again with the journal, the endpoint check and the window holds

`ferrata attest` against a LEZ v0.4.0-rc1 zone we started (the same run as [rf1](../attest-seq-rf1/README.md): a local
sequencer and Logos Blockchain node, LEZ's local configuration, the final attestation program, image `0x816d1fef…`; real
Sepolia updates and the mainnet attestation fixture over loopback), with the tool of branch `cli-rows` after the first
review's fixes: every send journaled in `--state-dir`, the Ethereum endpoint's chain and block checked before any proof,
the dry run made at the zone's clock and the window holds, one run per state directory. The hook's two deliberate
replays pass `--again` (the journal reports an earlier run's command and sends nothing otherwise).

```sh
mrun --put ../tmp/miniapp-sequencer/inputs -- 'bash cli/tests/attest-seq.sh rf2'      # Modal build box, 1,228 s
```

The `ferrata` binary was built from the tree of `1cb2c8ee` (its sha256 in [`binaries.sha256`](binaries.sha256)); the
commits since change the tests, the documents and one hold that no command of this run reaches (a step whose window
has not opened). Measured on 23 November 2026, before the mainnet fixture's checkpoint leaves its weak-subjectivity
period (2026-11-25 22:40:23 UTC; the hook's `mainnet-init` is held as stale after that on a wall-clock zone):

- **22 of 22 commands as expected** (exit status and the zone's refusal code; [`cli/summary.tsv`](cli/summary.tsv)):
  refusals 23 `PeriodSkipped`, 130 `PredicateFailed` (twice), 124 `ReceiptFailed`, 52 `NotNewer`, each the code the
  hook expected; 24 sent transactions, each equal to its dry run (no `DIFFERS` or `DISAGREE` line).
- The 52 outcome lines of the commands' outputs (verdict, execution gas, storage gas, number of events) are, as a
  multiset, identical to run rf1's.
- The two replays (`sepolia-advance-replayed`, `mainnet-advance-replayed`): "an earlier run sent this command (journal
  …, exit status 0); --again: sending it once more", then refused on the zone with 23 and 52 as before.
- `attest-monitor` on the zone's mainnet instance: divergence `confirmed` both times; no alert at the record's time;
  `ws-half` at the run's time, as in rf1.
- No machine path in the records (`grep -rln /home/` over the run's files, this README aside: nothing).
