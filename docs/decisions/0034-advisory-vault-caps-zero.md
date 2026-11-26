# ADR 0034: Answer the Groth16 advisory on the four November vaults by setting release and deposit caps to zero

- **Date:** 21 November 2026
- **Status:** accepted; executed on Sepolia, the vaults now superseded
- **Requirements:** RFP-022 R.8, Sec.2, Sec.3, Sec.6, Sec.9

## Context

RISC Zero's two advisories made the deployed verifier 3.0.0 accept a forged claim. The four Sepolia vaults used our own
verifier behind our own emergency-stop contracts, so RISC Zero's stop of its own deployment did not reach them. Their
stop owners were renounced or keyless, their freeze authority had no key holder and they had no watchers or caps. The
advisory supplied no receipt for the all-zero claim digest, and whether the described forgery could produce one was
not established; none of the four stops tripped. Deposits, releases and, on the three v5 vaults, refunds therefore
remained exposed to whatever anyone later deposited, although only 0.001 Sepolia ETH was then reachable.

## Options considered

1. Trip an emergency stop with a proof of exploit. *Recorded:*
   docs/security/risc0-groth16-advisory-2026-11-21/README.md, "What stops it". No public receipt had done so, no
   suitable proof was available, and this would not close deposits. A trip would also lock the escrow permanently.
2. Install a freeze authority and watcher through `proposeRoles`, then freeze and retire. *Recorded:*
   docs/security/risc0-groth16-advisory-2026-11-21/README.md, "What stops it". This was available only on the three
   v5 vaults and took 14 days; the v3 vault had no such path. A freeze also lapsed after 30 days unless renewed.
3. Use the admin's immediate controls: set both the release and deposit global caps to zero on every vault.

## Decision

The third. The owner sent `setGlobalCap(Release, 0, 0)` and `setGlobalCap(Deposit, 0, 0)` on each of the four vaults
on 21 November 2026. The read-back showed both caps at zero on all four. The three v5 vaults were to be retired when
keyless retirement opened, or through a freeze authority after its timelock, and none of the four was to be used again.

## Rationale and trade-offs

The caps were the only immediately available key-controlled operation common to all four vaults. A zero release cap
stopped every release and the v5 refunds; a zero deposit cap stopped new value entering. Unlike the one-way emergency
stop, these caps remained reversible by the admin and did not themselves strand existing escrow permanently.

The limits are material: caps do not gate checkpoint submission or finalisation, so the fork rehearsal still accepted
a forged checkpoint after the calls, although no release against it could pass. Zero release caps stopped refunds on
the v5 vaults too; the v3 vault's refund path did not draw on the cap but could not open through its keyless roles.
The next RISC Zero 3.1 vault superseded these deployments, while their caps remained at zero.

## Sources

- Commit `1e7a1cf3`; docs/security/risc0-groth16-advisory-2026-11-21/README.md, "Affected" and "Status, 21 November
  2026: closed" (the available controls and the eight cap transactions)
- docs/security/risc0-groth16-advisory-2026-11-21/fork-rehearsal.txt (the cap calls, checkpoint limitation and reverts)
- docs/EVIDENCE.md row 154 (read-back after the cap calls)
- docs/design/risc0-3.1-migration.md §3 (retirement and replacement)
