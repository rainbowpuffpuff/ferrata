# Rows file: fix-audit (the audit of `main` at `8c38d723`, its §1 applied)

Branch `fix-audit`, from public `main` at `8c38d723`. Unlike other streams, this branch edits the shared status files
directly, because its task is their consistency: `docs/requirements-status.md`, `docs/traceability.md`, the README's
status line, `docs/trust.md`, `docs/performance.md`, the consumer guide's P.1 table and the RISC Zero security note.
What it changed, by commit, is in the commit messages; this file keeps what waits.

## RFP-021 R.12: proposed text, not applied

Apply it only when both hold. First, every job of run 45075900693 (`ci.yml` at `8c38d723`) has passed. At 23:25 UTC
on 22 November 2026, 12 of 13 jobs had passed, `lez-harness` among them, its first completion on GitHub; `roundtrip`
was still running. Second, the `macos` workflow has passed on `main`. Its two runs of 22 November 2026 (45072883465
and 45075900706) were cancelled at start, with GitHub's annotation "The configuration 'macos-14-arm64-us-default' is
not supported": `runs-on: macos-14` (`.github/workflows/macos.yml:38`, `:74`) names a retired runner image. The fix is
on another branch.

If `main`'s tip has moved by then, cite the green runs of the tip instead: the clause asks that the default branch be
green.

| ID | Status | Why | Evidence |
|---|---|---|---|
| R.12 | shown | Every job of `ci.yml` passed on GitHub at `main` `8c38d723` (run 45075900693, 22 November 2026), the harness and updater suites (`lez-harness`) and the round trip (`roundtrip`) among them; the `macos` workflow passed at `<commit>` (run `<id>`) once its runner moved off the retired `macos-14` image. | GitHub runs 45075900693 and `<id>`; `.github/workflows/ci.yml`; `.github/workflows/macos.yml` |

## Not applied here (later pieces)

- RFP-022 R.11, an under-claim on the RFP's wording ("at least one corresponding test"). It needs a test for P.5 and
  P.6 (the documented figures recomputed from the committed records), piece 6 of the audit's plan. The row stays in
  part.
- RFP-021 F.12 returns to shown when branch `fix-updater-4` (`a90e424b`, `lez/updater/tests/redating.rs`) merges.
  Its rows file has the texts; the condition on LEZ's receipt check, added here, stays.
- RFP-022 U.9 and U.8 return to shown with the work named in their reasons: `set_endpoints` through Logos modules
  with a test inside Logos Core; the tool's two missing failure modes and the mini-app's five.
