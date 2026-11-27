# If LEZ's check of private-transaction receipts is unsound: affected funds, in-flight positions, and why there is no failsafe

RFP-022 Security #9, the case of a vulnerability below Ferrata's own verification: LEZ accepts a forged receipt of a
private transaction. Written 27 November 2026. The Ethereum side of every outcome below is shown by
`vault/test/LezReceiptUnsound.t.sol` (9 tests); the LEZ side is from reading the bridge program and the checkpoint
program at this commit (cited lines), not tested. Whether LEZ accepts a forged receipt at all has not been
established: we built none, and it is for Logos to confirm ([`docs/trust.md`](../trust.md) §5).

## 1. The case

Security #9 asks what happens to "funds already at risk" and to "deposits and burns left in-flight once a
vulnerability in the verification logic is found", and allows, where no failsafe meets its three constraints, a
document of "why a failsafe should not be implemented, and what happens to affected funds and in-flight positions in
its absence". This document is that alternative for one failure case the earlier records leave open (ADR 0016: a
tripped stop and a bug in Ferrata's guests; ADR 0033: a broken Ethereum verifier with no proof of exploit; the
row's "Not shown").

The premise ([`docs/trust.md`](../trust.md) §5): LEZ v0.4.0-rc1 and rc3 verify private-transaction receipts with
risc0-zkvm 3.0.5, in the range of RISC Zero's advisory GHSA-g6qg-9j8p-7j87, and replay the public calls a private
transaction's journal lists with the sender the journal names. If a forged receipt is accepted, a forger can deliver,
in the bridge program (`lez/guests/bridge/src/main.rs`):

- `Mint` from a nullifier's actor (`:180-193`): wrapped tokens credited to any holder, without a deposit and outside
  the gate that applies LEZ's mint caps and freezes;
- the burn record from a token's actor (`:758-765`): a burn note at `burn(cid, r)` that no holder paid for, outside
  the burn caps and freezes; the checkpoint program reads it as any burn (`checkpoint/crates/checkpoint-core/src/rule.rs:110-113`);
- `root(cid, n)` from the configuration's actor (`write_root`, `:443-459`): a deposit root of the forger's choosing,
  claimable on LEZ, which the checkpoint does not count (it counts `ImportRoot` messages only, `rule.rs:116-118`);
- through the attestation program's records, which are in reach too (trust.md §5), a top-level `ImportRoot` of a
  forged root and leaf count, which the checkpoint does count;
- **the configuration's role messages** (added by this document): `config(cid)` authorizes `Sunset`, `Freeze`,
  `SetCap`, `SetDepth`, `ProposeRoles`, `CancelRoles` and `Pause` by the sender actor (`from_account`, `:920-923`,
  `:980-984`). A role's own actor takes only top-level calls (`:972-977`), but a replayed call can be delivered at
  `config(cid)` directly, naming the role's actor as its sender. So the forger can also lift or set LEZ's freezes,
  raise or zero LEZ's caps, and send `Sunset` (which the checkpoint counts as a visible call, `rule.rs:119`).
  [`docs/trust.md`](../trust.md) §5 lists the role actors as out of reach; that holds for the role actors, not for
  the messages they forward.

## 2. Why the Ethereum side cannot tell, and what it can still do

Every check the vault makes is about whether a checkpoint states LEZ's state truly: the proof of the checkpoint
program, the candidate's wait, the watchers' cancellations, `revokeFrom` inside the window. In this case the checkpoint
states LEZ's state truly; LEZ's state is what is wrong. A forged burn's release is the same transaction, the same
checks and the same event as a paid one
(`test_lezUnsound_aForgedBurnInAnHonestCheckpointIsPaidLikeARealOne`). Revoking the checkpoint removes a root, not
the record: LEZ's burn tree only grows, the next honest checkpoint carries the forged burn again, and it is released
there (`test_lezUnsound_revocationOnlyDelaysAForgedBurnThatLezStateKeeps`).

What is left on Ethereum: the release caps bound the rate of forged releases, not their total
(`test_lezUnsound_releaseCapsBoundTheLossPerWindowNotInTotal`); the escrow of each token bounds the total; the
freezes stop releases while they are renewed; retirement stops deposits. Nothing on LEZ bounds forged mints (they
bypass the gate, and the role messages that set the gate are in reach).

## 3. The containment runbook (procedure, not enforced by the vault)

The freeze authority, on the first sign (an advisory, or wrapped supply on LEZ above the vault's escrow):

1. Freeze releases **per token** (`setTokenReleasesFrozen(token, true)` for every registered token), not globally:
   the global release freeze also stops refunds, which here are the one exit that stays sound (section 5).
2. Freeze deposits and retire the vault (`setDepositsFrozen(true)`, `retire()`).
3. On LEZ, the freeze authority's `Sunset`, so LEZ imports nothing more; the next checkpoint is a sunset checkpoint.
   (A forger can send `Sunset` too; either way the checkpoint carries it.)
4. Let that sunset checkpoint finalise, then freeze checkpoints (`setCheckpointsFrozen(true)`).
5. Renew every freeze before its `maxFreeze` (30 days by default) runs out; a lapse reopens releases.

`test_lezUnsound_theRunbookHoldsEveryPositionWhileRenewed` runs these steps and checks every position below, and that
a freeze renewed on day 29 still holds on day 31.

## 4. Each position

| Position | Under the runbook, while renewed | If a freeze lapses, or before detection | Shown by |
|---|---|---|---|
| Deposit LEZ never imported | refunded whole (denomination and relay fee) to the depositor its leaf names, once the sunset checkpoint is past its revocation window; per-token release freezes do not stop refunds | refunds draw on the token's escrow: forged releases made before detection can leave it short, and the refund is refused (`InsufficientEscrow`); a forged import count at the sunset moves the refund boundary up, and a deposit below it is not refundable; **a forged import count above the vault's tree, if it lands before LEZ's `Sunset`, means no checkpoint is ever finalised again, the sunset checkpoint included: the boundary is never set and nothing is refunded** (after retirement no deposit can catch the count up; a count above 2^32 never can) | `…theRunbookHoldsEveryPositionWhileRenewed`; `…forgedReleasesBeforeDetectionCanLeaveAnUnimportedDepositUnrefundable`; `…aForgedImportCountAtSunsetLeavesADepositBelowItUnrefundable`; `…afterRetirementAForgedCountAboveTheTreeEndsCheckpointsAndRefunds` |
| Deposit imported on LEZ, not yet claimed | claimable on LEZ (claims of imported roots go on after `Sunset`); the wrapped token it mints is diluted by forged mints, and its way back is a burn, which waits (below) | the same | LEZ side, by reading (`main.rs:154-160`, `:301-303`) |
| Wrapped tokens held on LEZ | diluted: the vault's escrow backs them and the forged mints alike; redeemable only through a burn, which waits | a race with forged burns for the token's escrow | by reading; the race: `…withoutTheFreezeTheShortfallFallsOnTheLastRedeemer` |
| Burn not yet in a finalised checkpoint | waits: no candidate can be submitted while checkpoints are frozen | released in the race once a checkpoint carries it; a forged import count above the vault's tree halts every checkpoint (`ImportedLeafCountTooHigh`) until real deposits catch up, which they cannot once the vault is retired, nor ever above 2^32 leaves: then the burn waits for good | `…theRunbookHoldsEveryPositionWhileRenewed`; `…aForgedImportCountAboveTheTreeHaltsCheckpointsUntilDepositsCatchUp`; `…afterRetirementAForgedCountAboveTheTreeEndsCheckpointsAndRefunds` |
| Burn under an accepted root, not released | held, its nullifier unused (`TokenReleasesAreFrozen`), and so is every forged one | paid in arrival order with the forged burns; when the escrow runs out the last are refused (`InsufficientEscrow`): the shortfall falls on the late redeemers | `…theRunbookHoldsEveryPositionWhileRenewed`; `…withoutTheFreezeTheShortfallFallsOnTheLastRedeemer` |
| `owed` balance (a failed payout, a relay or checkpoint fee) | claimable by its payee at any time; no freeze stops `claimOwed` | the same | `…theRunbookHoldsEveryPositionWhileRenewed` |
| The escrow of each token | stays in the vault; only refunds and `owed` claims move it | leaves at the rate of the release caps, to forged and paid burns alike | `…releaseCapsBoundTheLossPerWindowNotInTotal` |

If the vault is later replaced (a new vault and configuration once LEZ is fixed), the positions held in the old vault
strand there: unreleased burns and the wrapped tokens of the old configuration have no path to the new vault, which
accepts nothing of the old one's. If instead the freezes are left to lapse once LEZ is fixed, releases resume in
arrival order, and every forged burn already in LEZ's state is releasable among them.

## 5. Why no failsafe meets Security #9 here

The three constraints, against this case:

1. *"Any recovery must still be claimed by the depositor or burner proving their own entitlement."* A burner's proof
   of entitlement is a burn record in LEZ's state, which is exactly what the forger can write. No proof LEZ's state
   can give separates a paid burn from a forged one, and the release proof is sound for both (section 2). A recovery
   that pays burners pays the forger; one that refuses the forger refuses every burner.
2. *"Not by an admin authority ... identifying who owns what."* A list of honest burns, or a cut-off time before which
   burns are trusted, would be exactly that, and a cut-off would also need the vault to apply a new rule, which
   Security #8 forbids: the vault's verification logic cannot change after deployment. A forger acts before any
   cut-off anyway: the exploit precedes its detection.
3. *"Must not be able to act on funds beyond what a specific, proven vulnerability put at risk."* Nothing on Ethereum
   measures what a forged LEZ receipt put at risk: the forged mints are invisible to the vault, and the forged burns
   look like paid ones.

One exit does meet all three, and the runbook keeps it open as far as the vault can: a depositor's refund, proven
against Ethereum's own deposit tree and paid only to the depositor its leaf names, after a sunset checkpoint. That is
why the runbook freezes releases per token rather than globally. Its limits are in the table: it pays only out of what
the escrow still holds, only above the refund boundary the sunset checkpoint sets, which a forged import can move, and
only if a sunset checkpoint can be finalised at all, which a forged import count above the vault's tree, landed before
LEZ's `Sunset`, prevents for good.

So the affected funds stay in the vault while the freezes are renewed; unimported deposits are refunded, unless the
forger raised LEZ's imported count above the vault's tree first, in which case they stay in the vault with everything
else; `owed` balances are paid to their payees; holders of wrapped tokens and burners in flight wait, and lose what the forged
burns take if releases ever resume, or everything held in the old vault if it is replaced. The remedy is upstream:
a LEZ whose receipt check is sound (a LEZ tag on RISC Zero 3.1, the migration's phase 2,
[`docs/design/risc0-3.1-migration.md`](../design/risc0-3.1-migration.md)).

## 6. Running it

```bash
cd vault && forge test --match-path test/LezReceiptUnsound.t.sol   # 9 passed
just security-review                                               # these, the review's fixes and the R.8 checks
```
