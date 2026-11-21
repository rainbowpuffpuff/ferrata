# The operator rebate: paying whoever advances an instance

RFP-021 soft requirement 5, verbatim: "**Operator incentive.** Propose a mechanism that rewards whoever advances the
consensus state, so that liveness does not rest solely on interested parties choosing to spend the gas. Any such
mechanism must preserve the permissionless property in Functionality #11: it may make operating attractive, but must
not make it exclusive."

**Status: built and tested in the in-process harness (LEZ v0.4.0-rc1's state machine, dev mode, five real mainnet
updates); not deployed, not run on a sequencer.** Code: [`lez/rebate-wire/`](../../lez/rebate-wire/src/lib.rs) (the
design, in its module comment), [`lez/guests/rebate/`](../../lez/guests/rebate/src/main.rs) (`ferrata_rebate`), the
test [`lez/harness/tests/rebate.rs`](../../lez/harness/tests/rebate.rs), its record
[`lez/results/rebate.md`](../../lez/results/rebate.md). The attestation program and the bridge program are unchanged;
no deployed image id moves.

## The mechanism

- **A pool per instance.** `pool(ferrata, instance)` is an actor of the rebate program; its account holds a native
  balance. Anyone creates it once (`Configure { ferrata, instance_id, rebate }`), and anyone funds it with an ordinary
  native transfer to its account. The proposal's source of funds is the bridge's protocol fee: its fee recipient
  transfers part of what it receives to the pool. A consumer that wants an instance kept live can fund it too.
- **Paid for landing an advance.** An operator runs a light-client update as today (`Open`, `Miller`, adoption parts)
  and sends the last step through the pool: `Land { job_id, payee }`. The pool reads its balance and the instance,
  sends the attestation program's own `Finish` to the job (which verifies and applies the update), and reads the
  instance again. LEZ runs these calls in order, each completing before the next starts. If the instance's finalized
  slot advanced in this transaction, the pool pays `payee` the fixed rebate (or what it holds, if less) out of its own
  account. A refused `Finish` ends the whole transaction: nothing is paid for an update that does not verify.
- **Permissionless.** Any account may call `Land` and name any payee. Any account may still send `Finish` directly,
  exactly as before, and is paid nothing. An empty pool pays nothing and blocks nothing. The program grants no role and
  keeps no list of operators: it makes operating attractive, not exclusive.

## What it does not do, and its limit

- Progress made directly, or before the pool existed, earns nobody a rebate (the pool reads the instance before and
  after `Finish`; at `Configure` the instance's finalized slot is taken as already paid for).
- One rebate per landed advance, whatever its size: an update that finalizes a later slot pays the same as one that
  finalizes the next.
- **The limit:** the rebate goes to whoever lands the job's last step. If one operator opens and steps a job and
  another finishes it through the pool first, the second is paid. Neither is refunded its gas either way. An operator
  avoids this by sending its own `Land` as soon as its `Miller` step lands.
- The rebate is paid in LEZ's native token. Whether the bridge's fee recipient funds the pool, and how much, is a
  policy of whoever holds that role; the vault and the bridge program are not involved.

## What the test shows

`RISC0_DEV_MODE=1 cargo test -p harness --features rebate --locked --test rebate -- --nocapture` (from `lez/`):
2 tests pass (box, 21 November 2026). On the mainnet instance of periods 1899–1903, with every job's `Open` and
`Miller` sent as usual and a pool of 3,000,000 units (rebate 1,000,000) funded by an ordinary native transfer:

- update 0 through the pool adopts the next committee and finalizes nothing new: it lands, nobody is paid;
- update 1 through the pool advances the finalized slot from 15,556,608 to 15,564,800: operator 1 is paid 1,000,000,
  once;
- update 2 sent directly (`Finish` to the job, as today): it lands, nobody is paid;
- update 3 through the pool by operator 2: paid 1,000,000 for its own advance, not for update 2's;
- an unfunded pool: updates land through it, nothing is paid; with half a rebate in it, the next advance is paid what
  it holds; `Configure` twice is refused (75), `Land` at a pool never configured is refused (76), `Land` of a job that
  does not exist is refused and pays nothing.

Cost (measured, execution gas): `Land` 3,842,337 for a paid landing, against 3,736,661 for the same step sent
directly: about 106,000 more (2.8 %), the pool's reads and the payment.
