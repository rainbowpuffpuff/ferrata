//! The Ethereum side: the vault's events, the views the monitor reads, and its three transactions
//! (`cancelCandidate`, `pauseCheckpoints`, and `revokeFrom` when its key is the freeze authority), sent with the
//! watcher key over `--rpc` (the plain path) or through Logos Core's `tx_sender_module` (`--send-via logos`,
//! [`crate::logos_send`]).

// The event constructors `sol!` generates take one argument per field.
#![allow(clippy::too_many_arguments)]

use alloy::{
    eips::BlockId,
    network::{EthereumWallet, TransactionBuilder as _},
    primitives::{Address, Bytes, B256, U256},
    providers::{DynProvider, Provider, ProviderBuilder},
    rpc::types::{Filter, TransactionRequest},
    signers::local::PrivateKeySigner,
    sol,
    sol_types::{SolCall, SolEvent},
};
use anyhow::{anyhow, bail, Context, Result};

use crate::check::{Candidate, Rule};
use crate::logos_send::{Outcome, Route, Via};
use crate::node::H32;

sol! {
    interface IFerrataVault {
        event CandidateSubmitted(
            bytes32 indexed newState,
            bytes32 indexed parentState,
            bytes32 burnRoot,
            uint64 burnCount,
            uint64 importedLeafCount,
            bool sunset,
            uint256 deltaAtSubmit,
            uint64 tipId,
            bytes32 tipHash,
            uint32 n,
            uint32 t
        );
        event CheckpointFinalized(
            uint256 indexed index,
            bytes32 indexed newState,
            bytes32 parentState,
            bytes32 burnRoot,
            uint64 burnCount,
            uint64 importedLeafCount,
            bool sunset,
            uint64 tipId,
            bytes32 tipHash,
            uint32 n,
            uint32 t
        );
        event CandidateCancelled(bytes32 indexed newState, uint64 tipId, bytes32 tipHash);
        event CheckpointsRevoked(uint256 indexed fromIndex, uint256 toIndex, bytes32 restoredState);

        function mode() external view returns (uint8);
        function turnSeconds() external view returns (uint256);
        function finalState() external view returns (bytes32);
        function freezeAuthority() external view returns (address);
        function watchers(uint256 i) external view returns (address);
        function candidates(bytes32 newState) external view returns (
            bytes32 journalDigest,
            bytes32 tipHash,
            uint64 tipId,
            uint32 t,
            uint48 submittedAt,
            uint32 deltaAtSubmit,
            uint64 epoch,
            address prover
        );
        function checkpoints(uint256 i) external view returns (
            bytes32 parentState,
            bytes32 newState,
            bytes32 burnRoot,
            uint64 burnCount,
            uint64 importedLeafCount,
            bool sunset,
            bool revoked,
            uint64 parentImportedLeafCount,
            bool parentSunset,
            uint64 finalizedAt,
            address prover
        );
        function revokeWindow() external view returns (uint256);
        function checkpointsFrozenUntil() external view returns (uint256);
        function lezConfigId() external view returns (bytes32);
        function cancelCandidate(bytes32 newState) external;
        function pauseCheckpoints() external;
        function revokeFrom(uint256 index) external;
    }
}

/// A vault event the monitor follows, in log order.
#[derive(Clone, Debug)]
pub enum VaultEvent {
    /// `submitted_at` is filled from the vault's `candidates` when the monitor first checks it.
    Submitted {
        candidate: Candidate,
        block: u64,
    },
    Finalized {
        index: u64,
        candidate: Candidate,
        block: u64,
    },
    Cancelled {
        new_state: H32,
        block: u64,
        tx: Option<B256>,
    },
    Revoked {
        from_index: u64,
        to_index: u64,
        block: u64,
    },
}

/// A candidate as the vault stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OnChain {
    pub submitted_at: u64,
    pub delta_at_submit: u64,
    pub tip_id: u64,
    pub tip_hash: H32,
    pub t: u32,
}

/// What a transaction attempt came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sent {
    /// Mined and succeeded: transaction hash, block number, block time, and the path it went by.
    Mined { tx: B256, block: u64, time: u64, via: Via },
    /// Not needed any more (the candidate is gone, the checkpoint already revoked).
    Moot,
}

pub struct Chain {
    pub p: DynProvider,
    pub vault: Address,
    /// The monitor's own address, when it holds a key.
    pub me: Option<Address>,
    /// How long a sent transaction may stay unmined before the monitor gives up on it for this poll
    /// (alerted; the next poll looks at the vault again), so a stuck transaction never stops the loop.
    pub tx_timeout: std::time::Duration,
    /// `--send-via logos`: the transactions go through Logos Core's `tx_sender_module` from this account.
    pub logos: Option<Route>,
}

fn h(b: B256) -> H32 {
    H32(b.0)
}

impl Chain {
    pub fn connect(url: &str, vault: Address, signer: Option<PrivateKeySigner>) -> Result<Self> {
        let me = signer.as_ref().map(PrivateKeySigner::address);
        let p = match signer {
            Some(s) => ProviderBuilder::new().wallet(EthereumWallet::from(s)).connect_http(url.parse()?).erased(),
            None => ProviderBuilder::new().connect_http(url.parse()?).erased(),
        };
        Ok(Self { p, vault, me, tx_timeout: std::time::Duration::from_secs(180), logos: None })
    }

    pub async fn view<C: SolCall>(&self, call: C) -> Result<C::Return> {
        let tx = TransactionRequest::default().with_to(self.vault).with_input(call.abi_encode());
        let out = self.p.call(tx).await.map_err(|e| anyhow!("eth_call {}: {e}", C::SIGNATURE))?;
        C::abi_decode_returns(&out).map_err(|e| anyhow!("{} returned something else: {e}", C::SIGNATURE))
    }

    /// The latest block's number and time (the monitor's clock is the chain's).
    pub async fn head(&self) -> Result<(u64, u64)> {
        let b = self.p.get_block(BlockId::latest()).await?.context("no latest block")?;
        Ok((b.header.number, b.header.timestamp))
    }

    pub async fn rule(&self) -> Result<Rule> {
        match self.view(IFerrataVault::modeCall {}).await? {
            0 => Ok(Rule::A),
            1 => Ok(Rule::B),
            m => bail!("the vault reports mode {m}"),
        }
    }

    pub async fn turn_seconds(&self) -> Result<u64> {
        Ok(self.view(IFerrataVault::turnSecondsCall {}).await?.to::<u64>())
    }

    pub async fn revoke_window(&self) -> Result<u64> {
        Ok(self.view(IFerrataVault::revokeWindowCall {}).await?.to::<u64>())
    }

    pub async fn final_state(&self) -> Result<H32> {
        Ok(h(self.view(IFerrataVault::finalStateCall {}).await?))
    }

    pub async fn lez_config_id(&self) -> Result<H32> {
        Ok(h(self.view(IFerrataVault::lezConfigIdCall {}).await?))
    }

    pub async fn freeze_authority(&self) -> Result<Address> {
        self.view(IFerrataVault::freezeAuthorityCall {}).await
    }

    pub async fn watchers(&self) -> Result<Vec<Address>> {
        let mut out = Vec::new();
        for i in 0..3u64 {
            out.push(self.view(IFerrataVault::watchersCall { i: U256::from(i) }).await?);
        }
        Ok(out)
    }

    pub async fn checkpoints_frozen_until(&self) -> Result<u64> {
        Ok(self.view(IFerrataVault::checkpointsFrozenUntilCall {}).await?.to::<u64>())
    }

    /// The pending candidate at `new_state`, if any.
    pub async fn pending(&self, new_state: H32) -> Result<Option<OnChain>> {
        let r = self.view(IFerrataVault::candidatesCall { newState: B256::from(new_state.0) }).await?;
        if r.submittedAt.to::<u64>() == 0 {
            return Ok(None);
        }
        Ok(Some(OnChain {
            submitted_at: r.submittedAt.to::<u64>(),
            delta_at_submit: u64::from(r.deltaAtSubmit),
            tip_id: r.tipId,
            tip_hash: h(r.tipHash),
            t: r.t,
        }))
    }

    /// Whether checkpoint `index` is revoked, and when it was finalised.
    pub async fn checkpoint(&self, index: u64) -> Result<(bool, u64)> {
        let r = self.view(IFerrataVault::checkpointsCall { i: U256::from(index) }).await?;
        Ok((r.revoked, r.finalizedAt))
    }

    /// The vault's events in blocks `from..=to`, in log order, in chunks of `chunk` blocks.
    pub async fn events(&self, from: u64, to: u64, chunk: u64) -> Result<Vec<VaultEvent>> {
        let mut out = Vec::new();
        let mut start = from;
        while start <= to {
            let end = (start + chunk.max(1) - 1).min(to);
            let filter = Filter::new()
                .address(self.vault)
                .event_signature(vec![
                    IFerrataVault::CandidateSubmitted::SIGNATURE_HASH,
                    IFerrataVault::CheckpointFinalized::SIGNATURE_HASH,
                    IFerrataVault::CandidateCancelled::SIGNATURE_HASH,
                    IFerrataVault::CheckpointsRevoked::SIGNATURE_HASH,
                ])
                .from_block(start)
                .to_block(end);
            let mut logs = self.p.get_logs(&filter).await.with_context(|| format!("eth_getLogs {start}..{end}"))?;
            logs.sort_by_key(|l| (l.block_number, l.log_index));
            for log in logs {
                let block = log.block_number.unwrap_or(end);
                let topic = log.topic0().copied().unwrap_or_default();
                if topic == IFerrataVault::CandidateSubmitted::SIGNATURE_HASH {
                    let e = log.log_decode::<IFerrataVault::CandidateSubmitted>()?.inner.data;
                    out.push(VaultEvent::Submitted {
                        candidate: Candidate {
                            new_state: h(e.newState),
                            parent_state: h(e.parentState),
                            burn_root: h(e.burnRoot),
                            burn_count: e.burnCount,
                            imported_leaf_count: e.importedLeafCount,
                            sunset: e.sunset,
                            delta_at_submit: e.deltaAtSubmit.to::<u64>(),
                            tip_id: e.tipId,
                            tip_hash: h(e.tipHash),
                            n: e.n,
                            t: e.t,
                            submitted_at: 0,
                        },
                        block,
                    });
                } else if topic == IFerrataVault::CheckpointFinalized::SIGNATURE_HASH {
                    let e = log.log_decode::<IFerrataVault::CheckpointFinalized>()?.inner.data;
                    out.push(VaultEvent::Finalized {
                        index: e.index.to::<u64>(),
                        candidate: Candidate {
                            new_state: h(e.newState),
                            parent_state: h(e.parentState),
                            burn_root: h(e.burnRoot),
                            burn_count: e.burnCount,
                            imported_leaf_count: e.importedLeafCount,
                            sunset: e.sunset,
                            delta_at_submit: 0,
                            tip_id: e.tipId,
                            tip_hash: h(e.tipHash),
                            n: e.n,
                            t: e.t,
                            submitted_at: 0,
                        },
                        block,
                    });
                } else if topic == IFerrataVault::CandidateCancelled::SIGNATURE_HASH {
                    let e = log.log_decode::<IFerrataVault::CandidateCancelled>()?.inner.data;
                    out.push(VaultEvent::Cancelled { new_state: h(e.newState), block, tx: log.transaction_hash });
                } else if topic == IFerrataVault::CheckpointsRevoked::SIGNATURE_HASH {
                    let e = log.log_decode::<IFerrataVault::CheckpointsRevoked>()?.inner.data;
                    out.push(VaultEvent::Revoked {
                        from_index: e.fromIndex.to::<u64>(),
                        to_index: e.toIndex.to::<u64>(),
                        block,
                    });
                }
            }
            start = end + 1;
        }
        Ok(out)
    }

    /// Simulates, then sends and waits for one confirmation. A revert in simulation is an error. With `--send-via
    /// logos` the transaction goes through `tx_sender_module`; one it did not broadcast, and one it cannot account for
    /// ([`Outcome::Unknown`]: `stuck`, failed after its leg began to leave, not withdrawn, a request it no longer
    /// holds), goes the plain way when the monitor holds the watcher key too (the fallback), and is an error
    /// otherwise (retried at the next poll). The plain send takes the node's next nonce: when it goes through, that
    /// fills a number the module burnt without broadcasting (its ledger never hands it out again, so its own later
    /// sends would wait behind the gap); if it fails too, that is not handled: the module's later sends come back as
    /// [`Outcome::Broadcast`], so this fallback does not run again, and they wait behind the number. Against a
    /// transaction of the module's that did leave, at most one extra call: a second `cancelCandidate` or
    /// `revokeFrom` reverts (usually already at the gas estimate, so nothing is sent), and a second
    /// `pauseCheckpoints` moves the pause's end by the seconds between the two blocks.
    async fn send(&self, input: Bytes, label: &'static str) -> Result<Sent> {
        let from = self.me.context("the monitor holds no key")?;
        let tx = TransactionRequest::default().with_from(from).with_to(self.vault).with_input(input.clone());
        self.p.call(tx.clone()).await.map_err(|e| anyhow!("the vault would revert: {e}"))?;
        let mut via = Via::Plain;
        if let Some(route) = self.logos.clone() {
            let vault = self.vault;
            let outcome = tokio::task::spawn_blocking(move || route.send(vault, &input, label))
                .await
                .map_err(|e| anyhow!("the send through Logos Core did not finish: {e}"))?;
            match outcome {
                Outcome::Broadcast { hash, request_id, polls, millis } => {
                    let receipt = self.receipt(hash).await?;
                    return self.mined(receipt, Via::Logos { request_id, polls, millis }).await;
                }
                Outcome::NotSent(why) | Outcome::Unknown { reason: why, .. }
                    if self.logos.as_ref().is_some_and(|r| r.fallback) =>
                {
                    via = Via::Fallback { reason: why }
                }
                Outcome::Unknown { reason: why, .. } => bail!("{why}"),
                Outcome::NotSent(why) => bail!("not sent: {why}"),
            }
        }
        let pending = self.p.send_transaction(tx).await.map_err(|e| anyhow!("sending: {e}"))?;
        let hash = *pending.tx_hash();
        let receipt = pending
            .with_timeout(Some(self.tx_timeout))
            .get_receipt()
            .await
            .map_err(|e| anyhow!("{hash:#x} not mined within {:?}: {e}", self.tx_timeout))?;
        self.mined(receipt, via).await
    }

    /// The receipt of a transaction another party broadcast, waited for on the monitor's own node.
    async fn receipt(&self, hash: B256) -> Result<alloy::rpc::types::TransactionReceipt> {
        let t0 = std::time::Instant::now();
        loop {
            if let Some(r) = self.p.get_transaction_receipt(hash).await? {
                return Ok(r);
            }
            if t0.elapsed() > self.tx_timeout {
                bail!("{hash:#x} not mined within {:?}", self.tx_timeout);
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }

    async fn mined(&self, receipt: alloy::rpc::types::TransactionReceipt, via: Via) -> Result<Sent> {
        if !receipt.status() {
            bail!("the transaction {} reverted", receipt.transaction_hash);
        }
        let block = receipt.block_number.unwrap_or_default();
        let time = self.p.get_block(BlockId::number(block)).await?.map(|b| b.header.timestamp).unwrap_or_default();
        Ok(Sent::Mined { tx: receipt.transaction_hash, block, time, via })
    }

    /// `cancelCandidate(newState)`; `Moot` if nothing is pending there any more.
    pub async fn cancel(&self, new_state: H32) -> Result<Sent> {
        if self.pending(new_state).await?.is_none() {
            return Ok(Sent::Moot);
        }
        let input = IFerrataVault::cancelCandidateCall { newState: B256::from(new_state.0) }.abi_encode().into();
        match self.send(input, "cancelCandidate").await {
            Ok(s) => Ok(s),
            // Finalised or cancelled by someone else between the check and the transaction.
            Err(_) if self.pending(new_state).await?.is_none() => Ok(Sent::Moot),
            Err(e) => Err(e),
        }
    }

    pub async fn pause(&self) -> Result<Sent> {
        self.send(IFerrataVault::pauseCheckpointsCall {}.abi_encode().into(), "pauseCheckpoints").await
    }

    /// `revokeFrom(index)` (the freeze authority's power); `Moot` if already revoked.
    pub async fn revoke_from(&self, index: u64) -> Result<Sent> {
        if self.checkpoint(index).await?.0 {
            return Ok(Sent::Moot);
        }
        self.send(IFerrataVault::revokeFromCall { index: U256::from(index) }.abi_encode().into(), "revokeFrom").await
    }
}
