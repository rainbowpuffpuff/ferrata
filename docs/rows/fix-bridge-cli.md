# Stream fix-bridge-cli: the lost-burn defect (H1, H-1) and the tool's M-1 and M-2, proposed rows

For the integrator of `docs/requirements-status.md`, `docs/traceability.md` and `docs/EVIDENCE.md`. Branch
`fix-bridge-cli` from the integration tip `0ddb5168`: `bd2072e6` (the program), `ac32c821` (the tool's journal),
`64207d29` (M-1, M-2, the end-to-end scenarios), `c9894c39` (the new image id, its consumers, the regenerated records
and tables, the probes), and the commit that adds this file; after the branch's review (M1-M4, L1-L8): `0d31127b`,
`c8d4353d`, `b28969a8`, `ec7b778e`, `73283e13`, `55c0139e` and the commit that updates this file. Nothing was pushed. Box runs: the Modal build box
(Linux x86-64), RISC Zero dev mode, local anvil (chain id 1) where named; no public network was used. Links here are
relative to this file (`docs/rows/`): in `docs/EVIDENCE.md` or `docs/traceability.md` drop one `../`.

## The defect, the decision, the fix

Two notes made with one burn counter `i` share the tag `r_i`, hence `sOut` and `NOut = H("ferrata/nf-out/v2" ‖ sOut)`.
The bridge program recorded a burn at `burn(cid, H(note))` (D13), so it applied both, and the vault releases only one:
the other denomination is destroyed on LEZ and stays in escrow. The `ferrata` tool made such pairs: a second burn
planned while a first was in flight from another account of the seed took the same counter (the bridge code review's
H-1: the counter came from the applied burns only), and after an interrupted burn the next burn took the interrupted
burn's counter whatever its recipient, token or amount (the final review's H1). EVIDENCE 204, traceability R.7 and
requirements-status R.7 said "at most one of the two can be applied", which was false; the bridge core's test
`attack_repeated_burn_tag_is_refused_by_the_registry` tested a helper the program never called.

Reproduced on `0ddb5168` (the build box): the bridge code review's probe P1 applies both same-tag notes, 165,329
execution gas each, 10 → 8 ETH, one `NOut` `0x3922b646…098f`; through the tool's library (probe H1) two burns planned
while neither is applied take counter 0 both, both applied (164,907 gas each), one `NOut`, and after one release the
scan shows both released. Both probes and their outputs: `docs/reviews/lost-burn-2026-11-22/`.

Fix ([ADR 0032](../decisions/0032-burn-tag-used-once.md), spec D14): the program records a burn at `burn(cid, r)`, so
a second note with a used tag is refused (244) and its whole transaction reverts; the tool reserves each counter in a
journal before proving and never gives it to another burn (`cli/src/burns.rs`). The cost, stated in ADR 0032, D14,
`docs/design/bridge.md` and the user guide: a party that sees a pending burn (on LEZ v0.4, the sequencer) can make it
revert by burning a denomination of its own under the same tag; the owner loses the refused transaction's fee, its
denomination whole, and the copier can release its copy, so the attack costs it fees only (ADR 0032 as corrected after
the branch's review, M4; the bridge core's test `a_tag_copier_can_release_its_own_copy`). The program's image id moves
(B1, deployed nowhere): `0xac1fb59b…442e` → `0x847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515`;
it ships with the RISC Zero 3.1 rebuild (phase 2), which moves every LEZ guest again. The live deployments keep the
program `0xccde6171…71dd`, which has the defect (with the tool of their day): with the vaults' deposit and release caps
at zero and keyless retirement (the advisory's owner steps) nothing new is deposited or released there; holders on the
live LEZ programs can still burn with the old tool.

M-1 (the bridge code review): a burn bound the Ethereum RPC's chain id into `COut` without reading `config(cid)`; an
RPC of another chain (a local anvil's 31337, a fork started with another `--chain-id`; an anvil fork reports the forked
chain's id unless told otherwise) made a burn no release can open. `lez::check_pairing` now refuses unless `config(cid)` is paired
and names this chain and vault (`flow::burn`, `flow::claim`). M-2: `claim`'s dry run asked the sequencer for the
deposit's nullifier account, naming the deposit to the operator before the claim and defeating the claim delay;
"spent" now comes from the downloaded history alone.

## The branch review's findings (22 November 2026), fixed

| Finding | Fix | Commit | Shown by |
|---|---|---|---|
| M1: after 20 refused burns the next applied burn was not found | the scan counts refused tags as used; `ferrata release` and `recover` search past the journal's counters | `c8d4353d`, texts `ec7b778e` | `scan::tests::burns_found_past_a_run_of_refused_counters`, `scan::tests::burns_found_past_counters_the_journal_reserved`, `flow::tests::burns_searched_past_the_journal_counters` |
| M2: the library did not compile without `lez-wallet` | `ensure` imported for `check_pairing` | `0d31127b` | `cd cli && cargo +1.98.1 check --locked --no-default-features --lib` exit 0; `scripts/ci/modules-resolve.sh` 5 of 5 |
| M3: 3 broken links in this file | links relative to `docs/rows/` | this file's commit | CI's `links` command: 0 broken |
| M4: ADR 0032 said a tag copier loses its denomination | ADR, spec D14, design, user guide corrected | `73283e13` | `a_tag_copier_can_release_its_own_copy` (bridge core) |
| L1: recording a send failed after another run resolved the counter | a note, never the command's error | `b28969a8` | `burns::tests::burns_sent_after_another_run_resolved_the_counter` |
| L2: "a fork reports 31337" | an anvil fork reports the forked chain's id (measured: anvil 1.5.1 forking a chain-5 anvil reports 5, with `--chain-id 31337` 31337) | `ec7b778e`, `55c0139e`, this file | (texts) |
| L3: "the owner loses nothing" | the refused transaction's fee | `73283e13`, this file | (texts) |
| L4: option 2's memo contrast | under option 3 the refused note is public too | `73283e13` | (text) |
| L5: stale texts on this tree's program | B1's image `847a4336…7515` named; refused private calls; the journal's readers | `ec7b778e`, `73283e13` | (texts) |
| L6: NEW-D's "byte for byte" and its command | the time line named; the command as it runs | this file | (text) |
| L7: the M-2 e2e could pass vacuously | the dry run must reach the claim's plan | `55c0139e` | `cli/tests/e2e.sh` 1 passed (47.96 s) |
| L8: "nothing new is burnt there" | "nothing new is deposited or released there" | this file | (text) |

## RFP-022

| ID | Proposed | Clause by clause | Evidence (path; command; where, commit) |
|---|---|---|---|
| R.7 | **in part** (unchanged; text corrected) | *An interrupted user-side operation does not consume, corrupt, or expose the user's entitlement.* A claim killed after its proof, a burn killed after sending, gas drops across kills: shown on a running sequencer (row 204). The burn interrupted and then asked with other parameters: rows 204's runs showed the tool reuse the counter, which was unsafe (the trapped transaction was never forwarded, so the runs could not fail on it); now the tool keeps the interrupted burn's counter and takes a fresh one, the killed burn asked again once applied burns nothing, the same burn asked again while unseen resumes its counter with one of the two applied (CI's e2e job, row NEW-C), and the program refuses a second note with a used tag (CI's `lez-harness`, row NEW-B). Short: the deposit and release legs not interrupted; the Basecamp module not killed between a proof and its sending; the robust hook's new case h (a burn forwarded late, then another with a fresh counter) not re-run on a running sequencer. | 126, 204 (corrected), 216, NEW-A, NEW-B, NEW-C |
| R.2 | shown (unchanged; text and evidence) | *Burn and release each atomic; entitlement survives failures.* A refused burn burns nothing; a second note with a used tag is refused (244) and the holder keeps its denomination, public or private, so no two applied burns share a release nullifier (row NEW-B). | 19, 85, 102, NEW-B; `lez/harness/tests/bridge_burn_tag.rs` |
| R.3 | shown (unchanged; text) | Second claim (250), a repeated burn note and any other note with its tag (244), a replayed release refused. | 8, 46, 76, NEW-B |
| R.4 | in part (unchanged; text) | Add: a later burn that reuses a tag can no longer make an earlier burn unreleasable (on the program and tool of `0ddb5168` it could: row NEW-A). | 8, 28, 191, NEW-A, NEW-B |
| U.8 | shown (unchanged; evidence added) | Add: an Ethereum RPC of another chain than the zone's configuration (a local anvil's 31337, a fork started with another `--chain-id`) is refused before anything is planned or sent, naming both chains (M-1; row NEW-C). | 62, 85, 202, NEW-C |
| Priv.4 | shown (unchanged; evidence added) | Add: the tool's claim, dry run or held by its delay, asks the zone for no deposit's nullifier account (M-2; row NEW-C), as `docs/trust.md` §4.2's table says ("recovery downloads whole lists, never per item"). | 23, 89, NEW-C |
| Sec.6 | in part (unchanged) | Both halves randomised again on the rebuilt program (row NEW-D). | 12, 42, 154, 184, NEW-D |
| R.5 | shown (unchanged; evidence added) | *Restored client with no local state finds all positions.* Add: burns after a run of burns refused on the zone (each uses its counter) are found, a refused tag counting as used in the scan (the branch review's M1, a regression this branch had opened and closed; row NEW-E). | 23, 84, 89, 102, NEW-E |

## `docs/traceability.md`

- **R.2 (RFP-022), "How"**: replace "the burn account is keyed on the hash of the whole note, so a copied tag cannot
  make it revert" by "the burn account is keyed on the note's tag, `burn(cid, r)`, so two notes with one tag (one
  release nullifier) are never both applied (ADR 0032; a copy of a pending burn's tag can make that burn revert, its
  owner losing the refused transaction's fee, its denomination whole)". **Evidence**: after "the same note again is
  refused (244)" add "and so is any other note with its tag, public or private, the holder keeping its denomination
  ([EVIDENCE](../EVIDENCE.md) row NEW-B)". **Tests**: add
  `harness/tests/bridge_burn_tag.rs`.
- **R.7 (RFP-022), "Evidence"**: replace "a burn killed before it reached the zone, then asked with another recipient
  (it takes the interrupted burn's counter, so at most one of the two can be applied; applied once)" by "a burn killed
  before it reached the zone, then asked with another recipient, took the interrupted burn's counter and was applied
  once; the trapped transaction was never forwarded, and had it been, both would have been applied with one release
  nullifier (the final review's H1, reproduced in row NEW-A)", and add after it: "Since 22 November 2026 the tool
  reserves each burn counter in a journal before proving and never gives it to another burn, and the program refuses a
  second note with a used tag (rows NEW-B, NEW-C, in CI): a burn killed with its transaction at the sequencer keeps its
  counter and the next different burn takes a fresh one, both applied; the killed burn asked again once applied burns
  nothing; the same burn asked again while unseen resumes its counter and one of the two is applied." In "In part" add:
  "the robust hook's new case h (`e2e/seqnet/hook-robust.sh`) is not re-run on a running sequencer". **Tests**: add
  `cli/tests/e2e.rs` (`counters_pairing_and_quiet_dry_runs`), `cli/src/burns.rs`, `lez/harness/tests/bridge_burn_tag.rs`.
- **R.5 (RFP-022), "How"**: after "burn tags regenerated from the seed" add "; a tag in a refused burn counts as used,
  so a run of refused burns does not end the scan". **Evidence**: add "Burns after 20 to 60 burns refused on the zone
  found by the tool's recovery (row NEW-E)". **Tests**: add `cli/src/scan.rs` (`burns_found_past_a_run_of_refused_counters`).
- **F.16 (RFP-022)**: the image named there, `0xac1fb59b…`, is B1's first build; this tree's is `0x847a4336…7515`
  (`lez/results/b1/` re-run on it; the separation matrix's outcomes unchanged).

## `docs/requirements-status.md`

- RFP-022 **R.7**: "A claim killed after its proof, a burn killed after sending, gas drops across kills: nothing lost
  or spent twice on a running sequencer; dropped proofs consume nothing (in CI); burn counters journalled before
  proving, so a burn after an interrupted one or beside one in flight never shares a counter, and the program refuses a
  second note with a used tag (in CI); a Basecamp view closed mid-proof resumed once. Not shown: the deposit and release
  legs interrupted; the tool's new interrupted-burn case on a running sequencer." Rows: 126, 204, 216, NEW-B, NEW-C.
- RFP-022 **R.2**: append "; a second note with a used tag refused, so no two burns share a release nullifier (in CI)";
  rows add NEW-B. **R.3**: "repeated burn note" → "repeated burn tag (the same note or another)"; rows add NEW-B.
  **R.4**: append "; a reused burn tag no longer invalidates an earlier burn"; rows add NEW-B.

## `docs/EVIDENCE.md`

**Row 204, corrected** (the runs are records of the tool and hook of their day; their counts stand):

> | 204 | **Interrupted user commands on LEZ v0.4.0-rc1's running sequencer, the `ferrata` tool of these runs (RFP-022
> R.7)**: a claim killed after its proof with its send trapped (nothing reached the zone; still claimable; then claimed
> once); a burn killed after its send (applied once; the same command burns nothing more); gas drops across kills of
> the claim and of the relayer's daemon (each claim claims exactly one credit). A burn killed before it reached the
> zone, then asked with another recipient, took the interrupted burn's counter and was applied once: the trapped
> transaction was never forwarded, so these runs could not show that both notes would have been applied with one release
> nullifier (the final review's H1; row NEW-A). Since 22 November 2026 the tool never gives a counter to two burns and
> the program refuses a second note with a used tag (rows NEW-B, NEW-C); the hook's case h now forwards the interrupted
> burn late and expects a fresh counter (not re-run) | robust-1 14 of 14; robust-2 21 of 21; robust-3 22 of 22;
> robust-4 22 of 22 | (unchanged) | (unchanged) | `bash e2e/seqnet/run.sh robust NAME` with the hook of `0ddb5168` |
> (unchanged) |

**Proposed new rows** (numbers for the integrator to assign):

- **NEW-A. The lost-burn defect reproduced on `0ddb5168` (RFP-022 R.2, R.4, R.7; the final review's H1, the bridge code
  review's H-1)**: the probe P1 in the harness: two notes made with one counter, to two recipients, both applied by the
  program `0xac1fb59b…442e`, 165,329 execution gas each, the payer 10 → 8 ETH, one `NOut` `0x3922b646…098f`; the probe
  H1 through the `ferrata` tool's library: two burns planned while neither is applied (public /0 and /1 of one seed)
  take counter 0 both, both applied (164,907 each), one `NOut`, both shown released after one release. | P1 1 passed;
  H1 1 passed | the Modal build box, dev mode | none | `docs/reviews/lost-burn-2026-11-22/README.md` (the tree of
  `0ddb5168`; each probe copied into a crate's `tests/`, run, deleted) | [`docs/reviews/lost-burn-2026-11-22/`](../reviews/lost-burn-2026-11-22/README.md) |
- **NEW-B. A burn's tag is used once (RFP-022 R.2, R.3, R.4, R.7; ADR 0032), in CI's `lez-harness`**: on the program
  `0x847a4336…7515` (`burn(cid, r)`): the probe P1's second note (another holder, another recipient) refused with 244,
  its holder's 1 ETH whole, the first holder debited once, `burn(cid, r)` holding the first note; the same tag with
  another amount, and the same note again, refused; a private burn with a used tag cannot be proven (its public part
  refused, 244), and proven before the tag was used it settles reverted (244 after 20,873 cycles), the fee paid and the
  holding whole, then burns with its next counter (515,479 execution gas, 224,063 storage); a copy of a pending tag
  applied first makes that burn revert, the owner keeping its holding (the refused transaction's fee paid) and burning
  with its next counter (ADR 0032's cost; the copier can release its copy: the bridge core's
  `a_tag_copier_can_release_its_own_copy`, row NEW-E); the wallet finds counters 0, 1, 2, 4 with four distinct `NOut` (the copy's MAC fails); the probe P1 unchanged
  now fails at its second burn (244); a public burn costs 156,678 execution gas (165,329 on `0xac1fb59b…`, 166,477 on the
  deployed `0xccde6171…`); the red team's BR-BRN-05 MISMATCH by design, every other verdict as before | 1 of 1 | the
  build box, dev mode | none | `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked --test bridge_burn_tag --
  --nocapture` | [`lez/harness/tests/bridge_burn_tag.rs`](../../lez/harness/tests/bridge_burn_tag.rs),
  [`lez/results/b1/bridge-burn-tag.md`](../../lez/results/b1/bridge-burn-tag.md) |
- **NEW-C. The `ferrata` tool never gives a burn counter to two burns, refuses an RPC of another chain, and asks no
  nullifier account (RFP-022 R.7, U.8, Priv.4; H1, H-1, M-1, M-2), in CI's `e2e` job**: the journal's unit tests (8
  since the branch review's L1: recording a send after another run resolved the counter is a note); on
  anvil and the harness zone through the tool's own functions and a ledger that settles later: two burns in flight from
  public /0 and /1 take counters 12 and 13, both applied; a burn killed with its transaction at the sequencer keeps 14
  and a burn to another recipient takes 15, both applied; the killed burn asked again once applied burns nothing; the
  same burn asked again while unseen resumes 16, one applied, one refused (244), one denomination burnt; a dry run
  leaves the journal byte-identical; a burn refused on the zone (254) uses counter 17; a recovery from the seed alone
  finds counters 12–16 and 18 with six distinct `NOut`, next 19; chain 31337 under a chain-1 configuration refused
  before anything is sent; a claim's dry run, with and without the delay, reaches the claim's plan (asserted since the
  branch review's L7) and reads no nullifier account | e2e 1 passed (37.34 s; again 47.96 s at `55c0139e`); lib 41
  passed (45 at `55c0139e`) | the build box | anvil (chain 1), dev mode, mock verifier | `cli/tests/e2e.sh`; `cd cli &&
  cargo test --locked --lib burns` | [`cli/src/burns.rs`](../../cli/src/burns.rs), [`cli/tests/e2e.rs`](../../cli/tests/e2e.rs) |
- **NEW-D. Both halves randomised on the rebuilt program (RFP-022 Sec.6)**: 8 seeds of 60 steps on the program
  `0x847a4336…7515`: 141 deposits, 44 imports, 81 claims, 59 burns, 25 releases, 93 forgeries refused, 496 invariant
  checks; `e2e/both-halves/results/randomised.md` regenerated: the committed record but its third line, which holds the
  images (the bridge program's moved; the checkpoint and release ids unchanged) and the run's time (279 s on this run,
  321 s before the branch; the branch's review measured 326 s): not byte for byte but its image, as commit `494876aa`
  says | 1 passed (575 s for the run script, build included) | the
  build box | anvil, dev mode, the tests' mock verifier | `env -u RISC0_HOME FERRATA_BOTH_HALVES_SEEDS=1,2,3,4,5,6,7,8
  FERRATA_BOTH_HALVES_STEPS=60 e2e/both-halves/run.sh randomised` (without `env -u RISC0_HOME` when LEZ's RISC Zero
  home is not set: under it the release and checkpoint guests build to other ids) | [`cli/tests/both_halves.rs`](../../cli/tests/both_halves.rs) |

- **NEW-E. The tool's recovery finds every burn past refused counters and past its journal's counters; a copied tag's
  release; the journal's send record (RFP-022 R.5, R.7, F.9; the branch review's M1, M4, L1), in CI (`--lib burns`,
  `ferrata-bridge-core`)**: through the tool's `burns::choose` and `scan::burns`: counter 0 applied, then 1, 19, 20,
  25 or 60 burns refused on the zone, then the counter the journal hands out next (2, 20, 21, 26, 61) applied:
  `scan::burns`, behind `ferrata release` and `ferrata recover`, finds both burns every time, no refused note counted
  as a burn, the next counter one past (on `494876aa` the branch review's probe found counter 0 only from 20 refused,
  and `ferrata release` said "nothing to release"); from a journal file in which this seed reserved counters 0 to 30
  (another seed's 90 beside), `scan::burns_past` finds a burn at counter 30 past 29 counters the zone does not show,
  which the seed alone cannot, and an unreadable journal is a note; `Store::sent` after another run resolved the
  counter is a note and the counter stays used; a burn under a pending burn's tag with the copier's own `COut`
  verifies for release, with its own `NOut` (ADR 0032's corrected cost) | CLI lib 45 passed (`--lib burns` 11 of
  11); bridge core outbound 11 of 11; `bridge_burn_tag` 1 passed, its record reproduced byte for byte | the build
  box | none (dev mode for `bridge_burn_tag`) | `cd cli && cargo test --locked --lib burns`; `cd bridge && cargo test
  -p ferrata-bridge-core --locked --test outbound`; `cd lez && RISC0_DEV_MODE=1 cargo test -p harness --locked --test
  bridge_burn_tag -- --nocapture` | [`cli/src/scan.rs`](../../cli/src/scan.rs),
  [`cli/src/flow.rs`](../../cli/src/flow.rs), [`cli/src/burns.rs`](../../cli/src/burns.rs),
  [`bridge/crates/ferrata-bridge-core/tests/outbound.rs`](../../bridge/crates/ferrata-bridge-core/tests/outbound.rs) |

**Rows whose figures are of B1's first build** (records, true as such; the integrator may note the rebuild): 183
(the program `0xac1fb59b…`), 213 and 23 (the privacy tables, now regenerated on `0x847a4336…`: same verdicts), 215
(`burn(cid, H(note))` 185 B: the record's size is unchanged, its key is now `r`), 154 (both halves; NEW-D;
`e2e/both-halves/results/randomised-cap-20eth.md` stays a record of `0xac1fb59b…`, not re-run), and any row quoting
B1's burn gas.

## Not done here (next pieces)

- The robust hook's case h on a running sequencer (a seqnet run, `bash e2e/seqnet/run.sh robust NAME`).
- The Logos bridge module's `burn` takes its counter from its caller: with the program's refusal a reuse costs a fee,
  not a denomination, but the module should reserve counters as the tool does.
- The Logos bridge module's recovery (`recover`, `release`) takes the zone's applied burns from its chain service
  (`module.inputs`), which lists no refused burn: the module never leaves a run of refused counters of its own (its
  counter follows the applied burns), but a seed whose `ferrata` tool skipped 20 or more refused counters is recovered
  in full by the tool only. Next piece: the chain service lists the refused burn notes and the module passes them to
  `recover_burns` as used tags, as `cli/src/scan.rs` does.
- The relayer's M-5/M-6, the attestor's H-2/M-3/M-4 and the Lows of the bridge code review are other streams'.
- `--chain-id` mandatory with `--send` (M-1's second half) is not done: the configuration check covers the loss.
