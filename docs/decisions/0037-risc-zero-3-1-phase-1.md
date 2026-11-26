# ADR 0037: Move the Ethereum side to RISC Zero 3.1 first, and leave the LEZ side on LEZ's 3.0.5 until LEZ moves

- **Date:** 23 November 2026
- **Status:** accepted; phase 1 built and deployed on Sepolia, phase 2 waits on Logos
- **Requirements:** RFP-022 R.8, Sec.2, Sec.3, Sec.6

## Context

The Groth16 advisories fixed the zkVM at 3.1.0 with a new trusted setup and the Ethereum verifier at 3.1.0. The rv32im
seal format and circuit identifier changed, so earlier receipts and seals do not verify under 3.1.0. One proving
process uses one `risc0-zkvm`: LEZ v0.4.0-rc1 and rc3 use 3.0.5, while Ferrata's CLI and bridge module link LEZ's
wallet and prove releases in process. They cannot produce a 3.1 release seal and a valid rc1 or rc3 private transaction
in the same process. A complete migration therefore needed a LEZ tag on 3.1, which did not exist.

## Options considered

1. Wait to move everything together until LEZ publishes a tag on RISC Zero 3.1. *Recorded:*
   docs/design/risc0-3.1-migration.md §2–§3, which defines that LEZ-side phase and says that it waits on Logos. This
   would leave the deployed Ethereum verifier affected in the meantime.
2. Split the migration: move the Ethereum-side guests, provers, verifier, fixtures and new vaults to 3.1 now; keep the
   LEZ programs and LEZ-linked hosts on LEZ's 3.0.5 until the first suitable LEZ tag, using a separate Ethereum-side
   prover for release seals in the interval.

## Decision

The second, called phase 1 and phase 2 in the migration plan. Phase 1 moved the checkpoint and release workspaces,
Ethereum-side provers, the prover image and risc0-ethereum to their fixed 3.1-compatible versions; the old RISC Zero
3.0.5 image-id rows were kept as history and the Ethereum guests rebuilt as `*_31`. The CLI and module must use a
separate prover for release seals until phase 2. The implemented phase 1 includes a new rule (B) vault on Sepolia,
regenerated rule (B) seals and a fork rehearsal. LEZ programs and LEZ-linked hosts remain on 3.0.5.

## Rationale and trade-offs

The Ethereum migration has no Logos dependency, so splitting removes the affected verifier without pretending the LEZ
side can consume incompatible receipts. The new verifier refuses an old seal with `SelectorMismatch`; new image ids
make that incompatibility explicit. The phase-1 rehearsal repeated the recorded rc3 session's checkpoint and two
release proofs under 3.1 and exercised the new deployment's controls before the Sepolia deployment.

The cost is two proving versions and a separate release prover. Every Ethereum-side id moved and every seal used by
those vaults had to be made again; 11 suites representing old 3.0.5 records remained skipped, including every rule (A)
real-proof case. `claim` and `gasdrop` received 3.1 ids for later use, but their consumers still wait for phase 2. Most
importantly, the deployed test network's LEZ programs still depend on LEZ's 3.0.5 receipt verification, whose exposure
the security note leaves for Logos to fix.

## Sources

- Commit `b9057be6`; docs/design/risc0-3.1-migration.md §1–§3 (the version constraint, phases and order)
- docs/security/risc0-groth16-advisory-2026-11-21/README.md, "Affected" and "Correction, 23 November 2026"
- docs/image-ids.md, "RISC Zero 3.1: the Ethereum-side guests (migration phase 1)"
- vault/results/fork-rehearsal-31/README.md (regenerated proofs and the rehearsal's scope)
- docs/EVIDENCE.md rows 231–232 (the built phase and Sepolia deployment)
