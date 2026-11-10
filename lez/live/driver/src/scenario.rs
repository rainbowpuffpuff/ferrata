//! The run: deployment through LEZ's program loader, `Init` and the five mainnet updates over
//! the public job path, a finality update, every refusal of `../../harness/src/scenarios.rs`,
//! and what a block holds. Each job runs next to the native light client, as in the harness: the
//! instance's store, read from the sequencer, must equal the native store after every job.

use anyhow::{anyhow, bail, ensure, Result};
use ferrata_core::{
    fork::LcFork,
    steps::{self, HeaderRecord, Job, PendingApply, Phase},
    store::ApplyOutcome,
    types::{BeaconBlockHeader, LightClientBootstrap, LightClientUpdate},
    witness::{uncompressed_keys, CompactBootstrap, CompactStore, CompactUpdate},
    Root,
};
use ferrata_lez_wire::{code, job_id, InstanceState, Message, RecordState};
use lee::{AccountId, Actor};
use lee_core::program::{PdaSeed, ProgramHeader, PROGRAM_LOADER_ACCOUNT_ID};
use test_guest_core::Script;
use wallet::program_facades::program_loader::ProgramLoader;

use crate::{
    keys::{account_of, derived_key},
    live::{Live, Outcome, Tx},
    manifest::Row,
    plan::{
        as_finality_update, exit_name, mainnet_config, plan_job, root, signer_witness, Fixtures, Instance, Native,
        Pda, EXPECTED_FINALIZED, TRUSTED_BLOCK_ROOT,
    },
    programs::{image_id_hex, GuestBinary},
};

/// Ten years: an instance whose clock puts every mainnet slot of the fixtures in the future.
const FUTURE_SHIFT_SECONDS: u64 = 315_360_000;

// ---------------------------------------------------------------------------------------------
// Deployment
// ---------------------------------------------------------------------------------------------

pub fn header_key_label(name: &str) -> String {
    format!("program/{name}/header")
}

pub fn segment_key_label(name: &str, index: usize) -> String {
    format!("program/{name}/segment/{index}")
}

/// Where a deployment of `binary` named `name` lives: its header account and segment accounts.
pub fn deployment_accounts(name: &str, binary: &GuestBinary) -> (AccountId, Vec<AccountId>) {
    let header = account_of(&derived_key(&header_key_label(name)));
    let segments =
        (0..binary.chunks().len()).map(|i| account_of(&derived_key(&segment_key_label(name, i)))).collect();
    (header, segments)
}

/// Every key a deployment signs with (the wallet must hold them).
pub fn deployment_keys(name: &str, binary: &GuestBinary) -> Vec<lee::PrivateKey> {
    let mut keys = vec![derived_key(&header_key_label(name))];
    keys.extend((0..binary.chunks().len()).map(|i| derived_key(&segment_key_label(name, i))));
    keys
}

async fn deployed_header(live: &Live, header: AccountId) -> Result<Option<ProgramHeader>> {
    let state = live.actor_state(Actor::new(header, PROGRAM_LOADER_ACCOUNT_ID)).await?;
    Ok(crate::programs::program_header(&state))
}

/// Deploy `binary` with LEZ's wallet facade (`ProgramLoader::deploy`: one `WriteSegment` per
/// 96 KiB of the user ELF, last chunk first, then `CreateHeader`, immutable), paid by `payer`
/// with the wallet's default gas limit. The rows come from the blocks afterwards.
pub async fn deploy(live: &mut Live, name: &str, binary: &GuestBinary, payer: AccountId) -> Result<AccountId> {
    let (header, segments) = deployment_accounts(name, binary);
    if let Some(existing) = deployed_header(live, header).await? {
        ensure!(existing.image_id == binary.image_id, "{header} already holds another image");
        println!("[deploy] {name}: already deployed at {header}");
        return Ok(header);
    }
    live.sync().await?;
    let from = live.follower.height();
    let balance_before = live.balance(payer).await?;
    live.set_gas_limit(wallet::DEFAULT_GAS_LIMIT);
    let returned = ProgramLoader(&live.wallet).deploy(header, &segments, binary.bytes.clone(), true, Some(payer)).await?;
    ensure!(returned == header, "the loader facade returned another header account");
    // The facade waited for each transaction to be in a block of the head; wait until the
    // follower holds them all (in blocks final on Bedrock).
    let started = std::time::Instant::now();
    loop {
        live.sync().await?;
        let followed = live
            .follower
            .blocks
            .iter()
            .filter(|b| b.header.block_id > from)
            .flat_map(|b| b.body.transactions.iter())
            .filter_map(|tx| tx.public_call())
            .filter(|(to, _)| {
                to.program_account_id == PROGRAM_LOADER_ACCOUNT_ID && (to.account_id == header || segments.contains(&to.account_id))
            })
            .count();
        if followed == segments.len() + 1 {
            break;
        }
        ensure!(
            started.elapsed() < crate::live::SETTLE_TIMEOUT,
            "{name}: {followed} of {} loader transactions are final after {} s",
            segments.len() + 1,
            crate::live::SETTLE_TIMEOUT.as_secs()
        );
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    let balance_after = live.balance(payer).await?;

    // The loader's transactions of this deployment, as the sequencer included them.
    let mut rows = Vec::new();
    let mut fees: u128 = 0;
    for block in live.follower.blocks.iter().filter(|b| b.header.block_id > from) {
        for tx in &block.body.transactions {
            let Some((to, _)) = tx.public_call() else { continue };
            if to.program_account_id != PROGRAM_LOADER_ACCOUNT_ID {
                continue;
            }
            let step = if to.account_id == header {
                format!("deploy {name}: CreateHeader (immutable) at {header}")
            } else if let Some(i) = segments.iter().position(|s| *s == to.account_id) {
                format!("deploy {name}: WriteSegment {} of {} ({} bytes)", i + 1, segments.len(), binary.chunks()[i].len())
            } else {
                continue;
            };
            let hash = tx.hash();
            let receipt = live.follower.receipt(&hash).ok_or_else(|| anyhow!("no receipt for {hash}"))?;
            fees += receipt.fee_base.parse::<u128>().unwrap_or(0);
            rows.push(
                Row { phase: live.phase.clone(), step, tx_hash: hash.to_string(), ..Row::default() }
                    .with_receipt(receipt, exit_name),
            );
        }
    }
    ensure!(rows.len() == segments.len() + 1, "{name}: expected {} loader transactions, found {}", segments.len() + 1, rows.len());
    ensure!(rows.iter().all(|r| r.status == "applied"), "{name}: a loader transaction reverted");
    let debit = balance_before.saturating_sub(balance_after);
    if let Some(last) = rows.last_mut() {
        last.payer_debit = debit.to_string();
        last.note = format!("payer_debit is for the whole deployment; its fees sum to {fees}");
    }
    for row in &rows {
        println!("[{}] {:<58} {:<9} gas {:>9} + {:>7} bytes, block {:>4}", live.phase, row.step, row.status, row.exec_gas, row.stor_gas, row.block);
    }
    live.rows.extend(rows);

    let on_chain = deployed_header(live, header).await?.ok_or_else(|| anyhow!("{name}: no header at {header}"))?;
    ensure!(
        on_chain.image_id == binary.image_id,
        "{name}: the loader computed image {}, the binary is {}",
        image_id_hex(on_chain.image_id),
        binary.image_hex()
    );
    ensure!(on_chain.immutable && on_chain.program_first_segment == segments[0], "{name}: unexpected header");
    println!("[deploy] {name}: image {} at {header} (computed by the sequencer's loader), immutable", binary.image_hex());
    live.save()?;
    Ok(header)
}

// ---------------------------------------------------------------------------------------------
// An instance and its jobs
// ---------------------------------------------------------------------------------------------

/// An instance on the chain next to the native light client.
pub struct World {
    pub inst: Instance,
    pub native: Native,
    /// Job ids, in the order the jobs ran.
    pub jobs: Vec<Root>,
}

/// The harness rows a job's transactions correspond to.
#[derive(Clone)]
pub enum Harness {
    None,
    /// Labels are `"{prefix} {step}"` in `table`.
    Prefix(&'static str, String),
    /// One label per transaction, in order.
    List(&'static str, Vec<&'static str>),
}

impl Harness {
    fn key(&self, index: usize, step: &str) -> Option<(&'static str, String)> {
        match self {
            Harness::None => None,
            Harness::Prefix(table, prefix) => Some((*table, format!("{prefix} {step}"))),
            Harness::List(table, labels) => labels.get(index).map(|l| (*table, (*l).to_string())),
        }
    }
}

pub struct JobRun {
    pub job_id: Root,
    pub outcomes: Vec<Outcome>,
    /// What the job does when applied (computed natively before submitting).
    pub outcome: Option<ApplyOutcome>,
}

impl World {
    pub fn new(name: &str, program: AccountId, config: ferrata_lez_wire::Config, bootstrap: &LightClientBootstrap) -> Result<Self> {
        let checkpoint = root(TRUSTED_BLOCK_ROOT);
        let native = Native::new(&config, &checkpoint, bootstrap)?;
        Ok(Self { inst: Instance::new(name, program, config, checkpoint), native, jobs: Vec::new() })
    }
}

pub async fn instance_state(live: &Live, inst: &Instance) -> Result<Option<InstanceState>> {
    let state = live.actor_state(inst.pda().actor()).await?;
    if state.is_empty() {
        return Ok(None);
    }
    Ok(Some(borsh::from_slice(&state)?))
}

pub async fn store(live: &Live, inst: &Instance) -> Result<CompactStore> {
    let state = instance_state(live, inst).await?.ok_or_else(|| anyhow!("instance {} does not exist", inst.name))?;
    CompactStore::decode(&state.store).map_err(|e| anyhow!("instance {}: its store does not decode: {e:?}", inst.name))
}

/// The instance holds the compact form of the native store.
pub async fn check_store(live: &Live, world: &World) -> Result<()> {
    ensure!(
        store(live, &world.inst).await? == CompactStore::of(world.native.core()),
        "instance {} and the native store differ",
        world.inst.name
    );
    Ok(())
}

/// `header(id, slot)` holds the native finalized header's record.
pub async fn check_record(live: &Live, world: &World, slot: u64) -> Result<()> {
    let state = live.actor_state(world.inst.header(slot).actor()).await?;
    ensure!(!state.is_empty(), "instance {}: no record for slot {slot}", world.inst.name);
    let record: RecordState = borsh::from_slice(&state)?;
    ensure!(record.instance_id == world.inst.id, "the record names another instance");
    let expected = HeaderRecord::of(&world.native.core().finalized_header, LcFork::Electra, &world.native.spec);
    ensure!(record.record == expected.encode(), "the record is not the native finalized header's");
    Ok(())
}

/// The transactions of `Init`: the bootstrap check, then the adoption of its committee in parts.
pub fn init_txs(inst: &Instance, bootstrap: &LightClientBootstrap, label: &str, payer: AccountId) -> Result<(Root, Vec<Tx>)> {
    let fork = inst.fork();
    let spec = inst.config.spec();
    let compact = CompactBootstrap::of(bootstrap);
    let parts = inst.config.adoption_parts;
    let (store, job) = steps::initialize(&inst.checkpoint, &compact, fork, &spec, parts)
        .map_err(|e| anyhow!("the bootstrap does not verify: {e:?}"))?;
    let jid = job_id(&inst.id, &job.store_digest, &job.update_digest);
    let job_pda = inst.job(&jid);
    let mut txs = vec![Tx {
        step: format!("{label} Init"),
        harness: None,
        actors: vec![inst.pda(), job_pda],
        root: 0,
        message: borsh::to_vec(&Message::Init {
            config: inst.config.clone(),
            checkpoint: inst.checkpoint,
            bootstrap: compact.encode(fork),
        })?,
        payer,
        gas_limit: None,
        note: String::new(),
    }];
    let keys = uncompressed_keys(&bootstrap.current_sync_committee).map_err(|e| anyhow!("committee keys: {e:?}"))?;
    txs.extend(adopt_txs(inst, label, job_pda, &keys, Some(store.finalized.slot), payer)?);
    Ok((jid, txs))
}

/// The adoption transactions of a job. The last one also reaches the instance and, if it applies
/// the job (an initialization), the header record at `record_slot`.
fn adopt_txs(
    inst: &Instance,
    label: &str,
    job_pda: Pda,
    keys: &[[u8; 96]],
    record_slot: Option<u64>,
    payer: AccountId,
) -> Result<Vec<Tx>> {
    let parts = inst.config.adoption_parts;
    let chunk = keys.len() / parts as usize;
    let mut txs = Vec::new();
    for p in 0..parts {
        let mut actors = vec![job_pda];
        if p + 1 == parts {
            if let Some(slot) = record_slot {
                actors.extend([inst.pda(), inst.header(slot)]);
            }
        }
        let part = keys[p as usize * chunk..(p as usize + 1) * chunk].to_vec();
        txs.push(Tx {
            step: format!("{label} adopt {}/{parts}", p + 1),
            harness: None,
            actors,
            root: 0,
            message: borsh::to_vec(&Message::Adopt { part: p, keys: part })?,
            payer,
            gas_limit: None,
            note: String::new(),
        });
    }
    Ok(txs)
}

fn step_of(label: &str, step: &str) -> String {
    step.strip_prefix(label).map(str::trim).unwrap_or(step).to_string()
}

/// `Init` and the adoption of the bootstrap committee; afterwards the instance is active, holds
/// the native store and the bootstrap header's record.
pub async fn init(live: &mut Live, world: &mut World, bootstrap: &LightClientBootstrap, label: &str, harness: Harness, payer: AccountId) -> Result<Vec<Outcome>> {
    let (jid, txs) = init_txs(&world.inst, bootstrap, label, payer)?;
    let mut outcomes = Vec::new();
    for (index, mut tx) in txs.into_iter().enumerate() {
        tx.harness = harness.key(index, &step_of(label, &tx.step));
        outcomes.push(live.submit(tx).await?.expect_applied()?);
    }
    world.jobs.push(jid);
    finish_init(live, world, bootstrap).await?;
    Ok(outcomes)
}

pub async fn finish_init(live: &mut Live, world: &World, bootstrap: &LightClientBootstrap) -> Result<()> {
    let state = instance_state(live, &world.inst).await?.ok_or_else(|| anyhow!("no instance after Init"))?;
    ensure!(state.active, "the instance activates with its last adoption part");
    check_store(live, world).await?;
    check_record(live, world, bootstrap.header.beacon.slot).await?;
    live.save()
}

/// The whole job for one update: `Open`, `Miller`, the adoption parts if it adopts a committee,
/// `Finish`. The job is first run natively to name its actors and to know whether it adopts and
/// advances. Stops after a refused `Open`.
pub async fn update_job(
    live: &mut Live,
    world: &World,
    update: &LightClientUpdate,
    full: &ferrata_core::store::StoreCore,
    label: &str,
    harness: &Harness,
    payer: AccountId,
) -> Result<JobRun> {
    let inst = &world.inst;
    let fork = inst.fork();
    let spec = inst.config.spec();
    let on_chain = store(live, inst).await?;
    let compact = CompactUpdate::of(update);
    let witness = signer_witness(full, update, &spec);
    let jid = job_id(&inst.id, &on_chain.digest(), &compact.digest(fork));
    let job_pda = inst.job(&jid);
    let (planned, outcome) = plan_job(&on_chain, &compact, &witness, update, &inst.config);
    let mut outcomes = Vec::new();
    let mut index = 0;
    let mut next = |step: &str, actors: Vec<Pda>, message: Vec<u8>| {
        let tx = Tx {
            step: format!("{label} {step}"),
            harness: harness.key(index, step),
            actors,
            root: 0,
            message,
            payer,
            gas_limit: None,
            note: String::new(),
        };
        index += 1;
        tx
    };
    let open = live
        .submit(next(
            "Open (step 1)",
            vec![inst.pda(), job_pda],
            borsh::to_vec(&Message::Open { update: compact.encode(fork), signers: witness.encode() })?,
        ))
        .await?;
    let opened = open.applied();
    outcomes.push(open);
    if !opened {
        return Ok(JobRun { job_id: jid, outcomes, outcome: None });
    }
    outcomes.push(live.submit(next("Miller (step 2)", vec![job_pda], borsh::to_vec(&Message::Miller)?)).await?);
    if planned.as_ref().is_some_and(|job| job.adoption.is_some()) {
        let keys = uncompressed_keys(&update.next_sync_committee).map_err(|e| anyhow!("committee keys: {e:?}"))?;
        for mut tx in adopt_txs(inst, label, job_pda, &keys, None, payer)? {
            let step = step_of(label, &tx.step);
            tx.harness = harness.key(index, &step);
            index += 1;
            outcomes.push(live.submit(tx).await?);
        }
    }
    let mut actors = vec![job_pda, inst.pda()];
    if outcome.is_some_and(|o| o.advanced) {
        actors.push(inst.header(update.finalized_header.beacon.slot));
    }
    let finish = Tx {
        step: format!("{label} Finish (step 3)"),
        harness: harness.key(index, "Finish (step 3)"),
        actors,
        root: 0,
        message: borsh::to_vec(&Message::Finish)?,
        payer,
        gas_limit: None,
        note: String::new(),
    };
    outcomes.push(live.submit(finish).await?);
    Ok(JobRun { job_id: jid, outcomes, outcome })
}

/// Run `update` as a job on the chain and natively; they must agree.
pub async fn apply(live: &mut Live, world: &mut World, update: &LightClientUpdate, label: &str, harness: Harness, payer: AccountId) -> Result<JobRun> {
    let before = world.native.core().clone();
    let native = world.native.process(update);
    let run = update_job(live, world, update, &before, label, &harness, payer).await?;
    let all_applied = run.outcomes.iter().all(Outcome::applied);
    match native {
        Ok(outcome) => {
            ensure!(all_applied, "{label}: the sequencer refused what the native store accepts");
            ensure!(run.outcome == Some(outcome), "{label}: outcomes differ");
            check_store(live, world).await?;
            if outcome.advanced {
                check_record(live, world, update.finalized_header.beacon.slot).await?;
            }
            let job = live.actor_state(world.inst.job(&run.job_id).actor()).await?;
            ensure!(job.is_empty(), "{label}: the job is cleared");
        }
        Err(error) => {
            ensure!(!all_applied, "{label}: the sequencer accepted what the native store refuses ({error:?})");
            check_store(live, world).await?;
        }
    }
    world.jobs.push(run.job_id);
    live.save()?;
    Ok(run)
}

// ---------------------------------------------------------------------------------------------
// The phases
// ---------------------------------------------------------------------------------------------

/// Instance A on the main deployment: `Init`, then the five mainnet updates as served (each
/// adopts a committee; four rotate), exactly `harness::scenarios::replay(2)`.
pub async fn replay(live: &mut Live, program: AccountId, fixtures: &Fixtures, payer: AccountId) -> Result<()> {
    let mut world = World::new("A", program, mainnet_config(2), &fixtures.bootstrap)?;
    init(live, &mut world, &fixtures.bootstrap, "Init", Harness::Prefix("stage1-replay", "Init".into()), payer).await?;
    for (i, (slot, block_root)) in EXPECTED_FINALIZED.into_iter().enumerate() {
        let label = format!("update_{i}");
        apply(live, &mut world, &fixtures.updates[i], &label, Harness::Prefix("stage1-replay", label.clone()), payer).await?;
        let on_chain = store(live, &world.inst).await?;
        ensure!(on_chain.finalized.slot == slot, "{label}: finalized slot {}", on_chain.finalized.slot);
        ensure!(
            on_chain.finalized.hash_tree_root() == root(block_root),
            "{label}: the finalized root is not the canonical root a second provider reports"
        );
    }
    Ok(())
}

/// The twin deployment's instance B after `Init` and update_0, recomputed natively (for phases
/// that run in a later process).
pub fn world_b_after(program: AccountId, fixtures: &Fixtures, updates: &[LightClientUpdate]) -> Result<World> {
    let mut world = World::new("B", program, mainnet_config(2), &fixtures.bootstrap)?;
    for update in updates {
        world.native.process(update).map_err(|e| anyhow!("native replay: {e:?}"))?;
    }
    Ok(world)
}

/// update_1 as a finality update carrying update_2's signature: a valid point of G2 over
/// another message.
pub fn forged_update(fixtures: &Fixtures) -> LightClientUpdate {
    let mut forged = as_finality_update(&fixtures.updates[1]);
    forged.sync_aggregate.signature = fixtures.updates[2].sync_aggregate.signature;
    forged
}

/// The job the forged update opens on instance B (it stays open: its `Finish` is refused).
pub fn forged_job(world_after_update_0: &World, fixtures: &Fixtures) -> Root {
    let store = CompactStore::of(world_after_update_0.native.core());
    let compact = CompactUpdate::of(&forged_update(fixtures));
    job_id(&world_after_update_0.inst.id, &store.digest(), &compact.digest(world_after_update_0.inst.fork()))
}

/// The twin deployment: the finality path and every refusal, on one instance (B) and two more
/// (another chain's, and one whose clock is ten years ahead).
pub async fn scenarios(
    live: &mut Live,
    main_program: AccountId,
    twin: AccountId,
    scripted: AccountId,
    fixtures: &Fixtures,
    payer: AccountId,
) -> Result<()> {
    let refusals = "stage1-refusals";
    let updates = &fixtures.updates;
    let mut b = World::new("B", twin, mainnet_config(2), &fixtures.bootstrap)?;

    // Init, with one detour: the wallet's default gas limit on the first adoption part.
    let (init_job, txs) = init_txs(&b.inst, &fixtures.bootstrap, "twin Init", payer)?;
    let harness_init = ["Init Init", "Init adopt 1/2", "Init adopt 2/2"];
    for (index, mut tx) in txs.into_iter().enumerate() {
        tx.harness = Some(("stage1-replay", harness_init[index].to_string()));
        if index == 1 {
            let detour = Tx {
                step: format!("{} with the wallet's default gas limit", tx.step),
                harness: None,
                actors: tx.actors.clone(),
                root: tx.root,
                message: tx.message.clone(),
                payer,
                gas_limit: Some(wallet::DEFAULT_GAS_LIMIT),
                note: "LEZ's wallet declares 2,000,000 by default; the step needs 3.4M".into(),
            };
            let out = live.submit(detour).await?;
            ensure!(out.row.status == "reverted", "expected an out-of-gas revert, got {} {}", out.row.status, out.row.note);
            ensure!(
                out.row.exec_gas == wallet::DEFAULT_GAS_LIMIT.to_string(),
                "an out-of-gas action is charged its whole gas limit, got {}",
                out.row.exec_gas
            );
        }
        live.submit(tx).await?.expect_applied()?;
    }
    b.jobs.push(init_job);
    finish_init(live, &b, &fixtures.bootstrap).await?;

    apply(live, &mut b, &updates[0], "twin update_0", Harness::Prefix("stage1-replay", "update_0".into()), payer).await?;
    let b_update_0_job = *b.jobs.last().expect("a job");
    let store_before = store(live, &b.inst).await?;

    // A signature that is not a point: refused at step 1.
    {
        let mut bad = as_finality_update(&updates[1]);
        bad.sync_aggregate.signature[0] &= 0x7f; // compression flag cleared
        let run = apply(live, &mut b, &bad, "malformed signature:", Harness::List(refusals, vec!["malformed signature: Open"]), payer).await?;
        ensure!(run.outcomes.len() == 1 && run.outcomes[0].exit_code() == Some(41), "SignatureMalformed at step 1");
    }

    // A forged signature: steps 1 and 2 pass; step 3 refuses. The job stays.
    {
        let forged = forged_update(fixtures);
        let labels = vec!["forged signature: Open (passes)", "forged signature: Miller (passes)", "forged signature: Finish"];
        let run = apply(live, &mut b, &forged, "forged signature:", Harness::List(refusals, labels), payer).await?;
        ensure!(run.outcomes.len() == 3 && run.outcomes[0].applied() && run.outcomes[1].applied(), "steps 1 and 2 pass");
        ensure!(run.outcomes[2].exit_code() == Some(40), "SignatureInvalid at the final exponentiation");
        ensure!(run.job_id == forged_job(&b, fixtures), "the forged job's id");
        ensure!(store(live, &b.inst).await? == store_before, "a refused job changes nothing");
    }

    // Steps sent by the wrong actor.
    {
        let inst = b.inst.clone();
        let fake_job = Job {
            store_digest: store(live, &inst).await?.digest(),
            update_digest: [0; 32],
            phase: Phase::Verified,
            adoption: None,
            pending: PendingApply::Update {
                finalized: BeaconBlockHeader { slot: 99_999_999, ..Default::default() },
                next: None,
                record: HeaderRecord::of(&fixtures.bootstrap.header, LcFork::Electra, &b.native.spec),
            },
        };
        let jid = [7u8; 32];
        let apply_msg = Message::Apply { instance_id: inst.id, job_id: jid, job: fake_job.encode() };
        let tx = |step: &str, actors: Vec<Pda>, message: Vec<u8>| Tx {
            step: step.to_string(),
            harness: Some((refusals, step.to_string())),
            actors,
            root: 0,
            message,
            payer,
            gas_limit: None,
            note: String::new(),
        };
        live.submit(tx("Apply sent as the transaction root", vec![inst.pda()], borsh::to_vec(&apply_msg)?))
            .await?
            .expect_exit(code::WRONG_SENDER)?;

        // An actor of another program (LEZ's `scripted` test program) sending internal messages.
        let attacker = Pda { program: scripted, seed: PdaSeed::new([7; 32]) };
        let scripted_tx = |step: &str, to: Pda, message: &Message| -> Result<Tx> {
            let script = Script::default().send_sync(to.actor(), message);
            Ok(tx(step, vec![attacker, to], borsh::to_vec(&script)?))
        };
        live.submit(scripted_tx("Apply from another program's actor", inst.pda(), &apply_msg)?)
            .await?
            .expect_exit(code::WRONG_SENDER)?;
        let verified = Message::ApplyVerified { instance_id: inst.id, job: fake_job.encode() };
        live.submit(scripted_tx("ApplyVerified from another program's actor", inst.pda(), &verified)?)
            .await?
            .expect_exit(code::WRONG_SENDER)?;
        let open = Message::JobOpen { instance_id: inst.id, job_id: jid, committee_size: 512, job: fake_job.encode() };
        live.submit(scripted_tx("JobOpen from another program's actor", inst.job(&jid), &open)?)
            .await?
            .expect_exit(code::WRONG_SENDER)?;
        let record =
            Message::Record { instance_id: inst.id, slot: 99_999_999, chain_id: inst.config.chain_id, record: vec![1, 2, 3] };
        live.submit(scripted_tx("Record from another program's actor", inst.header(99_999_999), &record)?)
            .await?
            .expect_exit(code::WRONG_SENDER)?;
        // The same image deployed at another address is another program: its jobs are not ours.
        let other_deployment = Instance::new("A", main_program, inst.config.clone(), inst.checkpoint);
        let mut miller = tx("Miller at the other deployment's address of the job", vec![other_deployment.job(&jid)], borsh::to_vec(&Message::Miller)?);
        miller.harness = None;
        live.submit(miller).await?.expect_exit(code::NO_JOB)?;
        let absent = live.actor_state(inst.header(99_999_999).actor()).await?;
        ensure!(absent.is_empty(), "no record was written");
        check_store(live, &b).await?;
    }

    // Init again.
    {
        let msg = Message::Init {
            config: b.inst.config.clone(),
            checkpoint: b.inst.checkpoint,
            bootstrap: CompactBootstrap::of(&fixtures.bootstrap).encode(LcFork::Electra),
        };
        let step = "Init of an existing instance";
        live.submit(Tx {
            step: step.into(),
            harness: Some((refusals, step.into())),
            actors: vec![b.inst.pda()],
            root: 0,
            message: borsh::to_vec(&msg)?,
            payer,
            gas_limit: None,
            note: String::new(),
        })
        .await?
        .expect_exit(code::ALREADY_INITIALIZED)?;
    }

    // An update for another instance: the same mainnet data, an instance of another chain (its
    // genesis_validators_root differs). Its bootstrap verifies (no signature), the update does not.
    {
        let mut config = mainnet_config(2);
        config.genesis_validators_root = [0x42; 32];
        let mut other = World::new("C", twin, config, &fixtures.bootstrap)?;
        ensure!(other.inst.id != b.inst.id, "another instance");
        init(live, &mut other, &fixtures.bootstrap, "other chain Init", Harness::None, payer).await?;
        let run = apply(live, &mut other, &updates[0], "other chain update_0", Harness::None, payer).await?;
        let last = run.outcomes.last().expect("a transaction");
        ensure!(last.exit_code() == Some(40), "SignatureInvalid, got {} {}", last.row.status, last.row.exit);
        // The same row under the harness's name, for the comparison.
        if let Some(h) = live.harness.get(refusals, "update for another chain's instance: Finish").cloned() {
            if let Some(row) = live.rows.iter_mut().rev().find(|r| r.tx_hash == last.row.tx_hash) {
                row.harness_exec_gas = h.exec_gas.to_string();
                row.harness_stor_gas = h.stor_gas.to_string();
                row.harness_result = h.result;
            }
        }
        // And a step of instance B's job sent to this instance's job address: no such job.
        let step = "instance A's job id sent to instance B";
        live.submit(Tx {
            step: step.into(),
            harness: Some((refusals, step.into())),
            actors: vec![other.inst.job(&b_update_0_job)],
            root: 0,
            message: borsh::to_vec(&Message::Finish)?,
            payer,
            gas_limit: None,
            note: "here: the job id of the twin's instance (B) at the other chain's instance (C)".into(),
        })
        .await?
        .expect_exit(code::NO_JOB)?;
    }

    // Too early: an Open before its signature slot is valid only from that slot's start. A live
    // sequencer stamps blocks with its own clock, so the early instance is one whose genesis
    // time is ten years later.
    {
        let mut config = mainnet_config(2);
        config.genesis_time += FUTURE_SHIFT_SECONDS;
        let mut early = World::new("D", twin, config, &fixtures.bootstrap)?;
        init(live, &mut early, &fixtures.bootstrap, "late clock Init", Harness::None, payer).await?;
        let inst = &early.inst;
        let update = &updates[0];
        let on_chain = store(live, inst).await?;
        let compact = CompactUpdate::of(update);
        let witness = signer_witness(early.native.core(), update, &inst.config.spec());
        let jid = job_id(&inst.id, &on_chain.digest(), &compact.digest(LcFork::Electra));
        let step = "Open before its signature slot (validity window)";
        let out = live
            .submit(Tx {
                step: step.into(),
                harness: Some((refusals, step.into())),
                actors: vec![inst.pda(), inst.job(&jid)],
                root: 0,
                message: borsh::to_vec(&Message::Open { update: compact.encode(LcFork::Electra), signers: witness.encode() })?,
                payer,
                gas_limit: None,
                note: "update_0 on an instance whose slots start ten years later; the harness sets the block time instead and declares 10M".into(),
            })
            .await?;
        ensure!(
            out.row.status == "reverted" && out.row.note.contains("validity window"),
            "expected LEZ's validity-window revert, got {} {}",
            out.row.status,
            out.row.note
        );
        ensure!(out.row.exec_gas == live.opts.gas_limit.to_string(), "LEZ's own refusals are charged the whole gas limit");
    }

    // Replayed and duplicated steps, around a finality update that applies.
    {
        let update = as_finality_update(&updates[1]);
        let before = b.native.core().clone();
        let inst = b.inst.clone();
        let fork = LcFork::Electra;
        let compact = CompactUpdate::of(&update);
        let witness = signer_witness(&before, &update, &b.native.spec);
        let on_chain = store(live, &inst).await?;
        let jid = job_id(&inst.id, &on_chain.digest(), &compact.digest(fork));
        let job = inst.job(&jid);
        let header = inst.header(update.finalized_header.beacon.slot);
        let open_msg = borsh::to_vec(&Message::Open { update: compact.encode(fork), signers: witness.encode() })?;
        let finality = "stage1-finality";
        let tx = |step: &str, harness: Option<(&'static str, &str)>, actors: Vec<Pda>, message: Vec<u8>| Tx {
            step: step.to_string(),
            harness: harness.map(|(table, label)| (table, label.to_string())),
            actors,
            root: 0,
            message,
            payer,
            gas_limit: None,
            note: String::new(),
        };
        let finish = borsh::to_vec(&Message::Finish)?;
        let miller = borsh::to_vec(&Message::Miller)?;
        live.submit(tx(
            "update_1 as finality update Open (step 1)",
            Some((finality, "update_1 as finality update Open (step 1)")),
            vec![inst.pda(), job],
            open_msg.clone(),
        ))
        .await?
        .expect_applied()?;
        live.submit(tx("duplicate Open (same job)", Some((refusals, "duplicate Open (same job)")), vec![inst.pda(), job], open_msg.clone()))
            .await?
            .expect_exit(code::JOB_EXISTS)?;
        live.submit(tx(
            "Finish before Miller (skipped step)",
            Some((refusals, "Finish before Miller (skipped step)")),
            vec![job, inst.pda()],
            finish.clone(),
        ))
        .await?
        .expect_exit(81)?;
        live.submit(tx(
            "update_1 as finality update Miller (step 2)",
            Some((finality, "update_1 as finality update Miller (step 2)")),
            vec![job],
            miller.clone(),
        ))
        .await?
        .expect_applied()?;
        live.submit(tx("Miller replayed", Some((refusals, "Miller replayed")), vec![job], miller)).await?.expect_exit(81)?;
        live.submit(tx(
            "update_1 as finality update Finish (step 3)",
            Some((finality, "update_1 as finality update Finish (step 3)")),
            vec![job, inst.pda(), header],
            finish.clone(),
        ))
        .await?
        .expect_applied()?;
        live.submit(tx(
            "Finish replayed after the job applied",
            Some((refusals, "Finish replayed after the job applied")),
            vec![job, inst.pda(), header],
            finish,
        ))
        .await?
        .expect_exit(code::NO_JOB)?;
        live.submit(tx("Open an applied update again", Some((refusals, "Open an applied update again")), vec![inst.pda(), job], open_msg))
            .await?
            .expect_exit(52)?;
        let outcome = b.native.process(&update).map_err(|e| anyhow!("the native store refuses the finality update: {e:?}"))?;
        ensure!(outcome.advanced && !outcome.adopted, "a finality update advances and adopts nothing");
        b.jobs.push(jid);
        check_store(live, &b).await?;
        check_record(live, &b, update.finalized_header.beacon.slot).await?;
        live.save()?;
    }

    // update_1 as served: it now only supplies the period's next committee.
    {
        let label = "update_1 (next committee only)";
        let run = apply(live, &mut b, &updates[1], label, Harness::Prefix("stage1-finality", label.into()), payer).await?;
        ensure!(
            run.outcome.map(|o| (o.advanced, o.adopted)) == Some((false, true)),
            "the served update only hands over a committee"
        );
    }
    Ok(())
}

/// What a block holds: three `Finish` of the forged job (each refused with exit 40 after 3.6M
/// cycles, and the job stays), sent at once by three payers. The sequencer budgets a block by
/// the gas each transaction declares.
pub async fn capacity(live: &mut Live, twin: AccountId, fixtures: &Fixtures, payers: &[AccountId]) -> Result<()> {
    ensure!(payers.len() >= 3, "three funded payers are needed");
    let world = world_b_after(twin, fixtures, &fixtures.updates[..1])?;
    let jid = forged_job(&world, fixtures);
    let job = world.inst.job(&jid);
    ensure!(!live.actor_state(job.actor()).await?.is_empty(), "the forged job is not open (run the scenarios first)");
    let mut sent = Vec::new();
    for (i, payer) in payers.iter().take(3).enumerate() {
        let tx = Tx {
            step: format!("three at once: Finish of the forged job, {} of 3", i + 1),
            harness: Some(("stage1-refusals", "forged signature: Finish".to_string())),
            actors: vec![job, world.inst.pda()],
            root: 0,
            message: borsh::to_vec(&Message::Finish)?,
            payer: *payer,
            gas_limit: None,
            note: String::new(),
        };
        match live.send(tx).await? {
            Ok(s) => sent.push(s),
            Err(outcome) => bail!("the sequencer's RPC rejected a transaction: {}", outcome.row.note),
        }
    }
    let mut outcomes = Vec::new();
    for s in sent {
        outcomes.push(live.settle(s).await?);
    }
    for outcome in &outcomes {
        ensure!(outcome.exit_code() == Some(40), "each is refused with SignatureInvalid");
    }
    live.save()
}
