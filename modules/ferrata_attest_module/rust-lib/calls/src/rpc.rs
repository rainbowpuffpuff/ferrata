//! Ethereum JSON-RPC for the module, by the path the request names:
//!
//! * `"eth": {"via": "eth_rpc_module", "chain_id": N}`: through Logos Core's own `eth_rpc_module`
//!   (`raw_rpc(chain_id, method, params)`), with whatever endpoint, proxy policy and verified-proxy mode
//!   that module holds for chain N (`verifiedProxyMode: "required"` sends the call on to
//!   `verified_proxy_module`). Reachable only inside Logos Core: the module crate installs the client
//!   with [`set_eth_rpc_module`] (modules/LOGOS-CORE-MODULES.md).
//! * `"rpc_url": "http://..."` (or `"eth": {"via": "http", "rpc_url": ...}`): the plain path kept as the
//!   fallback, a minimal HTTP/1.1 client over `std::net`, plain `http://` only (a local node or a
//!   recorded-data server, `modules/scripts/recorded_rpc.py`); remote `https://` endpoints are refused.
//!
//! Nothing either path returns is trusted: every proof is checked against a record before it is used.
//! [`Rpc::report`] says which path answered and, through `eth_rpc_module`, the route it named for each
//! call (`direct`, `verified` or `proxied`).
//!
//! This file is the same in `ferrata_attest_module` and `ferrata_bridge_module` (scripts/native.sh
//! compares them).

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{json, Value};

/// The largest response read (bytes): a mainnet block's receipts are a few megabytes.
const MAX_RESPONSE: u64 = 64 << 20;

/// How the module crate reaches `eth_rpc_module.raw_rpc`: (chain id, method, the params as JSON array
/// text, a timeout in milliseconds) -> that module's reply text (`{ok: true, result, route}` or
/// `{ok: false, code, error}`), or why the call did not reach it.
pub type EthRpcModuleCall = fn(i64, &str, &str, u64) -> Result<String, String>;

static ETH_RPC_MODULE: OnceLock<EthRpcModuleCall> = OnceLock::new();

/// Installed once by the module crate (inside Logos Core); natively nothing is installed and the
/// `eth_rpc_module` path answers with an error.
pub fn set_eth_rpc_module(f: EthRpcModuleCall) {
    let _ = ETH_RPC_MODULE.set(f);
}

enum Via {
    Http(Http),
    Module { chain_id: i64 },
}

/// One Ethereum endpoint as the request chose it, and what answered.
pub struct Rpc {
    via: Via,
    timeout_ms: u64,
    pub calls: u32,
    routes: Vec<String>,
}

impl Rpc {
    /// The plain path: an `http://` endpoint.
    pub fn new(url: &str, timeout_ms: u64) -> Result<Self, String> {
        Ok(Self { via: Via::Http(Http::new(url, timeout_ms)?), timeout_ms, calls: 0, routes: Vec::new() })
    }

    /// The path a request names: `eth` (`{"via": "eth_rpc_module", "chain_id": N}` or
    /// `{"via": "http", "rpc_url": URL}`), else `rpc_url`.
    pub fn from_request(req: &Value, timeout_ms: u64) -> Result<Self, String> {
        let eth = &req["eth"];
        if eth.is_null() {
            let url = req["rpc_url"].as_str().ok_or("rpc_url or eth: where to read Ethereum from")?;
            return Self::new(url, timeout_ms);
        }
        match eth["via"].as_str() {
            Some("eth_rpc_module") => {
                let chain_id = eth["chain_id"]
                    .as_i64()
                    .filter(|c| *c > 0)
                    .ok_or("eth.chain_id: the chain eth_rpc_module is asked about (a positive integer)")?;
                Ok(Self { via: Via::Module { chain_id }, timeout_ms, calls: 0, routes: Vec::new() })
            }
            Some("http") => Self::new(eth["rpc_url"].as_str().ok_or("eth.rpc_url: an http:// endpoint")?, timeout_ms),
            other => Err(format!("eth.via {other:?}: \"eth_rpc_module\" (Logos Core) or \"http\"")),
        }
    }

    /// `method(params)`; the `result`, or the node's (or eth_rpc_module's) error as text.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.calls += 1;
        match &self.via {
            Via::Http(h) => {
                let v = h.call(self.calls, method, params)?;
                self.routes.push("http".into());
                Ok(v)
            }
            Via::Module { chain_id } => {
                let f = ETH_RPC_MODULE
                    .get()
                    .ok_or_else(|| format!("{method}: eth_rpc_module is reachable only from inside Logos Core"))?;
                let text = f(*chain_id, method, &params.to_string(), self.timeout_ms)?;
                let v: Value = serde_json::from_str(&text)
                    .map_err(|e| format!("{method}: eth_rpc_module's reply is not JSON: {e}"))?;
                if v["ok"].as_bool() != Some(true) {
                    return Err(format!(
                        "{method}: eth_rpc_module answered {} ({})",
                        v["error"].as_str().unwrap_or("no detail"),
                        v["code"].as_str().unwrap_or("no code")
                    ));
                }
                self.routes.push(v["route"].as_str().unwrap_or("unnamed").to_string());
                Ok(v["result"].clone())
            }
        }
    }

    /// Which path answered: `{via, chain_id | rpc_url, calls, routes}`.
    pub fn report(&self) -> Value {
        match &self.via {
            Via::Http(h) => json!({
                "via": "http",
                "rpc_url": format!("http://{}:{}{}", h.host, h.port, h.path),
                "calls": self.calls,
                "routes": self.routes,
            }),
            Via::Module { chain_id } => json!({
                "via": "eth_rpc_module",
                "chain_id": chain_id,
                "calls": self.calls,
                "routes": self.routes,
            }),
        }
    }
}

/// The plain path: HTTP/1.1 over `std::net`, `http://` only.
struct Http {
    host: String,
    port: u16,
    path: String,
    timeout: Duration,
}

impl Http {
    fn new(url: &str, timeout_ms: u64) -> Result<Self, String> {
        let rest = url.strip_prefix("http://").ok_or_else(|| {
            format!("rpc_url {url:?}: only http:// endpoints (a local node or a recorded-data server); in Logos Core use eth_rpc_module")
        })?;
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h, p.parse::<u16>().map_err(|_| format!("rpc_url {url:?}: bad port"))?),
            None => (authority, 80),
        };
        if host.is_empty() {
            return Err(format!("rpc_url {url:?}: no host"));
        }
        Ok(Self {
            host: host.to_string(),
            port,
            path: path.to_string(),
            timeout: Duration::from_millis(timeout_ms.clamp(100, 120_000)),
        })
    }

    fn call(&self, id: u32, method: &str, params: Value) -> Result<Value, String> {
        let body = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
        let addr = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(|e| format!("{method}: cannot resolve {}: {e}", self.host))?
            .next()
            .ok_or_else(|| format!("{method}: no address for {}", self.host))?;
        let mut stream =
            TcpStream::connect_timeout(&addr, self.timeout).map_err(|e| format!("{method}: connect {addr}: {e}"))?;
        stream.set_read_timeout(Some(self.timeout)).map_err(|e| e.to_string())?;
        stream.set_write_timeout(Some(self.timeout)).map_err(|e| e.to_string())?;
        let head = format!(
            "POST {} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\nAccept: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n",
            self.path,
            self.host,
            self.port,
            body.len()
        );
        stream
            .write_all(head.as_bytes())
            .and_then(|_| stream.write_all(body.as_bytes()))
            .map_err(|e| format!("{method}: send: {e}"))?;
        let mut raw = Vec::new();
        stream.take(MAX_RESPONSE).read_to_end(&mut raw).map_err(|e| format!("{method}: read: {e}"))?;
        let payload = http_body(&raw).map_err(|e| format!("{method}: {e}"))?;
        let v: Value = serde_json::from_slice(&payload).map_err(|e| format!("{method}: response is not JSON: {e}"))?;
        if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
            return Err(format!("{method}: the node answered with an error: {err}"));
        }
        v.get("result").cloned().ok_or_else(|| format!("{method}: no result in the response"))
    }
}

/// The body of an HTTP/1.x response with status 200 (`Content-Length`, chunked, or to the end).
fn http_body(raw: &[u8]) -> Result<Vec<u8>, String> {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("no HTTP header in the response")?;
    let head = std::str::from_utf8(&raw[..end]).map_err(|_| "HTTP header is not UTF-8")?;
    let mut lines = head.split("\r\n");
    let status = lines.next().unwrap_or_default();
    let code = status.split_whitespace().nth(1).unwrap_or_default();
    if code != "200" {
        return Err(format!("HTTP status {status:?}"));
    }
    let mut chunked = false;
    let mut length = None;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            if k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked") {
                chunked = true;
            } else if k == "content-length" {
                length = v.parse::<usize>().ok();
            }
        }
    }
    let body = &raw[end + 4..];
    if chunked {
        let mut out = Vec::new();
        let mut rest = body;
        loop {
            let eol = rest.windows(2).position(|w| w == b"\r\n").ok_or("truncated chunk size")?;
            let size_text = std::str::from_utf8(&rest[..eol]).map_err(|_| "bad chunk size")?;
            let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
                .map_err(|_| "bad chunk size")?;
            rest = &rest[eol + 2..];
            if size == 0 {
                return Ok(out);
            }
            if rest.len() < size + 2 {
                return Err("truncated chunk".into());
            }
            out.extend_from_slice(&rest[..size]);
            rest = &rest[size + 2..];
        }
    }
    match length {
        Some(n) if body.len() >= n => Ok(body[..n].to_vec()),
        Some(n) => Err(format!("truncated body: {} of {n} bytes", body.len())),
        None => Ok(body.to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_bodies() {
        let r = Http::new("http://127.0.0.1:8545", 1000).unwrap();
        assert_eq!((r.host.as_str(), r.port, r.path.as_str()), ("127.0.0.1", 8545, "/"));
        let r = Http::new("http://node.local/rpc/v1", 1000).unwrap();
        assert_eq!((r.host.as_str(), r.port, r.path.as_str()), ("node.local", 80, "/rpc/v1"));
        assert!(Rpc::new("https://example.org", 1000).is_err());
        assert_eq!(http_body(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}xx").unwrap(), b"{}");
        assert_eq!(
            http_body(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{\"\r\n3\r\na\":\r\n2\r\n1}\r\n0\r\n\r\n").unwrap(),
            b"{\"a\":1}"
        );
        assert!(http_body(b"HTTP/1.0 500 Oops\r\n\r\n").is_err());
    }

    #[test]
    fn the_request_names_the_path() {
        let r = Rpc::from_request(&json!({"rpc_url": "http://127.0.0.1:8545"}), 1000).unwrap();
        assert_eq!(r.report()["via"], "http");
        let r = Rpc::from_request(&json!({"eth": {"via": "http", "rpc_url": "http://h:1/x"}}), 1000).unwrap();
        assert_eq!(r.report()["rpc_url"], "http://h:1/x");
        let mut r = Rpc::from_request(&json!({"eth": {"via": "eth_rpc_module", "chain_id": 1}}), 1000).unwrap();
        assert_eq!(r.report(), json!({"via": "eth_rpc_module", "chain_id": 1, "calls": 0, "routes": []}));
        // Natively no client is installed: the call says so instead of reading anything.
        assert!(r.call("eth_chainId", json!([])).unwrap_err().contains("only from inside Logos Core"));
        assert!(Rpc::from_request(&json!({"eth": {"via": "eth_rpc_module"}}), 1000).is_err());
        assert!(Rpc::from_request(&json!({"eth": {"via": "web3"}}), 1000).is_err());
        assert!(Rpc::from_request(&json!({}), 1000).is_err());
    }
}
