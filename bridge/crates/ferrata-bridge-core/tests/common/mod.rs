//! Deterministic test inputs and proptest strategies shared by the integration tests.
#![allow(dead_code)]

use ferrata_bridge_core::encoding::u256_from_u128;
use ferrata_bridge_core::inbound::{commitment, deposit_leaf, deposit_secret, ClaimWitness, DestKind, VaultDeposit};
use ferrata_bridge_core::outbound::{burn_commitment, burn_leaf, ReleasePublic, ReleaseWitness};
use ferrata_bridge_core::tree::{MerklePath, Tree};
use ferrata_bridge_core::{chain_id_u256, sha256, Address, BridgeKey, BridgeKeys, Hash, U256};
use proptest::prelude::*;

/// A labelled 32-byte value: `sha256(label)`.
pub fn h(label: &str) -> Hash {
    sha256(label.as_bytes())
}

/// A labelled address: the first 20 bytes of `sha256(label)`.
pub fn addr(label: &str) -> Address {
    h(label)[..20].try_into().unwrap()
}

/// Wallet keys from a labelled bridge seed.
pub fn keys(label: &str) -> BridgeKeys {
    BridgeKeys::from_bridge_seed(&h(label))
}

pub fn kind_strategy() -> impl Strategy<Value = DestKind> {
    prop_oneof![Just(DestKind::Private), Just(DestKind::Public)]
}

pub fn path_strategy() -> impl Strategy<Value = MerklePath> {
    (any::<u32>(), prop::collection::vec(any::<[u8; 32]>(), 32))
        .prop_map(|(directions, v)| MerklePath { directions, siblings: v.try_into().unwrap() })
}

/// A claimable deposit in a tree of random other leaves.
#[derive(Clone, Debug)]
pub struct ClaimCase {
    pub root: Hash,
    pub chain_id: U256,
    pub vault: Address,
    pub witness: ClaimWitness,
}

pub fn claim_case() -> impl Strategy<Value = ClaimCase> {
    (
        prop::collection::vec(any::<[u8; 32]>(), 0..12),
        any::<prop::sample::Index>(),
        (any::<[u8; 32]>(), kind_strategy(), any::<[u8; 32]>()),
        (any::<[u8; 32]>(), any::<[u8; 20]>()),
        (any::<[u8; 20]>(), any::<[u8; 20]>(), any::<u128>()),
    )
        .prop_map(|(others, at, (s, kind, dest), (chain_id, vault), (depositor, token, amount))| {
            let mut leaves = others;
            let leaf_index = at.index(leaves.len() + 1) as u64;
            let amount = u256_from_u128(amount);
            let leaf = deposit_leaf(leaf_index, &depositor, &token, &amount, &commitment(&s, kind, &dest));
            leaves.insert(leaf_index as usize, leaf);
            let tree = Tree::from_leaves(leaves).unwrap();
            ClaimCase {
                root: tree.root(),
                chain_id,
                vault,
                witness: ClaimWitness {
                    depositor,
                    token,
                    amount,
                    leaf_index,
                    path: tree.path(leaf_index).unwrap(),
                    s,
                    kind,
                    dest,
                },
            }
        })
}

/// A releasable burn in a burn tree of random other leaves.
#[derive(Clone, Debug)]
pub struct ReleaseCase {
    pub burn_root: Hash,
    pub witness: ReleaseWitness,
    pub public: ReleasePublic,
}

pub fn release_case() -> impl Strategy<Value = ReleaseCase> {
    (
        prop::collection::vec(any::<[u8; 32]>(), 0..12),
        any::<prop::sample::Index>(),
        (any::<[u8; 32]>(), any::<[u8; 32]>()),
        (any::<[u8; 32]>(), any::<[u8; 20]>(), any::<[u8; 32]>()),
        (any::<[u8; 20]>(), any::<u128>(), any::<[u8; 20]>()),
        (any::<[u8; 20]>(), any::<u128>()),
    )
        .prop_map(
            |(others, at, (s_out, r), (chain_id, vault, config_id), (token, amount, recipient), (relayer, fee))| {
                let mut leaves = others;
                let burn_index = at.index(leaves.len() + 1) as u64;
                let public = ReleasePublic {
                    chain_id,
                    vault,
                    config_id,
                    token,
                    amount: u256_from_u128(amount),
                    recipient,
                    relayer,
                    relay_fee: u256_from_u128(fee),
                };
                let c_out = burn_commitment(&s_out, &chain_id, &vault, &recipient);
                leaves
                    .insert(burn_index as usize, burn_leaf(&config_id, burn_index, &token, &public.amount, &c_out, &r));
                let tree = Tree::from_leaves(leaves).unwrap();
                ReleaseCase {
                    burn_root: tree.root(),
                    witness: ReleaseWitness { burn_index, path: tree.path(burn_index).unwrap(), s_out, r },
                    public,
                }
            },
        )
}

/// Stand-in for the wallet's seed-derived LEZ account ids: account `a` of kind `kind`.
pub fn dest_of(kind: DestKind, a: u32) -> Hash {
    sha256(&[b"test seed-derived account".as_slice(), &[kind as u8], &a.to_be_bytes()].concat())
}

/// Deterministic pseudo-random 32 bytes for synthetic deposits.
fn prng(label: &str, i: u64) -> [u8; 32] {
    sha256(&[label.as_bytes(), &i.to_be_bytes()].concat())
}

/// An own deposit planted among synthetic ones: its position, kind and account index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Planted {
    pub leaf_index: u64,
    pub kind: DestKind,
    pub account: u32,
}

/// `total` synthetic vault deposits. Eight are made by `auth` from two depositor addresses: seven to
/// destinations inside a gap-5 window (both kinds, account indices 0..5) and one to account 7, which
/// a gap-5 scan must miss. Returns the deposits and the planted ones.
pub fn synthetic_deposits(
    auth: &BridgeKey,
    chain_id: &U256,
    vault: &Address,
    total: u64,
) -> (Vec<VaultDeposit>, Vec<Planted>) {
    let spots = [3, total / 7, total / 7 + 1, total / 2, total * 2 / 3 - 1, total * 8 / 9, total - 2, total - 1];
    let plan = [
        (DestKind::Private, 0u32),
        (DestKind::Public, 0),
        (DestKind::Private, 1),
        (DestKind::Public, 4),
        (DestKind::Private, 4),
        (DestKind::Public, 2),
        (DestKind::Private, 7), // outside a gap-5 window
        (DestKind::Private, 3),
    ];
    let our_addrs = [addr("our depositor A"), addr("our depositor B")];
    let mut our_j = [0u64; 2];
    let tokens = [[0u8; 20], addr("USDC"), addr("DAI")];
    let mut deposits = Vec::with_capacity(total as usize);
    let mut planted = Vec::new();
    for leaf_index in 0..total {
        let token = tokens[(leaf_index % 3) as usize];
        let amount = u256_from_u128(100 * 10u128.pow((leaf_index % 4) as u32));
        if let Some(n) = spots.iter().position(|x| *x == leaf_index) {
            let who = n % 2;
            let (depositor, j) = (our_addrs[who], our_j[who]);
            our_j[who] += 1;
            let (kind, account) = plan[n];
            let s = deposit_secret(auth, chain_id, vault, &depositor, j);
            deposits.push(VaultDeposit {
                depositor,
                j,
                leaf_index,
                token,
                amount,
                commitment: commitment(&s, kind, &dest_of(kind, account)),
            });
            planted.push(Planted { leaf_index, kind, account });
        } else {
            let r = prng("other", leaf_index);
            let depositor: [u8; 20] = r[..20].try_into().unwrap();
            let j = u64::from(r[20] % 4);
            deposits.push(VaultDeposit {
                depositor,
                j,
                leaf_index,
                token,
                amount,
                commitment: prng("other C", leaf_index),
            });
        }
    }
    (deposits, planted)
}

pub fn chain(n: u64) -> U256 {
    chain_id_u256(n)
}
