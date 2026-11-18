//! Which host a JSON-RPC URL names, read strictly, for the guards that decide "a sequencer on this machine or not"
//! (`live.rs`, `check_loopback` and `check_send`; the updater's `chain::check_loopback`, which includes this file by
//! path). Standard library only.
//!
//! The URL is `http://` or `https://`, then an authority made of a host and an optional port, then nothing or a path,
//! query or fragment. The host is an IPv6 literal in brackets, or letters, digits, `.` and `-`; the port is digits.
//! Anything else in the authority is refused: a userinfo (`user@`, which an HTTP client sends as credentials while it
//! connects to the host after the `@`), a backslash (which WHATWG parsers read as `/`), a percent sign, whitespace or
//! a control character anywhere. So for every URL accepted here, the host is the one the client connects to (its
//! parser, `url` in jsonrpsee, agrees on such input). A loopback address is `localhost` or an IP address in
//! 127.0.0.0/8 or `::1`; other spellings of them (`127.1`, `0x7f.0.0.1`, `[::ffff:127.0.0.1]`, `localhost.`) are read
//! as names, not loopback: refused rather than guessed. A loopback port forwarded to another machine (an SSH tunnel)
//! is outside what any URL check can see.

#![allow(dead_code)] // the crates that include this file use different parts of it

use std::net::IpAddr;

/// A URL's scheme, host (lowercase; an IPv6 literal without its brackets) and port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcUrl {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
}

impl RpcUrl {
    /// `localhost`, or an address in 127.0.0.0/8 or `::1`, over plain `http` (a sequencer on this machine).
    pub fn is_loopback(&self) -> bool {
        self.scheme == "http"
            && (self.host == "localhost" || self.host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback()))
    }
}

/// Read `url` strictly (module comment); `Err` says why it is refused.
pub fn parse(url: &str) -> Result<RpcUrl, String> {
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!("{url:?}: whitespace or a control character"));
    }
    let (scheme, rest) = if let Some(r) = url.strip_prefix("http://") {
        ("http", r)
    } else if let Some(r) = url.strip_prefix("https://") {
        ("https", r)
    } else {
        return Err(format!("{url}: not an http:// or https:// URL"));
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.contains('@') {
        return Err(format!("{url}: a userinfo (`...@`) is refused: the client would connect to the host after it"));
    }
    if authority.contains(['\\', '%']) {
        return Err(format!("{url}: a backslash or a percent sign in the host"));
    }
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let close = v6.find(']').ok_or_else(|| format!("{url}: an IPv6 literal without its `]`"))?;
        let literal = &v6[..close];
        literal.parse::<std::net::Ipv6Addr>().map_err(|_| format!("{url}: {literal:?} is not an IPv6 address"))?;
        let after = &v6[close + 1..];
        let port = match after {
            "" => None,
            p if p.starts_with(':') => Some(&p[1..]),
            _ => return Err(format!("{url}: something after the IPv6 literal")),
        };
        (literal.to_ascii_lowercase(), port)
    } else {
        let mut it = authority.splitn(2, ':');
        let host = it.next().unwrap_or("");
        let port = it.next();
        if host.is_empty() || !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
            return Err(format!("{url}: the host {host:?} is not letters, digits, `.` and `-`"));
        }
        (host.to_ascii_lowercase(), port)
    };
    let port = match port {
        None => None,
        Some(p) if !p.is_empty() && p.len() <= 5 && p.chars().all(|c| c.is_ascii_digit()) => {
            Some(p.parse::<u16>().map_err(|_| format!("{url}: the port {p} is out of range"))?)
        }
        Some(p) => return Err(format!("{url}: the port {p:?} is not digits")),
    };
    Ok(RpcUrl { scheme: scheme.to_string(), host, port })
}

/// `url` names a sequencer on this machine, read strictly; false for anything refused.
pub fn is_loopback(url: &str) -> bool {
    parse(url).is_ok_and(|u| u.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_hosts() {
        for url in [
            "http://127.0.0.1:3040",
            "http://127.0.0.1:3040/",
            "http://127.0.0.1",
            "http://localhost:3040",
            "http://LOCALHOST:3040/x?y#z",
            "http://[::1]:3040",
            "http://[0:0:0:0:0:0:0:1]:3040",
            "http://127.0.0.2:3041",
            "http://127.255.255.254:1",
        ] {
            assert!(is_loopback(url), "{url} is this machine");
        }
    }

    #[test]
    fn not_loopback() {
        for url in [
            // the review's cases (review-testnet04, finding 1): the client would connect to the host after the `@`
            "http://127.0.0.1:1@203.0.113.7:18545",
            "http://localhost:1@203.0.113.7:18545",
            "http://user@127.0.0.1:3040",
            "http://127.0.0.1@example.org",
            // names that start like a loopback name, and other spellings read as names
            "http://localhost.example.org:3040",
            "http://localhost.:3040",
            "http://127.0.0.1.example.org:3040",
            "http://127.1:3040",
            "http://0x7f.0.0.1:3040",
            "http://2130706433:3040",
            "http://[::ffff:127.0.0.1]:3040",
            "http://0.0.0.0:3040",
            // tricks a WHATWG parser reads otherwise, and malformed ones
            "http://127.0.0.1\\@example.org/",
            "http://127.0.0.1%2f@example.org/",
            "http://127.0.0.1\t:3040",
            "http://127.0.0.1:3040 ",
            "http://127.0.0.1:99999",
            "http://127.0.0.1:30x0",
            "http://[::1:3040",
            "http://[::1]x:3040",
            "http://:3040",
            "https://127.0.0.1:3040",
            "ftp://127.0.0.1:3040",
            "127.0.0.1:3040",
            "http://203.0.113.7:3041",
            "https://testnet.lez.logos.co",
        ] {
            assert!(!is_loopback(url), "{url} must not count as this machine");
        }
    }

    #[test]
    fn hosts_read() {
        assert_eq!(
            parse("https://testnet.lez.logos.co").unwrap(),
            RpcUrl { scheme: "https".into(), host: "testnet.lez.logos.co".into(), port: None }
        );
        assert_eq!(parse("http://[::1]:3040/").unwrap().host, "::1");
        assert_eq!(parse("http://Example.ORG:8545/rpc?x=1#f").unwrap().port, Some(8545));
        assert!(parse("http://127.0.0.1:1@203.0.113.7:18545").unwrap_err().contains("userinfo"));
        assert!(parse("http://u:p@203.0.113.7").unwrap_err().contains("userinfo"));
    }
}
