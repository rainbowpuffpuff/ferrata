//! The watcher's sends after a failed one, and after a send by another process with the same key (internal review of
//! 27 November 2026, docs/security/internal-review-2026-11-27.md, C-1).
//!
//! alloy's recommended fillers keep the account's nonce in a cache and count it up at every fill, failed sends
//! included, and never ask the node again. A long-running monitor that once failed to send (a 429 at the gas estimate,
//! a candidate another watcher cancelled between the simulation and the estimate, no funds) sent every later
//! `cancelCandidate`, `pauseCheckpoints` or `revokeFrom` one number too high, where it waited for good behind a gap
//! nothing fills; and one whose key also sent through the Logos module, or by hand, sent with a number already used.
//! The monitor's provider now takes the node's pending nonce at every send. Needs anvil on PATH (Foundry); without
//! it the test says so and passes, as CI's monitor job installs it.

use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::signers::local::PrivateKeySigner;
use ferrata_monitor::chain::signing_provider;

struct Anvil(Child);

impl Drop for Anvil {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn anvil() -> Option<(Anvil, String)> {
    let port = TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok()?.port();
    let child = Command::new("anvil")
        .args(["--port", &port.to_string(), "--silent"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let guard = Anvil(child);
    for _ in 0..100 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Some((guard, format!("http://127.0.0.1:{port}")));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

fn transfer(from: Address) -> TransactionRequest {
    TransactionRequest::default().with_from(from).with_to(Address::with_last_byte(0xee)).with_value(U256::from(1))
}

#[tokio::test]
async fn a_failed_send_or_another_senders_transaction_does_not_strand_the_next_send() {
    let Some((_anvil, url)) = anvil() else {
        eprintln!("anvil is not on PATH: test not run");
        return;
    };
    let signer = PrivateKeySigner::random();
    let me = signer.address();
    let p = signing_provider(&url, signer.clone()).expect("provider");
    let plain = ProviderBuilder::new().connect_http(url.parse().unwrap());

    // 1. A send that fails after its nonce was filled: the key holds no ETH.
    assert!(p.send_transaction(transfer(me)).await.is_err(), "a key with no ETH cannot send");

    // 2. Funded, the next send must take nonce 0 and be mined.
    let _: () = plain.raw_request("anvil_setBalance".into(), (me, U256::from(10u128.pow(19)))).await.unwrap();
    let pending = p.send_transaction(transfer(me)).await.expect("sent");
    let receipt = pending.with_timeout(Some(Duration::from_secs(10))).get_receipt().await;
    assert!(receipt.is_ok(), "the send after a failed one waits behind a nonce gap: {receipt:?}");
    let hash = receipt.unwrap().transaction_hash;
    let tx = plain.get_transaction_by_hash(hash).await.unwrap().unwrap();
    assert_eq!(alloy::consensus::Transaction::nonce(&tx), 0, "the account's first transaction");

    // 3. The same key sends elsewhere (the Logos module, an operator by hand): the monitor's next send must follow it.
    let other = signing_provider(&url, signer).expect("a second sender with the same key");
    other.send_transaction(transfer(me)).await.unwrap().get_receipt().await.expect("the other sender's transaction");
    let pending = p.send_transaction(transfer(me)).await;
    assert!(pending.is_ok(), "the monitor reused a number another sender used: {pending:?}");
    let receipt = pending.unwrap().with_timeout(Some(Duration::from_secs(10))).get_receipt().await;
    assert!(receipt.is_ok(), "not mined: {receipt:?}");
    assert_eq!(plain.get_transaction_count(me).await.unwrap(), 3, "three transactions from the key");
}
