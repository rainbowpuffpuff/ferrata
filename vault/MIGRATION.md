# Migrating a vault configuration, including a proof-system change (RFP-022 Security #8, #9; Soft Requirement 4)

This document is the migration mechanism the RFP asks for, and every claim in it is shown by a test:
[`test/Migration.t.sol`](test/Migration.t.sol) rehearses one configuration change, in order, with every step asserted;
[`test/MigrationFailsafe.t.sol`](test/MigrationFailsafe.t.sol) rehearses the Security #9 branches: a bug in the
proof system itself (the emergency stop), and a bug in one of Ferrata's own guests (the release and checkpoint
freezes and `revokeFrom`). The design behind it: [`../docs/guides/deployer-guide.md`](../docs/guides/deployer-guide.md) §9
("Retirement is the migration mechanism"), [`../docs/design/bridge.md`](../docs/design/bridge.md) §16,
[`../docs/trust.md`](../docs/trust.md) §6, and this vault's [README](README.md), "Refunds" and "Verifier and stop".

The three clauses, quoted:

> RFP-022 Security #8: "... bridge accepts as a valid proof after deployment. Change requires deploying a new
> version and migrating to it. Proposals must document the migration mechanism and how in-flight deposits and burns
> are honoured across it."

> RFP-022 Security #9 asks for a failsafe for funds at risk and in-flight positions once a verification bug is
> found, which must not be a backdoor (any recovery claimed by the depositor or burner proving their own
> entitlement, only to the address the proof specifies, only funds a proven vulnerability put at risk), or else a
> document of why none exists and what happens to affected funds and in-flight positions. This file is that
> document; §5 states why there is no failsafe beyond refunds, and what happens to affected funds.

> RFP-022 Soft Requirement 4: "Design the proof-system components as pluggable, so that future zkVM improvements,
> proof compression, or hardware acceleration can be adopted without restructuring the vault or the bridge program
> (redeployment and migration would be necessary, per Security #8)."

## 1. The mechanism, step by step

One configuration ("v1") is retired, its holders move to a new one ("v2"). There is no upgrade: each vault is
deployed once, `FerrataVault` is immutable, and the migration is a sequence of ordinary, individually permissionless
operations. The rehearsal builds one such world and asserts each step:

1. **Stop new activity.** For a LEZ change the configuration cannot follow, or any planned migration (v1's verifier
   and Ferrata's guests sound): the freeze authority freezes deposits (an immediate, reversible stop) and then
   calls `retire()` (one-way; new deposits revert forever, nothing else changes). When the bug is in one of
   Ferrata's own guests, releases and checkpoints are frozen as well and wrong checkpoints revoked; when it is in
   the verifier, the emergency stop applies and after it nothing more is released or refunded - both in §4. The
   steps below are the first case, the one in which v1 honours every in-flight position.
   — `test_migration_step3_freezeThenRetireStopsNewDepositsForever`
2. **Retire.** `retire()` is called by the freeze authority, or by anyone from `retireOpensAt()` on (30 days
   without checkpoint progress, or once a sunset checkpoint's revocation window has passed), so refunds never
   depend on the freeze key. The deposit tree stops moving, which is what makes refund paths stable.
   — `test_migration_step3_...`, and Roles.t.sol for `retireOpensAt`.
3. **Sunset.** The LEZ side stops importing (`Sunset` on LEZ) and a finalised checkpoint with `sunset = true` fixes
   the refund boundary to its `importedLeafCount`: every leaf at or above it was never imported on LEZ, every leaf
   below it was. A leaf is claimable on LEZ or refundable on Ethereum, never both, because LEZ's `ImportRoot`
   refuses a claim at or above the count it stored.
   — `test_migration_step5_inFlightDepositsAreHonouredAfterTheSunset`.
4. **Refunds.** After the revocation window of that sunset checkpoint (so a forged sunset can be revoked first),
   `refund(leafIndex, depositor, ...)` is permissionless, pays the depositor named in the leaf - proven with a
   Merkle path to the frozen `depositRoot` - and refuses a second refund of the same leaf and any refund that names
   someone else. — `test_migration_step5_...`.
5. **Releases continue.** Burns finalised before the retirement keep releasing as ordinary releases: a finalised,
   non-revoked burn root, the token's release delay elapsed, a proof of the burn, the recipient the burn names.
   Retirement changes nothing on this path (a bug in the release or checkpoint guest does: §4).
   — `test_migration_step4_inFlightBurnsAreHonouredOnV1`.
6. **Deploy the new configuration (v2).** A new vault - the same `FerrataVault` code, unchanged - with a new
   verifier implementation behind the same `RiscZeroVerifierEmergencyStop` wrapper, new image ids, and a new
   `lezConfigId` pairing it to the successor LEZ configuration. Users deposit into v2 exactly as they deposited
   into v1. — `test_migration_step6_v2RunsProofSystemBNeitherVaultAcceptsTheOther`.
7. **Users move.** A holder burns on LEZ against v1, releases on v1 to the address the burn names, and deposits the
   denomination again into v2. A depositor LEZ never imported claims the refund (step 4) instead.
   — `test_migration_step7_usersMove_burnOnLezReleaseOnV1DepositIntoV2`.

## 2. The in-flight positions, honoured

The rehearsal fixes four positions at the moment the bug is announced (`test_migration_step2_inFlightStateAtTheAnnouncement`),
and each is honoured:

| Position | What it is | How it is honoured | Shown by |
|---|---|---|---|
| d1 | deposited; imported by LEZ (counted in a finalised checkpoint's `importedLeafCount`) | its holder claims on LEZ, or burns there and releases on v1 once the migration is over; not refundable (LEZ has it), refused with `NotRefundable` | steps 5 and 7 |
| d2 | deposited; never imported | refunded after the sunset checkpoint and its revocation window, to the depositor named in the leaf, by anyone's call, once | step 5 |
| b1 | burned on LEZ, its burn root finalised, not yet released | released on v1 exactly as an ordinary release: after its root's delay, to the recipient the burn names; a release naming another address is refused | step 4 |
| b2 | burned, released already | nothing owed: it completed before the announcement | step 2 (asserted) |

The escrow accounting is asserted at each step (e.g. step 7 ends with only the relay fees of the already-burned
deposits in the vault: refunds return the leaf's whole escrow, releases pay the bare denomination and the fees
stayed).

## 3. The proof-system change (Soft Requirement 4)

What changes when the proof system does: the verifier contract (a constructor argument - the vault pins
`IRiscZeroVerifier`; a new zkVM release or a compressed proof format is a new implementation of that interface, a
different seal format among them), the image ids (the vault asks about its own), and the wrapper address in front
of it; plus a new vault address and a new `lezConfigId` for the successor LEZ configuration. What does not change:
`FerrataVault.sol` itself, the bridge program on LEZ, and the journal encodings (the domain-separated SHA-256
journals are part of the specification, not of the proof system).

What the rehearsal shows:

- v2, the same vault code, runs a deposit, a checkpoint and a release under proof system B.
- v1 asks its verifier only about its own image ids: a v2 proof, even one registered under v2's image id on v1's
  own verifier, is refused by v1, because v1 asks only for image A. That is the vault's property. (A v1 seal is
  refused by v2 too, but that is the mock's own seal-format check, `MockVerifierB.sol`, not something the vault
  does.)
- Neither vault has any function that changes its verifier or image ids. The vault's 90 external functions are
  listed from the compiled artifact `out/FerrataVault.sol/FerrataVault.json` inside the test: every one whose name
  names the verifier, an image id or the LEZ configuration has stateMutability `view` - there is no setter; calling
  `setVerifier` / `setCheckpointImageId` / `setReleaseImageId` / `setLezConfigId` hits the fallback and reverts;
  and after every admin, freeze-authority, watcher and permissionless action the tests can perform, the four
  getters still answer with the values deployment gave them.

All in `test_migration_step6_v2RunsProofSystemBNeitherVaultAcceptsTheOther` and
`test_migration_step6_noFunctionChangesVerifierOrImageIds` (the mock standing for system B is
[`test/mocks/MockVerifierB.sol`](test/mocks/MockVerifierB.sol)).

## 4. Which stop applies when

- **A LEZ change, any planned migration** - v1's verifier and Ferrata's guests sound: the freeze authority freezes
  deposits, then retires (step 1 above). Releases and refunds keep working: b1 is honoured on v1, d2 refunded after
  the sunset. — `test_migration_step3_...`, `test_migration_step4_...`, `test_migration_step5_...`.
- **A bug in one of Ferrata's own guests** (the specification's answer, D13, and [`../docs/trust.md`](../docs/trust.md)
  §6: the release freeze and `revokeFrom`). The emergency stop cannot be tripped: it needs RISC Zero's proof of
  exploit, which a sound RISC Zero cannot produce whatever Ferrata's guests prove. Every freeze lasts `maxFreeze`
  (30 days) and is renewed by setting it again; the freeze authority renews each one it relies on until v2 is live,
  because a lapsed freeze reopens what it held shut (shown below). Deposits are frozen and the vault retired as in
  step 1, and then:
  - *in the release guest* (a release nobody burned for would verify): **releases freeze at once**, which also
    stops refunds, and checkpoints freeze. While the freeze holds, b1 is not released on v1: its release can only
    be proven by the buggy guest, so it stays in escrow, stranded as after a stop unless the bug proves narrow
    enough to reopen releases safely - the vault cannot tell, the freeze authority decides. d2 can still be
    refunded: a refund does not run the release guest (the vault checks the leaf's Merkle path to the frozen
    deposit root itself), and the per-token release freeze stops releases but not refunds, so once the sunset
    checkpoint is final and its revocation window has passed, holding every registered token's releases frozen in
    place of the global release freeze refunds d2 to its depositor while no release can pay. Deposits LEZ imported
    and nobody burned stay claimable on LEZ. — `test_guestBug_releaseFreezeHoldsReleasesAndRefundsOnlyWhileRenewed`
    (the freeze holds releases and refunds while renewed; left to lapse, the forged release pays),
    `test_guestBug_tokenReleaseFreezesKeepReleasesShutWhileD2IsRefunded`.
  - *in a checkpoint guest* (burn roots, imported leaf counts and the sunset flag all come from it): **checkpoints
    and releases freeze at once** while the finalised checkpoints are checked against the LEZ chain, and
    `revokeFrom` takes back the first wrong one within the revocation window (it revokes every later one too, and
    clears a sunset set at or after it). Releases then reopen: b1, under a root an honest checkpoint finalised
    before the wrong one, releases to the recipient its burn names, and the forged root pays nothing. Checkpoints
    stay frozen until v2 is live, the guest that would make the next one being the buggy one: burns after the last
    honest checkpoint are not released on v1, and d2 is refunded only if an honest sunset checkpoint was already
    final (otherwise one sunset candidate would have to be let through under the watchers' cancel and the
    revocation window - a judgement on the bug, not something the vault decides). A wrong checkpoint finalised
    longer ago than the revocation window cannot be revoked and halts the vault for good (trust.md §6).
    — `test_guestBug_checkpointGuest_revokeTakesBackTheForgedRootAndB1StillReleases`.
- **A bug in the proof system itself** (the verifier would accept a forged seal): the emergency stop applies, as
  [`test/EmergencyStop.t.sol`](test/EmergencyStop.t.sol) shows. It is RISC Zero's unmodified
  `RiscZeroVerifierEmergencyStop`, its owner renounced at deployment, so the only key that trips it is a *verifying
  receipt for the all-zero claim digest* - RISC Zero's own proof of exploit, producible by anyone only if the
  system really is unsound. It is one-way and has no unpause. After it: no release, no refund, no checkpoint proof
  accepted. The in-flight burns are never released on v1 and the unimported deposits are never refunded - that case
  is §5.

## 5. Security #9: why there is no fund-recovery failsafe, and what happens to affected funds

This section is the proof-system case, the one the stop answers; a bug in one of Ferrata's own guests is answered
by the freezes and `revokeFrom`, and what it does to each in-flight position is in §4.

**Why none exists.** There is no escrow-moving recovery path, deliberately. A recovery would need some party to
choose new verification logic - everyone-getting-their-money-back logic, a new verifier, an override - and any such
party and any such choice is the backdoor Security #9 forbids: whoever holds it holds the escrow. Security #8
adds the other half: what the vault accepts as a proof cannot change after deployment, so there is no
"temporarily accept something else" either. The failsafe is stop-only: the emergency stop ends the bleeding
(nothing more is proven, released or refunded), freezing is immediate, and `revokeFrom` within the window takes back
wrong checkpoints. What is *not* a backdoor and does exist:

- `refund`, which pays the depositor proven by a Merkle path to the deposit root, to that depositor's address, out
  of that leaf's own escrow - after it nothing more after a stop (D13);
- `release`, which pays the amount and recipient a *proof* names, never the caller's choice;
- `claimOwed`, which pays a payee only its own already-credited balance, only to itself.

**What happens to affected funds and in-flight positions once the stop has tripped.** Everything is stranded
except `owed` balances (spec v3 D8, as built under D13's "after a stop nothing more is released or refunded"):

- an in-flight burn is never released - `MigrationFailsafe.t.sol`,
  `test_failsafe_stopStrandsInflightPositionsExceptOwedBalances`;
- an in-flight deposit is never refunded, *even when a sunset checkpoint was finalised before the stop and its
  revocation window had passed*: the same refund that succeeds before the stop is refused after it - the "unless a
  sunset checkpoint was finalised first" wording of D8 does not survive D13, and the control and the refusal are
  shown side by side in `test_failsafe_sunsetBeforeTheStopDoesNotOpenRefundsAfterIt`. Those escrows stay in the
  vault for good.
- deposits made after the stop also stay (deposits do not ask the verifier); the runbook retires a stopped vault at
  once so nothing new enters. Shown inside `test_failsafe_noRoleCanMoveEscrowToAnyAddressAfterTheStop`.
- what a payout failed to push before the stop is still claimable by its own payee, to itself - the one thing that
  still moves.

**No party can move escrow to any address.** After the stop, every role's every function that could move value is
tried: `release` and `refund` refuse for every role; `claimOwed` pays a payee only its own balance; every admin
function (token registration, fees, caps, delays, delta, the role changes through their timelocks), every
freeze-authority function (freezes, cancellation, revocation, retirement), the watchers' reject-only powers, and
the permissionless paths are exercised, and a hash of every balance involved is compared after each - nothing
moves. Raw sends are refused by `receive`. There is no `sweep`, no `rescue`, no `unpause`, and no setter for the
verifier or the image ids (§3). A wallet must simply check `verifier.paused()`. —
`test_failsafe_noRoleCanMoveEscrowToAnyAddressAfterTheStop`, and
`test_failsafe_theVerifierBugCase_theEmergencyStopAppliesAndB1Strands` for the verifier-bug variant of the
rehearsal's step 3: the emergency stop replaces the freeze there, and b1 - which the sound-verifier path honours in
step 4 - strands on v1.

`../docs/trust.md` §6 tabulates the same failure classes for the whole bridge.

## 6. Running the rehearsal

```sh
cd vault
nice -n 19 forge build
nice -n 19 forge test --match-path 'test/Migration*.t.sol' -vv
# 15 passed: 8 in test/Migration.t.sol, 7 in test/MigrationFailsafe.t.sol (3 of them the guest-bug branch)

# the whole suite, to show nothing else broke:
nice -n 19 forge test --no-match-contract PrivacySuite
```

The `-vv` run of `test_migration_step6_noFunctionChangesVerifierOrImageIds` also prints the vault's external
function list with each function's state mutability (the ABI check of §3).

Measured on the build box, 21 November 2026: 15 passed, 0 failed; the whole suite (CI's `vault` job) 57 suites,
595 passed, 0 failed.

## 7. What is not rehearsed here

- The LEZ side of the migration: `Sunset` on LEZ, the successor configuration's `Init` and pairing with v2, the
  import of v2's deposit roots, and the claim against them. Those run in the LEZ harness (`lez/`,
  [`../docs/guides/deployer-guide.md`](../docs/guides/deployer-guide.md) §5-§6); this vault test simulates a burn on
  LEZ the way the existing vault tests do - a burn root proved by a checkpoint and a nullifier.
- A real proof system behind system B: the mock stands for it. A real zkVM change would go through the pairing of
  `../docs/guides/deployer-guide.md` §6 with its own deployment check.
- Production parameters: the rehearsal uses the test bed's (delta 4 h, revoke window 3 days, thresholds n>=3/t>=2);
  the sequence is the same at any parameters the constructor accepts.
- The proof-of-exploit receipt itself is simulated (`acceptClaim(bytes32(0))` on the mock), exactly as
  `EmergencyStop.t.sol` simulates it; a real one would be published by RISC Zero. A guest bug is simulated the same
  way: the mock verifier accepts the forged release or checkpoint the buggy guest would let anyone prove.
- The finality rule: the rehearsal's vaults run rule (A) (candidate, beacon, finalisation), the test bed's default,
  while the live vaults run rule (B) (`finalizeAttested`, which reads the stop too). The migration steps do not
  depend on the rule; the rule (B) paths are tested in `test/real/` and `test/invariant/VaultInvariantsB.t.sol`.
