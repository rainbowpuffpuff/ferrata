//! The updater's converter (`ferrata_updater::lc_json`) on what `beacon_api_module.lc_request` answered in a
//! verified-proxy session (modules/scripts/verified-proxy-session.sh), against the SSZ the providers served for the same
//! requests at the same time, and against the 12 November fixture.
//!
//!     cargo run --example lc_json_compare -p ferrata-updater -- SESSION_DIR FIXTURE_DIR
//!
//! Writes SESSION_DIR/lc-ssz-compare.json; exits 1 when a comparison fails.

use std::{fs, path::Path};

use anyhow::{anyhow, Context as _, Result};
use ferrata_updater::{chain::split_chunks, lc_json};
use serde_json::{json, Value};

fn body(w: &Path, name: &str) -> Result<(Value, String)> {
    let r: Value = serde_json::from_slice(&fs::read(w.join("rep").join(format!("{name}.json")))?)?;
    if r["ok"].as_bool() != Some(true) {
        return Err(anyhow!("{name}: beacon_api_module answered {r}"));
    }
    let provider = r["provider"].as_str().unwrap_or("?").to_string();
    Ok((serde_json::from_str(r["body"].as_str().context("no body")?)?, provider))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (w, fx) = (Path::new(&args[1]), Path::new(&args[2]));
    let plain = |n: &str| fs::read(w.join("plain").join(n)).with_context(|| n.to_string());
    let mut rows = Vec::new();
    let mut all = true;
    let mut row = |what: &str, provider: &str, ours: &[u8], against: Vec<(&str, Result<Vec<u8>>)>| {
        let eq: Vec<Value> = against
            .into_iter()
            .map(|(name, b)| {
                let equal = b.as_ref().is_ok_and(|b| b.as_slice() == ours);
                all &= equal;
                json!({ "against": name, "equal": equal, "bytes": b.as_ref().map(Vec::len).ok() })
            })
            .collect();
        rows.push(json!({ "what": what, "provider": provider, "bytes": ours.len(), "comparisons": eq }));
    };

    let (v, p) = body(w, "B-bootstrap")?;
    let ours = lc_json::bootstrap(&v)?;
    row("bootstrap", &p, &ours, vec![
        ("Nimbus's SSZ (same request, same session)", plain("nimbus-bootstrap.ssz")),
        ("the fixture's bootstrap.ssz (Lodestar, 12 Nov)", fs::read(fx.join("bootstrap.ssz")).map_err(Into::into)),
    ]);

    let (v, p) = body(w, "B-updates")?;
    let ours = lc_json::updates(&v)?;
    let served = plain("lodestar-updates.ssz").map(|b| split_chunks(&b));
    for (i, u) in ours.iter().enumerate() {
        let mut against = vec![(
            "Lodestar's SSZ stream (same request), split",
            served.as_ref().map_err(|e| anyhow!("{e}")).and_then(|s| s.get(i).cloned().context("missing")),
        )];
        if i == 0 {
            against.push(("the fixture's update_1411.ssz", fs::read(fx.join("update_1411.ssz")).map_err(Into::into)));
        }
        row(&format!("update {i} (period {})", 1411 + i), &p, u, against);
    }

    let (v, p) = body(w, "B-finality")?;
    let ours = lc_json::finality_update(&v)?;
    row("finality update", &p, &ours, vec![("Lodestar's SSZ (same signature slot)", plain("lodestar-finality.ssz"))]);

    let out = json!({ "all_equal": all, "rows": rows });
    fs::write(w.join("lc-ssz-compare.json"), serde_json::to_string_pretty(&out)? + "\n")?;
    println!("{}", serde_json::to_string(&out)?);
    std::process::exit(if all { 0 } else { 1 })
}
