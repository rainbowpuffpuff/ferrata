//! Decoding of everything the bridge program and its peers read with borsh: the program's message
//! and state types, the attestation program's messages, LEZ's `ReceiveInput` and `StateReply`.
//! Input: a selector byte, then the bytes to decode.
//!
//! Properties: no panic; whatever decodes re-encodes to exactly the input (one encoding per
//! value: borsh with `de_strict_order`, strict bools and tags). For `BridgeMessage`, differential
//! against the checkpoint's hand-written decoder (`checkpoint_core::wire`), which must extract
//! exactly the `Burn`, `ImportRoot`, `Sunset` and `Echo` messages the program decodes, with the
//! same fields, and nothing the program refuses.
#![no_main]

use borsh::{BorshDeserialize, BorshSerialize};
use checkpoint_core::wire as cw;
use ferrata_lez_bridge_wire as w;
use libfuzzer_sys::fuzz_target;

fn canonical<T: BorshDeserialize + BorshSerialize>(bytes: &[u8]) -> Option<T> {
    let value = borsh::from_slice::<T>(bytes).ok()?;
    assert_eq!(borsh::to_vec(&value).unwrap(), bytes, "a non-canonical encoding decodes");
    Some(value)
}

/// The checkpoint's view of a message the program decoded.
fn expected(m: &w::BridgeMessage) -> Option<cw::BridgeMessage> {
    Some(match m {
        w::BridgeMessage::Burn { config_id, r, memo, mac, token, amount, c_out } => cw::BridgeMessage::Burn(cw::Burn {
            config_id: *config_id,
            r: *r,
            memo: *memo,
            mac: *mac,
            token: *token,
            amount: *amount,
            c_out: *c_out,
        }),
        w::BridgeMessage::ImportRoot { config_id, record_slot, root, leaf_count, .. } => {
            cw::BridgeMessage::ImportRoot(cw::ImportRoot {
                config_id: *config_id,
                record_slot: *record_slot,
                root: *root,
                leaf_count: *leaf_count,
            })
        }
        w::BridgeMessage::Sunset { config_id } => cw::BridgeMessage::Sunset { config_id: *config_id },
        w::BridgeMessage::Echo { nonce } => cw::BridgeMessage::Echo { nonce: *nonce },
        _ => return None,
    })
}

fuzz_target!(|data: &[u8]| {
    let Some((&selector, bytes)) = data.split_first() else { return };
    match selector % 16 {
        0 => {
            let program = canonical::<w::BridgeMessage>(bytes);
            let checkpoint = cw::BridgeMessage::decode(bytes);
            match (&program, &checkpoint) {
                (Some(p), Ok(c)) => assert_eq!(expected(p), *c, "the checkpoint reads another message"),
                (Some(p), Err(e)) => panic!("the checkpoint refuses a message the program decodes: {p:?}: {e:?}"),
                (None, Ok(Some(c))) => panic!("the checkpoint extracts a message the program refuses: {c:?}"),
                (None, Ok(None)) | (None, Err(_)) => {}
            }
            if let Ok(Some(c)) = &checkpoint {
                if !matches!(c, cw::BridgeMessage::ImportRoot(_)) {
                    assert_eq!(c.encode(), bytes, "the checkpoint's encoder disagrees");
                }
            }
        }
        1 => {
            canonical::<w::BridgeState>(bytes);
        }
        2 => {
            canonical::<w::ClaimWitness>(bytes);
        }
        3 => {
            if let Some(note) = canonical::<w::Note>(bytes) {
                let _ = note.hash();
            }
        }
        4 => {
            if let Some(entry) = canonical::<w::TokenEntry>(bytes) {
                let _ = entry.hash();
            }
        }
        5 => {
            if let Some(schedule) = canonical::<w::FeeSchedule>(bytes) {
                let _ = schedule.hash(&[0x11; 20]);
            }
        }
        6 => {
            if let Some(config) = canonical::<w::BridgeConfig>(bytes) {
                let _ = config.id(ferrata_fuzz_programs::program());
                let _ = config.controls.check();
            }
        }
        7 => {
            canonical::<w::TimeRecord>(bytes);
        }
        8 => {
            canonical::<w::Gate>(bytes);
        }
        9 => {
            canonical::<ferrata_lez_wire::Message>(bytes);
        }
        10 => {
            canonical::<ferrata_lez_wire::attest::Attestation>(bytes);
        }
        11 => {
            canonical::<lee_core::program::StateReply>(bytes);
        }
        12 => {
            canonical::<lee_core::program::ReceiveInput>(bytes);
        }
        13 => {
            canonical::<w::ImportProof>(bytes);
        }
        14 => {
            canonical::<w::VaultProof>(bytes);
        }
        _ => {
            canonical::<w::PairingProof>(bytes);
        }
    }
});
