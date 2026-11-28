//! The LEZ version a sequencer endpoint runs, checked before anything is signed or sent.
//!
//! This ferrata is built for one LEZ tag (its features: none for v0.4.0-rc1, `lez-rc2`, `lez-rc3`, `lez-040` for
//! v0.4.0), and LEZ v0.4.0 changed the transaction, fee and block formats (LEZ #1107 receipts, #1135 rotating keys,
//! #1144 actor authorities): a transaction built for one is refused by the other, after the user has signed it, or a
//! read misdecodes. LEZ v0.4.0 added `getSequencerStatus`; an earlier sequencer answers it with JSON-RPC error -32601
//! (method not found), and so does the test network's gateway for a method it does not forward
//! (deploy/testnet/host/bin/ferrata-gateway: HTTP 403 with error -32601). A v0.4.0 sequencer still bootstrapping
//! answers it too, with `state: "bootstrapping"`; v0.4.0's own "not online yet" error, -31902, which only v0.4.0 has and
//! which its other methods give while it bootstraps, is read as v0.4.0 too. The gateway's rule holds while it fronts an rc3
//! sequencer: a gateway in front of a v0.4.0 sequencer must forward `getSequencerStatus` (and `getTransactionReceipt`),
//! or every client reads that network as "before v0.4.0" (P2's v0.4.0 redeploy notes).
//!
//! A deposit (Ethereum side) is checked too: it commits to a LEZ account of this build's key tree, and LEZ v0.4.0
//! makes a public account's id differently (from the account secret's first key state, before it from the public key).

use std::time::Duration;

use alloy::transports::http::reqwest;
use anyhow::{anyhow, bail, Context as _, Result};

/// The LEZ tag this binary is built for.
pub const BUILT_FOR: &str = if cfg!(feature = "lez-040") {
    "v0.4.0"
} else if cfg!(feature = "lez-rc3") {
    "v0.4.0-rc3"
} else if cfg!(feature = "lez-rc2") {
    "v0.4.0-rc2"
} else {
    "v0.4.0-rc1"
};

/// What a sequencer's answer to `getSequencerStatus` says of its LEZ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkLez {
    /// It serves `getSequencerStatus`: LEZ v0.4.0.
    V040,
    /// It does not (-32601): a LEZ before v0.4.0 (the test network runs v0.4.0-rc3).
    BeforeV040,
}

/// LEZ v0.4.0's error while a sequencer bootstraps from its channel (`SEQUENCER_IS_NOT_ONLINE_ERROR_CODE`).
const NOT_ONLINE: i64 = -31902;
const METHOD_NOT_FOUND: i64 = -32601;

/// The network's LEZ from the HTTP status and body of its answer to `getSequencerStatus`.
pub fn classify(rpc: &str, status: u16, body: &[u8]) -> Result<NetworkLez> {
    let reply: serde_json::Value = serde_json::from_slice(body).map_err(|e| {
        anyhow!("the LEZ sequencer {rpc} answered getSequencerStatus with HTTP {status} and no JSON-RPC reply: {e}")
    })?;
    if let Some(code) = reply.pointer("/error/code").and_then(serde_json::Value::as_i64) {
        return match code {
            METHOD_NOT_FOUND => Ok(NetworkLez::BeforeV040),
            NOT_ONLINE => Ok(NetworkLez::V040),
            _ => bail!(
                "the LEZ sequencer {rpc} answered getSequencerStatus with error {code} ({}): its LEZ version is unknown, \
                 nothing was signed or sent",
                reply.pointer("/error/message").and_then(serde_json::Value::as_str).unwrap_or("no message")
            ),
        };
    }
    // v0.4.0's `SequencerStatus`, tagged by `state` (lez/sequencer/service/protocol/src/lib.rs): any other result (a
    // stand-in's null) says nothing of the version.
    let state = reply.pointer("/result/state").and_then(serde_json::Value::as_str);
    if (200..300).contains(&status) && matches!(state, Some("online" | "bootstrapping")) {
        return Ok(NetworkLez::V040);
    }
    bail!(
        "the LEZ sequencer {rpc} answered getSequencerStatus with HTTP {status} and neither a sequencer status nor an \
         error: its LEZ version is unknown, nothing was signed or sent"
    )
}

/// The refusal for a network of another LEZ than `built` (exact text; tests/lez_version.rs quotes it).
pub fn refusal(built: &str, network: NetworkLez, rpc: &str) -> String {
    match network {
        NetworkLez::BeforeV040 => format!(
            "refused before anything was signed or sent: this ferrata is built for LEZ {built}, but the sequencer at \
             {rpc} runs a LEZ before v0.4.0 (it does not serve getSequencerStatus, which v0.4.0 added); for the test \
             network (LEZ v0.4.0-rc3) use the ferrata deploy/testnet/cli-rc3/build.sh builds"
        ),
        NetworkLez::V040 => format!(
            "refused before anything was signed or sent: this ferrata is built for LEZ {built}, but the sequencer at \
             {rpc} runs LEZ v0.4.0 (it serves getSequencerStatus); use the ferrata cli/v040/build.sh builds"
        ),
    }
}

/// The refusal of a deposit to send without a sequencer to read the network's LEZ from (exact text; tests quote it).
pub fn deposit_needs_sequencer(built: &str) -> String {
    format!(
        "refused before anything was signed or sent: a deposit commits to a LEZ account of this ferrata's key tree, \
         which LEZ v0.4.0 changed, so this ferrata (built for LEZ {built}) reads the network's LEZ version first: pass \
         --sequencer (or FERRATA_SEQUENCER) with the zone's sequencer RPC"
    )
}

/// What [`require_for_deposit`] returns, and what [`crate::flow::deposit`] takes: the deposit's LEZ check was made. Only
/// this module makes one (its field is private), so no caller of the library deposits without the check.
#[derive(Clone, Copy, Debug)]
pub struct DepositCheck(());

/// Before a deposit: the network's LEZ whenever a sequencer is named; without one, a v0.4.0 build sends nothing (the
/// deployed network runs v0.4.0-rc3, whose builds keep depositing as before).
pub async fn require_for_deposit(rpc: Option<&str>, send: bool) -> Result<DepositCheck> {
    match rpc {
        Some(rpc) => require(rpc).await.map(|_| DepositCheck(())),
        None if send && BUILT_FOR == "v0.4.0" => bail!(deposit_needs_sequencer(BUILT_FOR)),
        None => Ok(DepositCheck(())),
    }
}

/// Whether a binary built for `built` may sign and send on a network of `network`.
pub fn check_for(built: &str, network: NetworkLez, rpc: &str) -> Result<()> {
    match (built == "v0.4.0", network) {
        (true, NetworkLez::V040) | (false, NetworkLez::BeforeV040) => Ok(()),
        _ => bail!(refusal(built, network, rpc)),
    }
}

/// How long [`probe`] waits for an answer.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

/// Asks the sequencer at `rpc` for its status (one JSON-RPC call, nothing signed).
pub async fn probe(rpc: &str) -> Result<NetworkLez> {
    probe_within(rpc, PROBE_TIMEOUT).await
}

/// [`probe`] with another timeout (the chain service checks a candidate sequencer within its own probe time).
pub async fn probe_within(rpc: &str, timeout: Duration) -> Result<NetworkLez> {
    let client = reqwest::Client::builder().timeout(timeout).build()?;
    let body = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "getSequencerStatus", "params": [] });
    let res = client
        .post(rpc)
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .with_context(|| format!("the LEZ sequencer {rpc} does not answer"))?;
    let status = res.status().as_u16();
    let bytes = res.bytes().await.with_context(|| format!("reading the LEZ sequencer {rpc}'s answer"))?;
    classify(rpc, status, &bytes)
}

/// [`probe`], then [`check_for`] this binary's tag: run before a command builds, signs or sends anything on LEZ.
pub async fn require(rpc: &str) -> Result<NetworkLez> {
    require_within(rpc, PROBE_TIMEOUT).await
}

/// [`require`] with another timeout.
pub async fn require_within(rpc: &str, timeout: Duration) -> Result<NetworkLez> {
    let network = probe_within(rpc, timeout).await?;
    check_for(BUILT_FOR, network, rpc)?;
    Ok(network)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    const RPC: &str = "http://127.0.0.1:1/rpc";

    #[test]
    fn answers_classify() {
        let ok = br#"{"jsonrpc":"2.0","id":1,"result":{"state":"online","height":7,"isOurTurn":true}}"#;
        assert_eq!(classify(RPC, 200, ok).unwrap(), NetworkLez::V040);
        let replaying = br#"{"jsonrpc":"2.0","id":1,"result":{"state":"bootstrapping","targetSlot":9,"replayedUpToSlot":null,"height":null}}"#;
        assert_eq!(classify(RPC, 200, replaying).unwrap(), NetworkLez::V040);
        // A stand-in answering every method with null names no version.
        assert!(classify(RPC, 200, br#"{"jsonrpc":"2.0","id":1,"result":null}"#).is_err());
        let boot = br#"{"jsonrpc":"2.0","id":1,"error":{"code":-31902,"message":"Sequencer is not online yet"}}"#;
        assert_eq!(classify(RPC, 200, boot).unwrap(), NetworkLez::V040);
        let rc3 = br#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        assert_eq!(classify(RPC, 200, rc3).unwrap(), NetworkLez::BeforeV040);
        // The test network's gateway (rc3): a method it does not forward.
        let gw = br#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"method getSequencerStatus is not served by this gateway"}}"#;
        assert_eq!(classify(RPC, 403, gw).unwrap(), NetworkLez::BeforeV040);
        let other = br#"{"jsonrpc":"2.0","id":1,"error":{"code":-32603,"message":"Internal error"}}"#;
        assert!(classify(RPC, 200, other).unwrap_err().to_string().contains("error -32603 (Internal error)"));
        assert!(classify(RPC, 502, b"<html>Bad Gateway</html>").unwrap_err().to_string().contains("HTTP 502"));
        assert!(classify(RPC, 500, br#"{"jsonrpc":"2.0","id":1,"result":1}"#).is_err());
    }

    #[test]
    fn refusals_are_exact() {
        assert!(check_for("v0.4.0", NetworkLez::V040, RPC).is_ok());
        assert!(check_for("v0.4.0-rc3", NetworkLez::BeforeV040, RPC).is_ok());
        assert!(check_for("v0.4.0-rc1", NetworkLez::BeforeV040, RPC).is_ok());
        assert_eq!(
            check_for("v0.4.0", NetworkLez::BeforeV040, RPC).unwrap_err().to_string(),
            "refused before anything was signed or sent: this ferrata is built for LEZ v0.4.0, but the sequencer at \
             http://127.0.0.1:1/rpc runs a LEZ before v0.4.0 (it does not serve getSequencerStatus, which v0.4.0 \
             added); for the test network (LEZ v0.4.0-rc3) use the ferrata deploy/testnet/cli-rc3/build.sh builds"
        );
        assert_eq!(
            check_for("v0.4.0-rc3", NetworkLez::V040, RPC).unwrap_err().to_string(),
            "refused before anything was signed or sent: this ferrata is built for LEZ v0.4.0-rc3, but the sequencer at \
             http://127.0.0.1:1/rpc runs LEZ v0.4.0 (it serves getSequencerStatus); use the ferrata cli/v040/build.sh \
             builds"
        );
    }

    #[test]
    fn built_for_follows_the_features() {
        let want = if cfg!(feature = "lez-040") {
            "v0.4.0"
        } else if cfg!(feature = "lez-rc3") {
            "v0.4.0-rc3"
        } else if cfg!(feature = "lez-rc2") {
            "v0.4.0-rc2"
        } else {
            "v0.4.0-rc1"
        };
        assert_eq!(BUILT_FOR, want);
    }

    /// A one-request HTTP stand-in on loopback answering `status` and `body`; returns its URL.
    async fn stand_in(status: &'static str, body: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/rpc", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let n = s.read(&mut buf).await.unwrap();
            assert!(String::from_utf8_lossy(&buf[..n]).contains("\"method\":\"getSequencerStatus\""));
            let reply = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            s.write_all(reply.as_bytes()).await.unwrap();
        });
        url
    }

    #[tokio::test]
    async fn probe_reads_a_sequencer_and_the_gateway() {
        let v040 =
            stand_in("200 OK", r#"{"jsonrpc":"2.0","id":1,"result":{"state":"online","height":3,"isOurTurn":false}}"#)
                .await;
        assert_eq!(probe(&v040).await.unwrap(), NetworkLez::V040);
        let gw = stand_in(
            "403 Forbidden",
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"method getSequencerStatus is not served by this gateway"}}"#,
        )
        .await;
        assert_eq!(probe(&gw).await.unwrap(), NetworkLez::BeforeV040);
        // Nothing listens: the command stops with the endpoint named.
        let closed = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}/rpc", l.local_addr().unwrap())
        };
        assert!(probe(&closed).await.unwrap_err().to_string().contains("does not answer"));
    }
}
