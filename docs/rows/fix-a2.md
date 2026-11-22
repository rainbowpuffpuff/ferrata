# Stream fix-a2: proposed changes for the integrator

Branch `fix-a2` (from `462d0cd1`). It answers the second critical review of stream A (light-client security),
`review-rfp-A2`: M1, M2, L1–L7 and N1, and then the review of this branch, `review-fix-a2`: M1–M3, L1–L6, N1 and N2
(last section). No program, guest or image id changed. This stream owns no new requirement row;
it proposes new wording for five RFP-021 rows, one new EVIDENCE row and edits to two EVIDENCE rows and three
traceability rows. **No count changes:** R.11 stays shown, now on a sequencer; RFP-021 hard stays 25 shown, 29 in part,
2 not yet, 3 waits on Logos.

## Requirement rows (proposed), `docs/requirements-status.md`

| Row | Now | Proposed | Reason to paste | Evidence to paste |
|---|---|---|---|---|
| R.11 | shown | **shown** | Several private reads in one block of LEZ v0.4.0-rc3's own sequencer (standalone, its mock Bedrock; sequencer and wallet in dev mode): four private attestations by four accounts proven against one snapshot and held; a private transaction moved the commitment tree and another instance verified a finality update before any was sent; then the four, sent back to back, applied in one block (896,252 of LEZ's 1,000,000 storage gas: four is the most a block takes), nothing re-proven, the record unchanged and every commitment they computed before inclusion in the tree; one account's second transaction from the same snapshot dropped. In process: the same with a later finality update of the instance read. | 152, 155; `lez/results/v0.4.0-rc3/r11-one-block/` |
| VS.1 | shown | shown (L3) | … by the reference consumer (213, 110); the instance's store unchanged, no record at the fabricated slot, nothing credited (the refused job's `Open` and `Miller` leave its job account). | unchanged |
| R.6 | in part | in part (L6, L1) | BLS is audited `blst`; the light-client logic is Ferrata's own transcription, checked against the official vectors, Helios (the vectors, 220 recorded updates, 2,000 random mutations of real updates) and Lodestar (the same mutations; its BLS is herumi's WebAssembly, not `blst`), 0 disagreements; the mutations judge the native stores, not the program's step path, and only update bytes; not audited. | unchanged |
| U.5 | shown | shown (L4) | … each code's name and message from the IDL the CLI prints. A checkpoint stale by the sequencer's clock is refused by LEZ with its own text ("Execution outside of the validity window", which the CLI prints as it is) before the program runs, on the window the program declares; the 53 message applies when the update is signed after the end. Open, as before this branch: that text names neither the checkpoint nor the remedy (a new bootstrap), and the CLI does not map it to code 53's message. | unchanged |
| F.8 | shown | shown (L5; branch review M2) | The full-diff test: two private claims (…), byte-identical once only fresh randomness, the account's own nullifier and two commitments (set by the private state spent and made) and the claim's nullifier are taken from one; the same claim proven twice from one state shows which is which; events, … (rest unchanged). | unchanged |

### R.11, clause by clause

| Clause (RFP-021 Reliability 11) | Shown by | Command | Commit |
|---|---|---|---|
| "multiple private transactions can submit statements in the same block" | the session's block 47: four `AttestPrivately` of four accounts, positions 0–3, each applied (`run.tsv` rows "reader 0..3"; `chain.tsv` block 47: 4 user transactions, 1,688,128 execution gas, 896,252 storage gas); `r11-one-block.json` `block` | `lez/live/run.sh run` with the variables in `lez/results/v0.4.0-rc3/r11-one-block/README.md` ("What ran"); re-check, from a fresh checkout (which selects rc1): `scripts/fetch-upstream.sh`, `lez/scripts/lez-select.sh v0.4.0-rc3`, then `LEZ_TAG=v0.4.0-rc3 FERRATA_VERIFY_DEV_MODE=1 lez/live/verify-run.sh --dir lez/results/v0.4.0-rc3/r11-one-block <ferrata_lez.bin> <scripted.bin> <ferrata_consumer.bin>` (51 blocks replayed by rc3's `chain_state`, 47 rows equal LEZ's settlement) | record `23aebcf1` (folder names replaced `cf5a3db8`), phase `dde19c87`, replay flag `fc1b46c3` |
| "statement submissions are independent of one another" | the same block: four statements about four logs, no conflict between them; the control (`control-0`, `control-1`: one account, one snapshot) is the one dependency, the same private state spent twice: the second dropped, "Commitment … already seen" | as above | as above |
| "a private transaction can compute its resulting state before inclusion" | proven after block 40 and held; included in block 47 after the commitment tree moved (block 42, root `ba64320a…` → `02d14c57…`) and instance E verified a finality update (blocks 43–45); the 28 commitments the four computed before inclusion are in the sequencer's tree after it; in process, a later update of the instance read itself (`security_privacy::r11_several_private_reads_in_one_block`) | as above; `cd lez && export RISC0_HOME=$PWD/../../risc0-home-lez && RISC0_DEV_MODE=1 cargo test -p harness --locked --test security_privacy r11` | `2a9a4fae` (results), `4e3410bb` (test text) |

## `docs/traceability.md` (proposed)

- **Line 78 (U.5), L7:** "program 200–214, consumer 220–229" → "program 200–217, consumer 218–229" (`lez/wire/src/lib.rs`
  `CODE_BLOCKS`).
- **Line 90 (R.6), L7 and L6:** "Helios on 26 cases (100 steps)" → "Helios on 33 of the 52 official cases (129
  steps)"; in the rationale, "has no Gloas layout at 0.9.4 and departs from the specification on four inputs" → "has no
  Gloas layout at 0.9.4 and departed from the specification on four inputs there (two at 0.12.0: #871, #872)"; at the
  end of the evidence, add: "Lodestar's BLS is herumi's WebAssembly through `@chainsafe/bls`, not `blst`; the mutations
  judge the native stores and update bytes only, not the program's step path (`HELIOS-DIFFERENTIAL.md`, "What this
  differential cannot give")."
- **Line 95 (R.11), M2:** the evidence column, to paste as is (the link is written for `docs/traceability.md`):

```markdown
[lez] mechanism read from the LEZ source (settlement re-runs the reply and refuses on mismatch). Several private statements in one block of LEZ v0.4.0-rc3's own sequencer ([EVIDENCE](EVIDENCE.md) row 155, `lez/results/v0.4.0-rc3/r11-one-block/`): four private attestations by four accounts proven against one snapshot and held; a private transaction moved the commitment tree and another instance verified a finality update; then the four, sent back to back, applied in one block, nothing re-proven, the record unchanged, every commitment computed before inclusion in the tree; one account's second transaction from the same snapshot dropped ("Commitment … already seen"); standalone sequencer (mock Bedrock), dev-mode receipts. In process (row 152, `lez/results/lc-security/r11-one-block.md`): the same with a later finality update of the instance read, in the test's own block loop around the harness's copy of rc1's per-transaction settlement.
```

  and the test column: "`harness/tests/security_privacy.rs`; `live/driver/src/block.rs` (phase `attest-block`)".

## `docs/EVIDENCE.md` (proposed)

- **Row 152, M2:** replace "the block is assembled by the test with rc1's block settlement, not by a sequencer" with
  "the block is the test's own loop around the harness's copy of rc1's per-transaction settlement (rc1's block function
  and the sequencer's admission screen are not run); the same on rc3's own sequencer: row 155". **Branch review M2:**
  replace "byte-identical once fresh randomness and the claim's nullifier are taken from one" with "byte-identical once
  only fresh randomness, the account's own nullifier and two commitments (set by the private state spent and made) and
  the claim's nullifier are taken from one". **L5:** after "(12 groups, each of equal size in both)" add "; X's claim
  proven twice from one state differs in every alias (5), note (8), recovery binding (1) and dummy action (6), and
  carries the account's own nullifier and two commitments unchanged".
- **Row 151, L3:** "the instance's store unchanged and nothing credited" already says it right; no change.
- **Header, "Words used for where it ran":** add "**standalone rc3 sequencer** (LEZ's v0.4.0-rc3 sequencer with its
  mock Bedrock, no Logos Blockchain node)".
- **New row** (number: the next free one, 155 at `462d0cd1`; streams run in parallel, so if 155 is taken by then,
  give it the next free number and change "155" in the R.11 row's evidence above, in the row 152 text and in the
  traceability text to match), to paste as is (the link is written for `docs/EVIDENCE.md`):

```markdown
| 155 | **Several private reads in one block of LEZ v0.4.0-rc3's own sequencer (RFP-021 R.11)**: the reference consumer's private attestation by four accounts, proven against one snapshot, applied together in one block after the chain moved (tests and records only; no program, guest or image id changed) | four new private accounts (funded by native transfers, blocks 37–40) each prove `AttestPrivately` of another `Transfer` of mainnet block 26,384,851 (instance S's record) against commitment root `ba64320a…` in 5.6 s with two transactions of a control, all held; the control's first applied (block 42, the root moves) and instance E verifies a finality update (blocks 43–45); the four, sent within 2 ms after block 46, applied in **block 47** (positions 0–3, each 422,032 execution gas, 224,063 storage gas, base fee 5,168,760; the block 896,252 of LEZ's 1,000,000 storage gas), the record's account unchanged, the 28 commitments computed before inclusion all in the tree; the control's second (same account, same snapshot) dropped, "Commitment … already seen"; 48 transactions, 47 applied, 51 blocks 6.0 s apart; `verify-run.sh` replays the 51 blocks through rc3's `chain_state`, 47 rows equal LEZ's settlement | standalone rc3 sequencer (its mock Bedrock), sequencer and wallet with `RISC0_DEV_MODE=1`, on the build box (22 Nov 2026) | none (dev mode: fake receipts, not verified by the sequencer; the same transaction kind proven for real on rc3: row 123) | from a fresh checkout: `scripts/fetch-upstream.sh && lez/scripts/lez-select.sh v0.4.0-rc3 && LEZ_TAG=v0.4.0-rc3 FERRATA_VERIFY_DEV_MODE=1 lez/live/verify-run.sh --dir lez/results/v0.4.0-rc3/r11-one-block <the three binaries>` | [`lez/results/v0.4.0-rc3/r11-one-block/`](../lez/results/v0.4.0-rc3/r11-one-block/README.md) |
```

## Other findings, already fixed on the branch (no shared-file change needed)

| Finding | Fixed in | Commit |
|---|---|---|
| M1: "the specification's store" in the tests and four results files | the tests' docs and output texts name Ferrata's policy store (`FinalizedStore`); the specification's store (`LightClientStore`) is now run on cases 50 and 51 and asserted to refuse neither and finalize neither (342 of 512: it finalizes); results regenerated | `4e3410bb`, `cb76b66d` |
| M2: "rc1's block settlement" | the test's text and `r11-one-block.md`: the test's own block loop around the harness's copy of rc1's per-transaction settlement; `docs/design/attestation.md` §6.4 | `4e3410bb`, `cb76b66d`, `e0f529a4` |
| L1: Lodestar's BLS | `HELIOS-DIFFERENTIAL.md`, measured after `npm ci` (`results/lodestar-bls-2026-11-22.txt`) | `522b92df`, `6eeed010` |
| L2: §6 → §8 | `lez/results/lc-security/README.md` | `b928cf0b` |
| L4: U.5's column and dry-run sentence | `security_lc.rs`, `u5-failure-modes.md` (the optional mapping of the validity-window revert in `ferrata-attest` is not done) | `4e3410bb`, `cb76b66d` |
| L5: F.8's alignment | `align_boundary` takes inherited authorizations only where none names a public account; `fresh_check` (the same claim twice from one state) | `4e3410bb`, `2a9a4fae` |
| L6: the differential's limits | `HELIOS-DIFFERENTIAL.md` ("What this differential cannot give") | `f729a9e4` |
| N1: proof size in dev mode | `f8-full-diff.md`'s text | `4e3410bb`, `2a9a4fae` |
