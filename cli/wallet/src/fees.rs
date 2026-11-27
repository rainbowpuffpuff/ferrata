//! The bridge program's `CollectFees` (RFP-022 F.13, ADR 0048): anyone attests the vault's cumulative total of the
//! protocol fees it escrowed for a token (every deposit's mint fee and every release's burn fee, at the vault's slot
//! [`bw::lez_fees_slot`]) under a record of the configuration's light-client instance, and the program mints the part
//! it has not minted yet, in the wrapped token, to the configuration's fee recipient. The call names no deposit, claim,
//! burn or leaf: the amount is a total over every deposit and release up to the record, public on Ethereum already.
//!
//! B1's program (`ferrata_bridge_b1`, the test network's) has no `CollectFees`: its claims mint the mint fee on LEZ one
//! by one. Callers check which program a zone runs before building this call ([`crate::deployed::BridgeProgram`]).

use ferrata_bridge_core::Hash;
use ferrata_lez_bridge_wire::{
    self as bw, config_actor, config_seed, fees_actor, fees_seed, holder_actor, holder_seed, token_actor, token_seed,
    Address, BridgeConfig, BridgeMessage, FeesState, VaultProof, Word,
};
use ferrata_lez_wire::{attest, header_actor, RecordState};
use lee_core::account::AccountId;

use crate::import::ImportCall;
use crate::Mention;

/// The proofs of a collection from `eth_getProof`'s `accountProof` and the storage proof of the vault's
/// [`bw::lez_fees_slot`] of the token, with the block's RLP header in front when the record binds the block's hash only
/// (a Gloas record), as the program reads them.
pub fn fees_proof(account: Vec<Vec<u8>>, slot: Vec<Vec<u8>>, header: Option<Vec<u8>>) -> VaultProof {
    let account = match header {
        Some(h) => bw::with_header(h, account),
        None => account,
    };
    VaultProof { account, slots: vec![slot] }
}

/// `CollectFees` of `token` for configuration `cid` of the bridge program at `program`, under the record at
/// `record_slot` of the configuration's attestation instance, minting at most `limit`. Sent at `config(cid)`, which asks
/// `header(id, slot)` to attest the vault's fee total, then `fees(cid, token)` (created on the first collection),
/// `token(cid, token)` (its mint freeze and bucket) and the fee recipient's `holder(cid, account)` (credited).
pub fn collect_call(
    program: AccountId,
    cid: &Hash,
    config: &BridgeConfig,
    record_slot: u64,
    token: Address,
    limit: u128,
    proof: VaultProof,
) -> ImportCall {
    let message = BridgeMessage::CollectFees { config_id: *cid, token, record_slot, limit, proof };
    let recipient = config.controls.fee_recipient;
    let mentions = vec![
        Mention::Pda { actor: config_actor(program, cid), seed: config_seed(cid) },
        Mention::Public(header_actor(config.attestation, &config.instance_id, record_slot)),
        Mention::Pda { actor: fees_actor(program, cid, &token), seed: fees_seed(cid, &token) },
        Mention::Pda { actor: token_actor(program, cid, &token), seed: token_seed(cid, &token) },
        Mention::Pda {
            actor: holder_actor(program, cid, AccountId::new(recipient)),
            seed: holder_seed(cid, &recipient),
        },
    ];
    ImportCall {
        what: format!("collection of the protocol fees of token 0x{} under the record of slot {record_slot}", hex(&token)),
        mentions,
        root_index: 0,
        message,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The collection as the two programs run it up to the mint, natively: the attestation program evaluates the bridge's
/// request against `record` (bound as its `Attest` handler binds it), the bridge program reads the attested total
/// (`check_lez_fees`) and `fees(cid, token)` computes what it mints (`FeesState::accrue`; `current` is that account's
/// state, `None` before the first collection). `Ok`: the attested total, the amount minted and the account's state
/// after it; `Err`: the code the transaction would exit with. Not run here: the token's mint freeze and bucket (they
/// depend on the block's time), which `MintFees` applies.
#[allow(clippy::too_many_arguments)]
pub fn dry_run_collect(
    cid: &Hash,
    config: &BridgeConfig,
    record: &RecordState,
    record_slot: u64,
    token: &Address,
    limit: u128,
    proof: &VaultProof,
    current: Option<&FeesState>,
) -> Result<(Word, u128, FeesState), u8> {
    let request = bw::vault_request(config, &[bw::lez_fees_slot(token)], proof)?;
    let recorded = record.header()?;
    let bound = if request.instance_id == record.root_id { &record.root_id } else { &record.instance_id };
    let attestation = attest::evaluate(bound, record.chain_id, &recorded, &request)?;
    let total = bw::check_lez_fees(config, token, record_slot, &attestation)?;
    let fees = current.copied().unwrap_or(FeesState::new(*cid, *token));
    let (amount, after) = fees.accrue(&total, limit)?;
    Ok((total, amount, after))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The call names the configuration, the record, the fees account, the token and the fee recipient's holder, and
    /// its message carries no deposit, claim, burn or leaf: only the configuration, the token, the record and the limit.
    #[test]
    fn a_collection_names_no_user_transaction() {
        let program = AccountId::new([0x9a; 32]);
        let cid = [0x0c; 32];
        let config = BridgeConfig {
            bedrock_genesis_id: [0x01; 32],
            channel_id: [0x02; 32],
            bridge_image_id: [0x03; 32],
            attestation: AccountId::new([0x55; 32]),
            instance_id: [0x1d; 32],
            evm_chain_id: 11155111,
            vault: [0x22; 20],
            controls: bw::Controls {
                admin: [0x21; 32],
                freeze_authority: [0x22; 32],
                fee_recipient: [0x23; 32],
                floor: bw::CapParams { capacity: 1, refill_per_second: 1 },
                token_cap: bw::CapParams { capacity: 1, refill_per_second: 1 },
                global_cap: bw::CapParams { capacity: 1, refill_per_second: 1 },
            },
        };
        let token = [0u8; 20];
        let proof = fees_proof(vec![vec![0xf8]], vec![vec![0xe3]], None);
        let call = collect_call(program, &cid, &config, 77, token, u128::MAX, proof.clone());
        let actors: Vec<_> = call.mentions.iter().map(|m| m.actor()).collect();
        assert_eq!(
            actors,
            vec![
                config_actor(program, &cid),
                header_actor(config.attestation, &config.instance_id, 77),
                fees_actor(program, &cid, &token),
                token_actor(program, &cid, &token),
                holder_actor(program, &cid, AccountId::new([0x23; 32])),
            ]
        );
        assert_eq!(
            call.message,
            BridgeMessage::CollectFees { config_id: cid, token, record_slot: 77, limit: u128::MAX, proof }
        );
        let bytes = borsh::to_vec(&call.message).unwrap();
        // tag, cid, token, record slot, limit, the proof's two lists (one node each, one slot list).
        assert_eq!(bytes.len(), 1 + 32 + 20 + 8 + 16 + (4 + 4 + 1) + (4 + 4 + 4 + 1));
        // A Gloas record's header travels in front of the account proof.
        let with = fees_proof(vec![vec![0xf8]], vec![vec![0xe3]], Some(vec![0xf9, 0x02]));
        assert_eq!(bw::split_header(&with.account), (Some(vec![0xf9, 0x02]), vec![vec![0xf8]]));
    }
}
