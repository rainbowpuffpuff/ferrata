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
//! Only an answer 404 or 410 means "this network publishes no relayer" (the claim then pays its fee from the account
//! that proves, as without this module, and its dry run says what that reveals). Every other outcome stops the claim
//! (and a burn that has no drop left, [`burn_without_remainder`]) with a message naming `--no-gas-drop`: 429 (with its
//! Retry-After), any other 4xx, 5xx, a timeout, a transport error, a document that does not parse, a relayer that is
//! paused or not accepting jobs ([`claim_choice`]). The terms are read over https, or over http from this machine only
//! (a plain-http origin elsewhere could be answered by anyone on the path, who would then name the relayer the claim
//! pays its relay fee to, or answer 404); the published inbox and guest must be https or the terms' own origin.

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
    /// Whether its inbox takes jobs now (the gateway's `accepting`; absent: yes). Read before the receipt is proven.
    pub accepting: bool,
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

/// Whether `url` is https, or http to this machine (a loopback host).
pub fn secure_origin(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https"
            || (u.scheme() == "http" && matches!(u.host_str(), Some("127.0.0.1" | "localhost" | "[::1]" | "::1")))
    })
}

/// A published URL read against `base`: https, or on `base`'s own origin.
fn http_url(base: &str, s: &str, what: &str) -> Result<String> {
    let b = url::Url::parse(base)?;
    let u = b.join(s).with_context(|| format!("the published {what} {s:?}"))?;
    ensure!(matches!(u.scheme(), "http" | "https"), "the published {what} {u} is not http(s)");
    ensure!(
        u.scheme() == "https" || u.origin() == b.origin(),
        "the published {what} {u} is plain http on another origin than the terms ({base})"
    );
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
        accepting: v.get("accepting").and_then(Value::as_bool).unwrap_or(true),
    })
}

fn client(timeout: Duration) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder().timeout(timeout).build()?)
}

/// The gas-drop relayer the network behind `sequencer` publishes: `None` only for an answer 404 or 410 (or a sequencer
/// URL that is not http(s)); an error for anything else ([`discover_with`], 20 s).
pub async fn discover(sequencer: &str) -> Result<Option<Published>> {
    discover_with(sequencer, Duration::from_secs(20)).await
}

/// [`discover`] with its timeout.
pub async fn discover_with(sequencer: &str, timeout: Duration) -> Result<Option<Published>> {
    let Some(url) = info_url(sequencer) else { return Ok(None) };
    ensure!(
        secure_origin(&url),
        "{url} is plain http to another machine: anyone on the path could answer it, naming the relayer the claim pays its relay fee to or hiding it; use the sequencer's https URL, or name a relayer's inbox yourself (--gas-drop-inbox and --relayer)"
    );
    let r = client(timeout)?.get(&url).send().await.with_context(|| format!("GET {url}"))?;
    let status = r.status();
    if matches!(status.as_u16(), 404 | 410) {
        return Ok(None);
    }
    let retry = r.headers().get("retry-after").and_then(|v| v.to_str().ok()).map(String::from);
    let body = r.bytes().await.with_context(|| format!("GET {url}"))?;
    let text = String::from_utf8_lossy(&body[..body.len().min(300)]).into_owned();
    if status.as_u16() == 429 {
        bail!("GET {url}: 429 Too Many Requests{}: {text}", retry.map(|r| format!(" (retry after {r} s)")).unwrap_or_default());
    }
    ensure!(status.is_success(), "GET {url}: {status}: {text}");
    parse(&url, &body).map(Some).with_context(|| format!("the gas-drop relayer published at {url}"))
}

/// The opt-out every refusal of this module names.
pub const OPT_OUT: &str = "or pay the LEZ fee from your own private account (--no-gas-drop; the dry run's fee payer line says what that reveals)";

/// A claim's fee payer from what [`discover`] found at `info`: the network's relayer if it publishes one that takes
/// jobs; `None` (the account that proves pays) only when the network publishes none (404, 410); an error, naming
/// `--no-gas-drop`, for everything else: the claim stops instead of taking the weaker fee payer.
pub fn claim_choice(found: Result<Option<Published>>, info: &str) -> Result<Option<Published>> {
    match found {
        Ok(Some(p)) if !p.accepting => {
            bail!("the network's gas-drop relayer {} takes no jobs now (its inbox is full): try again later, {OPT_OUT}", hx(&p.relayer))
        }
        Ok(found) => Ok(found),
        Err(e) => Err(e.context(format!("the network's gas-drop relayer ({info}) could not be read: try again, {OPT_OUT}"))),
    }
}

/// A private burn with no drop of this wallet holding native LEZ: its own private account pays only if the network
/// publishes no relayer (404, 410: as without a gas drop). With a relayer published, a drop would have been the default
/// (a drop pays the claim and one burn), so paying from the user's account needs `--no-gas-drop`; a network that cannot
/// be read stops the burn.
pub fn burn_without_remainder(found: Result<Option<Published>>, info: &str) -> Result<()> {
    match found {
        Ok(None) => Ok(()),
        Ok(Some(p)) => bail!(
            "no gas drop of this wallet holds native LEZ for this burn's fee (a drop pays a claim and one burn) and the network publishes relayer {}: paying from your own private account reveals where its native LEZ came from; add --no-gas-drop to confirm that",
            hx(&p.relayer)
        ),
        Err(e) => Err(e.context(format!("the network's gas-drop relayer ({info}) could not be read: try again, {OPT_OUT}"))),
    }
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
    ensure!(
        status.is_success(),
        "the relayer's inbox {url} refused the job: {status}: {short}; nothing was paid: try again later (the same command reuses its one-time account), name another relayer (--relayer, --gas-drop-inbox), {OPT_OUT}"
    );
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
    fn only_https_or_this_machine_is_read() {
        assert!(secure_origin("https://ferrata.example/relayer/gas-drop"));
        assert!(secure_origin("http://127.0.0.1:8090/relayer/gas-drop") && secure_origin("http://localhost/x"));
        assert!(!secure_origin("http://ferrata.example/relayer/gas-drop") && !secure_origin("http://10.0.0.5:3040/x"));
    }

    #[tokio::test]
    async fn plain_http_elsewhere_stops_before_any_request() {
        let e = format!("{:#}", claim_choice(discover("http://192.0.2.1:3040/rpc").await, "i").unwrap_err());
        assert!(e.contains("plain http") && e.contains("--no-gas-drop"), "{e}");
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
        // Without "covers": a claim's fee only; without "accepting": accepting.
        let p = parse("https://n.example/x", doc("").as_bytes()).unwrap();
        assert_eq!(p.covers, ["claim"]);
        assert!(p.accepting);
        assert!(!parse("https://n.example/x", doc(r#","accepting":false"#).as_bytes()).unwrap().accepting);
        // An https inbox elsewhere is accepted; plain http elsewhere is not; plain http on the terms' own origin is.
        let abs = doc("").replace("\"/relayer/gas-drop\"", "\"https://relay.example/in\"");
        assert_eq!(parse("https://n.example/x", abs.as_bytes()).unwrap().inbox, "https://relay.example/in");
        let plain = doc("").replace("\"/relayer/gas-drop\"", "\"http://relay.example/in\"");
        assert!(parse("https://n.example/x", plain.as_bytes()).is_err());
        assert!(parse("http://127.0.0.1:9/relayer/gas-drop", doc("").as_bytes()).is_ok());
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
        serve_once_with(status, "", body).await
    }

    /// [`serve_once`] with extra header lines (each ending in \r\n).
    async fn serve_once_with(
        status: &'static str,
        headers: &'static str,
        body: &'static str,
    ) -> (String, tokio::task::JoinHandle<String>) {
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
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            s.write_all(reply.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&buf[..n]).into_owned()
        });
        (format!("http://127.0.0.1:{port}/rpc"), h)
    }

    #[tokio::test]
    async fn only_404_and_410_mean_no_relayer() {
        for status in ["404 Not Found", "410 Gone"] {
            let (seq, h) = serve_once(status, "{}").await;
            assert_eq!(claim_choice(discover(&seq).await, "i").unwrap(), None, "{status}: no relayer published");
            assert!(h.await.unwrap().starts_with("GET /relayer/gas-drop "));
        }
        let body: &'static str = Box::leak(doc("").into_boxed_str());
        let (seq, _) = serve_once("200 OK", body).await;
        let p = claim_choice(discover(&seq).await, "i").unwrap().unwrap();
        assert!(p.inbox.ends_with("/relayer/gas-drop") && p.inbox.starts_with("http://127.0.0.1:"));
    }

    /// Every other answer stops the claim, naming the opt-out: never the user's own account silently (review F1).
    #[tokio::test]
    async fn every_other_answer_stops_the_claim_and_the_burn() {
        let cases: [(&'static str, &'static str, &'static str, &str); 9] = [
            ("429 Too Many Requests", "Retry-After: 10\r\n", r#"{"error":"too many gas-drop reads"}"#, "retry after 10 s"),
            ("403 Forbidden", "", "{}", "403"),
            ("405 Method Not Allowed", "", "", "405"),
            ("400 Bad Request", "", "{}", "400"),
            ("500 Internal Server Error", "", "{}", "500"),
            ("502 Bad Gateway", "", "", "502"),
            ("503 Service Unavailable", "Retry-After: 600\r\n", r#"{"error":"the gas-drop relayer is paused: refilling"}"#, "paused"),
            ("200 OK", "", "not json", "not JSON"),
            ("200 OK", "", r#"{"relayer":"0x12"}"#, "relayer"),
        ];
        for (status, headers, body, says) in cases {
            let (seq, _) = serve_once_with(status, headers, body).await;
            let e = format!("{:#}", claim_choice(discover(&seq).await, "the terms").unwrap_err());
            assert!(e.contains(says) && e.contains("--no-gas-drop"), "{status}: {e}");
            let (seq, _) = serve_once_with(status, headers, body).await;
            let e = format!("{:#}", burn_without_remainder(discover(&seq).await, "the terms").unwrap_err());
            assert!(e.contains("--no-gas-drop"), "burn, {status}: {e}");
        }
        // A relayer that takes no jobs: stops before the receipt is proven.
        let full: &'static str = Box::leak(doc(r#","accepting":false"#).into_boxed_str());
        let (seq, _) = serve_once("200 OK", full).await;
        let e = format!("{:#}", claim_choice(discover(&seq).await, "i").unwrap_err());
        assert!(e.contains("takes no jobs") && e.contains("--no-gas-drop"), "{e}");
    }

    #[tokio::test]
    async fn a_timeout_or_no_listener_stops_the_claim() {
        // A listener that accepts and never answers: the timeout.
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let hold = tokio::spawn(async move {
            let (s, _) = l.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(30)).await;
            drop(s);
        });
        let seq = format!("http://127.0.0.1:{port}/rpc");
        let e = format!("{:#}", claim_choice(discover_with(&seq, Duration::from_secs(1)).await, "i").unwrap_err());
        assert!(e.contains("--no-gas-drop"), "{e}");
        hold.abort();
        // Nothing listening: connection refused.
        let free = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = free.local_addr().unwrap().port();
        drop(free);
        let seq = format!("http://127.0.0.1:{port}/rpc");
        assert!(claim_choice(discover(&seq).await, "i").is_err());
        assert!(burn_without_remainder(discover(&seq).await, "i").is_err());
    }

    #[test]
    fn a_burn_without_a_drop_left() {
        assert!(burn_without_remainder(Ok(None), "i").is_ok(), "no relayer published: the own account, as without drops");
        let p = parse("https://n.example/x", doc("").as_bytes()).unwrap();
        let e = format!("{:#}", burn_without_remainder(Ok(Some(p)), "i").unwrap_err());
        assert!(e.contains("add --no-gas-drop to confirm"), "{e}");
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
        let e = format!("{:#}", post_job(&url, &job).await.unwrap_err());
        assert!(e.contains("per-IP limit") && e.contains("--no-gas-drop"), "{e}");
    }
}
