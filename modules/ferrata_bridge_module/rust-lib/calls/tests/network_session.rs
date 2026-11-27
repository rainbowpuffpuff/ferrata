//! The mini-app's whole network session from its committed terminal calls file. Every server is bound to
//! 127.0.0.1: the network configuration, faucet and LEZ JSON-RPC on one loopback origin, and Ethereum JSON-RPC on
//! another. Nothing on a public network is contacted. As in tests/endpoints.rs, these are protocol stand-ins, not
//! chains: they serve real Borsh states, a Deposit log and its recomputed root. The deployment pins B1 (the test
//! network's program): the plaintext claim is built in B1's layout (its fee schedule range), approved, and its confirm
//! must decode as that exact B1 claim, and the burn's as this tree's exact `Burn`, before resulting state is exposed.
//!
//! Run all targets: `cargo test --locked --offline` from calls/. The replay uses the same `wallet_calls` function as
//! `cargo run --example call -- wallet examples/network-session.jsonl` (with loopback URLs substituted here).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use base64::Engine as _;
use borsh::BorshDeserialize as _;
use ferrata_bridge_calls::{wallet_as, wallet_calls, ViewClient};
use ferrata_bridge_core::{chain_id_u256, hmac_sha256, inbound, outbound, sha256, tree::Tree, BridgeKeys, Hash, U256};
use ferrata_lez_bridge_wire::{self as bw, BridgeState};
use ferrata_wallet::deployed::{self, DecodedClaim, ScheduleRange};
use ferrata_wallet::AccountId;
use serde_json::{json, Value};
use tiny_keccak::{Hasher, Keccak};

const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const ACCOUNT0: &str = "AKRYNTU8c4MZ4fSr5FHcn6BGE6T4dfRCzn1ziQCNKFBu";
const PUBLIC_KEY0: &str = "c8730d3953a94b608d67710f3c7cbb2f0694665ae60af5c7584253f49346b231";
const PROGRAM_B58: &str = "36iqVMFpfgHrwoeuVvZhXX3caM1JPBJh56PBw5A4H1rk";
const IMAGE: &str = "847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515";
const CID_HEX: &str = "3773b58836d2fa242e16b3eb580a15630de2a437218a432f52e5b01152ae67cd";
const VAULT_HEX: &str = "19499061cec0bac2a3e1869ba5ae6b403aa82787";
const DEPOSITOR_HEX: &str = "9858effd232b4033e47d90003d41ec34ecaeda94";
const RECIPIENT: &str = "0x9858EfFD232B4033E47d90003D41EC34EcaEda94";
const CHAIN: u64 = 11_155_111;
const CLAIM_AMOUNT: u128 = 1_000_000_000_000_000;
const BURN_AMOUNT: u128 = 100_000_000_000_000;
const INITIAL_LGO: u128 = 1_000;
const FAUCET_LGO: u128 = 10_000_000_000;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}
fn hex(b: &[u8]) -> String {
    format!("0x{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>())
}
fn hash_hex(b: &[u8]) -> String {
    hex(b)[2..].to_string()
}
fn fixed<const N: usize>(s: &str) -> [u8; N] {
    unhex(s).try_into().expect("fixture's hex length")
}
fn amount(n: u128) -> U256 {
    let mut w = [0u8; 32];
    w[16..].copy_from_slice(&n.to_be_bytes());
    w
}
fn word64(n: u64) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&n.to_be_bytes());
    w
}
fn word_addr(a: &[u8; 20]) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[12..].copy_from_slice(a);
    w
}
fn keccak(s: &str) -> [u8; 32] {
    let mut k = Keccak::v256();
    k.update(s.as_bytes());
    let mut out = [0u8; 32];
    k.finalize(&mut out);
    out
}
fn id58(id: &[u8; 32]) -> String {
    bs58::encode(id).into_string()
}
fn encode_state(s: &BridgeState) -> Vec<u8> {
    borsh::to_vec(s).unwrap()
}

fn read_request(s: &mut TcpStream) -> Option<(String, Vec<u8>)> {
    s.set_read_timeout(Some(std::time::Duration::from_secs(10))).ok()?;
    let (mut buf, mut chunk) = (Vec::new(), [0u8; 8192]);
    loop {
        let n = s.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..end]);
            let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
            let len = head
                .to_ascii_lowercase()
                .lines()
                .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                .unwrap_or(0);
            while buf.len() < end + 4 + len {
                let n = s.read(&mut chunk).ok()?;
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            return Some((path, buf[end + 4..(end + 4 + len).min(buf.len())].to_vec()));
        }
    }
}

/// A loopback HTTP server. The handler returns the HTTP status and a JSON response body.
fn serve(f: impl Fn(&str, Value) -> (u16, Value) + Send + 'static) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let Some((path, body)) = read_request(&mut s) else { continue };
            let req = serde_json::from_slice(&body).unwrap_or(Value::Null);
            let (status, out) = f(&path, req);
            let out = out.to_string();
            let _ = write!(
                s,
                "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}",
                out.len()
            );
        }
    });
    let url = format!("http://{addr}");
    assert!(url.starts_with("http://127.0.0.1:"), "loopback only");
    url
}

struct Fixture {
    cid: Hash,
    holder: String,
    nullifier: String,
    burn: String,
    config: String,
    root: String,
    token: String,
    config_state: Vec<u8>,
    root_state: Vec<u8>,
    token_state: Vec<u8>,
    burn_state: Vec<u8>,
    burn_message: bw::BridgeMessage,
    /// The plaintext claim of deposit 0 the network expects, in B1's layout.
    claim: DecodedClaim,
    nullifier_state: Vec<u8>,
    holder_before_burn: Vec<u8>,
    holder_after_burn: Vec<u8>,
    deposit_log: Value,
    deposit_root: Hash,
}

impl Fixture {
    fn new() -> Self {
        let program = AccountId::new(bs58::decode(PROGRAM_B58).into_vec().unwrap().try_into().unwrap());
        let cid = fixed(CID_HEX);
        let vault = fixed(VAULT_HEX);
        let depositor = fixed(DEPOSITOR_HEX);
        let account0 = bs58::decode(ACCOUNT0).into_vec().unwrap().try_into().unwrap();
        let mnemonic = bip39::Mnemonic::parse(PHRASE).unwrap();
        let seed = mnemonic.to_seed("");
        let keys = BridgeKeys::from_bridge_seed(&hmac_sha256(&seed, b"ferrata/bridge-seed/v1"));
        let dest = account0;
        let secret = inbound::deposit_secret(&keys.auth, &chain_id_u256(CHAIN), &vault, &depositor, 0);
        let commitment = inbound::commitment(&secret, inbound::DestKind::Public, &dest);
        let leaf = inbound::deposit_leaf(0, &depositor, &[0u8; 20], &amount(CLAIM_AMOUNT), &commitment);
        let deposit_root = Tree::from_leaves(vec![leaf]).unwrap().root();
        let nullifier = inbound::nullifier(&secret, &inbound::deposit_id(&chain_id_u256(CHAIN), &vault, 0, &leaf));
        let image: Hash = fixed(IMAGE);
        let unlimited = bw::CapParams { capacity: u128::MAX, refill_per_second: u128::MAX };
        let controls = bw::Controls {
            admin: [1; 32],
            freeze_authority: [2; 32],
            fee_recipient: [3; 32],
            floor: bw::CapParams { capacity: 1, refill_per_second: 1 },
            token_cap: unlimited,
            global_cap: unlimited,
        };
        let config = bw::BridgeConfig {
            bedrock_genesis_id: [4; 32],
            channel_id: [5; 32],
            bridge_image_id: image,
            attestation: AccountId::new([6; 32]),
            instance_id: [7; 32],
            evm_chain_id: CHAIN,
            vault,
            controls,
        };
        let gate = bw::Gate::new(unlimited, 0);
        let config_state = encode_state(&BridgeState::Config(bw::ConfigState::Ready {
            config: config.clone(),
            pending: None,
            gate,
            sunset: false,
            roles: bw::Roles::new(&controls),
        }));
        let root_state = encode_state(&BridgeState::Root(bw::RootState {
            config_id: cid,
            evm_chain_id: CHAIN,
            vault,
            leaf_count: 1,
            root: deposit_root,
            record_slot: 1,
            block_number: 42,
        }));
        let token_state = encode_state(&BridgeState::Token(bw::TokenState {
            config_id: cid,
            evm_chain_id: CHAIN,
            vault,
            attestation: config.attestation,
            instance_id: config.instance_id,
            entry: bw::TokenEntry {
                token: [0u8; 20],
                lez_definition: [0; 32],
                decimals: 18,
                lez_unit: 1,
                unit_weight: 1,
                denominations: vec![CLAIM_AMOUNT],
                relay_fees: vec![0],
            },
            schedules: vec![bw::FeeSchedule {
                index: 0,
                from_leaf: 0,
                announced_at: 0,
                effective_at: 0,
                mint_fees: vec![0],
                burn_fees: vec![0],
                checkpoint_fees: vec![0],
            }],
            known_until: 1,
            controls,
            gate,
            pending: None,
            delisted: false,
            listing_slot: 0,
        }));
        let target = outbound::ReleaseTarget { chain_id: chain_id_u256(CHAIN), vault, recipient: fixed(DEPOSITOR_HEX) };
        let note = outbound::BurnNote::create(&keys, &cid, 0, &target, [0u8; 20], amount(BURN_AMOUNT));
        let burn_state = encode_state(&BridgeState::Burn(bw::BurnRecord {
            config_id: cid,
            note: bw::Note {
                r: note.r,
                memo: note.memo,
                mac: note.mac,
                token: note.token,
                amount: note.amount,
                c_out: note.c_out,
            },
        }));
        let burn_message = bw::BridgeMessage::Burn {
            config_id: cid,
            r: note.r,
            memo: note.memo,
            mac: note.mac,
            token: note.token,
            amount: note.amount,
            c_out: note.c_out,
        };
        let holder_state = |held| {
            encode_state(&BridgeState::Holder(bw::HolderState {
                holdings: vec![bw::Holding { config_id: cid, token: [0u8; 20], amount: held }],
                pending: None,
            }))
        };
        let mut path = Tree::from_leaves(vec![leaf]).unwrap().path(0).unwrap().siblings.to_vec();
        path.truncate(32);
        let claim = DecodedClaim {
            private: false,
            config_id: cid,
            leaf_count: 1,
            witness: bw::ClaimWitness {
                depositor,
                token: [0u8; 20],
                amount: amount(CLAIM_AMOUNT),
                leaf_index: 0,
                path,
                s: secret,
                kind: inbound::DestKind::Public as u8,
                dest,
            },
            range: Some(ScheduleRange { index: 0, from_leaf: 0, until_leaf: 1 }),
            relayer: None,
        };
        let nullifier_state = encode_state(&BridgeState::Nullifier(bw::NullifierState {
            config_id: cid,
            token: [0u8; 20],
            leaf_count: 1,
            amount: CLAIM_AMOUNT,
        }));
        let deposit_log = json!({
            "topics": [hex(&keccak("Deposit(address,uint64,uint64,address,uint256,bytes32)")), hex(&word_addr(&depositor)), hex(&word_addr(&[0u8; 20]))],
            "data": hex(&[word64(0), word64(0), amount(CLAIM_AMOUNT), commitment].concat()),
            "blockNumber": "0x2a",
        });
        Self {
            holder: id58(bw::holder_actor(program, &cid, AccountId::new(account0)).account_id.value()),
            nullifier: id58(bw::nullifier_actor(program, &cid, CHAIN, &vault, &nullifier).account_id.value()),
            burn: id58(bw::burn_actor(program, &cid, &note.r).account_id.value()),
            config: id58(bw::config_actor(program, &cid).account_id.value()),
            root: id58(bw::root_actor(program, &cid, 1).account_id.value()),
            token: id58(bw::token_actor(program, &cid, &[0u8; 20]).account_id.value()),
            claim,
            nullifier_state,
            holder_before_burn: holder_state(CLAIM_AMOUNT),
            holder_after_burn: holder_state(CLAIM_AMOUNT - BURN_AMOUNT),
            deposit_log,
            deposit_root,
            cid,
            burn_state,
            burn_message,
            config_state,
            root_state,
            token_state,
        }
    }
}

#[derive(Clone, Debug)]
struct NetworkRequest {
    method: String,
    /// The send ledger's length before this request was served; a sendTransaction records the preceding count.
    sends_before: usize,
}

#[derive(Debug, Default)]
struct NetworkState {
    sent: Vec<String>,
    decoded: Vec<bw::BridgeMessage>,
    /// Plaintext claims the network decoded in B1's layout.
    claims: Vec<DecodedClaim>,
    claim_applied: bool,
    burn_applied: bool,
    requests: Vec<NetworkRequest>,
    faucet_body: Option<Value>,
    faucet_called: bool,
}

/// Decode a captured public transaction's root call with the current bridge wire. Unlike the old send-count
/// simulation, malformed or wrong-layout data cannot make the stand-in report its actor states as applied.
fn captured_bridge_message(wire: &str) -> Result<bw::BridgeMessage, String> {
    bw::BridgeMessage::try_from_slice(&captured_bridge_data(wire)?).map_err(|e| format!("current BridgeMessage decoder: {e}"))
}

/// A captured transaction's root call's data.
fn captured_bridge_data(wire: &str) -> Result<Vec<u8>, String> {
    let raw = base64::engine::general_purpose::STANDARD.decode(wire).map_err(|e| e.to_string())?;
    let tx = raw.get(1..).ok_or("short public transaction")?;
    let mut at = 0usize;
    let mut take = |n: usize| -> Result<&[u8], String> {
        let out = tx.get(at..at + n).ok_or("truncated transaction message")?;
        at += n;
        Ok(out)
    };
    let count = |b: &[u8]| -> Result<usize, String> {
        Ok(u32::from_le_bytes(b.try_into().map_err(|_| "u32 length")?) as usize)
    };
    let actors = count(take(4)?)?;
    take(actors.checked_mul(64).ok_or("actor count overflow")?)?;
    let signers = count(take(4)?)?;
    take(signers.checked_mul(32).ok_or("signer count overflow")?)?;
    if count(take(4)?)? != 0 {
        return Err("promoted actors in a public bridge call".into());
    }
    take(64)?;
    let data_len = count(take(4)?)?;
    Ok(take(data_len)?.to_vec())
}

fn network(f: Arc<Fixture>, state: Arc<Mutex<NetworkState>>) -> String {
    serve(move |path, req| {
        if path == "/wallet/config.json" {
            return (
                200,
                json!({ "rpc": "/rpc", "faucet": "/faucet", "bridge": {
                    "program": PROGRAM_B58, "configId": format!("0x{CID_HEX}"), "vault": format!("0x{VAULT_HEX}"),
                    "evmChainId": CHAIN, "lezReady": true, "fromBlock": 12_196_188,
                } }),
            );
        }
        if path == "/faucet" {
            let mut st = state.lock().unwrap();
            st.faucet_called = true;
            st.faucet_body = Some(req.clone());
            return (200, json!({ "ok": true, "hash": "faucet-transaction", "credited": FAUCET_LGO.to_string() }));
        }
        assert_eq!(path, "/rpc", "only the recorded network paths");
        let method = req["method"].as_str().unwrap_or("").to_string();
        let mut st = state.lock().unwrap();
        let sends_before = st.sent.len();
        st.requests.push(NetworkRequest { method: method.clone(), sends_before });
        let burn_applied = st.burn_applied;
        let claim_applied = st.claim_applied;
        let result = match method.as_str() {
            "getLastBlockId" => json!(5),
            "getAccountsNonces" => {
                let n = req["params"][0].as_array().map_or(0, Vec::len);
                json!(vec![0; n])
            }
            "getFeeState" => json!({ "base_fee_exec": 8, "base_fee_stor": 8 }),
            "getAccountBalance" => {
                let first = req["params"][0].as_str() == Some(ACCOUNT0);
                json!(if first && st.faucet_called { FAUCET_LGO } else { INITIAL_LGO }.to_string())
            }
            "getTransaction" => json!(["loopback-block", 6]),
            "sendTransaction" => {
                let wire = req["params"][0].as_str().unwrap().to_string();
                let tx = base64::engine::general_purpose::STANDARD.decode(&wire).unwrap();
                assert_eq!(tx[0], 0, "public transaction wire");
                // The network runs B1: a plaintext claim must be the expected claim in B1's layout (its range), read
                // by B1's decoder; this tree's range-free layout is not B1's and is refused, as B1 would refuse it.
                let data = captured_bridge_data(&wire).unwrap_or_default();
                if let Some(claim) = deployed::decode_claim(&data) {
                    if claim != f.claim {
                        return (
                            200,
                            json!({ "jsonrpc": "2.0", "id": req["id"], "error": { "code": -32000, "message": format!("B1 network: not the expected claim in B1's layout: {claim:?}") } }),
                        );
                    }
                    let reply = hash_hex(&sha256(&tx[1..]));
                    st.claims.push(claim);
                    st.claim_applied = true;
                    st.sent.push(wire);
                    return (200, json!({ "jsonrpc": "2.0", "id": req["id"], "result": reply }));
                }
                let decoded = match captured_bridge_message(&wire) {
                    Ok(message) if message == f.burn_message => message,
                    Ok(message) => {
                        return (
                            200,
                            json!({ "jsonrpc": "2.0", "id": req["id"], "error": { "code": -32000, "message": format!("unexpected bridge message: {message:?}") } }),
                        );
                    }
                    Err(e) => {
                        return (
                            200,
                            json!({ "jsonrpc": "2.0", "id": req["id"], "error": { "code": -32000, "message": e } }),
                        );
                    }
                };
                let reply = hash_hex(&sha256(&tx[1..]));
                st.decoded.push(decoded);
                st.burn_applied = true;
                st.sent.push(wire);
                json!(reply)
            }
            "getAccount" => {
                let id = req["params"][0].as_str().unwrap_or("");
                if id == PROGRAM_B58 {
                    let mut loader = vec![0u8];
                    loader.extend_from_slice(&fixed::<32>(IMAGE));
                    return (
                        200,
                        json!({ "jsonrpc": "2.0", "id": req["id"], "result": { "data": { "actor_states": { "loader": loader } } } }),
                    );
                }
                let state = if id == f.config {
                    json!(f.config_state)
                } else if id == f.root {
                    json!(f.root_state)
                } else if id == f.token {
                    json!(f.token_state)
                } else if id == f.holder {
                    if burn_applied {
                        json!(&f.holder_after_burn)
                    } else if claim_applied {
                        json!(&f.holder_before_burn)
                    } else {
                        Value::Null // nothing claimed yet: no holding
                    }
                } else if id == f.nullifier {
                    // Spent only once the network decoded the expected B1 claim: no state from a send count.
                    if claim_applied { json!(&f.nullifier_state) } else { Value::Null }
                } else if id == f.burn && burn_applied {
                    json!(f.burn_state)
                } else {
                    Value::Null
                };
                json!({ "data": { "actor_states": { (PROGRAM_B58): state } } })
            }
            other => {
                return (
                    200,
                    json!({ "jsonrpc": "2.0", "id": req["id"], "error": { "code": -32601, "message": other } }),
                )
            }
        };
        (200, json!({ "jsonrpc": "2.0", "id": req["id"], "result": result }))
    })
}

fn ethereum(f: Arc<Fixture>) -> String {
    serve(move |_path, req| {
        let result = match req["method"].as_str().unwrap_or("") {
            "eth_chainId" => json!(format!("0x{CHAIN:x}")),
            "eth_getCode" => json!("0x6080604052"),
            "eth_blockNumber" => json!(format!("0x{:x}", 12_196_200)),
            "eth_getBlockByNumber" => json!({ "number": format!("0x{:x}", 12_196_200), "timestamp": "0x6553f100" }),
            "eth_getBalance" => json!("0x0"),
            "eth_getLogs" => {
                let topic = req["params"][0]["topics"][0].as_str().unwrap_or("");
                json!(if topic == hex(&keccak("Deposit(address,uint64,uint64,address,uint256,bytes32)")) {
                    vec![f.deposit_log.clone()]
                } else {
                    vec![]
                })
            }
            "eth_call" => {
                let data = req["params"][0]["data"].as_str().unwrap_or("");
                if data.starts_with(&hex(&keccak("lezConfigId()")[..4])) {
                    json!(hex(&f.cid))
                } else if data.starts_with(&hex(&keccak("nextLeafIndex()")[..4])) {
                    json!(hex(&word64(1)))
                } else if data.starts_with(&hex(&keccak("depositRoot()")[..4])) {
                    json!(hex(&f.deposit_root))
                } else {
                    return (
                        200,
                        json!({ "jsonrpc": "2.0", "id": req["id"], "error": { "code": -32000, "message": format!("unexpected eth_call {data}") } }),
                    );
                }
            }
            other => {
                return (
                    200,
                    json!({ "jsonrpc": "2.0", "id": req["id"], "error": { "code": -32601, "message": other } }),
                )
            }
        };
        (200, json!({ "jsonrpc": "2.0", "id": req["id"], "result": result }))
    })
}

fn value(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("reply is not JSON: {e}: {s}"))
}
fn assert_error(r: &Value, part: &str) {
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap_or("").contains(part), "wanted {part:?}: {r}");
}
fn assert_ok(r: &Value) -> &Value {
    assert_eq!(r["ok"], true, "{r}");
    r
}

/// Inspect the ledger before confirm is submitted, not only after the whole calls file. Each request's snapshot also
/// proves that the stand-in had received only the preceding operations' transactions while this phase was answered.
fn assert_no_new_send(state: &Mutex<NetworkState>, first_request: usize, sends_before: usize, phase: &str) {
    let st = state.lock().unwrap();
    assert_eq!(st.sent.len(), sends_before, "{phase} sent a transaction before its confirm phase");
    let requests = &st.requests[first_request..];
    assert!(requests.iter().all(|r| r.sends_before == sends_before), "{phase} observed an early send: {requests:?}");
    assert!(!requests.iter().any(|r| r.method == "sendTransaction"), "{phase} reached sendTransaction: {requests:?}");
}

fn confirm_and_wait(view: &mut ViewClient, pending_id: &str) -> Value {
    let started = value(&view.call("confirm", &json!({ "pending_id": pending_id })));
    assert_ok(&started);
    let id = started["result"]["job"]["id"].clone();
    assert!(!id.is_null(), "confirm returned no job: {started}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let job = value(&view.call("job", &json!({ "id": id })));
        assert_ok(&job);
        if job["result"]["running"] == false {
            return job;
        }
        assert!(std::time::Instant::now() < deadline, "confirm job did not finish: {job}");
        thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// The captured burn carries one valid signature over exactly its approved message hash.
fn assert_signed(wire: &str, approved_hash: &str) {
    let raw = base64::engine::general_purpose::STANDARD.decode(wire).unwrap();
    assert_eq!(raw[0], 0);
    let tx = &raw[1..];
    let (message, rest) = tx.split_at(tx.len() - 4 - 96);
    assert_eq!(rest[..4], 1u32.to_le_bytes(), "one witness");
    let mut domain = [0u8; 32];
    let prefix = b"/LEE/v0.3/Message/Public/";
    domain[..prefix.len()].copy_from_slice(prefix);
    let actual_hash = hash_hex(&sha256(&[domain.as_slice(), message].concat()));
    assert_eq!(actual_hash, approved_hash.trim_start_matches("0x"), "the signed message is the approved one");
    let signature = k256::schnorr::Signature::try_from(&rest[4..68]).unwrap();
    let key = k256::schnorr::VerifyingKey::from_bytes(&rest[68..100]).unwrap();
    let digest: [u8; 32] = fixed(approved_hash.trim_start_matches("0x"));
    key.verify_raw(&digest, &signature).expect("BIP-340 signature over the approved message");
}

#[test]
fn terminal_calls_file_drives_the_whole_network_session_offline() {
    let fixture = Arc::new(Fixture::new());
    let state = Arc::new(Mutex::new(NetworkState::default()));
    let base = network(fixture.clone(), state.clone());
    let eth = ethereum(fixture.clone());
    assert!(base.starts_with("http://127.0.0.1:") && eth.starts_with("http://127.0.0.1:"));
    let file = include_str!("../examples/network-session.jsonl").replace("$NETWORK", &base).replace("$ETH_RPC", &eth);
    assert!(!file.contains("https://"), "no public endpoint in the replay");
    let replies = wallet_calls(&file);
    assert_eq!(replies.len(), 18, "one reply per directive in the committed calls file");
    let r: Vec<Value> = replies.iter().map(|s| value(s)).collect();

    assert_error(&r[0], "opened by the Ferrata Bridge view");
    for i in [1, 2] {
        assert_error(&r[i], "no wallet session");
    }
    let opened = assert_ok(&r[3]);
    assert_eq!(opened["result"]["account0"], ACCOUNT0);
    assert_eq!(opened["result"]["chain_id"], CHAIN);
    assert_eq!(opened["result"]["public_accounts"].as_array().unwrap().len(), 8);
    assert_eq!(opened["result"]["lez_rpc"], format!("{base}/rpc"));
    assert_eq!(opened["result"]["eth_rpc"], eth);
    assert_eq!(opened["result"]["checks"].as_array().unwrap().len(), 3, "configuration, Ethereum and LEZ checks");
    assert!(!replies[3].contains(PHRASE), "the phrase is never printed by the open reply");
    assert_error(&r[4], "belongs to the Ferrata Bridge view");
    assert_error(&r[5], "each call must carry its counter and MAC");
    for (i, r) in r.iter().enumerate().take(16).skip(3) {
        assert_eq!(r["session_account"], ACCOUNT0, "reply {i} names the session's /0");
    }

    let balances = assert_ok(&r[6]);
    assert_eq!(balances["result"]["unit"], "10^-9 LGO");
    let accounts = balances["result"]["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 8, "all accounts in the phrase's recovery window");
    for (index, a) in accounts.iter().enumerate() {
        assert_eq!(a["index"], index as u64);
        assert_eq!(a["balance"], INITIAL_LGO.to_string(), "before the faucet: {a}");
    }
    assert_eq!(accounts[0]["id"], ACCOUNT0);
    let faucet = assert_ok(&r[7]);
    assert_eq!(faucet["result"]["account"], 0);
    assert_eq!(faucet["result"]["id"], ACCOUNT0);
    assert_eq!(faucet["result"]["balance_before"], INITIAL_LGO.to_string());
    assert_eq!(faucet["result"]["balance"], FAUCET_LGO.to_string());
    assert_eq!(faucet["result"]["credited"], FAUCET_LGO.to_string());
    assert_eq!(faucet["result"]["seen"], true);
    assert_eq!(faucet["result"]["tx_hash"], "faucet-transaction");
    assert_error(&r[8], "no transaction waits for approval");

    // The deployment pins B1 (the test network's program): the plaintext claim is built in B1's layout, with the fee
    // schedule range its token state gives, for approval; nothing is signed or sent until its confirm.
    let claim = assert_ok(&r[9]);
    let claim_approval = &claim["result"]["to_approve"];
    assert_eq!(claim_approval["kind"], "claim");
    assert_eq!(claim_approval["signed"], false);
    assert_eq!(claim_approval["program"], "ferrata_bridge_b1");
    assert_eq!(claim_approval["fee_schedule_range"], json!({ "index": 0, "from_leaf": 0, "until_leaf": 1 }));
    let claim_bytes = unhex(&claim_approval["message"].as_str().unwrap()[2..]);
    assert_eq!(deployed::decode_claim(&claim_bytes).as_ref(), Some(&fixture.claim), "the approved claim is B1's");
    assert!(bw::BridgeMessage::try_from_slice(&claim_bytes).is_err(), "not this tree's range-free layout");
    assert_eq!(assert_ok(&r[10])["result"]["job"]["kind"], "claim");
    let claim_job = assert_ok(&r[11]);
    assert_eq!(claim_job["result"]["state"], "settled", "{claim_job}");
    let claim_done = &claim_job["result"]["result"];
    assert_eq!(claim_done["approved"]["message_hash"], claim_approval["message_hash"]);
    assert_eq!(claim_done["balance"], CLAIM_AMOUNT.to_string(), "{claim_done}");
    assert_eq!(state.lock().unwrap().faucet_body, Some(json!({ "account": ACCOUNT0, "public_key": PUBLIC_KEY0 })));

    let burn = assert_ok(&r[12]);
    let burn_approval = &burn["result"]["to_approve"];
    assert_eq!(burn_approval["signed"], false);
    assert_eq!(burn_approval["kind"], "burn");
    assert_eq!(burn_approval["i"], 0);
    assert_eq!(burn_approval["amount"], BURN_AMOUNT.to_string());
    assert_eq!(burn_approval["recipient"], RECIPIENT);
    assert!(
        burn["result"].get("tx_hash").is_none() && burn["result"].get("block").is_none(),
        "the public burn's approve step sends nothing"
    );
    assert_eq!(assert_ok(&r[13])["result"]["job"]["kind"], "burn");
    let burn_job = assert_ok(&r[14]);
    assert_eq!(burn_job["result"]["state"], "settled", "{burn_job}");
    let burn_done = &burn_job["result"]["result"];
    assert_eq!(burn_done["approved"]["message_hash"], burn_approval["message_hash"]);
    assert_eq!(burn_done["balance"], (CLAIM_AMOUNT - BURN_AMOUNT).to_string());
    {
        let st = state.lock().unwrap();
        assert_eq!(st.sent.len(), 2, "the confirmed claim, then the confirmed burn");
        assert_signed(&st.sent[0], claim_done["approved"]["message_hash"].as_str().unwrap());
        assert_eq!(captured_bridge_data(&st.sent[0]).unwrap(), claim_bytes);
        assert_eq!(st.claims.as_slice(), &[fixture.claim.clone()], "state followed the decoded B1 claim");
        assert_signed(&st.sent[1], burn_done["approved"]["message_hash"].as_str().unwrap());
        assert_eq!(captured_bridge_message(&st.sent[1]).unwrap(), fixture.burn_message);
        assert_eq!(
            st.decoded.as_slice(),
            &[fixture.burn_message.clone()],
            "state followed the decoded Burn, not a send counter"
        );
    }

    let release = assert_ok(&r[15]);
    assert_eq!(release["result"]["handoff"], true);
    assert_eq!(release["result"]["settled"], false);
    assert_eq!(release["result"]["released"], false);
    assert_eq!(release["result"]["i"], 0);
    assert_eq!(release["result"]["amount"], BURN_AMOUNT.to_string());
    assert_eq!(release["result"]["recipient"], format!("0x{DEPOSITOR_HEX}"));
    assert!(release["result"]["what"].as_str().unwrap().contains("ferrata release --burn 0"));
    assert_eq!(assert_ok(&r[16])["result"]["keys"], "wiped");
    assert_error(&r[17], "no wallet session");

    // A second open exercises the same client as the file: a genuine envelope succeeds once, its exact replay is
    // refused, a forged tag and another caller cannot replace it, and a fresh counter works afterwards.
    // A fresh B1 fixture, so the direct approval-boundary checks below begin with an empty send ledger.
    let fixture = Arc::new(Fixture::new());
    let state = Arc::new(Mutex::new(NetworkState::default()));
    let base = network(fixture.clone(), state.clone());
    let eth = ethereum(fixture.clone());
    let mut view = ViewClient::new("module:ferrata_bridge_ui");
    let (opened, words) = view.open(&json!({ "network": base, "eth_rpc": eth }), Some(PHRASE));
    assert!(words.is_none());
    assert_eq!(assert_ok(&value(&opened))["result"]["account0"], ACCOUNT0);
    let envelope = view.envelope("balances", &json!({}));
    assert_ok(&value(&view.send(&envelope)));
    assert_error(&value(&view.send(&envelope)), "a replay");
    let other = wallet_as("module:other_module", &envelope);
    assert_error(&value(&other), "belongs to the Ferrata Bridge view");
    let forged = r#"{"call":"{\"method\":\"balances\",\"params\":{}}","n":99,"mac":"00"}"#;
    assert_error(&value(&wallet_as("module:ferrata_bridge_ui", forged)), "MAC does not hold");
    assert_ok(&value(&view.call("balances", &json!({}))));

    // Review r1 M1: drive these boundaries interactively. Neither the claim nor the burn signs or sends at approve time
    // (a send hidden in a ledger inspected only after confirm would fail here). The request snapshots cover every RPC
    // made before each assertion, including the prepare nonce/fee/balance reads and any misplaced sendTransaction.
    assert_ok(&value(&view.call("faucet", &json!({ "account": 0 }))));
    let first_request = state.lock().unwrap().requests.len();
    let boundary_claim = value(&view.call("claim", &json!({ "leaf_index": 0, "private": false })));
    assert_ok(&boundary_claim);
    let approval = &boundary_claim["result"]["to_approve"];
    assert_eq!((approval["kind"].as_str(), approval["program"].as_str()), (Some("claim"), Some("ferrata_bridge_b1")));
    assert_no_new_send(&state, first_request, 0, "claim approval");
    let claim_hash = approval["message_hash"].as_str().unwrap().to_string();
    let claim_job = confirm_and_wait(&mut view, approval["pending_id"].as_str().unwrap());
    assert_eq!(claim_job["result"]["state"], "settled", "{claim_job}");
    assert_eq!(claim_job["result"]["result"]["approved"]["message_hash"], claim_hash);

    let first_request = state.lock().unwrap().requests.len();
    let boundary_burn = value(&view.call(
        "burn",
        &json!({
            "private": false, "account": 0, "amount": BURN_AMOUNT.to_string(), "recipient": RECIPIENT,
            "release_delay_ms": 1000,
        }),
    ));
    assert_ok(&boundary_burn);
    let approval = &boundary_burn["result"]["to_approve"];
    assert_eq!(approval["kind"], "burn");
    assert_eq!(approval["signed"], false);
    assert_no_new_send(&state, first_request, 1, "burn approval");
    let pending_id = approval["pending_id"].as_str().unwrap().to_string();
    let approved_hash = approval["message_hash"].as_str().unwrap().to_string();
    let boundary_job = confirm_and_wait(&mut view, &pending_id);
    let boundary_done = &boundary_job["result"]["result"];
    assert_eq!(boundary_job["result"]["state"], "settled", "{boundary_job}");
    assert_eq!(boundary_done["settled"], true, "{boundary_done}");
    assert_eq!(boundary_done["approved"]["message_hash"], approved_hash);
    {
        let st = state.lock().unwrap();
        assert_eq!(st.sent.len(), 2, "exactly the confirmed claim and the confirmed burn were sent");
        assert_signed(&st.sent[0], &claim_hash);
        assert_eq!(st.claims.as_slice(), &[fixture.claim.clone()]);
        assert_signed(&st.sent[1], &approved_hash);
        assert_eq!(captured_bridge_message(&st.sent[1]).unwrap(), fixture.burn_message);
        assert_eq!(st.decoded.as_slice(), &[fixture.burn_message.clone()]);
    }
    assert_ok(&value(&view.call("close", &json!({}))));
}
