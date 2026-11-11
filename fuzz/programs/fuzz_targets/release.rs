//! The release and claim guests' inputs (`ReleaseInput`, 1340 bytes; `ClaimInput`, 1260 bytes),
//! their statements (`verify_release`, `verify_claim`), and the Rust decoders of the journals the
//! vault reads (`ReleaseJournal`, the bridge core's `CheckpointJournal`), the segment journal and
//! rule (B)'s signed attestation. Input: a selector byte, then the bytes.
//!
//! Properties: no panic (overflow checks on); a fixed-length input decodes to a value that
//! encodes back to the same bytes (no byte of a private input is ignored); a release the
//! statement accepts commits a journal that the vault-side decoder reads back to exactly the
//! public fields, burn root and nullifier; each journal decoder accepts exactly its encoder's
//! output.
#![no_main]

use checkpoint_core::journal::SegmentJournal;
use ferrata_bridge_core::encoding::{
    CheckpointJournal, ClaimInput, ReleaseInput, ReleaseJournal, CLAIM_INPUT_LEN, RELEASE_INPUT_LEN,
};
use ferrata_bridge_core::inbound::verify_claim;
use ferrata_bridge_core::outbound::{release_journal, verify_release};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&selector, bytes)) = data.split_first() else { return };
    match selector % 6 {
        0 => {
            let Some(bytes) = bytes.get(..RELEASE_INPUT_LEN) else { return };
            let bytes: &[u8; RELEASE_INPUT_LEN] = bytes.try_into().unwrap();
            let input = ReleaseInput::decode(bytes);
            assert_eq!(&input.encode(), bytes, "release input: a byte is ignored");
            if let Ok(nullifier) = verify_release(&input.burn_root, &input.witness, &input.public) {
                let journal = release_journal(&input.burn_root, &nullifier, &input.public);
                let j = ReleaseJournal::decode(&journal).expect("the vault-side decoder reads the journal");
                let p = &input.public;
                assert_eq!(j.chain_id, p.chain_id);
                assert_eq!(j.vault, p.vault);
                assert_eq!(j.lez_config_id, p.config_id);
                assert_eq!(j.burn_root, input.burn_root);
                assert_eq!(j.nullifier, nullifier);
                assert_eq!(j.token, p.token);
                assert_eq!(j.amount, p.amount);
                assert_eq!(j.recipient, p.recipient);
                assert_eq!(j.relayer, p.relayer);
                assert_eq!(j.relay_fee, p.relay_fee);
                assert_eq!(j.encode(), journal);
            }
        }
        1 => {
            let Some(bytes) = bytes.get(..CLAIM_INPUT_LEN) else { return };
            let bytes: &[u8; CLAIM_INPUT_LEN] = bytes.try_into().unwrap();
            let input = ClaimInput::decode(bytes);
            assert_eq!(&input.encode(), bytes, "claim input: a byte is ignored");
            let _ = verify_claim(&input.root, &input.witness, &input.chain_id, &input.vault);
        }
        2 => {
            if let Some(j) = ReleaseJournal::decode(bytes) {
                assert_eq!(j.encode(), bytes, "release journal: another encoding decodes");
            }
        }
        3 => {
            if let Some(j) = CheckpointJournal::decode(bytes) {
                assert_eq!(j.encode(), bytes, "checkpoint journal: another encoding decodes");
            }
        }
        4 => {
            if let Ok(j) = SegmentJournal::decode(bytes) {
                assert_eq!(j.encode(), bytes, "segment journal: another encoding decodes");
            }
        }
        _ => {
            let Some(bytes) = bytes.get(..attestor_core::SIGNED_ATTESTATION_LEN) else { return };
            let bytes: &[u8; attestor_core::SIGNED_ATTESTATION_LEN] = bytes.try_into().unwrap();
            let a = attestor_core::SignedAttestation::decode(bytes);
            assert_eq!(&a.encode(), bytes, "signed attestation: a byte is ignored");
        }
    }
});
