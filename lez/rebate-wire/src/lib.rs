//! `ferrata_rebate`: a fixed rebate, from a pool anyone funds, to whoever lands a light-client update through it
//! (RFP-021 soft requirement 5, "Operator incentive"). The attestation program is unchanged: updating stays open to
//! anyone, with or without this program.
//!
//! * `pool(terms)`: an actor of this program whose account holds the pool's native balance. Its address commits to
//!   its [`Terms`]: the attestation program and instance it pays for, the rebate per paid advance, and the least
//!   advance of the finalized slot it pays for (`min_gap`). A funder pays the address of the terms it wants (an
//!   ordinary native transfer: the bridge's fee recipient by policy, a consumer, a foundation). `Configure { terms }`
//!   creates the pool once and reads the instance: the instance's finalized slot then is the slot already paid for, so
//!   nobody is paid for progress made before the pool existed. Anyone may send it, but at `pool(terms)` it can only
//!   write `terms`: nobody can configure a pool with terms other than the ones its address commits to.
//! * `Land { job_id, payee }` at the pool: reads the pool's native balance and the instance, sends the attestation
//!   program's own `Finish` to `job(instance_id, job_id)` (the update's last step, which applies it), then reads the
//!   instance again. LEZ runs sibling calls in order, each subtree completing before the next starts, so the second
//!   read sees the applied update. If the instance's finalized slot advanced within this transaction, to at least
//!   `paid_slot + min_gap`, the pool pays `payee` the rebate (or what it holds, if less) out of its own account
//!   (`custody_transfer`) and `paid_slot` becomes the new finalized slot. Otherwise nothing is paid and the update
//!   still lands. A refused `Finish` ends the whole transaction: nothing is paid for a refused update; progress others
//!   made directly earns nobody a rebate.
//! * The payout is bounded: at most one rebate per `min_gap` slots of finalized progress, however the progress is split
//!   into updates. While the instance's finalized slot advances by `progress`, the pool pays at most
//!   `rebate × (1 + progress / min_gap)`.
//! * Permissionless: any account may call `Land`, naming any payee; any account may still send `Finish` directly and
//!   be paid nothing. An empty pool pays nothing and blocks nothing.
//! * No withdrawal: what a pool holds leaves only as rebates. A pool whose instance is succeeded, or never advances
//!   again, keeps its balance for good.
//! * Known limit: the rebate goes to whoever lands the job's last step. A job opened and stepped by one operator can be
//!   finished by another through the pool, who then takes the rebate for the first operator's earlier steps (their gas
//!   is not refunded either way). An operator narrows this by submitting its own `Land` immediately after its job's
//!   last step (the last `Adopt`, or `Miller` when there is none), without waiting for that step's inclusion; a
//!   sequencer that reorders transactions can still give the rebate to a copier.

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

const DOMAIN: &[u8] = b"ferrata-rebate/v2/";

/// What a pool pays for, fixed by its address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Terms {
    /// The attestation program.
    pub ferrata: AccountId,
    /// The instance whose advances are paid for.
    pub instance_id: Root,
    /// Paid per paid advance (or what the pool holds, if less).
    pub rebate: Balance,
    /// The least advance of the finalized slot, since the last one paid for, that is paid for.
    pub min_gap: u64,
}

pub fn pool_seed(terms: &Terms) -> PdaSeed {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(b"pool");
    h.update(terms.ferrata.as_ref());
    h.update(terms.instance_id);
    h.update(terms.rebate.to_be_bytes());
    h.update(terms.min_gap.to_be_bytes());
    PdaSeed::new(h.finalize().into())
}

/// `pool(terms)` of the rebate program deployed at `rebate`.
pub fn pool_actor(rebate: AccountId, terms: &Terms) -> Actor {
    Actor::new(AccountId::for_public_pda(&rebate, &pool_seed(terms)), rebate)
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum RebateMessage {
    /// Create `pool(terms)`.
    Configure { terms: Terms },
    /// Land `job(instance_id, job_id)` (send its `Finish`) and pay `payee` if the finalized slot advanced enough.
    Land { job_id: Root, payee: AccountId },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Pool {
    pub terms: Terms,
    /// The finalized slot of the last advance paid for (at `Configure`: the instance's finalized slot then).
    pub paid_slot: u64,
    /// Rebates paid so far, and their total.
    pub paid_count: u64,
    pub paid_total: Balance,
}

impl Pool {
    /// What a landing that took the finalized slot from `before` to `after` earns, with `held` in the pool.
    pub fn earned(&self, before: u64, after: u64, held: Balance) -> Balance {
        if after <= before || after < self.paid_slot.saturating_add(self.terms.min_gap) {
            return 0;
        }
        self.terms.rebate.min(held)
    }
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
