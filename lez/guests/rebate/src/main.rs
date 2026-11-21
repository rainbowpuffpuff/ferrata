//! `ferrata_rebate`: a fixed rebate, from a pool anyone funds, to whoever lands a light-client update through it
//! (RFP-021 soft requirement 5). `ferrata_lez_rebate_wire` has the whole design.

use ferrata_core::witness::CompactStore;
use ferrata_lez_rebate_wire::{code, pool_actor, pool_seed, Balance, Pool, RebateMessage, RebateState};
use ferrata_lez_wire::{instance_actor, job_actor, InstanceState, Message};
use lee_core::{
    account::Actor,
    native_token::{custody_transfer, decode_balance, Message as NativeMessage},
    program::{read_input_frame, ReceiveInput, Response, StateReply},
};
use risc0_zkvm::guest::env;

fn main() {
    let input: ReceiveInput = borsh::from_slice(&read_input_frame()).expect("the runtime's input decodes");
    match handle(&input) {
        Ok(response) => {
            response.into_transition(input).commit();
            env::exit(0)
        }
        Err(code) => env::exit(code),
    }
}

fn encode<T: borsh::BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("borsh serialization is infallible")
}

/// The finalized slot in an instance's `Read` reply.
fn finalized_slot(message: &[u8]) -> Result<u64, u8> {
    let reply: StateReply = borsh::from_slice(message).map_err(|_| code::BAD_REPLY)?;
    let instance: InstanceState = borsh::from_slice(&reply.state).map_err(|_| code::BAD_REPLY)?;
    let store = CompactStore::decode(&instance.store).map_err(|_| code::BAD_REPLY)?;
    Ok(store.finalized.slot)
}

fn handle(input: &ReceiveInput) -> Result<Response, u8> {
    let program = input.receiver.program_account_id;
    let state: Option<RebateState> = if input.pre_state.is_empty() {
        None
    } else {
        Some(borsh::from_slice(&input.pre_state).map_err(|_| code::BAD_STATE)?)
    };
    match state {
        None => match borsh::from_slice(&input.message).map_err(|_| code::BAD_MESSAGE)? {
            RebateMessage::Configure { terms } => {
                // The address commits to the terms: at `pool(terms)` only `terms` can be written.
                if input.receiver != pool_actor(program, &terms) {
                    return Err(code::WRONG_RECEIVER);
                }
                let pool = Pool { terms, paid_slot: 0, paid_count: 0, paid_total: 0 };
                Ok(Response::set_state(encode(&RebateState::Configuring(pool)))
                    .send_sync(instance_actor(terms.ferrata, &terms.instance_id), &Message::Read))
            }
            RebateMessage::Land { .. } => Err(code::NOT_CONFIGURED),
        },
        // The instance's state when the pool is created: its finalized slot is already paid for.
        Some(RebateState::Configuring(mut pool)) => {
            if input.from != Some(instance_actor(pool.terms.ferrata, &pool.terms.instance_id)) {
                return Err(code::WRONG_SENDER);
            }
            pool.paid_slot = finalized_slot(&input.message)?;
            Ok(Response::set_state(encode(&RebateState::Ready(pool))))
        }
        Some(RebateState::Ready(pool)) => match borsh::from_slice(&input.message).map_err(|_| code::BAD_MESSAGE)? {
            RebateMessage::Configure { .. } => Err(code::ALREADY_CONFIGURED),
            RebateMessage::Land { job_id, payee } => {
                // In order, each call's subtree completing before the next: the pool's balance, the instance, the
                // update's last step (a refusal there ends the transaction), the instance after it.
                let instance = instance_actor(pool.terms.ferrata, &pool.terms.instance_id);
                let job = job_actor(pool.terms.ferrata, &pool.terms.instance_id, &job_id);
                let balance = Actor::native_balance(input.receiver.account_id);
                Ok(Response::set_state(encode(&RebateState::Landing { pool, payee, balance: None, before: None }))
                    .send_sync(balance, &NativeMessage::ReadState)
                    .send_sync(instance, &Message::Read)
                    .send_sync(job, &Message::Finish)
                    .send_sync(instance, &Message::Read))
            }
        },
        Some(RebateState::Landing { mut pool, payee, balance, before }) => {
            let own_balance = Actor::native_balance(input.receiver.account_id);
            let instance = instance_actor(pool.terms.ferrata, &pool.terms.instance_id);
            if input.from == Some(own_balance) && balance.is_none() {
                let reply: StateReply = borsh::from_slice(&input.message).map_err(|_| code::BAD_REPLY)?;
                let held: Balance = decode_balance(&reply.state).map_err(|_| code::BAD_REPLY)?;
                let landing = RebateState::Landing { pool, payee, balance: Some(held), before };
                return Ok(Response::set_state(encode(&landing)));
            }
            if input.from != Some(instance) || balance.is_none() {
                return Err(code::WRONG_SENDER);
            }
            let slot = finalized_slot(&input.message)?;
            let Some(before) = before else {
                let landing = RebateState::Landing { pool, payee, balance, before: Some(slot) };
                return Ok(Response::set_state(encode(&landing)));
            };
            let amount = pool.earned(before, slot, balance.unwrap_or(0));
            if amount == 0 {
                // Landed, but no advance in this transaction, an advance short of `min_gap` since the last one paid
                // for, or nothing to pay: the update stands, nothing is paid.
                return Ok(Response::set_state(encode(&RebateState::Ready(pool))));
            }
            pool.paid_slot = slot;
            pool.paid_count = pool.paid_count.saturating_add(1);
            pool.paid_total = pool.paid_total.saturating_add(amount);
            let seed = pool_seed(&pool.terms);
            let pay = custody_transfer(input.receiver.account_id, seed, payee, amount);
            Ok(Response::set_state(encode(&RebateState::Ready(pool))).send(pay))
        }
    }
}
