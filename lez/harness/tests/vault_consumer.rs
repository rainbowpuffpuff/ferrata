//! The reference consumer with the Recommended Consumer Pattern (design §7, rules 1 to 7) on the
//! real deposits of FerrataVault v5 on Sepolia (`e2e/runs/crown-live-1`): the deposit is the
//! vault's `Deposit` log, one EIP-2935 hop back from the record the instance wrote (a Gloas
//! record), its commitment `C` opens with the depositor's secret `s`, and the nullifier is
//! `N = H(s ‖ statement identifier)`. A public claim opens `C` in the clear; a private claim reads
//! the record and proves the opening inside its own execution, publishing `N` only. One deposit
//! is claimed once, whichever path comes first.
//!
//! The deposits' secrets are the run's: `s = HMAC(kAuth, "ferrata/s/v1" ‖ chain ‖ vault ‖
//! depositor ‖ j)` from the test seed `sha256("e2e: user bridge seed (test value)")`
//! (`e2e/crown/identity.py`). Deposit 0 is public-kind (to LEZ's public test account 0), deposit
//! 1 private-kind (to LEZ's private test account 0, whose keys this harness does not hold).

use ferrata_bridge_core::{inbound::deposit_secret, sha256, BridgeKeys};
use ferrata_lez_wire::consumer::{
    claim_actor, claim_seed, code, commitment, holder_actor, secret_nullifier, ClaimantState, ConsumerMessage,
    ConsumerState, HolderState, NullifierState, VaultPool, KIND_PRIVATE, KIND_PUBLIC,
};
use ferrata_spectests::execkit::word;
use harness::{
    attest::{consumer_state, execute, render_executed, Executed},
    private::*,
    render,
    sepolia::SEPOLIA_CHAIN_ID,
    settle::LeeTransaction,
    vault::RealVault,
    *,
};
use lee::{Actor, PublicAccountEvidence};
use lee_core::program::{ReceiveInput, StateReply};

/// 0.001 ETH, each deposit's amount.
const AMOUNT: u128 = 1_000_000_000_000_000;

fn require_dev_mode() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
}

fn occurs(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Each deposit's opening (s, kind, dest), checked against the commitment on chain.
fn openings(v: &RealVault) -> Vec<([u8; 32], u8, [u8; 32])> {
    let keys = BridgeKeys::from_bridge_seed(&sha256(b"e2e: user bridge seed (test value)"));
    let chain = ferrata_bridge_core::chain_id_u256(SEPOLIA_CHAIN_ID);
    let dests = [
        (KIND_PUBLIC, word("0x54d662b664519cb8aef608ae84a63e041cc16019e4a07c34828311996f0c817a")),
        (KIND_PRIVATE, word("0x2d6cb849edc6d8b1599c558ecd87bc09b4420bdc60f1b3be85547a5834b153fc")),
    ];
    v.deposits
        .iter()
        .zip(dests)
        .enumerate()
        .map(|(j, (d, (kind, dest)))| {
            // Both deposits are the deployer's: j is the leaf index.
            let s = deposit_secret(&keys.auth, &chain, &v.vault, &d.deposit.depositor, j as u64);
            assert_eq!(commitment(&s, kind, &dest), d.deposit.commitment, "deposit {j}: the run's opening of C");
            (s, kind, dest)
        })
        .collect()
}

/// The core's code for deposit `i` asked of `pool` (the program's verdict, natively).
fn native(v: &RealVault, pool: &VaultPool, deposit: &ferrata_lez_wire::consumer::DepositProof) -> u32 {
    let request = pool.request(deposit);
    let code = ferrata_lez_wire::attest::evaluate(&v.w.inst.id, SEPOLIA_CHAIN_ID, &v.record, &request).err().expect("refused");
    u32::from(code)
}

/// The native-ETH pool of the vault, bound to the instance.
fn pool(v: &RealVault) -> VaultPool {
    VaultPool { ferrata: v.w.inst.program, instance_id: v.w.inst.id, chain_id: SEPOLIA_CHAIN_ID, vault: v.vault, token: [0; 20] }
}

/// `ClaimDeposit` of deposit `i`, sent to `claim(pool, n)`.
#[allow(clippy::too_many_arguments)]
fn claim(v: &mut RealVault, label: &str, pool: &VaultPool, i: usize, s: [u8; 32], kind: u8, dest: [u8; 32], n: [u8; 32]) -> TxReport {
    let program = consumer_account();
    let pool_id = pool.id();
    let at = claim_actor(program, &pool_id, &n);
    let holder = holder_actor(program, &pool_id, &dest);
    let header = harness::vault::header_of(pool, v.slot);
    let message = ConsumerMessage::ClaimDeposit { pool: pool.clone(), slot: v.slot, deposit: v.deposit_proof(i), s, kind, dest };
    v.w.chain.submit(
        label,
        at,
        borsh::to_vec(&message).unwrap(),
        vec![at, header, holder],
        vec![PublicAccountEvidence::Pda { program, seed: claim_seed(&pool_id, &n) }],
        v.time,
    )
}

fn nullifier_state(v: &RealVault, pool: &VaultPool, n: &[u8; 32]) -> Option<NullifierState> {
    match consumer_state(&v.w.chain, claim_actor(consumer_account(), &pool.id(), n)) {
        Some(ConsumerState::Nullifier(state)) => Some(state),
        None => None,
        Some(other) => panic!("a claim actor holds {other:?}"),
    }
}

fn holder_state(v: &RealVault, pool: &VaultPool, dest: &[u8; 32]) -> Option<HolderState> {
    match consumer_state(&v.w.chain, holder_actor(consumer_account(), &pool.id(), dest)) {
        Some(ConsumerState::Holder(state)) => Some(state),
        None => None,
        Some(other) => panic!("a holder actor holds {other:?}"),
    }
}

/// The consumer's two private transitions in the executor: the root call at `account`'s actor
/// (sends `Read`), then the record's reply, in which the claim is verified (sends `Spend`).
fn private_transitions(v: &RealVault, account: lee::AccountId, message: &ConsumerMessage, slot: u64) -> Vec<Executed> {
    let elf = consumer_program();
    let actor = Actor::new(account, consumer_account());
    let call = ReceiveInput {
        receiver: actor,
        from: None,
        is_authorized: true,
        pre_state: Default::default(),
        message: borsh::to_vec(message).unwrap(),
    };
    let first = execute("ClaimDepositPrivately (sends Read)", elf.elf(), &call);
    let Some(transition) = &first.transition else { return vec![first] };
    let header = harness::vault::header_of(&pool(v), slot);
    let reply = ReceiveInput {
        receiver: actor,
        from: Some(header),
        is_authorized: true,
        pre_state: transition.response.post_state.clone().expect("the claim is kept for the reply"),
        message: borsh::to_vec(&StateReply { state: v.w.chain.actor_state(header) }).unwrap(),
    };
    let second = execute("the record's reply (verifies the deposit, sends Spend)", elf.elf(), &reply);
    vec![first, second]
}

/// The public claim of a real deposit, once; its refusals; and the private claim of the same
/// deposit after it, which cannot be proven.
#[test]
fn a_real_deposit_is_claimed_publicly_once() {
    require_dev_mode();
    let mut v = RealVault::load();
    let open = openings(&v);
    let p = pool(&v);
    let (s0, kind0, dest0) = open[0];
    let n0 = secret_nullifier(&s0, &v.deposit_id(0));
    let (s1, _, dest1) = open[1];
    let n1 = secret_nullifier(&s1, &v.deposit_id(1));

    let mut refused = Vec::new();
    let mut refuse = |v: &mut RealVault, label: &str, pool: &VaultPool, i, s, kind, dest, n, code: u32| {
        let r = claim(v, label, pool, i, s, kind, dest, n);
        assert_eq!(r.exit_code, Some(code), "{label}: {:?}", r.charge.revert);
        assert!(nullifier_state(v, pool, &n).is_none(), "{label}: nothing is spent");
        refused.push(r);
    };
    // Before the claim: each refusal consumes nothing.
    let wrong_s = [0x5a; 32];
    let n_wrong = secret_nullifier(&wrong_s, &v.deposit_id(0));
    refuse(&mut v, "a wrong secret (the opening does not hash to C)", &p, 0, wrong_s, kind0, dest0, n_wrong, u32::from(code::COMMITMENT_MISMATCH));
    refuse(&mut v, "another destination than C commits to", &p, 0, s0, kind0, [0x77; 32], n0, u32::from(code::COMMITMENT_MISMATCH));
    refuse(&mut v, "the right opening at a claim actor its nullifier does not name", &p, 0, s0, kind0, dest0, [0x66; 32], u32::from(code::NULLIFIER_MISMATCH));
    refuse(&mut v, "a private-kind deposit claimed publicly", &p, 1, s1, KIND_PRIVATE, dest1, n1, u32::from(code::WRONG_KIND));
    refuse(&mut v, "a private-kind deposit opened as public-kind", &p, 1, s1, KIND_PUBLIC, dest1, n1, u32::from(code::COMMITMENT_MISMATCH));
    let erc20 = VaultPool { token: [0x11; 20], ..p.clone() };
    let false_predicate = native(&v, &erc20, &v.deposit_proof(0));
    assert_eq!(false_predicate, 130, "PredicateFailed");
    refuse(&mut v, "an ETH deposit claimed in a pool of another token (false predicate)", &erc20, 0, s0, kind0, dest0, n0, false_predicate);
    let mainnet = VaultPool { chain_id: 1, ..p.clone() };
    let binding = native(&v, &mainnet, &v.deposit_proof(0));
    assert_eq!(binding, 140, "the chain binding");
    refuse(&mut v, "a pool configured for mainnet (chain binding)", &mainnet, 0, s0, kind0, dest0, n0, binding);

    // The claim.
    let accepted = claim(&mut v, "the public claim of deposit 0 (block 12,111,477, one hop)", &p, 0, s0, kind0, dest0, n0);
    assert!(accepted.ok(), "{:?} (exit {:?})", accepted.charge.revert, accepted.exit_code);
    assert_eq!(nullifier_state(&v, &p, &n0), Some(NullifierState::Spent { pool_id: p.id() }));
    let held = holder_state(&v, &p, &dest0).expect("credited");
    assert_eq!((held.amount, held.credits), (AMOUNT, 1));

    // Replays: the same claim, and the same deposit privately (cross-path).
    let again = claim(&mut v, "the same claim again (replay)", &p, 0, s0, kind0, dest0, n0);
    assert_eq!(again.exit_code, Some(u32::from(code::ALREADY_CLAIMED)));
    refused.push(again);
    assert_eq!(holder_state(&v, &p, &dest0).map(|h| h.credits), Some(1), "credited once");

    let account = v.w.chain.add_private_account(0x91, 1_000_000_000_000);
    let message = ConsumerMessage::ClaimDepositPrivately { pool: p.clone(), slot: v.slot, deposit: v.deposit_proof(0), s: s0, kind: kind0, dest: dest0 };
    let publics = vec![v.actor(), claim_actor(consumer_account(), &p.id(), &n0), holder_actor(consumer_account(), &p.id(), &dest0)];
    let root = Actor::new(account.id(), consumer_account());
    let err = v
        .w
        .chain
        .prove_and_settle_private(&account, root, borsh::to_vec(&message).unwrap(), publics, &catalog(), 100_000_000, v.time)
        .err()
        .expect("a deposit claimed publicly cannot be claimed privately");
    println!("the private claim after the public one: {err}");
    assert!(err.contains(&code::ALREADY_CLAIMED.to_string()), "{err}");

    println!("## Accepted\n\n{}\n## Refused\n\n{}", render(std::slice::from_ref(&accepted)), render(&refused));
    let out = format!(
        "# The reference consumer on FerrataVault v5's real deposits: the public claim\n\n\
         Generated by `RISC0_DEV_MODE=1 cargo test -p harness --locked --test vault_consumer` (see `../RESULTS.md`).\n\n\
         ## Accepted\n\n{}\n## Refused (nothing spent; the last is the replay)\n\n{}\n\
         The same deposit claimed privately after the public claim: not provable, `{err}`.\n",
        render(std::slice::from_ref(&accepted)),
        render(&refused)
    );
    std::fs::write(results_dir().join("stage5-consumer-public.md"), out).unwrap();
}

/// The private claim of a real deposit: proven (dev mode) and settled; only `Read`, `Spend` (the
/// nullifier) and, for this public-kind deposit, the destination's credit cross into public
/// execution. Then the public claim of the same deposit and a second private claim are refused.
#[test]
fn a_real_deposit_is_claimed_privately_once() {
    require_dev_mode();
    let mut v = RealVault::load();
    let open = openings(&v);
    let p = pool(&v);
    let (s0, kind0, dest0) = open[0];
    let id0 = v.deposit_id(0);
    let n0 = secret_nullifier(&s0, &id0);
    let account = v.w.chain.add_private_account(0x92, 1_000_000_000_000);
    let message = ConsumerMessage::ClaimDepositPrivately { pool: p.clone(), slot: v.slot, deposit: v.deposit_proof(0), s: s0, kind: kind0, dest: dest0 };

    // The executor: what a prover has to prove for the consumer.
    let executed = private_transitions(&v, account.id(), &message, v.slot);
    let table = render_executed(&executed);
    println!("{table}");
    assert_eq!(executed.iter().map(|e| e.exit_code).collect::<Vec<_>>(), [0, 0]);
    let state = executed[1].transition.as_ref().unwrap().response.post_state.clone().unwrap();
    let ConsumerState::Claimant(kept) = borsh::from_slice(&state).unwrap() else { panic!("a claimant's state") };
    assert_eq!(kept, ClaimantState { awaiting: None, pending: None, holdings: vec![], claims: 1 }, "the claim is not kept");

    // Through LEZ: proven (dev mode) with the privacy-preserving circuit, settled.
    let program = consumer_account();
    let publics = vec![v.actor(), claim_actor(program, &p.id(), &n0), holder_actor(program, &p.id(), &dest0)];
    let root = Actor::new(account.id(), program);
    let run = v
        .w
        .chain
        .prove_and_settle_private(&account, root, borsh::to_vec(&message).unwrap(), publics.clone(), &catalog(), 100_000_000, v.time)
        .expect("settles");
    assert!(run.charge.revert.is_none(), "{:?}", run.charge.revert);
    assert_eq!(nullifier_state(&v, &p, &n0), Some(NullifierState::Spent { pool_id: p.id() }));
    let held = holder_state(&v, &p, &dest0).expect("credited");
    assert_eq!((held.amount, held.credits), (AMOUNT, 1));

    // What the transaction carries: not the secret, the deposit's identifier, its commitment, its
    // depositor, its block or its receipt proof.
    let bytes = borsh::to_vec(&LeeTransaction::PrivacyPreserving(run.tx.clone())).unwrap();
    let d = &v.deposits[0];
    let proof = v.deposit_proof(0);
    let absent = [
        ("the secret s", s0.to_vec()),
        ("the statement identifier", id0.to_vec()),
        ("the commitment C", d.deposit.commitment.to_vec()),
        ("the depositor", d.deposit.depositor.to_vec()),
        ("the deposit block's header", proof.hops[0].header.clone()),
        ("the receipt proof's last node", proof.proof.last().unwrap().clone()),
    ];
    let mut report = format!("The {} serialized bytes of the transaction were searched:\n\n", bytes.len());
    for (what, needle) in &absent {
        let found = occurs(&bytes, needle);
        report.push_str(&format!("- {what} ({} bytes): {}\n", needle.len(), if found { "FOUND" } else { "absent" }));
        assert!(!found, "{what} is in the transaction");
    }
    report.push_str(&format!("- the nullifier N ({} bytes): {}\n", n0.len(), if occurs(&bytes, &n0) { "present (spent publicly, by design)" } else { "absent" }));
    println!("{report}");

    // Replays: the public claim of the same deposit, and the private claim again.
    let public = claim(&mut v, "the public claim after the private one", &p, 0, s0, kind0, dest0, n0);
    assert_eq!(public.exit_code, Some(u32::from(code::ALREADY_CLAIMED)), "{:?}", public.charge.revert);
    let account2 = v.w.chain.add_private_account(0x93, 1_000_000_000_000);
    let err = v
        .w
        .chain
        .prove_and_settle_private(&account2, Actor::new(account2.id(), program), borsh::to_vec(&message).unwrap(), publics, &catalog(), 100_000_000, v.time)
        .err()
        .expect("a second private claim cannot be proven");
    assert!(err.contains(&code::ALREADY_CLAIMED.to_string()), "{err}");
    assert_eq!(holder_state(&v, &p, &dest0).map(|h| h.credits), Some(1), "credited once");

    let out = format!(
        "# The reference consumer on FerrataVault v5's real deposits: the private claim\n\n\
         Generated by `RISC0_DEV_MODE=1 cargo test -p harness --locked --test vault_consumer` (see `../RESULTS.md`).\n\
         Deposit 0 (block 12,111,477) under the record of block 12,112,773, one EIP-2935 hop, a Gloas record; \
         the consumer's two private transitions in the executor:\n\n{table}\n\
         Proven in dev mode and settled: {} transaction bytes, {} proof bytes, execution gas {}.\n\n{report}\n\
         Then refused: the public claim of the same deposit (`{:?}`), a second private claim (`{err}`).\n",
        run.tx_bytes,
        run.proof_bytes,
        run.charge.gas_exec,
        public.exit_code,
    );
    std::fs::write(results_dir().join("stage5-consumer-private.md"), out).unwrap();
}

/// Private claims that do not verify end in a non-zero exit of a transition, so they have no
/// receipt and nothing can be submitted.
#[test]
fn private_claims_that_do_not_verify_are_refused() {
    require_dev_mode();
    let v = RealVault::load();
    let open = openings(&v);
    let p = pool(&v);
    let (s0, kind0, dest0) = open[0];
    let (s1, kind1, dest1) = open[1];
    let account = lee::AccountId::new([0x31; 32]);
    let base = |i: usize, s, kind, dest| ConsumerMessage::ClaimDepositPrivately { pool: p.clone(), slot: v.slot, deposit: v.deposit_proof(i), s, kind, dest };
    let mut forged = base(0, s0, kind0, dest0);
    if let ConsumerMessage::ClaimDepositPrivately { deposit, .. } = &mut forged {
        let last = deposit.proof.len() - 1;
        let middle = deposit.proof[last].len() / 2;
        deposit.proof[last][middle] ^= 0x01;
    }
    let forged_code = match &forged {
        ConsumerMessage::ClaimDepositPrivately { deposit, .. } => native(&v, &p, deposit),
        _ => unreachable!(),
    };
    let other_chain = ConsumerMessage::ClaimDepositPrivately { pool: VaultPool { chain_id: 1, ..p.clone() }, slot: v.slot, deposit: v.deposit_proof(0), s: s0, kind: kind0, dest: dest0 };
    let other_token = ConsumerMessage::ClaimDepositPrivately { pool: VaultPool { token: [0x11; 20], ..p.clone() }, slot: v.slot, deposit: v.deposit_proof(0), s: s0, kind: kind0, dest: dest0 };
    // (what, message, exit codes of the two transitions; the second runs only after a zero)
    let cases: Vec<(&str, ConsumerMessage, Vec<u32>)> = vec![
        ("a private-kind deposit claimed by an account that is not its destination", base(1, s1, kind1, dest1), vec![u32::from(code::WRONG_DESTINATION)]),
        ("a byte that is no destination kind", base(0, s0, 0x03, dest0), vec![u32::from(code::WRONG_KIND)]),
        ("a wrong secret", base(0, [0x5a; 32], kind0, dest0), vec![0, u32::from(code::COMMITMENT_MISMATCH)]),
        ("a private-kind deposit opened as public-kind", base(1, s1, KIND_PUBLIC, dest1), vec![0, u32::from(code::COMMITMENT_MISMATCH)]),
        ("a forged receipt", forged, vec![0, forged_code]),
        ("a pool configured for mainnet", other_chain, vec![0, 140]),
        ("a pool of another token (false predicate)", other_token, vec![0, 130]),
    ];
    for (what, message, codes) in cases {
        let executed = private_transitions(&v, account, &message, v.slot);
        let got: Vec<u32> = executed.iter().map(|e| e.exit_code).collect();
        println!("{what}: exits {got:?}");
        assert_eq!(got, codes, "{what}");
    }
}
