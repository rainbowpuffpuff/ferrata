# The lost-burn defect of 22 November 2026: two probes, before and after the fix

Two reviews of 22 November 2026 found the same High: a burn note whose tag `r` another note of the same wallet already
used was applied by the bridge program, and both notes had one release secret `sOut`, hence one release nullifier
`NOut = H("ferrata/nf-out/v2" ‖ sOut)`: the vault releases one of them, the other denomination is destroyed on LEZ and
stays in escrow on Ethereum. The bridge code review (its H-1) found the protocol side and the `ferrata` tool's in-flight
case (a second burn planned while a first is in flight takes the same counter, because the counter came from the
burns already applied); the final review (its H1) found the interrupted case (after an interrupted burn the next burn
took the interrupted burn's counter whatever its recipient, token or amount) and the texts that claimed the opposite.
The fix: [ADR 0032](../../decisions/0032-burn-tag-used-once.md) (the bridge program keys the burn record on the tag,
`burn(cid, r)`; the tool journals its counters, `cli/src/burns.rs`).

The two probes are kept here as they ran. Neither is part of a build: each is copied into a crate's `tests/` on a
checkout, run, and deleted.

| Probe | What | Copy to | Run (dev mode) |
|---|---|---|---|
| [`probe_p1.rs`](probe_p1.rs) | the bridge code review's P1: two notes made with one counter (i = 0) to two recipients, through the bridge program in the harness | `lez/harness/tests/zz_review_probe.rs` | `cd lez && RISC0_HOME=<LEZ's RISC Zero home> RISC0_DEV_MODE=1 cargo test -p harness --locked --test zz_review_probe -- --nocapture` |
| [`probe_h1.rs`](probe_h1.rs) | the tool's library (`lez::plan_burn`, `scan::burns`) on the harness's zone: two burns of one seed planned from one history, from public accounts /0 and /1, both sent | `cli/tests/zz_probe_h1.rs` | `cd cli && RISC0_HOME=<LEZ's RISC Zero home> RISC0_DEV_MODE=1 cargo test --locked --test zz_probe_h1 -- --nocapture` (with `scripts/fetch_native.py env` as `cli/tests/e2e.sh` sets it) |

## Measured (the Modal build box, 22 November 2026; RISC Zero dev mode)

**Before: the tree of `0ddb5168`** (the bridge program since B1, image `0xac1fb59b…442e`, and the tool of that commit).

- P1 (the review's file unchanged; `1 passed`): both burns applied, **165,329 execution gas each**; the payer's
  holding 10 → 8 ETH; both burn records written; **one `NOut`** for both,
  `0x3922b646252d6295aee59c10b124f5256e1799dce59c6d0646e66a756d45098f`.
- H1 (`1 passed`): the tool planned **counter 0 for both** burns (one tag); **both applied**, 164,907 execution gas
  each; the wallet's scan finds two burns (counters `[0, 0]`) with **one distinct `NOut`**, and once one of them is
  released it shows **both released** (`[true, true]`): the loss is silent.

**After: this tree** (the bridge program `0x847a4336…7515`, [`image-ids.md`](../../image-ids.md); the tool with its
journal).

- P1 unchanged now fails at its second burn (`assert!(rep.ok())`): the first applied (156,678 execution gas), the
  second **refused with 244 `ALREADY_WRITTEN`** (158,053 execution gas charged). Its fixed form is the first step of
  [`lez/harness/tests/bridge_burn_tag.rs`](../../../lez/harness/tests/bridge_burn_tag.rs), which asserts the refusal
  and that the second holder keeps its denomination, and goes on to a private burn, a copy of a pending tag and the
  wallet's view.
- H1 (`1 passed`): the library without the journal still plans counter 0 twice (it cannot see a burn in flight; the
  Logos module and a restored client are in the same position), but the program applies the first and **refuses the
  second (244)**: one burn of the seed, one `NOut`, and public account /1 keeps its 1 ETH. The tool's own `burn`
  reserves a counter in its journal before proving and never gives it to another burn:
  [`cli/tests/e2e.rs`](../../../cli/tests/e2e.rs), `counters_pairing_and_quiet_dry_runs` (two burns in flight take
  counters 12 and 13, both applied; after a burn killed with its transaction at the sequencer the next burn takes a
  fresh counter, both applied; a recovery from the seed alone finds every burn with its own `NOut`).
