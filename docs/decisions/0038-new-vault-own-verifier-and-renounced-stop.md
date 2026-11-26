# ADR 0038: New vaults keep their own verifier behind their own renounced stop, backed by keyed freezes, watchers and caps

- **Date:** 23 November 2026
- **Status:** accepted; deployed on Sepolia for the test network
- **Requirements:** RFP-022 Sec.5, Sec.8, Sec.9

## Context

The four November vaults' own verifier and renounced stop meant RISC Zero could not halt them, but no available proof
of exploit tripped their stops and no key holder could freeze them. The advisory therefore left only the admin's caps.
For replacement vaults, migration decision D1 had to choose between keeping that verifier ownership model, delegating
the permanent stop to RISC Zero's deployed guardian, or retaining an owner key for our own stop. Any permanent stop
prevents releases and refunds and leaves escrow locked.

## Options considered

1. Point the vault at RISC Zero's own deployed emergency stop. *Recorded:*
   docs/design/risc0-3.1-migration.md §4, option (b). RISC Zero's guardian would halt on an advisory and the vault
   would require no stop change, but that third party could impose a permanent halt. Its 3.0.0 stop occurred days after
   the advisory, by owner key rather than a proof of exploit, and the stop owners were externally owned accounts.
2. Keep our own stop with an owner, such as a multisig. *Recorded:*
   docs/design/risc0-3.1-migration.md §4, option (c). This gives us a permanent halt we control, while the freeze
   authority already supplies a reversible halt.
3. Use our own verifier 3.1.0 behind our own deployment of RISC Zero's unmodified emergency-stop contract, with its
   owner renounced. Pair it with a freeze authority whose key is held, at least one watcher, release caps on every
   deployment holding value and a non-zero production release floor. On an advisory, freeze releases and checkpoints,
   set deposits to zero, then retire and replace. *Recorded:* docs/design/risc0-3.1-migration.md §4, option (a) and its
   recommendation; vault/results/fork-rehearsal-31/README.md identifies the rehearsed deployment as D1 (a).

## Decision

The third, D1 (a). The deployment creates the RISC Zero verifier and stop itself, checks the verifier's 3.1.0 selector
and control identifiers, and renounces the stop owner before token registration. It does not use RISC Zero's hosted
stop address. The deployed vault has a held freeze authority and bounded caps (`vault/deployments/sepolia-31-B.json`,
EVIDENCE row 232); the fork rehearsal also set a watcher, which the deployment record does not list. The stop remains
available only to a verifying proof of exploit.

## Rationale and trade-offs

No third party can irreversibly freeze the vault, and no retained stop-owner key can bypass the proof-of-exploit rule.
Unlike the advisory response on the old vaults, a broken verifier now has an immediate reversible answer: the keyed
freeze authority can stop releases and checkpoints while the caps bound what can leave before it acts, and a watcher
can pause checkpoints. `estop(receipt)` remains the last resort rather than the only operational response.

A one-way stop still strands all non-owed escrow, so the freeze must be used before a proof of exploit trips the stop.
The freeze authority can repeatedly renew freezes and a compromised watcher can pause checkpoints until replaced;
neither can move escrow. The deployment depends on those keys being held and operated, and on caps being configured,
so renouncing the stop owner alone is not sufficient. How escrow can leave after a verifier itself is broken remained
open here because both releases and refunds rest on proofs; ADR 0033 records the answer for a verifier found unsound
with no proof of exploit.

## Sources

- Commits `d34c75c2` and `b9057be6`; docs/design/risc0-3.1-migration.md §4 (all three options and D1's recommendation)
- vault/results/fork-rehearsal-31/README.md (D1 (a), renouncement order, freeze and watcher refusal tests)
- vault/README.md, "Verifier and stop" and the deployment sequence (the unmodified stop and its effects)
- vault/deployments/sepolia-31-B.check.sh (the deployed stop, verifier, roles and caps read back)
- docs/EVIDENCE.md row 232 (the Sepolia deployment)
