//! Command-line wallet and verifier for voluntary bridge-activity disclosure.
//!
//! ```text
//! ferrata-disclose make --bridge-seed-file SEED --destinations DESTINATIONS.json --view VIEW.json --out DISCLOSURE.json
//! ferrata-disclose verify --disclosure DISCLOSURE.json --view VIEW.json
//! ```
//!
//! `SEED` is a file holding the 32-byte bridge seed as 64 hexadecimal digits (a trailing newline is allowed): the seed
//! is the user's secret, so it is never an argument (process listings and shell history show arguments). All hashes and
//! `uint256` amounts in the JSON files are 64 lowercase hexadecimal digits without `0x`; addresses are 40 digits and
//! burn MACs are 32. `DISCLOSURE.json` is the library's ordinary `Disclosure` JSON document.
//!
//! `VIEW.json`, public chain data only (the third party's input as much as the wallet's), has this shape (arrays may be
//! empty; an unknown field is refused):
//!
//! ```json
//! {
//!   "chainId": "0000000000000000000000000000000000000000000000000000000000aa36a7",
//!   "vault": "0000000000000000000000000000000000000000",
//!   "configId": "0000000000000000000000000000000000000000000000000000000000000000",
//!   "deposits": [{
//!     "leafIndex": 0,
//!     "j": 0,
//!     "depositor": "0000000000000000000000000000000000000000",
//!     "token": "0000000000000000000000000000000000000000",
//!     "amount": "0000000000000000000000000000000000000000000000000000000000000001",
//!     "commitment": "0000000000000000000000000000000000000000000000000000000000000000"
//!   }],
//!   "spentNullifiers": ["0000000000000000000000000000000000000000000000000000000000000000"],
//!   "burns": [{
//!     "burnIndex": 0,
//!     "r": "0000000000000000000000000000000000000000000000000000000000000000",
//!     "memo": "0000000000000000000000000000000000000000",
//!     "mac": "00000000000000000000000000000000",
//!     "token": "0000000000000000000000000000000000000000",
//!     "amount": "0000000000000000000000000000000000000000000000000000000000000001",
//!     "cOut": "0000000000000000000000000000000000000000000000000000000000000000"
//!   }]
//! }
//! ```
//!
//! Deposits use the vault event's depositor, per-depositor counter `j`, leaf index, token, amount
//! and commitment. `spentNullifiers` is the LEZ claim set. Burn notes have their published fields
//! and burn-tree index.
//!
//! `DESTINATIONS.json`, the wallet's own (read by `make` only), lists the LEZ accounts the wallet's seed derives, the
//! candidates the library's recovery scan matches deposits against: `{"destinations": [{"kind": 1, "account": 0,
//! "dest": "<64 hex digits>"}]}`, kind 1 private and 2 public, `account` the candidate counter. The scan gap is one more
//! than the largest account number. `verify` reads only `VIEW.json` and the disclosure, and prints the report as JSON.
//! Library refusals are JSON errors on stdout and exit 1. Bad arguments, files or JSON exit 2.

#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use ferrata_bridge_core::inbound::DestKind;
use ferrata_bridge_core::outbound::{BurnNote, PublishedBurn};
use ferrata_bridge_core::{Address, Hash, U256};
use ferrata_disclose::{
    make_disclosure, verify, Disclosure, DisclosureError, PublicDeposit, PublicView, Report, WalletKeys,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::json;

const USAGE: &str = "usage:\n  ferrata-disclose make --bridge-seed-file SEED --destinations DESTINATIONS.json --view VIEW.json --out DISCLOSURE.json\n  ferrata-disclose verify --disclosure DISCLOSURE.json --view VIEW.json";

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("ferrata-disclose: {message}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run(args: Vec<String>) -> Result<u8, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err("missing command".into());
    };
    match command {
        "make" => {
            let options = options(&args[1..], &["--bridge-seed-file", "--destinations", "--view", "--out"])?;
            let seed = read_seed(path(required(&options, "--bridge-seed-file")?))?;
            let destinations: DestinationsFile = read_json(path(required(&options, "--destinations")?))?;
            let view = read_json(path(required(&options, "--view")?))?;
            let out = path(required(&options, "--out")?);
            let public = into_public_view(view);
            let candidates = destination_map(destinations.destinations)?;
            let gap = candidates.keys().map(|(_, account)| *account).max().map_or(Ok(0), |account| {
                account.checked_add(1).ok_or_else(|| "destination account u32::MAX cannot be scanned".to_string())
            })?;
            let disclosure = make_disclosure(&WalletKeys::from_bridge_seed(&seed), &public, gap, |kind, account| {
                candidates.get(&(kind as u8, account)).copied().unwrap_or([0; 32])
            });
            write_json(out, &disclosure)?;
            Ok(0)
        }
        "verify" => {
            let options = options(&args[1..], &["--disclosure", "--view"])?;
            let disclosure: Disclosure = read_json(path(required(&options, "--disclosure")?))?;
            let view = read_json(path(required(&options, "--view")?))?;
            let public = into_public_view(view);
            match verify(&disclosure, &public) {
                Ok(report) => {
                    print_json(&report_json(&report))?;
                    Ok(0)
                }
                Err(error) => {
                    print_json(&error_json(&error))?;
                    Ok(1)
                }
            }
        }
        _ => Err(format!("unknown command {command:?}")),
    }
}

fn path(text: &str) -> &Path {
    Path::new(text)
}

fn required<'a>(options: &'a HashMap<&str, &str>, name: &str) -> Result<&'a str, String> {
    options.get(name).copied().ok_or_else(|| format!("missing {name}"))
}

fn options<'a>(args: &'a [String], allowed: &[&str]) -> Result<HashMap<&'a str, &'a str>, String> {
    if !args.len().is_multiple_of(2) {
        return Err("every option needs a value".into());
    }
    let mut options = HashMap::new();
    for pair in args.chunks_exact(2) {
        let name = pair[0].as_str();
        if !allowed.contains(&name) {
            return Err(format!("unknown option {name:?}"));
        }
        if options.insert(name, pair[1].as_str()).is_some() {
            return Err(format!("duplicate option {name}"));
        }
    }
    Ok(options)
}

/// The bridge seed from its file: 64 hexadecimal digits, surrounding whitespace allowed.
fn read_seed(path: &Path) -> Result<[u8; 32], String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    decode::<32>(text.trim()).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: invalid JSON: {e}", path.display()))
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    let mut file = fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn print_json(value: &serde_json::Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    println!("{text}");
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ViewFile {
    #[serde(deserialize_with = "de32")]
    chain_id: U256,
    #[serde(deserialize_with = "de20")]
    vault: Address,
    #[serde(deserialize_with = "de32")]
    config_id: Hash,
    deposits: Vec<DepositFile>,
    spent_nullifiers: HashSet<HashHex>,
    burns: Vec<BurnFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DestinationsFile {
    destinations: Vec<DestinationFile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DepositFile {
    leaf_index: u64,
    j: u64,
    #[serde(deserialize_with = "de20")]
    depositor: Address,
    #[serde(deserialize_with = "de20")]
    token: Address,
    #[serde(deserialize_with = "de32")]
    amount: U256,
    #[serde(deserialize_with = "de32")]
    commitment: Hash,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BurnFile {
    burn_index: u64,
    #[serde(deserialize_with = "de32")]
    r: Hash,
    #[serde(deserialize_with = "de20")]
    memo: Address,
    #[serde(deserialize_with = "de16")]
    mac: [u8; 16],
    #[serde(deserialize_with = "de20")]
    token: Address,
    #[serde(deserialize_with = "de32")]
    amount: U256,
    #[serde(deserialize_with = "de32")]
    c_out: Hash,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DestinationFile {
    kind: u8,
    account: u32,
    #[serde(deserialize_with = "de32")]
    dest: Hash,
}

#[derive(Hash, PartialEq, Eq)]
struct HashHex(Hash);

impl<'de> Deserialize<'de> for HashHex {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        de32(deserializer).map(Self)
    }
}

fn into_public_view(view: ViewFile) -> PublicView {
    let deposits = view
        .deposits
        .into_iter()
        .map(|d| PublicDeposit {
            leaf_index: d.leaf_index,
            j: d.j,
            depositor: d.depositor,
            token: d.token,
            amount: d.amount,
            commitment: d.commitment,
        })
        .collect();
    let burns = view
        .burns
        .into_iter()
        .map(|b| PublishedBurn {
            burn_index: b.burn_index,
            note: BurnNote { r: b.r, memo: b.memo, mac: b.mac, token: b.token, amount: b.amount, c_out: b.c_out },
        })
        .collect();
    PublicView {
        chain_id: view.chain_id,
        vault: view.vault,
        config_id: view.config_id,
        deposits,
        spent_nullifiers: view.spent_nullifiers.into_iter().map(|h| h.0).collect(),
        burns,
    }
}

fn destination_map(destinations: Vec<DestinationFile>) -> Result<HashMap<(u8, u32), Hash>, String> {
    let mut map = HashMap::new();
    for destination in destinations {
        DestKind::from_byte(destination.kind)
            .ok_or_else(|| format!("invalid destination kind {}", destination.kind))?;
        if map.insert((destination.kind, destination.account), destination.dest).is_some() {
            return Err(format!("duplicate destination kind {} account {}", destination.kind, destination.account));
        }
    }
    Ok(map)
}

fn report_json(report: &Report) -> serde_json::Value {
    json!({
        "chainId": hex::encode(report.chain_id),
        "vault": hex::encode(report.vault),
        "deposits": report.deposits.iter().map(|d| json!({
            "leafIndex": d.leaf_index,
            "depositor": hex::encode(d.depositor),
            "token": hex::encode(d.token),
            "amount": hex::encode(d.amount),
            "commitment": hex::encode(d.commitment),
            "kind": d.kind as u8,
            "dest": hex::encode(d.dest),
            "verified": d.verified,
            "nullifier": d.nullifier.map(hex::encode),
        })).collect::<Vec<_>>(),
        "burns": report.burns.iter().map(|b| json!({
            "i": b.i,
            "burnIndex": b.burn_index,
            "token": hex::encode(b.token),
            "amount": hex::encode(b.amount),
            "recipient": hex::encode(b.recipient),
        })).collect::<Vec<_>>(),
    })
}

fn error_json(error: &DisclosureError) -> serde_json::Value {
    let (kind, leaf_index, field) = match error {
        DisclosureError::ContextMismatch { field } => ("ContextMismatch", None, Some(*field)),
        DisclosureError::UnknownLeaf { leaf_index } => ("UnknownLeaf", Some(*leaf_index), None),
        DisclosureError::Mismatch { leaf_index, field } => ("Mismatch", Some(*leaf_index), Some(*field)),
        DisclosureError::UnspentNullifier { leaf_index } => ("UnspentNullifier", Some(*leaf_index), None),
    };
    json!({ "error": { "kind": kind, "leafIndex": leaf_index, "field": field, "message": error.to_string() } })
}

fn decode<const N: usize>(text: &str) -> Result<[u8; N], String> {
    let bytes = hex::decode(text).map_err(|e| format!("invalid hex: {e}"))?;
    bytes.try_into().map_err(|v: Vec<u8>| format!("expected {} bytes, got {}", N, v.len()))
}

fn de32<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Hash, D::Error> {
    decode::<32>(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
}

fn de20<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Address, D::Error> {
    decode::<20>(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
}

fn de16<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 16], D::Error> {
    decode::<16>(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
}
