# The operator rebate: paying whoever advances an instance

RFP-021 soft requirement 5, verbatim: "**Operator incentive.** Propose a mechanism that rewards whoever advances the
consensus state, so that liveness does not rest solely on interested parties choosing to spend the gas. Any such
mechanism must preserve the permissionless property in Functionality #11: it may make operating attractive, but must
not make it exclusive."

**Status: built and tested in the in-process harness (LEZ v0.4.0-rc1's state machine, dev mode, five real mainnet
updates); not deployed, not run on a sequencer.** Code: [`lez/rebate-wire/`](../../lez/rebate-wire/src/lib.rs) (the
design, in its module comment), [`lez/guests/rebate/`](../../lez/guests/rebate/src/main.rs) (`ferrata_rebate`), the
test [`lez/harness/tests/rebate.rs`](../../lez/harness/tests/rebate.rs), its records
[`lez/results/rebate.md`](../../lez/results/rebate.md) and [`lez/results/rebate-terms.md`](../../lez/results/rebate-terms.md). The attestation program and the bridge program are unchanged;
no deployed image id moves.

## The mechanism

- **A pool per set of terms.** `pool(terms)` is an actor of the rebate program; its account holds a native balance.
  Its address commits to its terms: the attestation program and instance it pays for, the rebate per paid advance,
  and the least advance of the finalized slot it pays for (`min_gap`). A funder computes the address of the terms it
  wants and funds it with an ordinary native transfer. Anyone may create the pool (`Configure { terms }`), but at that
  address only those terms can be written, so nobody can set another pool's terms; a second `Configure` is refused.
- **Paid for landing an advance.** An operator runs a light-client update as today (`Open`, `Miller`, adoption parts)
  and sends the last step through the pool: `Land { job_id, payee }`. The pool reads its balance and the instance,
  sends the attestation program's own `Finish` to the job (which verifies and applies the update), and reads the
  instance again. LEZ runs these calls in order, each completing before the next starts. If the instance's finalized
  slot advanced in this transaction, to at least `min_gap` slots past the last slot the pool paid for, the pool pays
  `payee` the rebate (or what it holds, if less) out of its own account. A refused `Finish` ends the whole
  transaction: nothing is paid for an update that does not verify.
- **A bounded payout.** One advance pays one rebate, never the pool. Progress split into many small updates is paid at
  most once per `min_gap` slots: while the instance's finalized slot advances by `progress`, the pool pays at most
  `rebate × (1 + progress / min_gap)`. With `min_gap` one sync-committee period (8,192 slots, about 27 hours on
  mainnet), that is about one rebate a day, however an operator splits its updates; with `min_gap` 0, one rebate per
  landed advance, up to one per finalized epoch (225 a day on mainnet, derived), which a funder should only choose
  with that drain rate in mind.
- **Permissionless.** Any account may call `Land` and name any payee. Any account may still send `Finish` directly,
  exactly as before, and is paid nothing. An empty pool pays nothing and blocks nothing. The program grants no role and
  keeps no list of operators: it makes operating attractive, not exclusive.
- **What an update costs, to size a rebate.** In the test, update 1's five transactions (`Open`, `Miller`, two
  `Adopt`s and the `Land`) paid 130,034,496 native units in base fees (17,733,944 + 26,510,168 + 27,487,800 +
  27,545,352 + 30,757,232: [`rebate.md`](../../lez/results/rebate.md), rows 8 to 12; the harness's fee state, not a
  live network's), so the test's rebate of 1,000,000 is 0.77 % of it (derived). A rebate below an update's cost only
  offsets part of the gas; to make operating attractive by itself it has to cover the cost and a margin.

## What it does not do, and its limits

- Progress made directly, or before the pool existed, is not paid for itself (the pool reads the instance before and
  after `Finish`; at `Configure` the instance's finalized slot is taken as already paid for). Progress the pool did not
  pay for after `Configure`, made directly or landed below the gap, still counts toward `min_gap`: once the gap is
  reached, the next landing through the pool is paid the full rebate, whatever its own advance (in
  [`rebate-terms.md`](../../lez/results/rebate-terms.md), update 2 advances one period and is paid against a gap of
  two, because update 1's unpaid period counted). The bound above is unaffected.
- A paid advance pays the same whatever its size above `min_gap`.
- **No withdrawal.** What a pool holds leaves only as rebates. A pool whose instance is succeeded (a successor instance
  has other terms, so another pool), or whose instance never advances again, keeps its balance for good: fund a pool
  in amounts you are ready to see stranded.
- **The last step's sender is paid.** If one operator opens and steps a job and another finishes it through the pool
  first, the second is paid. Neither is refunded its gas either way. An operator narrows this by submitting its own
  `Land` immediately after its job's last step (the last `Adopt`, or `Miller` when there is none), without waiting for
  that step's inclusion. A sequencer that reorders transactions can still give the rebate to a copier.
- **The funding needs a conversion.** The rebate is paid in LEZ's native token. Since ADR 0048 the configuration's fee
  recipient receives deposit mint fees and release burn fees in the wrapped token on LEZ, through `CollectFees`; the
  immutable Ethereum `feeRecipient` receives neither. The wrapped token is not LEZ's native token, so a policy that
  funds the pool from protocol fees needs to exchange it for the native token, then transfer it. That funding path is
  not built. Whether the fee recipient funds the pool at all, and how much, is a policy of whoever holds that role; the
  vault and the bridge program do not fund the pool.

## What the test shows

`RISC0_DEV_MODE=1 cargo test -p harness --features rebate --locked --test rebate -- --nocapture` (from `lez/`):
4 tests pass (box, 21 November 2026). On the mainnet instance of periods 1899–1903 (each update advances the
finalized slot by one period, 8,192 slots), with every job's `Open` and `Miller` sent as usual:

- **A pool of 3,000,000 units paying 1,000,000 per period of progress** ([`rebate.md`](../../lez/results/rebate.md)):
  update 0 through the pool adopts the next committee and finalizes nothing new: it lands, nobody is paid; update 1
  through the pool advances the finalized slot from 15,556,608 to 15,564,800: operator 1 is paid 1,000,000, once;
  update 2 sent directly (`Finish` to the job, as today): it lands, nobody is paid; update 3 through the pool by
  operator 2: paid 1,000,000 for its own advance, not for update 2's.
- **The squat and the drain** ([`rebate-terms.md`](../../lez/results/rebate-terms.md)): a `Configure` carrying rebate
  2^128 − 1 and no gap, sent to the address of a funder's terms (1,000,000 per two periods), is refused (75); those
  terms make a pool of their own at another address, which nobody funds; the funder's pool, funded with 3,000,000,
  pays one advance 1,000,000 and keeps 2,000,000. Progress split into one-period updates (updates 0 to 3, 24,576
  slots) is paid once: update 1 (one period) earns nothing, update 2 (two periods since the pool's start) earns the
  rebate, update 3 (one period after it) earns nothing.
- **Refusals and edges**: an unfunded pool: updates land through it, nothing is paid; with half a rebate in it, the
  next paid advance is paid what it holds; `Configure` twice (78); `Configure` at another instance's pool actor,
  with that actor's own evidence (75); `Land` at a pool never configured (79); `Land` of a job that does not exist,
  with the job actor's evidence: the attestation program refuses it (205, no job) and nothing is paid. The payment
  rule alone at its boundaries: one slot short of the gap pays nothing, exactly the gap pays.

Cost (measured, execution gas): `Land` 3,843,871 for each paid landing (updates 1 and 3), against 3,736,661 for
update 2's `Finish` sent directly: about 107,000 more (2.9 %). The comparison is across updates, not of one update
both ways: the attestation program's `Finish` handler itself took 3,654,943 cycles for updates 1 and 3 and 3,653,838
for update 2, `Apply` 65,473 for each, so nearly all of the difference is the pool's reads and the payment.
