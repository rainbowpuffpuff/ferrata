//! The Ethereum side: the vault's calls and events, a keystore signer, and the download of the
//! vault's whole deposit list from any RPC, checked against the vault's own `depositRoot`.

use std::path::Path;

use alloy::{
    eips::BlockId,
    network::{EthereumWallet, TransactionBuilder as _},
    primitives::{Address, Bytes, B256, U256},
    providers::{DynProvider, Provider, ProviderBuilder},
    rpc::types::{Filter, TransactionReceipt, TransactionRequest},
    signers::local::PrivateKeySigner,
    sol,
    sol_types::{SolCall, SolEvent},
    transports::{RpcError, TransportErrorKind},
};
use anyhow::{anyhow, bail, ensure, Context as _, Result};
use ferrata_bridge_core::{inbound::VaultDeposit, tree::IncrementalTree};

use crate::codes::{vault_refusal, Refusal};

sol! {
    /// The parts of FerrataVault (vault/src/FerrataVault.sol) the tool calls.
    #[allow(missing_docs)]
    interface IVault {
        #[derive(Debug, PartialEq, Eq)]
        struct ReleaseRequest {
            address token;
            uint8 denomIndex;
            address recipient;
            address relayer;
            uint256 relayFee;
            bytes32 burnRoot;
            bytes32 nullifier;
        }

        event Deposit(address indexed depositor, uint64 index, uint64 leafIndex, address indexed token, uint256 amount, bytes32 commitment);
        event Release(bytes32 indexed nullifier, address indexed token, address indexed recipient, uint256 amount, address relayer, uint256 relayFee, bytes32 burnRoot);
        event Refund(uint64 indexed leafIndex, address indexed depositor, address indexed token, uint256 amount);
        event CheckpointFinalized(uint256 indexed index, bytes32 indexed newState, bytes32 parentState, bytes32 burnRoot, uint64 burnCount, uint64 importedLeafCount, bool sunset, uint64 tipId, bytes32 tipHash, uint32 n, uint32 t);

        function deposit(address token, uint8 denomIndex, bytes32 commitment, uint64 index) external;
        function depositETH(uint8 denomIndex, bytes32 commitment, uint64 index) external payable;
        function release(ReleaseRequest r, bytes seal) external;

        function depositCount(address depositor) external view returns (uint64);
        function nextLeafIndex() external view returns (uint64);
        function depositRoot() external view returns (bytes32);
        function nullifierUsed(bytes32 nullifier) external view returns (bool);
        function escrowed(address token) external view returns (uint256);
        function tokenConfig(address token) external view returns (bool registered, bool depositsEnabled, uint256 lezUnit, uint256 unitWeight, uint256[] denominations_);
        function fees(address token, uint256 index, uint8 denomIndex) external view returns (uint256 mintFee, uint256 relayFee, uint256 burnFee, uint256 checkpointFee);
        function feeState(address token) external view returns (uint64 current, uint64 pendingAt);
        function lezConfigId() external view returns (bytes32);
        function retired() external view returns (bool);
        function depositsFrozenUntil() external view returns (uint256);
        function releasesFrozenUntil() external view returns (uint256);
        function checkpointCount() external view returns (uint256);
        function finalImportedLeafCount() external view returns (uint64);
        function burnRootStatus(bytes32 burnRoot) external view returns (bool accepted, uint256 finalizedAt);
        function releaseDelayFor(address token, uint256 checkpointIndex) external view returns (uint256);
        function tokenReleasesFrozenUntil(address token) external view returns (uint256);
        function tokenDepositsFrozenUntil(address token) external view returns (uint256);
        function tokenCap(address token, uint8 direction) external view returns (uint256 capacity, uint256 refillPerSecond, uint256 available);
        function globalCap(uint8 direction) external view returns (uint256 capacity, uint256 refillPerSecond, uint256 available);
    }

    #[allow(missing_docs)]
    interface IERC20Meta {
        function decimals() external view returns (uint8);
        function symbol() external view returns (string);
        function allowance(address owner, address spender) external view returns (uint256);
        function approve(address spender, uint256 amount) external returns (bool);
    }
}

/// The native token's address in the vault.
pub const ETH: Address = Address::ZERO;

/// A read-only provider for `url`.
pub fn provider(url: &str) -> Result<DynProvider> {
    let url = url.parse().with_context(|| format!("not a URL: {url}"))?;
    Ok(ProviderBuilder::new().connect_http(url).erased())
}

/// The keystore at `keystore` (a Web3 Secret Storage file), decrypted with the password in
/// `password_file`. The key never appears on a command line.
pub fn keystore_signer(keystore: &Path, password_file: &Path) -> Result<PrivateKeySigner> {
    let password = std::fs::read_to_string(password_file)
        .with_context(|| format!("reading the password file {}", password_file.display()))?;
    let password = password.trim_end_matches(['\n', '\r']);
    PrivateKeySigner::decrypt_keystore(keystore, password)
        .map_err(|e| anyhow!("the keystore {} does not open with that password: {e}", keystore.display()))
}

/// A provider that signs with `signer`.
pub fn signing_provider(url: &str, signer: PrivateKeySigner) -> Result<DynProvider> {
    let url = url.parse().with_context(|| format!("not a URL: {url}"))?;
    Ok(ProviderBuilder::new().wallet(EthereumWallet::from(signer)).connect_http(url).erased())
}

/// Whether an RPC failure is the endpoint's and may pass on a retry: a transport failure (a closed connection, a
/// timeout, an HTTP 429 or 5xx), a body that is not the JSON-RPC answer, a rate limit (-32005), or a node that lacks
/// the block or state asked for (a load balancer's backend behind: "header not found", "block not found", "missing
/// trie node"; -32001). A revert, or any other error the node answers, is not.
pub fn transient(e: &RpcError<TransportErrorKind>) -> bool {
    match e {
        RpcError::Transport(_) | RpcError::DeserError { .. } | RpcError::NullResp => true,
        RpcError::ErrorResp(payload) => {
            if payload.as_revert_data().is_some() {
                return false;
            }
            let m = payload.message.to_lowercase();
            payload.code == -32005
                || payload.code == -32001
                || ["header not found", "block not found", "missing trie node", "too many requests", "unknown block"]
                    .iter()
                    .any(|t| m.contains(t))
        }
        _ => false,
    }
}

/// `f` until it succeeds or fails for good: up to four attempts, 0.1 s, 0.3 s and 0.9 s apart, while the failure is
/// [`transient`]. One read the endpoint drops does not fail the whole round it is part of (R.06).
pub async fn retry_rpc<T, F, Fut>(mut f: F) -> std::result::Result<T, RpcError<TransportErrorKind>>
where
    F: FnMut() -> Fut,
    Fut: std::future::IntoFuture<Output = std::result::Result<T, RpcError<TransportErrorKind>>>,
{
    let mut delay = 100u64;
    let mut attempt = 0;
    loop {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) if attempt < 3 && transient(&e) => {
                attempt += 1;
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                delay *= 3;
            }
            Err(e) => return Err(e),
        }
    }
}

/// The vault error behind an RPC error, if the node returned revert data.
pub fn refusal_of(error: &RpcError<TransportErrorKind>, how: &str) -> Option<Refusal> {
    let data = error.as_error_resp().and_then(|payload| payload.as_revert_data())?;
    vault_refusal(&data, how)
}

/// `eth_call` of `call` at `to`, at block `at` (latest if `None`).
pub async fn view<C: SolCall>(p: &DynProvider, to: Address, call: C, at: Option<u64>) -> Result<C::Return> {
    let tx = TransactionRequest::default().with_to(to).with_input(call.abi_encode());
    let out = retry_rpc(|| {
        let request = p.call(tx.clone());
        async move {
            match at {
                Some(n) => request.block(BlockId::number(n)).await,
                None => request.await,
            }
        }
    })
    .await;
    let out = out.map_err(|e| match refusal_of(&e, "eth_call") {
        Some(r) => anyhow!(r),
        None => anyhow!("eth_call {}: {e}", C::SIGNATURE),
    })?;
    C::abi_decode_returns(&out).map_err(|e| anyhow!("{} returned something else: {e}", C::SIGNATURE))
}

/// A token's decimals and symbol (18 and "ETH" for the native token).
pub async fn token_meta(p: &DynProvider, token: Address) -> Result<(u8, String)> {
    if token == ETH {
        return Ok((18, "ETH".to_string()));
    }
    let decimals = view(p, token, IERC20Meta::decimalsCall {}, None).await?;
    let symbol = view(p, token, IERC20Meta::symbolCall {}, None).await.unwrap_or_else(|_| "tokens".to_string());
    Ok((decimals, symbol))
}

/// What a deposit of `denom_index` of `token` costs now: the denomination and the mint and relay
/// fees of the fee schedule in force (D13).
#[derive(Clone, Debug)]
pub struct DepositPrice {
    pub denomination: u128,
    pub mint_fee: u128,
    pub relay_fee: u128,
    pub schedule: u64,
    pub total: u128,
}

pub async fn deposit_price(p: &DynProvider, vault: Address, token: Address, denom_index: u8) -> Result<DepositPrice> {
    let cfg = view(p, vault, IVault::tokenConfigCall { token }, None).await?;
    ensure!(
        cfg.registered,
        crate::codes::vault_refusal(&crate::codes::selector("TokenNotRegistered"), "checked before sending")
            .expect("known")
    );
    let denomination = cfg.denominations_.get(usize::from(denom_index)).ok_or_else(|| {
        anyhow!(vault_refusal(&crate::codes::selector("InvalidDenominationIndex"), "checked before sending")
            .expect("known"))
    })?;
    let state = view(p, vault, IVault::feeStateCall { token }, None).await?;
    let now = p.get_block(BlockId::latest()).await?.map(|b| b.header.timestamp).unwrap_or(0);
    let schedule = if state.pendingAt != 0 && now >= state.pendingAt { state.current + 1 } else { state.current };
    let fees =
        view(p, vault, IVault::feesCall { token, index: U256::from(schedule), denomIndex: denom_index }, None).await?;
    let denomination: u128 = (*denomination).try_into()?;
    let mint_fee: u128 = fees.mintFee.try_into()?;
    let relay_fee: u128 = fees.relayFee.try_into()?;
    Ok(DepositPrice { denomination, mint_fee, relay_fee, schedule, total: denomination + mint_fee + relay_fee })
}

/// The calldata of a deposit.
pub fn deposit_calldata(token: Address, denom_index: u8, commitment: B256, j: u64) -> Bytes {
    if token == ETH {
        IVault::depositETHCall { denomIndex: denom_index, commitment, index: j }.abi_encode().into()
    } else {
        IVault::depositCall { token, denomIndex: denom_index, commitment, index: j }.abi_encode().into()
    }
}

/// Sends `input` to `to` with `value` from the provider's signer, after an `eth_call` from the
/// same sender: a revert is reported with the vault's error before anything is signed.
pub async fn send(
    p: &DynProvider,
    from: Address,
    to: Address,
    input: Bytes,
    value: U256,
    confirmations: u64,
) -> Result<TransactionReceipt> {
    let tx = TransactionRequest::default().with_from(from).with_to(to).with_input(input).with_value(value);
    if let Err(e) = p.call(tx.clone()).await {
        match refusal_of(&e, "eth_call before sending") {
            Some(r) => bail!(r),
            None => bail!("the vault would revert: {e}"),
        }
    }
    let pending = p.send_transaction(tx).await.map_err(|e| match refusal_of(&e, "eth_sendTransaction") {
        Some(r) => anyhow!(r),
        None => anyhow!("sending: {e}"),
    })?;
    let receipt = pending.with_required_confirmations(confirmations.max(1)).get_receipt().await?;
    ensure!(receipt.status(), "the transaction {} reverted", receipt.transaction_hash);
    Ok(receipt)
}

/// The vault's deposit list as one block saw it.
#[derive(Clone, Debug)]
pub struct DepositList {
    pub block: u64,
    pub chain_id: u64,
    pub deposits: Vec<VaultDeposit>,
    /// The block of each deposit, by leaf index (from the same logs: no per-deposit query).
    pub blocks: Vec<u64>,
    /// The vault's `depositRoot` at `block`, equal to the root of `deposits` (checked).
    pub root: B256,
}

/// Downloads every `Deposit` event of `vault` up to the latest block, in ranges of `chunk`
/// blocks from `from_block`, and checks the list against the vault: `nextLeafIndex` deposits,
/// leaf indices 0, 1, 2 …, and the root of their leaves equal to `depositRoot` at that block. A
/// provider that leaves a deposit out, adds one or changes one is caught. No per-item query: the
/// provider learns only that the whole list was read.
pub async fn deposit_list(p: &DynProvider, vault: Address, from_block: u64, chunk: u64) -> Result<DepositList> {
    let chain_id = p.get_chain_id().await?;
    let block = p.get_block_number().await?;
    let mut deposits = Vec::new();
    let mut start = from_block;
    while start <= block {
        let end = (start + chunk.max(1) - 1).min(block);
        let filter = Filter::new()
            .address(vault)
            .event_signature(IVault::Deposit::SIGNATURE_HASH)
            .from_block(start)
            .to_block(end);
        for log in p.get_logs(&filter).await.with_context(|| format!("eth_getLogs {start}..{end}"))? {
            let at = log.block_number.unwrap_or(end);
            let event = log
                .log_decode::<IVault::Deposit>()
                .map_err(|e| anyhow!("a Deposit log does not decode: {e}"))?
                .inner
                .data;
            deposits.push((
                at,
                VaultDeposit {
                    depositor: event.depositor.into_array(),
                    j: event.index,
                    leaf_index: event.leafIndex,
                    token: event.token.into_array(),
                    amount: event.amount.to_be_bytes::<32>(),
                    commitment: event.commitment.0,
                },
            ));
        }
        start = end + 1;
    }
    deposits.sort_by_key(|(_, d)| d.leaf_index);
    let (blocks, deposits): (Vec<u64>, Vec<VaultDeposit>) = deposits.into_iter().unzip();
    let count = view(p, vault, IVault::nextLeafIndexCall {}, Some(block)).await?;
    let root = view(p, vault, IVault::depositRootCall {}, Some(block)).await?;
    ensure!(
        deposits.len() as u64 == count,
        "the provider returned {} Deposit events, the vault holds {count} leaves at block {block}: the list is incomplete (try a smaller --chunk or another RPC)",
        deposits.len()
    );
    let mut tree = IncrementalTree::new();
    for (i, d) in deposits.iter().enumerate() {
        ensure!(d.leaf_index == i as u64, "leaf {i} is missing from the provider's list");
        tree.append(ferrata_bridge_core::inbound::deposit_leaf(
            d.leaf_index,
            &d.depositor,
            &d.token,
            &d.amount,
            &d.commitment,
        ))
        .map_err(|_| anyhow!("more than 2^32 leaves"))?;
    }
    ensure!(
        tree.root() == root.0,
        "the deposits the provider returned do not hash to the vault's depositRoot at block {block}: a changed or forged list"
    );
    Ok(DepositList { block, chain_id, deposits, blocks, root })
}

/// A burn root the vault accepts, with what the wallet's release rule needs.
#[derive(Clone, Copy, Debug)]
pub struct AcceptedRoot {
    pub root: B256,
    /// Burns it covers (burn indices below it).
    pub count: u64,
    /// Its checkpoint index.
    pub index: u64,
    /// When its checkpoint was finalised (Ethereum time).
    pub finalized_at: u64,
}

/// Every accepted (finalised, not revoked) burn root, oldest first, from the vault's
/// `CheckpointFinalized` events (one download; the vault's `burnRootStatus` confirms each).
pub async fn accepted_roots(p: &DynProvider, vault: Address, from_block: u64, chunk: u64) -> Result<Vec<AcceptedRoot>> {
    let block = p.get_block_number().await?;
    let mut out = Vec::new();
    let mut start = from_block;
    while start <= block {
        let end = (start + chunk.max(1) - 1).min(block);
        let filter = Filter::new()
            .address(vault)
            .event_signature(IVault::CheckpointFinalized::SIGNATURE_HASH)
            .from_block(start)
            .to_block(end);
        for log in p.get_logs(&filter).await.with_context(|| format!("eth_getLogs {start}..{end}"))? {
            let e = log
                .log_decode::<IVault::CheckpointFinalized>()
                .map_err(|e| anyhow!("a CheckpointFinalized log does not decode: {e}"))?
                .inner
                .data;
            let status = view(p, vault, IVault::burnRootStatusCall { burnRoot: e.burnRoot }, None).await?;
            if status.accepted {
                out.push(AcceptedRoot {
                    root: e.burnRoot,
                    count: e.burnCount,
                    index: u64::try_from(e.index).unwrap_or(u64::MAX),
                    finalized_at: u64::try_from(status.finalizedAt).unwrap_or(u64::MAX),
                });
            }
        }
        start = end + 1;
    }
    Ok(out)
}

/// What a call would cost, checked with `eth_call` first: a refusal is reported with the vault's error.
#[derive(Clone, Copy, Debug)]
pub struct Preflight {
    pub gas: u64,
    pub gas_price: u128,
}

impl Preflight {
    /// The network fee in wei at the current gas price.
    pub fn fee(&self) -> u128 {
        u128::from(self.gas) * self.gas_price
    }
}

/// `eth_call`, then `eth_estimateGas` and `eth_gasPrice`, for a call from `from`; nothing is signed.
pub async fn preflight(p: &DynProvider, from: Address, to: Address, input: Bytes, value: U256) -> Result<Preflight> {
    let tx = TransactionRequest::default().with_from(from).with_to(to).with_input(input).with_value(value);
    if let Err(e) = p.call(tx.clone()).await {
        match refusal_of(&e, "eth_call, nothing sent") {
            Some(r) => bail!(r),
            None => bail!("the vault would revert: {e}"),
        }
    }
    let gas = p.estimate_gas(tx).await.context("eth_estimateGas")?;
    let gas_price = p.get_gas_price().await.context("eth_gasPrice")?;
    Ok(Preflight { gas, gas_price })
}

/// The latest block's time.
pub async fn now(p: &DynProvider) -> Result<u64> {
    Ok(p.get_block(BlockId::latest()).await?.ok_or_else(|| anyhow!("no latest block"))?.header.timestamp)
}

/// Every nullifier the vault has released, from its `Release` events (one download, matched
/// locally).
pub async fn released_nullifiers(
    p: &DynProvider,
    vault: Address,
    from_block: u64,
    chunk: u64,
) -> Result<std::collections::HashSet<[u8; 32]>> {
    let block = p.get_block_number().await?;
    let mut out = std::collections::HashSet::new();
    let mut start = from_block;
    while start <= block {
        let end = (start + chunk.max(1) - 1).min(block);
        let filter = Filter::new()
            .address(vault)
            .event_signature(IVault::Release::SIGNATURE_HASH)
            .from_block(start)
            .to_block(end);
        for log in p.get_logs(&filter).await.with_context(|| format!("eth_getLogs {start}..{end}"))? {
            let event = log
                .log_decode::<IVault::Release>()
                .map_err(|e| anyhow!("a Release log does not decode: {e}"))?
                .inner
                .data;
            out.insert(event.nullifier.0);
        }
        start = end + 1;
    }
    Ok(out)
}

/// Leaf indices the vault has refunded, from its `Refund` events.
pub async fn refunded_leaves(
    p: &DynProvider,
    vault: Address,
    from_block: u64,
    chunk: u64,
) -> Result<std::collections::HashSet<u64>> {
    let block = p.get_block_number().await?;
    let mut out = std::collections::HashSet::new();
    let mut start = from_block;
    while start <= block {
        let end = (start + chunk.max(1) - 1).min(block);
        let filter = Filter::new()
            .address(vault)
            .event_signature(IVault::Refund::SIGNATURE_HASH)
            .from_block(start)
            .to_block(end);
        for log in p.get_logs(&filter).await.with_context(|| format!("eth_getLogs {start}..{end}"))? {
            let event = log
                .log_decode::<IVault::Refund>()
                .map_err(|e| anyhow!("a Refund log does not decode: {e}"))?
                .inner
                .data;
            out.insert(event.leafIndex);
        }
        start = end + 1;
    }
    Ok(out)
}

/// Finalized checkpoints from the vault's `CheckpointFinalized` events: (burn root, burn count),
/// oldest first, each still accepted (`burnRootStatus`: not revoked).
pub async fn accepted_burn_roots(
    p: &DynProvider,
    vault: Address,
    from_block: u64,
    chunk: u64,
) -> Result<Vec<(B256, u64)>> {
    let block = p.get_block_number().await?;
    let mut out = Vec::new();
    let mut start = from_block;
    while start <= block {
        let end = (start + chunk.max(1) - 1).min(block);
        let filter = Filter::new()
            .address(vault)
            .event_signature(IVault::CheckpointFinalized::SIGNATURE_HASH)
            .from_block(start)
            .to_block(end);
        for log in p.get_logs(&filter).await.with_context(|| format!("eth_getLogs {start}..{end}"))? {
            let e = log
                .log_decode::<IVault::CheckpointFinalized>()
                .map_err(|e| anyhow!("a CheckpointFinalized log does not decode: {e}"))?
                .inner
                .data;
            let status = view(p, vault, IVault::burnRootStatusCall { burnRoot: e.burnRoot }, None).await?;
            if status.accepted {
                out.push((e.burnRoot, e.burnCount));
            }
        }
        start = end + 1;
    }
    Ok(out)
}

/// The calldata of `release(r, seal)`.
pub fn release_calldata(r: IVault::ReleaseRequest, seal: Bytes) -> Bytes {
    IVault::releaseCall { r, seal }.abi_encode().into()
}

/// The first `Deposit` event in a receipt (the deposit just sent).
pub fn deposit_of_receipt(receipt: &TransactionReceipt) -> Option<IVault::Deposit> {
    receipt.inner.logs().iter().find_map(|log| log.log_decode::<IVault::Deposit>().ok().map(|l| l.inner.data))
}

/// An address from 20 bytes.
pub fn address(bytes: [u8; 20]) -> Address {
    Address::from(bytes)
}
