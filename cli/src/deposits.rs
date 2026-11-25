//! The deposit journal: a deposit sent once is never sent a second time by a run that did not see it land (RFP-022
//! R.7, the deposit leg).
//!
//! `ferrata deposit --send` numbers the depositor's deposit `j = depositCount(depositor)` and the vault refuses any
//! other `j` (`WrongDepositIndex`). A run killed after its transaction left (the answer lost, the tool killed while it
//! waited for the receipt) leaves a deposit on the chain that the user never saw; until this journal, the same command
//! again read `depositCount` past it and sent a second deposit (`j + 1`) of the user's money. Now:
//! - Before the transaction is signed, the journal records the deposit (chain, vault, depositor, `j`, token,
//!   denomination, destination, commitment) and the nonce it takes; the hash once the node has it. The entry goes when
//!   the receipt is in.
//! - The next run resolves every open entry of that depositor from the chain alone before it plans anything:
//!   `depositCount` past `j` (the vault numbers a depositor's deposits and only the depositor's key makes one):
//!   **on chain**; the nonce spent with the count unmoved (reverted, or replaced): **not deposited**; the transaction
//!   known to the node or a pending nonce past it: **pending**; otherwise **not sent** (it never reached a node, its
//!   nonce unspent).
//! - The same deposit asked again (same token, denomination and destination) and found on chain is reported and not
//!   sent again; pending, the run waits for it; not sent, it is sent again with the same `j` and the same nonce, so at
//!   most one of the two can ever be mined. Another deposit is planned after the open one is resolved, and refused
//!   while it is pending.
//!
//! Nothing here is needed to find a deposit: `ferrata scan` and `ferrata recover` find every deposit of the seed from the
//! chain. The file is beside the user's other local state (`<LEZ wallet home>/ferrata-deposits.json`, or beside the
//! keystore), mode 0600; `ferrata-deposits.lock` serialises the tool's processes on it.

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use alloy::primitives::{Address, B256};
use anyhow::{bail, Context as _, Result};
use serde::{Deserialize, Serialize};

/// The journal's file name.
pub const FILE: &str = "ferrata-deposits.json";
/// The lock file beside it.
pub const LOCK: &str = "ferrata-deposits.lock";

/// A deposit recorded before its transaction was signed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub chain_id: u64,
    pub vault: Address,
    pub depositor: Address,
    /// The depositor's deposit number the transaction names.
    pub j: u64,
    pub token: Address,
    pub denomination: u8,
    /// `private /a` or `public /a`.
    pub destination: String,
    /// The commitment `C` (hex).
    pub commitment: String,
    /// The depositor's nonce the transaction takes.
    pub nonce: u64,
    /// Its hash, once a node answered the send.
    pub hash: Option<B256>,
    /// Unix seconds when it was recorded.
    pub at: u64,
}

impl Entry {
    /// The same deposit: the same depositor, vault, token, denomination and destination.
    pub fn same_ask(&self, ask: &Entry) -> bool {
        self.chain_id == ask.chain_id
            && self.vault == ask.vault
            && self.depositor == ask.depositor
            && self.token == ask.token
            && self.denomination == ask.denomination
            && self.destination == ask.destination
    }

    pub fn describe(&self) -> String {
        format!(
            "deposit j = {} of {} (denomination {} of token {}, to your {} account), nonce {}{}",
            self.j,
            self.depositor,
            self.denomination,
            self.token,
            self.destination,
            self.nonce,
            self.hash.map(|h| format!(", transaction {h}")).unwrap_or_default()
        )
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Journal {
    pub version: u32,
    pub open: Vec<Entry>,
}

/// What the chain says of an open entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// `depositCount` moved past `j`: the deposit is on chain.
    OnChain,
    /// Known to the node, or its nonce taken by a pending transaction: not mined yet.
    Pending,
    /// The nonce spent and the count unmoved: the transaction reverted or another took the nonce; nothing deposited.
    NotDeposited,
    /// Neither mined nor known, the nonce unspent: it never reached a node.
    NotSent,
}

/// The decision, from the chain's numbers alone: `deposit_count` = `depositCount(depositor)` now, `latest_nonce` and
/// `pending_nonce` = the depositor's transaction counts at the latest block and with the node's pending ones, `known` =
/// the recorded hash is known to the node.
pub fn status(e: &Entry, deposit_count: u64, latest_nonce: u64, pending_nonce: u64, known: bool) -> Status {
    if deposit_count > e.j {
        Status::OnChain
    } else if latest_nonce > e.nonce {
        Status::NotDeposited
    } else if known || pending_nonce > e.nonce {
        Status::Pending
    } else {
        Status::NotSent
    }
}

/// The journal in one directory.
pub struct Store {
    dir: PathBuf,
}

/// The lock on a store, held while a run resolves, records and sends.
pub struct Locked<'a> {
    store: &'a Store,
    _file: fs::File,
}

impl Store {
    pub fn new(dir: &Path) -> Self {
        Store { dir: dir.to_path_buf() }
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(FILE)
    }

    /// Takes the lock (waits for another run of the tool on the same journal).
    pub fn lock(&self) -> Result<Locked<'_>> {
        fs::create_dir_all(&self.dir).with_context(|| format!("creating {}", self.dir.display()))?;
        let path = self.dir.join(LOCK);
        let f = private_file(&path, false)?;
        f.lock().with_context(|| format!("locking {}", path.display()))?;
        Ok(Locked { store: self, _file: f })
    }

    /// The journal, read without the lock (it is replaced whole): for a dry run's report.
    pub fn peek(&self) -> Result<Journal> {
        self.load()
    }

    fn load(&self) -> Result<Journal> {
        let path = self.path();
        match fs::read(&path) {
            Ok(b) => {
                let j: Journal = serde_json::from_slice(&b)
                    .with_context(|| format!("reading the deposit journal {}", path.display()))?;
                if j.version != 1 {
                    bail!("the deposit journal {} has version {}; this tool reads version 1", path.display(), j.version);
                }
                Ok(j)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Journal { version: 1, open: vec![] }),
            Err(e) => Err(e).with_context(|| format!("reading the deposit journal {}", path.display())),
        }
    }

    /// Written to a temporary file, synced, renamed over the journal, and the directory synced.
    fn save(&self, j: &Journal) -> Result<()> {
        let path = self.path();
        let tmp = self.dir.join(format!("{FILE}.tmp"));
        {
            let mut f = private_file(&tmp, true)?;
            f.write_all(&serde_json::to_vec_pretty(j)?)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &path).with_context(|| format!("writing the deposit journal {}", path.display()))?;
        if let Ok(d) = fs::File::open(&self.dir) {
            let _ = d.sync_all();
        }
        Ok(())
    }
}

impl Locked<'_> {
    /// The open entries of one depositor on one vault and chain.
    pub fn open_of(&self, chain_id: u64, vault: Address, depositor: Address) -> Result<Vec<Entry>> {
        Ok(self
            .store
            .load()?
            .open
            .into_iter()
            .filter(|e| e.chain_id == chain_id && e.vault == vault && e.depositor == depositor)
            .collect())
    }

    /// Records `e` before its transaction is signed (replacing an entry with the same depositor and `j`).
    pub fn record(&self, e: &Entry) -> Result<()> {
        let mut j = self.store.load()?;
        j.open.retain(|o| !same_slot(o, e));
        j.open.push(e.clone());
        self.store.save(&j)
    }

    /// The node took the transaction: its hash.
    pub fn sent(&self, e: &Entry, hash: B256) -> Result<()> {
        let mut j = self.store.load()?;
        for o in j.open.iter_mut().filter(|o| same_slot(o, e)) {
            o.hash = Some(hash);
        }
        self.store.save(&j)
    }

    /// The entry resolved (its receipt in, or found on chain, or found never deposited): removed.
    pub fn resolved(&self, e: &Entry) -> Result<()> {
        let mut j = self.store.load()?;
        j.open.retain(|o| !same_slot(o, e));
        self.store.save(&j)
    }
}

fn same_slot(a: &Entry, b: &Entry) -> bool {
    a.chain_id == b.chain_id && a.vault == b.vault && a.depositor == b.depositor && a.j == b.j
}

/// Opens (creating it, mode 0600 on Unix) a file for writing (`truncate`) or locking.
fn private_file(path: &Path, truncate: bool) -> Result<fs::File> {
    let mut o = fs::OpenOptions::new();
    o.create(true).write(true).truncate(truncate);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        o.mode(0o600);
    }
    o.open(path).with_context(|| format!("opening {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(j: u64, nonce: u64) -> Entry {
        Entry {
            chain_id: 11155111,
            vault: Address::repeat_byte(0x11),
            depositor: Address::repeat_byte(0x22),
            j,
            token: Address::ZERO,
            denomination: 0,
            destination: "private /0".into(),
            commitment: "0xc0".into(),
            nonce,
            hash: None,
            at: 1,
        }
    }

    /// Every case of the decision: the count past `j` wins (the deposit is on chain, whoever's transaction carried
    /// it); then a spent nonce means nothing was deposited; then pending; then never sent.
    #[test]
    fn the_chain_decides_each_open_deposit() {
        let e = entry(3, 7);
        assert_eq!(status(&e, 4, 8, 8, false), Status::OnChain);
        assert_eq!(status(&e, 4, 7, 7, false), Status::OnChain, "the count alone (another node mined it)");
        assert_eq!(status(&e, 3, 8, 8, true), Status::NotDeposited, "reverted or replaced: the nonce spent");
        assert_eq!(status(&e, 3, 7, 8, false), Status::Pending, "a pending transaction holds the nonce");
        assert_eq!(status(&e, 3, 7, 7, true), Status::Pending, "the node knows the hash");
        assert_eq!(status(&e, 3, 7, 7, false), Status::NotSent);
    }

    #[test]
    fn the_same_ask_is_the_same_token_denomination_and_destination() {
        let a = entry(3, 7);
        let mut b = entry(4, 9);
        assert!(a.same_ask(&b), "j and the nonce are the chain's, not the ask's");
        b.destination = "private /1".into();
        assert!(!a.same_ask(&b));
        let mut c = entry(3, 7);
        c.denomination = 1;
        assert!(!a.same_ask(&c));
    }

    /// The store: recorded before sending, the hash added, resolved; another depositor's entries untouched; mode 0600;
    /// a journal of another version refused.
    #[test]
    fn the_store_records_sends_and_resolves() {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join(format!("deposits-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = Store::new(&dir);
        let l = s.lock().unwrap();
        let a = entry(3, 7);
        let mut other = entry(0, 0);
        other.depositor = Address::repeat_byte(0x33);
        l.record(&a).unwrap();
        l.record(&other).unwrap();
        assert_eq!(l.open_of(a.chain_id, a.vault, a.depositor).unwrap(), vec![a.clone()]);
        let h = B256::repeat_byte(0xab);
        l.sent(&a, h).unwrap();
        assert_eq!(l.open_of(a.chain_id, a.vault, a.depositor).unwrap()[0].hash, Some(h));
        l.record(&a).unwrap();
        assert_eq!(l.open_of(a.chain_id, a.vault, a.depositor).unwrap().len(), 1, "one entry per depositor and j");
        l.resolved(&a).unwrap();
        assert!(l.open_of(a.chain_id, a.vault, a.depositor).unwrap().is_empty());
        assert_eq!(s.peek().unwrap().open, vec![other]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(fs::metadata(s.path()).unwrap().permissions().mode() & 0o777, 0o600);
        }
        fs::write(s.path(), br#"{"version": 2, "open": []}"#).unwrap();
        assert!(format!("{:#}", s.peek().unwrap_err()).contains("version 2"));
        let _ = fs::remove_dir_all(&dir);
    }
}
