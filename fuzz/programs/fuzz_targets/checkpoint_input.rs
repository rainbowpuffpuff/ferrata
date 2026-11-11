//! The checkpoint guests' inputs and rules: `CheckpointInput` (rule A, `run_checkpoint`),
//! `CheckpointInputB` (rule B, `run_checkpoint_b`: the tip, channel and attestation checks) and
//! `SegmentInput` (`run_segment`). Input: a selector byte, then the guest's input bytes.
//!
//! Properties: no panic (overflow checks on); an input that decodes re-encodes to exactly its
//! bytes (one input per statement); an accepted checkpoint's journal names the parent state the
//! input continues from and the new state the rule returns (digest, tip id and hash); a segment's
//! journal decodes back to itself.
#![no_main]

use checkpoint_core::input::{CheckpointInput, CheckpointInputB, SegmentInput};
use checkpoint_core::journal::{CheckpointJournal, SegmentJournal};
use checkpoint_core::rule::{run_checkpoint, run_segment};
use checkpoint_core::rule_b::run_checkpoint_b;
use checkpoint_core::state::State;
use libfuzzer_sys::fuzz_target;

fn check_journal(j: &CheckpointJournal, parent: &State, new_state: &State) {
    assert_eq!(j.parent_state, parent.digest(), "journal: another parent state");
    assert_eq!(j.new_state, new_state.digest(), "journal: another new state");
    assert_eq!(j.tip_id, new_state.tip_id, "journal: another tip");
    assert_eq!(j.tip_hash, new_state.tip_hash, "journal: another tip hash");
    let bytes = j.encode();
    // The bridge core's reader of the same journal (its first 13 words), where it applies.
    if let Some(b) = ferrata_bridge_core::encoding::CheckpointJournal::decode(&bytes[..13 * 32]) {
        assert_eq!(b.parent_state, j.parent_state);
        assert_eq!(b.new_state, j.new_state);
        assert_eq!(b.burn_root, j.burn_root);
        assert_eq!(b.burn_count, j.burn_count);
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((&selector, bytes)) = data.split_first() else { return };
    match selector % 3 {
        0 => {
            let Ok(input) = CheckpointInput::decode(bytes) else { return };
            assert_eq!(input.encode(), bytes, "rule (A) input: another encoding decodes");
            if let Ok(out) = run_checkpoint(&input) {
                check_journal(&out.journal, &input.parent, &out.new_state);
            }
        }
        1 => {
            let Ok(input) = CheckpointInputB::decode(bytes) else { return };
            assert_eq!(input.encode(), bytes, "rule (B) input: another encoding decodes");
            if let Ok(out) = run_checkpoint_b(&input) {
                check_journal(&out.journal, &input.parent, &out.new_state);
            }
        }
        _ => {
            let Ok(input) = SegmentInput::decode(bytes) else { return };
            assert_eq!(input.encode(), bytes, "segment input: another encoding decodes");
            if let Ok((journal, end)) = run_segment(&input) {
                assert_eq!(journal.start_state, input.parent.digest());
                assert_eq!(journal.end_state, end.digest());
                assert_eq!(SegmentJournal::decode(&journal.encode()).ok(), Some(journal));
            }
        }
    }
});
