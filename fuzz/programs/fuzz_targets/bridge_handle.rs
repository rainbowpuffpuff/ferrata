//! The bridge program's own `handle` (the guest's source, compiled on the host) over a sequence of
//! up to six deliveries to one actor, its state carried from one to the next as LEZ carries it.
//!
//! Input: `mode` (bits 0-2: which actor receives, named by the first message's fields; bit 3: an
//! initial state follows as `u16 len ‖ bytes`), then steps `flags ‖ u16 len ‖ message`, where
//! flags bits 0-2 pick the sender (none, the actor the state waits for, an actor named by the
//! message, the receiver itself) and bit 3 sets `is_authorized`.
//!
//! Properties, every delivery: no panic (overflow checks on); a refusal is a code of the
//! program's block (240..=255), never 0, which would end the guest as a success with nothing
//! committed; a written state decodes as a `BridgeState` and re-encodes to the same bytes (a state
//! the program cannot read back would brick its actor); a sync message to the program's own
//! actors decodes canonically as a `BridgeMessage`, a `StateReply` or (when a configuration names
//! the bridge program as its attestation program) the attestation program's `Message`.
#![no_main]

use ferrata_fuzz_programs::{actor, awaited, bridge_guest, names, program, Names};
use ferrata_lez_bridge_wire::{BridgeMessage, BridgeState, CODE_BLOCK};
use lee_core::account::Actor;
use lee_core::program::{ReceiveInput, StateReply};
use libfuzzer_sys::fuzz_target;

fn take<'a>(data: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
    if data.len() < n {
        return None;
    }
    let (head, tail) = data.split_at(n);
    *data = tail;
    Some(head)
}

fn chunk<'a>(data: &mut &'a [u8]) -> Option<&'a [u8]> {
    let len = take(data, 2)?;
    let len = usize::from(u16::from_le_bytes([len[0], len[1]]));
    let len = len.min(data.len());
    take(data, len)
}

fn canonical<T: borsh::BorshDeserialize + borsh::BorshSerialize>(bytes: &[u8]) -> Option<T> {
    let value = borsh::from_slice::<T>(bytes).ok()?;
    assert_eq!(borsh::to_vec(&value).unwrap(), bytes, "a non-canonical encoding decodes");
    Some(value)
}

/// `FERRATA_FUZZ_EXPLAIN=1`: say which type each own-program message decoded as (replays only).
fn explain() -> bool {
    static EXPLAIN: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *EXPLAIN.get_or_init(|| std::env::var_os("FERRATA_FUZZ_EXPLAIN").is_some())
}

fuzz_target!(|data: &[u8]| {
    let mut data = data;
    let Some(&[mode]) = take(&mut data, 1) else { return };
    let mut state: Vec<u8> = Vec::new();
    if mode & 8 != 0 {
        let Some(initial) = chunk(&mut data) else { return };
        state = initial.to_vec();
    }
    let mut receiver: Option<Actor> = None;
    for _ in 0..6 {
        let Some(&[flags]) = take(&mut data, 1) else { return };
        let Some(message) = chunk(&mut data) else { return };
        let decoded = borsh::from_slice::<BridgeMessage>(message).ok();
        let n = decoded.as_ref().map(names).unwrap_or(Names::default());
        let to = *receiver.get_or_insert_with(|| actor(mode & 7, &n));
        let current = if state.is_empty() { None } else { borsh::from_slice::<BridgeState>(&state).ok() };
        let from = match flags & 7 {
            0 => None,
            1 => current.as_ref().and_then(awaited),
            7 => Some(to),
            k => Some(actor(k - 2, &n)),
        };
        let input = ReceiveInput {
            receiver: to,
            from,
            is_authorized: flags & 8 != 0,
            pre_state: state.clone().into(),
            message: message.to_vec(),
        };
        match bridge_guest::run(&input) {
            Err(code) => {
                assert!(CODE_BLOCK.contains(&code), "refusal code {code} outside the program's block");
                return;
            }
            Ok(response) => {
                for sync in &response.sync_messages {
                    if sync.to.program_account_id == program() {
                        // The program also sends the attestation program's `Attest` and `Read` to the
                        // account its configuration names as that program; a configuration may name
                        // the bridge program itself (the fuzzer builds such ones), and then those
                        // land on the program's own id.
                        let kind = if canonical::<BridgeMessage>(&sync.message).is_some() {
                            "bridge message"
                        } else if canonical::<StateReply>(&sync.message).is_some() {
                            "state reply"
                        } else if canonical::<ferrata_lez_wire::Message>(&sync.message).is_some() {
                            "attestation program's message"
                        } else {
                            panic!("a message to the program's own actor does not decode")
                        };
                        if explain() {
                            let named = format!("{current:?} {decoded:?}").contains(&format!("{:?}", program()));
                            eprintln!("explain: to the program's own id: {kind}; the program's id named in the state or message: {named}");
                        }
                    }
                }
                if let Some(post) = &response.post_state {
                    let post: &[u8] = post;
                    assert!(canonical::<BridgeState>(post).is_some(), "the program wrote a state it cannot read");
                    state = post.to_vec();
                }
            }
        }
    }
});
