//! The second build of the bridge program (`guests/bridge-next`, not deployed): the nullifier and holder accounts
//! keyed by the configuration id as well. Cargo feature `config-keyed`, off by default; the deployed program
//! (`guests/bridge`, the image id the live Sepolia vault's `configId` commits to) keeps the keys of the crate root.
//!
//! | actor | address | deployed program |
//! |---|---|---|
//! | `nullifier(cid, chain, vault, N)` | PDA `H(…/nullifier ‖ cid ‖ chain ‖ vault ‖ N)` | `nullifier(chain, vault, N)`: one namespace for every configuration |
//! | `holder(cid, account)` | PDA `H(…/holder ‖ cid ‖ account)` | `holder(account)`: one account for every configuration |
//!
//! Why (`docs/design/bridge.md` §3, "Known issues, 18 November 2026"; `lez/harness/tests/tenant.rs`):
//!
//! * R1. A configuration names its attestation program and instance freely, so anyone can make a second
//!   configuration of a vault whose records its creator writes. Keyed by `(chain, vault, N)` alone, such a
//!   configuration spends the nullifier of any deposit whose `s` it knows, and the paired configuration's claim of it
//!   is then refused (250): the deposit frozen. Keyed by `cid` too, each configuration spends only its own. Nothing is
//!   lost by separating them: a vault releases only burns under the one `configId` it pins (`lezConfigId`), so
//!   another configuration's mints are worth nothing to it, and within a configuration a deposit still mints once.
//! * R2. One `holder(account)` for every configuration is a list that anyone can lengthen, one holding per
//!   (configuration, token) pair, until a mint, burn or transfer there exceeds a transaction's execution gas. Keyed
//!   by `cid`, `holder(cid, account)` is credited only under `cid` (`Mint` comes only from `nullifier(cid, …)`,
//!   `Credit` only from `token(cid, token)`, and both check the receiver), so it holds at most one holding per token
//!   registered under `cid`: other configurations cannot lengthen it.
//!
//! The seeds are the crate root's with `cid` in front, as the LEZ v0.3.1 port has them (branch `testnet-v03-bridge`,
//! `lez-v03-bridge/core/src/lib.rs`). The program account differs from the deployed program's (another image), so
//! none of these addresses is an address of the deployed program.
//!
//! Also here, and also the second build's only: [`check_init_roles`], the red team's finding F1
//! (`docs/reviews/redteam-2026-11-18.md`, vector BR-INIT-05): the deployed program's `Init` checks only
//! `Controls::check`, so it writes a configuration whose freeze authority is its admin, although SPEC-v2 §7 says the two
//! must differ and `ProposeRoles` refuses the admin as a member (`FreezeAuthority::check`). The second build's `Init`
//! refuses it with the same code.

use lee_core::{
    account::{AccountId, Actor},
    program::PdaSeed,
};

use super::{chain_id_u256, pda, tagged_hash, Address, Controls, FreezeAuthority, Root};

/// `nullifier(cid, chain, vault, N)`'s seed.
pub fn nullifier_seed(config_id: &Root, evm_chain_id: u64, vault: &Address, nullifier: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"nullifier", &[config_id, &chain_id_u256(evm_chain_id), vault, nullifier]))
}

/// `holder(cid, account)`'s seed.
pub fn holder_seed(config_id: &Root, account: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"holder", &[config_id, account]))
}

/// `nullifier(cid, chain, vault, N)`: `N` spent once by a claim under configuration `cid`.
pub fn nullifier_actor(
    program: AccountId,
    config_id: &Root,
    evm_chain_id: u64,
    vault: &Address,
    nullifier: &Root,
) -> Actor {
    pda(program, &nullifier_seed(config_id, evm_chain_id, vault, nullifier))
}

/// `holder(cid, account)`: `account`'s public wrapped balances under configuration `cid`. Its seed carries the role,
/// so it is never the address of another actor of the program, whatever account a claim names (security review C-1).
pub fn holder_actor(program: AccountId, config_id: &Root, account: AccountId) -> Actor {
    pda(program, &holder_seed(config_id, account.value()))
}

/// F1: the roles `Init` would write, checked as `ProposeRoles` checks a proposed authority. `Roles::new` makes the
/// configured key the live freeze authority (`FreezeAuthority::key`); [`FreezeAuthority::check`] refuses it with
/// `BAD_MESSAGE` (240) if it is the admin (no account holds both roles, as on the vault, `RolesMustDiffer`).
pub fn check_init_roles(controls: &Controls) -> Result<(), u8> {
    FreezeAuthority::key(controls.freeze_authority).check(&controls.admin)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM: [u8; 32] = [0x9a; 32];

    #[test]
    fn keyed_by_the_configuration() {
        let program = AccountId::new(PROGRAM);
        let (a, b, vault, n, account) = ([0xaa; 32], [0xbb; 32], [0x22; 20], [0x4e; 32], AccountId::new([0x77; 32]));
        // Two configurations of one vault: two nullifier accounts and two holder accounts.
        assert_ne!(nullifier_actor(program, &a, 1, &vault, &n), nullifier_actor(program, &b, 1, &vault, &n));
        assert_ne!(holder_actor(program, &a, account), holder_actor(program, &b, account));
        // None of them is the deployed program's key at the same program account.
        assert_ne!(nullifier_actor(program, &a, 1, &vault, &n), crate::nullifier_actor(program, 1, &vault, &n));
        assert_ne!(holder_actor(program, &a, account), crate::holder_actor(program, account));
        // The crate root's tagging: the same seed with the configuration id left out is the deployed key.
        assert_eq!(
            PdaSeed::new(tagged_hash(b"holder", &[account.value()])),
            crate::holder_seed(account.value()),
            "the same domain and tag as the deployed program's"
        );
        assert_eq!(
            PdaSeed::new(tagged_hash(b"nullifier", &[&chain_id_u256(1), &vault, &n])),
            crate::nullifier_seed(1, &vault, &n)
        );
        // The roles stay apart: a holder's address is never a nullifier's, whatever the account.
        assert_ne!(holder_seed(&a, &n), nullifier_seed(&a, 1, &vault, &n));
    }

    #[test]
    fn init_refuses_the_admin_as_freeze_authority() {
        let cap = crate::CapParams { capacity: 10, refill_per_second: 1 };
        let ok = Controls {
            admin: [0x21; 32],
            freeze_authority: [0x22; 32],
            fee_recipient: [0x23; 32],
            floor: cap,
            token_cap: cap,
            global_cap: cap,
        };
        assert_eq!(check_init_roles(&ok), Ok(()));
        let same = Controls { freeze_authority: ok.admin, ..ok };
        assert_eq!(same.check(), Ok(()), "the deployed program's only check at Init lets it through");
        assert_eq!(check_init_roles(&same), Err(crate::code::BAD_MESSAGE));
        // The code ProposeRoles gives the admin as a member of a proposed authority.
        let proposed = FreezeAuthority { members: alloc::vec![[0x24; 32], ok.admin], threshold: 1 };
        assert_eq!(proposed.check(&ok.admin), Err(crate::code::BAD_MESSAGE));
    }
}
