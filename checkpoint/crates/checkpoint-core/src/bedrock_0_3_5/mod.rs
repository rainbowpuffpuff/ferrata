//! The checkpoint rule for a zone whose Bedrock runs Logos Blockchain 0.3.5-rc.1 (release `f6d4267f`, the revision
//! LEZ v0.4.0-rc3 pins and Logos's devnet runs). Built only with the cargo feature `bedrock-0-3-5`, by the guests of
//! `guests-0-3-5/` (`checkpoint_0_3_5`, `checkpoint_b_0_3_5`). The default build, and with it every guest of
//! `docs/image-ids.md` (the vault on Sepolia pins `checkpoint` and `checkpoint_b`), reads the `d480efc6` layout
//! ([`crate::mantle`]) and does not compile this module.
//!
//! What 0.3.5-rc.1 changes for the program, and so what this build changes, and nothing else:
//! - Every Mantle transaction starts with a 32-byte fork digest, and the transaction hash that every operation's proof
//!   signs covers it: `BLAKE2b-256("MANTLE_TXHASH_V1" ‖ fork_digest ‖ ops)` (logos-blockchain #3954). The program
//!   reads Mantle transactions in three places: `T`'s inscription, rule (A)'s support inscriptions and the channel
//!   configuration changes. Each is framed and hashed with its digest here ([`mantle`]), and its digest must be the
//!   deployment's ([`fork_digest`]): an inscription signed on another chain, or under another era's rules, is refused.
//! - A `ChannelConfigOp` whose `configuration_threshold` exceeds its key count does not decode on 0.3.5-rc.1
//!   (logos-blockchain #3781, `ChannelConfigOp::new`), so Bedrock never applies it; this build refuses it too. The
//!   rotation rule still accepts `threshold(n)` signatures, not the channel's own threshold, and does not check that
//!   Bedrock applied the change (as the default build). (The `d480efc6` node applied such a configuration.)
//!
//! The operations' own encodings are unchanged for the five opcodes the program reads (upstream's codec fixtures of a
//! transfer, an inscription, a channel withdrawal and a channel transfer are byte for byte equal at both revisions; the
//! channel configuration's differs only in its threshold, 21 over two keys at `d480efc6`, 2 at `f6d4267f`:
//! `core/src/mantle/fixtures/ops/op_values.rs`). The rules ([`rule`]) are [`crate::rule`]'s rule (A) and
//! [`crate::rule_b`]'s rule (B) with these checks; the input format ([`crate::input`]: the transactions it carries are
//! now 0.3.5-rc.1's), the configuration and its id, the state and its hash, the burn tree and the 17-word journal are
//! the default build's, unchanged.
//!
//! **Where the deployment pins the fork digest: in `lezConfigId`.** For a zone on 0.3.5-rc.1 the configuration's
//! `bedrockGenesisId` is the zone's fork digest (what `GET /chain/fork` of its node answers), so
//! `lezConfigId = SHA-256("ferrata/config/v1" ‖ forkDigest ‖ channelId ‖ …)` commits to it. Why there:
//! 1. The vault pins `lezConfigId` already (an immutable, checked against the journal's word), so it pins the digest
//!    with no change: FerrataVault v5's code and its 17-word journal serve this build, deployed with this build's two
//!    image ids. A journal word instead would need a new vault.
//! 2. The bridge program binds every burn leaf and every message it accepts to `lezConfigId`, which its deployer
//!    computes with the same v1 preimage: the final bridge program (`docs/image-ids.md`) serves such a zone unchanged.
//!    A new preimage field or tag would give a `lezConfigId` the bridge's deployment tools do not compute.
//! 3. The fork digest is `BLAKE2b-256("FORK_DIGEST_V1" ‖ genesis block id ‖ era digests)` (`core/src/era.rs`): it names
//!    Bedrock's chain more precisely than a genesis id, and unlike the genesis id (which the default build only hashes
//!    into `lezConfigId`) it is checked, on every Bedrock transaction the program reads.
//!
//! What follows from one pinned digest: when Bedrock activates a new era, its node signs with the new era's digest
//! (`/chain/fork` answers the digest of the era its clock is in), and this program refuses the zone's inscriptions from
//! then on; a deployment does not follow a Bedrock era change, and the retirement plan for "a change the deployed
//! checkpoint program cannot follow" applies. Bedrock itself may accept several digests in one era (its own and those
//! of earlier eras back to the last that refuses older transactions, `AcceptedForkDigests`); this program accepts one.

pub mod mantle;
pub mod rule;

use core::fmt;

use crate::state::Config;
use crate::Hash;

/// The fork digest every Bedrock transaction of the zone must carry: the configuration's `bedrockGenesisId` (module
/// docs: where the deployment pins it).
pub const fn fork_digest(config: &Config) -> &Hash {
    &config.bedrock_genesis_id
}

/// Why this build refused an input: a refusal the default build's code makes (parsing, linking, the rule's own
/// checks), or one of the two this build adds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Core(crate::Error),
    /// A Mantle transaction carries another fork digest than the deployment's.
    WrongForkDigest,
    /// A `ChannelConfigOp` whose `configuration_threshold` exceeds its key count (logos-blockchain #3781).
    UnreachableThreshold { threshold: u16, keys: u32 },
}

impl From<crate::Error> for Error {
    fn from(e: crate::Error) -> Self {
        Self::Core(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(e) => write!(f, "{e:?}"),
            Self::WrongForkDigest => write!(f, "a Mantle transaction carries another fork digest than the deployment's"),
            Self::UnreachableThreshold { threshold, keys } => {
                write!(f, "a channel configuration's threshold {threshold} exceeds its {keys} keys")
            }
        }
    }
}
