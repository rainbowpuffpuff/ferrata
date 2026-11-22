# Records of the bridge program since milestone B1

The bridge program of `lez/guests/bridge` since milestone B1 (image
`0x847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515` since 22 November 2026, ADR 0032: a burn's tag is
used once, `burn(cid, r)`; before it `0xac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e`, whose
records these were until the suites below were run again on the rebuilt program, the same day,
[`docs/image-ids.md`](../../../docs/image-ids.md)):
every account keyed by the configuration (`nullifier(cid, chain, vault, N)`, `holder(cid, account)`), `Init` refusing a
freeze authority that is the admin, the admin's inbound finality depth (`SetDepth`, RFP-022 F.12) and the LEZ mirror
of the vault's delisting (`AttestListing`, F.10). Not deployed anywhere: no vault pins it.

The files beside this folder (`lez/results/bridge-*.md`, `lez/results/tenant/`, `lez/results/redteam-2026-11-18/`)
are the records of the program the live runs of November 2026 deployed (`0xccde6171…71dd`, source at commit
`f13faee2`), which the evidence index cites; they are kept as they were. This folder holds the same suites run
against the program since B1, and the suites B1 added:

| File | Suite (`lez/harness/tests/`) | RFP-022 |
|---|---|---|
| `bridge-depth.md` | `bridge_depth.rs` | F.12 (the inbound depth) |
| `bridge-delist.md` | `bridge_delist.rs` | F.10, R.4 (delisting on LEZ) |
| `bridge-refusals.md` | `bridge_refusals.rs` | Priv.5 (refused, repeated and capped claims) |
| `bridge-burn-tag.md` | `bridge_burn_tag.rs` | R.2, R.3, R.4, R.7 (a burn's tag used once, ADR 0032) |
| `tenant/` | `tenant.rs` | F.16 (the malicious-tenant attacks and the separation matrix) |
| `redteam-2026-11-18/` | `redteam_bridge.rs` | the blind red team's bridge vectors, re-run |
| `captured-committee-exposure.md` | `captured_committee_exposure.rs` | RFP-021 VS.4 (a captured committee's take, bounded by the caps and the freeze) |
| `interrupted-operations-claim.md`, `interrupted-operations-burn.md` | `interrupted_operations.rs` | R.7 (dropped proofs consume nothing) |
| `privacy/` | `privacy_suite.rs` with `scripts/privacy/run.sh` | the privacy suite (F.2, F.8, Privacy #1–#7) |
| `bridge-burn.md`, `bridge-controls.md`, `bridge-gloas.md`, `bridge-inbound.md`, `bridge-private-claim.md`, `bridge-roles.md`, `bridge-time.md`, `bridge-transfer-private.md`, `bridge-vault-storage.md` | the suites of the same names | as their records beside this folder |

Where: LEZ v0.4.0-rc1's own state machine and a copy of its fee settlement (the in-process harness), every transaction
in a block of its own, RISC Zero dev mode; the build box. Re-run (in `lez/`, with `RISC0_HOME` the LEZ RISC Zero home):

```sh
FERRATA_LEZ_RESULTS_DIR="$PWD/results/b1" RISC0_DEV_MODE=1 cargo test -p harness --locked \
  --test tenant --test bridge_depth --test bridge_delist --test bridge_refusals --test bridge_inbound \
  --test bridge_controls --test bridge_burn --test bridge_private --test bridge_roles --test bridge_secfix \
  --test bridge_time --test bridge_vault_storage --test redteam_bridge --test bridge_burn_tag \
  --test captured_committee_exposure --test interrupted_operations
scripts/privacy/run.sh lez/results/b1/privacy        # from the repository root
```

Without `FERRATA_LEZ_RESULTS_DIR` the suites write beside this folder, over the records of the deployed program.

## What changed against the deployed program's records

Every outcome is the same, except where B1 changed the behaviour on purpose (below). The figures moved with the
program's code: over the 267 transactions the two programs' `bridge-*.md` records share, execution gas changed by
−5,272 to +4,718 a transaction (derived from the committed records: the largest fall `bridge-inbound.md` row 32, a
claim of vault B's deposit, 267,044 → 261,772; the largest rise `bridge-vault-storage.md` row 23, a claim of leaf 5,
296,766 → 301,484), the keyed accounts changing what a claim, a mint and a burn derive and check.

- **Since ADR 0032 (22 November 2026)** the burn record is `burn(cid, r)`: every burn step hashes no note any more
  (execution gas against the deployed program's records now −9,799 to +4,718 a transaction, the largest fall
  `bridge-burn.md` row 1, the payer's burn of 1 ETH, 166,477 → 156,678; 165,329 on B1's first build), a second note
  with a used tag is refused (244, `bridge-burn-tag.md`), and the red team's BR-BRN-05 is MISMATCH by design: it
  expects D13's "a copied tag cannot make the owner's burn revert", which ADR 0032 withdraws (the copy applied first
  makes the owner's burn revert, and the owner's holding stays whole). Every other verdict and outcome below is as it
  was on B1's first build; the configuration ids, which commit to the program's image id, and the burn records'
  accounts moved with it.
- **The red team's bridge vectors** (118; `redteam-2026-11-18/`, against `lez/results/redteam-2026-11-18/`): 113 PASS,
  3 MISMATCH, 2 HARNESS-ERROR on both programs (B1's first build), with two verdicts swapped; 112 PASS and 4 MISMATCH
  since ADR 0032 (BR-BRN-05, above). BR-INIT-05 is now PASS: `Init` refuses a
  freeze authority that is the admin (240; the red team's F1). BR-TEN-04 is now MISMATCH, by design: the vector expects
  a deposit that a second configuration of the same vault claimed first to be refused (250) under the paired
  configuration, that is one nullifier namespace per vault. B1 removes exactly that: each configuration spends its own
  `nullifier(cid, chain, vault, N)`, so a second configuration can no longer block the paired one's claim (R1, F.16's
  "as if each were a fully separate deployment"); what the second configuration mints is its own token, which no vault
  releases (a release binds the vault's `lezConfigId`). Unchanged: BR-CLM-29 (the vector expects no state change at
  `config(A)`, where the claim's value is admitted to the global bucket), BR-PRV-01 (a private claim's `Issue` names
  its fee schedule range, `from_leaf` 6 to 8 in this world, which holds the leaf index's value; the client counts the
  range in the claim's anonymity set, `cli/wallet/src/claim.rs`), BR-TEN-01/02 (HARNESS-ERROR: their setup's `Register`
  of a configuration of vault `0xe5…e5` fails with exit 101 after 131,032 cycles, a panic, on both programs; `tenant.rs`'s separation matrix
  sends both vectors' messages, another configuration's `SetCap` and `Freeze` naming A, and both are refused, 253).
- **`tenant/`**: every attack of `lez/results/tenant.md` fails (`r1.txt`, `r1-frontrun.txt`, `r2.txt`), and
  `separation.txt` holds the separation matrix (new).
- **`privacy/`** (the privacy suite, `scripts/privacy/run.sh`; against `docs/privacy/generated/`): an adversary
  reading every public byte of 20 users' round trips pairs at chance (5.0 %) in every private game, with candidate sets
  of 20, as with the deployed program; the informational claim-to-burn game is 5.0 % (4.5 % with the deployed program,
  where two of its candidate sets had one member); recovery from the seed alone finds 10 of 10 unclaimed deposits and 10
  of 10 unreleased burns.
