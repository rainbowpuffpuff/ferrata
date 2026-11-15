# Security

Ferrata is a research prototype. It has not been audited, it holds no funds of anyone but its
author, and it must not be used with real value. The open findings and the review status are in
[docs/EVIDENCE.md](docs/EVIDENCE.md), [docs/REVIEW-core.md](docs/REVIEW-core.md),
[docs/REVIEW-vault.md](docs/REVIEW-vault.md) and [docs/REVIEW-guests.md](docs/REVIEW-guests.md).

## Reporting a vulnerability

Please report it privately through GitHub: the repository's **Security** tab, **Report a
vulnerability**. Include the commit, the component (`lez/`, `vault/`, `bridge/`, `checkpoint/`,
`modules/`, `cli/`) and the steps or test that show it. I will acknowledge a report within seven days
and publish the fix and an advisory once it is fixed. There is no bug bounty.

Problems in the systems Ferrata builds on (LEZ, Logos Core modules, Helios, RISC Zero) belong to those
projects' own trackers.
