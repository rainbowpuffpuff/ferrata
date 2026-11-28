//! `ferrata lez-version` against loopback stand-ins of a LEZ v0.4.0 sequencer, an earlier one and the test network's
//! gateway (src/lezver.rs): the tag this binary is built for, the network's, and the exact refusal every sending
//! command gives on a mismatch. Nothing leaves 127.0.0.1.
//!
//!   cargo test --locked --test lez_version                         (an rc1 or rc3 build)
//!   cli/v040/build.sh SRC test --test lez_version                  (the v0.4.0 build)

use std::io::{Read as _, Write as _};
use std::process::Command;

/// A loopback HTTP stand-in answering every request with `status` and `body`, in a thread; its URL.
fn stand_in(status: &'static str, body: &'static str) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/rpc", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 8192];
            let _ = s.read(&mut buf);
            let reply = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = s.write_all(reply.as_bytes());
        }
    });
    url
}

fn lez_version(url: &str) -> (i32, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_ferrata"))
        .args(["--sequencer", url, "lez-version"])
        .env_remove("FERRATA_SEQUENCER")
        .output()
        .expect("ferrata runs");
    (
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout).into(),
        String::from_utf8_lossy(&o.stderr).into(),
    )
}

const V040: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"state":"online","height":12,"isOurTurn":true}}"#;
const RC3: &str = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
const GATEWAY: &str = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"method getSequencerStatus is not served by this gateway"}}"#;

#[test]
fn the_network_and_the_build_must_match() {
    let built_040 = cfg!(feature = "lez-040");
    let built = if built_040 {
        "v0.4.0"
    } else if cfg!(feature = "lez-rc3") {
        "v0.4.0-rc3"
    } else {
        "v0.4.0-rc1"
    };
    for (status, body, network_040) in
        [("200 OK", V040, true), ("200 OK", RC3, false), ("403 Forbidden", GATEWAY, false)]
    {
        let url = stand_in(status, body);
        let (code, out, err) = lez_version(&url);
        assert!(out.contains(&format!("this ferrata: LEZ {built}")), "{out}{err}");
        if built_040 == network_040 {
            assert_eq!(code, 0, "{out}{err}");
            assert!(out.contains("match: this ferrata may sign and send there"), "{out}");
        } else {
            assert_eq!(code, 1, "{out}{err}");
            let refusal = if network_040 {
                format!(
                    "refused before anything was signed or sent: this ferrata is built for LEZ {built}, but the sequencer \
                     at {url} runs LEZ v0.4.0 (it serves getSequencerStatus); use the ferrata cli/v040/build.sh builds"
                )
            } else {
                format!(
                    "refused before anything was signed or sent: this ferrata is built for LEZ {built}, but the sequencer \
                     at {url} runs a LEZ before v0.4.0 (it does not serve getSequencerStatus, which v0.4.0 added); for \
                     the test network (LEZ v0.4.0-rc3) use the ferrata deploy/testnet/cli-rc3/build.sh builds"
                )
            };
            assert!(err.contains(&refusal), "expected the exact refusal, got: {err}");
        }
    }
}

#[test]
fn an_unknown_answer_stops_without_a_verdict() {
    let url = stand_in("502 Bad Gateway", "<html>bad gateway</html>");
    let (code, _, err) = lez_version(&url);
    assert_eq!(code, 1);
    assert!(err.contains("answered getSequencerStatus with HTTP 502 and no JSON-RPC reply"), "{err}");
}

fn deposit(sequencer: Option<&str>) -> (i32, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ferrata"));
    if let Some(url) = sequencer {
        cmd.args(["--sequencer", url]);
    }
    // The Ethereum RPC is a closed port and there is no seed: the version check must stop the deposit before either.
    let o = cmd
        .args(["--eth-rpc", "http://127.0.0.1:9", "deposit", "--denomination", "1", "--kind", "public"])
        .args(["--i-accept-public-account", "--send", "--yes"])
        .env_remove("FERRATA_SEQUENCER")
        .env_remove("FERRATA_SEED")
        .output()
        .expect("ferrata runs");
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stderr).into())
}

/// A deposit commits to a LEZ account of this build's key tree (LEZ v0.4.0 changed how a public account id is made):
/// the network's LEZ is read before the seed, the Ethereum side or a signature.
#[test]
fn a_deposit_checks_the_network_first() {
    let built_040 = cfg!(feature = "lez-040");
    let built = if built_040 {
        "v0.4.0"
    } else if cfg!(feature = "lez-rc3") {
        "v0.4.0-rc3"
    } else {
        "v0.4.0-rc1"
    };
    let other = if built_040 { stand_in("200 OK", RC3) } else { stand_in("200 OK", V040) };
    let (code, err) = deposit(Some(other.as_str()));
    assert_eq!(code, 1, "{err}");
    let refusal = if built_040 {
        format!(
            "refused before anything was signed or sent: this ferrata is built for LEZ {built}, but the sequencer at \
             {other} runs a LEZ before v0.4.0 (it does not serve getSequencerStatus, which v0.4.0 added); for the \
             test network (LEZ v0.4.0-rc3) use the ferrata deploy/testnet/cli-rc3/build.sh builds"
        )
    } else {
        format!(
            "refused before anything was signed or sent: this ferrata is built for LEZ {built}, but the sequencer at \
             {other} runs LEZ v0.4.0 (it serves getSequencerStatus); use the ferrata cli/v040/build.sh builds"
        )
    };
    assert!(err.contains(&refusal), "expected the exact refusal, got: {err}");

    // The matching network: past the check, the deposit stops at its first own need (the seed), not at the version.
    let same = if built_040 { stand_in("200 OK", V040) } else { stand_in("403 Forbidden", GATEWAY) };
    let (code, err) = deposit(Some(same.as_str()));
    assert_ne!(code, 0);
    assert!(!err.contains("refused before anything was signed or sent") && err.contains("--seed"), "{err}");

    // No sequencer: a v0.4.0 build sends no deposit; an rc build deposits as before (here: stops at the seed).
    let (code, err) = deposit(None);
    assert_ne!(code, 0);
    if built_040 {
        assert!(
            err.contains(
                "refused before anything was signed or sent: a deposit commits to a LEZ account of this ferrata's key \
                 tree, which LEZ v0.4.0 changed, so this ferrata (built for LEZ v0.4.0) reads the network's LEZ \
                 version first: pass --sequencer (or FERRATA_SEQUENCER) with the zone's sequencer RPC"
            ),
            "{err}"
        );
    } else {
        assert!(!err.contains("refused before anything was signed or sent") && err.contains("--seed"), "{err}");
    }
}
