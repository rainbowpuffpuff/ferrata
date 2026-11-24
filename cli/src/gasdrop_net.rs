//! The network's gas-drop relayer, used by default (RFP-022 U.5 and Priv.7; bridge.md §6; ADR 0022).
//!
//! A network publishes its gas-drop relayer at `<origin of the sequencer's RPC>/relayer/gas-drop` (on the test network,
//! `deploy/testnet/host/bin/ferrata-gateway`): the relayer's LEZ account (named in the receipt and in the claim), its
//! inbox (a URL the job is posted to), the gas-drop guest's image id and where the guest is served, the native LEZ per
//! drop and what it pays for. `ferrata claim` reads it unless the user names an inbox (`--gas-drop-inbox`) or opts out
//! (`--no-gas-drop`), so a private claim's fee is paid from that relayer's sealed credit with no setting, and a later
//! private burn's fee from what the credit left in the one-time account (`ferrata burn`).
//!
//! What the tool does not take from the network: which program it proves. The published image id must be one this
//! build pins ([`PINNED_GUESTS`], docs/image-ids.md), the downloaded guest must hash to it, and the receipt must verify
//! under it with exactly the journal the tool computes natively before the job leaves the machine ([`check_receipt`]):
//! a guest served by a dishonest network could otherwise write the claim's witness into what the relayer is handed.
//!
//! Absent (an origin that answers 4xx, or not over HTTP: a local sequencer), the claim pays its fee from the account
//! that proves and its dry run says what that reveals; unreachable or malformed (5xx, a transport error, a document
//! that does not parse), the claim stops: the weaker choice is never taken without the user's `--no-gas-drop`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use alloy::transports::http::reqwest;
use anyhow::{anyhow, bail, ensure, Context as _, Result};
use serde_json::Value;

use crate::{h32, hx};

/// Where a network publishes its gas-drop relayer, relative to the origin of its sequencer's RPC.
pub const INFO_PATH: &str = "/relayer/gas-drop";
/// The largest guest the tool downloads.
const MAX_GUEST: usize = 4 << 20;

/// Gas-drop guests this tool proves with (docs/image-ids.md: `gasdrop`, RISC Zero 3.0.5; `gasdrop_31`, 3.1). A build
/// may pin more at compile time with `FERRATA_GASDROP_PINS` (comma-separated 32-byte hex ids: a local build of the
/// guest whose id differs, as e2e/seqnet's).
pub const PINNED_GUESTS: [&str; 2] = [
    "0x895804161df14b00e1cb02e5d27fe136efb82b178db511c7697c030676fc9eb4",
    "0xce3ad533d2c72b38c9779f1c692dd784e3fd754612808696e1362da49c7f8125",
];

/// Whether this build of the tool proves with the guest `id`.
pub fn pinned(id: &[u8; 32]) -> bool {
    let h = hx(id);
    let extra = option_env!("FERRATA_GASDROP_PINS").unwrap_or("");
    PINNED_GUESTS.iter().copied().chain(extra.split(',').map(str::trim)).any(|p| !p.is_empty() && p.eq_ignore_ascii_case(&h))
}

/// Where a gas-drop job goes: a directory (a relayer on this machine) or a URL it is posted to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inbox {
    Dir(PathBuf),
    Url(String),
}

impl Inbox {
    /// `http(s)://…` is a URL; anything else a directory.
    pub fn parse(s: &str) -> Self {
        if s.starts_with("https://") || s.starts_with("http://") {
            Inbox::Url(s.to_string())
        } else {
            Inbox::Dir(PathBuf::from(s))
        }
    }
}

impl std::fmt::Display for Inbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Inbox::Dir(p) => write!(f, "{}", p.display()),
            Inbox::Url(u) => f.write_str(u),
        }
    }
}

/// A network's gas-drop relayer, as it publishes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Published {
    /// The URL it was read from.
    pub source: String,
    /// The relayer's LEZ account: named in the receipt and in the claim (paid the relay fee).
    pub relayer: [u8; 32],
    /// The URL the job is posted to.
    pub inbox: String,
    /// The gas-drop guest the relayer verifies receipts against.
    pub image_id: [u8; 32],
    /// Where that guest is served.
    pub guest: String,
    /// Native LEZ per drop (one per deposit).
    pub amount: u128,
    /// What a drop is meant to pay for (`claim`, `burn`).
    pub covers: Vec<String>,
}

impl Published {
    /// One line for a dry run.
    pub fn describe(&self) -> String {
        format!(
            "relayer {} (published at {}; inbox {}; {} native LEZ per deposit, for: {})",
            hx(&self.relayer),
            self.source,
            self.inbox,
            self.amount,
            if self.covers.is_empty() { "claim".to_string() } else { self.covers.join(", ") }
        )
    }
}

/// The URL a network behind `sequencer` publishes its gas-drop relayer at; `None` unless `sequencer` is http(s).
pub fn info_url(sequencer: &str) -> Option<String> {
    let u = url::Url::parse(sequencer).ok()?;
    if !matches!(u.scheme(), "http" | "https") || u.host_str().is_none() {
        return None;
    }
    u.join(INFO_PATH).ok().map(String::from)
}

fn http_url(base: &str, s: &str, what: &str) -> Result<String> {
    let u = url::Url::parse(base)?.join(s).with_context(|| format!("the published {what} {s:?}"))?;
    ensure!(matches!(u.scheme(), "http" | "https"), "the published {what} {u} is not http(s)");
    Ok(u.into())
}

/// The published document at `source` (`relayer`, `inbox`, `imageId`, `guest`, `amount`, `covers`); relative URLs
/// are read against `source`.
pub fn parse(source: &str, body: &[u8]) -> Result<Published> {
    let v: Value = serde_json::from_slice(body).context("not JSON")?;
    let s = |k: &str| v.get(k).and_then(Value::as_str).ok_or_else(|| anyhow!("no {k:?}"));
    let amount = match v.get("amount") {
        Some(Value::String(a)) => a.parse::<u128>().context("\"amount\"")?,
        Some(Value::Number(n)) => n.as_u64().ok_or_else(|| anyhow!("\"amount\" is not a whole number"))?.into(),
        _ => bail!("no \"amount\""),
    };
    ensure!(amount > 0, "\"amount\" is 0");
    let covers = match v.get("covers") {
        None => vec!["claim".to_string()],
        Some(Value::Array(a)) => {
            a.iter().map(|x| x.as_str().map(String::from).ok_or_else(|| anyhow!("\"covers\" holds a non-string"))).collect::<Result<_>>()?
        }
        Some(_) => bail!("\"covers\" is not a list"),
    };
    Ok(Published {
        source: source.to_string(),
        relayer: h32(s("relayer")?).context("\"relayer\"")?,
        inbox: http_url(source, s("inbox")?, "inbox")?,
        image_id: h32(s("imageId")?).context("\"imageId\"")?,
        guest: http_url(source, s("guest")?, "guest")?,
        amount,
        covers,
    })
}

fn client(timeout: Duration) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder().timeout(timeout).build()?)
}

/// The gas-drop relayer the network behind `sequencer` publishes: `None` when it publishes none (an answer 4xx, or a
/// sequencer not reached over HTTP); an error when it cannot be read (5xx, transport, a document that does not parse).
pub async fn discover(sequencer: &str) -> Result<Option<Published>> {
    let Some(url) = info_url(sequencer) else { return Ok(None) };
    let r = client(Duration::from_secs(20))?.get(&url).send().await.with_context(|| format!("GET {url}"))?;
    let status = r.status();
    if status.is_client_error() {
        return Ok(None);
    }
    let body = r.bytes().await.with_context(|| format!("GET {url}"))?;
    ensure!(status.is_success(), "GET {url}: {status}: {}", String::from_utf8_lossy(&body[..body.len().min(300)]));
    parse(&url, &body).map(Some).with_context(|| format!("the gas-drop relayer published at {url}"))
}

/// The image id of a guest ELF.
pub fn image_id_of(elf: &[u8]) -> Result<[u8; 32]> {
    let d = risc0_zkvm::compute_image_id(elf).map_err(|e| anyhow!("not a RISC Zero guest: {e}"))?;
    Ok(d.as_bytes().try_into().expect("32 bytes"))
}

/// The published guest, checked: its id pinned by this tool ([`pinned`]) and the bytes hashing to it; downloaded once
/// into `cache` (`gasdrop-<id>.bin`).
pub async fn fetch_guest(p: &Published, cache: &Path) -> Result<PathBuf> {
    ensure!(
        pinned(&p.image_id),
        "the network's gas-drop relayer verifies receipts of guest {}, which this tool does not prove with (it pins {}): \
         refusing to prove with a guest the network chose; use a tool that pins it, give the guest yourself \
         (--gas-drop-elf), or pay the LEZ fee from your own private account (--no-gas-drop)",
        hx(&p.image_id),
        PINNED_GUESTS.join(", ")
    );
    let path = cache.join(format!("gasdrop-{}.bin", &hx(&p.image_id)[2..18]));
    if let Ok(b) = std::fs::read(&path) {
        if image_id_of(&b).ok() == Some(p.image_id) {
            return Ok(path);
        }
    }
    let r = client(Duration::from_secs(60))?.get(&p.guest).send().await.with_context(|| format!("GET {}", p.guest))?;
    ensure!(r.status().is_success(), "GET {}: {}", p.guest, r.status());
    let elf = r.bytes().await?;
    ensure!(elf.len() <= MAX_GUEST, "the guest at {} is {} bytes (at most {MAX_GUEST})", p.guest, elf.len());
    let id = image_id_of(&elf)?;
    ensure!(id == p.image_id, "the guest served at {} is {}, not the published {}", p.guest, hx(&id), hx(&p.image_id));
    std::fs::create_dir_all(cache)?;
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, &elf)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Before a receipt leaves the machine: it verifies under `image_id` (exit code 0) and its journal is exactly
/// `journal`, the one the tool computed natively (nothing else of the witness can ride along).
pub fn check_receipt(receipt: &risc0_zkvm::Receipt, image_id: &[u8; 32], journal: &[u8]) -> Result<()> {
    ensure!(
        receipt.journal.bytes == journal,
        "the gas-drop receipt's journal ({} bytes) is not the one computed natively ({} bytes): not handing it over",
        receipt.journal.bytes.len(),
        journal.len()
    );
    receipt
        .verify(risc0_zkvm::sha::Digest::from_bytes(*image_id))
        .map_err(|e| anyhow!("the gas-drop receipt does not verify under {}: {e}", hx(image_id)))
}

/// Posts a job to a relayer's inbox URL. An answer 409 (the inbox holds this deposit's job already) counts as
/// handed over.
pub async fn post_job(url: &str, job: &Value) -> Result<String> {
    let body = serde_json::to_vec(job)?;
    let r = client(Duration::from_secs(120))?
        .post(url)
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;
    let status = r.status();
    let text = r.text().await.unwrap_or_default();
    let short = &text[..text.len().min(400)];
    if status.as_u16() == 409 {
        return Ok(format!("{url} (it holds this deposit's job already: {short})"));
    }
    ensure!(status.is_success(), "the relayer's inbox {url} refused the job: {status}: {short}");
    let job = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["job"].as_str().map(String::from));
    Ok(match job {
        Some(j) => format!("{url} (job {j})"),
        None => url.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0x895804161df14b00e1cb02e5d27fe136efb82b178db511c7697c030676fc9eb4";
    const RL: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

    fn doc(extra: &str) -> String {
        format!(
            r#"{{"relayer":"{RL}","inbox":"/relayer/gas-drop","imageId":"{ID}","guest":"/relayer/gas-drop/guest","amount":"200000000"{extra}}}"#
        )
    }

    #[test]
    fn info_url_is_the_sequencers_origin() {
        assert_eq!(info_url("https://ferrata.example/rpc").as_deref(), Some("https://ferrata.example/relayer/gas-drop"));
        assert_eq!(info_url("https://rpc.example").as_deref(), Some("https://rpc.example/relayer/gas-drop"));
        assert_eq!(info_url("http://127.0.0.1:8090/rpc").as_deref(), Some("http://127.0.0.1:8090/relayer/gas-drop"));
        assert_eq!(info_url("ws://x/rpc"), None);
        assert_eq!(info_url("not a url"), None);
    }

    #[test]
    fn parses_a_published_relayer() {
        let p = parse("https://n.example/relayer/gas-drop", doc(r#","covers":["claim","burn"]"#).as_bytes()).unwrap();
        assert_eq!(p.inbox, "https://n.example/relayer/gas-drop");
        assert_eq!(p.guest, "https://n.example/relayer/gas-drop/guest");
        assert_eq!(p.amount, 200_000_000);
        assert_eq!(p.covers, ["claim", "burn"]);
        assert_eq!(hx(&p.relayer), RL);
        assert!(p.describe().contains("for: claim, burn"));
        // Without "covers": a claim's fee only.
        assert_eq!(parse("https://n.example/x", doc("").as_bytes()).unwrap().covers, ["claim"]);
    }

    #[test]
    fn refuses_malformed_documents() {
        let base = "https://n.example/relayer/gas-drop";
        assert!(parse(base, b"<html>").is_err());
        assert!(parse(base, doc("").replace(RL, "0x12").as_bytes()).is_err());
        assert!(parse(base, doc("").replace("200000000", "0").as_bytes()).is_err());
        assert!(parse(base, doc("").replace("/relayer/gas-drop\"", "ftp://x/inbox\"").as_bytes()).is_err());
        assert!(parse(base, doc(r#","covers":"burn""#).as_bytes()).is_err());
    }

    #[test]
    fn pins_the_recorded_guests_only() {
        assert!(pinned(&h32(PINNED_GUESTS[0]).unwrap()));
        assert!(pinned(&h32(PINNED_GUESTS[1]).unwrap()));
        assert!(!pinned(&[7u8; 32]) || option_env!("FERRATA_GASDROP_PINS").is_some_and(|p| p.contains("0707")));
    }

    #[test]
    fn inbox_kinds() {
        assert_eq!(Inbox::parse("https://n.example/relayer/gas-drop"), Inbox::Url("https://n.example/relayer/gas-drop".into()));
        assert_eq!(Inbox::parse("relayer-inbox"), Inbox::Dir("relayer-inbox".into()));
    }

    /// One HTTP answer from a local listener, for discover() and post_job().
    async fn serve_once(status: &'static str, body: &'static str) -> (String, tokio::task::JoinHandle<String>) {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let h = tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            // The request whole: its head, then Content-Length bytes of body.
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let k = s.read(&mut chunk).await.unwrap();
                buf.extend_from_slice(&chunk[..k]);
                let text = String::from_utf8_lossy(&buf).to_lowercase();
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                        .unwrap_or(0);
                    if buf.len() >= end + 4 + len {
                        break;
                    }
                }
                if k == 0 {
                    break;
                }
            }
            let n = buf.len();
            let reply = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            s.write_all(reply.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&buf[..n]).into_owned()
        });
        (format!("http://127.0.0.1:{port}/rpc"), h)
    }

    #[tokio::test]
    async fn discover_none_error_and_found() {
        let (seq, h) = serve_once("404 Not Found", "{}").await;
        assert_eq!(discover(&seq).await.unwrap(), None);
        assert!(h.await.unwrap().starts_with("GET /relayer/gas-drop "));
        let (seq, _) = serve_once("405 Method Not Allowed", "").await;
        assert_eq!(discover(&seq).await.unwrap(), None);
        let (seq, _) = serve_once("503 Service Unavailable", r#"{"error":"the relayer is paused"}"#).await;
        assert!(format!("{:#}", discover(&seq).await.unwrap_err()).contains("paused"));
        let (seq, _) = serve_once("200 OK", "not json").await;
        assert!(discover(&seq).await.is_err());
        let body: &'static str = Box::leak(doc("").into_boxed_str());
        let (seq, _) = serve_once("200 OK", body).await;
        let p = discover(&seq).await.unwrap().unwrap();
        assert!(p.inbox.ends_with("/relayer/gas-drop") && p.inbox.starts_with("http://127.0.0.1:"));
    }

    #[tokio::test]
    async fn post_job_outcomes() {
        let job = serde_json::json!({"kind": "gas-drop"});
        let (url, h) = serve_once("202 Accepted", r#"{"job":"gas-drop-0011.json"}"#).await;
        assert!(post_job(&url, &job).await.unwrap().contains("gas-drop-0011.json"));
        let req = h.await.unwrap();
        assert!(req.starts_with("POST /rpc ") && req.contains("\"kind\":\"gas-drop\""));
        let (url, _) = serve_once("409 Conflict", r#"{"error":"held"}"#).await;
        assert!(post_job(&url, &job).await.unwrap().contains("holds this deposit's job already"));
        let (url, _) = serve_once("429 Too Many Requests", r#"{"error":"per-IP limit"}"#).await;
        assert!(format!("{:#}", post_job(&url, &job).await.unwrap_err()).contains("per-IP limit"));
    }
}
