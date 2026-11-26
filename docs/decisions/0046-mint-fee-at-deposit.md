# ADR 0046: Take the mint fee on Ethereum at deposit; a claim on LEZ mints the denomination and the fixed relay fee only

- **Date:** 26–27 November 2026
- **Status:** accepted; the vault built; the bridge program's claim not yet changed
- **Requirements:** RFP-022 F.13

## Context

RFP-022 F.13 asks that "the fee value must not distinguish a user's transaction from others". Under D13 a deposit
escrowed its denomination plus the mint fee and the relay fee of the fee schedule in force, and the claim on LEZ named
the leaf range of its deposit's schedule (`ScheduleRange`) and minted the mint fee of that schedule to the fee
recipient. Within one schedule every claim of a denomination paid the same; across a fee change, a claim's public range
and mint fee told which schedule, so which span of deposit time, its deposit belonged to: the anonymity set split at
the leaf where the change took effect (`e2e/both-halves/results/fees.md`: six deposits of 1 ETH, three and three after a
change at leaf 3). The relay fee is fixed at registration and release fees are taken on the vault at release, so only
the mint fee carried the schedule into a claim.

## Options considered

1. Keep it, disclosed: fee changes are rare and announced seven days ahead, and the client counts the anonymity set
   per schedule. *Recorded:* docs/requirements-status.md, RFP-022's F.13 row before this record ("a design question
   for the owner"). The clause would hold within a schedule only.
2. Mint the fees on LEZ in bulk: a permissionless instruction mints to the fee recipient the mint fees of every
   deposit up to a leaf count the program has learnt; claims carry no fee. *Reconstructed for contrast* (weighed with
   the others on 26 November 2026, in notes not kept in the repository). It keeps the fee on LEZ but adds an instruction
   and keeps the schedule replication on LEZ.
3. Take the mint fee on Ethereum at deposit.

## Decision

The third. A deposit still pays the denomination plus the mint and relay fees of the schedule in force (the depositor's
price and the deposit caps do not change). At the deposit the vault credits the mint fee to the fee recipient through
its pull-payment ledger (`owed`, withdrawn with `claimOwed`; never pushed, so a fee recipient cannot block a deposit),
and its escrow for the deposit is the denomination plus the relay fee: exactly what LEZ mints for it. On LEZ a claim
names no schedule: `Claim`, `ClaimPrivately` and `Issue` lost their `range`, `verify_claim` no longer takes one, and
`Issue` mints the denomination to the destination and the token's registered relay fee to the relayer (the fee
recipient when the claim names none). A refund after a sunset returns the escrow (denomination and relay fee); the mint
fee was paid at the deposit and is not refunded. Schedules, their announcement delay and ceiling, and their publication
for the LEZ side are unchanged; the LEZ program still mirrors them (`Register`, `ImportFees`, `ExtendFees`), but no
claim reads them.

## Rationale and trade-offs

Every public value of a claim is now a function of the denomination alone: two claims of one denomination mint the same
amounts to the same kinds of accounts whatever the deposit's schedule, so a fee change no longer splits the anonymity
set on LEZ. The deposit was already public on Ethereum, with its amount, so taking the fee there discloses nothing new.
The vault already escrowed the fee at the deposit; crediting it then needs no new trust: the credit goes through the
ledger every release fee already uses. Costs: the vault, the bridge program and every client that builds a claim
change, so the bridge program's image id moves, a new vault and a new LEZ instance are needed, and the both-halves and
end-to-end runs are made again; the fee recipient is paid in the deposited token on Ethereum instead of in wrapped units
on LEZ; a refunded deposit no longer gets its mint fee back (the fee is earned at the deposit, which the vault cannot
undo once the recipient has withdrawn it). Keeping the schedule mirror on LEZ keeps every other message's encoding
unchanged; removing it is a later clean-up.

## Pairing and what does not change

A vault of this rule must be paired only with a bridge program whose claim mints no mint fee: a claim of the earlier
program would mint the mint fee with no escrow behind it. Vaults and programs are immutable, so this is a new vault
(and its linked library) and a new LEZ configuration, never an upgrade in place. No public function, event definition,
leaf or commitment encoding, journal, storage slot, registry entry or published fee-schedule encoding of the vault
changes; a deposit under a nonzero mint fee additionally emits the existing `OwedCredited(feeRecipient, token,
mintFee)`. The fee recipient withdraws deposit mint fees and release burn fees with `claimOwed`; a client must not
expect a refund to return the mint fee.

## Sources

- The F.13 row of docs/requirements-status.md before this record (the split, measured in
  `e2e/both-halves/results/fees.md`); the choice of the third option, 26 November 2026
- ADR 0021 (the fees of D13); D13 in `docs/design/spec/bridge-spec-v3-changes.md`
- `vault/src/FerrataVault.sol` (`_checkDeposit`, `_recordDeposit`, `refund`), `vault/src/FerrataRegistry.sol`
  (`refundAmount`)
- `vault/test/Fees.t.sol` (the deposits on both sides of a fee change, the refunds, the fee recipient's withdrawal)
  and `vault/test/invariant/VaultInvariants.t.sol` (`invariant_depositMintFeeCreditsMatchTheInForceSchedules`)
