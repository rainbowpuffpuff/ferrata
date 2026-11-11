//! A minimal Ethereum JSON-RPC client: HTTP/1.1 over `std::net`, plain `http://` only, no
//! dependency beyond `serde_json`. It is how the module reads a local node or a recorded-data
//! server (`modules/scripts/recorded_rpc.py`). Remote `https://` endpoints are refused: in Logos
//! Core those reads go through `eth_rpc_module` (proposal §B7), and nothing the module receives is
//! trusted anyway, since every proof is checked against a record before it is used.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde_json::{json, Value};

/// The largest response read (bytes): a mainnet block's receipts are a few megabytes.
const MAX_RESPONSE: u64 = 64 << 20;

/// One endpoint, and how many calls were made through it.
pub struct Rpc {
    host: String,
    port: u16,
    path: String,
    timeout: Duration,
    pub calls: u32,
}

impl Rpc {
    pub fn new(url: &str, timeout_ms: u64) -> Result<Self, String> {
        let rest = url.strip_prefix("http://").ok_or_else(|| {
            format!("rpc_url {url:?}: only http:// endpoints (a local node or a recorded-data server)")
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
            calls: 0,
        })
    }

    /// `method(params)`; the `result`, or the node's `error` as text.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.calls += 1;
        let body = json!({ "jsonrpc": "2.0", "id": self.calls, "method": method, "params": params }).to_string();
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
        let r = Rpc::new("http://127.0.0.1:8545", 1000).unwrap();
        assert_eq!((r.host.as_str(), r.port, r.path.as_str()), ("127.0.0.1", 8545, "/"));
        let r = Rpc::new("http://node.local/rpc/v1", 1000).unwrap();
        assert_eq!((r.host.as_str(), r.port, r.path.as_str()), ("node.local", 80, "/rpc/v1"));
        assert!(Rpc::new("https://example.org", 1000).is_err());
        assert_eq!(http_body(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}xx").unwrap(), b"{}");
        assert_eq!(
            http_body(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{\"\r\n3\r\na\":\r\n2\r\n1}\r\n0\r\n\r\n").unwrap(),
            b"{\"a\":1}"
        );
        assert!(http_body(b"HTTP/1.0 500 Oops\r\n\r\n").is_err());
    }
}
