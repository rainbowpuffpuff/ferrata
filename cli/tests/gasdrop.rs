//! The relayer's gas-drop checks on a real receipt: `bridge/artifacts/gasdrop/receipt-succinct.json`, a
//! succinct proof of the gas-drop guest (`bridge/host prove-gasdrop`, CPU, not dev mode), verified here
//! against its image id. One drop per `N`; a receipt replayed to another relayer, under a root LEZ has
//! not imported, for a claimed deposit, with a changed journal or for another image is refused.

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Result;
use ferrata_bridge_core::gasdrop::GasDropJournal;
use ferrata_cli::relayer::{self, Credit, DropBook, DropContext, DropRefusal};

fn artifacts() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../bridge/artifacts/gasdrop")
}

/// Stands in for the sequencer: records the credits it is asked to send.
#[derive(Default)]
struct Recorded(Vec<([u8; 32], u128)>);
impl Credit for Recorded {
    async fn credit(&mut self, to: [u8; 32], amount: u128) -> Result<String> {
        self.0.push((to, amount));
        Ok(format!("credit {}", self.0.len()))
    }
}

#[tokio::test]
async fn one_drop_per_n_on_a_real_receipt() -> Result<()> {
    if std::env::var("RISC0_DEV_MODE").is_ok_and(|v| !v.is_empty() && v != "0") {
        eprintln!("skipped: RISC0_DEV_MODE is set; this test verifies a real receipt");
        return Ok(());
    }
    let receipt = std::fs::read(artifacts().join("receipt-succinct.json"))?;
    let image_id = ferrata_cli::h32(&std::fs::read_to_string(artifacts().join("image-id.txt"))?)?;
    let parsed: risc0_zkvm::Receipt = serde_json::from_slice(&receipt)?;
    let j = GasDropJournal::decode(&parsed.journal.bytes).expect("a gas-drop journal");
    let roots: HashSet<[u8; 32]> = [j.root].into_iter().collect();
    let ctx = DropContext { image_id, me: j.relayer, chain_id: j.chain_id, vault: j.vault, imported_roots: &roots };
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("gasdrop");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let book_path = dir.join("drops.json");
    let mut book = DropBook::load(&book_path)?;
    let mut credit = Recorded::default();

    // Bound to another relayer: replaying it to this one is refused.
    let other = DropContext { me: [0x11; 32], ..DropContext { imported_roots: &roots, ..ctx } };
    assert_eq!(relayer::check_gas_drop(&receipt, &other, |_| false, &book).unwrap_err(), DropRefusal::OtherRelayer);
    // Another configuration, a root LEZ has not imported, a claimed deposit.
    let other = DropContext { vault: [0x22; 20], ..DropContext { imported_roots: &roots, ..ctx } };
    assert_eq!(
        relayer::check_gas_drop(&receipt, &other, |_| false, &book).unwrap_err(),
        DropRefusal::OtherConfiguration
    );
    let none = HashSet::new();
    let other = DropContext { imported_roots: &none, ..ctx };
    assert_eq!(relayer::check_gas_drop(&receipt, &other, |_| false, &book).unwrap_err(), DropRefusal::RootNotImported);
    let ctx = DropContext { imported_roots: &roots, ..other };
    assert_eq!(relayer::check_gas_drop(&receipt, &ctx, |_| true, &book).unwrap_err(), DropRefusal::AlreadyClaimed);
    // Another image id; a journal changed after proving (the one-time address redirected).
    let wrong = DropContext { image_id: [0x33; 32], ..DropContext { imported_roots: &roots, ..ctx } };
    assert!(matches!(relayer::check_gas_drop(&receipt, &wrong, |_| false, &book), Err(DropRefusal::BadReceipt(_))));
    let mut redirected = parsed.clone();
    redirected.journal.bytes[150] ^= 1;
    let redirected = serde_json::to_vec(&redirected)?;
    assert!(matches!(relayer::check_gas_drop(&redirected, &ctx, |_| false, &book), Err(DropRefusal::BadReceipt(_))));

    // The drop: sent once, recorded on disk; the second request for the same N is refused, also after a restart.
    let (dropped, tx) =
        relayer::gas_drop(&receipt, &ctx, |_| false, &mut book, &book_path, &mut credit, 50_000, 1).await?;
    assert_eq!(dropped, j);
    assert_eq!(credit.0, vec![(j.one_time_address, 50_000)]);
    println!("drop sent: N {} -> {} ({tx})", ferrata_cli::hx(&j.nullifier), ferrata_cli::hx(&j.one_time_address));
    let again = relayer::gas_drop(&receipt, &ctx, |_| false, &mut book, &book_path, &mut credit, 50_000, 2).await;
    assert!(again.unwrap_err().to_string().contains("already had its drop"));
    let reloaded = DropBook::load(&book_path)?;
    assert_eq!(relayer::check_gas_drop(&receipt, &ctx, |_| false, &reloaded).unwrap_err(), DropRefusal::AlreadyDropped);
    assert_eq!(credit.0.len(), 1, "one credit per N");
    println!("gas drop: 1 credit for N; refused: other relayer, other configuration, root not imported, claimed, wrong image, changed journal, second drop (also after restart)");
    Ok(())
}
