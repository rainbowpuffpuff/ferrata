//! Voluntary, wallet-side disclosure of one user's bridge activity to a chosen third party
//! (Logos RFP-022, soft requirement 2): "Optional viewing keys allowing a user to *voluntarily*
//! disclose their own bridge activity to a chosen third party, without weakening privacy for
//! anyone else and without any protocol-level disclosure capability."
//!
//! A disclosure is a JSON document the wallet writes from its own keys and public chain data.
//! It carries:
//!
//! * `chainId` and `vault`, the deposit side the disclosure is about;
//! * `kView`, the 32-byte view key, which is exactly what already discloses the user's burns
//!   (the existing scan, [`ferrata_bridge_core::outbound::view_burns`]);
//! * one entry per deposit the wallet recognises as its own: the public leaf fields
//!   (`leafIndex`, `depositor`, `token`, `amount`, `commitment`), the destination the wallet
//!   committed to (`kind`, `dest`), and — only when the deposit's nullifier is spent — the
//!   deposit secret `s` and the claim's `nullifier` (`N`).
//!
//! Why `s` is disclosed only after the claim: `s` is what a claim proves knowledge of. Whoever
//! holds it before the claim can submit the claim itself — to the user's own destination, but
//! naming itself as the relayer (so taking the relay fee) and choosing the timing. Once the
//! nullifier is spent, `s` grants nothing: a refund pays the depositor named in the leaf and
//! needs no `s`, and a release needs `sOut`, which derives from `kAuth` and is never disclosed.
//! An unclaimed entry's `kind` and `dest` are the wallet's declaration, which the third party can
//! neither check nor act on until the claim happens; [`verify`] marks them unverified.
//!
//! Everything here is wallet-side. No guest, no LEZ program, no vault and no existing source file
//! of `ferrata-bridge-core` is involved or changed, and no protocol component can disclose
//! anything: the document is a file the user chooses to hand over. See
//! `docs/privacy/viewing-keys.md` for what the third party learns, cannot do, and what it means
//! for others if it publishes what it learned.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::collections::{HashMap, HashSet};
use std::fmt;

use ferrata_bridge_core::inbound::{self, commitment, deposit_id, deposit_leaf, nullifier, DestKind};
use ferrata_bridge_core::outbound::{view_burns, PublishedBurn};
use ferrata_bridge_core::{derive_keys, Address, BridgeKey, BridgeKeys, Hash, U256};

/// How far past the last used burn-tag counter the burn scan looks. Both the wallet (which knows
/// what it is disclosing) and the third party (whose scan this is) use the same limit.
pub const BURN_SCAN_GAP: u64 = 32;

// ------------------------------------------------------------------------------------------------
// JSON: plain lowercase hex, no prefixes
// ------------------------------------------------------------------------------------------------

/// Serialises a 32-byte value ([`Hash`], [`U256`]) as 64 lowercase hex digits.
mod hex32 {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Lowercase hex, no `0x`.
    pub fn serialize<S: Serializer>(value: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(value))
    }

    /// 64 hex digits.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 32], D::Error> {
        let text = <&str>::deserialize(deserializer)?;
        let bytes = hex::decode(text).map_err(serde::de::Error::custom)?;
        bytes.as_slice().try_into().map_err(|_| serde::de::Error::invalid_length(64, &"64 hex digits (32 bytes)"))
    }
}

/// Serialises a 20-byte address as 40 lowercase hex digits.
mod hex20 {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Lowercase hex, no `0x`.
    pub fn serialize<S: Serializer>(value: &[u8; 20], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(value))
    }

    /// 40 hex digits.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 20], D::Error> {
        let text = <&str>::deserialize(deserializer)?;
        let bytes = hex::decode(text).map_err(serde::de::Error::custom)?;
        bytes.as_slice().try_into().map_err(|_| serde::de::Error::invalid_length(40, &"40 hex digits (20 bytes)"))
    }
}

// ------------------------------------------------------------------------------------------------
// The public view
// ------------------------------------------------------------------------------------------------

/// One deposit of the vault, as its `Deposit` event publishes it: (`depositor`, `index`, `leafIndex`,
/// `token`, `amount`, `commitment`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicDeposit {
    /// Position in the deposit tree.
    pub leaf_index: u64,
    /// `msg.sender` of the deposit.
    pub depositor: Address,
    /// The depositor's counter `depositCount[depositor]` at deposit time.
    pub j: u64,
    /// Token (`0x00…00` = native ETH).
    pub token: Address,
    /// Amount deposited.
    pub amount: U256,
    /// The commitment `C`.
    pub commitment: Hash,
}

/// What anyone can read from the chains: the vault's deposits, the LEZ bridge's spent nullifiers,
/// the burn notes published on LEZ, and the bridge configuration those burns belong to.
#[derive(Debug)]
pub struct PublicView {
    /// The vault's chain id.
    pub chain_id: U256,
    /// The vault's address.
    pub vault: Address,
    /// The LEZ bridge configuration the burn notes belong to (`configId`), public on LEZ.
    pub config_id: Hash,
    /// Every deposit leaf of the vault, in leaf index order.
    pub deposits: Vec<PublicDeposit>,
    /// The nullifiers the LEZ bridge program has spent (recorded claims).
    pub spent_nullifiers: HashSet<Hash>,
    /// The burn notes published on LEZ, with their leaf index in the burn tree.
    pub burns: Vec<PublishedBurn>,
}

// ------------------------------------------------------------------------------------------------
// The wallet side
// ------------------------------------------------------------------------------------------------

/// A wallet's bridge keys together with the raw `kView` bytes the disclosure document carries.
/// Built from the bridge seed exactly as [`BridgeKeys::from_bridge_seed`] builds the keys.
#[derive(Clone)]
pub struct WalletKeys {
    /// The wallet's bridge keys.
    pub keys: BridgeKeys,
    /// The raw view key, as it appears in the document.
    pub k_view: Hash,
}

impl WalletKeys {
    /// Both keys and the raw `kView` from the bridge seed.
    pub fn from_bridge_seed(bridge_seed: &[u8]) -> Self {
        let (k_auth, k_view) = derive_keys(bridge_seed);
        Self { keys: BridgeKeys { auth: BridgeKey::new(&k_auth), view: BridgeKey::new(&k_view) }, k_view }
    }
}

/// The claim fields a disclosed deposit carries once its nullifier is spent: the deposit secret and
/// the nullifier the claim published.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ClaimSecret {
    /// The deposit secret `s`, disclosed only after the claim.
    #[serde(with = "hex32")]
    pub s: Hash,
    /// The spent nullifier `N` of the claim.
    #[serde(with = "hex32")]
    pub nullifier: Hash,
}

/// One deposit of the document: the public leaf fields, the declared destination, and — only for a
/// spent nullifier — the [`ClaimSecret`] that ties the deposit to its claim.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisclosedDeposit {
    /// Position in the deposit tree.
    pub leaf_index: u64,
    /// `msg.sender` of the deposit.
    #[serde(with = "hex20")]
    pub depositor: Address,
    /// Token (`0x00…00` = native ETH).
    #[serde(with = "hex20")]
    pub token: Address,
    /// Amount deposited.
    #[serde(with = "hex32")]
    pub amount: U256,
    /// The commitment `C`, as published in the leaf.
    #[serde(with = "hex32")]
    pub commitment: Hash,
    /// Destination kind byte: 0x01 private, 0x02 public.
    pub kind: u8,
    /// The LEZ destination account id the wallet committed to.
    #[serde(with = "hex32")]
    pub dest: Hash,
    /// Present only when the deposit's nullifier is spent.
    #[serde(default, flatten, skip_serializing_if = "Option::is_none")]
    pub claim: Option<ClaimSecret>,
}

/// The disclosure document: a JSON object the user hands to a third party.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Disclosure {
    /// The vault's chain id.
    #[serde(with = "hex32")]
    pub chain_id: U256,
    /// The vault's address.
    #[serde(with = "hex20")]
    pub vault: Address,
    /// The view key: whoever holds it can read the user's burns with the existing scan.
    #[serde(with = "hex32")]
    pub k_view: Hash,
    /// The user's deposits, in leaf index order.
    pub deposits: Vec<DisclosedDeposit>,
}

/// Builds the disclosure document from the user's own keys and the public view.
///
/// The deposits are found by the wallet's existing recovery scan
/// ([`inbound::recover`]): for every public deposit, `s` from `kAuth` and `C` recomputed over the
/// `2·gap` candidate destinations `dest_of(kind, a)`, `a = 0..gap`, both kinds. The burns need no
/// entry: the document carries `kView`, and [`verify`] runs the same public scan the wallet runs
/// ([`outbound::view_burns`]), so the third party lists them itself.
///
/// `s` and `N` are written only for a deposit whose nullifier is in the public spent set.
pub fn make_disclosure(
    keys: &WalletKeys,
    public_view: &PublicView,
    gap: u32,
    dest_of: impl Fn(DestKind, u32) -> Hash,
) -> Disclosure {
    let deposits: Vec<inbound::VaultDeposit> = public_view
        .deposits
        .iter()
        .map(|d| inbound::VaultDeposit {
            depositor: d.depositor,
            j: d.j,
            leaf_index: d.leaf_index,
            token: d.token,
            amount: d.amount,
            commitment: d.commitment,
        })
        .collect();
    let owned = inbound::recover(&keys.keys.auth, &public_view.chain_id, &public_view.vault, &deposits, gap, dest_of);
    let mut entries: Vec<DisclosedDeposit> = owned
        .into_iter()
        .map(|o| {
            let claimed = public_view.spent_nullifiers.contains(&o.nullifier);
            DisclosedDeposit {
                leaf_index: o.deposit.leaf_index,
                depositor: o.deposit.depositor,
                token: o.deposit.token,
                amount: o.deposit.amount,
                commitment: o.deposit.commitment,
                kind: o.kind as u8,
                dest: o.dest,
                claim: claimed.then_some(ClaimSecret { s: o.s, nullifier: o.nullifier }),
            }
        })
        .collect();
    entries.sort_by_key(|d| d.leaf_index);
    Disclosure { chain_id: public_view.chain_id, vault: public_view.vault, k_view: keys.k_view, deposits: entries }
}

// ------------------------------------------------------------------------------------------------
// The third party's side
// ------------------------------------------------------------------------------------------------

/// Why a disclosure was refused: the error names the entry (by leaf index) and the field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisclosureError {
    /// The document names another chain or vault than the public view (`field`: `"chainId"` or
    /// `"vault"`).
    ContextMismatch {
        /// The mismatched field.
        field: &'static str,
    },
    /// An entry names a leaf index the vault has no deposit at.
    UnknownLeaf {
        /// The declared leaf index.
        leaf_index: u64,
    },
    /// A declared field disagrees with the vault's record, or with itself under recomputation.
    /// Which field is best-effort: the refusal itself is exact. For a claimed entry the checks
    /// are: `s` must open the declared `nullifier` under the public `fid` (else `"s"`, or
    /// `"nullifier"` when the recomputed value is the spent one); `s`, `kind` and `dest` must
    /// recompute the declared `C` (else `"kind"` when the other kind fixes it, `"dest"`
    /// otherwise).
    Mismatch {
        /// The entry's declared leaf index.
        leaf_index: u64,
        /// The field the mismatch is attributed to. `"depositor"`, `"token"`, `"amount"`,
        /// `"commitment"` and `"kind"` are exact; `"s"`, `"nullifier"`, `"dest"` name the pair
        /// or the opening that failed.
        field: &'static str,
    },
    /// An entry is marked claimed, but its nullifier is not in the spent set.
    UnspentNullifier {
        /// The entry's declared leaf index.
        leaf_index: u64,
    },
}

impl fmt::Display for DisclosureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisclosureError::ContextMismatch { field } => {
                write!(f, "the disclosure names another {field} than the public view")
            }
            DisclosureError::UnknownLeaf { leaf_index } => {
                write!(f, "leaf index {leaf_index}: the vault has no deposit there")
            }
            DisclosureError::Mismatch { leaf_index, field } => {
                write!(f, "leaf index {leaf_index}: declared {field} does not match")
            }
            DisclosureError::UnspentNullifier { leaf_index } => {
                write!(f, "leaf index {leaf_index}: marked claimed, but its nullifier is not spent")
            }
        }
    }
}

impl std::error::Error for DisclosureError {}

/// One deposit of the report: what the third party learned about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportedDeposit {
    /// Position in the deposit tree.
    pub leaf_index: u64,
    /// `msg.sender` of the deposit.
    pub depositor: Address,
    /// Token (`0x00…00` = native ETH).
    pub token: Address,
    /// Amount deposited.
    pub amount: U256,
    /// The commitment `C` of the leaf.
    pub commitment: Hash,
    /// Destination kind.
    pub kind: DestKind,
    /// The LEZ destination account id the deposit went to, as declared (and, once verified, as
    /// opened by `s`).
    pub dest: Hash,
    /// Whether the destination is verified: only a claimed entry's `s` can recompute `C`, so an
    /// unclaimed entry's `kind` and `dest` stay the discloser's declaration until the claim.
    pub verified: bool,
    /// The claim's nullifier, present when the deposit is claimed.
    pub nullifier: Option<Hash>,
}

/// One burn of the report: found and opened by the scan under the document's `kView`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportedBurn {
    /// The burn tag counter whose tag the burn used.
    pub i: u64,
    /// Leaf index of the burn leaf.
    pub burn_index: u64,
    /// Token released (`0x00…00` = native ETH).
    pub token: Address,
    /// Amount released.
    pub amount: U256,
    /// The Ethereum recipient, decrypted from the memo (the note's MAC covers it).
    pub recipient: Address,
}

/// What [`verify`] certifies: every entry of the document, checked against the public view, and
/// every burn the document's `kView` opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// The vault's chain id.
    pub chain_id: U256,
    /// The vault's address.
    pub vault: Address,
    /// The document's deposits, checked.
    pub deposits: Vec<ReportedDeposit>,
    /// The user's burns, found by the scan with the document's `kView`.
    pub burns: Vec<ReportedBurn>,
}

/// Verifies a disclosure against the public view, or refuses it with an error naming the entry and
/// the field. The public view is everything the verifier reads from the chains itself; nothing
/// besides the document is taken from the discloser.
///
/// Per claimed entry: the entry's public fields must equal the vault's record at `leafIndex`; the
/// leaf and `fid` are recomputed from public data; `nullifier(s, fid)` must equal the declared
/// `N`, which must be spent; `commitment(s, kind, dest)` must equal the declared (and public) `C`.
/// An unclaimed entry is checked against the vault's record only, and reported unverified.
/// Burns are the existing `kView` scan over the public burn notes.
pub fn verify(disclosure: &Disclosure, public_view: &PublicView) -> Result<Report, DisclosureError> {
    if disclosure.chain_id != public_view.chain_id {
        return Err(DisclosureError::ContextMismatch { field: "chainId" });
    }
    if disclosure.vault != public_view.vault {
        return Err(DisclosureError::ContextMismatch { field: "vault" });
    }
    let public: HashMap<u64, &PublicDeposit> = public_view.deposits.iter().map(|d| (d.leaf_index, d)).collect();
    let mut seen: HashSet<u64> = HashSet::new();
    let mut deposits: Vec<ReportedDeposit> = Vec::new();
    for entry in &disclosure.deposits {
        let leaf_index = entry.leaf_index;
        if !seen.insert(leaf_index) {
            return Err(DisclosureError::Mismatch { leaf_index, field: "leafIndex" });
        }
        let record = public.get(&leaf_index).copied().ok_or(DisclosureError::UnknownLeaf { leaf_index })?;
        for (field, matches) in [
            ("depositor", entry.depositor == record.depositor),
            ("token", entry.token == record.token),
            ("amount", entry.amount == record.amount),
            ("commitment", entry.commitment == record.commitment),
        ] {
            if !matches {
                return Err(DisclosureError::Mismatch { leaf_index, field });
            }
        }
        let kind = DestKind::from_byte(entry.kind).ok_or(DisclosureError::Mismatch { leaf_index, field: "kind" })?;
        let reported = |verified: bool, nullifier: Option<Hash>| ReportedDeposit {
            leaf_index,
            depositor: record.depositor,
            token: record.token,
            amount: record.amount,
            commitment: record.commitment,
            kind,
            dest: entry.dest,
            verified,
            nullifier,
        };
        let Some(claim) = &entry.claim else {
            deposits.push(reported(false, None));
            continue;
        };
        // fid from public data alone; the declared s must open the declared N under it.
        let leaf = deposit_leaf(leaf_index, &record.depositor, &record.token, &record.amount, &record.commitment);
        let fid = deposit_id(&disclosure.chain_id, &disclosure.vault, leaf_index, &leaf);
        let recomputed_n = nullifier(&claim.s, &fid);
        if recomputed_n != claim.nullifier {
            let field = if public_view.spent_nullifiers.contains(&recomputed_n) { "nullifier" } else { "s" };
            return Err(DisclosureError::Mismatch { leaf_index, field });
        }
        // s, kind and dest must open the declared (and public) C.
        if commitment(&claim.s, kind, &entry.dest) != entry.commitment {
            let other = if kind == DestKind::Private { DestKind::Public } else { DestKind::Private };
            let field = if commitment(&claim.s, other, &entry.dest) == entry.commitment { "kind" } else { "dest" };
            return Err(DisclosureError::Mismatch { leaf_index, field });
        }
        if !public_view.spent_nullifiers.contains(&claim.nullifier) {
            return Err(DisclosureError::UnspentNullifier { leaf_index });
        }
        deposits.push(reported(true, Some(claim.nullifier)));
    }
    let (viewed, _) =
        view_burns(&BridgeKey::new(&disclosure.k_view), &public_view.config_id, &public_view.burns, BURN_SCAN_GAP);
    let burns: Vec<ReportedBurn> = viewed
        .into_iter()
        .map(|v| ReportedBurn {
            i: v.i,
            burn_index: v.burn.burn_index,
            token: v.burn.note.token,
            amount: v.burn.note.amount,
            recipient: v.recipient,
        })
        .collect();
    Ok(Report { chain_id: disclosure.chain_id, vault: disclosure.vault, deposits, burns })
}
