# ADR 0048: Charge the protocol fees on LEZ in the wrapped token, minted in totals by a permissionless `CollectFees`; claims and burns carry no fee value

- **Date:** 27 November 2026
- **Status:** accepted; built (the vault, the bridge program and its clients); not deployed; supersedes ADR 0046 where they differ (where the protocol fees are paid out)
- **Requirements:** RFP-022 F.13, F.8, F.11, F.14, Sec.3

## Context

RFP-022 Functionality #13, quoted: "A protocol fee can be charged on both minting and burning of the wrapped token on
LEZ, in a denomination (wrapped token, native LEZ token, or a configurable mix, per Design Rationale, "Fee structure")
and at a rate configurable by the admin authority per deployment. The fee value must not distinguish a user's
transaction from others, consistent with Functionality #8. The admin can also set either fee to zero." Its "Fee
structure" adds: "This RFP requires that the program support a protocol fee on both minting (deposit-side) and burning
(redemption-side) of the wrapped token on LEZ", leaves "an Ethereum-side protocol fee ... to the proposal", asks the
proposal to state the denomination, the rate and its mechanism, the collection point ("Whether the mint-side fee is
deducted at claim time, and the burn-side fee at burn or at release, and how partial amounts (after fee deduction)
interact with the fixed-denomination requirement") and the routing, and says: "any fee paid by a user (protocol or
relayer) must take a value that does not distinguish their transaction from others, since a distinctive fee is itself a
fingerprint".

Before this record (ADR 0046) a deposit paid denomination + mint fee + relay fee, the vault credited the mint fee to an
Ethereum fee recipient at the deposit, and a release credited the burn fee there too. Claims named no fee schedule, so
a fee change no longer split the anonymity set; but both protocol fees were paid out on Ethereum in the deposited
asset, which is the addition the RFP leaves to a proposal, not the fee on LEZ it requires. Before ADR 0046 (D13, the
program the test network runs, `ferrata_bridge_b1`) the claim minted its deposit's mint fee on LEZ and named the
schedule's leaf range, so across a fee change the public value of a claim told which span of deposit time it came
from (`e2e/both-halves/results/fees.md`: three and three after a change at leaf 3).

## Options considered

1. Keep ADR 0046: both protocol fees paid out on Ethereum. *Recorded:* `docs/decisions/0046-mint-fee-at-deposit.md`
   (its decision), which left F.13's fee on LEZ unmet.
2. Mint each claim's mint fee on LEZ by its deposit's schedule (D13, B1's program). *Recorded:* the split in
   `e2e/both-halves/results/fees.md`; ruled out by F.13's clause that the fee value must not distinguish a user's
   transaction.
3. Take the burn fee on LEZ at the burn. *Reconstructed for contrast*. On top of the denomination, a holder of exactly
   one denomination (every claimant) could not burn it without first getting fee units from someone; out of the
   denomination, the note would carry the denomination less the fee in force, and the release's amount on Ethereum
   would tell which span of burn time its burn came from, the split of option 2 on the outbound side.
4. Charge the fees in native LEZ tokens. *Reconstructed for contrast*. The user (or the relayer) would need a native
   LEZ balance at the claim or the burn, the funding source the RFP's "Fee payers correlate" warns re-identifies a user,
   and the vault, which holds the deposited asset, could not back a fee in another asset without a price.
5. Mint the fees on LEZ in totals, not per claim or burn: a permissionless instruction mints to the fee recipient the
   fees the vault has escrowed for it and LEZ has not minted yet; claims and burns carry no fee value.

## Decision

The fifth, for both fees, in the wrapped token.

**The vault.** A deposit pays the denomination, the mint fee of the schedule in force and the token's relay fee, as
before (the deposit caps count all of it); the vault now escrows the whole payment and adds the mint fee to
`lezFees(token)`, one cumulative, only-growing total per token, published at the storage slot
`sha256("ferrata/vault/lezfees/v1" ‖ token)` (as the registry entry and the fee schedules are) and announced by
`LezFeesAccrued(token, amount, total)`. A release pays the recipient the denomination less the relay fee (0 or the
registered one), the burn fee and the checkpoint fee in force, as before; the checkpoint fee is still credited to the
checkpoint's prover on Ethereum (it pays provers, not the protocol), and the burn fee stays in escrow and is added to
`lezFees(token)`. A refund after a sunset returns the denomination and the relay fee (the mint fee is earned at the
deposit, as under ADR 0046, and stays in escrow for the fee recipient on LEZ). The Ethereum `feeRecipient` immutable is
kept (the deployment parameters do not change) and is credited nothing. Mint and burn fees must be whole LEZ units;
every fee is at most 1% of its denomination (immutable), schedule 0 (the registration's) has no mint or burn fee, and a
change takes effect seven days after the admin announces it, as before.

**The bridge program.** `CollectFees { config_id, token, record_slot, limit, proof }` (message variant 31), sent by
anyone at `config(cid)`, attests the vault's `lezFees(token)` slot under the configuration's light-client record at
`record_slot` (the attestation program's `Attest`, as `Register` and `AttestListing` attest their slots). The reply
sends `AccrueFees` (32) to the new account `fees(cid, token)` (a PDA keyed by the configuration and the token, state
`FeesState { config_id, token, collected }`), which mints the part of the attested total it has not minted yet, at
most `limit`, and keeps the new count: a total below the count (an older record) is refused (`IMPORT_MISMATCH`), as is
a collection with nothing due (`ALREADY_WRITTEN`). The total and the count are 256-bit (the slot's word), so they never
wrap; one collection mints at most `limit`, a `u128` and a whole number of LEZ units, and a larger amount due takes
several. It sends `MintFees` (33) to `token(cid, token)`, which applies the
token's mint freeze and bucket and has `config(cid)` apply the global ones (`Admit`), as a claim's `Issue` does, then
credits the amount (`Credit`) to the configuration's fee recipient (`Controls::fee_recipient`, fixed at `Init` and
committed by `configId`). `Claim`, `ClaimPrivately` and `Issue` are as since ADR 0046 (the denomination to the
destination, the registered relay fee to the relayer); `Burn` debits exactly one denomination. The new variants and
the new account state are appended, so every message and account the B1 program encodes reads the same.

**The clients.** `ferrata_wallet::fees` builds and dry-runs `CollectFees`; `ferrata-relayer collect-fees` sends it
under the newest record the admin's inbound depth allows, or the one `--record-slot` names (the record's block checked
against the Ethereum endpoint, as for a root import), and refuses before sending on a B1 zone (B1 has no such
instruction), for a token not registered, and for an amount that is not whole LEZ units; with nothing due it sends
nothing. This tree's program is `ferrata_bridge_lezfee` (`docs/image-ids.md`). `ferrata` and the relayer read which
program a zone runs (the image id its program loader names for the bridge account; the chain service, as `ferrata`
does, before it signs a module's claim), the bridge module the one it records for its deployment (checked against the
program account at open); each builds B1's claims for B1 (the test network) and this layout for
`ferrata_bridge_lezfee`, and refuses any other program (ADR 0046's included, never deployed) before anything is sent.

## How each part of the clause is met

| The RFP | Here |
|---|---|
| a protocol fee on minting and on burning of the wrapped token on LEZ | the bridge program mints both, on LEZ, to the fee recipient (`CollectFees`): the mint fee of every deposit and the burn fee of every release |
| in a denomination: wrapped token, native LEZ token or a mix | the wrapped token: self-contained (the vault holds exactly the asset that backs it), no LEZ balance needed from the user or the relayer (option 4) |
| at a rate configurable by the admin authority per deployment | `announceFees` per token and denomination, by the deployment's admin; each deployment (vault and configuration) has its own (F.16) |
| the admin can also set either fee to zero | a schedule's mint or burn fee may be 0 independently; schedule 0 has neither |
| the fee value must not distinguish a user's transaction from others | a claim mints the denomination and the fixed relay fee only; a burn burns one denomination; the deposit's payment depends only on the schedule in force at its block, the release's payout only on the schedule in force at its block (both times public on Ethereum anyway); `CollectFees` mints a total over every deposit and release up to a record and names no deposit, claim, burn or leaf |
| collection point | the mint fee is paid at the deposit (on top of the denomination) and the burn fee at the release (out of the payout), each escrowed by the vault; both are minted on LEZ by `CollectFees`; a claim mints, and a burn burns, whole denominations, so no partial amount meets the fixed denominations |
| routing | the configuration's fee recipient on LEZ (`Controls::fee_recipient`); the relay fee still pays the relayer and the checkpoint fee the checkpoint's prover |

## Rationale and trade-offs

No public value of a claim or a burn depends on a fee schedule, so a fee change splits no anonymity set on either
side; what a collection reveals (the fees of a span of deposits and releases) is the sum of public Ethereum values.
Conservation: per token, the vault's escrow less the wrapped supply on LEZ is the unminted deposits (denomination and
relay fee each), the unreleased burns and the fees escrowed but not yet collected (`lezFees − collected`), never
negative, since a collection mints at most the attested total and the count never goes back. The fee mint passes the
mint freeze (F.14) and the mint buckets (F.11) as any mint does; `limit` splits a collection that a bucket would
refuse. Costs: the fee recipient is paid only when someone collects (anyone may; the fee recipient has the reason to);
one more instruction and one more account kind; the vault, the bridge program (its image id moves) and the clients
change, so a deployment needs a new vault and a new LEZ configuration, never an upgrade in place; the both-halves
scenario and the end-to-end runs are made again.

## Pairing and what does not change

A vault of this rule pairs with a bridge program of this rule. A program of this rule paired with an earlier vault
reads a zero total (the slot is never written there) and mints nothing; B1's program paired with a vault of this rule would mint
each claim's mint fee on LEZ (backed: the vault escrows it) and leave the burn fees in escrow with nothing to collect
them, so the redeployment replaces both (`deploy/testnet/F13-REDEPLOY.md`). Unchanged: every vault function and event
that existed (one view, `lezFees`, and one event, `LezFeesAccrued`, are added), the deposit leaf and commitment, the
release journal and the release and checkpoint guests (they do not embed the bridge program's id), the checkpoint's
reading of LEZ blocks (it decodes variants 0–3 and ignores the rest), the burn note, and the claim encoding since ADR
0046. Not deployed: the test network runs B1's program and its vault.

## Sources

- RFP-022 F.13 (logos-co/rfp at `920fad9`: Functionality #13 and Design Rationale, "Fee structure"; `site/data/rfp-requirements.json`)
- ADR 0021 (the fees of D13), ADR 0046 (the mint fee at deposit), `e2e/both-halves/results/fees.md` (the split under D13)
- `vault/src/FerrataVault.sol` (`_recordDeposit`, `release`, `lezFees`), `vault/src/FerrataRegistry.sol` (`_storeFees`)
- `lez/bridge-wire/src/lib.rs` (`lez_fees_slot`, `FeesState::accrue`, `check_lez_fees`, messages 31–33),
  `lez/guests/bridge/src/main.rs` (`collect_fees`, `accrue_fees`, `mint_fees`)
- `cli/wallet/src/fees.rs`, `cli/src/relayer.rs` (`plan_fee_collection`), `cli/src/relayerd.rs` (`collect-fees`)
- The tests and runs that show it: the F.13 rows of `docs/requirements-status.md` and `docs/traceability.md`
