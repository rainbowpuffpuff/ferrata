//! The vault, read through Logos Core's own `eth_rpc_module` (or, natively and as the fallback, a plain `http://`
//! endpoint): the module's Ethereum reads without the chain service (`session.rs`) and without the `ferrata` tool's
//! alloy client. The reads are those of `cli/src/eth.rs` (`deposit_price`, `deposit_list`, `accepted_roots`,
//! `released_nullifiers`, `refunded_leaves`) and of `seqnet-chain`'s `registry` and `info`, made here with the ABI
//! encoded by hand (the module crate keeps its few dependencies; its `Cargo.lock` is what the builder builds).
//!
//! Nothing read here is trusted for a proof: a claim's root is the one LEZ imported under an attested record, a
//! release's burn root is checked by the vault. The deposit list is checked as `eth.rs` checks it: `nextLeafIndex`
//! deposits, leaf indices 0, 1, 2 ..., and the root of their leaves equal to `depositRoot` at the same block.

use ferrata_bridge_core::{
    inbound,
    tree::IncrementalTree,
    Address, Hash, U256,
};
use serde_json::{json, Value};

use crate::rpc::Rpc;
use crate::{hex, unhex};

/// Keccak-256.
pub(crate) fn keccak(data: &[u8]) -> [u8; 32] {
    use tiny_keccak::{Hasher, Keccak};
    let mut k = Keccak::v256();
    k.update(data);
    let mut out = [0u8; 32];
    k.finalize(&mut out);
    out
}

/// An address in EIP-55 mixed case (what `tx_sender_module` and the signer manager key accounts by when they print them).
pub(crate) fn checksum(a: &Address) -> String {
    let lower: String = a.iter().map(|b| format!("{b:02x}")).collect();
    let h = keccak(lower.as_bytes());
    let mut out = String::from("0x");
    for (i, c) in lower.chars().enumerate() {
        let nibble = (h[i / 2] >> if i % 2 == 0 { 4 } else { 0 }) & 0xf;
        out.push(if c.is_ascii_alphabetic() && nibble >= 8 { c.to_ascii_uppercase() } else { c });
    }
    out
}

/// A function's selector: the first four bytes of the Keccak-256 of its signature.
pub(crate) fn selector(signature: &str) -> [u8; 4] {
    let h = keccak(signature.as_bytes());
    [h[0], h[1], h[2], h[3]]
}

/// One ABI value, as the encoder needs it: a word (every static elementary type), a dynamic array, dynamic
/// bytes, or a tuple (static or dynamic by its members; a fixed-size array `T[k]` is a tuple of k).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Abi {
    W([u8; 32]),
    Arr(Vec<Abi>),
    Bytes(Vec<u8>),
    Tup(Vec<Abi>),
}

impl Abi {
    pub(crate) fn addr(a: &Address) -> Abi {
        let mut w = [0u8; 32];
        w[12..].copy_from_slice(a);
        Abi::W(w)
    }
    pub(crate) fn uint(n: u128) -> Abi {
        let mut w = [0u8; 32];
        w[16..].copy_from_slice(&n.to_be_bytes());
        Abi::W(w)
    }
    pub(crate) fn boolean(b: bool) -> Abi {
        Abi::uint(u128::from(b))
    }
    fn dynamic(&self) -> bool {
        match self {
            Abi::W(_) => false,
            Abi::Arr(_) | Abi::Bytes(_) => true,
            Abi::Tup(v) => v.iter().any(Abi::dynamic),
        }
    }
    fn head_size(&self) -> usize {
        match self {
            Abi::Tup(v) if !self.dynamic() => v.iter().map(Abi::head_size).sum(),
            _ => 32,
        }
    }
    /// The value's own encoding (what goes in a tail, or inline when static).
    fn encode(&self) -> Vec<u8> {
        match self {
            Abi::W(w) => w.to_vec(),
            Abi::Tup(v) => encode_seq(v),
            Abi::Arr(v) => {
                let mut out = Abi::uint(v.len() as u128).encode();
                out.extend(encode_seq(v));
                out
            }
            Abi::Bytes(b) => {
                let mut out = Abi::uint(b.len() as u128).encode();
                out.extend_from_slice(b);
                out.resize(32 + b.len().div_ceil(32) * 32, 0);
                out
            }
        }
    }
}

/// The ABI's head-and-tail encoding of a sequence (a call's arguments, a tuple's members, an array's elements).
pub(crate) fn encode_seq(items: &[Abi]) -> Vec<u8> {
    let head_len: usize = items.iter().map(Abi::head_size).sum();
    let (mut head, mut tail) = (Vec::with_capacity(head_len), Vec::new());
    for item in items {
        if item.dynamic() {
            head.extend(Abi::uint((head_len + tail.len()) as u128).encode());
            tail.extend(item.encode());
        } else {
            head.extend(item.encode());
        }
    }
    head.extend(tail);
    head
}

/// A call's data: the signature's selector and its arguments.
pub(crate) fn calldata(signature: &str, args: &[Abi]) -> Vec<u8> {
    let mut out = selector(signature).to_vec();
    out.extend(encode_seq(args));
    out
}

/// The 32-byte word `i` of ABI-encoded data.
pub(crate) fn word(data: &[u8], i: usize) -> Result<[u8; 32], String> {
    data.get(i * 32..i * 32 + 32)
        .map(|w| w.try_into().expect("32 bytes"))
        .ok_or_else(|| format!("the ABI data has no word {i} ({} bytes)", data.len()))
}

pub(crate) fn as_u64(w: &[u8; 32]) -> Result<u64, String> {
    if w[..24].iter().any(|b| *b != 0) {
        return Err(format!("{} does not fit in 64 bits", hex(w)));
    }
    Ok(u64::from_be_bytes(w[24..].try_into().expect("8 bytes")))
}

pub(crate) fn as_u128(w: &[u8; 32]) -> Result<u128, String> {
    if w[..16].iter().any(|b| *b != 0) {
        return Err(format!("{} does not fit in 128 bits", hex(w)));
    }
    Ok(u128::from_be_bytes(w[16..].try_into().expect("16 bytes")))
}

pub(crate) fn as_address(w: &[u8; 32]) -> Address {
    w[12..].try_into().expect("20 bytes")
}

/// A 256-bit big-endian number in decimal (amounts are shown and sent to `tx_sender_module` as decimal wei).
pub(crate) fn dec(w: &[u8; 32]) -> String {
    let mut n = *w;
    let mut digits = Vec::new();
    loop {
        // n = n / 10, the remainder is the next digit.
        let mut rem = 0u32;
        for byte in n.iter_mut() {
            let cur = (rem << 8) | u32::from(*byte);
            *byte = (cur / 10) as u8;
            rem = cur % 10;
        }
        digits.push(b'0' + rem as u8);
        if n.iter().all(|b| *b == 0) {
            break;
        }
    }
    digits.reverse();
    String::from_utf8(digits).expect("ASCII digits")
}

/// A decimal amount as hex digits without leading zeros (`0` for zero), as JSON-RPC quantities are written.
pub(crate) fn dec_to_hex(decimal: &str) -> Result<String, String> {
    let w = crate::u256(&json!(decimal), "an amount")?;
    let h: String = w.iter().map(|b| format!("{b:02x}")).collect();
    let t = h.trim_start_matches('0');
    Ok(if t.is_empty() { "0".to_string() } else { t.to_string() })
}

/// The dynamic `uint256[]` whose offset is head word `i` of `data`.
fn word_array(data: &[u8], i: usize) -> Result<Vec<[u8; 32]>, String> {
    let offset = as_u64(&word(data, i)?)? as usize;
    if offset % 32 != 0 {
        return Err(format!("an array offset {offset} that is not a whole word"));
    }
    let base = offset / 32;
    let n = as_u64(&word(data, base)?)? as usize;
    (0..n).map(|k| word(data, base + 1 + k)).collect()
}

/// A hex word or quantity (`0x1`, `0x0a…`: JSON-RPC quantities have no leading zeros), right-aligned in 32 bytes.
fn hex32(v: &Value, what: &str) -> Result<[u8; 32], String> {
    let text = v.as_str().ok_or_else(|| format!("{what}: a hex string"))?;
    let digits = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")).unwrap_or(text);
    let even = if digits.len() % 2 == 1 { format!("0{digits}") } else { digits.to_string() };
    let b = unhex(&even).map_err(|e| format!("{what}: {e}"))?;
    let mut w = [0u8; 32];
    if b.len() > 32 {
        return Err(format!("{what}: more than 32 bytes"));
    }
    w[32 - b.len()..].copy_from_slice(&b);
    Ok(w)
}

fn quantity(n: u64) -> String {
    format!("0x{n:x}")
}

fn parse_quantity(v: &Value, what: &str) -> Result<u64, String> {
    as_u64(&hex32(v, what)?)
}

/// The vault's `Deposit`, `Release`, `Refund` and `CheckpointFinalized` events (`vault/src/FerrataVault.sol`).
pub(crate) const EV_DEPOSIT: &str = "Deposit(address,uint64,uint64,address,uint256,bytes32)";
pub(crate) const EV_RELEASE: &str = "Release(bytes32,address,address,uint256,address,uint256,bytes32)";
pub(crate) const EV_REFUND: &str = "Refund(uint64,address,address,uint256)";
pub(crate) const EV_TOKEN_REGISTERED: &str = "TokenRegistered(address,uint256[],uint256[],uint256,uint256)";
pub(crate) const EV_FINALIZED: &str =
    "CheckpointFinalized(uint256,bytes32,bytes32,bytes32,uint64,uint64,bool,uint64,bytes32,uint32,uint32)";

/// A deposit as the vault's `Deposit` event tells it, with its block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DepositEvent {
    pub depositor: Address,
    pub j: u64,
    pub leaf_index: u64,
    pub token: Address,
    pub amount: U256,
    pub commitment: Hash,
    pub block: u64,
}

/// Decodes one log of the vault as a `Deposit` (`None` when it is another event).
pub(crate) fn deposit_of_log(log: &Value) -> Result<Option<DepositEvent>, String> {
    let topics: Vec<[u8; 32]> =
        log["topics"].as_array().into_iter().flatten().map(|t| hex32(t, "topic")).collect::<Result<_, _>>()?;
    if topics.first() != Some(&keccak(EV_DEPOSIT.as_bytes())) {
        return Ok(None);
    }
    if topics.len() != 3 {
        return Err("a Deposit log without its two indexed topics".into());
    }
    let data = unhex(log["data"].as_str().unwrap_or("0x")).map_err(|e| format!("a Deposit log's data: {e}"))?;
    Ok(Some(DepositEvent {
        depositor: as_address(&topics[1]),
        token: as_address(&topics[2]),
        j: as_u64(&word(&data, 0)?)?,
        leaf_index: as_u64(&word(&data, 1)?)?,
        amount: word(&data, 2)?,
        commitment: word(&data, 3)?,
        block: log["blockNumber"].as_str().map_or(Ok(0), |_| parse_quantity(&log["blockNumber"], "blockNumber"))?,
    }))
}

/// A burn root the vault accepts (`eth.rs`'s `AcceptedRoot`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AcceptedRoot {
    pub root: Hash,
    pub count: u64,
    pub index: u64,
    pub finalized_at: u64,
}

/// The vault as one Ethereum path sees it.
pub(crate) struct Vault {
    pub rpc: Rpc,
    pub vault: Address,
    pub from_block: u64,
    pub chunk: u64,
}

impl Vault {
    /// `eth`: `{"via": "eth_rpc_module", "chain_id": N}` (Logos Core) or `{"via": "http", "rpc_url": URL}`.
    pub fn new(eth: &Value, vault: Address, from_block: u64) -> Result<Self, String> {
        let rpc = Rpc::from_request(&json!({ "eth": eth }), 30_000)?;
        Ok(Self { rpc, vault, from_block, chunk: 10_000 })
    }

    /// `eth_call` of `to` at `block` (`None`: latest); the returned data.
    pub fn call_at(&mut self, to: &Address, data: &[u8], block: Option<u64>) -> Result<Vec<u8>, String> {
        let tag = block.map_or_else(|| json!("latest"), |b| json!(quantity(b)));
        let r = self.rpc.call("eth_call", json!([{ "to": hex(to), "data": hex(data) }, tag]))?;
        unhex(r.as_str().ok_or("eth_call: no data")?).map_err(|e| format!("eth_call: {e}"))
    }

    /// A view of the vault.
    pub fn view(&mut self, signature: &str, args: &[Abi], block: Option<u64>) -> Result<Vec<u8>, String> {
        let vault = self.vault;
        self.call_at(&vault, &calldata(signature, args), block).map_err(|e| format!("{signature}: {e}"))
    }

    fn view_word(&mut self, signature: &str, args: &[Abi], i: usize) -> Result<[u8; 32], String> {
        let data = self.view(signature, args, None)?;
        word(&data, i)
    }

    pub fn block_number(&mut self) -> Result<u64, String> {
        parse_quantity(&self.rpc.call("eth_blockNumber", json!([]))?, "eth_blockNumber")
    }

    /// The latest block's time (Ethereum time, seconds).
    pub fn now(&mut self) -> Result<u64, String> {
        let b = self.rpc.call("eth_getBlockByNumber", json!(["latest", false]))?;
        parse_quantity(&b["timestamp"], "the latest block's timestamp")
    }

    pub fn balance(&mut self, who: &Address) -> Result<U256, String> {
        hex32(&self.rpc.call("eth_getBalance", json!([hex(who), "latest"]))?, "eth_getBalance")
    }

    pub fn chain_id(&mut self) -> Result<u64, String> {
        parse_quantity(&self.rpc.call("eth_chainId", json!([]))?, "eth_chainId")
    }

    /// `tokenConfig(token)`: registered, deposits enabled, LEZ unit, unit weight, denominations.
    pub fn token_config(&mut self, token: &Address) -> Result<(bool, bool, U256, U256, Vec<U256>), String> {
        let d = self.view("tokenConfig(address)", &[Abi::addr(token)], None)?;
        Ok((word(&d, 0)?[31] == 1, word(&d, 1)?[31] == 1, word(&d, 2)?, word(&d, 3)?, word_array(&d, 4)?))
    }

    /// `feeState(token)`: the schedule in force and when the announced one takes effect (0: none).
    pub fn fee_state(&mut self, token: &Address) -> Result<(u64, u64), String> {
        let d = self.view("feeState(address)", &[Abi::addr(token)], None)?;
        Ok((as_u64(&word(&d, 0)?)?, as_u64(&word(&d, 1)?)?))
    }

    /// `fees(token, index, denomIndex)`: mint, relay, burn and checkpoint fee.
    pub fn fees(&mut self, token: &Address, index: u64, d: u8) -> Result<[U256; 4], String> {
        let data = self.view(
            "fees(address,uint256,uint8)",
            &[Abi::addr(token), Abi::uint(u128::from(index)), Abi::uint(u128::from(d))],
            None,
        )?;
        Ok([word(&data, 0)?, word(&data, 1)?, word(&data, 2)?, word(&data, 3)?])
    }

    /// `tokenCap(token, direction)` or, without a token, `globalCap(direction)`: capacity, refill, available.
    pub fn cap(&mut self, token: Option<&Address>, direction: u8) -> Result<Value, String> {
        let d = match token {
            Some(t) => self.view("tokenCap(address,uint8)", &[Abi::addr(t), Abi::uint(u128::from(direction))], None)?,
            None => self.view("globalCap(uint8)", &[Abi::uint(u128::from(direction))], None)?,
        };
        Ok(json!({ "capacity": dec(&word(&d, 0)?), "refill_per_second": dec(&word(&d, 1)?), "available": dec(&word(&d, 2)?) }))
    }

    pub fn deposit_count(&mut self, depositor: &Address) -> Result<u64, String> {
        as_u64(&self.view_word("depositCount(address)", &[Abi::addr(depositor)], 0)?)
    }

    pub fn next_leaf_index(&mut self, block: Option<u64>) -> Result<u64, String> {
        as_u64(&word(&self.view("nextLeafIndex()", &[], block)?, 0)?)
    }

    pub fn release_delay_for(&mut self, token: &Address, checkpoint_index: u64) -> Result<u64, String> {
        let w = self.view_word(
            "releaseDelayFor(address,uint256)",
            &[Abi::addr(token), Abi::uint(u128::from(checkpoint_index))],
            0,
        )?;
        Ok(as_u64(&w).unwrap_or(u64::MAX))
    }

    pub fn nullifier_used(&mut self, nullifier: &Hash) -> Result<bool, String> {
        Ok(self.view_word("nullifierUsed(bytes32)", &[Abi::W(*nullifier)], 0)?[31] == 1)
    }

    /// A word-sized public getter without arguments (`admin()`, `retired()`, `depositsFrozenUntil()` ...).
    pub fn getter(&mut self, name: &str) -> Result<[u8; 32], String> {
        self.view_word(&format!("{name}()"), &[], 0)
    }

    /// What a deposit of denomination `d` of `token` costs now (`eth.rs`'s `deposit_price`): the denomination, the
    /// mint and relay fees of the schedule in force (an announced one in force once its time has come).
    pub fn price(&mut self, token: &Address, d: u8) -> Result<Value, String> {
        let (registered, _, _, _, denominations) = self.token_config(token)?;
        if !registered {
            return Err("refused: vault error TokenNotRegistered() (checked before sending)".into());
        }
        let denomination = *denominations
            .get(usize::from(d))
            .ok_or("refused: vault error InvalidDenominationIndex() (checked before sending)")?;
        let (current, pending_at) = self.fee_state(token)?;
        let now = self.now()?;
        let schedule = if pending_at != 0 && now >= pending_at { current + 1 } else { current };
        let f = self.fees(token, schedule, d)?;
        let (den, mint, relay) = (as_u128(&denomination)?, as_u128(&f[0])?, as_u128(&f[1])?);
        Ok(json!({ "denomination": den.to_string(), "mint_fee": mint.to_string(), "relay_fee": relay.to_string(),
                   "total": (den + mint + relay).to_string(), "schedule": schedule }))
    }

    /// The vault's side of the registry for `token` (as `seqnet-chain`'s `registry`).
    pub fn registry(&mut self, token: &Address) -> Result<Value, String> {
        let (registered, deposits_enabled, lez_unit, unit_weight, denominations) = self.token_config(token)?;
        let (current, pending_at) = self.fee_state(token)?;
        let mut list = Vec::new();
        for (d, amount) in denominations.iter().enumerate() {
            let f = self.fees(token, current, d as u8)?;
            list.push(json!({ "index": d, "amount": dec(amount), "mint_fee": dec(&f[0]), "relay_fee": dec(&f[1]),
                              "burn_fee": dec(&f[2]), "checkpoint_fee": dec(&f[3]) }));
        }
        let escrowed = self.view_word("escrowed(address)", &[Abi::addr(token)], 0)?;
        let token_deposits_frozen = self.view_word("tokenDepositsFrozenUntil(address)", &[Abi::addr(token)], 0)?;
        let token_releases_frozen = self.view_word("tokenReleasesFrozenUntil(address)", &[Abi::addr(token)], 0)?;
        Ok(json!({
            "token": {
                "token": hex(token),
                "registered": registered,
                "deposits_enabled": deposits_enabled,
                "lez_unit": dec(&lez_unit),
                "unit_weight": dec(&unit_weight),
                "fee_schedule": current,
                "fee_schedule_pending_at": pending_at,
                "denominations": list,
                "escrowed": dec(&escrowed),
                "vault_deposit_cap": self.cap(Some(token), 0)?,
                "vault_release_cap": self.cap(Some(token), 1)?,
                "deposits_frozen_until": dec(&token_deposits_frozen),
                "releases_frozen_until": dec(&token_releases_frozen),
                // The delay a release under the next checkpoints waits (a change by setReleaseDelay applies to those).
                "release_delay_next": if registered { json!(self.release_delay_for(token, u64::MAX)?) } else { Value::Null },
            },
            "vault": {
                "deposits_frozen_until": dec(&self.getter("depositsFrozenUntil")?),
                "releases_frozen_until": dec(&self.getter("releasesFrozenUntil")?),
                "checkpoints_frozen_until": dec(&self.getter("checkpointsFrozenUntil")?),
                "retired": self.getter("retired")?[31] == 1,
                "deposits": self.next_leaf_index(None)?,
                "checkpoints": dec(&self.getter("checkpointCount")?),
                "global_deposit_cap": self.cap(None, 0)?,
                "global_release_cap": self.cap(None, 1)?,
            },
        }))
    }

    /// Every token the vault registered, ETH first: the `TokenRegistered` logs from `from_block`, and ETH when its
    /// entry is registered (whatever block registered it).
    pub fn registered_tokens(&mut self) -> Result<Vec<Address>, String> {
        let to = self.block_number()?;
        let mut out: Vec<Address> = Vec::new();
        if self.token_config(&[0u8; 20])?.0 {
            out.push([0u8; 20]);
        }
        for log in self.logs(EV_TOKEN_REGISTERED, to)? {
            let t = as_address(&hex32(&log["topics"][1], "TokenRegistered's token")?);
            if !out.contains(&t) {
                out.push(t);
            }
        }
        Ok(out)
    }

    /// An ERC-20's `symbol()` and `decimals()` (`eth_call`), or a short address and 18 when it does not answer them.
    pub fn erc20_meta(&mut self, token: &Address) -> (String, u8) {
        let short = format!("{}…", &hex(token)[..8]);
        let decimals = self.call_at(token, &calldata("decimals()", &[]), None).ok()
            .and_then(|d| word(&d, 0).ok()).and_then(|w| as_u64(&w).ok()).and_then(|d| u8::try_from(d).ok()).unwrap_or(18);
        let symbol = self.call_at(token, &calldata("symbol()", &[]), None).ok().and_then(|d| {
            // An ABI string: offset, length, bytes.
            let len = as_u64(&word(&d, 1).ok()?).ok()? as usize;
            let bytes = d.get(64..64 + len)?;
            std::str::from_utf8(bytes).ok().filter(|t| !t.is_empty() && t.len() <= 32).map(str::to_string)
        });
        (symbol.unwrap_or(short), decimals)
    }

    /// The vault's roles: admin, pending admin, freeze authority, the three watchers, the fee recipient.
    pub fn roles(&mut self) -> Result<Value, String> {
        let a = |w: [u8; 32]| hex(&as_address(&w));
        let mut watchers = Vec::new();
        for i in 0..3u128 {
            watchers.push(a(self.view_word("watchers(uint256)", &[Abi::uint(i)], 0)?));
        }
        Ok(json!({
            "admin": a(self.getter("admin")?),
            "pending_admin": a(self.getter("pendingAdmin")?),
            "freeze_authority": a(self.getter("freezeAuthority")?),
            "fee_recipient": a(self.getter("feeRecipient")?),
            "watchers": watchers,
        }))
    }

    /// The vault's logs of `event` from `from_block` to `to`, in ranges of `chunk` blocks.
    pub fn logs(&mut self, event: &str, to: u64) -> Result<Vec<Value>, String> {
        let topic = hex(&keccak(event.as_bytes()));
        let mut out = Vec::new();
        let mut start = self.from_block;
        while start <= to {
            let end = (start + self.chunk.max(1) - 1).min(to);
            let r = self.rpc.call(
                "eth_getLogs",
                json!([{ "address": hex(&self.vault), "topics": [topic], "fromBlock": quantity(start), "toBlock": quantity(end) }]),
            )?;
            out.extend(r.as_array().cloned().unwrap_or_default());
            start = end + 1;
        }
        Ok(out)
    }

    /// Every deposit, checked against the vault at one block (`eth.rs`'s `deposit_list`): `(block, deposits, root)`.
    pub fn deposit_list(&mut self) -> Result<(u64, Vec<DepositEvent>, Hash), String> {
        let block = self.block_number()?;
        let mut deposits = Vec::new();
        for log in self.logs(EV_DEPOSIT, block)? {
            if let Some(d) = deposit_of_log(&log)? {
                deposits.push(d);
            }
        }
        deposits.sort_by_key(|d| d.leaf_index);
        let count = self.next_leaf_index(Some(block))?;
        let root = word(&self.view("depositRoot()", &[], Some(block))?, 0)?;
        if deposits.len() as u64 != count {
            return Err(format!(
                "the provider returned {} Deposit events, the vault holds {count} leaves at block {block}: the list is incomplete",
                deposits.len()
            ));
        }
        let mut tree = IncrementalTree::new();
        for (i, d) in deposits.iter().enumerate() {
            if d.leaf_index != i as u64 {
                return Err(format!("leaf {i} is missing from the provider's list"));
            }
            tree.append(inbound::deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment))
                .map_err(|_| "more than 2^32 leaves")?;
        }
        if tree.root() != root {
            return Err(format!(
                "the deposits the provider returned do not hash to the vault's depositRoot at block {block}: a changed or forged list"
            ));
        }
        Ok((block, deposits, root))
    }

    /// Every nullifier the vault has released (`Release` events, topic 1).
    pub fn released_nullifiers(&mut self) -> Result<Vec<Hash>, String> {
        let block = self.block_number()?;
        let mut out = Vec::new();
        for log in self.logs(EV_RELEASE, block)? {
            let t = log["topics"].as_array().and_then(|t| t.get(1)).ok_or("a Release log without its nullifier")?;
            out.push(hex32(t, "a Release log's nullifier")?);
        }
        Ok(out)
    }

    /// Every leaf index the vault has refunded (`Refund` events, topic 1).
    pub fn refunded_leaves(&mut self) -> Result<Vec<u64>, String> {
        let block = self.block_number()?;
        let mut out = Vec::new();
        for log in self.logs(EV_REFUND, block)? {
            let t = log["topics"].as_array().and_then(|t| t.get(1)).ok_or("a Refund log without its leaf index")?;
            out.push(as_u64(&hex32(t, "a Refund log's leaf index")?)?);
        }
        Ok(out)
    }

    /// Every accepted burn root, oldest first (`CheckpointFinalized` events, each confirmed by `burnRootStatus`).
    pub fn accepted_roots(&mut self) -> Result<Vec<AcceptedRoot>, String> {
        let block = self.block_number()?;
        let mut out = Vec::new();
        for log in self.logs(EV_FINALIZED, block)? {
            let topics = log["topics"].as_array().ok_or("a CheckpointFinalized log without topics")?;
            let index = as_u64(&hex32(topics.get(1).ok_or("no index topic")?, "index")?)?;
            let data = unhex(log["data"].as_str().unwrap_or("0x")).map_err(|e| format!("CheckpointFinalized data: {e}"))?;
            // data: parentState, burnRoot, burnCount, importedLeafCount, sunset, tipId, tipHash, n, t.
            let root = word(&data, 1)?;
            let count = as_u64(&word(&data, 2)?)?;
            let status = self.view("burnRootStatus(bytes32)", &[Abi::W(root)], None)?;
            if word(&status, 0)?[31] == 1 {
                out.push(AcceptedRoot { root, count, index, finalized_at: as_u64(&word(&status, 1)?).unwrap_or(u64::MAX) });
            }
        }
        Ok(out)
    }

    /// The vault's accepted roots with each one's release delay for `token` (what `release_by_rule` takes), and now.
    pub fn roots_for_release(&mut self, token: &Address) -> Result<Value, String> {
        let mut roots = Vec::new();
        for r in self.accepted_roots()? {
            let delay = self.release_delay_for(token, r.index)?;
            roots.push(json!({ "root": hex(&r.root), "count": r.count, "index": r.index, "finalized_at": r.finalized_at, "delay": delay }));
        }
        Ok(json!({ "roots": roots, "now": self.now()? }))
    }

    /// A transaction's receipt, waiting for it up to `polls` reads (a local chain mines at once).
    pub fn receipt(&mut self, hash: &str, polls: u32) -> Result<Value, String> {
        for i in 0..polls.max(1) {
            let r = self.rpc.call("eth_getTransactionReceipt", json!([hash]))?;
            if !r.is_null() {
                return Ok(r);
            }
            if i + 1 < polls {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
        Err(format!("no receipt for {hash} after {polls} reads"))
    }

    /// Which path answered, and how many calls it made.
    pub fn report(&self) -> Value {
        self.rpc.report()
    }
}

/// A receipt's status (`0x1`), block number and the `Deposit` event `vault` emitted in it, if any (a log of the same
/// shape from any other address is not the vault's).
pub(crate) fn receipt_summary(r: &Value, vault: &Address) -> Result<(bool, u64, Option<DepositEvent>), String> {
    let status = r["status"].as_str() == Some("0x1");
    let block = parse_quantity(&r["blockNumber"], "the receipt's blockNumber")?;
    let mut deposit = None;
    for log in r["logs"].as_array().into_iter().flatten() {
        let from_vault = log["address"].as_str().and_then(|a| unhex(a).ok()).is_some_and(|a| a.as_slice() == vault.as_slice());
        if !from_vault {
            continue;
        }
        if let Some(d) = deposit_of_log(log)? {
            deposit = Some(d);
        }
    }
    Ok((status, block, deposit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors_are_the_known_ones() {
        // ERC-20 and the vault's deposit, as every tool computes them.
        assert_eq!(hex(&selector("approve(address,uint256)")), "0x095ea7b3");
        assert_eq!(hex(&selector("transfer(address,uint256)")), "0xa9059cbb");
        assert_eq!(hex(&selector("balanceOf(address)")), "0x70a08231");
        // keccak("") (the empty string's hash, a classic vector).
        assert_eq!(hex(&keccak(b"")), "0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470");
    }

    #[test]
    fn a_dynamic_tuple_is_encoded_with_offsets() {
        // f(uint256, uint256[]) with (1, [2, 3]): head 1, offset 0x40; tail: 2, 2, 3 (Solidity's ABI spec example
        // shape).
        let data = encode_seq(&[Abi::uint(1), Abi::Arr(vec![Abi::uint(2), Abi::uint(3)])]);
        let words: Vec<u64> = (0..data.len() / 32).map(|i| as_u64(&word(&data, i).unwrap()).unwrap()).collect();
        assert_eq!(words, vec![1, 0x40, 2, 2, 3]);
        // A static tuple is inline; a dynamic one behind an offset.
        let st = encode_seq(&[Abi::Tup(vec![Abi::uint(7), Abi::uint(8)]), Abi::uint(9)]);
        assert_eq!(st.len(), 96);
        let dy = encode_seq(&[Abi::uint(5), Abi::Tup(vec![Abi::Arr(vec![Abi::uint(1)]), Abi::uint(4)])]);
        let w: Vec<u64> = (0..dy.len() / 32).map(|i| as_u64(&word(&dy, i).unwrap()).unwrap()).collect();
        // 5, offset 0x40 -> tuple: offset 0x40 (relative to the tuple), 4, then the array: 1, 1.
        assert_eq!(w, vec![5, 0x40, 0x40, 4, 1, 1]);
        // bytes: length, then padded to a whole word.
        let b = encode_seq(&[Abi::Bytes(vec![0x5e, 0xa1])]);
        assert_eq!(b.len(), 96);
        assert_eq!(as_u64(&word(&b, 0).unwrap()).unwrap(), 0x20);
        assert_eq!(as_u64(&word(&b, 1).unwrap()).unwrap(), 2);
        assert_eq!(&b[64..66], &[0x5e, 0xa1]);
    }

    #[test]
    fn eip55_checksums() {
        // EIP-55's own examples and anvil's first account.
        let a = |h: &str| -> Address { crate::unhex(h).unwrap().try_into().unwrap() };
        assert_eq!(checksum(&a("5aaeb6053f3e94c9b9a09f33669435e7ef1beaed")), "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed");
        assert_eq!(checksum(&a("fb6916095ca1df60bb79ce92ce3ea74c37c5d359")), "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359");
        assert_eq!(checksum(&a("f39fd6e51aad88f6f4ce6ab8827279cfffb92266")), "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
    }

    #[test]
    fn quantities_without_leading_zeros_parse() {
        assert_eq!(parse_quantity(&json!("0x9"), "q").unwrap(), 9);
        assert_eq!(parse_quantity(&json!("0x1b4"), "q").unwrap(), 436);
        assert_eq!(parse_quantity(&json!("0x0"), "q").unwrap(), 0);
        assert!(parse_quantity(&json!("0xzz"), "q").is_err());
    }

    #[test]
    fn decimal_and_arrays_decode() {
        let mut w = [0u8; 32];
        w[16..].copy_from_slice(&10_000_000_000_000_000u128.to_be_bytes());
        assert_eq!(dec(&w), "10000000000000000");
        assert_eq!(dec(&[0u8; 32]), "0");
        assert_eq!(dec(&[0xffu8; 32]), "115792089237316195423570985008687907853269984665640564039457584007913129639935");
        let ret = encode_seq(&[Abi::boolean(true), Abi::Arr(vec![Abi::uint(10), Abi::uint(20)])]);
        let a = word_array(&ret, 1).unwrap();
        assert_eq!(a.iter().map(|w| as_u64(w).unwrap()).collect::<Vec<_>>(), vec![10, 20]);
    }

    #[test]
    fn a_deposit_log_decodes() {
        let depositor = [0x11u8; 20];
        let token = [0u8; 20];
        let data = encode_seq(&[Abi::uint(3), Abi::uint(7), Abi::uint(1_000), Abi::W([0xccu8; 32])]);
        let log = json!({
            "topics": [hex(&keccak(EV_DEPOSIT.as_bytes())), hex(&Abi::addr(&depositor).encode()), hex(&Abi::addr(&token).encode())],
            "data": hex(&data),
            "blockNumber": "0x2a",
        });
        let d = deposit_of_log(&log).unwrap().unwrap();
        assert_eq!((d.depositor, d.token, d.j, d.leaf_index, d.block), (depositor, token, 3, 7, 42));
        assert_eq!(as_u128(&d.amount).unwrap(), 1_000);
        assert_eq!(d.commitment, [0xcc; 32]);
        // Another event of the vault is not a deposit.
        let other = json!({ "topics": [hex(&keccak(EV_REFUND.as_bytes()))], "data": "0x" });
        assert!(deposit_of_log(&other).unwrap().is_none());
    }
}
