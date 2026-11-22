//! RFP-021 F.8 (consumption adds no correlation signal, over full event and state diffs) and R.11
//! (several private transactions submit statements in one block, each computing its result before
//! inclusion), through the attestation program and the reference consumer on LEZ v0.4's state
//! machine (the in-process harness; run with `RISC0_DEV_MODE=1`: the private transitions and LEZ's
//! privacy-preserving circuit are executed, not proven; settlement is LEZ's own). Writes
//! `results/lc-security/f8-full-diff.md` and `r11-one-block.md`.
//!
//! The F.8 test is the one `docs/design/attestation.md` §6.5 specifies: two deposits of the same
//! token and amount, in different blocks, one needing no hop and one needing three, claimed
//! privately against the same record. Their transactions, events and state diffs must be identical
//! except for fresh randomness and the nullifiers (the claim's, and the account's own with its new
//! commitments, set by the private state spent and made); a private consumer that calls `Attest`
//! instead of reading must fail the same comparison. The chain is synthetic (`security_common::Synth`, a
//! mainnet-preset light client whose committees the test holds) because no real chain has two
//! deposits to accounts whose keys a test holds, one of them three hops back.

mod security_common;

use std::collections::{BTreeMap, BTreeSet};

use borsh::BorshDeserialize;
use fee_core::BlockFeeSummary;
use ferrata_core::Root;
use ferrata_lez_wire::{
    attest::{evaluate, Address},
    consumer::{
        claim_actor, claim_seed, commitment, secret_nullifier, ConsumerMessage, DepositProof, NullifierState, Pool,
        VaultPool, KIND_PRIVATE,
    },
    header_actor,
};
use harness::{
    attest::consumer_state,
    consumer_account, mainnet_config,
    private::{catalog, PrivateAccount, PrivateRun, Shape},
    settle::{classify, opening_fee_state, settle_private_transaction, LeeTransaction},
    Chain, EXPECTED_FINALIZED,
};
use lee::{Account, AccountId, Actor, PrivacyPreservingTransaction};
use lee_core::{execution_state::BoundaryStep, program::PdaSeed, PrivateAction};
use security_common::*;

const VAULT: Address = [0x5a; 20];
/// 0.001 ETH: both deposits' denomination.
const AMOUNT: u128 = 1_000_000_000_000_000;
const RESERVE: u128 = 100_000_000;

fn require_dev_mode() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
}

fn occurs(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Every public account of the state (the first field of `V03State`'s borsh encoding is its
/// public accounts, sorted by id).
fn public_accounts(chain: &Chain) -> BTreeMap<AccountId, Account> {
    let bytes = borsh::to_vec(&chain.state).unwrap();
    let accounts: Vec<(AccountId, Account)> = BorshDeserialize::deserialize(&mut &bytes[..]).unwrap();
    accounts.into_iter().collect()
}

/// The public accounts that differ between two states: (id, before, after).
fn diff(
    before: &BTreeMap<AccountId, Account>,
    after: &BTreeMap<AccountId, Account>,
) -> Vec<(AccountId, Option<Account>, Option<Account>)> {
    let mut ids: Vec<AccountId> = before.keys().chain(after.keys()).copied().collect();
    ids.sort();
    ids.dedup();
    ids.into_iter()
        .filter(|id| before.get(id) != after.get(id))
        .map(|id| (id, before.get(&id).cloned(), after.get(&id).cloned()))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The synthetic deposits
// ---------------------------------------------------------------------------------------------

/// A record of block B (holding deposit A), and blocks B1, B2, B3 reached from it by EIP-2935
/// hops; B3 holds deposit D. Both deposits are 0.001 ETH to the vault, private-kind, each to the
/// private account that claims it.
struct Deposits {
    s: Synth,
    slot: u64,
    pool: VaultPool,
    x: PrivateAccount,
    y: PrivateAccount,
    /// (secret, deposit proof, statement id, nullifier) of A and of D.
    a: ([u8; 32], DepositProof, Root, Root),
    d: ([u8; 32], DepositProof, Root, Root),
    blocks: Vec<u64>,
    time: u64,
}

fn deposits() -> Deposits {
    let slot0 = EXPECTED_FINALIZED[0].0 + 64;
    let mut s = Synth::new(mainnet_config(2), 0x71, slot0, filler_header(slot0), 2);
    let x = s.w.chain.add_private_account(0x81, 1_000_000_000_000);
    let y = s.w.chain.add_private_account(0x82, 1_000_000_000_000);
    let (dx, dy) = (*x.id().value(), *y.id().value());
    let (sa, sd) = ([0x1a; 32], [0x2d; 32]);
    let n = 24_000_000u64;
    let eth: Address = [0; 20];
    let b3 = Block::new(
        n - 24_000,
        &[AccountSpec::eoa([0x33; 20], 5)],
        &[
            plain_receipt(0),
            plain_receipt(1),
            deposit_receipt(VAULT, [0xd0; 20], eth, 1, AMOUNT, commitment(&sd, KIND_PRIVATE, &dy)),
        ],
    );
    let b2 = Block::new(n - 16_000, &[AccountSpec::history(&[(b3.number, b3.hash())])], &[plain_receipt(0)]);
    let b1 = Block::new(n - 8_000, &[AccountSpec::history(&[(b2.number, b2.hash())])], &[plain_receipt(0)]);
    let b = Block::new(
        n,
        &[AccountSpec::history(&[(b1.number, b1.hash())])],
        &[plain_receipt(0), deposit_receipt(VAULT, [0xa0; 20], eth, 0, AMOUNT, commitment(&sa, KIND_PRIVATE, &dx))],
    );
    let slot = slot0 + 64;
    let u = s.update(0, Some(header_of(slot, &b)), filler_header(slot0 + 96), None, slot0 + 97, 512);
    assert!(s.w.apply(&u, "the record of block B").reports.iter().all(harness::TxReport::ok));
    let pool = VaultPool {
        ferrata: s.w.inst.program,
        instance_id: s.w.inst.id,
        chain_id: s.w.inst.config.chain_id,
        vault: VAULT,
        token: eth,
    };
    let record = s.w.inst.record(&s.w.chain, slot).unwrap().header().unwrap();
    let prove = |secret: [u8; 32], deposit: DepositProof| {
        let a =
            evaluate(&pool.instance_id, pool.chain_id, &record, &pool.request(&deposit)).expect("the deposit verifies");
        let id = a.statements[0].id;
        (secret, deposit, id, secret_nullifier(&secret, &id))
    };
    let a =
        prove(sa, DepositProof { header: None, hops: vec![], tx_index: 1, log_index: 0, proof: b.receipt_proof(1) });
    let d = prove(
        sd,
        DepositProof {
            header: None,
            hops: vec![b.hop_to(&b1), b1.hop_to(&b2), b2.hop_to(&b3)],
            tx_index: 2,
            log_index: 0,
            proof: b3.receipt_proof(2),
        },
    );
    let time = s.time(&u) + 60_000;
    Deposits { s, slot, pool, x, y, a, d, blocks: vec![b.number, b1.number, b2.number, b3.number], time }
}

impl Deposits {
    fn record_actor(&self) -> Actor {
        header_actor(self.pool.ferrata, &self.pool.instance_id, self.slot)
    }

    /// The private claim of `dep` by `account`, proven against `chain`'s state and settled on it.
    fn claim(
        &self,
        chain: &mut Chain,
        account: &PrivateAccount,
        dep: &([u8; 32], DepositProof, Root, Root),
    ) -> PrivateRun {
        let message = ConsumerMessage::ClaimDepositPrivately {
            pool: self.pool.clone(),
            slot: self.slot,
            deposit: dep.1.clone(),
            s: dep.0,
            kind: KIND_PRIVATE,
            dest: *account.id().value(),
        };
        let publics = vec![self.record_actor(), claim_actor(consumer_account(), &self.pool.id(), &dep.3)];
        chain
            .prove_and_settle_private_as(
                account,
                Actor::new(account.id(), consumer_account()),
                borsh::to_vec(&message).unwrap(),
                publics,
                &catalog(),
                RESERVE,
                self.time,
                Shape::Wallet,
            )
            .unwrap_or_else(|e| panic!("the private claim settles: {e}"))
    }

    /// The same consumer calling `Attest` privately with the deposit's request instead of reading.
    fn call(
        &self,
        chain: &mut Chain,
        account: &PrivateAccount,
        dep: &([u8; 32], DepositProof, Root, Root),
    ) -> PrivateRun {
        let message = ConsumerMessage::CallAttest {
            ferrata: self.pool.ferrata,
            slot: self.slot,
            request: self.pool.request(&dep.1),
        };
        chain
            .prove_and_settle_private_as(
                account,
                Actor::new(account.id(), consumer_account()),
                borsh::to_vec(&message).unwrap(),
                vec![self.record_actor()],
                &catalog(),
                RESERVE,
                self.time,
                Shape::Wallet,
            )
            .unwrap_or_else(|e| panic!("the private call settles: {e}"))
    }
}

// ---------------------------------------------------------------------------------------------
// The comparison
// ---------------------------------------------------------------------------------------------

/// What the claim's nullifier fixes in each transaction (A's, then D's): N itself, as `Spend`
/// carries it, and the actor and PDA seed of `claim(pool, N)`.
#[derive(Clone, Copy)]
struct Claims {
    nullifier: (Root, Root),
    actor: (Actor, Actor),
    seed: (PdaSeed, PdaSeed),
}

impl Claims {
    fn new(program: AccountId, pool_id: &Root, n_a: &Root, n_d: &Root) -> Self {
        Claims {
            nullifier: (*n_a, *n_d),
            actor: (claim_actor(program, pool_id, n_a), claim_actor(program, pool_id, n_d)),
            seed: (claim_seed(pool_id, n_a), claim_seed(pool_id, n_d)),
        }
    }
}

/// Align `d` to `a`: every field that may differ is taken from `a`, after checking it has the same
/// encoded length in both: the private side's actors (aliases), the private actions' nullifiers,
/// commitments and encrypted notes, the recovery bindings, the fee fallback's private action and
/// the payer alias's crossing in its boundary; and the claim's nullifier N, in `Spend` and in the
/// address and PDA seed of `claim(pool, N)` (each transaction's own). Of those, the aliases, notes,
/// recovery bindings and dummy actions are fresh randomness, and the account's own nullifier and
/// commitments are determined by the private state it spends and makes (the same if the same state
/// is spent twice; that they reveal nothing of the account is LEZ's nullifier and commitment scheme,
/// not shown here); `fresh_check` shows which is which. Every other
/// field must already be equal. Returns the log of what was aligned, or the first difference no
/// rule allows, by field. The caller then requires the two messages' encodings to be
/// byte-identical.
fn align(
    a: &PrivacyPreservingTransaction,
    d: &PrivacyPreservingTransaction,
    claims: Option<Claims>,
) -> Result<(PrivacyPreservingTransaction, Vec<String>), String> {
    let mut out = d.clone();
    let mut log = Vec::new();
    // Public context: the claim actor is N's.
    if let Some(Claims { actor: (ca, cd), .. }) = claims {
        if out.message.public_context.actors.remove(&cd) {
            out.message.public_context.actors.insert(ca);
            log.push("public_context.actors: claim(pool, N) (the nullifier's address)".to_string());
        }
    }
    let publics = a.message.public_context.actors.clone();
    let (ea, eo) = (&a.message.execution, &mut out.message.execution);
    align_boundary("boundary", &ea.boundary, &mut eo.boundary, &publics, claims, &mut log)?;
    // Private actions: nullifiers, commitments and notes of equal sizes (the account's own nullifier
    // and commitment set by the state spent and made, the dummies' fresh, every note fresh); the
    // commitment root they were proven against must be the same.
    if ea.private_actions.len() != eo.private_actions.len() {
        return Err("private actions: another count".into());
    }
    for (i, (pa, po)) in ea.private_actions.iter().zip(eo.private_actions.iter_mut()).enumerate() {
        align_action(&format!("private action {i}"), pa, po)?;
    }
    log.push(format!(
        "{} private actions: nullifier, commitment and encrypted note (same sizes, same root): the account's own \
         (nullifier and commitment set by the private state spent and made, note fresh) and the dummies (nullifier, \
         commitment and note fresh)",
        ea.private_actions.len()
    ));
    if ea.recovery_bindings.len() != eo.recovery_bindings.len() {
        return Err("recovery bindings: another count".into());
    }
    for (i, (ra, ro)) in ea.recovery_bindings.iter().zip(eo.recovery_bindings.iter_mut()).enumerate() {
        if borsh::to_vec(ra).unwrap().len() != borsh::to_vec(ro).unwrap().len() {
            return Err(format!("recovery binding {i}: another size"));
        }
        *ro = ra.clone();
    }
    if !ea.recovery_bindings.is_empty() {
        log.push(format!("{} recovery binding(s) of the fee refund (fresh; same sizes)", ea.recovery_bindings.len()));
    }
    // The fee: the same refund target; its fallback (the payer's transition after the fee
    // transfer alone) is a private action of its own and the payer alias's crossing.
    match (&ea.fee, &mut eo.fee) {
        (None, None) => {}
        (Some(fa), Some(fo)) => {
            if fa.refund_to != fo.refund_to {
                return Err("the fee's refund target differs".into());
            }
            match (&fa.fallback, &mut fo.fallback) {
                (None, None) => {}
                (Some(xa), Some(xo)) => {
                    align_action("the fee fallback's private action", &xa.action, &mut xo.action)?;
                    align_boundary(
                        "the fee fallback's boundary",
                        &xa.boundary,
                        &mut xo.boundary,
                        &publics,
                        claims,
                        &mut log,
                    )?;
                    log.push(
                        "the fee fallback's private action: the account's own nullifier and commitment (set by the \
                         private state spent and made) and encrypted note (fresh); same sizes, same root"
                            .to_string(),
                    );
                }
                _ => return Err("the fee fallback is in one transaction only".into()),
            }
        }
        _ => return Err("the fee is in one transaction only".into()),
    }
    let (ba, bo) = (borsh::to_vec(&a.message).unwrap(), borsh::to_vec(&out.message).unwrap());
    if ba != bo {
        let at = ba.iter().zip(&bo).position(|(x, y)| x != y).unwrap_or(ba.len().min(bo.len()));
        let (ma, mo) = (&a.message, &out.message);
        let (xa, xo) = (&ma.execution, &mo.execution);
        let fields: [(&str, Vec<u8>, Vec<u8>); 11] = [
            ("public_context", borsh::to_vec(&ma.public_context).unwrap(), borsh::to_vec(&mo.public_context).unwrap()),
            ("execution.boundary", borsh::to_vec(&xa.boundary).unwrap(), borsh::to_vec(&xo.boundary).unwrap()),
            (
                "execution.async_messages",
                borsh::to_vec(&xa.async_messages).unwrap(),
                borsh::to_vec(&xo.async_messages).unwrap(),
            ),
            (
                "execution.recovery_bindings",
                borsh::to_vec(&xa.recovery_bindings).unwrap(),
                borsh::to_vec(&xo.recovery_bindings).unwrap(),
            ),
            ("execution.public_root", borsh::to_vec(&xa.public_root).unwrap(), borsh::to_vec(&xo.public_root).unwrap()),
            (
                "execution.private_actions",
                borsh::to_vec(&xa.private_actions).unwrap(),
                borsh::to_vec(&xo.private_actions).unwrap(),
            ),
            ("execution.validity", borsh::to_vec(&xa.validity).unwrap(), borsh::to_vec(&xo.validity).unwrap()),
            (
                "execution.program_image_claims",
                borsh::to_vec(&xa.program_image_claims).unwrap(),
                borsh::to_vec(&xo.program_image_claims).unwrap(),
            ),
            ("execution.fee", borsh::to_vec(&xa.fee).unwrap(), borsh::to_vec(&xo.fee).unwrap()),
            ("nonces", borsh::to_vec(&ma.nonces).unwrap(), borsh::to_vec(&mo.nonces).unwrap()),
            (
                "admission_evidence",
                borsh::to_vec(&ma.admission_evidence).unwrap(),
                borsh::to_vec(&mo.admission_evidence).unwrap(),
            ),
        ];
        let field = fields.iter().find(|(_, x, y)| x != y).map_or("an unnamed field", |(name, _, _)| name);
        return Err(format!(
            "the messages differ at byte {at} of {} / {} after alignment, in `{field}`",
            ba.len(),
            bo.len()
        ));
    }
    Ok((out, log))
}

/// Align one boundary of `d` to `a`'s, step by step: an actor may differ only where it is the
/// claim's (`claims`) or on the private side (in neither transaction's public context, so a
/// fresh alias); a message only as `Spend`'s nullifier, each transaction's own N; PDA seeds only
/// as the seed of each transaction's own `claim(pool, N)`; inherited authorizations only at equal
/// sizes, and only where neither names a public account (an authorization of the private side).
fn align_boundary(
    label: &str,
    ba: &[BoundaryStep],
    bo: &mut [BoundaryStep],
    publics: &BTreeSet<Actor>,
    claims: Option<Claims>,
    log: &mut Vec<String>,
) -> Result<(), String> {
    if ba.len() != bo.len() {
        return Err(format!("{label}: {} steps against {}", ba.len(), bo.len()));
    }
    for (i, (sa, so)) in ba.iter().zip(bo.iter_mut()).enumerate() {
        let (da, dd) = match (sa, so) {
            (BoundaryStep::PrivateToPublic(x), BoundaryStep::PrivateToPublic(y))
            | (BoundaryStep::PublicToPrivate(x), BoundaryStep::PublicToPrivate(y)) => (x, y),
            (x, y) if x == y => continue,
            _ => return Err(format!("{label} step {i}: another kind of step")),
        };
        for (which, xa, xd) in
            [("from", da.envelope.from, &mut dd.envelope.from), ("to", da.envelope.to, &mut dd.envelope.to)]
        {
            if xa == *xd {
                continue;
            }
            let claim = claims.is_some_and(|c| xa == c.actor.0 && *xd == c.actor.1);
            let private_side = !publics.contains(&xa) && !publics.contains(xd);
            if !(claim || private_side) {
                return Err(format!("{label} step {i}: `{which}` differs"));
            }
            *xd = xa;
            log.push(format!(
                "{label} step {i}: `{which}` ({})",
                if claim { "claim(pool, N)" } else { "an alias of the private account (fresh)" }
            ));
        }
        if da.envelope.message != dd.envelope.message {
            let (ma, md) = (
                ConsumerMessage::try_from_slice(&da.envelope.message),
                ConsumerMessage::try_from_slice(&dd.envelope.message),
            );
            match (ma, md) {
                (
                    Ok(ConsumerMessage::Spend { pool_id: pa, nullifier: na, credit: ra }),
                    Ok(ConsumerMessage::Spend { pool_id: pd, nullifier: nd, credit: rd }),
                ) if pa == pd
                    && ra == rd
                    && claims.is_some_and(|c| (na, nd) == c.nullifier)
                    && da.envelope.message.len() == dd.envelope.message.len() =>
                {
                    dd.envelope.message = da.envelope.message.clone();
                    log.push(format!("{label} step {i}: `Spend`'s nullifier N"));
                }
                _ => {
                    return Err(format!(
                        "{label} step {i}: the message differs ({} bytes against {})",
                        da.envelope.message.len(),
                        dd.envelope.message.len()
                    ))
                }
            }
        }
        if da.pda_seeds != dd.pda_seeds {
            let own = claims.is_some_and(|c| {
                da.pda_seeds == BTreeSet::from([c.seed.0]) && dd.pda_seeds == BTreeSet::from([c.seed.1])
            });
            if !own {
                return Err(format!("{label} step {i}: PDA seeds"));
            }
            dd.pda_seeds = da.pda_seeds.clone();
            log.push(format!("{label} step {i}: the PDA seed of claim(pool, N)"));
        }
        if da.inherited_authorizations != dd.inherited_authorizations {
            if da.inherited_authorizations.len() != dd.inherited_authorizations.len() {
                return Err(format!("{label} step {i}: inherited authorizations"));
            }
            if names_public(&da.inherited_authorizations, publics)
                || names_public(&dd.inherited_authorizations, publics)
            {
                return Err(format!("{label} step {i}: an inherited authorization names a public account"));
            }
            dd.inherited_authorizations = da.inherited_authorizations.clone();
            log.push(format!("{label} step {i}: inherited authorizations (the private account's alias)"));
        }
    }
    Ok(())
}

/// Align one private action of `d` to `a`'s: the same commitment root, a note of the same size;
/// the note is fresh, the nullifier and commitment fresh for a dummy and set by the state spent and
/// made for the account's own.
fn align_action(label: &str, pa: &PrivateAction, po: &mut PrivateAction) -> Result<(), String> {
    if pa.root != po.root {
        return Err(format!("{label}: another commitment root"));
    }
    if borsh::to_vec(&pa.encrypted_post_state).unwrap().len() != borsh::to_vec(&po.encrypted_post_state).unwrap().len()
    {
        return Err(format!("{label}: a note of another size"));
    }
    po.nullifier = pa.nullifier;
    po.commitment = pa.commitment;
    po.encrypted_post_state = pa.encrypted_post_state.clone();
    Ok(())
}

/// True if `ids` names the account of an actor of the public context.
fn names_public(ids: &BTreeSet<AccountId>, publics: &BTreeSet<Actor>) -> bool {
    ids.iter().any(|id| publics.iter().any(|p| p.account_id == *id))
}

/// What `align` takes from one transaction must not carry the account's identity: `t1` and `t2`
/// are the same claim by the same account, proven twice from the same state. Every alias on the
/// private side (in the boundary and the fee fallback's), every inherited authorization, encrypted
/// note, recovery binding and dummy action must differ between them, and no inherited
/// authorization may name a public account; the account's own nullifier and new commitments are
/// the values the same in both (one nullifier: the state spent, by the action and by the fee
/// fallback; one commitment for each state made), so spending the same state twice is refused
/// (R.11's control). A per-account deterministic alias (`Shape::Bare`'s) fails this check, though
/// `align` alone would accept it between two accounts. Returns what was checked.
fn fresh_check(
    t1: &PrivacyPreservingTransaction,
    t2: &PrivacyPreservingTransaction,
    publics: &BTreeSet<Actor>,
) -> Result<Vec<String>, String> {
    let (e1, e2) = (&t1.message.execution, &t2.message.execution);
    let fallback = |e: &lee_core::ProvenExecution| e.fee.as_ref().and_then(|f| f.fallback.clone());
    let (f1, f2) = (fallback(e1), fallback(e2));
    if f1.is_some() != f2.is_some() {
        return Err("the fee fallback is in one proof only".into());
    }
    let mut boundaries = vec![("boundary", e1.boundary.clone(), e2.boundary.clone())];
    if let (Some(x1), Some(x2)) = (&f1, &f2) {
        boundaries.push(("the fee fallback's boundary", x1.boundary.clone(), x2.boundary.clone()));
    }
    let (mut aliases, mut authorizations) = (0usize, 0usize);
    for (label, b1, b2) in &boundaries {
        if b1.len() != b2.len() {
            return Err(format!("{label}: {} steps against {}", b1.len(), b2.len()));
        }
        for (i, (s1, s2)) in b1.iter().zip(b2).enumerate() {
            let (d1, d2) = match (s1, s2) {
                (BoundaryStep::PrivateToPublic(x), BoundaryStep::PrivateToPublic(y))
                | (BoundaryStep::PublicToPrivate(x), BoundaryStep::PublicToPrivate(y)) => (x, y),
                (x, y) if x == y => continue,
                _ => return Err(format!("{label} step {i}: another kind of step")),
            };
            for (which, a1, a2) in
                [("from", d1.envelope.from, d2.envelope.from), ("to", d1.envelope.to, d2.envelope.to)]
            {
                let private_side = !publics.contains(&a1) && !publics.contains(&a2);
                if private_side && a1 == a2 {
                    return Err(format!(
                        "{label} step {i}: the alias `{which}` is the same in two proofs of one claim"
                    ));
                }
                if !private_side && a1 != a2 {
                    return Err(format!("{label} step {i}: the public actor `{which}` differs"));
                }
                aliases += usize::from(private_side);
            }
            if names_public(&d1.inherited_authorizations, publics)
                || names_public(&d2.inherited_authorizations, publics)
            {
                return Err(format!("{label} step {i}: an inherited authorization names a public account"));
            }
            if d1.inherited_authorizations.len() != d2.inherited_authorizations.len() {
                return Err(format!("{label} step {i}: inherited authorizations of another count"));
            }
            if d1.inherited_authorizations.intersection(&d2.inherited_authorizations).next().is_some() {
                return Err(format!("{label} step {i}: an inherited authorization repeats in two proofs of one claim"));
            }
            authorizations += d1.inherited_authorizations.len();
        }
    }
    let actions = |e: &lee_core::ProvenExecution, f: &Option<lee_core::FeeFallback>| -> Vec<PrivateAction> {
        e.private_actions.iter().cloned().chain(f.iter().map(|x| x.action.clone())).collect()
    };
    let (p1, p2) = (actions(e1, &f1), actions(e2, &f2));
    if p1.len() != p2.len() {
        return Err(format!("{} private actions against {}", p1.len(), p2.len()));
    }
    let note = |a: &PrivateAction| borsh::to_vec(&a.encrypted_post_state).unwrap();
    if let Some(i) = (0..p1.len()).find(|&i| p2.iter().any(|b| note(b) == note(&p1[i]))) {
        return Err(format!("private action {i}: its encrypted note repeats in two proofs of one claim"));
    }
    // The account's own: the actions whose nullifier is in both proofs.
    let own: Vec<&PrivateAction> = p1.iter().filter(|a| p2.iter().any(|b| b.nullifier == a.nullifier)).collect();
    let mut spent: Vec<_> = own.iter().map(|a| borsh::to_vec(&a.nullifier).unwrap()).collect();
    spent.sort();
    spent.dedup();
    if spent.len() != 1 {
        return Err(format!("{} nullifiers repeat in two proofs of one claim, not the one state spent", spent.len()));
    }
    let made = p1.iter().filter(|a| p2.iter().any(|b| b.commitment == a.commitment)).count();
    if made != own.len() {
        return Err(format!(
            "{made} commitments repeat in two proofs of one claim, against {} actions of the account's own",
            own.len()
        ));
    }
    let dummies = p1.len() - own.len();
    let bindings = e1.recovery_bindings.len();
    if bindings != e2.recovery_bindings.len() {
        return Err("recovery bindings: another count".into());
    }
    for (i, (r1, r2)) in e1.recovery_bindings.iter().zip(&e2.recovery_bindings).enumerate() {
        if borsh::to_vec(r1).unwrap() == borsh::to_vec(r2).unwrap() {
            return Err(format!("recovery binding {i} repeats in two proofs of one claim"));
        }
    }
    Ok(vec![
        format!("{aliases} alias(es) on the private side (the boundary's and the fee fallback's): each different"),
        if authorizations == 0 {
            "inherited authorizations: none in either proof".to_string()
        } else {
            format!("{authorizations} inherited authorization(s): each different, none naming a public account")
        },
        format!("{} encrypted notes: each different", p1.len()),
        format!("{dummies} dummy actions: nullifier and commitment each different"),
        format!("{bindings} recovery binding(s): each different"),
        format!(
            "the account's own {} action(s) ({}): one nullifier, the private state spent, and {made} commitment(s), the \
             states made, the same in both; so spending that state a second time is refused",
            own.len(),
            if f1.is_some() { "the claim's and the fee fallback's" } else { "the claim's" }
        ),
    ])
}

#[test]
fn f8_two_deposits_full_diff() {
    require_dev_mode();
    let dep = deposits();
    let s0 = dep.s.w.chain.clone();
    let before = public_accounts(&s0);
    let program = consumer_account();
    let claims = Claims::new(program, &dep.pool.id(), &dep.a.3, &dep.d.3);
    let (ca, cd) = claims.actor;

    // The two private claims, each from the same state.
    let (mut chain_a, mut chain_d) = (s0.clone(), s0.clone());
    let ra = dep.claim(&mut chain_a, &dep.x, &dep.a);
    let rd = dep.claim(&mut chain_d, &dep.y, &dep.d);
    assert!(ra.charge.revert.is_none() && rd.charge.revert.is_none());
    assert!(consumer_state(&chain_a, ca).is_some(), "N_A is spent");

    let mut out = format!(
        "# F.8: the full-diff test (two deposits, no hop and three hops)\n\n\
         Generated by `RISC0_DEV_MODE=1 cargo test -p harness --locked --test security_privacy f8` ({}; LEZ's state \
         machine and settlement in process, dev mode: the private part is executed, not proven). A synthetic \
         mainnet-preset light client (committees the test holds; every update also through Ferrata's policy store, \
         `FinalizedStore`, which must give the same verdict) \
         records block B = {}. Deposit A (0.001 ETH, private-kind, to account X) is in B; deposit D (0.001 ETH, \
         private-kind, to account Y) is in block {}, three EIP-2935 hops back (B → {} → {} → {}). Each is claimed by the \
         reference consumer's `ClaimDepositPrivately` (it reads the record, verifies the deposit and its hops and opens \
         C inside its own execution, and spends N publicly), proven in the wallet's shape (padded, fresh randomness), \
         from the same state, against the same record.\n\n",
        harness::LEZ_TAG,
        dep.blocks[0],
        dep.blocks[3],
        dep.blocks[1],
        dep.blocks[2],
        dep.blocks[3]
    );

    // 1. The transactions.
    let (ta, td) = (LeeTransaction::PrivacyPreserving(ra.tx.clone()), LeeTransaction::PrivacyPreserving(rd.tx.clone()));
    let (bytes_a, bytes_d) = (borsh::to_vec(&ta).unwrap(), borsh::to_vec(&td).unwrap());
    assert_eq!(bytes_a.len(), bytes_d.len(), "the same transaction size");
    assert_eq!(ra.proof_bytes, rd.proof_bytes, "the same proof size");
    let (_, log) = align(&ra.tx, &rd.tx, Some(claims)).unwrap_or_else(|e| panic!("the two claims differ: {e}"));
    let differing = bytes_a.iter().zip(&bytes_d).filter(|(x, y)| x != y).count();
    // What was taken is fresh, or the account's own per-state value: X's claim of A proven a second
    // time from the same state.
    let ra2 = dep.claim(&mut s0.clone(), &dep.x, &dep.a);
    let fresh = fresh_check(&ra.tx, &ra2.tx, &ra.tx.message.public_context.actors)
        .unwrap_or_else(|e| panic!("a field align takes is neither fresh nor the account's own per-state value: {e}"));
    out.push_str(&format!(
        "## The transactions\n\nBoth are {} bytes (proof field {} bytes). {} of those bytes differ. After taking from A every \
         field that may differ — each with the same encoded size in both — the two messages are byte-identical. The fields \
         taken:\n\n{}\n\nThose fields are fresh randomness or the account's own per-state values (LEZ's nullifier and \
         commitments, never shown twice): X's claim of A proven a second time from the same state (the same account, the \
         same state, the same claim) gives, against the first:\n\n{}\n\nSo the aliases, notes, recovery bindings and \
         dummies are fresh randomness, and the account's own nullifier and commitments are determined by the private \
         state spent and made: never shown twice, since a state is spent once, but computed from that state, so that \
         they reveal nothing of the account rests on LEZ's nullifier and commitment scheme, which this check does not \
         test.\n\n\
         The proof field differs as two proofs of different statements do; its size is the same. Here the proofs are \
         dev-mode fake receipts; a real private transaction carries LEZ's succinct receipt, of fixed seal size whatever the \
         statement (`ProverOpts::succinct()`, rc1 `lee/state_machine/src/privacy_preserving_transaction/circuit/mod.rs` \
         line 655), and LEZ charges every private transaction the same flat proof storage (`PROOF_BYTES` = 223,551 of the \
         224,063 storage gas shown below, rc1 `lez/programs/fee/core/src/market.rs`).\n\n",
        grouped(bytes_a.len()),
        grouped(ra.proof_bytes),
        grouped(differing),
        log.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n"),
        fresh.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n")
    ));

    // 2. What neither transaction contains.
    let mut searched = String::new();
    for (name, bytes, account, (secret, deposit, id, _)) in
        [("A", &bytes_a, &dep.x, &dep.a), ("D", &bytes_d, &dep.y, &dep.d)]
    {
        let mut needles: Vec<(String, Vec<u8>)> = vec![
            ("the secret s".into(), secret.to_vec()),
            ("the statement identifier".into(), id.to_vec()),
            ("the destination (the claimant's account id)".into(), account.id().value().to_vec()),
            ("the vault's address".into(), VAULT.to_vec()),
            ("the receipt proof's last node".into(), deposit.proof.last().unwrap().clone()),
        ];
        for (k, hop) in deposit.hops.iter().enumerate() {
            needles.push((format!("hop {k}'s block header"), hop.header.clone()));
        }
        for (what, needle) in needles {
            let found = occurs(bytes, &needle);
            assert!(!found, "{name}: {what} is in the transaction");
            searched.push_str(&format!("- {name}: {what} ({} bytes): absent\n", needle.len()));
        }
    }
    out.push_str(&format!("Searched in each transaction's bytes:\n\n{searched}\n"));

    // 3. Events and charges: the same, once D's N and claim address read as A's.
    assert_eq!(ra.events.len(), rd.events.len(), "the same number of events");
    let mut n_in_events = 0;
    for (ea, ed) in ra.events.iter().zip(&rd.events) {
        let account_same =
            ea.account_id == ed.account_id || (ea.account_id == ca.account_id && ed.account_id == cd.account_id);
        let mut data = ed.event.data.clone();
        n_in_events += replace_all(&mut data, &dep.d.3, &dep.a.3);
        n_in_events += replace_all(&mut data, cd.account_id.value(), ca.account_id.value());
        assert!(
            account_same && ea.event.selector == ed.event.selector && ea.event.data == data,
            "the events differ: {ea:?} against {ed:?}"
        );
    }
    assert_eq!(ra.charge, rd.charge, "the same charge");
    out.push_str(&format!(
        "## Events and charges\n\nEvents: {} in each, identical once D's N reads as A's ({n_in_events} occurrence(s) of N or its address). Charge: identical (execution gas {}, of which public part {} cycles; storage gas {}; base fee {}), so the three hops inside D's proof cost nothing visible.\n\n",
        ra.events.len(),
        grouped(ra.charge.gas_exec as usize),
        grouped(ra.charge.cycles as usize),
        grouped(ra.charge.gas_stor as usize),
        ra.charge.fee_base
    ));

    // 4. The state diffs.
    let (after_a, after_d) = (public_accounts(&chain_a), public_accounts(&chain_d));
    let (diff_a, diff_d) = (diff(&before, &after_a), diff(&before, &after_d));
    assert_eq!(diff_a.len(), diff_d.len(), "the same number of public accounts change");
    // Keyed by account, D's claim(pool, N) read as A's.
    let keyed = |d: &[(AccountId, Option<Account>, Option<Account>)], from: AccountId, to: AccountId| {
        d.iter()
            .map(|(id, b, a)| (if *id == from { to } else { *id }, (b.clone(), a.clone())))
            .collect::<BTreeMap<AccountId, (Option<Account>, Option<Account>)>>()
    };
    let (ka, kd) = (keyed(&diff_a, ca.account_id, ca.account_id), keyed(&diff_d, cd.account_id, ca.account_id));
    assert!(
        ka.contains_key(&ca.account_id) && diff_d.iter().any(|(id, _, _)| *id == cd.account_id),
        "each claim writes its N's account"
    );
    assert_eq!(ka, kd, "the same public accounts change, the same way");
    let mut rows = String::from("| Public account | Before | After (A) | Same in D |\n|---|---|---|---|\n");
    for (ida, (ba, aa)) in &ka {
        let is_claim = *ida == ca.account_id;
        let name = if is_claim {
            "claim(pool, N) — its address is N's".to_string()
        } else if *ida == system_accounts::fee_inbox_account_id() {
            "the fee inbox".to_string()
        } else if *ida == system_accounts::fee_state_account_id() {
            "the fee state".to_string()
        } else {
            format!("{ida}")
        };
        let show = |a: &Option<Account>| match a {
            None => "absent".to_string(),
            Some(a) => format!("nonce {:?}, {} bytes of data", a.nonce, borsh::to_vec(&a.data).unwrap().len()),
        };
        rows.push_str(&format!("| {name} | {} | {} | yes |\n", show(ba), show(aa)));
    }
    assert_eq!(consumer_state(&chain_a, ca), consumer_state(&chain_d, cd), "the claim actors hold the same state");
    assert!(matches!(
        consumer_state(&chain_a, ca),
        Some(ferrata_lez_wire::consumer::ConsumerState::Nullifier(NullifierState::Spent { .. }))
    ));
    let (state_a, state_d) =
        (borsh::to_vec(&chain_a.state).unwrap().len(), borsh::to_vec(&chain_d.state).unwrap().len());
    assert_eq!(state_a, state_d, "the whole states encode to the same size");
    for (chain, run) in [(&chain_a, &ra), (&chain_d, &rd)] {
        for action in &run.tx.message.execution.private_actions {
            assert!(chain.state.is_spent(&action.nullifier), "each action's nullifier is spent");
            assert!(
                chain.state.get_proof_for_commitment(&action.commitment).is_some(),
                "each new commitment is in the tree"
            );
        }
    }
    out.push_str(&format!(
        "## The state diffs\n\nEvery public account of the state, before and after each claim (decoded from the state's own encoding):\n\n{rows}\n\
         Nothing else of the public state changes. The private state gains, for each, its {} private actions' nullifiers and commitments (the dummies' fresh, the account's own per state); the whole state encodes to {} bytes after either claim. The record is read, never written; no account of X, Y, the depositors or the vault exists on LEZ.\n\n",
        ra.tx.message.execution.private_actions.len(),
        grouped(state_a)
    ));

    // 5. The control: a private consumer that calls Attest instead of reading fails the same test.
    let (mut call_a, mut call_d) = (s0.clone(), s0.clone());
    let qa = dep.call(&mut call_a, &dep.x, &dep.a);
    let qd = dep.call(&mut call_d, &dep.y, &dep.d);
    let failure = align(&qa.tx, &qd.tx, None).map(|_| ()).expect_err("calling must fail the comparison");
    let (call_bytes_a, call_bytes_d) = (
        borsh::to_vec(&LeeTransaction::PrivacyPreserving(qa.tx.clone())).unwrap(),
        borsh::to_vec(&LeeTransaction::PrivacyPreserving(qd.tx.clone())).unwrap(),
    );
    let leaked = dep.d.1.hops.iter().all(|h| occurs(&call_bytes_d, &h.header))
        && occurs(&call_bytes_d, dep.d.1.proof.last().unwrap());
    assert!(leaked, "the call publishes D's hops and receipt proof");
    out.push_str(&format!(
        "## Control: a consumer that calls instead of reading\n\nThe same two deposits, the same consumer, `CallAttest` (the request crosses into public execution): the transactions are {} and {} bytes, and the comparison fails: {failure}. D's three hop headers and its receipt proof are in D's transaction.\n",
        grouped(call_bytes_a.len()),
        grouped(call_bytes_d.len())
    ));
    write("f8-full-diff.md", &out);
    println!("{out}");
}

// ---------------------------------------------------------------------------------------------
// R.11
// ---------------------------------------------------------------------------------------------

/// Settle `txs` in one block: the test's own block loop around the harness's copy of rc1's
/// per-transaction settlement (`harness::settle::settle_private_transaction`, rc1's
/// `settle_private_transaction`), with one opening fee state, one fee summary, one block id and
/// timestamp, each transaction in order; a transaction whose settlement fails is dropped, as the
/// sequencer's block builder drops it (settled on a scratch copy first). rc1's block function
/// (`chain_state::apply::apply_block_to_state`: the block's clock and fee transactions, the
/// reverted list) and the sequencer's admission screen are not run here; the same kind of block
/// made by rc3's own sequencer is recorded in `lez/results/v0.4.0-rc3/r11-one-block/`.
/// Returns each transaction's result.
fn settle_block(
    chain: &mut Chain,
    txs: &[LeeTransaction],
    timestamp: u64,
) -> Vec<Result<harness::settle::Charge, String>> {
    let opening = opening_fee_state(&chain.state);
    let mut summary = BlockFeeSummary::default();
    let block_id = chain.block_id;
    let mut out = Vec::new();
    for tx in txs {
        let mut state = chain.state.clone();
        let mut s = summary;
        let result = classify(tx)
            .and_then(|view| settle_private_transaction(tx, &view, &mut state, &opening, block_id, timestamp, &mut s));
        match result {
            Ok((_, charge)) => {
                chain.state = state;
                summary = s;
                out.push(Ok(charge));
            }
            Err(e) => out.push(Err(e)),
        }
    }
    chain.block_id += 1;
    out
}

#[test]
fn r11_several_private_reads_in_one_block() {
    require_dev_mode();
    let slot0 = EXPECTED_FINALIZED[0].0 + 64;
    let mut s = Synth::new(mainnet_config(2), 0x91, slot0, filler_header(slot0), 2);
    let token: Address = [0x7e; 20];
    let recipient: Address = [0x7f; 20];
    let senders: Vec<Address> = (1..=4u8).map(|i| [i; 20]).collect();
    let receipts: Vec<Vec<u8>> = senders
        .iter()
        .enumerate()
        .map(|(i, from)| transfer_receipt(token, *from, recipient, 1_000 + i as u128))
        .collect();
    let b = Block::new(30_000_000, &[AccountSpec::eoa([0x33; 20], 5)], &receipts);
    let slot = slot0 + 64;
    let u = s.update(0, Some(header_of(slot, &b)), filler_header(slot0 + 96), None, slot0 + 97, 512);
    assert!(s.w.apply(&u, "the record of block B").reports.iter().all(harness::TxReport::ok));
    let (program, id, chain_id) = (s.w.inst.program, s.w.inst.id, s.w.inst.config.chain_id);
    let pool = Pool { ferrata: program, instance_id: id, chain_id, token, recipient };
    let record = header_actor(program, &id, slot);
    let accounts: Vec<PrivateAccount> =
        (0..4u8).map(|i| s.w.chain.add_private_account(0x91 + i, 1_000_000_000_000)).collect();
    let snapshot = s.w.chain.clone();
    let time = s.time(&u) + 60_000;

    // Each account proves its private attestation of one Transfer against the same snapshot; the
    // transaction (with every output it will have) exists before any of them is included.
    let prove = |account: &PrivateAccount, i: u64| -> PrivacyPreservingTransaction {
        let message = ConsumerMessage::AttestPrivately {
            pool: pool.clone(),
            slot,
            tx_index: i,
            log_index: 0,
            proof: b.receipt_proof(i),
            header: None,
        };
        let mut scratch = snapshot.clone();
        scratch
            .prove_and_settle_private_as(
                account,
                Actor::new(account.id(), consumer_account()),
                borsh::to_vec(&message).unwrap(),
                vec![record],
                &catalog(),
                RESERVE,
                time,
                Shape::Wallet,
            )
            .unwrap_or_else(|e| panic!("proves: {e}"))
            .tx
    };
    let txs: Vec<PrivacyPreservingTransaction> = accounts.iter().enumerate().map(|(i, a)| prove(a, i as u64)).collect();
    let root = snapshot.state.commitment_root();
    for tx in &txs {
        assert!(
            tx.message.execution.private_actions.iter().all(|a| a.root == root),
            "every proof is against the snapshot's commitment root"
        );
    }

    // Between proving and inclusion the instance moves on: a later finality update is verified and
    // writes a new record; the record read is untouched.
    let later = s.update(0, Some(filler_header(slot0 + 160)), filler_header(slot0 + 192), None, slot0 + 193, 512);
    assert!(s
        .w
        .apply(&later, "a later finality update, between proving and inclusion")
        .reports
        .iter()
        .all(harness::TxReport::ok));
    assert_ne!(s.w.inst.store(&s.w.chain), snapshot_store(&snapshot, &s), "the instance changed");

    // The four in one block.
    let record_before = s.w.chain.actor_state(record);
    let block: Vec<LeeTransaction> = txs.iter().cloned().map(LeeTransaction::PrivacyPreserving).collect();
    let results = settle_block(&mut s.w.chain, &block, time + 12_000);
    let mut rows = String::from("| Transaction | Settled | Charge (execution gas, storage gas, base fee) | Outputs present after the block |\n|---|---|---|---|\n");
    for (i, (tx, result)) in txs.iter().zip(&results).enumerate() {
        let charge = result.as_ref().unwrap_or_else(|e| panic!("transaction {i} of the block: {e}"));
        assert!(charge.revert.is_none(), "transaction {i}: {:?}", charge.revert);
        let actions = &tx.message.execution.private_actions;
        assert!(actions.iter().all(|a| s.w.chain.state.is_spent(&a.nullifier)));
        assert!(actions.iter().all(|a| s.w.chain.state.get_proof_for_commitment(&a.commitment).is_some()));
        rows.push_str(&format!(
            "| private attestation {i} (Transfer of tx {i}, account {}) | yes | {}, {}, {} | {} nullifiers spent, {} commitments in the tree, as proven |\n",
            i + 1,
            grouped(charge.gas_exec as usize),
            grouped(charge.gas_stor as usize),
            charge.fee_base,
            actions.len(),
            actions.len()
        ));
    }
    assert_eq!(s.w.chain.actor_state(record), record_before, "reads write nothing");

    // Control: one account's two transactions proven against the same snapshot conflict.
    let mut c = snapshot.clone();
    let twice = [prove(&accounts[0], 0), prove(&accounts[0], 1)].map(LeeTransaction::PrivacyPreserving);
    let control = settle_block(&mut c, &twice, time + 12_000);
    assert!(control[0].is_ok(), "{:?}", control[0]);
    let refused = control[1].as_ref().expect_err("the second spends the same private account state");

    let out = format!(
        "# R.11: several private statements in one block\n\n\
         Generated by `RISC0_DEV_MODE=1 cargo test -p harness --locked --test security_privacy r11` ({}; LEZ's state \
         machine and settlement in process, dev mode). A synthetic mainnet-preset light client records block B with four \
         Transfers. Four private accounts each prove, against the same snapshot of the state, the reference consumer's \
         private attestation of one of them (`AttestPrivately`: it reads the record, verifies the Transfer inside its own \
         execution and keeps the result in its private state). Every transaction is complete — its nullifiers, its new \
         commitments, its predicted reply from the record — before any is included. Then a later finality update is \
         verified and applied (the instance's store and a new record change), and the four transactions are settled \
         together in one block by the test's own block loop around the harness's copy of rc1's per-transaction \
         settlement (one opening fee state, one fee summary, one block id and timestamp; rc1's block function, with the \
         block's clock and fee transactions and its reverted list, and the sequencer's admission screen are not run \
         here). The same kind of block made by LEZ v0.4.0-rc3's own sequencer is recorded in \
         `lez/results/v0.4.0-rc3/r11-one-block/`.\n\n{rows}\n\
         None was re-proven; the record they read is unchanged (reads write nothing); LEZ keeps every past commitment root, \
         so proofs against the snapshot settle after the state moved. Control: one account's two transactions proven \
         against the same snapshot — the second is dropped: `{refused}`.\n",
        harness::LEZ_TAG
    );
    write("r11-one-block.md", &out);
    println!("{out}");
}

/// The instance's store in `snapshot`.
fn snapshot_store(snapshot: &Chain, s: &Synth) -> ferrata_core::witness::CompactStore {
    s.w.inst.store(snapshot)
}

/// Replace every occurrence of `from` in `bytes` by `to` (same length); returns how many.
fn replace_all(bytes: &mut [u8], from: &[u8], to: &[u8]) -> usize {
    assert_eq!(from.len(), to.len());
    let mut count = 0;
    let mut i = 0;
    while i + from.len() <= bytes.len() {
        if &bytes[i..i + from.len()] == from {
            bytes[i..i + from.len()].copy_from_slice(to);
            count += 1;
            i += from.len();
        } else {
            i += 1;
        }
    }
    count
}

fn grouped(n: usize) -> String {
    harness::grouped(n as u64)
}
