//! RFP-021 "Verification Security #3" (`docs/requirements-status.md`, row VS.3): **the
//! verification logic cannot be swapped.** No consensus or inclusion verification may be reached
//! through a `ProgramId` held in mutable account state; change requires deploying a new version
//! and having consumers move to it.
//!
//! The deployed attestation program went through the program loader with `immutable: true`, so
//! its `ProgramHeader` — the only thing that maps its account to code — can never be repointed:
//! every replacement attempt through the loader (`UpdateHeader` to other code, `CreateHeader`
//! over the deployed account, `WriteSegment` over its first segment) is refused on the loader's
//! own assertions, which run before even the authorization check, after which the program still
//! attests exactly as before. The control is a mutable deployment of the same code at a fresh
//! account: an authorized `UpdateHeader` swaps it to other code, and the same account then runs
//! the other code on the same message bytes — the attack that immutability makes impossible.
//! The consumer-side rule follows: the holdings consumer keys `holding(gate, holder)` on the
//! gate, which hashes the attestation program's account, so a holding made under one program id
//! is never mixed with one made under another.
//!
//! The loader messages are built here with the same variants, in the same order, as
//! `program_loader_core::Message` of LEZ v0.4.0-rc1
//! (`upstream/lez-v0.4.0-rc1/lez/programs/program_loader/core/src/lib.rs`): the crate is not a
//! dependency of the harness, but the loader runs natively at settlement and decodes exactly
//! these borsh bytes from the message an actor `(target, PROGRAM_LOADER_ACCOUNT_ID)` receives.
//! Everything else is the real settlement path: signed, fee-paying public transactions through
//! `harness::settle`. A refusal reverts the transaction, is charged (an uncoded failure is
//! charged the whole gas limit) and the payer's nonce still advances; what must not change is the
//! state the attempt could have touched, which is compared account by account before and after.
//!
//! Run: `RISC0_DEV_MODE=1 cargo test -p harness --test immutable_program -- --test-threads 4`.

use borsh::{BorshDeserialize, BorshSerialize};
use ferrata_lez_holdings_wire::{holding_actor, Gate, HoldingsState};
use ferrata_lez_wire::{attest::Address, code, Message};
use harness::{
    attest::u256,
    ferrata_account, ferrata_program,
    holdings::{holdings_account, Holdings},
    program_header, scripted_account, Chain, TxReport, TWIN,
};
use lee::{Account, AccountId, Actor, PrivateKey, PublicKey};
use lee_core::program::{ProgramHeader, ProgramSegment, PROGRAM_LOADER_ACCOUNT_ID};
use test_guest_core::Script;

/// `program_loader`'s message (LEZ v0.4.0-rc1), rebuilt byte-for-byte: the loader is not a
/// dependency of the harness, but its handler decodes exactly these bytes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum LoaderMessage {
    /// Write `bytecode` to the target's still-empty loader actor state, linking to `next_segment`.
    WriteSegment { bytecode: Vec<u8>, next_segment: Option<AccountId> },
    /// Point the target's still-empty loader actor state at a segment chain.
    CreateHeader { first_segment: AccountId, immutable: bool },
    /// Repoint the target's existing header — refused while the old header is immutable.
    UpdateHeader { first_segment: AccountId, immutable: bool },
}

/// Send `message` to the loader at `target`, as one settled public transaction. A target whose
/// account does not exist yet is admitted by `signer`'s key signing beside the fee payer — how a
/// live deploy gets each fresh segment and header account admitted — and that signer is what
/// authorizes `CreateHeader` and `UpdateHeader` for the account it owns.
fn loader_tx(
    chain: &mut Chain,
    label: &str,
    target: AccountId,
    message: &LoaderMessage,
    signer: Option<&PrivateKey>,
    time: u64,
) -> TxReport {
    let root = Actor::new(target, PROGRAM_LOADER_ACCOUNT_ID);
    let signers: Vec<&PrivateKey> = signer.into_iter().collect();
    chain.submit_signed(label, root, borsh::to_vec(message).unwrap(), vec![root], vec![], time, &signers)
}

/// A loader refusal: the transaction reverted while the loader ran (a native panic, so no program
/// exit code), the revert names the loader's own assertion, and nothing was emitted.
fn refused_by_the_loader(report: &TxReport, assertion: &str) {
    assert!(!report.ok(), "{}: accepted", report.label);
    assert_eq!(report.exit_code, None, "{}: the refusal should carry no program exit code", report.label);
    let revert = report.charge.revert.as_deref().unwrap_or_default();
    assert!(revert.contains(assertion), "{}: the revert is {revert:?}", report.label);
    assert!(report.events.is_empty(), "{}: a refusal emits nothing", report.label);
}

/// The header the deployment at `account` holds in its loader actor state.
fn header_of(chain: &Chain, account: AccountId) -> ProgramHeader {
    let state = chain.state.get_account_by_id(account).data.actor_state(PROGRAM_LOADER_ACCOUNT_ID).clone();
    program_header(&state).unwrap_or_else(|| panic!("no program header at {account}"))
}

/// The deployment's own accounts: its header account and every segment account its header links.
fn deployment_accounts(chain: &Chain, account: AccountId) -> Vec<AccountId> {
    let mut out = vec![account];
    let mut next = Some(header_of(chain, account).program_first_segment);
    while let Some(segment_id) = next {
        let state = chain.state.get_account_by_id(segment_id).data.actor_state(PROGRAM_LOADER_ACCOUNT_ID).clone();
        let segment = ProgramSegment::from_bytes(&state).unwrap_or_else(|| panic!("no segment at {segment_id}"));
        next = segment.next_segment;
        out.push(segment_id);
    }
    out
}

/// Every `before` account is exactly what it was.
fn assert_unchanged(chain: &Chain, before: &[(AccountId, Account)]) {
    for (id, account) in before {
        assert_eq!(chain.state.get_account_by_id(*id), *account, "the account at {id} changed");
    }
}

/// The consumer-side rule, at the level of addressing: the gate hashes the attestation program's
/// account, so another program id is another gate id and another holding actor — a consumer that
/// keys its state on the program id it was given can never read, write or replace a holding
/// through a pointer someone else controls.
fn gates_never_mix(program: AccountId, other: AccountId) {
    let holder: Address = [0x42; 20];
    let gate = Gate {
        ferrata: program,
        instance_id: [0x77; 32],
        chain_id: 1,
        token: [0x00; 20],
        balances_slot: [0x00; 32],
        minimum: [0x00; 32],
    };
    let other_gate = Gate { ferrata: other, ..gate.clone() };
    assert_ne!(gate.id(), other_gate.id(), "the gate id hashes the attestation program's account");
    assert_ne!(
        holding_actor(holdings_account(), &gate.id(), &holder).account_id,
        holding_actor(holdings_account(), &other_gate.id(), &holder).account_id,
        "another program id addresses another holding"
    );
}

#[test]
fn the_verification_logic_of_the_deployment_cannot_be_replaced() {
    // The Stage 4 chain with the holdings consumer deployed beside the attestation program: an
    // instance with a record of the fixture's finalized block, and the fixture's WETH proofs.
    let mut h = Holdings::new();
    let holder = h.holder;
    let slot = h.slot();
    let time = h.s.time;
    let gate = h.gate(u256(1));

    // 1. The deployment's header is immutable and names the attestation program's image.
    let header = header_of(&h.s.w.chain, ferrata_account());
    assert!(header.immutable, "the deployed program must be deployed immutable");
    assert_eq!(header.image_id, methods::FERRATA_LEZ_ID, "the header names the attestation image");

    // The other code a replacement would point the account at: the `scripted` test program the
    // harness deploys at genesis beside the attestation program.
    let scripted = header_of(&h.s.w.chain, scripted_account());
    assert_eq!(scripted.image_id, methods::SCRIPTED_ID);

    // 2. A run of the program before any attempt: `Attest` at the record of the finalized block,
    //    as the root of a transaction, with the same request the holdings consumer's gate sends.
    let (account_proof, storage_proof) = h.proofs();
    let request = gate.request(&holder, account_proof.clone(), storage_proof.clone(), None);
    assert!(h.s.expected(slot, &request).is_ok(), "the fixture's proofs pass the native evaluation");
    let (before, _) = h.s.accept("attest before the replacement attempts", slot, &request);

    // Every account an attempt could touch, before: the deployment's header and segments, the
    // other code's, the instance and its record, and both holdings actors below.
    let other_gate = Gate { ferrata: TWIN, ..gate.clone() };
    let holding_here = holding_actor(holdings_account(), &gate.id(), &holder);
    let holding_there = holding_actor(holdings_account(), &other_gate.id(), &holder);
    let touched: Vec<AccountId> = deployment_accounts(&h.s.w.chain, ferrata_account())
        .into_iter()
        .chain(deployment_accounts(&h.s.w.chain, scripted_account()))
        .chain([h.s.w.inst.actor(), h.s.header(slot), holding_here, holding_there].map(|a| a.account_id))
        .collect();
    let before_accounts: Vec<(AccountId, Account)> =
        touched.iter().map(|&id| (id, h.s.w.chain.state.get_account_by_id(id))).collect();

    // 3. The replacement attempts, each through the loader exactly as a deploy would send it.
    //    `UpdateHeader` pointing the deployed account at the other code: refused on the loader's
    //    immutability assertion — which runs before the loader even looks at the signer, so no
    //    authorization can reach past it.
    let r = loader_tx(
        &mut h.s.w.chain,
        "UpdateHeader: the deployed header pointed at other code",
        ferrata_account(),
        &LoaderMessage::UpdateHeader { first_segment: scripted.program_first_segment, immutable: false },
        None,
        time,
    );
    refused_by_the_loader(&r, "immutable and cannot be updated");
    // Again, asking the loader to make the replacement immutable at the same time.
    let r = loader_tx(
        &mut h.s.w.chain,
        "UpdateHeader: the deployed header pointed at other code, made immutable",
        ferrata_account(),
        &LoaderMessage::UpdateHeader { first_segment: scripted.program_first_segment, immutable: true },
        None,
        time,
    );
    refused_by_the_loader(&r, "immutable and cannot be updated");
    // `CreateHeader` over the deployed account: the target does not hold an empty state.
    let r = loader_tx(
        &mut h.s.w.chain,
        "CreateHeader at the deployed account",
        ferrata_account(),
        &LoaderMessage::CreateHeader { first_segment: scripted.program_first_segment, immutable: false },
        None,
        time,
    );
    refused_by_the_loader(&r, "header target already deployed");
    // `WriteSegment` over the deployment's first segment: segments are write-once, so the code
    // the header points at cannot be replaced beneath it either.
    let r = loader_tx(
        &mut h.s.w.chain,
        "WriteSegment over the deployment's first segment",
        header.program_first_segment,
        &LoaderMessage::WriteSegment { bytecode: b"replacement code".to_vec(), next_segment: None },
        None,
        time,
    );
    refused_by_the_loader(&r, "segment target already deployed");

    // 4. Every refusal changed nothing.
    assert_eq!(header_of(&h.s.w.chain, ferrata_account()), header, "the deployed header is unchanged");
    assert_eq!(header_of(&h.s.w.chain, scripted_account()), scripted, "the other code is unchanged");
    assert_unchanged(&h.s.w.chain, &before_accounts);

    // 5. The same run after: the same transaction bytes, applied, at the same cycles, emitting
    //    the same attestation event — the original program still runs the original verification.
    let (after, _) = h.s.accept("attest after the replacement attempts", slot, &request);
    assert_eq!(after.exit_code, before.exit_code);
    assert_eq!(after.charge.cycles, before.charge.cycles, "the same code ran at the same cost");
    assert_eq!(after.charge.gas_stor, before.charge.gas_stor, "the same transaction bytes");
    assert_eq!(after.events.len(), before.events.len(), "the same events");
    for (a, b) in before.events.iter().zip(&after.events) {
        assert_eq!((a.account_id, a.event.selector, &a.event.data), (b.account_id, b.event.selector, &b.event.data));
    }
    assert_unchanged(&h.s.w.chain, &before_accounts);

    // 6. The consumer-side rule, for real. The holdings consumer accepts a gate over the deployed
    //    program and records the holding at `holding(gate, holder)`...
    let accepted = h.check("a gate over the deployed program", &gate, slot);
    assert!(accepted.ok(), "{}: {:?} (exit {:?})", accepted.label, accepted.charge.revert, accepted.exit_code);
    let HoldingsState::Held(held) = h.holding(&gate, &holder).expect("the holding was recorded") else {
        panic!("no holding")
    };
    assert_eq!(held.gate_id, gate.id());

    // ...and a gate that differs only in the program id it pins — the same instance, chain, token,
    // holder and proofs — is another gate entirely: it asks the attestation program exactly the
    // same thing, but it is another gate id addressing another holding actor, and the program its
    // records would live under is not the one the records were written by, so the check is
    // refused, nothing is recorded, and the holding made under the deployed program is not
    // touched by any of it. Whoever controlled the program id could not substitute a module that
    // attests to anything *into* a consumer keyed on the deployed one.
    assert_ne!(gate.id(), other_gate.id(), "the gate id hashes the attestation program's account");
    assert_ne!(holding_here.account_id, holding_there.account_id, "another gate addresses another holding");
    assert_eq!(
        borsh::to_vec(&request).unwrap(),
        borsh::to_vec(&other_gate.request(&holder, account_proof, storage_proof, None)).unwrap(),
        "the two gates ask the attestation program exactly the same thing; only the program id differs",
    );
    let held_before = h.holding(&gate, &holder);
    assert!(h.holding(&other_gate, &holder).is_none(), "no holding under the other gate");
    let (other_account_proof, other_storage_proof) = h.proofs();
    let refused = h.check_with(
        "a gate over another program id: the same proofs, refused",
        &other_gate,
        slot,
        &holder,
        other_account_proof,
        other_storage_proof,
        h.record_evidence(&other_gate, slot),
    );
    assert_eq!(refused.exit_code, Some(u32::from(code::NO_RECORD)), "{}: {:?}", refused.label, refused.charge.revert,);
    assert!(h.holding(&other_gate, &holder).is_none(), "the refusal recorded nothing under the other gate");
    assert_eq!(h.holding(&gate, &holder), held_before, "the holding under the deployed program is untouched");
    gates_never_mix(ferrata_account(), TWIN);
}

#[test]
fn the_control_a_mutable_deployment_of_the_same_code_can_be_swapped() {
    let mut chain = Chain::new();
    let payer = chain.payer;
    let time = 1_700_000_000_000;

    // A mutable deployment of the same code at a fresh account, as a live deploy sends it: the
    // attestation program's user ELF in segments of the loader's recommended size, written
    // tail-to-head, each signed by its fresh account's key, then a header created with
    // `immutable: false`, signed by its own key.
    let user_elf = ferrata_program().user_elf().expect("the attestation program is a program binary");
    let chunks: Vec<&[u8]> = user_elf.chunks(96 * 1024).collect();
    assert!(chunks.len() >= 2, "the attestation program must split into several segments");
    let keys: Vec<PrivateKey> = (0..chunks.len()).map(|i| PrivateKey::try_new([0x61 + i as u8; 32]).unwrap()).collect();
    let segments: Vec<AccountId> = keys.iter().map(|k| AccountId::from(&PublicKey::new_from_private_key(k))).collect();
    for i in (0..chunks.len()).rev() {
        let r = loader_tx(
            &mut chain,
            &format!("WriteSegment {}/{} of the mutable deployment", i + 1, chunks.len()),
            segments[i],
            &LoaderMessage::WriteSegment { bytecode: chunks[i].to_vec(), next_segment: segments.get(i + 1).copied() },
            Some(&keys[i]),
            time,
        );
        assert!(r.ok(), "{}: {:?} (exit {:?})", r.label, r.charge.revert, r.exit_code);
    }
    let header_key = PrivateKey::try_new([0xB1; 32]).unwrap();
    let mutable = AccountId::from(&PublicKey::new_from_private_key(&header_key));
    let r = loader_tx(
        &mut chain,
        "CreateHeader (mutable) at a fresh account",
        mutable,
        &LoaderMessage::CreateHeader { first_segment: segments[0], immutable: false },
        Some(&header_key),
        time,
    );
    assert!(r.ok(), "{}: {:?} (exit {:?})", r.label, r.charge.revert, r.exit_code);
    let header = header_of(&chain, mutable);
    assert!(!header.immutable, "this deployment is mutable — that is the point of the control");
    assert_eq!(header.image_id, methods::FERRATA_LEZ_ID, "the same code as the deployed program");
    assert_eq!(header.program_first_segment, segments[0]);

    // It runs the same code: a `Read` as the root of a transaction has nobody to reply to, so it
    // is refused with the attestation program's own UNROUTABLE code, at exactly the cycles the
    // genesis deployment takes for the same message.
    let at = Actor::new(payer, mutable);
    let read = borsh::to_vec(&Message::Read).unwrap();
    let genesis = chain.submit(
        "Read at the genesis deployment",
        Actor::new(payer, ferrata_account()),
        read.clone(),
        vec![Actor::new(payer, ferrata_account())],
        vec![],
        time,
    );
    let fresh =
        chain.submit("Read at the mutable deployment, before the swap", at, read.clone(), vec![at], vec![], time);
    assert_eq!(fresh.exit_code, Some(u32::from(code::UNROUTABLE)));
    assert_eq!(genesis.exit_code, Some(u32::from(code::UNROUTABLE)));
    assert_eq!(fresh.charge.cycles, genesis.charge.cycles, "the same image runs at both addresses");

    // The message the other code accepts — LEZ's `scripted` test program writing the actor's
    // state — is refused with a coded exit by the attestation program, and writes nothing.
    let script = borsh::to_vec(&Script::write(vec![7; 4])).unwrap();
    let before_state = chain.actor_state(at);
    let refused = chain.submit(
        "Script::write at the mutable deployment, before the swap",
        at,
        script.clone(),
        vec![at],
        vec![],
        time,
    );
    assert!(!refused.ok(), "{}: accepted", refused.label);
    assert!(refused.exit_code.is_some(), "{}: {:?}", refused.label, refused.charge.revert);
    assert_eq!(chain.actor_state(at).as_ref(), before_state.as_ref(), "a coded refusal writes nothing");

    // The swap the immutable deployment refuses (above): an `UpdateHeader` the header account's
    // key authorizes, pointing the mutable header at the `scripted` program's code.
    let scripted = header_of(&chain, scripted_account());
    let r = loader_tx(
        &mut chain,
        "UpdateHeader: the mutable deployment swapped to other code",
        mutable,
        &LoaderMessage::UpdateHeader { first_segment: scripted.program_first_segment, immutable: false },
        Some(&header_key),
        time,
    );
    assert!(r.ok(), "{}: {:?} (exit {:?})", r.label, r.charge.revert, r.exit_code);
    let swapped = header_of(&chain, mutable);
    assert!(!swapped.immutable);
    assert_eq!(swapped.image_id, methods::SCRIPTED_ID, "the account now names the other code's image");
    assert_eq!(swapped.program_first_segment, scripted.program_first_segment);

    // The same `Read` bytes no longer reach the attestation program's handler...
    let after = chain.submit("Read at the mutable deployment, after the swap", at, read, vec![at], vec![], time);
    assert!(!after.ok(), "{}: the other code has no Read to refuse", after.label);
    assert_ne!(
        after.exit_code,
        Some(u32::from(code::UNROUTABLE)),
        "{}: the attestation code no longer runs",
        after.label
    );

    // ...and the same `Script::write` bytes now write the actor's state: the same account, the
    // same message, the other code. Whoever holds the header account's key has substituted a
    // module that runs whatever it pleases — which is why consumers must not accept a program id
    // held in mutable state, and why the deployed attestation program is immutable.
    let applied =
        chain.submit("Script::write at the mutable deployment, after the swap", at, script, vec![at], vec![], time);
    assert!(applied.ok(), "{}: {:?} (exit {:?})", applied.label, applied.charge.revert, applied.exit_code);
    assert_eq!(chain.actor_state(at).as_ref(), &[7, 7, 7, 7][..], "the other code ran at the same account");

    // The consumer-side rule: the holdings consumer's gate keys every holding on the attestation
    // program's account, so nothing a consumer recorded under the deployed program is reachable
    // through the mutable account — a consumer keyed on it is a consumer of whatever its header
    // points at next, on its own, never of the deployed program's holdings.
    gates_never_mix(ferrata_account(), mutable);
}
