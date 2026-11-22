//! The chain the updater writes to. A `Ledger` reads an actor's state and settles one public
//! transaction paid by the updater's key, with the gas limit and the maximum fee the
//! configuration allows. `Err` is a transport failure (retried with backoff); a transaction the
//! chain settled, applied or refused, is `Ok`.

use anyhow::Result;
use lee::{Actor, PrivateKey, PublicAccountEvidence};

/// One public transaction of the job path, before it is signed.
#[derive(Clone, Debug)]
pub struct Tx {
    pub label: String,
    pub root: Actor,
    pub message: Vec<u8>,
    pub actors: Vec<Actor>,
    pub evidence: Vec<PublicAccountEvidence>,
    /// The block time the transaction is meant for (the harness's blocks take it; a sequencer
    /// sets its own).
    pub time_ms: u64,
}

/// What the chain did with a transaction.
#[derive(Clone, Debug, Default)]
pub struct TxOutcome {
    /// The program accepted it.
    pub applied: bool,
    /// The program's exit code when it refused it with one.
    pub exit_code: Option<u32>,
    /// Never included (the sequencer rejected it before execution: no fee).
    pub rejected: bool,
    pub gas_exec: u64,
    pub gas_stor: u64,
    /// What the payer was charged.
    pub fee: u128,
    pub block: u64,
    pub note: String,
}

pub trait Ledger {
    /// The state of `actor` (empty if it has none).
    fn actor_state(&self, actor: Actor) -> Result<Vec<u8>>;
    /// Sign `tx` with `payer` (at its current nonce), declare `gas_limit` and `max_fee`, send it and
    /// wait until it is settled.
    fn submit(&mut self, payer: &PrivateKey, tx: &Tx, gas_limit: u64, max_fee: u128) -> Result<TxOutcome>;
    /// Save what the backend keeps between runs (the sequencer backend: its follower of the chain); the daemon calls
    /// it now and then and when it stops.
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
    /// The time (Unix milliseconds) the chain's next block will carry, as near as the backend knows: the program's
    /// timestamp validity windows (`ferrata_advance::Window`) are checked against it before a step is sent, since LEZ
    /// charges a transaction executed outside its window the whole gas limit. `None` (the default): every transaction
    /// carries its own block time (`Tx::time_ms`: the in-process harness, and a wrapper of it that does not say
    /// otherwise). The daemon gives every other backend [`machine_now_ms`] (`daemon::Daemon::open_with`): a sequencer
    /// on this machine stamps its blocks with the same clock; the one remote sequencer the owner may name
    /// (`daemon::MachineClock`) with its own, within the window margin or held after a refusal.
    fn now_ms(&self) -> Option<u64> {
        None
    }
}

/// This machine's clock, in Unix milliseconds.
pub fn machine_now_ms() -> Option<u64> {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

#[cfg(feature = "harness")]
pub use harness_ledger::HarnessLedger;

#[cfg(feature = "harness")]
mod harness_ledger {
    use std::collections::BTreeMap;

    use anyhow::{bail, Result};
    use fee_core::BlockFeeSummary;
    use harness::{
        settle::{classify, opening_fee_state, settle_charged_transaction, LeeTransaction},
        trace::trace_public,
    };
    use lee::{
        public_transaction::{Message as PublicMessage, WitnessSet},
        Account, AccountId, Actor, FeeDeclaration, PrivateKey, PublicKey, PublicTransaction, V03State,
    };

    use super::{Ledger, Tx, TxOutcome};

    /// LEZ v0.4.0-rc1's state machine in process, with its fee program and settlement (as
    /// `harness::Chain`), the Ferrata program at genesis, and one funded account per payer key.
    /// Each transaction is settled in a block of its own. `fail_next` makes the next submissions
    /// fail as a dropped connection would (nothing sent). `now` sets the block time of every
    /// transaction (the chain's clock, which the updater reads before sending: `Ledger::now_ms`);
    /// `None` (the default): each transaction's own `time_ms`.
    pub struct HarnessLedger {
        pub state: V03State,
        pub block_id: u64,
        pub fail_next: usize,
        pub now: Option<u64>,
        /// (payer, label, outcome) of every settled transaction.
        pub log: Vec<(AccountId, String, TxOutcome)>,
    }

    impl HarnessLedger {
        pub fn new(payers: &[PrivateKey]) -> Self {
            let mut accounts = vec![
                (system_accounts::fee_state_account_id(), system_accounts::fee_state_account()),
                (system_accounts::fee_escrow_account_id(), Account::default()),
                (system_accounts::fee_inbox_account_id(), Account::default()),
            ];
            for key in payers {
                accounts.push((AccountId::from(&PublicKey::new_from_private_key(key)), Account::funded(1 << 100)));
            }
            let state = V03State::new()
                .with_public_accounts(accounts)
                .with_named_programs([(programs::fee_account_id(), programs::fee())])
                .with_genesis_programs([(harness::ferrata_program(), true)]);
            Self { state, block_id: 2, fail_next: 0, now: None, log: Vec::new() }
        }

        /// The Ferrata program's account.
        pub fn program() -> AccountId {
            harness::ferrata_account()
        }

        pub fn balance(&self, key: &PrivateKey) -> u128 {
            self.state
                .get_account_by_id(AccountId::from(&PublicKey::new_from_private_key(key)))
                .data
                .native_balance()
                .unwrap_or(0)
        }
    }

    impl Ledger for HarnessLedger {
        fn actor_state(&self, actor: Actor) -> Result<Vec<u8>> {
            Ok(self.state.get_account_by_id(actor.account_id).data.actor_state(actor.program_account_id).to_vec())
        }

        fn now_ms(&self) -> Option<u64> {
            self.now
        }

        fn submit(&mut self, payer: &PrivateKey, tx: &Tx, gas_limit: u64, max_fee: u128) -> Result<TxOutcome> {
            if self.fail_next > 0 {
                self.fail_next -= 1;
                bail!("connection reset (injected)");
            }
            let payer_id = AccountId::from(&PublicKey::new_from_private_key(payer));
            let nonce = self.state.get_account_by_id(payer_id).nonce;
            let fee = FeeDeclaration::new(payer_id, gas_limit, 0, max_fee);
            let msg = PublicMessage::new(
                tx.root,
                tx.message.clone(),
                tx.actors.clone(),
                BTreeMap::from([(payer_id, nonce)]),
                Some(fee),
                tx.evidence.clone(),
            );
            let witness_set = WitnessSet::for_message(&msg, &[payer]);
            let ptx = PublicTransaction::new(msg, witness_set);
            let (transitions, _) = trace_public(&ptx, &self.state, gas_limit);
            let lee_tx = LeeTransaction::Public(ptx);
            let view = match classify(&lee_tx) {
                Ok(v) => v,
                Err(e) => {
                    let out = TxOutcome { rejected: true, note: e, ..Default::default() };
                    self.log.push((payer_id, tx.label.clone(), out.clone()));
                    return Ok(out);
                }
            };
            let opening = opening_fee_state(&self.state);
            let mut summary = BlockFeeSummary::default();
            let before = self.state.get_account_by_id(payer_id).data.native_balance().unwrap_or(0);
            let out = match settle_charged_transaction(
                &lee_tx,
                &view,
                &mut self.state,
                &opening,
                self.block_id,
                self.now.unwrap_or(tx.time_ms),
                &mut summary,
            ) {
                Ok((_, charge)) => {
                    let after = self.state.get_account_by_id(payer_id).data.native_balance().unwrap_or(0);
                    let exit_code = transitions.iter().map(|t| t.exit_code).find(|c| *c != 0);
                    let out = TxOutcome {
                        applied: charge.revert.is_none(),
                        exit_code: if charge.revert.is_some() { exit_code } else { None },
                        rejected: false,
                        gas_exec: charge.gas_exec,
                        gas_stor: charge.gas_stor,
                        fee: before.saturating_sub(after),
                        block: self.block_id,
                        note: charge.revert.unwrap_or_default(),
                    };
                    self.block_id += 1;
                    out
                }
                // Not admitted (a nonce, a fee the payer cannot cover, a max fee too low):
                // nothing is charged, nothing is included.
                Err(e) => TxOutcome { rejected: true, note: e, ..Default::default() },
            };
            self.log.push((payer_id, tx.label.clone(), out.clone()));
            Ok(out)
        }
    }
}
