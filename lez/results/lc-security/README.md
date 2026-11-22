# Light-client verification security through the attestation program (RFP-021)

Each file is written by a test of `lez/harness/tests/security_lc.rs` or `security_privacy.rs` (the shared code is
`tests/security_common/`): every case is sent as the transactions a submitter or a consumer sends, settled by LEZ
v0.4.0-rc1's own state machine and fee settlement in process (the harness), and judged by the exit code the program
returns or by LEZ's own refusal. RISC Zero dev mode: the guests and LEZ's privacy-preserving circuit are executed, not
proven (proving is not what these requirements are about; the private transactions of the same kinds were proven for
real elsewhere, EVIDENCE rows 5, 73 and 123).

| File | Requirement | Test |
|---|---|---|
| [`vs1-fabricated-chain.md`](vs1-fabricated-chain.md) | VS.1, the fabricated chain | `security_lc::vs1_a_fabricated_chain_is_refused_through_the_program` |
| [`vs2-forgeries.md`](vs2-forgeries.md) | VS.2, the six forgeries | `security_lc::vs2_and_f2_forgeries_are_refused_through_the_program` |
| [`f2-finalised-only.md`](f2-finalised-only.md) | F.2, finalised headers only | the same test |
| [`u5-failure-modes.md`](u5-failure-modes.md) | U.5, each listed failure mode, its code and the IDL's message | `security_lc::u5_every_listed_failure_mode_returns_its_code_through_the_program` |
| [`f4-comparisons.md`](f4-comparisons.md) | F.4, `=, ≠, <, ≤, >, ≥` against a constant and between facts, from the consumer | `security_lc::f4_every_comparison_from_the_consumer` |
| [`r3-rotations.md`](r3-rotations.md) | R.3, records valid after committee rotations | `security_lc::r3_records_stay_valid_after_committee_rotations` |
| [`f8-full-diff.md`](f8-full-diff.md) | F.8, the full-diff test (two deposits, no hop and three hops) | `security_privacy::f8_two_deposits_full_diff` |
| [`r11-one-block.md`](r11-one-block.md) | R.11, several private statements in one block | `security_privacy::r11_several_private_reads_in_one_block` |

Re-run (from `lez/`, with `RISC0_HOME` set as `lez/RESULTS.md` §8 says):

```sh
RISC0_DEV_MODE=1 cargo test -p harness --locked --test security_lc --test security_privacy -- --test-threads 4
```

Ran on the build box (a Modal CPU container), 21 November 2026: `security_lc` 5 of 5 and `security_privacy` 2 of 2
passed; with them the red team's `redteam_lc` (11) and `redteam_attest` (11), which share the test kit these tests
extended, passed unchanged. Every file here is the output of that run, except `u5-failure-modes.md` (its re-run
after the two stale-checkpoint cases were added) and `f8-full-diff.md` (re-run at the branch's last commit; only the
count of differing bytes moves between runs, as fresh random bytes sometimes coincide). No program, guest or image id
changed.

Run again at the integration (build box, 21 November 2026), after two changes to the tests: F.2's text names the two
other ways of offering a header and where they are refused, and F.8's alignment requires the nullifier and the PDA
seed it takes from A to be each claim's own. The four suites passed (5, 2, 11, 11); seven of the eight files here were
byte-identical, and `f8-full-diff.md` differed only in that count (29,786); the red team's 22 output files were
byte-identical to the committed ones.

Run again on 22 November 2026 (build box), after the second review of these results: the texts say what runs next to
the program (Ferrata's policy store, `FinalizedStore`; the specification's store, `LightClientStore`, is run on the two
cases where they part, 50 and 51, and refuses neither while finalizing neither); U.5's last column is the message a
client shows (LEZ's own text where LEZ refuses); F.8 also proves X's claim a second time from the same state and checks
that every field its alignment takes is fresh, or the account's own per-state value; R.11 names the test's own block
loop. `security_lc` 5 of 5 (121 s) and `security_privacy` 2 of 2 passed; after two wording changes to F.8's
freshness lines, `security_privacy` with `redteam_lc` 11 and `redteam_attest` 11 (their 22 output files byte-identical
to the committed ones), then `security_privacy` alone for the final F.8 text (29,818 differing bytes that run), all
passed; `f4-comparisons.md` was byte-identical. R.11 on LEZ v0.4.0-rc3's own sequencer:
[`../v0.4.0-rc3/r11-one-block/`](../v0.4.0-rc3/r11-one-block/README.md).
