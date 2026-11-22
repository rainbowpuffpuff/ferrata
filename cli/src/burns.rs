//! The burn-counter journal: a burn's counter is never given to another burn (RFP-022 R.7; the reviews of 22 November
//! 2026: the final review's H1, the bridge code review's H-1).
//!
//! A wallet's `i`-th burn under a configuration has the tag `r_i = HMAC(kView, cid ‖ i)`, which fixes its release
//! secret `sOut` and nullifier `NOut`: two notes with one counter could never both be released. Until these reviews the
//! tool took the next counter from the burns already applied on the zone, so a second burn planned while a first was in
//! flight (from another holder of the seed) took the same counter, and after an interrupted burn the next one took the
//! interrupted burn's counter whatever its recipient, token or amount. The bridge program now refuses a second note with
//! a used tag (ADR 0032), so such a burn is refused rather than lost; the journal keeps the tool from sending one.
//!
//! The rules, for one seed and one configuration:
//! - A counter is reserved here, under a lock, before the burn is proven and sent, and is never given to another
//!   burn. Its entry stays until the zone shows the burn's tag (applied, refused, or taken by another note).
//! - The same burn asked again (the same holder, token, amount and recipient) while its entry is open takes its counter
//!   again: the note is the same, and the program applies one note per tag, so at most one of the two is applied.
//!   Interrupted before it was known to be sent and found applied, it is not burnt again.
//! - Any other burn takes the next counter after every reserved one and every one the zone shows used (a refused
//!   burn's tag too: its memo is public in its block).
//! - At most [`MAX_AHEAD`] counters are reserved past the zone's next one while none of them is on the zone: a
//!   recovery from the seed alone stops after `lez::BURN_GAP` unused counters, and must find every burn.
//!
//! Nothing here is needed to recover anything: a client restored from the seed alone finds every burn on the zone
//! (`outbound::recover_burns`). The file is beside LEZ's wallet (`<LEZ wallet home>/ferrata-burns.json`, mode 0600),
//! which holds the user's keys already; `ferrata-burns.lock` beside it serialises the tool's processes on one wallet.

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use alloy::primitives::Address;
use anyhow::{anyhow, bail, Context as _, Result};
use ferrata_bridge_core::Hash;
use serde::{Deserialize, Serialize};

use crate::{h32, hx};

/// The journal's file name, in the LEZ wallet's home.
pub const FILE: &str = "ferrata-burns.json";
/// The lock file beside it.
pub const LOCK: &str = "ferrata-burns.lock";
/// The interrupted-burn record of the tool before 22 November 2026 (one burn), imported once.
pub const LEGACY: &str = "ferrata-pending-burn.json";
/// The holder recorded for the old tool's record, which did not keep one: the same note from any holder resumes it.
const LEGACY_FROM: &str = "(recorded by an earlier version of the tool)";
/// The most counters the tool reserves past the zone's next one while none of them is on the zone (half the recovery
/// scan's gap, `lez::BURN_GAP`).
pub const MAX_AHEAD: u64 = crate::lez::BURN_GAP / 2;

/// A reserved counter and the burn it was reserved for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reserved {
    pub i: u64,
    /// The holder: `private /a` or `public /a`.
    pub from: String,
    pub token: Address,
    /// Base units, in decimal.
    pub amount: String,
    pub recipient: Address,
    /// The note's tag `r_i` (hex).
    pub r: String,
    /// The note's hash (`H(…/note ‖ r ‖ memo ‖ mac ‖ token ‖ amount ‖ COut)`, hex): tells this burn's note from another
    /// note with its tag.
    pub note: String,
    /// When it was reserved (Unix seconds).
    pub at: u64,
    /// The transaction's hash once the sequencer took it; `None`: not known to be sent (interrupted before or while
    /// sending).
    pub sent: Option<String>,
}

impl Reserved {
    fn matches(&self, ask: &Ask) -> bool {
        (self.from == ask.from || self.from == LEGACY_FROM)
            && self.token == ask.token
            && self.amount == ask.amount.to_string()
            && self.recipient == ask.recipient
    }

    fn describe(&self) -> String {
        format!(
            "counter {}, {} base units of {} from {} to {}",
            self.i, self.amount, self.token, self.from, self.recipient
        )
    }

    fn state(&self) -> String {
        match &self.sent {
            Some(h) => format!("sent as {h}"),
            None => "interrupted before it was known to be sent".into(),
        }
    }
}

/// One seed's counters under one configuration.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counters {
    /// The configuration (hex), for the reader.
    pub cid: String,
    /// The lowest counter never reserved.
    pub next: u64,
    /// Reserved counters whose tag the zone does not show yet.
    pub open: Vec<Reserved>,
}

/// The journal: per seed and configuration ([`key`]).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Journal {
    pub version: u32,
    pub wallets: BTreeMap<String, Counters>,
}

/// The journal's key for a seed under a configuration: a hash of the seed's first tag there (`r_0`), so that two seeds
/// (or two configurations) sharing one LEZ wallet home keep their counters apart.
pub fn key(r0: &Hash) -> String {
    hx(&ferrata_bridge_core::sha256(&[&b"ferrata/burn-journal/v1"[..], r0].concat()))
}

/// What a burn asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    pub from: String,
    pub token: Address,
    pub amount: u128,
    pub recipient: Address,
}

/// What the zone shows of this configuration's burns: each tag's note hash, applied or refused, and the first counter
/// after the last one used (applied or refused) as the recovery scan finds it.
#[derive(Clone, Debug, Default)]
pub struct Seen {
    pub applied: HashMap<Hash, Hash>,
    pub refused: HashMap<Hash, Hash>,
    pub next: u64,
}

/// The counter a burn takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Counter {
    /// A counter no burn of this seed has had.
    Fresh(u64),
    /// The counter of an earlier run's burn of the same note that the zone does not show: the same note again.
    Resume(Reserved),
    /// An earlier run's burn of the same note, interrupted before it was known to be sent, is applied: nothing to burn.
    Done(Reserved),
}

impl Counter {
    pub fn i(&self) -> u64 {
        match self {
            Counter::Fresh(i) => *i,
            Counter::Resume(e) | Counter::Done(e) => e.i,
        }
    }
}

/// The decision and what to tell the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub counter: Counter,
    pub notes: Vec<String>,
}

/// Decides `ask`'s counter in `c`, after resolving its open entries against `zone`. `tag_of(i)` and `note_of(i)` give
/// the tag and the note hash of `ask`'s burn with counter `i`. Pure: the caller writes `c` back (or not, for a dry run).
pub fn choose(
    c: &mut Counters,
    zone: &Seen,
    ask: &Ask,
    now: u64,
    tag_of: &dyn Fn(u64) -> Hash,
    note_of: &dyn Fn(u64) -> Hash,
) -> Result<Choice> {
    let mut notes = Vec::new();
    let mut done = None;
    let mut open = Vec::new();
    for e in std::mem::take(&mut c.open) {
        let (Ok(tag), Ok(note)) = (h32(&e.r), h32(&e.note)) else {
            notes.push(format!(
                "a journal entry that does not parse was dropped ({}); its counter stays used",
                e.describe()
            ));
            continue;
        };
        match (zone.applied.get(&tag), zone.refused.get(&tag)) {
            (Some(n), _) if *n == note => {
                notes.push(format!("an earlier burn ({}) is on the zone", e.describe()));
                if e.sent.is_none() && e.matches(ask) && done.is_none() {
                    done = Some(e);
                }
            }
            (Some(_), _) => notes.push(format!(
                "the tag of an earlier burn ({}) is on the zone with another note (another device or tool using this \
                 seed): that burn is not applied and the program refuses it; its counter stays used",
                e.describe()
            )),
            (None, Some(_)) => notes.push(format!(
                "an earlier burn ({}) was refused on the zone: nothing was burnt; its counter stays used",
                e.describe()
            )),
            (None, None) => open.push(e),
        }
    }
    c.open = open;
    c.next = c.next.max(zone.next).max(c.open.iter().map(|e| e.i + 1).max().unwrap_or(0));
    if let Some(e) = done {
        return Ok(Choice { counter: Counter::Done(e), notes });
    }
    if let Some(e) = c.open.iter().find(|e| e.matches(ask)) {
        notes.push(match &e.sent {
            None => format!(
                "an earlier run's burn of the same note ({}) was interrupted before it was known to be sent and is not \
                 on the zone: this burn takes its counter again, the same note, so at most one of the two is applied",
                e.describe()
            ),
            Some(h) => format!(
                "an earlier burn of the same note ({}) was sent as {h} and is not on the zone yet: this burn sends the \
                 same note again, so at most one of the two is applied; to burn the same amount to the same recipient a \
                 second time, wait until the first is on the zone (`ferrata scan`)",
                e.describe()
            ),
        });
        return Ok(Choice { counter: Counter::Resume(e.clone()), notes });
    }
    for e in &c.open {
        notes.push(format!(
            "an earlier burn ({}) is not on the zone yet ({}): it keeps its counter",
            e.describe(),
            e.state()
        ));
    }
    let i = c.next;
    if i - zone.next >= MAX_AHEAD {
        bail!(
            "refusing to reserve burn counter {i}: counters {}..{} are reserved by earlier burns of this seed and none of \
             them is on the zone; a recovery from the seed alone looks {} counters past the last burn it finds, so the \
             tool reserves at most {MAX_AHEAD} ahead. Wait until they are on the zone (`ferrata scan`), or run one of \
             them again with the same holder, amount and recipient: it keeps its counter",
            zone.next,
            i - 1,
            crate::lez::BURN_GAP
        );
    }
    c.next = i + 1;
    c.open.push(Reserved {
        i,
        from: ask.from.clone(),
        token: ask.token,
        amount: ask.amount.to_string(),
        recipient: ask.recipient,
        r: hx(&tag_of(i)),
        note: hx(&note_of(i)),
        at: now,
        sent: None,
    });
    Ok(Choice { counter: Counter::Fresh(i), notes })
}

/// The old tool's interrupted-burn record, as an open entry of this seed's counters: `None` when there is none, or
/// when its tag is not this seed's under this configuration (left in place, with a note).
pub fn legacy(
    dir: &Path,
    decimals: u8,
    tag_of: &dyn Fn(u64) -> Hash,
    note_of: &dyn Fn(u64, Address, u128, Address) -> Hash,
) -> Result<(Option<Reserved>, Vec<String>)> {
    #[derive(Deserialize)]
    struct Old {
        i: u64,
        token: Address,
        amount: String,
        recipient: Address,
        r: String,
        at: u64,
    }
    let path = dir.join(LEGACY);
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((None, Vec::new())),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let old: Old = serde_json::from_slice(&bytes).with_context(|| format!("reading {}", path.display()))?;
    if h32(&old.r).ok() != Some(tag_of(old.i)) {
        return Ok((
            None,
            vec![format!(
                "{} names burn counter {} with a tag that is not this seed's under this configuration: left as it is",
                path.display(),
                old.i
            )],
        ));
    }
    let amount = crate::units::parse(&old.amount, decimals).unwrap_or(0);
    let e = Reserved {
        i: old.i,
        from: LEGACY_FROM.into(),
        token: old.token,
        amount: amount.to_string(),
        recipient: old.recipient,
        r: old.r,
        note: hx(&note_of(old.i, old.token, amount, old.recipient)),
        at: old.at,
        sent: None,
    };
    let note = format!("imported the interrupted-burn record of an earlier version of the tool ({})", e.describe());
    Ok((Some(e), vec![note]))
}

/// The journal in the LEZ wallet's home `dir`.
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: &Path) -> Self {
        Store { dir: dir.to_path_buf() }
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(FILE)
    }

    fn lock(&self) -> Result<fs::File> {
        fs::create_dir_all(&self.dir).with_context(|| format!("creating {}", self.dir.display()))?;
        let path = self.dir.join(LOCK);
        let f = private_file(&path, false)?;
        f.lock().with_context(|| format!("locking {}", path.display()))?;
        Ok(f)
    }

    fn load(&self) -> Result<Journal> {
        let path = self.path();
        match fs::read(&path) {
            Ok(b) => {
                let j: Journal = serde_json::from_slice(&b)
                    .with_context(|| format!("reading the burn journal {}", path.display()))?;
                if j.version != 1 {
                    bail!("the burn journal {} has version {}; this tool reads version 1", path.display(), j.version);
                }
                Ok(j)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Journal { version: 1, ..Journal::default() }),
            Err(e) => Err(e).with_context(|| format!("reading the burn journal {}", path.display())),
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
        fs::rename(&tmp, &path).with_context(|| format!("writing the burn journal {}", path.display()))?;
        if let Ok(d) = fs::File::open(&self.dir) {
            let _ = d.sync_all();
        }
        Ok(())
    }

    /// The decision for `ask`, read and written nowhere: a dry run, or the plan before [`Store::reserve`].
    #[allow(clippy::too_many_arguments)]
    pub fn peek(
        &self,
        key: &str,
        cid: &Hash,
        zone: &Seen,
        ask: &Ask,
        now: u64,
        legacy: Option<Reserved>,
        tag_of: &dyn Fn(u64) -> Hash,
        note_of: &dyn Fn(u64) -> Hash,
    ) -> Result<Choice> {
        let mut c =
            self.load()?.wallets.remove(key).unwrap_or_else(|| Counters { cid: hx(cid), ..Counters::default() });
        merge(&mut c, legacy);
        choose(&mut c, zone, ask, now, tag_of, note_of)
    }

    /// Under the lock: the same decision, written before the burn is proven and sent, if it is still `expect` (another
    /// run of the tool may have reserved that counter since the plan). Imports and then removes the old tool's record.
    #[allow(clippy::too_many_arguments)]
    pub fn reserve(
        &self,
        key: &str,
        cid: &Hash,
        zone: &Seen,
        ask: &Ask,
        now: u64,
        legacy: Option<Reserved>,
        tag_of: &dyn Fn(u64) -> Hash,
        note_of: &dyn Fn(u64) -> Hash,
        expect: &Counter,
    ) -> Result<Choice> {
        let _lock = self.lock()?;
        let mut j = self.load()?;
        let mut c = j.wallets.remove(key).unwrap_or_else(|| Counters { cid: hx(cid), ..Counters::default() });
        let had_legacy = legacy.is_some();
        merge(&mut c, legacy);
        let choice = choose(&mut c, zone, ask, now, tag_of, note_of)?;
        if choice.counter.i() != expect.i() || std::mem::discriminant(&choice.counter) != std::mem::discriminant(expect)
        {
            bail!(
                "another run of the tool reserved burn counter {} since this one planned it (this one would now take {}): \
                 nothing was sent; run the command again",
                expect.i(),
                choice.counter.i()
            );
        }
        j.wallets.insert(key.to_string(), c);
        self.save(&j)?;
        if had_legacy {
            let old = self.dir.join(LEGACY);
            fs::remove_file(&old)
                .or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) })?;
        }
        Ok(choice)
    }

    /// Under the lock: the burn with counter `i` was taken by the sequencer as transaction `h`.
    pub fn sent(&self, key: &str, i: u64, h: &str) -> Result<()> {
        let _lock = self.lock()?;
        let mut j = self.load()?;
        let c = j.wallets.get_mut(key).ok_or_else(|| anyhow!("the burn journal has no counters for this seed"))?;
        let e = c
            .open
            .iter_mut()
            .find(|e| e.i == i)
            .ok_or_else(|| anyhow!("the burn journal has no open burn counter {i}"))?;
        e.sent = Some(h.to_string());
        self.save(&j)
    }

    /// This seed's counters, as the journal holds them (`ferrata scan` shows the open ones).
    pub fn counters(&self, key: &str) -> Result<Option<Counters>> {
        Ok(self.load()?.wallets.remove(key))
    }
}

/// Adds the old tool's record to `c` unless that counter is already there.
fn merge(c: &mut Counters, legacy: Option<Reserved>) {
    if let Some(e) = legacy {
        c.next = c.next.max(e.i + 1);
        if !c.open.iter().any(|o| o.i == e.i) {
            c.open.push(e);
        }
    }
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

    fn tag(i: u64) -> Hash {
        [i as u8 + 1; 32]
    }

    /// A note hash that depends on the counter and the burn's recipient, as a real note does.
    fn note(i: u64, to: Address) -> Hash {
        let mut h = [0u8; 32];
        h[0] = i as u8;
        h[1..21].copy_from_slice(to.as_slice());
        h
    }

    fn ask(from: &str, amount: u128, to: u8) -> Ask {
        Ask { from: from.into(), token: Address::ZERO, amount, recipient: Address::repeat_byte(to) }
    }

    fn pick(c: &mut Counters, zone: &Seen, a: &Ask) -> Result<Choice> {
        let to = a.recipient;
        choose(c, zone, a, 1_700_000_000, &tag, &move |i| note(i, to))
    }

    fn applied(pairs: &[(u64, Address)]) -> HashMap<Hash, Hash> {
        pairs.iter().map(|(i, to)| (tag(*i), note(*i, *to))).collect()
    }

    /// H-1: two burns in flight from two holders of the seed: the second never takes the first's counter.
    #[test]
    fn a_second_burn_in_flight_takes_the_next_counter() {
        let mut c = Counters::default();
        let zone = Seen { next: 3, ..Seen::default() };
        let first = pick(&mut c, &zone, &ask("private /0", 1, 0xe1)).unwrap();
        assert_eq!(first.counter, Counter::Fresh(3));
        c.open[0].sent = Some("0xaa".into());
        // The zone has not applied it yet: its next counter is still 3.
        let second = pick(&mut c, &zone, &ask("private /1", 1, 0xe2)).unwrap();
        assert_eq!(second.counter, Counter::Fresh(4), "never the in-flight burn's counter");
        assert!(
            second.notes.iter().any(|n| n.contains("counter 3") && n.contains("sent as 0xaa")),
            "{:?}",
            second.notes
        );
        assert_eq!(c.next, 5);
        assert_eq!(c.open.iter().map(|e| e.i).collect::<Vec<_>>(), vec![3, 4]);
    }

    /// H1: a burn interrupted before it was known to be sent, then another burn (another recipient): a fresh counter,
    /// never the interrupted one's; the interrupted one keeps its counter.
    #[test]
    fn an_interrupted_burn_keeps_its_counter_from_another_burn() {
        let mut c = Counters::default();
        let zone = Seen { next: 0, ..Seen::default() };
        assert_eq!(pick(&mut c, &zone, &ask("private /0", 1, 0xe1)).unwrap().counter, Counter::Fresh(0));
        let other = pick(&mut c, &zone, &ask("private /0", 1, 0xe2)).unwrap();
        assert_eq!(other.counter, Counter::Fresh(1));
        assert!(other.notes.iter().any(|n| n.contains("counter 0") && n.contains("interrupted")), "{:?}", other.notes);
        // Another amount, or another holder, is another burn too.
        assert_eq!(pick(&mut c, &zone, &ask("private /0", 2, 0xe1)).unwrap().counter, Counter::Fresh(2));
        assert_eq!(pick(&mut c, &zone, &ask("public /0", 1, 0xe1)).unwrap().counter, Counter::Fresh(3));
    }

    /// The same burn asked again takes its counter again (the same note: at most one is applied); found applied after
    /// an interruption, it is not burnt again; found applied after it was known sent, the same ask is a new burn.
    #[test]
    fn the_same_burn_resumes_or_is_done() {
        let mut c = Counters::default();
        let zone0 = Seen::default();
        let a = ask("private /0", 1, 0xe1);
        assert_eq!(pick(&mut c, &zone0, &a).unwrap().counter, Counter::Fresh(0));
        let again = pick(&mut c, &zone0, &a).unwrap();
        assert!(matches!(&again.counter, Counter::Resume(e) if e.i == 0), "{again:?}");
        assert_eq!(c.next, 1, "resuming reserves nothing new");
        // Applied (it had reached the sequencer when the tool was killed): done, the entry resolved.
        let zone1 = Seen { applied: applied(&[(0, a.recipient)]), next: 1, ..Seen::default() };
        let done = pick(&mut c, &zone1, &a).unwrap();
        assert!(matches!(&done.counter, Counter::Done(e) if e.i == 0), "{done:?}");
        assert!(c.open.is_empty());
        // A burn known sent and applied: the same ask afterwards is a second burn.
        let mut c = Counters::default();
        assert_eq!(pick(&mut c, &zone0, &a).unwrap().counter, Counter::Fresh(0));
        c.open[0].sent = Some("0xbb".into());
        assert_eq!(pick(&mut c, &zone1, &a).unwrap().counter, Counter::Fresh(1));
    }

    /// A refused burn's tag, and a tag another note took (another device of the seed), resolve the entry; their
    /// counters stay used.
    #[test]
    fn refused_and_taken_tags_resolve_and_stay_used() {
        let mut c = Counters::default();
        let zone0 = Seen::default();
        let a = ask("private /0", 1, 0xe1);
        assert_eq!(pick(&mut c, &zone0, &a).unwrap().counter, Counter::Fresh(0));
        assert_eq!(pick(&mut c, &zone0, &ask("public /0", 1, 0xe2)).unwrap().counter, Counter::Fresh(1));
        let zone = Seen {
            applied: applied(&[(1, Address::repeat_byte(0x99))]), // counter 1's tag with another device's note
            refused: applied(&[(0, a.recipient)]),
            next: 2,
        };
        let next = pick(&mut c, &zone, &a).unwrap();
        assert_eq!(next.counter, Counter::Fresh(2), "neither 0 nor 1 again");
        assert!(next.notes.iter().any(|n| n.contains("counter 0") && n.contains("refused")), "{:?}", next.notes);
        assert!(next.notes.iter().any(|n| n.contains("counter 1") && n.contains("another note")), "{:?}", next.notes);
        assert_eq!(c.open.iter().map(|e| e.i).collect::<Vec<_>>(), vec![2]);
    }

    /// The zone moves the counter on (burns of this seed made elsewhere); the journal never moves it back.
    #[test]
    fn the_counter_only_moves_on() {
        let mut c = Counters { next: 7, ..Counters::default() };
        assert_eq!(
            pick(&mut c, &Seen { next: 2, ..Seen::default() }, &ask("private /0", 1, 1)).unwrap().counter,
            Counter::Fresh(7)
        );
        let mut c = Counters { next: 2, ..Counters::default() };
        assert_eq!(
            pick(&mut c, &Seen { next: 9, ..Seen::default() }, &ask("private /0", 1, 1)).unwrap().counter,
            Counter::Fresh(9)
        );
    }

    /// No more than MAX_AHEAD counters past the zone's next one while none is on the zone (seed-only recovery).
    #[test]
    fn at_most_max_ahead_unseen_counters() {
        let mut c = Counters::default();
        let zone = Seen::default();
        for k in 0..MAX_AHEAD {
            assert_eq!(pick(&mut c, &zone, &ask("private /0", 1, k as u8)).unwrap().counter, Counter::Fresh(k));
        }
        let e = pick(&mut c, &zone, &ask("private /0", 1, 0xfe)).unwrap_err().to_string();
        assert!(e.contains("refusing to reserve burn counter 10") && e.contains("seed alone"), "{e}");
        // The same as a reserved one still resumes it.
        assert!(
            matches!(pick(&mut c, &zone, &ask("private /0", 1, 3)).unwrap().counter, Counter::Resume(e) if e.i == 3)
        );
    }

    /// The store: written before sending, under a lock; a stale plan is refused; `sent` recorded; mode 0600; the old
    /// tool's record imported once and removed.
    #[test]
    fn the_store_reserves_once_and_imports_the_old_record() {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join(format!("burns-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let s = Store::new(&dir);
        let a = ask("private /0", 1_000_000_000_000_000, 0xe1);
        let to = a.recipient;
        let n = move |i: u64| note(i, to);
        let zone = Seen::default();
        let k = key(&tag(0));
        // The old record: counter 4 to 0xe2, 0.001 of an 18-decimal token.
        let old = serde_json::json!({"i": 4, "token": Address::ZERO, "amount": "0.001", "recipient": Address::repeat_byte(0xe2), "r": hx(&tag(4)), "at": 1});
        fs::write(dir.join(LEGACY), old.to_string()).unwrap();
        let (legacy_entry, notes) = legacy(&dir, 18, &tag, &|i, _t, amount, r| {
            assert_eq!(amount, 1_000_000_000_000_000);
            note(i, r)
        })
        .unwrap();
        assert!(legacy_entry.is_some() && notes[0].contains("imported"), "{notes:?}");
        let plan = s.peek(&k, &[1; 32], &zone, &a, 5, legacy_entry.clone(), &tag, &n).unwrap();
        assert_eq!(plan.counter, Counter::Fresh(5), "above the old record's counter 4");
        assert!(!s.path().exists(), "a peek writes nothing");
        let got = s.reserve(&k, &[1; 32], &zone, &a, 5, legacy_entry, &tag, &n, &plan.counter).unwrap();
        assert_eq!(got.counter, Counter::Fresh(5));
        assert!(!dir.join(LEGACY).exists(), "the old record imported and removed");
        // Another run planned 5 too, before this one reserved it: refused.
        let stale = s.reserve(&k, &[1; 32], &zone, &ask("private /1", 1, 0xe3), 6, None, &tag, &n, &Counter::Fresh(5));
        assert!(stale.unwrap_err().to_string().contains("another run of the tool reserved burn counter 5"));
        s.sent(&k, 5, "0xcc").unwrap();
        let c = s.counters(&k).unwrap().unwrap();
        assert_eq!(c.next, 6);
        assert_eq!(
            c.open.iter().map(|e| (e.i, e.sent.clone())).collect::<Vec<_>>(),
            vec![(4, None), (5, Some("0xcc".into()))]
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(fs::metadata(s.path()).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // Another seed (another key) in the same home keeps its own counters.
        let other = key(&tag(9));
        let p = s.peek(&other, &[1; 32], &zone, &a, 7, None, &tag, &n).unwrap();
        assert_eq!(p.counter, Counter::Fresh(0));
        let _ = fs::remove_dir_all(&dir);
    }
}
