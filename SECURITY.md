# Security

Ferrata is a research prototype. It has not been audited, it holds no funds of anyone but its
author, and it must not be used with real value. The open findings and the review status are in
[docs/EVIDENCE.md](docs/EVIDENCE.md), [docs/REVIEW-core.md](docs/REVIEW-core.md),
[docs/REVIEW-vault.md](docs/REVIEW-vault.md) and [docs/REVIEW-guests.md](docs/REVIEW-guests.md).

## Advisories

- 21 November 2026: RISC Zero's Groth16 advisories of 11 November 2026 (GHSA-g6qg-9j8p-7j87,
  GHSA-mm6c-35cm-g4x2). The Sepolia vaults verify through verifier 3.0.0, which accepts forged claims; what that
  exposes and the plan: [docs/security/risc0-groth16-advisory-2026-11-21/](docs/security/risc0-groth16-advisory-2026-11-21/README.md).

## Reporting a vulnerability

Please report it privately through GitHub: the repository's **Security** tab, **Report a
vulnerability**. Include the commit, the component (`lez/`, `vault/`, `bridge/`, `checkpoint/`,
`modules/`, `cli/`) and the steps or test that show it. I will acknowledge a report within seven days
and publish the fix and an advisory once it is fixed. There is no bug bounty.

Problems in the systems Ferrata builds on (LEZ, Logos Core modules, Helios, RISC Zero) belong to those
projects' own trackers.
