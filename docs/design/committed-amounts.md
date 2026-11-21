# Committed amounts: the migration path (RFP-022 soft requirement 1)

The clause, verbatim: "**Committed amounts.** Record transferred values on the LEZ side as value commitments rather
than restricting transfers to a fixed set of permitted amounts, removing the amount-visibility constraint. This merges
all per-amount anonymity sets into one and reduces the need for users to split transfers. Whatever is delivered under
the hard requirements should be designed so this can be adopted later without redeploying the vault or resetting
accumulated anonymity; document the intended migration path even if it is not implemented."

**Status: not implemented. This note is the migration path the clause asks for, checked against the code as it is.**

## What shows an amount today

Every deposit, mint, burn and release is one registered denomination ([`bridge.md`](bridge.md) §13, ADR 0014). What
each step publishes ([`bridge.md`](bridge.md) §17):

| Step | Chain | Shows the denomination because |
|---|---|---|
| Deposit | Ethereum | an ERC-20 transfer or `msg.value` is public on Ethereum by construction; no LEZ-side design changes it |
| Claim | LEZ | the bridge program mints the denomination and pays the mint and relay fees of that denomination in public; the LEZ caps count it |
| Burn | LEZ | the burn note carries `token, amount` and the burn leaf hashes them: `H(0x02 ‖ BURN_TAG ‖ configId ‖ burnIndex ‖ token ‖ amount ‖ COut ‖ r)` |
| Release | Ethereum | the vault pays `amount`; public on Ethereum by construction |

So a claim hides among the deposits of its token **and denomination**, a release among the burns of its token and
denomination. Committed amounts remove the denomination from the two LEZ rows.

## The inbound leg: adoptable without touching the vault

What the deployed vault already provides:

1. **Each deposit leaf binds its amount**: `leaf = H(0x00 ‖ LEAF_TAG ‖ leafIndex ‖ depositor ‖ token ‖ amount ‖ C)`
   (`bridge/crates/ferrata-bridge-core/src/inbound.rs`, `deposit_leaf`). A claim that keeps `amount` inside its proof
   and publishes a value commitment `V` to it instead (with a blinding factor) can be checked against the same tree.
2. **The nullifier does not depend on the LEZ program**: `N = H("ferrata/nf/v1" ‖ s ‖ fid)`, `fid` from the chain,
   the vault, the leaf index and the leaf (`nullifier`, `deposit_id`). A second LEZ program computes the same `N` for
   the same deposit.
3. **The vault does not care how LEZ mints.** It escrows, records the leaf and later releases against finalized
   checkpoints; the claim side is LEZ's.

The path, for deposits:

1. Deploy a second LEZ bridge program (a new image, beside the deployed one, as every program change is: ADR 0004)
   whose claim publishes `N`, the token, the root read and `V`, mints the amount into the claimant's **private**
   holding as a committed value, and takes fees that do not depend on the amount (a fixed fee per claim, or fees
   collected on the Ethereum side only, which the vault already escrows per deposit).
2. **No double claim across the two programs:** before minting, the new program reads the deployed program's
   nullifier actor for `N` and refuses if it exists; the deployed program is retired for claims (`Sunset` at a leaf
   count) so its set stops growing. The same `N` on both sides is what makes this a read, not a migration of state.
3. **No reset of anonymity:** the new program claims against the same deposit tree, from leaf 0. Every deposit the
   vault ever took, of every denomination, is a candidate for every committed claim: the per-denomination sets merge.
4. **Caps:** the LEZ-side mint caps count public amounts. With committed mints they move to the vault's deposit caps
   (already enforced per token and globally on Ethereum, `vault/test/Caps.t.sol`), or the new program sums commitments
   homomorphically with a range proof per claim (Pedersen commitments; a larger proof).

The deposits stay fixed denominations on Ethereum (they are public there anyway); what changes is that a claim no
longer says which one.

## The outbound leg: needs a new release guest, so a new vault

A committed burn publishes `V` instead of `amount`, and its leaf hashes `V`. The checkpoint program and the release
guest read burn leaves, and the vault pins both programs' image ids as immutables (Security #8). So committed burns
mean a new release guest, a new vault, and the retirement path of
[`deployer-guide.md`](../guides/deployer-guide.md) §9: holders release under the old vault until it is retired, new
burns go to the new one. This is the part of the clause the current vault does not meet: adopting committed amounts on
the way out redeploys the vault. What would have avoided it, stated for the next version: a vault that pins, from its
deployment, a second release image for committed burn leaves (unused until LEZ switches), so the switch is LEZ-side
only.

## What is built and tested toward this

Nothing of the committed-amount programs. Two properties the path rests on are already tested: the deposit leaf binds
the amount (`bridge/crates/ferrata-bridge-core/tests/inbound.rs`, every single-field change of a claim fails) and the
nullifier is a function of the deposit and the user's secret only (`tests/vectors.rs` against `bridge/vectors/`).
