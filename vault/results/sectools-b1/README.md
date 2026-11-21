# Static analysis of the vault on branch `rfp-bridge-hardening` (milestone B1)

The tools and settings of [`docs/reviews/security/tools-2026-11-11.md`](../../../docs/reviews/security/tools-2026-11-11.md)
(`scripts/security-tools.sh slither aderyn`, `SECTOOLS_OUT` this folder), run on 21 November 2026 on the build box after
the vault changes of this branch (`setDepositsEnabled` publishes the listing slot through `FerrataRegistry.publishListing`;
the deployment script capped by default; three test files). Versions: Slither 0.11.6, Aderyn 0.6.8, solc 0.8.26,
forge 1.3.6 (as on 11 November). Aderyn 0.6.8's release no longer carries binaries and is not on crates.io: it was built
from the release's `source.tar.gz` (sha256 `2d4afdb52bae30f8488e5d9470a281d284c0b286455b61b474e6c2e7dafe343a`, checked)
with `RUSTC_BOOTSTRAP=1` for the one nightly feature its source enables (`result_option_map_or_default`).

| Tool | Result | Against |
|---|---|---|
| Slither, every detector, `src/` only | 38 results (`slither-checklist.md`; `slither.json` is not committed, as on 11 November) | the vault of `f13faee2` run the same way: 38 results, the same 38 once line numbers are set aside (`compare-f13faee2.txt`); the change adds none. 11 November's run had 37 on the code of that day |
| Aderyn, every detector, `src/` | 5 High and 13 Low issue kinds (`aderyn.md`, `aderyn.json`) | 11 November: 5 High and 12 Low. The added kind, "Loop contains `require`/`revert`", is in `src/standin/StandInCheckpointVerifier.sol:150`, a stand-in added after 11 November; no instance is in the code this branch changed |

No result points at `publishListing` or `setDepositsEnabled`; the findings near them (`FerrataRegistry.sol:581`'s
`abi.encodePacked` of three equal-length arrays, `applyFees` ignoring `_feesInForce`'s return value) are 11 November's,
triaged there. Re-run: `SECTOOLS_OUT=$PWD/vault/results/sectools-b1 scripts/security-tools.sh install slither aderyn`.
