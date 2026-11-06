//! Upgrading light-client data to a later fork's layout (the `fork.md` documents of the
//! light-client specs). A later store can process earlier data only after this upgrade.

use alloc::{vec, vec::Vec};

use crate::{
    fork::{ChainSpec, Fork, LcFork},
    merkle::{
        normalize_merkle_branch, CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, CURRENT_SYNC_COMMITTEE_GINDEX_GLOAS,
        EXECUTION_BLOCK_HASH_GINDEX_GLOAS, FINALIZED_ROOT_GINDEX_ELECTRA, FINALIZED_ROOT_GINDEX_GLOAS,
        NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA, NEXT_SYNC_COMMITTEE_GINDEX_GLOAS, ZERO,
    },
    types::{Execution, ExecutionPayloadHeader, LightClientBootstrap, LightClientHeader, LightClientUpdate},
    Error, Root,
};

/// `upgrade_lc_header_to_gloas`.
fn header_to_gloas(pre: LightClientHeader, spec: &ChainSpec) -> Result<LightClientHeader, Error> {
    if pre.is_empty() {
        return Ok(LightClientHeader::empty(LcFork::Gloas));
    }
    let Execution::Payload { header, branch } = pre.execution else {
        return Err(Error::ForkMismatch);
    };
    let slot = pre.beacon.slot;
    let (block_hash, branch) = if spec.is_at_or_after(slot, Fork::Capella) {
        // Prove `block_hash` inside the payload header, then the payload inside the body.
        let deneb = spec.is_at_or_after(slot, Fork::Deneb);
        let mut full = header.block_hash_proof(deneb);
        full.extend_from_slice(&branch);
        (header.block_hash, normalize_merkle_branch(&full, EXECUTION_BLOCK_HASH_GINDEX_GLOAS))
    } else {
        (ZERO, vec![ZERO; LcFork::Gloas.execution_branch_len()])
    };
    Ok(LightClientHeader { beacon: pre.beacon, execution: Execution::BlockHash { block_hash, branch } })
}

/// Upgrade a header from layout `from` to layout `to`, one fork at a time.
pub fn upgrade_header(
    header: LightClientHeader,
    from: LcFork,
    to: LcFork,
    spec: &ChainSpec,
) -> Result<LightClientHeader, Error> {
    if from > to || !header.has_layout(from) {
        return Err(Error::ForkMismatch);
    }
    let mut fork = from;
    let mut h = header;
    while fork < to {
        let next = fork.next().ok_or(Error::ForkMismatch)?;
        h = match next {
            // `upgrade_lc_header_to_capella`: empty execution data.
            LcFork::Capella => LightClientHeader {
                beacon: h.beacon,
                execution: Execution::Payload {
                    header: ExecutionPayloadHeader::empty(),
                    branch: vec![ZERO; LcFork::Capella.execution_branch_len()],
                },
            },
            // `upgrade_lc_header_to_deneb`: blob-gas fields set to zero.
            LcFork::Deneb => match h.execution {
                Execution::Payload { mut header, branch } => {
                    header.blob_gas_used = 0;
                    header.excess_blob_gas = 0;
                    LightClientHeader { beacon: h.beacon, execution: Execution::Payload { header, branch } }
                }
                _ => return Err(Error::ForkMismatch),
            },
            // `upgrade_lc_header_to_electra`: unchanged.
            LcFork::Electra => h,
            LcFork::Gloas => header_to_gloas(h, spec)?,
            LcFork::Altair => return Err(Error::ForkMismatch),
        };
        fork = next;
    }
    Ok(h)
}

/// Pad a state branch when crossing Electra (state grew one level) and Gloas (EIP-7688).
fn upgrade_state_branch(
    branch: Vec<Root>,
    from: LcFork,
    to: LcFork,
    electra_gindex: u64,
    gloas_gindex: u64,
) -> Vec<Root> {
    let mut branch = branch;
    if from < LcFork::Electra && to >= LcFork::Electra {
        branch = normalize_merkle_branch(&branch, electra_gindex);
    }
    if from < LcFork::Gloas && to >= LcFork::Gloas {
        branch = normalize_merkle_branch(&branch, gloas_gindex);
    }
    branch
}

/// `upgrade_lc_bootstrap_to_*`, composed from `from` to `to`.
pub fn upgrade_bootstrap(
    pre: LightClientBootstrap,
    from: LcFork,
    to: LcFork,
    spec: &ChainSpec,
) -> Result<LightClientBootstrap, Error> {
    if pre.current_sync_committee_branch.len() != from.sync_committee_branch_len() {
        return Err(Error::ForkMismatch);
    }
    Ok(LightClientBootstrap {
        header: upgrade_header(pre.header, from, to, spec)?,
        current_sync_committee: pre.current_sync_committee,
        current_sync_committee_branch: upgrade_state_branch(
            pre.current_sync_committee_branch,
            from,
            to,
            CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA,
            CURRENT_SYNC_COMMITTEE_GINDEX_GLOAS,
        ),
    })
}

/// `upgrade_lc_update_to_*`, composed from `from` to `to`.
pub fn upgrade_update(
    pre: LightClientUpdate,
    from: LcFork,
    to: LcFork,
    spec: &ChainSpec,
) -> Result<LightClientUpdate, Error> {
    if pre.next_sync_committee_branch.len() != from.sync_committee_branch_len()
        || pre.finality_branch.len() != from.finality_branch_len()
    {
        return Err(Error::ForkMismatch);
    }
    Ok(LightClientUpdate {
        attested_header: upgrade_header(pre.attested_header, from, to, spec)?,
        next_sync_committee: pre.next_sync_committee,
        next_sync_committee_branch: upgrade_state_branch(
            pre.next_sync_committee_branch,
            from,
            to,
            NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA,
            NEXT_SYNC_COMMITTEE_GINDEX_GLOAS,
        ),
        finalized_header: upgrade_header(pre.finalized_header, from, to, spec)?,
        finality_branch: upgrade_state_branch(
            pre.finality_branch,
            from,
            to,
            FINALIZED_ROOT_GINDEX_ELECTRA,
            FINALIZED_ROOT_GINDEX_GLOAS,
        ),
        sync_aggregate: pre.sync_aggregate,
        signature_slot: pre.signature_slot,
    })
}
