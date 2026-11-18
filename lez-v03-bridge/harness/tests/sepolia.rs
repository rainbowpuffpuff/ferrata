//! Real Sepolia state: the live vault `0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c` of the crown run
//! (`e2e/runs/crown-live-3`), its storage proofs as `eth_getProof` gave them at Sepolia's finalized
//! blocks 12,119,428 (one deposit) and 12,119,519 (two), and those blocks' own headers. The light
//! client's records of them (Gloas layout: the block hash) are placed at `header(instance, slot)`
//! for the finalized slots the captures name; the light client's verification of the finality
//! updates behind them is the v0.4 run's (`crown-live-3`, LEZ session B), not repeated here.
//!
//! That vault pins the v0.4 configuration `0x10837afe…5553`: no configuration of this program can
//! have that id, so `Init` is refused (246, the pairing working), an import is refused for want of
//! a configuration (245) after its plan verified both of Sepolia's storage proofs, and the
//! deposits' claims are refused (245, no imported root). Writes `results/sepolia.md`.

use std::path::{Path, PathBuf};

use ferrata_bridge_core::{inbound::DestKind, sha256};
use ferrata_lez03_bridge_core::{
    code, lc_code, vault_facts, verify_claim, word_of, Denomination, ImportProof, RootState, ETH, SLOT_COUNT,
    SLOT_ROOT,
};
use harness03::{
    deposits::{self, Deposit},
    eth::Captured,
    render, World,
};

const SEPOLIA: u64 = 11_155_111;
const VAULT: [u8; 20] = hex20("04ee8ff19cc11cee4ae65d03e12b9888043de44c");
const DEPLOYER: [u8; 20] = hex20("79a95ae9a3f9000eb41f7cbab6a6f97826895c07");

const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("hex"),
    }
}

const fn hex20(s: &str) -> [u8; 20] {
    let b = s.as_bytes();
    let mut out = [0u8; 20];
    let mut i = 0;
    while i < 20 {
        out[i] = nibble(b[2 * i]) << 4 | nibble(b[2 * i + 1]);
        i += 1;
    }
    out
}

fn word(s: &str) -> [u8; 32] {
    hex::decode(s.trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn captures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../e2e/runs/crown-live-3/captures")
}

#[test]
fn sepolia_live_vault_under_its_own_finalized_blocks() {
    let proof = |b: u64| format!("proof_0x04ee8ff19cc11cee4ae65d03e12b9888043de44c_{b}.json");
    let c1 = Captured::load(&captures().join("capture-1"), &proof(12_119_428), 11_563_328);
    let c2 = Captured::load(&captures().join("capture-2"), &proof(12_119_519), 11_563_424);

    // The two deposits, opened as the crown run's wallet made them (lez/harness/src/vault.rs,
    // `crown_openings`): its test seed, deposit 0 public-kind to LEZ's public test account 0,
    // deposit 1 private-kind to LEZ's private test account 0; 0.001 ETH each.
    let wallet = deposits::keys("e2e: user bridge seed (test value)");
    let dests = [
        (DestKind::Public, word("0x54d662b664519cb8aef608ae84a63e041cc16019e4a07c34828311996f0c817a")),
        (DestKind::Private, word("0x2d6cb849edc6d8b1599c558ecd87bc09b4420bdc60f1b3be85547a5834b153fc")),
    ];
    let deps: Vec<Deposit> = dests
        .iter()
        .enumerate()
        .map(|(j, (kind, dest))| {
            Deposit::new(&wallet, SEPOLIA, &VAULT, DEPLOYER, j as u64, j as u64, ETH, 1_000_000_000_000_000, *kind, *dest)
        })
        .collect();
    let leaves: Vec<_> = deps.iter().map(Deposit::leaf).collect();
    // The vault's own slots, as eth_getProof gave them, are the bridge core's tree of these leaves.
    assert_eq!(c1.value(&SLOT_ROOT), deposits::root(&leaves, 1));
    assert_eq!(c1.value(&SLOT_COUNT), word_of(1));
    assert_eq!(c2.value(&SLOT_ROOT), deposits::root(&leaves, 2));
    assert_eq!(c2.value(&SLOT_COUNT), word_of(2));
    let crown_config_id = word("0x10837afeaf6402e9d4bc2c4a13039e70f88762f13336d9c222d2f6d3b6a95553");

    let mut w = World::new();
    let instance = sha256(b"ferrata/lez-v03-bridge/harness/sepolia-instance");
    let config = w.config(SEPOLIA, VAULT, instance, vec![Denomination { token: ETH, amount: 1_000_000_000_000_000 }]);
    let cid = w.config_id(&config);
    assert_ne!(cid, crown_config_id);
    let r1 = w.place_record(instance, SEPOLIA, &c1.record);
    let r2 = w.place_record(instance, SEPOLIA, &c2.record);

    // Natively, the program's own check of Sepolia's proofs: the vault's slots 0 and 1 at both blocks.
    let i1 = c1.import_proof();
    let f1 = vault_facts(&config, &r1, &i1.account, [(SLOT_ROOT, i1.root_slot.as_slice()), (SLOT_COUNT, i1.count_slot.as_slice())]).unwrap();
    assert_eq!((f1.block_number, f1.values), (12_119_428, [deposits::root(&leaves, 1), word_of(1)]));
    let i2 = c2.import_proof();
    let f2 = vault_facts(&config, &r2, &i2.account, [(SLOT_ROOT, i2.root_slot.as_slice()), (SLOT_COUNT, i2.count_slot.as_slice())]).unwrap();
    assert_eq!((f2.block_number, f2.values), (12_119_519, [deposits::root(&leaves, 2), word_of(2)]));
    // The claim statement, natively, under the root at two leaves: both deposits open it.
    let root2 = RootState {
        config_id: cid,
        evm_chain_id: SEPOLIA,
        vault: VAULT,
        leaf_count: 2,
        root: deposits::root(&leaves, 2),
        record_slot: 11_563_424,
        block_number: 12_119_519,
    };
    for d in &deps {
        let claimed = verify_claim(&root2, &d.witness(&leaves, 2)).unwrap();
        assert_eq!(claimed.nullifier, d.nullifier(SEPOLIA, &VAULT));
    }

    // Through the state machine.
    let t = w.init("Init: the live vault's slot 2 at block 12,119,428", &config, &r1, c1.pairing_proof());
    assert_eq!(t.code, Some(u32::from(code::PAIRING_MISMATCH)));
    let bare = ImportProof { account: c1.proof.proof.clone(), ..c1.import_proof() };
    let t = w.import_root("ImportRoot n = 1 without the block's header", &config, &r1, deposits::root(&leaves, 1), 1, bare);
    assert_eq!(t.code, Some(u32::from(lc_code::HEADER_REQUIRED)));
    let t = w.import_root("ImportRoot n = 1 naming another root", &config, &r1, [5; 32], 1, c1.import_proof());
    assert_eq!(t.code, Some(u32::from(code::IMPORT_MISMATCH)));
    let t = w.import_root("ImportRoot n = 2 at block 12,119,428 (it holds 1)", &config, &r1, deposits::root(&leaves, 1), 2, c1.import_proof());
    assert_eq!(t.code, Some(u32::from(code::IMPORT_MISMATCH)));
    let t = w.import_root("ImportRoot n = 1 at block 12,119,428 (proofs verified; no configuration)", &config, &r1, deposits::root(&leaves, 1), 1, c1.import_proof());
    assert_eq!(t.code, Some(u32::from(code::NOT_FOUND)));
    let t = w.import_root("ImportRoot n = 2 at block 12,119,519 (proofs verified; no configuration)", &config, &r2, deposits::root(&leaves, 2), 2, c2.import_proof());
    assert_eq!(t.code, Some(u32::from(code::NOT_FOUND)));
    let t = w.claim("Claim of deposit 0 (public kind) under the root at two leaves", &config, &root2, &deps[0].witness(&leaves, 2));
    assert_eq!(t.code, Some(u32::from(code::NOT_FOUND)));

    let out = format!(
        "# Real Sepolia state: the live vault `0x04eE8FF1…e44c` (crown-live-3) on LEZ v0.3.1's state machine\n\n\
         Written by `harness/tests/sepolia.rs` (`cargo test -p harness03 --test sepolia`). The vault's storage proofs \
         as `eth_getProof` returned them at Sepolia's finalized blocks 12,119,428 and 12,119,519, with those blocks' own \
         RLP headers (`e2e/runs/crown-live-3/captures/capture-{{1,2}}`); the light client's records (Gloas layout, the \
         block hash) placed for finalized slots 11,563,328 and 11,563,424 (the light client's verification of those \
         finality updates is the v0.4 run's). The deposits' openings are the crown wallet's (test seed): the vault's \
         slot 0 equals the bridge core's tree of the two leaves at both blocks.\n\n\
         Natively, the program's check (`vault_facts`) of both captures gives (block, root, count) = (12,119,428, \
         `{}`, 1) and (12,119,519, `{}`, 2), and the claim statement opens both deposits under the second root.\n\n\
         The vault pins the v0.4 configuration `{}`; this program's configuration is `{}`. So `Init` is refused \
         (246, the pairing working); each import's plan verifies both storage proofs and is then refused by the \
         configuration's guard (245); the claim is refused (245, no root imported).\n\n{}",
        hex::encode(f1.values[0]),
        hex::encode(f2.values[0]),
        hex::encode(crown_config_id),
        hex::encode(cid),
        render(&w.reports),
    );
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../results");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("sepolia.md"), out).unwrap();
}
