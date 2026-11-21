//! `ferrata_rebate`: a fixed rebate, from a pool anyone funds, to whoever lands a light-client update through it
//! (RFP-021 soft requirement 5, "Operator incentive"). The attestation program is unchanged: updating stays open to
//! anyone, with or without this program.
//!
//! * `pool(ferrata, instance_id)`: an actor of this program whose account holds the pool's native balance (anyone
//!   funds it with an ordinary native transfer: the bridge's fee recipient by policy, a consumer, a foundation).
//!   `Configure { ferrata, instance_id, rebate }` creates it once and reads the instance: the instance's finalized slot
//!   then is the slot already paid for, so nobody is paid for progress made before the pool existed.
//! * `Land { job_id, payee }` at the pool: reads the pool's native balance and the instance, sends the attestation
//!   program's own `Finish` to `job(instance_id, job_id)` (the update's last step, which applies it), then reads the
//!   instance again. LEZ runs sibling calls in order, each subtree completing before the next starts, so the second
//!   read sees the applied update. If the instance's finalized slot advanced within this transaction, the pool pays
//!   `payee` the rebate (or what it holds, if less) out of its own account (`custody_transfer`). Otherwise nothing is
//!   paid and the update still lands. A refused `Finish` ends the whole transaction: nothing is paid for a refused
//!   update; progress others made directly earns nobody a rebate.
//! * Permissionless: any account may call `Land`, naming any payee; any account may still send `Finish` directly and
//!   be paid nothing. An empty pool pays nothing and blocks nothing. One rebate per landed advance of the finalized
//!   slot, however many slots it advances.
//! * Known limit: the rebate goes to whoever lands the job's last step. A job opened and stepped by one operator can be
//!   finished by another through the pool, who then takes the rebate for the first operator's earlier steps (their gas
//!   is not refunded either way). An operator avoids it by sending its own `Land` as soon as its `Miller` step lands.

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Actor},
    program::PdaSeed,
};
use sha2::{Digest, Sha256};

/// Root and hash type (32 bytes).
pub type Root = [u8; 32];

/// The pool's native balance unit.
pub type Balance = u128;

const DOMAIN: &[u8] = b"ferrata-rebate/v1/";

pub fn pool_seed(ferrata: &AccountId, instance_id: &Root) -> PdaSeed {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(b"pool");
    h.update(ferrata.as_ref());
    h.update(instance_id);
    PdaSeed::new(h.finalize().into())
}

/// `pool(ferrata, instance_id)` of the rebate program deployed at `rebate`.
pub fn pool_actor(rebate: AccountId, ferrata: &AccountId, instance_id: &Root) -> Actor {
    Actor::new(AccountId::for_public_pda(&rebate, &pool_seed(ferrata, instance_id)), rebate)
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum RebateMessage {
    /// Create `pool(ferrata, instance_id)`, paying `rebate` per landed advance.
    Configure { ferrata: AccountId, instance_id: Root, rebate: Balance },
    /// Land `job(instance_id, job_id)` (send its `Finish`) and pay `payee` if the finalized slot advanced.
    Land { job_id: Root, payee: AccountId },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Pool {
    pub ferrata: AccountId,
    pub instance_id: Root,
    pub rebate: Balance,
    /// The finalized slot of the last advance paid for (at `Configure`: the instance's finalized slot then).
    pub paid_slot: u64,
    /// Rebates paid so far, and their total.
    pub paid_count: u64,
    pub paid_total: Balance,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum RebateState {
    /// Waiting for the instance's state at `Configure` (never outlives its transaction).
    Configuring(Pool),
    Ready(Pool),
    /// Inside `Land` (never outlives its transaction): the pool's native balance and the instance's finalized slot
    /// before `Finish`, as their replies arrive.
    Landing {
        pool: Pool,
        payee: AccountId,
        balance: Option<Balance>,
        before: Option<u64>,
    },
}

/// The exit codes `ferrata_rebate` owns: 73–79, a block of its own beside the light client's 1–61 and the
/// recorded-statement programs' 62–72 (`ferrata_lez_wire::CODE_BLOCKS` names them; `harness/tests/exit_codes.rs`
/// checks that no other program's code is in it). `Land` runs the attestation program's `Finish`, whose refusals
/// (the core's 1–61 and 100–199, the step API's 80–99, the program's 200–217) end the transaction with their own code.
pub const CODE_BLOCK: core::ops::RangeInclusive<u8> = 73..=79;

macro_rules! exit_codes {
    ($($(#[doc = $doc:literal])* $name:ident = $value:literal;)*) => {
        $($(#[doc = $doc])* pub const $name: u8 = $value;)*
        /// Every code of this module, with its name.
        pub const ALL: &[(&str, u8)] = &[$((stringify!($name), $name)),*];
    };
}

/// Exit codes of the rebate program.
pub mod code {
    exit_codes! {
        /// The message does not decode.
        BAD_MESSAGE = 73;
        /// Stored state that does not decode.
        BAD_STATE = 74;
        /// The receiver is not the actor the message is for.
        WRONG_RECEIVER = 75;
        /// A reply from an actor other than the one waited for.
        WRONG_SENDER = 76;
        /// A reply that does not decode.
        BAD_REPLY = 77;
        /// `Configure` of a pool that exists.
        ALREADY_CONFIGURED = 78;
        /// `Land` at a pool that does not exist.
        NOT_CONFIGURED = 79;
    }
}
