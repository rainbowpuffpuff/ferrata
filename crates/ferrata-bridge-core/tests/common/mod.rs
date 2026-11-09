//! Deterministic test inputs and proptest strategies shared by the integration tests.
#![allow(dead_code)]

use ferrata_bridge_core::encoding::u256_from_u128;
use ferrata_bridge_core::inbound::{commitment, deposit_leaf, deposit_secret, ClaimWitness, VaultDeposit};
use ferrata_bridge_core::outbound::{BurnNote, ReleasePublic, ReleaseTarget, ReleaseWitness};
use ferrata_bridge_core::tree::Tree;
use ferrata_bridge_core::{outbound, sha256, Address, BridgeKey, Hash, U256};
use proptest::prelude::*;

/// A labelled 32-byte value: `sha256(label)`.
pub fn h(label: &str) -> Hash {
    sha256(label.as_bytes())
}

/// A labelled address: the first 20 bytes of `sha256(label)`.
pub fn addr(label: &str) -> Address {
    h(label)[..20].try_into().unwrap()
}

/// Flips the bits of `mask` (non-zero) in byte `at % len` of `bytes`.
pub fn flip(bytes: &mut [u8], at: usize, mask: u8) {
    assert_ne!(mask, 0);
    let len = bytes.len();
    bytes[at % len] ^= mask;
}

pub fn nonzero_mask() -> impl Strategy<Value = u8> {
    1u8..=255
}

/// A claimable deposit in a tree of random other leaves.
#[derive(Clone, Debug)]
pub struct ClaimCase {
    pub root: Hash,
    pub chain_id: u64,
    pub vault: Address,
    pub witness: ClaimWitness,
}

pub fn claim_case() -> impl Strategy<Value = ClaimCase> {
    (
        prop::collection::vec(any::<[u8; 32]>(), 0..12),
        any::<prop::sample::Index>(),
        any::<[u8; 32]>(),
        any::<u64>(),
        any::<[u8; 20]>(),
        any::<[u8; 20]>(),
        any::<u128>(),
        any::<[u8; 32]>(),
    )
        .prop_map(|(others, at, s, chain_id, vault, token, amount, dest)| {
            let mut leaves = others;
            let leaf_index = at.index(leaves.len() + 1);
            let amount = u256_from_u128(amount);
            let leaf = deposit_leaf(leaf_index as u64, &token, &amount, &commitment(&s, &dest));
            leaves.insert(leaf_index, leaf);
            let tree = Tree::from_leaves(leaves).unwrap();
            ClaimCase {
                root: tree.root(),
                chain_id,
                vault,
                witness: ClaimWitness {
                    token,
                    amount,
                    leaf_index: leaf_index as u64,
                    path: tree.path(leaf_index as u64).unwrap(),
                    s,
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
        any::<[u8; 32]>(),
        (any::<u64>(), any::<[u8; 20]>(), any::<[u8; 32]>()),
        (any::<[u8; 20]>(), any::<u128>(), any::<[u8; 20]>()),
        (any::<[u8; 20]>(), any::<u128>()),
    )
        .prop_map(
            |(
                others,
                at,
                s_out,
                (chain_id, vault, lez_config_id),
                (token, amount, recipient),
                (fee_recipient, fee),
            )| {
                let mut leaves = others;
                let leaf_index = at.index(leaves.len() + 1);
                let public = ReleasePublic {
                    chain_id,
                    vault,
                    lez_config_id,
                    token,
                    amount: u256_from_u128(amount),
                    recipient,
                    fee_recipient,
                    relay_fee: u256_from_u128(fee),
                };
                let c_out = outbound::burn_commitment(&s_out, chain_id, &vault, &recipient);
                leaves.insert(leaf_index, outbound::burn_leaf(leaf_index as u64, &token, &public.amount, &c_out));
                let tree = Tree::from_leaves(leaves).unwrap();
                ReleaseCase {
                    burn_root: tree.root(),
                    witness: ReleaseWitness {
                        leaf_index: leaf_index as u64,
                        path: tree.path(leaf_index as u64).unwrap(),
                        s_out,
                    },
                    public,
                }
            },
        )
}

/// The wallet side of a deposit: `s`, `C` for `(depositor, j)` and destination `dest`.
pub fn own_commitment(
    key: &BridgeKey,
    chain_id: u64,
    vault: &Address,
    depositor: &Address,
    j: u64,
    dest: &Hash,
) -> Hash {
    commitment(&deposit_secret(key, chain_id, vault, depositor, j), dest)
}

/// A burn note of `key`'s `i`-th burn.
pub fn own_burn(
    key: &BridgeKey,
    lez_config_id: &Hash,
    i: u64,
    target: &ReleaseTarget,
    token: Address,
    amount: U256,
) -> BurnNote {
    BurnNote::create(key, lez_config_id, i, target, token, amount)
}

/// Deterministic pseudo-random 32 bytes for synthetic deposits.
fn prng(label: &str, i: u64) -> [u8; 32] {
    sha256(&[label.as_bytes(), &i.to_be_bytes()].concat())
}

/// 100,000 synthetic vault deposits, 7 of them made by `key` from two depositor addresses. Returns
/// the deposits and the positions of the key's own.
pub fn synthetic_deposits(key: &BridgeKey, chain_id: u64, vault: &Address) -> (Vec<VaultDeposit>, Vec<usize>) {
    const TOTAL: u64 = 100_000;
    let ours_at: [u64; 7] = [3, 17_011, 17_012, 50_000, 64_999, 88_888, 99_999];
    let our_addrs = [addr("our depositor A"), addr("our depositor B")];
    let mut our_j = [0u64; 2];
    let tokens = [[0u8; 20], addr("USDC"), addr("DAI")];
    let mut deposits = Vec::with_capacity(TOTAL as usize);
    for leaf_index in 0..TOTAL {
        let token = tokens[(leaf_index % 3) as usize];
        let amount = u256_from_u128(100 * 10u128.pow((leaf_index % 4) as u32));
        if let Some(n) = ours_at.iter().position(|x| *x == leaf_index) {
            let who = n % 2;
            let (depositor, j) = (our_addrs[who], our_j[who]);
            our_j[who] += 1;
            let commitment = own_commitment(key, chain_id, vault, &depositor, j, &dest_for(&depositor, j));
            deposits.push(VaultDeposit { depositor, j, leaf_index, token, amount, commitment });
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
    (deposits, ours_at.iter().map(|x| *x as usize).collect())
}

/// The wallet's destination rule used by the synthetic deposits (any deterministic rule works).
pub fn dest_for(depositor: &[u8; 20], j: u64) -> [u8; 32] {
    sha256(&[b"test dest".as_slice(), depositor, &j.to_be_bytes()].concat())
}
