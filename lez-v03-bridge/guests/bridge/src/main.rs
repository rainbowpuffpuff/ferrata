//! The Ferrata bridge program on LEZ v0.3.1 (`ferrata_lez03_bridge_core`; `../../README.md`).
//! EXPERIMENTAL, UNAUDITED: the inbound leg and the burn of the v0.4 program
//! (`lez/guests/bridge`) on the public testnet's program model.
//!
//! * **`Init`**: the vault's slot 2 holds this configuration's id under a light client record; the
//!   configuration is written once at `config(cid)`.
//! * **`ImportRoot`** (top-level only): the vault's slots 0 and 1 hold the root and the count under a
//!   record, `config(cid)` exists; `root(cid, n)` is written once.
//! * **`Claim`**: a public-kind deposit's leaf under an imported root (the bridge core's claim
//!   statement), its nullifier `nullifier(cid, chain, vault, N)` spent once, the denomination credited at
//!   `holder(cid, dest)` (both keyed by the configuration: the port departs from v0.4 there, README).
//! * **`ClaimPrivately`**: the same for a private-kind deposit, run by its destination, a private
//!   account, inside a privacy-preserving transaction; the credit is that account's own shard.
//! * **`Burn`** (top-level only, signed by the holder): one denomination debited from the holder's own
//!   shard (a private account) or its `holder(cid, account)`; the note written once at `burn(cid, H(note))`
//!   and its leaf appended to the configuration's burn tree at `burns(cid)`.
//!
//! The plan sees no account data; every account the v0.4 handler read is a guard here, checked
//! by the apply on the account's current data (at settlement, for the public accounts of a private
//! transaction). A refusal exits with a code (`code`, `lc_code`, or the light client core's).

use ferrata_lez03_bridge_core::{
    apply, burn_account, burns_account, code, config_account, header_account, holder_account, nullifier_account,
    root_account, sha256, state_hash, u128_of, vault_facts, verify_claim, word_of, BridgeConfig, BridgeState, BurnFrom,
    BurnRecord, ClaimWitness, ConfigState, DestKind, Effect, ImportProof, Instruction, Note, NullifierState,
    PairingProof, RecordRef, Root, RootState, SLOT_CONFIG_ID, SLOT_COUNT, SLOT_ROOT,
};
use lee_core::{
    account::{AccountId, ShardData},
    program::{apply_keep, apply_write, read_input_frame, AccountMeta, ApplyInput, CallKind, Plan, PlanInput},
};
use risc0_zkvm::guest::env;

type Refusal = u8;

fn main() {
    let kind: CallKind = borsh::from_slice(&read_input_frame()).expect("the runtime's call kind decodes");
    let payload = read_input_frame();
    match kind {
        CallKind::Plan => {
            let input: PlanInput = borsh::from_slice(&payload).expect("the runtime's plan input decodes");
            let planned = borsh::from_slice::<Instruction>(&input.instruction_data)
                .map_err(|_| code::BAD_MESSAGE)
                .and_then(|instruction| plan(&input, instruction));
            match planned {
                Ok(plan) => plan.write(),
                Err(c) => env::exit(c),
            }
        }
        CallKind::Apply => {
            let input: ApplyInput = borsh::from_slice(&payload).expect("the runtime's apply input decodes");
            let Ok(effect) = borsh::from_slice::<Effect>(&input.effect_data) else { env::exit(code::BAD_MESSAGE) };
            match apply(&effect, &input.pre_data) {
                // `apply` refuses a state over the shard limit (CAPPED); this cannot fail after it.
                Ok(Some(data)) => match ShardData::try_from(data) {
                    Ok(data) => apply_write(input, data),
                    Err(_) => env::exit(code::CAPPED),
                },
                Ok(None) => apply_keep(input),
                Err(c) => env::exit(c),
            }
        }
    }
}

fn plan(input: &PlanInput, instruction: Instruction) -> Result<Plan, Refusal> {
    match instruction {
        Instruction::Init { config, record, proof } => init(input, config, &record, &proof),
        Instruction::ImportRoot { config, record, root, leaf_count, proof } => {
            import_root(input, config, &record, root, leaf_count, &proof)
        }
        Instruction::Claim { config, root, witness } => claim(input, &config, &root, &witness),
        Instruction::ClaimPrivately { config, root, witness } => claim_privately(input, &config, &root, &witness),
        Instruction::Burn { config, note, from } => burn(input, &config, &note, from),
    }
}

/// The input accounts, exactly `N` of them.
fn accounts<const N: usize>(input: &PlanInput) -> Result<&[AccountMeta; N], Refusal> {
    <&[AccountMeta; N]>::try_from(input.accounts.as_slice()).map_err(|_| code::BAD_MESSAGE)
}

/// `meta` selects `program`'s shard of `account`.
fn is(meta: &AccountMeta, account: AccountId, program: AccountId) -> Result<(), Refusal> {
    if meta.account_id == account && meta.program_account_id == program {
        Ok(())
    } else {
        Err(code::WRONG_SENDER)
    }
}

fn record_guard(record: &RecordRef) -> Effect {
    Effect::RecordIs { hash: sha256(&record.state) }
}

/// Attest the vault's slot 2 under the record; write `config(cid)` if it holds `cid`.
fn init(input: &PlanInput, config: BridgeConfig, record: &RecordRef, proof: &PairingProof) -> Result<Plan, Refusal> {
    config.check()?;
    let program = input.self_account_id;
    let config_id = config.id(program);
    let [at_config, at_header] = accounts::<2>(input)?;
    is(at_config, config_account(program, &config_id), program)?;
    is(at_header, header_account(config.attestation, &config.instance_id, record.slot), config.attestation)?;
    let facts = vault_facts(&config, record, &proof.account, [(SLOT_CONFIG_ID, proof.config_slot.as_slice())])?;
    if facts.values[0] != config_id {
        return Err(code::PAIRING_MISMATCH);
    }
    let attestation = config.attestation;
    let mut plan = Plan::new(input);
    plan.inspect(at_header, attestation, &record_guard(record));
    plan.effect(at_config, &Effect::WriteConfig(ConfigState { config }));
    Ok(plan)
}

/// Attest the vault's slots 0 and 1 under the record; write `root(cid, n)` once, after `Init`.
fn import_root(
    input: &PlanInput,
    config: BridgeConfig,
    record: &RecordRef,
    root: Root,
    leaf_count: u64,
    proof: &ImportProof,
) -> Result<Plan, Refusal> {
    // D10: the checkpoint counts only the imports it sees, a top-level call or a private root.
    if input.caller_account_id.is_some() {
        return Err(code::NOT_AUTHORIZED);
    }
    config.check()?;
    let program = input.self_account_id;
    let config_id = config.id(program);
    let [at_config, at_header, at_root] = accounts::<3>(input)?;
    is(at_config, config_account(program, &config_id), program)?;
    is(at_header, header_account(config.attestation, &config.instance_id, record.slot), config.attestation)?;
    is(at_root, root_account(program, &config_id, leaf_count), program)?;
    let facts = vault_facts(
        &config,
        record,
        &proof.account,
        [(SLOT_ROOT, proof.root_slot.as_slice()), (SLOT_COUNT, proof.count_slot.as_slice())],
    )?;
    if facts.values[0] != root || facts.values[1] != word_of(leaf_count) {
        return Err(code::IMPORT_MISMATCH);
    }
    let state = RootState {
        config_id,
        evm_chain_id: config.evm_chain_id,
        vault: config.vault,
        leaf_count,
        root,
        record_slot: record.slot,
        block_number: facts.block_number,
    };
    let attestation = config.attestation;
    let configured = state_hash(&BridgeState::Config(ConfigState { config }));
    let mut plan = Plan::new(input);
    plan.effect(at_config, &Effect::ConfigIs { hash: configured });
    plan.inspect(at_header, attestation, &record_guard(record));
    plan.effect(at_root, &Effect::WriteRoot(state));
    Ok(plan)
}

/// The claim statement against the root the instruction carries (guarded at `root(cid, n)`), and
/// the registry: the nullifier and the amount.
fn verified(
    program: AccountId,
    config: &BridgeConfig,
    root: &RootState,
    witness: &ClaimWitness,
) -> Result<(Root, u128, DestKind), Refusal> {
    config.check()?;
    if root.config_id != config.id(program) {
        return Err(code::NOT_FOUND);
    }
    let claimed = verify_claim(root, witness)?;
    Ok((claimed.nullifier, claimed.amount, claimed.kind))
}

fn spent(root: &RootState, witness: &ClaimWitness, amount: u128) -> Effect {
    Effect::Spend(NullifierState { config_id: root.config_id, token: witness.token, leaf_count: root.leaf_count, amount })
}

/// The plaintext claim of a public-kind deposit: credited at `holder(cid, dest)`.
fn claim(input: &PlanInput, config: &BridgeConfig, root: &RootState, witness: &ClaimWitness) -> Result<Plan, Refusal> {
    let program = input.self_account_id;
    let (nullifier, amount, kind) = verified(program, config, root, witness)?;
    if kind != DestKind::Public {
        return Err(code::WRONG_DESTINATION);
    }
    if !config.registered(&witness.token, amount) {
        return Err(code::NOT_REGISTERED);
    }
    let [at_root, at_nullifier, at_holder] = accounts::<3>(input)?;
    is(at_root, root_account(program, &root.config_id, root.leaf_count), program)?;
    is(at_nullifier, nullifier_account(program, &root.config_id, root.evm_chain_id, &root.vault, &nullifier), program)?;
    is(at_holder, holder_account(program, &root.config_id, &witness.dest), program)?;
    let mut plan = Plan::new(input);
    plan.effect(at_root, &Effect::RootIs { hash: state_hash(&BridgeState::Root(*root)) });
    plan.effect(at_nullifier, &spent(root, witness, amount));
    plan.effect(at_holder, &Effect::Credit { config_id: root.config_id, token: witness.token, amount });
    Ok(plan)
}

/// The claim of a private-kind deposit by its destination (a private account, authorised: its
/// owner's key is in the proof). The credit is the account's own shard; the root guard and the
/// spend are the transaction's public part.
fn claim_privately(
    input: &PlanInput,
    config: &BridgeConfig,
    root: &RootState,
    witness: &ClaimWitness,
) -> Result<Plan, Refusal> {
    let program = input.self_account_id;
    let [at_dest, at_root, at_nullifier] = accounts::<3>(input)?;
    if !at_dest.is_authorized {
        return Err(code::WRONG_DESTINATION);
    }
    let (nullifier, amount, kind) = verified(program, config, root, witness)?;
    if kind != DestKind::Private || at_dest.account_id.value() != &witness.dest {
        return Err(code::WRONG_DESTINATION);
    }
    if !config.registered(&witness.token, amount) {
        return Err(code::NOT_REGISTERED);
    }
    is(at_dest, AccountId::new(witness.dest), program)?;
    is(at_root, root_account(program, &root.config_id, root.leaf_count), program)?;
    is(at_nullifier, nullifier_account(program, &root.config_id, root.evm_chain_id, &root.vault, &nullifier), program)?;
    let mut plan = Plan::new(input);
    plan.effect(at_dest, &Effect::Credit { config_id: root.config_id, token: witness.token, amount });
    plan.effect(at_root, &Effect::RootIs { hash: state_hash(&BridgeState::Root(*root)) });
    plan.effect(at_nullifier, &spent(root, witness, amount));
    Ok(plan)
}

/// Burn one denomination: debit the holder, write the note once, append its leaf.
fn burn(input: &PlanInput, config: &BridgeConfig, note: &Note, from: BurnFrom) -> Result<Plan, Refusal> {
    // D10: only a top-level call (or a private transaction's root) is visible to a checkpoint.
    if input.caller_account_id.is_some() {
        return Err(code::NOT_AUTHORIZED);
    }
    config.check()?;
    let program = input.self_account_id;
    let config_id = config.id(program);
    let amount = u128_of(&note.amount).ok_or(code::NOT_REGISTERED)?;
    let (owner, debited, at_record, at_burns) = match from {
        BurnFrom::Own => {
            let [holder, record, burns] = accounts::<3>(input)?;
            is(holder, holder.account_id, program)?;
            (holder, holder, record, burns)
        }
        BurnFrom::Holder => {
            let [owner, holder, record, burns] = accounts::<4>(input)?;
            is(holder, holder_account(program, &config_id, owner.account_id.value()), program)?;
            (owner, holder, record, burns)
        }
    };
    if !owner.is_authorized {
        return Err(code::NOT_AUTHORIZED);
    }
    if !config.registered(&note.token, amount) {
        return Err(code::NOT_REGISTERED);
    }
    is(at_record, burn_account(program, &config_id, &note.hash()), program)?;
    is(at_burns, burns_account(program, &config_id), program)?;
    let mut plan = Plan::new(input);
    plan.effect(debited, &Effect::Debit { config_id, token: note.token, amount });
    plan.effect(at_record, &Effect::RecordBurn(BurnRecord { config_id, note: *note }));
    plan.effect(at_burns, &Effect::AppendBurn { config_id, note: *note });
    Ok(plan)
}
