//! Which bridge program a zone runs, and the claim layout of the one the test network still runs.
//!
//! This tree's bridge program is `ferrata_bridge_f13` (ADR 0046): a claim names no fee schedule, and the vault took
//! the mint fee on Ethereum at the deposit. The test network still runs `ferrata_bridge_b1`, the program before ADR
//! 0046 (`idl/deployed/ferrata_bridge_b1.idl.json`): its `Claim` and `ClaimPrivately` carry a [`ScheduleRange`]
//! between the witness and the relayer, its token actor checks that range against the imported fee schedules and
//! mints the schedule's mint fee to the fee recipient on LEZ. Nothing else a wallet sends or reads differs between
//! the two: `Burn`, `ImportRoot` and every account state are encoded alike (a holder's `pending` claim, the one state
//! that names a range in B1, lives only within one transaction).
//!
//! [`BridgeProgram::of_image_id`] tells the two apart by the image id the zone's loader header names for the bridge
//! program's account; an image id this wallet does not know is refused before anything is built, signed or sent.
//! [`b1_claim`] turns a claim this tree's builder made into B1's bytes, with the checks B1 adds (the range covers the
//! leaf and is one of the token's imported schedules), so a claim to the test network is refused here with the
//! program's own code instead of being refused, or misread, there. [`decode_claim`] reads a plaintext claim of
//! either layout from a zone's blocks.

use borsh::{BorshDeserialize, BorshSerialize};
use ferrata_lez_bridge_wire::{self as bw, code, BridgeMessage, ClaimWitness, Root, TokenState};

use crate::Refusal;

/// `ferrata_bridge_f13`'s image id: this tree's bridge program (`docs/image-ids.md`, ADR 0046).
pub const F13_IMAGE_ID: [u8; 32] = hex32("ffb47f6eaec6ef13f49505e5e80209f73d38ea53792e77656994f8069dde7675");
/// `ferrata_bridge_b1`'s image id: the program the test network runs (`docs/image-ids.md`, "Superseded";
/// `scripts/reproduce/deployed-programs.json`).
pub const B1_IMAGE_ID: [u8; 32] = hex32("847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515");

const fn hex32(s: &str) -> [u8; 32] {
    const fn nibble(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => panic!("not a lowercase hex digit"),
        }
    }
    let b = s.as_bytes();
    assert!(b.len() == 64, "an image id is 64 hex digits");
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = nibble(b[2 * i]) << 4 | nibble(b[2 * i + 1]);
        i += 1;
    }
    out
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A bridge program this wallet can build claims for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeProgram {
    /// This tree's program (ADR 0046): claims name no fee schedule.
    F13,
    /// The test network's program: claims name a fee schedule's range.
    B1,
}

impl BridgeProgram {
    /// The program whose image id the zone's loader header names, or why none: an unknown image id is refused, never
    /// assumed to read one of the two layouts.
    pub fn of_image_id(image_id: &[u8; 32]) -> Result<Self, String> {
        if *image_id == F13_IMAGE_ID {
            Ok(BridgeProgram::F13)
        } else if *image_id == B1_IMAGE_ID {
            Ok(BridgeProgram::B1)
        } else {
            Err(format!(
                "refused before anything was signed or sent: the zone's bridge program runs image 0x{}, which this ferrata \
                 does not know; it builds claims for ferrata_bridge_f13 (0x{}, this tree's program, ADR 0046) and for \
                 ferrata_bridge_b1 (0x{}, the program our test network runs)",
                hex(image_id),
                hex(&F13_IMAGE_ID),
                hex(&B1_IMAGE_ID)
            ))
        }
    }

    pub fn image_id(self) -> [u8; 32] {
        match self {
            BridgeProgram::F13 => F13_IMAGE_ID,
            BridgeProgram::B1 => B1_IMAGE_ID,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BridgeProgram::F13 => "ferrata_bridge_f13",
            BridgeProgram::B1 => "ferrata_bridge_b1",
        }
    }
}

/// The image id a zone's program loader names in its header for a program's account, from the loader's actor state
/// there: LEZ v0.4.0-rc3's `LoaderEntry::Header` (a 0 tag, then the header) or rc1's bare `ProgramHeader` (the image
/// id, the first segment's account, the immutable flag; 65 bytes). `None` for anything else (no state, an rc3
/// segment). rc1's header is told by its length alone, so read this from the program's own account only (where the
/// loader keeps the header): 65 bytes of another account would parse, and the id they give, matching no program this
/// crate knows, is refused by the caller, never sent to.
pub fn loader_header_image_id(state: &[u8]) -> Option<[u8; 32]> {
    let header = match state.len() {
        66 if state[0] == 0 => &state[1..],
        65 => state,
        _ => return None,
    };
    if header[64] > 1 {
        return None;
    }
    header[..32].try_into().ok()
}

/// A RISC Zero image id's bytes from LEZ's `ProgramId` (eight little-endian words, as its loader header holds it).
pub fn program_id_bytes(words: &[u32; 8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (chunk, w) in out.chunks_exact_mut(4).zip(words) {
        chunk.copy_from_slice(&w.to_le_bytes());
    }
    out
}

/// B1's `ScheduleRange`: the claim says its leaf lies in `[from_leaf, until_leaf)`, under schedule `index`. A private
/// claim on B1 publishes this range (its `Issue` to the token actor is public), so the range, not the root alone,
/// bounds its anonymity set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ScheduleRange {
    pub index: u64,
    pub from_leaf: u64,
    pub until_leaf: u64,
}

impl ScheduleRange {
    pub fn covers(&self, leaf_index: u64) -> bool {
        leaf_index >= self.from_leaf && leaf_index < self.until_leaf
    }
}

/// The range a B1 claim of leaf `leaf` names: the widest the token state allows (B1's wallet rule, as this crate had
/// it before ADR 0046): the last schedule starting at or before the leaf, up to the next schedule's start or, for the
/// last, the leaf count the imported schedules are known to cover.
pub fn range_for(token: &TokenState, leaf: u64) -> ScheduleRange {
    if token.schedules.is_empty() {
        return ScheduleRange { index: 0, from_leaf: 0, until_leaf: u64::MAX };
    }
    let j = token.schedules.iter().rposition(|s| s.from_leaf <= leaf).unwrap_or(0);
    let until = token.schedules.get(j + 1).map_or(token.known_until, |s| s.from_leaf);
    ScheduleRange { index: j as u64, from_leaf: token.schedules[j].from_leaf, until_leaf: until }
}

/// B1's `TokenState::fees_for`: the mint fee of denomination `amount` under `range`'s schedule, or the code B1's
/// token actor refuses the claim's `Issue` with (`NOT_REGISTERED`, `LEAF_NOT_IMPORTED`).
pub fn mint_fee(token: &TokenState, amount: u128, range: &ScheduleRange) -> Result<u128, u8> {
    let d = token.entry.index_of(amount).ok_or(code::NOT_REGISTERED)?;
    let index = usize::try_from(range.index).map_err(|_| code::LEAF_NOT_IMPORTED)?;
    let schedule = token.schedules.get(index).ok_or(code::LEAF_NOT_IMPORTED)?;
    let bound = token.schedules.get(index + 1).map_or(token.known_until, |next| next.from_leaf);
    if range.from_leaf != schedule.from_leaf || range.until_leaf > bound || range.until_leaf <= range.from_leaf {
        return Err(code::LEAF_NOT_IMPORTED);
    }
    schedule.mint_fees.get(d).copied().ok_or(code::NOT_REGISTERED)
}

/// The fields of B1's `Claim` (variant 5) and `ClaimPrivately` (variant 6), in their order.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct B1ClaimFields {
    pub config_id: Root,
    pub leaf_count: u64,
    pub witness: ClaimWitness,
    pub range: ScheduleRange,
    pub relayer: Option<Root>,
}

const CLAIM: u8 = 5;
const CLAIM_PRIVATELY: u8 = 6;

/// `message` (a `Claim` or `ClaimPrivately` of this tree's layout) in B1's layout with `range`; `None` for any other
/// message (those are encoded alike by both programs).
pub fn encode_b1(message: &BridgeMessage, range: &ScheduleRange) -> Option<Vec<u8>> {
    let (variant, config_id, leaf_count, witness, relayer) = match message {
        BridgeMessage::Claim { config_id, leaf_count, witness, relayer } => {
            (CLAIM, config_id, leaf_count, witness, relayer)
        }
        BridgeMessage::ClaimPrivately { config_id, leaf_count, witness, relayer } => {
            (CLAIM_PRIVATELY, config_id, leaf_count, witness, relayer)
        }
        _ => return None,
    };
    let fields = B1ClaimFields {
        config_id: *config_id,
        leaf_count: *leaf_count,
        witness: witness.clone(),
        range: *range,
        relayer: *relayer,
    };
    let mut out = vec![variant];
    out.extend(borsh::to_vec(&fields).expect("borsh writes to a vector"));
    Some(out)
}

/// A claim built for B1: its bytes, the range it names and the mint fee B1 mints to the fee recipient.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct B1Claim {
    pub bytes: Vec<u8>,
    pub range: ScheduleRange,
    pub mint_fee: u128,
}

/// The claim `message`, built and checked by this tree's builder ([`crate::claim::build_claim`]), for B1: the range
/// B1's wallet names for its leaf, the checks B1 makes of it (the leaf inside the range, the range one of the token's
/// imported schedules: `LEAF_NOT_IMPORTED`), and B1's bytes. Every other check is the builder's, which B1 makes alike.
pub fn b1_claim(message: &BridgeMessage, token: &TokenState) -> Result<B1Claim, Refusal> {
    let witness = match message {
        BridgeMessage::Claim { witness, .. } | BridgeMessage::ClaimPrivately { witness, .. } => witness,
        _ => return Err(Refusal::wallet("a claim plan holds a claim")),
    };
    let range = range_for(token, witness.leaf_index);
    if !range.covers(witness.leaf_index) {
        return Err(Refusal::program(
            code::LEAF_NOT_IMPORTED,
            format!(
                "checked before sending: the test network's program (ferrata_bridge_b1) needs the deposit's leaf {} inside an \
                 imported fee schedule's range, and the token's imported schedules cover leaves below {} only; wait for \
                 the next fee import",
                witness.leaf_index, token.known_until
            ),
        ));
    }
    let amount = bw::u128_of(&witness.amount)
        .ok_or_else(|| Refusal::program(code::NOT_REGISTERED, "checked before sending: the amount is above 2^128"))?;
    let mint_fee = mint_fee(token, amount, &range).map_err(|code| {
        Refusal::program(code, "checked before sending: the test network's program (ferrata_bridge_b1) checks the fee range")
    })?;
    let bytes = encode_b1(message, &range).expect("a claim");
    Ok(B1Claim { bytes, range, mint_fee })
}

/// A plaintext or private claim as a zone's block holds it, of either program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedClaim {
    pub private: bool,
    pub config_id: Root,
    pub leaf_count: u64,
    pub witness: ClaimWitness,
    /// `Some` for B1's layout.
    pub range: Option<ScheduleRange>,
    pub relayer: Option<Root>,
}

/// A `Claim` or `ClaimPrivately` from its bytes, in this tree's layout or B1's: the two never both decode (each
/// decoder must consume every byte, and B1's carries 24 more before the relayer), so a message reads as at most one.
pub fn decode_claim(bytes: &[u8]) -> Option<DecodedClaim> {
    match BridgeMessage::try_from_slice(bytes) {
        Ok(BridgeMessage::Claim { config_id, leaf_count, witness, relayer }) => {
            return Some(DecodedClaim { private: false, config_id, leaf_count, witness, range: None, relayer })
        }
        Ok(BridgeMessage::ClaimPrivately { config_id, leaf_count, witness, relayer }) => {
            return Some(DecodedClaim { private: true, config_id, leaf_count, witness, range: None, relayer })
        }
        Ok(_) => return None,
        Err(_) => {}
    }
    let (&variant, rest) = bytes.split_first()?;
    let private = match variant {
        CLAIM => false,
        CLAIM_PRIVATELY => true,
        _ => return None,
    };
    let f = B1ClaimFields::try_from_slice(rest).ok()?;
    Some(DecodedClaim {
        private,
        config_id: f.config_id,
        leaf_count: f.leaf_count,
        witness: f.witness,
        range: Some(f.range),
        relayer: f.relayer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    fn token(schedules: &[(u64, u128)], known_until: u64) -> TokenState {
        let unlimited = bw::CapParams { capacity: u128::MAX, refill_per_second: u128::MAX };
        TokenState {
            config_id: [1; 32],
            evm_chain_id: 11_155_111,
            vault: [2; 20],
            attestation: lee_core::account::AccountId::new([3; 32]),
            instance_id: [4; 32],
            entry: bw::TokenEntry {
                token: [5; 20],
                lez_definition: [6; 32],
                decimals: 18,
                lez_unit: 1,
                unit_weight: 1,
                denominations: vec![1_000],
                relay_fees: vec![7],
            },
            schedules: schedules
                .iter()
                .enumerate()
                .map(|(index, &(from_leaf, fee))| bw::FeeSchedule {
                    index: index as u64,
                    from_leaf,
                    announced_at: 0,
                    effective_at: 0,
                    mint_fees: vec![fee],
                    burn_fees: vec![0],
                    checkpoint_fees: vec![0],
                })
                .collect(),
            known_until,
            controls: bw::Controls {
                admin: [8; 32],
                freeze_authority: [9; 32],
                fee_recipient: [10; 32],
                floor: bw::CapParams { capacity: 1, refill_per_second: 1 },
                token_cap: unlimited,
                global_cap: unlimited,
            },
            gate: bw::Gate::new(unlimited, 0),
            pending: None,
            delisted: false,
            listing_slot: 0,
        }
    }

    fn witness(leaf_index: u64) -> ClaimWitness {
        let mut amount = [0u8; 32];
        amount[16..].copy_from_slice(&1_000u128.to_be_bytes());
        ClaimWitness {
            depositor: [11; 20],
            token: [5; 20],
            amount,
            leaf_index,
            path: vec![[12; 32]; 32],
            s: [13; 32],
            kind: 2,
            dest: [14; 32],
        }
    }

    /// The loader header as LEZ v0.4.0-rc1 writes it (`ProgramHeader`'s own bytes) and as rc3 does (its
    /// `LoaderEntry::Header`, tag 0), and what is not a header.
    #[test]
    fn the_loader_header_names_the_image_in_both_lez_layouts() {
        let mut words = [0u32; 8];
        for (i, w) in words.iter_mut().enumerate() {
            *w = u32::from_le_bytes(B1_IMAGE_ID[4 * i..4 * i + 4].try_into().unwrap());
        }
        assert_eq!(program_id_bytes(&words), B1_IMAGE_ID);
        let header = lee_core::program::ProgramHeader {
            image_id: words,
            program_first_segment: lee_core::account::AccountId::new([0x33; 32]),
            immutable: true,
        };
        let rc1 = borsh::to_vec(&header).unwrap(); // `ProgramHeader`'s own bytes (rc1's `to_bytes`)
        assert_eq!(rc1.len(), 65);
        assert_eq!(loader_header_image_id(&rc1), Some(B1_IMAGE_ID));
        let mut rc3 = vec![0u8];
        rc3.extend_from_slice(&rc1);
        assert_eq!(loader_header_image_id(&rc3), Some(B1_IMAGE_ID));
        let mut segment = rc3.clone();
        segment[0] = 1;
        assert_eq!(loader_header_image_id(&segment), None, "a segment is not a header");
        assert_eq!(loader_header_image_id(&[]), None, "no loader state: not a deployed program");
        assert_eq!(loader_header_image_id(&rc1[..64]), None);
        let mut bad_flag = rc1.clone();
        bad_flag[64] = 2;
        assert_eq!(loader_header_image_id(&bad_flag), None);
    }

    #[test]
    fn the_image_ids_tell_the_programs_apart_and_refuse_any_other() {
        assert_eq!(BridgeProgram::of_image_id(&F13_IMAGE_ID), Ok(BridgeProgram::F13));
        assert_eq!(BridgeProgram::of_image_id(&B1_IMAGE_ID), Ok(BridgeProgram::B1));
        let other = [0x5a; 32];
        let e = BridgeProgram::of_image_id(&other).unwrap_err();
        assert!(e.contains(&hex(&other)) && e.contains("does not know"), "{e}");
        assert_eq!(BridgeProgram::B1.image_id(), B1_IMAGE_ID);
        assert_eq!(BridgeProgram::F13.name(), "ferrata_bridge_f13");
    }

    /// The constants are the ids `docs/image-ids.md` records (this tree's program) and the deployed list names (B1).
    #[test]
    fn the_image_ids_are_the_recorded_ones() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        let ids = std::fs::read_to_string(format!("{root}/docs/image-ids.md")).unwrap();
        let f13 = ids
            .lines()
            .find(|l| l.contains("`ferrata_bridge_f13` |"))
            .expect("docs/image-ids.md has ferrata_bridge_f13's row");
        assert!(f13.contains(&format!("`0x{}`", hex(&F13_IMAGE_ID))), "{f13}");
        let deployed: serde_like::Programs = serde_like::programs(&format!("{root}/scripts/reproduce/deployed-programs.json"));
        assert_eq!(deployed.image_id("ferrata_bridge_b1"), format!("0x{}", hex(&B1_IMAGE_ID)));
    }

    /// A small reader for `deployed-programs.json` (this crate has no JSON dependency without `prove`).
    mod serde_like {
        pub struct Programs(String);
        pub fn programs(path: &str) -> Programs {
            Programs(std::fs::read_to_string(path).unwrap())
        }
        impl Programs {
            pub fn image_id(&self, name: &str) -> String {
                let at = self.0.find(&format!("\"name\": \"{name}\"")).expect("the program is listed");
                let rest = &self.0[at..];
                let key = "\"image_id\": \"";
                let start = rest.find(key).expect("its image id") + key.len();
                rest[start..start + 66].to_string()
            }
        }
    }

    /// B1's frozen encoding vectors (`idl/deployed/ferrata_bridge_b1.vectors.json`): each `Claim` and `ClaimPrivately`
    /// there is this tree's message (the same bytes without the 24-byte range) re-encoded with its range {1, 10, 20};
    /// the B1 bytes do not decode as this tree's message, and [`decode_claim`] reads both layouts back.
    #[test]
    fn b1s_frozen_claim_vectors_are_this_trees_claims_with_the_range() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        let vectors = std::fs::read_to_string(format!("{root}/idl/deployed/ferrata_bridge_b1.vectors.json")).unwrap();
        let range = ScheduleRange { index: 1, from_leaf: 10, until_leaf: 20 };
        let mut seen = 0;
        for name in ["BridgeMessage::Claim#1", "BridgeMessage::Claim#2", "BridgeMessage::ClaimPrivately#1"] {
            let at = vectors.find(&format!("\"name\": \"{name}\"")).unwrap_or_else(|| panic!("{name} is a vector"));
            let rest = &vectors[at..];
            let start = rest.find("\"hex\": \"").unwrap() + 8;
            let end = start + rest[start..].find('"').unwrap();
            let b1 = unhex(&rest[start..end]);
            // The range starts after the variant, the configuration, the leaf count and the witness, whose length
            // follows from its path's length (borsh: a u32 count after depositor, token, amount and leaf index).
            let w_at = 1 + 32 + 8;
            let n_path = u32::from_le_bytes(b1[w_at + 80..w_at + 84].try_into().unwrap()) as usize;
            let cut = w_at + 20 + 20 + 32 + 8 + 4 + 32 * n_path + 32 + 1 + 32;
            let mut f13 = b1[..cut].to_vec();
            f13.extend_from_slice(&b1[cut + 24..]);
            let message = BridgeMessage::try_from_slice(&f13).unwrap_or_else(|e| panic!("{name} without its range: {e}"));
            assert!(matches!(message, BridgeMessage::Claim { .. } | BridgeMessage::ClaimPrivately { .. }));
            assert_eq!(encode_b1(&message, &range).unwrap(), b1, "{name}");
            assert!(BridgeMessage::try_from_slice(&b1).is_err(), "{name}: B1's bytes must not decode as this tree's");
            let back = decode_claim(&b1).unwrap();
            assert_eq!(back.range, Some(range));
            assert_eq!(back.private, name.contains("Privately"));
            let back = decode_claim(&f13).unwrap();
            assert_eq!(back.range, None);
            seen += 1;
        }
        assert_eq!(seen, 3);
    }

    #[test]
    fn the_range_is_b1s_widest_and_its_checks_are_b1s() {
        // Schedules from leaves 0 (fee 3) and 10 (fee 4), known up to leaf 20.
        let t = token(&[(0, 3), (10, 4)], 20);
        assert_eq!(range_for(&t, 4), ScheduleRange { index: 0, from_leaf: 0, until_leaf: 10 });
        assert_eq!(range_for(&t, 10), ScheduleRange { index: 1, from_leaf: 10, until_leaf: 20 });
        assert_eq!(mint_fee(&t, 1_000, &range_for(&t, 4)), Ok(3));
        assert_eq!(mint_fee(&t, 1_000, &range_for(&t, 15)), Ok(4));
        assert_eq!(mint_fee(&t, 999, &range_for(&t, 4)), Err(code::NOT_REGISTERED));
        // A range that does not start at its schedule, or runs past the next one, is B1's LEAF_NOT_IMPORTED.
        assert_eq!(mint_fee(&t, 1_000, &ScheduleRange { index: 0, from_leaf: 1, until_leaf: 10 }), Err(code::LEAF_NOT_IMPORTED));
        assert_eq!(mint_fee(&t, 1_000, &ScheduleRange { index: 0, from_leaf: 0, until_leaf: 11 }), Err(code::LEAF_NOT_IMPORTED));
        assert_eq!(mint_fee(&t, 1_000, &ScheduleRange { index: 2, from_leaf: 20, until_leaf: 30 }), Err(code::LEAF_NOT_IMPORTED));

        let claim = |leaf| BridgeMessage::Claim { config_id: [1; 32], leaf_count: 30, witness: witness(leaf), relayer: None };
        let built = b1_claim(&claim(12), &t).unwrap();
        assert_eq!(built.range, ScheduleRange { index: 1, from_leaf: 10, until_leaf: 20 });
        assert_eq!(built.mint_fee, 4);
        assert_eq!(decode_claim(&built.bytes).unwrap().range, Some(built.range));
        // Leaf 25 is past what the imported schedules cover: refused here, as B1 would refuse it.
        match b1_claim(&claim(25), &t) {
            Err(Refusal::Program { code: c, detail }) => {
                assert_eq!(c, code::LEAF_NOT_IMPORTED);
                assert!(detail.contains("ferrata_bridge_b1") && detail.contains("below 20"), "{detail}");
            }
            other => panic!("{other:?}"),
        }
        // No schedule imported at all: B1 refuses every claim of the token.
        let none = token(&[], 0);
        assert!(matches!(b1_claim(&claim(0), &none), Err(Refusal::Program { code: c, .. }) if c == code::LEAF_NOT_IMPORTED));
        // A private claim keeps its variant; anything but a claim is not B1's to re-encode.
        let private = BridgeMessage::ClaimPrivately { config_id: [1; 32], leaf_count: 30, witness: witness(3), relayer: Some([9; 32]) };
        let built = b1_claim(&private, &t).unwrap();
        assert_eq!(built.bytes[0], CLAIM_PRIVATELY);
        let back = decode_claim(&built.bytes).unwrap();
        assert!(back.private && back.relayer == Some([9; 32]) && back.witness == witness(3));
        assert_eq!(encode_b1(&BridgeMessage::Read, &built.range), None);
        assert!(b1_claim(&BridgeMessage::Read, &t).is_err());
    }
}
