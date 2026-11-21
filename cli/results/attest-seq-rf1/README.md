# attest-seq-rf1: run r4 again, after the review's fixes to the run's hook

`ferrata attest` against a LEZ v0.4.0-rc1 zone we started (the same run as [r4](../attest-seq-r4/README.md): the final
attestation program, image `0x816d1fef…`; real Sepolia updates and the mainnet attestation fixture over loopback),
after review rfp-B's finding L10 and L2 were fixed in the run's scripts (`d3aff4e2`):

- the hook expects each refusal's code, not only exit 3 (`3:23`, `3:130`, `3:124`, `3:52`), reads the code the zone
  refused the sent transaction with, and writes `as expected` or `NOT as expected` per command
  ([`cli/summary.tsv`](cli/summary.tsv)); the two monitor reads are listed apart, with no expectation;
- the records are written with no machine paths by the scripts themselves (commands with `$REPO`; build logs kept in
  the work directory; an exit trap rewrites any absolute repository path), with no edit by hand.

```sh
mrun --put ../tmp/miniapp-sequencer/inputs -- 'bash cli/tests/attest-seq.sh rf1'      # Modal build box, 893 s
```

Tree `d3aff4e2` (the commits made while it ran changed only documents and `logos-build.sh`, which the run does not
use). Measured on 21 November 2026:

- **22 of 22 commands as expected** (exit status and the zone's refusal code): refusals 23 `PeriodSkipped`, 130
  `PredicateFailed` (twice), 124 `ReceiptFailed`, 52 `NotNewer`, each the code the hook expected; every sent
  transaction equal to its dry run (no `DIFFERS` or `DISAGREE` line).
- The 52 outcome lines of the commands' outputs (verdict, execution gas, storage gas, number of events) are, as a
  multiset, identical to run r4's.
- `attest-monitor` on the zone's mainnet instance through the sequencer's `getAccount`: divergence `confirmed` both
  times; no alert at the record's time; `ws-half` at the run's time (the record is 11 days old), as in r4.
- `grep -rln /home/` over this directory: nothing.
