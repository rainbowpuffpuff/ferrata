# Stream rfp-advisory: proposed changes for the integrator

Branch `risc0-advisory` (from `5cf1e8e7`). This stream owns no requirement row. It answers RISC Zero's advisories of
11 November 2026 (GHSA-g6qg-9j8p-7j87, GHSA-mm6c-35cm-g4x2) and asks for the changes below to shared files. Already
on the branch, as the brief asked: README row 50 and two dated sentences, EVIDENCE row 149 and one header sentence
(commit `43e31d01`); the security note `docs/security/risc0-groth16-advisory-2026-11-21/` and the migration plan
`docs/design/risc0-3.1-migration.md` (`de60ab91`).

## Requirement rows (proposed)

| Row | Now | Proposed | Why |
|---|---|---|---|
| RFP-022 R.8 | shown | in part | The clause asks for mature, audited proof systems; the pinned versions (risc0-zkvm 3.0.5, verifier 3.0.0) are yanked and accept forged claims (GHSA-g6qg-9j8p-7j87). Shown again when phase 1 of the migration lands (3.1.0, verifier `0xe71e6620`), by the same tests on regenerated seals. |
| RFP-022 Sec.2 | shown | in part | "Both sides reject invalid proofs": tampered honest seals are refused (rows 12, 42, 46, 75), but verifier 3.0.0 accepts a forged claim, and LEZ rc1/rc3 verify private transactions with risc0 3.0.5 (Logos to confirm). Ethereum side shown again after phase 1; LEZ side waits on Logos. |
| RFP-022 Sec.3 | shown | in part | A forged release journal can name any recipient and fee under verifier 3.0.0 (rehearsed on a fork, new row below). The binding is right; the verifier under it is not. Shown again after phase 1. |
| RFP-022 Sec.8 | in part | in part (reason extended) | Add: the four Sepolia vaults cannot change their verifier; the migration is retirement and new vaults (`docs/design/risc0-3.1-migration.md` §3). |
| RFP-021 rows on the private path (those citing EVIDENCE rows 5, 73, 123, 152) | as they are | no change proposed | Their clauses are about the program's checks and privacy, which hold; the soundness of LEZ's privacy circuit at rc1/rc3 is Logos's. Flag for review if a clause is about proving. |

## New EVIDENCE row (number: the next free one, 154 at `5cf1e8e7`)

| # | What it shows | Numbers | Where it ran | Proofs | Re-check | Details |
|---|---|---|---|---|---|---|
| 154 | **The Sepolia vaults under RISC Zero's Groth16 advisory (GHSA-mm6c-35cm-g4x2): what a forgery could take, and what stops it**, read-only and on a local fork; nothing sent | Sepolia block 12,183,432 (21 Nov 2026 18:00:48 UTC), two endpoints, identical answers: the four vaults verify through their own stops (owner renounced; for the v3 vault the keyless `0xC820…`) around our verifier `0x8f0A…A2A0` (`VERSION()` "3.0.0", `SELECTOR()` `0x73c457ba`); freeze authority `0xC820…129f` (no key), no watchers, caps unlimited with a zero floor, none frozen or retired, admin the deployer on all four. The v5 vaults hold 6·10^12 wei each (escrow 4·10^12, below the smallest denomination of 10^15: nothing reachable); the v3 vault 10^15, reachable after a forged rule (A) checkpoint (wait ≥ 1,500 s, delay 120 s). The owner's controls by `eth_estimateGas` (nothing sent): `setGlobalCap` release 66,941 and deposit 63,396 gas per v5 vault, 59,005 and 58,982 for the v3 vault (508,998 in all; base fee 16 wei); `proposeRoles` 369,833, applied 14 days later. Fork (anvil 1.5.1 at the same block, the verifier's code replaced by STOP as the forgery's stand-in): two 0.001 ETH deposits taken by a stranger (a release against the existing burn root; one against a forged checkpoint finalised after 3,600 s); after the admin's two calls the same releases revert `GlobalReleaseCapExceeded`, a deposit `GlobalDepositCapExceeded`, another address's call `NotAdmin`, and all four vaults accept both calls; through the timelock a new freeze authority freezes releases and checkpoints and retires the vault, until the freeze lapses after 30 days | Sepolia, read-only (`eth_call`, `eth_getBalance`, `eth_estimateGas`) on publicnode and ethpandaops; an anvil fork of Sepolia; laptop, 21 Nov 2026; $0 | none (the forgery is stood in for, not built) | `docs/security/risc0-groth16-advisory-2026-11-21/reads.sh URL BLOCK OUT`; `estimate.sh URL OUT`; `fork-rehearsal.sh URL BLOCK OUTDIR` | [`docs/security/risc0-groth16-advisory-2026-11-21/`](../security/risc0-groth16-advisory-2026-11-21/README.md) |

## One-line notes for other EVIDENCE rows (proposed)

- Rows 41, 91, 92, 141, 142 (the live round trips), at the end of "What it shows": "The vault verifies through our
  verifier 3.0.0, which accepts forged claims (GHSA-mm6c-35cm-g4x2, 11 Nov 2026): the seal checks show the seals are
  well-formed; see the security note."
- The EVIDENCE header's list of transactions sent to a public network: once the owner has sent the cap transactions,
  add them ("and the owner's eight `setGlobalCap` transactions of <date>, which closed the four vaults").
- The security note's "Status" line, README's security sentence and `docs/sepolia.md`'s note: change when the caps
  read zero on chain (`reads.sh` again, `globalCap(1)` and `globalCap(0)` = 0, 0, 0 on all four).

## docs/README.md (proposed lines; `rfp-docs-final` edits this file)

- Under the design documents:
  `- [docs/design/risc0-3.1-migration.md](design/risc0-3.1-migration.md): moving to RISC Zero 3.1 after the Groth16 advisories: what moves, in what order, what waits on Logos, the tests.`
- Under the reviews:
  `- [security/](security/): dated security notes; 21 November 2026: RISC Zero's Groth16 advisories and the Sepolia vaults.`
