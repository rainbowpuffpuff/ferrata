# The both-halves tests under ADR 0046 (27 November 2026)

`e2e/both-halves/run.sh` on the Modal build box, this tree's vault and bridge program (ADR 0046: the mint fee taken on
Ethereum at the deposit, a claim naming no fee schedule): all three tests passed (`fee_schedules_over_both_halves`,
`randomised_runs_keep_supply_within_holdings` over seeds 1, 2, 3, `migration_rehearsal_honours_positions_in_flight`).
Only the fee scenario's record is kept, [`fees.md`](fees.md): the other two do not involve fees, and their records in
[`..`](..) (`randomised.md`, `migration.md`) are of the earlier program, which their pass here did not rewrite (the runs
wrote into a scratch directory). The earlier fee record, [`../fees.md`](../fees.md) (sets of 3 and 3 across the fee
change), is the measurement ADR 0046 cites. Since this record the tests write here by default.
