# Rows file: fix-audit (the audit of `main` at `8c38d723`, its §1 applied, then its review's findings)

Branch `fix-audit`, from public `main` at `8c38d723`. Unlike other streams, this branch edits the shared status files
directly, because its task is their consistency. The files are `docs/requirements-status.md`, `docs/traceability.md`,
the README's status line, `docs/EVIDENCE.md` (row 223's wording), `docs/trust.md`, `docs/performance.md`, the
consumer, user and operator guides, `lez/ATTEST-CLI.md` and the RISC Zero security note. What it changed, by commit, is
in the commit messages. This file keeps what waits.

## RFP-021 R.12: proposed text, not applied

Apply it when the `macos` workflow has passed on `main`. Its two runs of 22 November 2026 (45072883465 and 45075900706)
were cancelled at start, with GitHub's annotation "The configuration 'macos-14-arm64-us-default' is not supported":
`runs-on: macos-14` (`.github/workflows/macos.yml:38`, `:74`) names a retired runner image. The fix is on another
branch.

Run 45075900693 (`ci.yml` at `8c38d723`) passed 13 of 13 jobs only on its second attempt. On the first attempt the
`roundtrip` job hung for 73 minutes in `cli/tests/e2e.rs`, in its `forge script` call, and was cancelled. The second
attempt reran that job alone, and it passed in the usual time; the cause is not established. A text that cites the run
must say so. If `main`'s tip has moved by then, cite the green runs of the tip instead: the clause asks that the default
branch be green.

| ID | Status | Why | Evidence |
|---|---|---|---|
| R.12 | shown | Every job of `ci.yml` passed on GitHub at `main` `8c38d723` (run 45075900693, 22 November 2026), the harness and updater suites (`lez-harness`) and the round trip (`roundtrip`) among them. The round trip passed on the run's second attempt: on the first, its `forge script` call in `cli/tests/e2e.rs` hung for 73 minutes and the job was cancelled; that one job was rerun alone, cause not established. The `macos` workflow passed at `<commit>` (run `<id>`) once its runner moved off the retired `macos-14` image. | GitHub runs 45075900693 (attempt 2) and `<id>`; `.github/workflows/ci.yml`; `.github/workflows/macos.yml` |

## Not applied here (later pieces)

- RFP-022 R.11, an under-claim on the RFP's wording ("at least one corresponding test"). It needs a test for P.5 and
  P.6 (the documented figures recomputed from the committed records), piece 6 of the audit's plan. The row stays in
  part.
- Rows that return to shown with the work their reasons name ("To be shown"):
  - RFP-021 F.12, when the updater's fix under review merges, with the texts of its own rows file. The condition on
    LEZ's receipt check, added here, stays.
  - RFP-021 R.5 (the attestation CLI under a failing Ethereum endpoint) and P.9 (the listed actions measured at two
    state sizes).
  - RFP-022 U.9 (`set_endpoints` through Logos modules, with a test inside Logos Core), U.8 (the tool's verification and
    finality messages, and the mini-app's five modes) and Priv.7 (the fee payer's mitigation with no user action, as
    U.5).
- `lez/updater/README.md:444` still names an internal review stream by its working name. The public status documents
  now say "an internal review's finding M1, 22 November 2026". That file is the updater stream's to change.
