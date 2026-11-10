//! The release (proposal §B3): the witness of one of the wallet's burns under a burn root the vault
//! accepted, the journal the vault rebuilds, the release guest run on that input (executed, or
//! proven, by r0vm from a program file), and the vault call.

use std::path::Path;

use alloy::primitives::{Address, Bytes, B256, U256};
use anyhow::{anyhow, ensure, Context as _, Result};
use ferrata_bridge_core::{
    chain_id_u256,
    encoding::ReleaseInput,
    outbound::{burn_leaf, release_journal, verify_release, OwnedBurn, ReleasePublic, ReleaseWitness},
    tree::Tree,
    Hash,
};

use crate::{
    eth::IVault,
    lez::{Context, History},
    units::{u128_to_word, word_to_u128},
};

/// A release, built and checked natively.
#[derive(Clone, Debug)]
pub struct ReleasePlan {
    pub input: ReleaseInput,
    pub journal: Vec<u8>,
    pub nullifier: Hash,
    pub request: IVault::ReleaseRequest,
}

/// The release of `owned` under the accepted burn root of the first `burn_count` burns.
#[allow(clippy::too_many_arguments)]
pub fn plan(
    ctx: &Context,
    history: &History,
    owned: &OwnedBurn,
    burn_root: Hash,
    burn_count: u64,
    denom_index: u8,
    relayer: [u8; 20],
    relay_fee: u128,
) -> Result<ReleasePlan> {
    ensure!(
        owned.burn.burn_index < burn_count,
        "burn {} is not under the newest accepted burn root ({burn_count} burns): wait for the next checkpoint",
        owned.burn.burn_index
    );
    ensure!(burn_count as usize <= history.burns.len(), "the zone shows {} burns, the burn root {burn_count}", history.burns.len());
    let leaves: Vec<Hash> = history.burns[..burn_count as usize]
        .iter()
        .map(|b| burn_leaf(&ctx.cid, b.burn_index, &b.note.token, &b.note.amount, &b.note.c_out, &b.note.r))
        .collect();
    let tree = Tree::from_leaves(leaves).map_err(|_| anyhow!("too many burns"))?;
    ensure!(
        tree.root() == burn_root,
        "the zone's burns do not give the vault's accepted burn root at {burn_count} burns: the zone or the RPC serves another chain"
    );
    let path = tree.path(owned.burn.burn_index).ok_or_else(|| anyhow!("burn {} is not in the tree", owned.burn.burn_index))?;
    let witness = ReleaseWitness { burn_index: owned.burn.burn_index, path, s_out: owned.s_out, r: owned.burn.note.r };
    let public = ReleasePublic {
        chain_id: chain_id_u256(ctx.chain_id),
        vault: ctx.vault,
        config_id: ctx.cid,
        token: owned.burn.note.token,
        amount: owned.burn.note.amount,
        recipient: owned.recipient,
        relayer,
        relay_fee: u128_to_word(relay_fee),
    };
    let nullifier = verify_release(&burn_root, &witness, &public).map_err(|e| anyhow!("the release statement does not hold: {e:?}"))?;
    ensure!(nullifier == owned.nullifier, "the release nullifier is not the recovered one");
    let journal = release_journal(&burn_root, &nullifier, &public).to_vec();
    let request = IVault::ReleaseRequest {
        token: Address::from(public.token),
        denomIndex: denom_index,
        recipient: Address::from(public.recipient),
        relayer: Address::from(relayer),
        relayFee: U256::from(word_to_u128(&public.relay_fee)?),
        burnRoot: B256::from(burn_root),
        nullifier: B256::from(nullifier),
    };
    Ok(ReleasePlan { input: ReleaseInput { burn_root, witness, public }, journal, nullifier, request })
}

/// What r0vm made of the release guest.
pub struct Proven {
    pub image_id: [u8; 32],
    /// The receipt (JSON), succinct; a real one only without `RISC0_DEV_MODE`.
    pub receipt: Vec<u8>,
    pub dev_mode: bool,
}

/// Runs the release guest in `program` on the plan's input through r0vm (`RISC0_DEV_MODE=1`:
/// executed only, a fake receipt; unset: a succinct proof on this machine's CPU), and checks that
/// its journal is the native one. The vault takes a Groth16 seal: wrap the receipt (RISC Zero's
/// Groth16 prover image, which needs Docker on x86) and pass the seal to `ferrata release --seal`.
pub fn prove(program: &Path, plan: &ReleasePlan) -> Result<Proven> {
    use risc0_zkvm::{compute_image_id, default_prover, ExecutorEnv, ProverOpts};
    let elf = std::fs::read(program).with_context(|| format!("reading the release program {}", program.display()))?;
    let image_id = compute_image_id(&elf).map_err(|e| anyhow!("not a guest program: {e}"))?;
    let input = plan.input.encode();
    let env = ExecutorEnv::builder().write_slice(&input).build().map_err(|e| anyhow!("{e}"))?;
    let info = default_prover().prove_with_opts(env, &elf, &ProverOpts::succinct()).map_err(|e| anyhow!("r0vm: {e}"))?;
    ensure!(info.receipt.journal.bytes == plan.journal, "the guest's journal is not the native release journal");
    let dev_mode = std::env::var("RISC0_DEV_MODE").map(|v| v == "1" || v == "true").unwrap_or(false);
    if !dev_mode {
        info.receipt.verify(image_id).map_err(|e| anyhow!("the receipt does not verify: {e}"))?;
    }
    let receipt = receipt_json(&info.receipt)?;
    let mut id = [0u8; 32];
    id.copy_from_slice(image_id.as_bytes());
    Ok(Proven { image_id: id, receipt, dev_mode })
}

fn receipt_json(receipt: &risc0_zkvm::Receipt) -> Result<Vec<u8>> {
    serde_json::to_vec(receipt).map_err(|e| anyhow!("{e}"))
}

/// The calldata of the vault call.
pub fn calldata(plan: &ReleasePlan, seal: Bytes) -> Bytes {
    crate::eth::release_calldata(plan.request.clone(), seal)
}
