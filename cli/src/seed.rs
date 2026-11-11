//! The seed file and everything derived from it.
//!
//! The seed file holds the BIP-39 mnemonic of the user's LEZ wallet (LEZ's own format: the wallet
//! derives its 64-byte seed with an empty passphrase, `SeedHolder::from_mnemonic(m, "")`). From it:
//!
//! * the LEZ wallet keys, with LEZ's own key trees (`key_protocol`): public account `/a` and private
//!   account `/a` are the `a`-th children of the roots, as the wallet creates them;
//! * the bridge seed, `HMAC-SHA256(seed64, "ferrata/bridge-seed/v1")`: a label of Ferrata's own,
//!   disjoint from LEZ's `/LEE-Keys/v1/…` and `LEE_seed` labels, so no LEZ key equals a bridge key;
//! * `kAuth` and `kView` from the bridge seed (SPEC-v2 §2, `ferrata_bridge_core::derive_keys`).
//!
//! Every deposit's destination is one of the first `gap` accounts of either kind, so a wallet that
//! kept nothing finds its deposits again from the seed alone (022 F.9, R.5).

use std::{fs, path::Path};

use anyhow::{anyhow, Context as _, Result};
use bip39::Mnemonic;
use ferrata_bridge_core::{derive_keys, hmac_sha256, inbound::DestKind, BridgeKeys, Hash};
use key_protocol::key_management::{
    key_tree::{chain_index::ChainIndex, KeyTreePrivate, KeyTreePublic},
    secret_holders::SeedHolder,
};
use lee::{AccountId, PrivateKey};

/// The label of the bridge seed.
pub const BRIDGE_SEED_LABEL: &[u8] = b"ferrata/bridge-seed/v1";

/// How many accounts of each kind a deposit's destination is searched among, by default.
pub const DEFAULT_GAP: u32 = 8;

/// One public LEZ account of the wallet.
#[derive(Clone)]
pub struct PublicAccount {
    pub index: u32,
    pub id: AccountId,
    pub key: PrivateKey,
}

/// The seed and what the tool derives from it. No `Debug`: it must not end up in a log.
pub struct Seed {
    mnemonic: Mnemonic,
    seed64: [u8; 64],
}

impl Seed {
    /// Reads the mnemonic from `path`. Warns (on stderr) if the file is readable by others.
    pub fn from_file(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("reading the seed file {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            if let Ok(meta) = fs::metadata(path) {
                if meta.permissions().mode() & 0o077 != 0 {
                    eprintln!("warning: the seed file {} is readable by other users (chmod 600 it)", path.display());
                }
            }
        }
        Self::from_phrase(text.trim())
    }

    /// From the mnemonic words.
    pub fn from_phrase(phrase: &str) -> Result<Self> {
        let mnemonic: Mnemonic =
            phrase.parse().map_err(|e| anyhow!("the seed file does not hold a BIP-39 mnemonic: {e}"))?;
        let seed64 = mnemonic.to_seed("");
        Ok(Self { mnemonic, seed64 })
    }

    /// The mnemonic (for LEZ's wallet storage).
    pub fn mnemonic(&self) -> &Mnemonic {
        &self.mnemonic
    }

    /// `HMAC-SHA256(seed64, "ferrata/bridge-seed/v1")`.
    pub fn bridge_seed(&self) -> Hash {
        hmac_sha256(&self.seed64, BRIDGE_SEED_LABEL)
    }

    /// `kAuth` and `kView` as keyed HMAC states.
    pub fn bridge_keys(&self) -> BridgeKeys {
        BridgeKeys::from_bridge_seed(&self.bridge_seed())
    }

    /// `(kAuth, kView)` as bytes (for `keys --show-secrets` and the tests).
    pub fn bridge_key_bytes(&self) -> (Hash, Hash) {
        derive_keys(&self.bridge_seed())
    }

    fn holder(&self) -> SeedHolder {
        SeedHolder::from_mnemonic(&self.mnemonic, "")
    }

    /// Public accounts `/0` … `/(n-1)`, as LEZ's wallet creates them from this seed.
    pub fn public_accounts(&self, n: u32) -> Result<Vec<PublicAccount>> {
        let mut tree = KeyTreePublic::new(&self.holder());
        let mut out = Vec::new();
        for index in 0..n {
            let (id, _) = tree
                .generate_new_public_node(&ChainIndex::root())
                .ok_or_else(|| anyhow!("LEZ's key tree refused public account /{index}"))?;
            let node = tree.get_node(id).ok_or_else(|| anyhow!("public account /{index} is not in the tree"))?;
            let key: &PrivateKey = node.into();
            out.push(PublicAccount { index, id, key: key.clone() });
        }
        Ok(out)
    }

    /// Private account ids `/0` … `/(n-1)`, as LEZ's wallet creates them from this seed.
    pub fn private_account_ids(&self, n: u32) -> Result<Vec<AccountId>> {
        let mut tree = KeyTreePrivate::new(&self.holder());
        let mut out = Vec::new();
        for index in 0..n {
            let chain_index = tree
                .create_private_accounts_key_node(&ChainIndex::root())
                .ok_or_else(|| anyhow!("LEZ's key tree refused private account /{index}"))?;
            let node =
                tree.key_map.get(&chain_index).ok_or_else(|| anyhow!("private account /{index} is not in the tree"))?;
            let (kind, _) =
                node.value.1.first_key_value().ok_or_else(|| anyhow!("private account /{index} has no account"))?;
            out.push(AccountId::for_private_account(
                &node.value.0.nullifier_public_key,
                &node.value.0.viewing_public_key,
                kind,
            ));
        }
        Ok(out)
    }

    /// The candidate destinations of recovery: `(kind, a) -> account id`, `a < gap`.
    pub fn destinations(&self, gap: u32) -> Result<Destinations> {
        let public = self.public_accounts(gap)?.into_iter().map(|a| *a.id.value()).collect();
        let private = self.private_account_ids(gap)?.into_iter().map(|id| *id.value()).collect();
        Ok(Destinations { public, private })
    }
}

/// The wallet's first `gap` destinations of each kind.
#[derive(Clone, Debug)]
pub struct Destinations {
    pub public: Vec<Hash>,
    pub private: Vec<Hash>,
}

impl Destinations {
    pub fn gap(&self) -> u32 {
        self.public.len() as u32
    }

    pub fn of(&self, kind: DestKind, a: u32) -> Hash {
        match kind {
            DestKind::Public => self.public[a as usize],
            DestKind::Private => self.private[a as usize],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BIP-39's all-`abandon` test phrase: a published test value, never a wallet.
    pub const TEST_PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn derivations_are_deterministic_and_disjoint() {
        let a = Seed::from_phrase(TEST_PHRASE).unwrap();
        let b = Seed::from_phrase(TEST_PHRASE).unwrap();
        assert_eq!(a.bridge_seed(), b.bridge_seed());
        let (k_auth, k_view) = a.bridge_key_bytes();
        assert_ne!(k_auth, k_view);
        let pubs = a.public_accounts(3).unwrap();
        assert_eq!(pubs.len(), 3);
        assert_ne!(pubs[0].id, pubs[1].id);
        assert_eq!(pubs[2].id, b.public_accounts(3).unwrap()[2].id);
        let privs = a.private_account_ids(2).unwrap();
        assert_ne!(privs[0], privs[1]);
        assert!(!pubs.iter().any(|p| privs.contains(&p.id)));
        // The bridge seed is not any LEZ secret key.
        assert!(pubs.iter().all(|p| *p.key.value() != a.bridge_seed()));
        assert!(Seed::from_phrase("not a mnemonic").is_err());
    }
}
