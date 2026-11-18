//! STAND-IN for the light client, for the local sequencer run only (`../../README.md`): writes, once,
//! whatever record it is given at its `header(instance, slot)` account, in the v0.3.1 light client's
//! layout (`lez-v03/wire`: the seed and `RecordState`). It verifies nothing: anyone can write any
//! record. It stands where the v0.4 harness places records at genesis (`lez/BRIDGE-RESULTS.md` §8a);
//! the bridge reads records the same way whichever program wrote them. Never deploy it to a public
//! network with a configuration anyone relies on.

use ferrata_lez03_bridge_core::{header_account, LcRecord, Root};
use lee_core::{
    account::ShardData,
    program::{apply_keep, apply_write, read_input_frame, ApplyInput, CallKind, Plan, PlanInput},
};
use risc0_zkvm::guest::env;

/// `{ instance_id, slot, state }`: write `state` (a borsh `LcRecord`) at `header(instance_id, slot)`.
#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct Write {
    instance_id: Root,
    slot: u64,
    state: Vec<u8>,
}

/// The record is already written (another one, or the same).
const RECORD_CONFLICT: u8 = 208;
/// The instruction or the record does not decode, or the account is not the one it names.
const BAD_INSTRUCTION: u8 = 200;

fn main() {
    let kind: CallKind = borsh::from_slice(&read_input_frame()).expect("the runtime's call kind decodes");
    let payload = read_input_frame();
    match kind {
        CallKind::Plan => {
            let input: PlanInput = borsh::from_slice(&payload).expect("the runtime's plan input decodes");
            let Ok(write) = borsh::from_slice::<Write>(&input.instruction_data) else { env::exit(BAD_INSTRUCTION) };
            let Ok(record) = borsh::from_slice::<LcRecord>(&write.state) else { env::exit(BAD_INSTRUCTION) };
            let program = input.self_account_id;
            let [at] = <&[_; 1]>::try_from(input.accounts.as_slice()).unwrap_or_else(|_| env::exit(BAD_INSTRUCTION));
            if record.instance_id != write.instance_id
                || at.account_id != header_account(program, &write.instance_id, write.slot)
                || at.program_account_id != program
            {
                env::exit(BAD_INSTRUCTION);
            }
            let mut plan = Plan::new(&input);
            plan.effect(at, &write.state);
            plan.write()
        }
        CallKind::Apply => {
            let input: ApplyInput = borsh::from_slice(&payload).expect("the runtime's apply input decodes");
            let Ok(state) = borsh::from_slice::<Vec<u8>>(&input.effect_data) else { env::exit(BAD_INSTRUCTION) };
            if !input.pre_data.is_empty() {
                env::exit(RECORD_CONFLICT);
            }
            match ShardData::try_from(state) {
                Ok(data) => apply_write(input, data),
                Err(_) => apply_keep(input),
            }
        }
    }
}
