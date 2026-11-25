//! End to end: the vault on a local anvil (chain id 1, never a public network), the zone in the
//! LEZ harness (LEZ's state transition with the bridge guest, `RISC0_DEV_MODE=1`).
//!
//! Ethereum side through the `ferrata` binary itself (deposits signed from a keystore file, the
//! scan from the anvil RPC) and the library; LEZ side through the library's calls, checked with the
//! program's own functions, then settled by the harness (`HarnessLedger` below: the harness signs
//! and assembles the transaction the way LEZ's wallet does on a sequencer).
//!
//! Stand-ins, named: the root import, the configuration's `Init` and the token's `Register` are
//! the relayer's and run through the harness's own calls, from a synthetic execution block that
//! holds the anvil vault's real storage (read with eth_getStorageAt); the checkpoint and the
//! Groth16 proofs are the vault tests' MockRiscZeroVerifier accepting the exact journals (the test
//! acts as the checkpoint prover and the wrapper, as `VaultTestBase._acceptRootOn` does).
//!
//! Runs only with `FERRATA_E2E=1`, `anvil` and `forge` on the path, and `vault/lib` installed
//! (`cli/tests/e2e.sh` does all of it).

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use alloy::{
    network::TransactionBuilder as _,
    primitives::{Address, Bytes, B256, U256},
    providers::{DynProvider, Provider},
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
    sol,
    sol_types::{sol_data, SolCall, SolType},
};
use anyhow::{anyhow, bail, ensure, Context as _, Result};
use borsh::BorshDeserialize as _;
use ferrata_bridge_core::{inbound::DestKind, outbound::burn_leaf, sha256, tree::Tree, Hash};
use ferrata_cli::{
    burns, codes, eth,
    eth::IVault,
    flow, hx,
    lez::{self, Applied, BurnFrom, Context, Ledger, LezCall, Mention},
    release, scan,
    seed::{PrivateSecrets, Seed},
};
use ferrata_lez_bridge_wire::{fee_slot, registry_slot, BridgeMessage, BridgeState, FeeSchedule, HolderState, Holding};
use harness::{bridge as hb, private::PrivateAccount, TxReport};
use lee::{test_utils::TestPrivateKeys, AccountId, PrivateKey, PublicAccountEvidence};
use lee_core::account::Actor;

sol! {
    #[allow(missing_docs)]
    interface IMock {
        function accept(bytes32 imageId, bytes32 journalDigest) external;
    }
    #[allow(missing_docs)]
    interface IAdmin {
        struct CapParams { uint256 capacity; uint256 refillPerSecond; }
        struct TokenParams {
            uint256[] denominations; uint256[] relayFees; uint256 lezUnit; uint256 unitWeight; CapParams depositCap;
            CapParams releaseCap; uint256 releaseDelay; bytes32 lezDefinition; uint8 decimals; uint256[] checkpointFees;
        }
        function registerToken(address token, TokenParams p) external;
        function mint(address to, uint256 amount) external;
        function balanceOf(address who) external view returns (uint256);
    }
    #[allow(missing_docs)]
    interface ICheckpoints {
        struct Checkpoint {
            uint32 nMin; uint32 tMin; bytes32 parentState; bytes32 newState; bytes32 burnRoot; uint64 burnCount;
            uint64 importedLeafCount; bool sunset; bytes32 echoNonce; uint64 tipId; bytes32 tipHash; uint32 n; uint32 t;
        }
        function submitCandidate(Checkpoint c, bytes seal) external;
        function recordBeacon() external returns (uint256);
        function beacons(uint256 i) external view returns (uint256 timestamp, bytes32 nonce);
        function beaconCount() external view returns (uint256);
        function finalize(uint256 beaconIndex, Checkpoint c, bytes seal) external;
        function finalState() external view returns (bytes32);
        function checkpointImageId() external view returns (bytes32);
        function setTokenCap(address token, uint8 direction, uint256 capacity, uint256 refillPerSecond) external;
        function releaseImageId() external view returns (bytes32);
    }
    #[allow(missing_docs)]
    interface IFreeze {
        function setDepositsFrozen(bool frozen) external;
        function setReleasesFrozen(bool frozen) external;
        function setTokenDepositsFrozen(address token, bool frozen) external;
        function setTokenReleasesFrozen(address token, bool frozen) external;
    }
}

/// BIP-39 test phrases (published test values, never wallets): the user, and another user.
const USER: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const OTHER: &str = "legal winner thank year wave sausage worth useful legal winner thank yellow";
/// anvil's first default account (unlocked on anvil; it signs nothing of the user's).
const ANVIL0: Address = alloy::primitives::address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
const SEAL: [u8; 4] = [0x5e, 0xa1, 0x5e, 0xa1];
/// The LEZ fee a private transaction pays from the private account's native balance, as the
/// harness's own private tests reserve it.
const RESERVE: u128 = 100_000_000;
const ETH_1: u128 = 1_000_000_000_000_000_000;

// ---- the zone: the harness behind the tool's `Ledger` ------------------------------------------

struct HarnessLedger {
    world: hb::World,
    applied: Vec<Applied>,
    refused: Vec<Applied>,
    keys: HashMap<AccountId, PrivateKey>,
    /// The wallet's private accounts: their secret keys (ask, d, z) and their state as LEZ's wallet
    /// holds it after decrypting it from the zone (stand-in: the harness keeps the state the chain
    /// committed, checked against the commitment after every private transaction).
    private: HashMap<AccountId, (PrivateSecrets, lee::Account)>,
    /// Public accounts named by applied privacy-preserving transactions.
    touched: HashSet<AccountId>,
    /// Every call the tool handed to the zone (`Ledger::submit`), whatever came of it: applied, refused by the program,
    /// or a private one that did not prove.
    submitted: usize,
}

/// What sending anything would move on the zone: the calls the tool handed to it, its block height, and the nonces of
/// the fee payer and of the wallet's private accounts. A refusal "before anything is signed" leaves all of it as it
/// was.
#[derive(Debug, PartialEq, Eq)]
struct Marks {
    submitted: usize,
    block: u64,
    nonces: Vec<String>,
}

/// The zone as `before` a refused command: nothing handed to it, no block, no nonce used. Checked first, on its own, so
/// that a send behind a refusal's text fails here.
fn unmoved(zone: &HarnessLedger, before: &Marks, what: &str) -> Result<()> {
    let now = zone.marks();
    ensure!(now == *before, "{what}: something was sent ({before:?} -> {now:?})");
    Ok(())
}

fn test_keys(s: &PrivateSecrets) -> TestPrivateKeys {
    TestPrivateKeys { ask: lee_core::AuthorizationSecretKey(s.0), d: s.1, z: s.2 }
}

impl HarnessLedger {
    fn marks(&self) -> Marks {
        let chain = &self.world.chain;
        let mut nonces: Vec<String> =
            self.private.iter().map(|(id, (_, account))| format!("{id:?} {:?}", account.nonce)).collect();
        nonces.sort();
        nonces.insert(0, format!("payer {:?}", chain.state.get_account_by_id(chain.payer).nonce));
        Marks { submitted: self.submitted, block: chain.block_id, nonces }
    }

    /// Keeps the last settled transaction's root call, applied or refused.
    fn record(&mut self, report: &TxReport) {
        let tx =
            common::transaction::LeeTransaction::try_from_slice(&self.world.chain.last_tx).expect("a LEZ transaction");
        if let common::transaction::LeeTransaction::Public(tx) = tx {
            let root = &tx.message.execution.root;
            let call = Applied { to: root.to, message: root.message.clone() };
            // LEZ keeps a refused transaction in its block, message and all (a refused plaintext claim publishes its
            // witness): as `Sequencer::included` reads it.
            if report.ok() { self.applied.push(call) } else { self.refused.push(call) }
        }
    }

    /// The seed's private accounts /0 … /n-1 on the zone, from the keys LEZ's wallet derives, each with a
    /// native balance for its private fees (stand-in: the gas drop's credit, which a relayer sends).
    fn add_private_accounts(&mut self, seed: &Seed, n: u32) -> Result<()> {
        let ids = seed.private_account_ids(n)?;
        for (a, secrets) in seed.private_account_secrets(n)?.into_iter().enumerate() {
            ensure!(
                test_keys(&secrets).account_id() == ids[a],
                "the harness derives private account /{a} from the wallet's keys as LEZ's wallet does"
            );
            let added = self.world.chain.add_private_account_with_keys(test_keys(&secrets), 1_000_000_000_000);
            self.private.insert(ids[a], (secrets, added.account));
        }
        Ok(())
    }

    /// Proves and settles a private call (rooted at one of the wallet's private accounts) as LEZ's
    /// wallet does, in RISC Zero dev mode; keeps its visible calls and the public accounts it names,
    /// and the account's new state (checked against the chain's commitment).
    fn settle_private(&mut self, call: &LezCall, me: Actor) -> Result<String> {
        let program = self.world.program;
        let (secrets, before) = self
            .private
            .get(&me.account_id)
            .cloned()
            .ok_or_else(|| anyhow!("not one of the wallet's private accounts"))?;
        let sender = PrivateAccount { keys: test_keys(&secrets), account: before.clone() };
        let publics: Vec<Actor> = call.mentions[1..].iter().map(Mention::actor).collect();
        let time = self.world.time;
        let run = self
            .world
            .chain
            .prove_and_settle_private(
                &sender,
                me,
                borsh::to_vec(&call.message)?,
                publics,
                &hb::catalog(),
                RESERVE,
                time,
            )
            .map_err(|e| anyhow!("the private transaction does not prove (the program refuses it): {e}"))?;
        ensure!(run.charge.revert.is_none(), "the private transaction reverted: {:?}", run.charge.revert);
        self.applied.extend(lez::visible_calls(&run.tx, program));
        self.touched.extend(run.tx.message.public_account_ids());
        let mut holder = match BridgeState::try_from_slice(before.data.actor_state(program)) {
            Ok(BridgeState::Holder(h)) => h,
            _ => HolderState::default(),
        };
        let mut change = |cid: [u8; 32], token: [u8; 20], delta: i128| match holder
            .holdings
            .iter()
            .position(|h| h.config_id == cid && h.token == token)
        {
            Some(k) => {
                let left = (holder.holdings[k].amount as i128 + delta) as u128;
                if left == 0 {
                    holder.holdings.remove(k);
                } else {
                    holder.holdings[k].amount = left;
                }
            }
            None => holder.holdings.push(Holding { config_id: cid, token, amount: delta as u128 }),
        };
        match &call.message {
            BridgeMessage::ClaimPrivately { config_id, witness, .. }
                if AccountId::new(witness.dest) == me.account_id =>
            {
                change(*config_id, witness.token, ferrata_cli::units::word_to_u128(&witness.amount)? as i128)
            }
            BridgeMessage::Burn { config_id, token, amount, .. } => {
                change(*config_id, *token, -(ferrata_cli::units::word_to_u128(amount)? as i128))
            }
            _ => {}
        }
        let after = hb::private_post_state(&self.world.chain, &test_keys(&secrets), &before, RESERVE, &holder)
            .ok_or_else(|| anyhow!("the private account's new state is not the one the chain committed"))?;
        self.private.insert(me.account_id, (secrets, after));
        Ok(format!("{} (harness, private, block {})", call.what, self.world.chain.block_id - 1))
    }

    /// Sends `call` without the tool's checks: what the program itself answers.
    fn settle(&mut self, call: &LezCall) -> TxReport {
        let actors: Vec<Actor> = call.mentions.iter().map(Mention::actor).collect();
        let root = actors[call.root_index];
        let mut evidence = Vec::new();
        if let Mention::Pda { actor, seed } = &call.mentions[call.root_index] {
            if self.world.chain.state.get_account_by_id_ref(actor.account_id).is_none() {
                evidence.push(PublicAccountEvidence::Pda { program: actor.program_account_id, seed: *seed });
            }
        }
        for m in &call.mentions {
            if let Mention::Other(actor, Some(key)) = m {
                if self.world.chain.state.get_account_by_id_ref(actor.account_id).is_none() {
                    evidence.push(PublicAccountEvidence::Key(key.clone()));
                }
            }
        }
        let signers: Vec<&PrivateKey> = call
            .mentions
            .iter()
            .filter_map(|m| match m {
                Mention::Signer(a) => Some(self.keys.get(&a.account_id).expect("the holder's key is the seed's")),
                _ => None,
            })
            .collect();
        let signers: Vec<PrivateKey> = signers.into_iter().cloned().collect();
        let refs: Vec<&PrivateKey> = signers.iter().collect();
        let report = self.world.submit_signed(&call.what, root, &call.message, actors, evidence, &refs);
        self.record(&report);
        report
    }
}

impl Ledger for HarnessLedger {
    async fn applied(&self, program: AccountId) -> Result<Vec<Applied>> {
        Ok(self.applied.iter().filter(|a| a.to.program_account_id == program).cloned().collect())
    }

    async fn included(&self, program: AccountId) -> Result<lez::Included> {
        let refused = self.refused.iter().filter(|a| a.to.program_account_id == program).cloned().collect();
        Ok(lez::Included { applied: self.applied(program).await?, refused, touched: self.touched.clone() })
    }

    async fn private_account(&mut self, id: AccountId) -> Result<Option<lee::Account>> {
        Ok(self.private.get(&id).map(|(_, a)| a.clone()))
    }

    async fn actor_state(&self, actor: Actor) -> Result<Vec<u8>> {
        Ok(self.world.chain.actor_state(actor).to_vec())
    }

    async fn submit(&mut self, call: &LezCall) -> Result<String> {
        self.submitted += 1;
        if let Some(Mention::PrivateSelf(me)) = call.mentions.first() {
            return self.settle_private(call, *me);
        }
        let report = self.settle(call);
        match (report.ok(), report.exit_code) {
            (true, _) => Ok(format!("{} (harness, block {})", call.what, self.world.chain.block_id - 1)),
            (false, Some(code)) => Err(anyhow!(codes::program_refusal(code, "returned by the program in the harness"))),
            (false, None) => Err(anyhow!("reverted without an exit code: {:?}", report.charge.revert)),
        }
    }
}

// ---- Ethereum: anvil and the vault ------------------------------------------------------------

struct Anvil(Child);

impl Drop for Anvil {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

/// Runs `cmd` with stdout and stderr sent to `log` (preventing pipe deadlocks while polling), killing it and
/// returning an error if it does not exit within `timeout`. Portable: polls `try_wait` instead of GNU `timeout`.
fn run_with_timeout(cmd: &mut Command, log: &Path, timeout: std::time::Duration, label: &str) -> Result<()> {
    let out = std::fs::File::create(log).with_context(|| format!("creating {}", log.display()))?;
    let err = out.try_clone().with_context(|| format!("cloning {}", log.display()))?;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .spawn()
        .with_context(|| format!("starting {label}"))?;
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait().with_context(|| format!("waiting for {label}"))? {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let output = std::fs::read(log).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
            bail!("{label} timed out after {} s and was killed:\n{output}", timeout.as_secs());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    if !status.success() {
        let output = std::fs::read(log).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        bail!("{label} failed ({status}):\n{output}");
    }
    Ok(())
}

fn scratch() -> PathBuf {
    let dir = std::env::var("FERRATA_E2E_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/cli/e2e"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn wait_for(rpc: &str) -> Result<DynProvider> {
    let p = eth::provider(rpc)?;
    for _ in 0..100 {
        if p.get_block_number().await.is_ok() {
            return Ok(p);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    bail!("anvil did not start")
}

/// An unlocked anvil account sends a transaction (stand-in roles: deployer, checkpoint relayer).
async fn anvil_send(p: &DynProvider, to: Address, input: Vec<u8>, value: U256) -> Result<()> {
    let tx = TransactionRequest::default().with_from(ANVIL0).with_to(to).with_input(input).with_value(value);
    let receipt = p.send_transaction(tx).await?.get_receipt().await?;
    ensure!(receipt.status(), "a stand-in transaction reverted");
    Ok(())
}

/// CliLocal's freeze authority: an address no one holds a key for (stand-in for the freeze authority's multisig),
/// impersonated by anvil.
fn freeze_authority() -> Address {
    Address::from_slice(&alloy::primitives::keccak256(b"ferrata cli e2e: freeze authority (no key)")[12..])
}

/// The vault's freeze authority sends `input` to the vault.
async fn freezer_send(p: &DynProvider, vault: Address, input: Vec<u8>) -> Result<()> {
    let f = freeze_authority();
    p.raw_request::<_, serde_json::Value>("anvil_impersonateAccount".into(), (f,)).await?;
    p.raw_request::<_, serde_json::Value>("anvil_setBalance".into(), (f, U256::from(ETH_1))).await?;
    let tx = TransactionRequest::default().with_from(f).with_to(vault).with_input(input);
    let receipt = p.send_transaction(tx).await?.get_receipt().await?;
    ensure!(receipt.status(), "the freeze authority's transaction reverted");
    Ok(())
}

/// The bridge program's freeze authority (the harness's role key) freezes, or with `lift` unfreezes, the mints
/// (`mints`) or the burns of `token`, or of the whole configuration (`None`), for an hour, against a record of
/// Ethereum's current time (as the mini-app's admin view does, `admin.freeze`).
async fn lez_freeze(
    zone: &mut HarnessLedger,
    p: &DynProvider,
    cid: &Hash,
    token: Option<[u8; 20]>,
    mints: bool,
    lift: bool,
) -> Result<()> {
    let now = eth::now(p).await?.max(zone.world.newest_record_seconds() + 1);
    let newest = zone.world.finalize(now);
    let (t, record) = (zone.world.blocks[newest].time_ms(), zone.world.blocks[newest].time_record());
    let until_ms = if lift { 0 } else { t + 3_600_000 };
    let message = BridgeMessage::Freeze { config_id: *cid, token, mints, burns: !mints, until_ms, record };
    let program = zone.world.program;
    let target = match token {
        Some(t) => ferrata_lez_bridge_wire::token_actor(program, cid, &t),
        None => ferrata_lez_bridge_wire::config_actor(program, cid),
    };
    let label = if lift { "Freeze lifted (freeze authority)" } else { "Freeze (freeze authority)" };
    let r = zone.world.role(label, hb::FREEZE_SEED, &message, target);
    zone.record(&r);
    ensure!(r.ok(), "{label} refused with exit code {:?}", r.exit_code);
    Ok(())
}

/// The program's and the tool's own words that speak of a leaf in general and name none: 248's name, the meanings of
/// 245, 248 and 249 (`cli/src/codes.rs`), and the tool's "no imported root covers this leaf yet" (`pick_import`).
/// Each is removed whole before [`names_nothing`] looks for a leaf, so a number put inside one leaves its "leaf" to be
/// found.
const GENERAL_LEAF_WORDS: [&str; 5] = [
    "LEAF_NOT_IMPORTED",
    "no imported root covers this leaf yet",
    "no imported root at this leaf count",
    "the leaf index is at or above the imported root's leaf count",
    "the opening, the leaf and the path do not hash to the root (a wrong leaf, path or destination)",
];

/// Words that name a deposit, a burn or a position when a number follows them (within two words).
const NUMBERED_WORDS: [&str; 13] = [
    "deposit",
    "deposits",
    "position",
    "burn",
    "burns",
    "index",
    "indices",
    "indexes",
    "counter",
    "number",
    "nullifier",
    "commitment",
    "tag",
];

/// A refusal that names no deposit and no burn (U.08's last sentence), by its form: no hash, commitment, nullifier or
/// address (no run of 40 or more hex digits); no leaf at all, in any case or form ("leaf 5", "Leaf (5)", "leaf number
/// 5", "the 5th leaf", "leaf_index: 5"), once the program's and the tool's general wordings
/// ([`GENERAL_LEAF_WORDS`]) are removed whole; no deposit, burn, position, index or counter followed within two words
/// by a number, nor "#5"; no "tag counter" or "j = ". Called on the whole error chain, `{e:#}`, as the binary prints it
/// (`main.rs`). Every form the base's check (no "leaf " at all) refused, it refuses, outside those exact wordings
/// (`names_nothing_catches_every_form`). [`names_none_of`] adds the case's own values.
fn names_nothing(text: &str) -> bool {
    let mut run = 0usize;
    for c in text.chars() {
        run = if c.is_ascii_hexdigit() { run + 1 } else { 0 };
        if run >= 40 {
            return false;
        }
    }
    let mut rest = text.to_string();
    for general in GENERAL_LEAF_WORDS {
        rest = rest.replace(general, " ");
    }
    let rest = rest.to_lowercase();
    if rest.contains("leaf") {
        return false;
    }
    let words: Vec<&str> = rest.split_whitespace().collect();
    let numbered = words.iter().enumerate().any(|(k, w)| {
        let core: String = w.chars().filter(|c| c.is_ascii_alphabetic()).collect();
        NUMBERED_WORDS.contains(&core.as_str())
            && words[k + 1..].iter().take(2).any(|n| n.chars().any(|c| c.is_ascii_digit()))
    });
    let hash_number = rest.match_indices('#').any(|(k, _)| rest[k + 1..].starts_with(|c: char| c.is_ascii_digit()));
    !numbered && !hash_number && !text.contains("tag counter") && !text.contains("j = ")
}

/// [`names_nothing`], and none of the case's own values: no 8 hex digits in a row of any of `values` (see
/// [`deposit_values`], [`burn_values`]; any case, so no short prefix either), and not `index` (its leaf index or burn
/// counter) as a number on its own.
fn names_none_of(text: &str, values: &[Vec<u8>], index: Option<u64>) -> bool {
    let lower = text.to_lowercase();
    let hex_named = values.iter().any(|v| {
        let h = hex::encode(v);
        (0..=h.len().saturating_sub(8)).any(|k| lower.contains(&h[k..k + 8]))
    });
    let index_named =
        index.is_some_and(|i| text.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| w == i.to_string()));
    names_nothing(text) && !hex_named && !index_named
}

/// A deposit's own values, any of which in a refusal would name it: its nullifier, commitment, depositor, destination,
/// secret `s` and leaf.
fn deposit_values(o: &ferrata_bridge_core::inbound::OwnedDeposit) -> Vec<Vec<u8>> {
    let d = &o.deposit;
    let leaf =
        ferrata_bridge_core::inbound::deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment);
    vec![
        o.nullifier.to_vec(),
        d.commitment.to_vec(),
        d.depositor.to_vec(),
        o.dest.to_vec(),
        o.s.to_vec(),
        leaf.to_vec(),
    ]
}

/// A burn's own values, any of which in a refusal would name it: its tag `r`, `c_out`, recipient, release secret, leaf
/// and `NOut`.
fn burn_values(b: &ferrata_bridge_core::outbound::OwnedBurn) -> Vec<Vec<u8>> {
    let n = &b.burn.note;
    vec![n.r.to_vec(), n.c_out.to_vec(), b.recipient.to_vec(), b.s_out.to_vec(), b.leaf.to_vec(), b.nullifier.to_vec()]
}

/// The form check on its own (no anvil, no zone: runs without `FERRATA_E2E`): the messages the tool gives pass; a leaf,
/// deposit, burn or position named by number, in every form the reviews tried (an internal review's L1: the round-1 check missed
/// seven of them), fails, alone and inside each message.
#[test]
fn names_nothing_catches_every_form() {
    let not_imported = "checked before sending: no imported root covers this leaf yet; wait for the next root import";
    let clean = [
        codes::program_refusal(245, not_imported).to_string(),
        codes::program_refusal(248, not_imported).to_string(),
        codes::program_refusal(
            248,
            format!(
                "{not_imported}; this configuration imports a root only once the block it reads is 64 beacon \
                 slots (about 13 min) past Ethereum finality (its inbound finality depth, set by its admin), so a \
                 deposit becomes claimable that much later"
            ),
        )
        .to_string(),
        codes::program_refusal(249, "checked before sending with the program's verify_claim").to_string(),
        codes::program_refusal(252, "checked before sending: a private-kind deposit is claimed privately").to_string(),
        codes::program_refusal(250, "checked before sending: the nullifier is spent").to_string(),
        codes::program_refusal(254, "checked before sending: the holding is 0 base units").to_string(),
        codes::program_refusal(
            242,
            "checked before sending: mints (claims) of this token are frozen until Ethereum time 1808000769 \
             (2027-04-17 22:26:09 UTC as the bridge's attested clock counts it; a freeze authority may lift it \
             sooner); nothing is sent and nothing is consumed: the deposit stays claimable",
        )
        .to_string(),
        codes::program_refusal(
            242,
            "checked before sending: burns of this token are frozen until Ethereum time 1808000769 (2027-04-17 \
             22:26:09 UTC as the bridge's attested clock counts it; a freeze authority may lift it sooner); nothing is \
             sent and nothing is burned: the holding stays yours",
        )
        .to_string(),
    ]
    .into_iter()
    .chain(
        [
            (
                "DepositsAreFrozen",
                "checked before sending: frozen until Ethereum time 1797871892 (2026-12-21 16:51:32)",
            ),
            (
                "ReleasesAreFrozen",
                "checked before sending: frozen until Ethereum time 1797872813 (2026-12-21 17:06:53)",
            ),
            (
                "BurnRootNotAccepted",
                "checked before sending: no finalized checkpoint covers this burn yet; wait for the next checkpoint",
            ),
            ("VerificationFailed", "eth_call before sending"),
            ("NullifierAlreadyUsed", "checked before sending: the vault's Release events show it released"),
            ("TokenNotRegistered", "checked before sending"),
            ("InvalidDenominationIndex", "checked before sending"),
            ("TokenDepositCapExceeded", "eth_call before sending"),
        ]
        .map(|(name, how)| codes::vault_refusal(&codes::selector(name), how).expect("a vault error").to_string()),
    )
    .collect::<Vec<String>>();
    for m in &clean {
        assert!(names_nothing(m), "names nothing, yet refused: {m}");
    }
    let named = [
        "leaf 5",
        "Leaf 5",
        "LEAF 5",
        "leaf (5)",
        "leaf number 5",
        "leaf no. 5",
        "leaf at index 5",
        "leaf indices 5",
        "leaf indexes 5",
        "the 5th leaf",
        "leaf_index: 5",
        "leaf=5",
        "leaf:5",
        "leaf #5",
        "leaves' leaf 5",
        "deposit 5",
        "deposit #5",
        "deposit number 5",
        "position 5",
        "position #16",
        "burn 5",
        "burn counter 3",
        "burn index 3",
        "index 5",
        "counter 7",
        "nullifier 3",
        "tag 9",
        "#16",
        "tag counter",
        "j = 2",
    ]
    .map(String::from)
    .into_iter()
    .chain(["ab".repeat(20)]);
    for n in named {
        assert!(!names_nothing(&n), "names a position, yet passed: {n}");
        for m in &clean {
            let t = format!("{m} ({n})");
            assert!(!names_nothing(&t), "names a position, yet passed: {t}");
        }
    }
}

async fn slot(p: &DynProvider, vault: Address, key: [u8; 32]) -> Result<[u8; 32]> {
    Ok(p.get_storage_at(vault, U256::from_be_bytes(key)).await?.to_be_bytes::<32>())
}

fn word(n: u64) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&n.to_be_bytes());
    w
}

/// Runs the `ferrata` binary; returns its stdout.
fn ferrata(args: &[&str]) -> Result<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_ferrata")).args(args).output()?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    ensure!(out.status.success(), "ferrata {}: {}\n{stdout}", args.join(" "), String::from_utf8_lossy(&out.stderr));
    Ok(stdout)
}

/// Runs the `ferrata` binary with `--json`; returns whether it succeeded and the one JSON object it printed.
fn ferrata_json(args: &[&str]) -> Result<(bool, serde_json::Value)> {
    let out = Command::new(env!("CARGO_BIN_EXE_ferrata")).arg("--json").args(args).output()?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let v: serde_json::Value =
        serde_json::from_str(stdout.trim()).with_context(|| format!("not one JSON object: {stdout}"))?;
    Ok((out.status.success(), v))
}

/// The checkpoint journal the vault rebuilds (VaultTestBase._checkpointJournalBytes).
fn checkpoint_journal(chain_id: u64, vault: Address, cid: B256, c: &ICheckpoints::Checkpoint) -> Vec<u8> {
    let mut j = Vec::new();
    j.extend_from_slice(&sha256(b"ferrata/vault/checkpoint/v3"));
    j.extend_from_slice(&word(chain_id));
    j.extend_from_slice(&[[0u8; 12].as_slice(), vault.as_slice()].concat());
    j.extend_from_slice(cid.as_slice());
    j.extend_from_slice(&word(u64::from(c.nMin)));
    j.extend_from_slice(&word(u64::from(c.tMin)));
    for h in [c.parentState, c.newState, c.burnRoot] {
        j.extend_from_slice(h.as_slice());
    }
    j.extend_from_slice(&word(c.burnCount));
    j.extend_from_slice(&word(c.importedLeafCount));
    j.extend_from_slice(&word(u64::from(c.sunset)));
    j.extend_from_slice(c.echoNonce.as_slice());
    j.extend_from_slice(&word(c.tipId));
    j.extend_from_slice(c.tipHash.as_slice());
    j.extend_from_slice(&word(u64::from(c.n)));
    j.extend_from_slice(&word(u64::from(c.t)));
    j
}

/// Stands in for the checkpoint prover and relayer: candidate, wait, beacon, finalisation.
async fn accept_burn_root(
    p: &DynProvider,
    vault: Address,
    cid: B256,
    burn_root: Hash,
    burn_count: u64,
    wait_release_delay: bool,
) -> Result<()> {
    let c = propose_burn_root(p, vault, cid, burn_root, burn_count).await?;
    finalize_burn_root(p, vault, cid, c, wait_release_delay).await
}

/// The mock verifier's acceptance of `journal` under `image` (stand-in for a Groth16 proof of it).
fn mock_accept(image: B256, journal: &[u8]) -> Vec<u8> {
    IMock::acceptCall { imageId: image, journalDigest: B256::from(sha256(journal)) }.abi_encode()
}

/// The checkpoint relayer's candidate of the burn root (its wait starts; not final).
async fn propose_burn_root(
    p: &DynProvider,
    vault: Address,
    cid: B256,
    burn_root: Hash,
    burn_count: u64,
) -> Result<ICheckpoints::Checkpoint> {
    let chain_id = p.get_chain_id().await?;
    let mock_image = eth::view(p, vault, ICheckpoints::checkpointImageIdCall {}, None).await?;
    let verifier = std::env::var("FERRATA_E2E_VERIFIER")?.parse::<Address>()?;
    let parent = eth::view(p, vault, ICheckpoints::finalStateCall {}, None).await?;
    let c = ICheckpoints::Checkpoint {
        nMin: 3,
        tMin: 2,
        parentState: parent,
        newState: B256::from(sha256(&[b"cli e2e state".as_slice(), &burn_root].concat())),
        burnRoot: B256::from(burn_root),
        burnCount: burn_count,
        importedLeafCount: 0,
        sunset: false,
        echoNonce: B256::ZERO,
        tipId: 1000 + burn_count,
        tipHash: B256::from(sha256(&[b"cli e2e tip".as_slice(), &burn_root].concat())),
        n: 4,
        t: 3,
    };
    anvil_send(p, verifier, mock_accept(mock_image, &checkpoint_journal(chain_id, vault, cid, &c)), U256::ZERO).await?;
    anvil_send(
        p,
        vault,
        ICheckpoints::submitCandidateCall { c: c.clone(), seal: Bytes::from(SEAL.to_vec()) }.abi_encode(),
        U256::ZERO,
    )
    .await?;
    Ok(c)
}

/// The candidate `c`'s wait passes, a beacon is recorded and `c` is finalised; with `wait_release_delay`, the token's
/// release delay passes too.
async fn finalize_burn_root(
    p: &DynProvider,
    vault: Address,
    cid: B256,
    mut c: ICheckpoints::Checkpoint,
    wait_release_delay: bool,
) -> Result<()> {
    let chain_id = p.get_chain_id().await?;
    let mock_image = eth::view(p, vault, ICheckpoints::checkpointImageIdCall {}, None).await?;
    let verifier = std::env::var("FERRATA_E2E_VERIFIER")?.parse::<Address>()?;
    let accept = |j: &[u8]| mock_accept(mock_image, j);
    // W = delta + (t - 1) * turnSeconds = 600 + 2 * 2.
    p.raw_request::<_, serde_json::Value>("evm_increaseTime".into(), (700u64,)).await?;
    p.raw_request::<_, serde_json::Value>("evm_mine".into(), Vec::<u64>::new()).await?;
    anvil_send(p, vault, ICheckpoints::recordBeaconCall {}.abi_encode(), U256::ZERO).await?;
    let index = eth::view(p, vault, ICheckpoints::beaconCountCall {}, None).await? - U256::from(1);
    let beacon = eth::view(p, vault, ICheckpoints::beaconsCall { i: index }, None).await?;
    c.echoNonce = beacon.nonce;
    anvil_send(p, verifier, accept(&checkpoint_journal(chain_id, vault, cid, &c)), U256::ZERO).await?;
    anvil_send(
        p,
        vault,
        ICheckpoints::finalizeCall { beaconIndex: index, c, seal: Bytes::from(SEAL.to_vec()) }.abi_encode(),
        U256::ZERO,
    )
    .await?;
    if wait_release_delay {
        pass_release_delay(p).await?;
    }
    Ok(())
}

/// The token's release delay (120 s) passes.
async fn pass_release_delay(p: &DynProvider) -> Result<()> {
    p.raw_request::<_, serde_json::Value>("evm_increaseTime".into(), (200u64,)).await?;
    p.raw_request::<_, serde_json::Value>("evm_mine".into(), Vec::<u64>::new()).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn deposit_scan_claim_burn_release_and_recovery() -> Result<()> {
    if std::env::var("FERRATA_E2E").is_err() {
        eprintln!("skipped: FERRATA_E2E is not set (cli/tests/e2e.sh runs it)");
        return Ok(());
    }
    let dir = scratch();
    let port = 18545u16;
    let rpc = format!("http://127.0.0.1:{port}");
    let _anvil = Anvil(
        Command::new("anvil")
            .args(["--port", &port.to_string(), "--chain-id", "1", "--silent", "--code-size-limit", "100000"])
            .stdout(Stdio::null())
            .spawn()
            .context("starting anvil")?,
    );
    let p = wait_for(&rpc).await?;

    // The zone, and the configuration of the vault anvil's first account is about to deploy.
    let mut zone = HarnessLedger {
        world: hb::World::new(),
        applied: Vec::new(),
        refused: Vec::new(),
        keys: HashMap::new(),
        private: HashMap::new(),
        touched: HashSet::new(),
        submitted: 0,
    };
    let program = zone.world.program;
    // The deployer's contracts: the linked FerrataRegistry library, the mock, its emergency stop, the vault.
    let vault = ANVIL0.create(3);
    let config = hb::config(vault.into_array(), hb::controls());
    let cid = config.id(program);
    // Run offline (solc 0.8.26 is installed by vault's prior `forge build`, and vault/foundry.toml keeps forge
    // offline, overriding CI's workflow-level FOUNDRY_OFFLINE=false), one transaction at a time (--slow: works around
    // anvil 1.3.6 automine occasionally leaving batched deployer txs unmined, as in roundtrip_b.rs), and bounded by a
    // portable timeout so a stalled broadcast fails instead of hanging CI.
    let mut forge = Command::new("forge");
    forge
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../vault"))
        .args([
            "script",
            "script/CliLocal.s.sol:CliLocal",
            "--rpc-url",
            &rpc,
            "--broadcast",
            "--slow",
            "--unlocked",
            "--sender",
        ])
        .arg(ANVIL0.to_string())
        .env("FOUNDRY_OFFLINE", "true")
        .env("FERRATA_CID", hx(&cid));
    run_with_timeout(
        &mut forge,
        &dir.join("forge-cli-local.log"),
        std::time::Duration::from_secs(300),
        "forge script CliLocal",
    )?;
    ensure!(!p.get_code_at(vault).await?.is_empty(), "the vault is not at the predicted address");
    std::env::set_var("FERRATA_E2E_VERIFIER", ANVIL0.create(1).to_string());
    let registered_at = p.get_block(alloy::eips::BlockId::latest()).await?.expect("a block").header.timestamp;
    println!("anvil: vault {vault}, configId {}", hx(&cid));

    // The relayer's part on LEZ: Init and Register from the vault's own storage.
    let reg = registry_slot(&[0; 20]);
    let fee0 = fee_slot(&[0; 20], 0);
    let mut storage = Vec::new();
    for key in [word(2), reg, fee0] {
        storage.push((key, slot(&p, vault, key).await?));
    }
    let stored = eth::view(&p, vault, IVault::lezConfigIdCall {}, None).await?;
    ensure!(
        storage[0].1 == cid,
        "the vault's slot 2 is not the configuration's id: slot 2 {} lezConfigId {stored} cid {}",
        hx(&storage[0].1),
        hx(&cid)
    );
    let b0 = zone.world.add_block(hb::Block::with_storage(9_000_000, 9_000_000, vault.into_array(), &storage));
    let r = zone.world.init("Init (relayer)", &config, b0);
    ensure!(r.ok(), "Init: {:?}", r.exit_code);
    zone.record(&r);
    let entry = hb::eth_entry();
    let schedule = FeeSchedule {
        index: 0,
        from_leaf: 0,
        announced_at: registered_at,
        effective_at: registered_at,
        mint_fees: vec![0; 3],
        burn_fees: vec![0; 3],
        checkpoint_fees: vec![0; 3],
    };
    let r = zone.world.register_with("Register ETH (relayer)", &cid, b0, &entry, &schedule, &vault.into_array());
    ensure!(r.ok(), "Register: {:?}", r.exit_code);
    zone.record(&r);

    // Keystores: the user's depositor and another user's, funded by anvil's account.
    let password = dir.join("password.txt");
    std::fs::write(&password, "e2e test password\n")?;
    let mut rng = rand::thread_rng();
    let mut keystores = Vec::new();
    for name in ["user", "other"] {
        let path = dir.join(format!("keystore-{name}.json"));
        let _ = std::fs::remove_file(&path);
        let (signer, _) = PrivateKeySigner::new_keystore(
            &dir,
            &mut rng,
            "e2e test password",
            Some(&format!("keystore-{name}.json")),
        )?;
        anvil_send(&p, signer.address(), vec![], U256::from(100u64) * U256::from(10u64).pow(U256::from(18u64))).await?;
        keystores.push((path, signer.address()));
    }
    for (phrase, name) in [(USER, "user"), (OTHER, "other")] {
        std::fs::write(dir.join(format!("seed-{name}.txt")), phrase)?;
    }
    let seed_user = dir.join("seed-user.txt");
    let common = |seed: &Path| -> Vec<String> {
        vec![
            "--seed".into(),
            seed.display().to_string(),
            "--eth-rpc".into(),
            rpc.clone(),
            "--vault".into(),
            vault.to_string(),
        ]
    };
    let deposit = |seed: &Path, keystore: &Path, denom: u8, kind: &str, account: u32| -> Result<String> {
        let mut args = common(seed);
        args.extend(
            [
                "deposit",
                "--denomination",
                &denom.to_string(),
                "--kind",
                kind,
                "--account",
                &account.to_string(),
                "--keystore",
                &keystore.display().to_string(),
                "--password-file",
                &password.display().to_string(),
                "--send",
                "--yes",
            ]
            .map(String::from),
        );
        if kind == "public" {
            args.push("--i-accept-public-account".into());
        }
        ferrata(&args.iter().map(String::as_str).collect::<Vec<_>>())
    };

    // Deposits, through the binary: the user's 10 ETH to public account /0 (claimed below), ten
    // more of 0.1 ETH to accounts of both kinds (left unclaimed), and five of another user's.
    let user_ks = &keystores[0].0;
    let other_ks = &keystores[1].0;
    // The public path needs its own acknowledgement: refused without it, after the warning.
    let mut unacked = common(&seed_user);
    unacked.extend(
        ["deposit", "--denomination", "2", "--kind", "public", "--keystore", &user_ks.display().to_string()]
            .map(String::from),
    );
    unacked.extend(["--password-file".to_string(), password.display().to_string(), "--send".into(), "--yes".into()]);
    let refused_public = ferrata(&unacked.iter().map(String::as_str).collect::<Vec<_>>()).expect_err("unacknowledged");
    println!("public deposit without acknowledgement: {refused_public}");
    ensure!(
        refused_public.to_string().contains("WARNING")
            && refused_public.to_string().contains("High: a direct link is public")
            && refused_public.to_string().contains("refusing a public destination without --i-accept-public-account"),
        "the public deposit warns and is refused without its acknowledgement"
    );
    let first = deposit(&seed_user, user_ks, 2, "public", 0)?;
    print!("{first}");
    ensure!(first.contains("WARNING") && first.contains("sent: "), "the acknowledged public deposit warns, then sends");
    let mut expected_unclaimed = Vec::new();
    for k in 0..10u32 {
        if k % 2 == 0 {
            deposit(&dir.join("seed-other.txt"), other_ks, 0, "private", k % 3)?;
        }
        let kind = if k % 2 == 0 { "private" } else { "public" };
        deposit(&seed_user, user_ks, 0, kind, k % 8)?;
        expected_unclaimed.push((kind, k % 8));
    }
    // The vault refuses a deposit with a stale j: the tool reports the vault's error.
    let refused = {
        let signer = eth::keystore_signer(user_ks, &password)?;
        let sp = eth::signing_provider(&rpc, signer)?;
        let price = eth::deposit_price(&p, vault, eth::ETH, 0).await?;
        let input = eth::deposit_calldata(eth::ETH, 0, B256::repeat_byte(7), 0);
        eth::send(&sp, keystores[0].1, vault, input, U256::from(price.total), 1).await.expect_err("a stale j")
    };
    println!("stale j: {refused}");
    ensure!(refused.to_string().contains("WrongDepositIndex"), "the vault's error is named");

    // Errors through the binary (RFP-022 U.08), each naming its cause and what to do: an unsupported
    // token, an invalid amount, a cap the deposit would exceed (eth_call, before signing), an RPC that
    // does not answer, an RPC of another chain, no vault at the address.
    let user_addr = keystores[0].1.to_string();
    let dry_deposit = |extra: &[&str]| -> Result<String> {
        let mut a = common(&seed_user);
        a.extend(["deposit".to_string(), "--from".into(), user_addr.clone()]);
        a.extend(extra.iter().map(|x| x.to_string()));
        ferrata(&a.iter().map(String::as_str).collect::<Vec<_>>())
    };
    // The refusal's own line: the binary's error also repeats its command line, the depositor's address included.
    let refusal_line = |e: &anyhow::Error| {
        e.to_string().lines().find(|l| l.contains("refused:")).unwrap_or_default().to_string()
    };
    // A deposit's refusal names no deposit: not the depositor (the only value of it the dry run has before it plans).
    let depositor = vec![keystores[0].1.as_slice().to_vec()];
    let unsupported = dry_deposit(&["--token", "0x000000000000000000000000000000000000dEaD", "--denomination", "0"])
        .expect_err("an unregistered token");
    let line = refusal_line(&unsupported);
    println!("UNSUPPORTED TOKEN (deposit): {line}");
    ensure!(
        line.contains("refused: vault error TokenNotRegistered() (checked before sending)")
            && line.contains("`ferrata status --token <address>` shows whether a token is")
            && names_none_of(&line, &depositor, None),
        "{unsupported}"
    );
    let invalid = dry_deposit(&["--denomination", "7"]).expect_err("no such denomination");
    let line = refusal_line(&invalid);
    println!("INVALID AMOUNT (deposit): {line}");
    ensure!(
        line.contains("refused: vault error InvalidDenominationIndex() (checked before sending")
            && line.contains("`ferrata status --token <address>` lists them by index")
            && names_none_of(&line, &depositor, None),
        "{invalid}"
    );
    let set_cap = |capacity: U256| ICheckpoints::setTokenCapCall {
        token: Address::ZERO,
        direction: 0,
        capacity,
        refillPerSecond: U256::ZERO,
    };
    anvil_send(&p, vault, set_cap(U256::from(1u64)).abi_encode(), U256::ZERO).await?;
    let capped = dry_deposit(&["--denomination", "0"])?;
    let line = capped.lines().find(|l| l.contains("refused:")).unwrap_or_default();
    println!("CAP (deposit, eth_call): {line}");
    ensure!(
        line.contains("refused: vault error TokenDepositCapExceeded()")
            && capped.contains("not sent")
            && names_none_of(line, &depositor, None),
        "the cap, from eth_call: {capped}"
    );
    anvil_send(&p, vault, set_cap(U256::MAX).abi_encode(), U256::ZERO).await?;
    // Freezes through the binary (U.08, "program or per-token frozen"): the vault's freeze authority freezes the
    // deposits of the whole vault, then ETH's; each dry run is refused before signing with the vault's error, the
    // time the freeze ends and what to do, naming no deposit; lifted, the dry run passes again.
    for (freeze, lift, want) in [
        (
            IFreeze::setDepositsFrozenCall { frozen: true }.abi_encode(),
            IFreeze::setDepositsFrozenCall { frozen: false }.abi_encode(),
            "DepositsAreFrozen",
        ),
        (
            IFreeze::setTokenDepositsFrozenCall { token: Address::ZERO, frozen: true }.abi_encode(),
            IFreeze::setTokenDepositsFrozenCall { token: Address::ZERO, frozen: false }.abi_encode(),
            "TokenDepositsAreFrozen",
        ),
    ] {
        freezer_send(&p, vault, freeze).await?;
        let e = dry_deposit(&["--denomination", "0"]).expect_err("a deposit while frozen");
        let line = refusal_line(&e);
        println!("FREEZE (vault, deposit): {line}");
        ensure!(
            line.contains(&format!("vault error {want}()"))
                && line.contains("checked before sending: frozen until Ethereum time")
                && line.contains("retry once the freeze ends")
                && names_none_of(&line, &depositor, None),
            "the frozen deposit's refusal: {e}"
        );
        freezer_send(&p, vault, lift).await?;
        let passes = dry_deposit(&["--denomination", "0"])?;
        ensure!(passes.contains("not sent") && !passes.contains("Frozen"), "lifted: {passes}");
    }
    let seed_arg = seed_user.display().to_string();
    let vault_arg = vault.to_string();
    let no_rpc = ferrata(&["--seed", &seed_arg, "--eth-rpc", "http://127.0.0.1:9", "--vault", &vault_arg, "scan"])
        .expect_err("no RPC");
    let other_chain =
        ferrata(&["--seed", &seed_arg, "--eth-rpc", &rpc, "--vault", &vault_arg, "--chain-id", "11155111", "scan"])
            .expect_err("another chain");
    let no_vault = ferrata(&[
        "--seed",
        &seed_arg,
        "--eth-rpc",
        &rpc,
        "--vault",
        "0x000000000000000000000000000000000000dEaD",
        "scan",
    ])
    .expect_err("no vault");
    for (e, want) in [
        (&unsupported, "TokenNotRegistered"),
        (&invalid, "InvalidDenominationIndex"),
        (&no_rpc, "does not answer"),
        (&other_chain, "serves chain 1, not 11155111"),
        (&no_vault, "no contract at"),
    ] {
        let line = e.to_string().lines().find(|l| l.contains("error:")).unwrap_or_default().to_string();
        println!("refused ({want}): {line}");
        ensure!(e.to_string().contains(want), "{e}");
    }

    // Scan through the binary, Ethereum only (deposit list checked against depositRoot).
    let scan_out =
        ferrata(&[common(&seed_user), vec!["scan".into()]].concat().iter().map(String::as_str).collect::<Vec<_>>())?;
    print!("{scan_out}");
    ensure!(scan_out.contains("deposits of this seed: 11"), "the binary's scan finds the user's 11 deposits");
    // The same for scripts (`--json`): one object with the report's lines and a structured result.
    let args = [common(&seed_user), vec!["scan".into()]].concat();
    let (ok, scan_json) = ferrata_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
    ensure!(ok && scan_json["command"] == "scan" && scan_json["result"]["deposits"] == 11, "scan --json: {scan_json}");
    let mut args = common(&seed_user);
    args.extend([
        "deposit".to_string(),
        "--denomination".into(),
        "0".into(),
        "--from".into(),
        keystores[0].1.to_string(),
    ]);
    let (ok, dep_json) = ferrata_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
    ensure!(
        ok && dep_json["result"]["commitment"].as_str().is_some_and(|c| c.len() == 66)
            && dep_json["result"]["sent"].is_null()
            && dep_json["report"]
                .as_array()
                .is_some_and(|l| l.iter().any(|x| x.as_str() == Some("privacy of these choices: Minimal: only the residual correlations (token, denomination, time)"))),
        "deposit --json, a dry run: {dep_json}"
    );
    let (ok, err_json) = ferrata_json(&[
        "--seed",
        &seed_user.display().to_string(),
        "--eth-rpc",
        &rpc,
        "--vault",
        "0x000000000000000000000000000000000000dEaD",
        "scan",
    ])?;
    ensure!(
        !ok && err_json["error"].as_str().is_some_and(|e| e.contains("no contract at")),
        "an error as JSON: {err_json}"
    );
    println!(
        "--json: scan {} deposits; deposit dry run C {}; error {}",
        scan_json["result"]["deposits"], dep_json["result"]["commitment"], err_json["error"]
    );

    // Status through the binary, Ethereum part: the registry, fees and caps in the token's units.
    let status_out =
        ferrata(&[common(&seed_user), vec!["status".into()]].concat().iter().map(String::as_str).collect::<Vec<_>>())?;
    print!("{status_out}");
    ensure!(
        status_out.contains("registered true") && status_out.contains("16 deposits"),
        "the binary's status reads the vault"
    );

    // The user: the tool's library on the zone.
    let user = Seed::from_phrase(USER)?;
    for a in user.public_accounts(8)? {
        zone.keys.insert(a.id, a.key.clone());
    }
    let ctx = Context { program, cid, chain_id: 1, vault: vault.into_array() };
    let eth_side = flow::Eth { p: p.clone(), vault, chain_id: 1, from_block: 0, chunk: 10_000 };

    // Insufficient finality (U.08), before any root is imported: the claim of leaf 0 is refused before anything is
    // signed, with the program's code (245, no imported root) and what to do, naming no deposit; the zone is as it was.
    {
        let first = scan::deposits(
            &user,
            &ctx,
            &eth::deposit_list(&p, vault, 0, 10_000).await?,
            8,
            &lez::History::default(),
            &HashSet::new(),
        )?
        .into_iter()
        .find(|r| r.owned.deposit.leaf_index == 0)
        .context("leaf 0 is the user's")?
        .owned;
        let early = flow::ClaimOpts {
            leaf: Some(0),
            plaintext: true,
            accept_plaintext: true,
            no_delay: true,
            relayer: None,
            gap: 8,
            send: true,
            yes: true,
            gas_drop: None,
        };
        let before = zone.marks();
        let e = flow::claim(&eth_side, &user, &mut zone, &ctx, &early, &mut flow::Out::new(true))
            .await
            .err()
            .context("a claim before any root is imported")?;
        println!("NOT FINAL (claim, no root imported): {e}");
        unmoved(&zone, &before, "the claim before any import")?;
        ensure!(
            e.to_string().contains("refused: LEZ bridge program exit code 245 NOT_FOUND")
                && e.to_string().contains("checked before sending: no imported root covers this leaf yet")
                && e.to_string().contains("wait for the next root import")
                && names_none_of(&format!("{e:#}"), &deposit_values(&first), Some(first.deposit.leaf_index)),
            "the claim before any import: {e:#}"
        );
    }

    // The relayer imports the vault's root (its storage after the deposits) on LEZ.
    let n = eth::view(&p, vault, IVault::nextLeafIndexCall {}, None).await?;
    let root = eth::view(&p, vault, IVault::depositRootCall {}, None).await?;
    let mut storage2 = vec![(word(0), slot(&p, vault, word(0)).await?), (word(1), slot(&p, vault, word(1)).await?)];
    storage2.push((word(2), cid));
    ensure!(storage2[0].1 == root.0, "slot 0 is depositRoot");
    let b1 = zone.world.add_block(hb::Block::with_storage(9_000_064, 9_000_064, vault.into_array(), &storage2));
    let r = zone.world.import("ImportRoot (relayer)", &cid, b1, root.0, n, &vault.into_array());
    ensure!(r.ok(), "ImportRoot: {:?}", r.exit_code);
    zone.record(&r);
    // The relayer attests that ETH has no fee schedule 1 at this count: the known range grows to it.
    let r =
        zone.world.import_fees("ImportFees ETH 1 = none (relayer)", &cid, b1, [0; 20], 1, None, &vault.into_array());
    ensure!(r.ok(), "ImportFees: {:?}", r.exit_code);
    zone.record(&r);
    println!("zone: root({n}) imported; ETH fees known to leaf {n}");

    let list = eth::deposit_list(&p, vault, 0, 10_000).await?;
    let history = lez::history(&ctx, &zone.applied(program).await?);
    let rows = scan::deposits(&user, &ctx, &list, 8, &history, &HashSet::new())?;
    ensure!(rows.len() == 11, "11 deposits of the user, found {}", rows.len());
    let big = rows.iter().find(|r| r.owned.deposit.leaf_index == 0).expect("leaf 0 is the user's");
    ensure!(big.owned.kind == DestKind::Public && big.owned.account_index == 0, "leaf 0 is to public /0");
    let leaves: Vec<Hash> = list
        .deposits
        .iter()
        .map(|d| {
            ferrata_bridge_core::inbound::deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment)
        })
        .collect();

    // The gas drop (bridge.md §6, v3 D13), with `FERRATA_GASDROP_ELF` (cli/tests/relayer.sh): the user
    // proves, for real (not dev mode), a receipt that leaf 0 has nullifier N under the imported root,
    // bound to a one-time address and the relayer's LEZ account; the relayer checks it against the zone
    // (imported roots, claimed nullifiers) and the vault's deposit list and sends one credit per N; a
    // replay to another relayer and a second drop are refused; the claim below names the relayer.
    let mut gas_drop: Option<(Vec<u8>, ferrata_cli::relayer::DropContext<'static>, [u8; 32])> = None;
    if let Ok(elf_path) = std::env::var("FERRATA_GASDROP_ELF") {
        use ferrata_bridge_core::encoding::ClaimInput;
        use ferrata_bridge_core::gasdrop::GasDropInput;
        use ferrata_bridge_core::inbound::ClaimWitness;
        use ferrata_cli::relayer::{self as rl, DropBook, DropContext, DropRefusal};
        struct Credits(Vec<([u8; 32], u128)>);
        impl rl::Credit for Credits {
            async fn credit(
                &mut self,
                to: [u8; 32],
                _keys: Option<&rl::OneTimeKeys>,
                amount: u128,
            ) -> Result<String> {
                // Stand-in: the harness does not model LEZ's native token program; the sequencer
                // implementation (`impl Credit for lez::Sequencer`) sends a native transfer.
                self.0.push((to, amount));
                Ok(format!("native credit {} (harness stand-in)", self.0.len()))
            }
        }
        let relayer_lez = *Seed::from_phrase(OTHER)?.public_accounts(1)?[0].id.value();
        let d = &big.owned.deposit;
        let tree = Tree::from_leaves(leaves.clone()).map_err(|_| anyhow!("tree"))?;
        let input = GasDropInput {
            claim: ClaimInput {
                root: tree.root(),
                chain_id: word(1),
                vault: vault.into_array(),
                witness: ClaimWitness {
                    depositor: d.depositor,
                    token: d.token,
                    amount: d.amount,
                    leaf_index: d.leaf_index,
                    path: tree.path(d.leaf_index).context("a path")?,
                    s: big.owned.s,
                    kind: big.owned.kind,
                    dest: big.owned.dest,
                },
            },
            one_time_address: sha256(b"cli e2e: the claim's one-time address"),
            relayer: relayer_lez,
        };
        let elf = std::fs::read(&elf_path).with_context(|| elf_path.clone())?;
        let image = risc0_zkvm::compute_image_id(&elf).map_err(|e| anyhow!("{e}"))?;
        let env =
            risc0_zkvm::ExecutorEnv::builder().write_slice(&input.encode()).build().map_err(|e| anyhow!("{e}"))?;
        // The zone's guests run in dev mode; this receipt is a real one.
        std::env::remove_var("RISC0_DEV_MODE");
        let started = std::time::Instant::now();
        let proven = risc0_zkvm::default_prover().prove_with_opts(env, &elf, &risc0_zkvm::ProverOpts::succinct());
        std::env::set_var("RISC0_DEV_MODE", "1");
        let info = proven.map_err(|e| anyhow!("r0vm: {e}"))?;
        println!(
            "gas-drop receipt: real succinct proof in {:.1} s ({} user cycles)",
            started.elapsed().as_secs_f64(),
            info.stats.user_cycles
        );
        let receipt_json = serde_json::to_vec(&info.receipt)?;
        let history = lez::history_of(&zone, &ctx).await?;
        let roots: &'static HashSet<Hash> = Box::leak(Box::new(rl::imported_roots(&list.deposits, &history.roots)?));
        ensure!(roots.contains(&tree.root()), "the receipt's root is the zone's imported root");
        let mut image_id = [0u8; 32];
        image_id.copy_from_slice(image.as_bytes());
        let dctx = DropContext {
            image_id,
            me: relayer_lez,
            chain_id: word(1),
            vault: vault.into_array(),
            imported_roots: roots,
        };
        let book_path = dir.join("relayer-drops.json");
        let _ = std::fs::remove_file(&book_path);
        let mut book = DropBook::load(&book_path)?;
        let mut credits = Credits(Vec::new());
        let claimed = |n: &Hash| lez::is_claimed(&ctx, &history, n);
        let replay = rl::check_gas_drop(&receipt_json, &DropContext { me: [0x77; 32], ..dctx }, claimed, &book);
        ensure!(replay == Err(DropRefusal::OtherRelayer), "a receipt replayed to another relayer: {replay:?}");
        let (j, tx) =
            rl::gas_drop(&receipt_json, None, &dctx, claimed, &mut book, &book_path, &mut credits, 1_000_000, 0).await?;
        ensure!(j.nullifier == big.owned.nullifier, "the receipt's N is leaf 0's");
        let second =
            rl::gas_drop(&receipt_json, None, &dctx, claimed, &mut book, &book_path, &mut credits, 1_000_000, 1).await;
        ensure!(second.is_err() && credits.0.len() == 1, "one drop per N");
        println!(
            "GAS DROP: N {} -> one-time address {} ({tx}); replay to another relayer and a second drop refused",
            hx(&j.nullifier),
            hx(&j.one_time_address)
        );
        gas_drop = Some((receipt_json, dctx, relayer_lez));
    }

    // Claim leaf 0 through the tool's claim, the public path: held by the deposit's claim delay
    // unless the user opts out, and refused without its own acknowledgement after the warning.
    let mut copts = flow::ClaimOpts {
        leaf: Some(0),
        plaintext: true,
        accept_plaintext: false,
        no_delay: false,
        relayer: gas_drop.as_ref().map(|g| g.2),
        gap: 8,
        send: true,
        yes: true,
        gas_drop: None,
    };
    let held = flow::claim(&eth_side, &user, &mut zone, &ctx, &copts, &mut flow::Out::new(true))
        .await
        .err()
        .context("a claim within its delay is held")?;
    println!("claim within the delay: {held}");
    ensure!(held.to_string().contains("held by your claim delay"), "{held}");
    copts.no_delay = true;
    let mut out = flow::Out::new(true);
    let unacked = flow::claim(&eth_side, &user, &mut zone, &ctx, &copts, &mut out)
        .await
        .err()
        .context("the plaintext path needs its acknowledgement")?;
    println!("plaintext claim without acknowledgement: {unacked}");
    ensure!(
        unacked.to_string().contains("refusing the plaintext path")
            && out.text().contains("WARNING")
            && out.text().contains("Plaintext claim: names on LEZ which deposit paid which account.")
            && out.text().contains("Claiming before your random delay ends"),
        "the plaintext claim warns, names its exposure and is refused without its acknowledgement"
    );
    copts.accept_plaintext = true;
    // Freezes on LEZ (U.08): the bridge program's freeze authority freezes the mints of ETH, then of the whole
    // configuration; the tool refuses the claim before anything is proven or sent, with the program's code, the
    // freeze's end and what to do, naming no deposit; lifted, the same claim goes through below (nothing consumed).
    for token in [Some([0u8; 20]), None] {
        lez_freeze(&mut zone, &p, &cid, token, true, false).await?;
        let before = zone.marks();
        let e = flow::claim(&eth_side, &user, &mut zone, &ctx, &copts, &mut flow::Out::new(true))
            .await
            .err()
            .context("a claim while mints are frozen")?;
        let scope = if token.is_some() { "of this token" } else { "of the whole configuration" };
        println!("FREEZE (LEZ, claim, mints {scope}): {e}");
        unmoved(&zone, &before, "the frozen claim's refusal")?;
        ensure!(
            e.to_string().contains("exit code 242 FROZEN")
                && e.to_string().contains(&format!("mints (claims) {scope} are frozen until Ethereum time"))
                && e.to_string().contains("the deposit stays claimable")
                && e.to_string().contains("retry once the freeze ends")
                && names_none_of(&format!("{e:#}"), &deposit_values(&big.owned), Some(big.owned.deposit.leaf_index)),
            "the frozen claim's refusal: {e}"
        );
        lez_freeze(&mut zone, &p, &cid, token, true, true).await?;
    }
    // Verification failures (U.08), each refused before anything is signed with the program's code and what to do,
    // naming no deposit, the zone as it was (nothing handed to it, no block, no nonce used): a private-kind deposit
    // claimed by the plaintext path (252, the tool's `claim --plaintext`), and leaf 0's opening redirected to another
    // account than its commitment names (249, the program's verify_claim), handed to the tool's own claim-and-send step
    // (`flow::claim_row`, what `ferrata claim` runs once it has scanned the deposit) with `--send --yes`, as a faulty
    // or hostile wallet would hand it: `ferrata claim` itself offers no destination.
    let private_kind = rows.iter().find(|r| r.owned.kind == DestKind::Private).context("a private-kind deposit")?;
    let wrong_path = flow::ClaimOpts {
        leaf: Some(private_kind.owned.deposit.leaf_index),
        plaintext: true,
        accept_plaintext: true,
        no_delay: true,
        relayer: None,
        gap: 8,
        send: true,
        yes: true,
        gas_drop: None,
    };
    let before = zone.marks();
    let e = flow::claim(&eth_side, &user, &mut zone, &ctx, &wrong_path, &mut flow::Out::new(true))
        .await
        .err()
        .context("a private-kind deposit by the plaintext path")?;
    println!("VERIFICATION (claim, plaintext path of a private-kind deposit): {e}");
    let o = &private_kind.owned;
    unmoved(&zone, &before, "the plaintext claim of a private-kind deposit")?;
    ensure!(
        e.to_string().contains("refused: LEZ bridge program exit code 252 WRONG_DESTINATION")
            && e.to_string().contains("checked before sending: a private-kind deposit is claimed privately")
            && e.to_string().contains("claim a private-kind deposit privately (the default, without --plaintext)")
            && names_none_of(&format!("{e:#}"), &deposit_values(o), Some(o.deposit.leaf_index)),
        "the plaintext claim of a private-kind deposit: {e:#}"
    );
    let mut redirected = scan::DepositRow { owned: big.owned.clone(), state: big.state.clone() };
    redirected.owned.dest = *user.public_accounts(2)?[1].id.value();
    let list_now = eth::deposit_list(&p, vault, 0, 10_000).await?;
    let history = lez::history_of(&zone, &ctx).await?;
    let before = zone.marks();
    let e = flow::claim_row(
        &eth_side,
        &user,
        &mut zone,
        &ctx,
        &copts,
        &mut flow::Out::new(true),
        &list_now,
        &history,
        &redirected,
    )
    .await
    .err()
    .context("a claim redirected to another account")?;
    println!("VERIFICATION (claim, opening redirected): {e}");
    let o = &big.owned;
    let mut values = deposit_values(o);
    values.push(redirected.owned.dest.to_vec());
    unmoved(&zone, &before, "the redirected claim")?;
    ensure!(
        e.to_string().contains("refused: LEZ bridge program exit code 249 CLAIM_INVALID")
            && e.to_string().contains("checked before sending with the program's verify_claim")
            && e.to_string().contains("claim it with the seed that made it, to the destination it commits to")
            && names_none_of(&format!("{e:#}"), &values, Some(o.deposit.leaf_index)),
        "the redirected claim: {e:#}"
    );
    // The control: the same command, once nothing refuses it, sends one transaction, and the marks see it.
    let before = zone.marks();
    let claimed0 = flow::claim(&eth_side, &user, &mut zone, &ctx, &copts, &mut flow::Out::new(true)).await?;
    let after = zone.marks();
    ensure!(
        after.submitted == before.submitted + 1 && after.block == before.block + 1,
        "the claim sent one transaction: {before:?} -> {after:?}"
    );
    let plan = claimed0.plan;
    println!("claim: {:?}; N {}", claimed0.sent, hx(&plan.nullifier));
    let holder = AccountId::new(big.owned.dest);
    ensure!(zone.world.balance(holder, &cid) == 10_000_000_000_000_000_000, "the holder holds 10 ETH on LEZ");
    if let Some((receipt_json, dctx, relayer_lez)) = &gas_drop {
        use ferrata_cli::relayer::{self as rl, DropBook, DropRefusal};
        let fee = zone.world.balance(AccountId::new(*relayer_lez), &cid);
        ensure!(
            fee == plan.relay_fee && fee > 0,
            "the claim minted the relay fee to the relayer: {fee} of {}",
            plan.relay_fee
        );
        let history = lez::history_of(&zone, &ctx).await?;
        let after =
            rl::check_gas_drop(receipt_json, dctx, |n| lez::is_claimed(&ctx, &history, n), &DropBook::default());
        ensure!(after == Err(DropRefusal::AlreadyClaimed), "a relayer with no record refuses a claimed N: {after:?}");
        println!("GAS DROP: the claim named the relayer, minted it the relay fee {fee}; afterwards any relayer refuses the receipt (claimed)");
    }

    // A second claim of the same deposit: the tool refuses it with the program's code, and the
    // program, sent the same call without the tool's check, exits with that code.
    let history = lez::history(&ctx, &zone.applied(program).await?);
    let again =
        lez::plan_claim(&zone, &ctx, &leaves, &big.owned, &history, false, None, None).await.expect_err("claimed");
    println!("second claim: {again}");
    ensure!(
        again.to_string().contains("250 ALREADY_CLAIMED")
            && names_none_of(&format!("{again:#}"), &deposit_values(&big.owned), Some(big.owned.deposit.leaf_index)),
        "the tool names 250, and not the deposit: {again:#}"
    );
    let report = zone.settle(&plan.call);
    ensure!(report.exit_code == Some(250), "the program exits 250, got {:?}", report.exit_code);

    // Burns: eleven of 0.1 ETH from public /0, released to the depositor's address. The first through
    // the tool's burn: the public path is refused without its acknowledgement, after the warning.
    let keys = user.bridge_keys();
    let recipient = keystores[0].1;
    let mut bopts = flow::BurnOpts {
        public: true,
        accept_public: false,
        account: 0,
        token: eth::ETH,
        amount: "0.1".into(),
        recipient,
        send: true,
        yes: true,
        journal: None,
        gas_drops: None,
        network: None,
    };
    let mut out = flow::Out::new(true);
    let unacked = flow::burn(&eth_side, &user, &mut zone, &ctx, &bopts, &mut out)
        .await
        .err()
        .context("a public burn needs its acknowledgement")?;
    ensure!(
        unacked.to_string().contains("refusing to burn from a public account")
            && out
                .text()
                .contains("Burn from a public account: the holder, its balance and the burn are public on LEZ.")
            && out.text().contains("The recipient is an address you used before"),
        "the public burn warns and is refused without its acknowledgement: {unacked}"
    );
    bopts.accept_public = true;
    // Freezes on LEZ (U.08): the burns of ETH, then of the whole configuration, frozen: the tool refuses the burn
    // before sending (the zone as it was), naming no burn; lifted, the same burn goes through, and the refusals are
    // checked against its values (its tag, `c_out`, holder, recipient, counter).
    let mut frozen_burns = Vec::new();
    for token in [Some([0u8; 20]), None] {
        lez_freeze(&mut zone, &p, &cid, token, false, false).await?;
        let before = zone.marks();
        let e = flow::burn(&eth_side, &user, &mut zone, &ctx, &bopts, &mut flow::Out::new(true))
            .await
            .err()
            .context("a burn while burns are frozen")?;
        let scope = if token.is_some() { "of this token" } else { "of the whole configuration" };
        println!("FREEZE (LEZ, burn, burns {scope}): {e}");
        unmoved(&zone, &before, "the frozen burn's refusal")?;
        ensure!(
            e.to_string().contains("exit code 242 FROZEN")
                && e.to_string().contains(&format!("burns {scope} are frozen until Ethereum time"))
                && e.to_string().contains("nothing is burned: the holding stays yours")
                && names_nothing(&format!("{e:#}")),
            "the frozen burn's refusal: {e}"
        );
        frozen_burns.push(format!("{e:#}"));
        lez_freeze(&mut zone, &p, &cid, token, false, true).await?;
    }
    let first_burn = flow::burn(&eth_side, &user, &mut zone, &ctx, &bopts, &mut flow::Out::new(true)).await?;
    let burn_named = vec![
        first_burn.plan.note.r.to_vec(),
        first_burn.plan.note.c_out.to_vec(),
        holder.value().to_vec(),
        recipient.as_slice().to_vec(),
    ];
    for e in &frozen_burns {
        ensure!(names_none_of(e, &burn_named, Some(first_burn.plan.i)), "the frozen burn's refusal names it: {e}");
    }
    for _ in 0..10 {
        let history = lez::history(&ctx, &zone.applied(program).await?);
        let plan = lez::plan_burn(
            &zone,
            &ctx,
            &keys,
            BurnFrom::Public(holder),
            [0; 20],
            100_000_000_000_000_000,
            recipient.into_array(),
            &history,
        )
        .await?;
        zone.submit(&plan.call).await?;
    }
    ensure!(
        zone.world.balance(holder, &cid) == 10_000_000_000_000_000_000 - 1_100_000_000_000_000_000,
        "11 burns debited"
    );
    // More than the holding: refused before sending with 254.
    let history = lez::history(&ctx, &zone.applied(program).await?);
    let too_much = lez::plan_burn(
        &zone,
        &ctx,
        &keys,
        BurnFrom::Public(holder),
        [0; 20],
        10_000_000_000_000_000_000,
        recipient.into_array(),
        &history,
    )
    .await
    .expect_err("more than held");
    ensure!(
        too_much.to_string().contains("254 INSUFFICIENT_BALANCE")
            && names_none_of(&format!("{too_much:#}"), &burn_named[2..], None),
        "{too_much:#}"
    );

    // The checkpoint of all eleven burns (stand-in), then the release of the first.
    let history = lez::history(&ctx, &zone.applied(program).await?);
    ensure!(history.burns.len() == 11, "11 burns published");
    let burn_leaves: Vec<Hash> = history
        .burns
        .iter()
        .map(|b| burn_leaf(&cid, b.burn_index, &b.note.token, &b.note.amount, &b.note.c_out, &b.note.r))
        .collect();
    let burn_root = Tree::from_leaves(burn_leaves).map_err(|_| anyhow!("tree"))?.root();
    // The relayer's burn watch (with FERRATA_GASDROP_ELF): 11 burns on the zone, none accepted, no candidate
    // live: a checkpoint is needed; once the root of all 11 is accepted, none is.
    let watch = ferrata_cli::relayer::Relayer { vault, me: ANVIL0, accept_unpaid: false };
    if gas_drop.is_some() {
        let accepted = eth::accepted_burn_roots(&p, vault, 0, 10_000).await?;
        let live = watch.candidates(&p, 0).await?.len();
        let need = ferrata_cli::relayer::checkpoint_needed(history.burns.len() as u64, &accepted, live);
        ensure!(need == Some((11, 0)), "the burn watch asks for a checkpoint of 11 burns: {need:?}");
    }
    accept_burn_root(&p, vault, B256::from(cid), burn_root, 11, true).await?;
    if gas_drop.is_some() {
        let accepted = eth::accepted_burn_roots(&p, vault, 0, 10_000).await?;
        let live = watch.candidates(&p, 0).await?.len();
        let need = ferrata_cli::relayer::checkpoint_needed(history.burns.len() as u64, &accepted, live);
        ensure!(need.is_none() && live == 0, "no checkpoint needed once 11 burns are accepted: {need:?}, {live} live");
        println!("BURN WATCH: 11 burns on the zone -> checkpoint needed (0 accepted); after acceptance -> none");
    }
    let released = eth::released_nullifiers(&p, vault, 0, 10_000).await?;
    let (burn_rows, next_i) = scan::burns(&user, &ctx, &history, &released);
    ensure!(burn_rows.len() == 11 && next_i == 11, "11 burns of the user, next counter 11");
    let roots = eth::accepted_burn_roots(&p, vault, 0, 10_000).await?;
    let (root_b, count) = *roots.last().expect("an accepted burn root");
    ensure!(root_b.0 == burn_root && count == 11, "the vault accepted the burn root");
    // The release of burn 0 through the tool's release, sent by the user (`--self-send`): a dry run
    // first (the root by the wallet's rule, the cost, the exposure), then signed and sent.
    let self_opts = |seal: Option<Bytes>, send: bool| flow::ReleaseOpts {
        burn: Some(0),
        journal: None,
        via: flow::Via::Myself,
        seal,
        job_out: None,
        input_out: None,
        program: None,
        prove: false,
        send,
        yes: send,
        confirmations: 1,
    };
    let mut out = flow::Out::new(true);
    let dry = flow::release_burn(&eth_side, &user, &mut zone, &ctx, None, &self_opts(None, false), &mut out).await?;
    ensure!(
        dry.root.count == 11
            && dry.root.root.0 == burn_root
            && out.text().contains("the newest accepted root whose release delay (")
            && out.text().contains("Elevated: a timing or fee-payer correlation")
            && out.text().contains("You send the release yourself")
            && out.text().contains("not sent: a dry run"),
        "the self-sent release's dry run"
    );
    let rplan = dry.plan;
    let release_image = eth::view(&p, vault, ICheckpoints::releaseImageIdCall {}, None).await?;
    let verifier: Address = std::env::var("FERRATA_E2E_VERIFIER")?.parse()?;
    // Stand-in for the Groth16 proof: the mock verifier accepts the exact journal.
    anvil_send(
        &p,
        verifier,
        IMock::acceptCall { imageId: release_image, journalDigest: B256::from(sha256(&rplan.journal)) }.abi_encode(),
        U256::ZERO,
    )
    .await?;
    let before = p.get_balance(recipient).await?;
    let sp = eth::signing_provider(&rpc, eth::keystore_signer(other_ks, &password)?)?;
    let sender = flow::Signer { p: sp.clone(), address: keystores[1].1 };
    // Freezes on Ethereum (U.08): the vault's releases, then ETH's, frozen by its freeze authority: the tool refuses
    // the release before signing with the vault's error, the freeze's end and what to do, naming no burn; lifted,
    // the same release is sent below.
    for (freeze, lift, want) in [
        (
            IFreeze::setReleasesFrozenCall { frozen: true }.abi_encode(),
            IFreeze::setReleasesFrozenCall { frozen: false }.abi_encode(),
            "ReleasesAreFrozen",
        ),
        (
            IFreeze::setTokenReleasesFrozenCall { token: Address::ZERO, frozen: true }.abi_encode(),
            IFreeze::setTokenReleasesFrozenCall { token: Address::ZERO, frozen: false }.abi_encode(),
            "TokenReleasesAreFrozen",
        ),
    ] {
        freezer_send(&p, vault, freeze).await?;
        let nonce = p.get_transaction_count(keystores[1].1).await?;
        let e = flow::release_burn(
            &eth_side,
            &user,
            &mut zone,
            &ctx,
            Some(&sender),
            &self_opts(Some(Bytes::from(SEAL.to_vec())), true),
            &mut flow::Out::new(true),
        )
        .await
        .err()
        .context("a release while releases are frozen")?;
        println!("FREEZE (vault, release): {e}");
        ensure!(
            e.to_string().contains(&format!("vault error {want}()"))
                && e.to_string().contains("checked before sending: frozen until Ethereum time")
                && e.to_string().contains("the burn stays releasable")
                && names_none_of(&format!("{e:#}"), &burn_values(&burn_rows[0].owned), Some(burn_rows[0].owned.i))
                && p.get_transaction_count(keystores[1].1).await? == nonce,
            "the frozen release's refusal: {e}"
        );
        freezer_send(&p, vault, lift).await?;
    }
    let mut out = flow::Out::new(true);
    let sent = flow::release_burn(
        &eth_side,
        &user,
        &mut zone,
        &ctx,
        Some(&sender),
        &self_opts(Some(Bytes::from(SEAL.to_vec())), true),
        &mut out,
    )
    .await?;
    ensure!(sent.plan.nullifier == rplan.nullifier && out.text().contains("network fee  about"), "sent as planned");
    println!("release: {:?}", sent.sent);
    ensure!(
        p.get_balance(recipient).await? - before == U256::from(100_000_000_000_000_000u128),
        "the recipient received 0.1 ETH"
    );
    ensure!(
        eth::view(&p, vault, IVault::nullifierUsedCall { nullifier: B256::from(rplan.nullifier) }, None).await?,
        "NOut used"
    );
    // Twice: the tool refuses it before sending (the signer's nonce unchanged); the vault, sent it anyway, refuses it
    // too.
    let nonce = p.get_transaction_count(keystores[1].1).await?;
    let again = flow::release_burn(
        &eth_side,
        &user,
        &mut zone,
        &ctx,
        Some(&sender),
        &self_opts(Some(Bytes::from(SEAL.to_vec())), true),
        &mut flow::Out::new(true),
    )
    .await
    .err()
    .context("a released burn")?;
    ensure!(
        again.to_string().contains("NullifierAlreadyUsed")
            && again.to_string().contains("checked before sending")
            && names_none_of(&format!("{again:#}"), &burn_values(&burn_rows[0].owned), Some(burn_rows[0].owned.i))
            && p.get_transaction_count(keystores[1].1).await? == nonce,
        "{again:#}"
    );
    let twice =
        eth::send(&sp, keystores[1].1, vault, release::calldata(&rplan, Bytes::from(SEAL.to_vec())), U256::ZERO, 1)
            .await
            .expect_err("released");
    ensure!(twice.to_string().contains("NullifierAlreadyUsed"), "{twice}");

    // ---- The private round trip, on the defaults ------------------------------------------------
    // The wallet's private accounts /0 and /1 on the zone, from the keys LEZ's wallet derives.
    zone.add_private_accounts(&user, 2)?;
    let private0 = user.private_account_ids(1)?[0];
    // 1. A deposit on the defaults through the binary (private /0; 1 ETH): a dry run, then sent.
    let dep_args = |send: bool| -> Vec<String> {
        let mut a = common(&seed_user);
        a.extend(["deposit".to_string(), "--denomination".into(), "1".into()]);
        if send {
            a.extend([
                "--keystore".to_string(),
                user_ks.display().to_string(),
                "--password-file".into(),
                password.display().to_string(),
                "--send".into(),
                "--yes".into(),
            ]);
        } else {
            a.extend(["--from".to_string(), keystores[0].1.to_string()]);
        }
        a
    };
    let dry = ferrata(&dep_args(false).iter().map(String::as_str).collect::<Vec<_>>())?;
    print!("{dry}");
    ensure!(
        dry.contains("your private LEZ account /0")
            && dry.contains("would send: to")
            && dry.contains("cost: ")
            && dry.contains("eth_estimateGas")
            && dry.contains("privacy of these choices: Minimal")
            && !dry.contains("WARNING")
            && dry.contains("not sent: a dry run"),
        "the default deposit's dry run: what it sends, what becomes public, its cost"
    );
    let sent = ferrata(&dep_args(true).iter().map(String::as_str).collect::<Vec<_>>())?;
    print!("{sent}");
    ensure!(sent.contains("sent: "), "the default deposit is sent");
    // 2. Insufficient finality (U.08), then the relayer imports root(17) and ETH's fee range to it. The vault's storage
    //    is recorded at the newest beacon slot, after every record the configuration's clock has read.
    let n2 = eth::view(&p, vault, IVault::nextLeafIndexCall {}, None).await?;
    let leaf_p = n2 - 1;
    let root2 = eth::view(&p, vault, IVault::depositRootCall {}, None).await?;
    let mut storage3 = vec![(word(0), slot(&p, vault, word(0)).await?), (word(1), slot(&p, vault, word(1)).await?)];
    storage3.push((word(2), cid));
    let s2 = zone.world.next_slot();
    let b2 = zone.world.add_block(hb::Block::with_storage(s2, s2, vault.into_array(), &storage3));
    let mut copts = flow::ClaimOpts {
        leaf: Some(leaf_p),
        plaintext: false,
        accept_plaintext: false,
        no_delay: false,
        relayer: None,
        gap: 8,
        send: true,
        yes: true,
        gas_drop: None,
    };
    let pending = scan::deposits(
        &user,
        &ctx,
        &eth::deposit_list(&p, vault, 0, 10_000).await?,
        8,
        &lez::History::default(),
        &HashSet::new(),
    )?
    .into_iter()
    .find(|r| r.owned.deposit.leaf_index == leaf_p)
    .context("leaf 16 is the user's")?
    .owned;
    let named = deposit_values(&pending);
    let not_final = |e: &anyhow::Error| {
        e.to_string().contains("refused: LEZ bridge program exit code 248 LEAF_NOT_IMPORTED")
            && e.to_string().contains(
                "checked before sending: no imported root covers this leaf yet; wait for the next root import",
            )
            && names_none_of(&format!("{e:#}"), &named, Some(leaf_p))
    };
    //    a. Before its root is imported: the private claim is refused before it is proven (248), the zone as it was.
    let before = zone.marks();
    let e = flow::claim(&eth_side, &user, &mut zone, &ctx, &copts, &mut flow::Out::new(true))
        .await
        .err()
        .context("a claim before its root is imported")?;
    println!("NOT FINAL (claim, before its root is imported): {e}");
    unmoved(&zone, &before, "the claim before its root is imported")?;
    ensure!(
        not_final(&e) && !e.to_string().contains("inbound finality depth"),
        "the claim before its root is imported: {e:#}"
    );
    //    b. Inside the depth (F.12's admin depth): the admin sets 64 beacon slots beyond Ethereum finality; the
    //       relayer's import under the newest record waits (242) until the configuration's clock has read a record 64
    //       slots later; meanwhile the claim is refused before it is proven (248), saying how long the depth holds
    //       imports.
    let config_at = ferrata_lez_bridge_wire::config_actor(program, &cid);
    let set_depth = |slots: u64| BridgeMessage::SetDepth { config_id: cid, slots };
    let r = zone.world.role("SetDepth 64 (admin)", hb::ADMIN_SEED, &set_depth(64), config_at);
    zone.record(&r);
    ensure!(r.ok(), "SetDepth 64: {:?}", r.exit_code);
    let r = zone.world.import("ImportRoot 17 inside the depth (relayer)", &cid, b2, root2.0, n2, &vault.into_array());
    zone.record(&r);
    ensure!(r.exit_code == Some(242), "an import inside the depth waits with 242: {:?}", r.exit_code);
    let before = zone.marks();
    let e = flow::claim(&eth_side, &user, &mut zone, &ctx, &copts, &mut flow::Out::new(true))
        .await
        .err()
        .context("a claim inside the depth")?;
    println!("NOT FINAL (claim, its root's import inside the depth): {e}");
    unmoved(&zone, &before, "the claim inside the depth")?;
    ensure!(
        not_final(&e)
            && e.to_string().contains(
                "this configuration imports a root only once the block it reads is 64 beacon slots (about 13 min) past \
                 Ethereum finality (its inbound finality depth, set by its admin)"
            ),
        "the claim inside the depth: {e:#}"
    );
    //    The clock reads a record 64 slots on (a Tick, anyone's); the same import applies; the depth back to 0.
    let tip = loop {
        let now = eth::now(&p).await?.max(zone.world.newest_record_seconds() + 1);
        let b = zone.world.finalize(now);
        if zone.world.blocks[b].slot >= s2 + 64 {
            break b;
        }
    };
    let r = zone.world.tick("Tick: a record 64 slots past the import's", &cid, None, tip);
    zone.record(&r);
    ensure!(r.ok(), "Tick: {:?}", r.exit_code);
    let r = zone.world.import("ImportRoot 17 (relayer)", &cid, b2, root2.0, n2, &vault.into_array());
    ensure!(r.ok(), "ImportRoot 17 past the depth: {:?}", r.exit_code);
    zone.record(&r);
    let r = zone.world.role("SetDepth 0 (admin)", hb::ADMIN_SEED, &set_depth(0), config_at);
    zone.record(&r);
    ensure!(r.ok(), "SetDepth 0: {:?}", r.exit_code);
    let r =
        zone.world.import_fees("ImportFees ETH 1 = none (relayer)", &cid, b2, [0; 20], 1, None, &vault.into_array());
    ensure!(r.ok(), "ImportFees at 17: {:?}", r.exit_code);
    zone.record(&r);
    // 3. The claim on the defaults: held by the deposit's claim delay; past it, private.
    let held = flow::claim(&eth_side, &user, &mut zone, &ctx, &copts, &mut flow::Out::new(true))
        .await
        .err()
        .context("held by its delay")?;
    ensure!(held.to_string().contains("held by your claim delay"), "{held}");
    let list = eth::deposit_list(&p, vault, 0, 10_000).await?;
    let mine = scan::deposits(&user, &ctx, &list, 8, &lez::History::default(), &HashSet::new())?;
    let n_p =
        mine.iter().find(|r| r.owned.deposit.leaf_index == leaf_p).context("leaf 16 is the user's")?.owned.nullifier;
    let opens = flow::claimable_from(list.blocks[leaf_p as usize], user.claim_delay_hours(&n_p));
    let blocks = opens - list.block;
    p.raw_request::<_, serde_json::Value>("anvil_mine".into(), (format!("0x{blocks:x}"),)).await?;
    println!("claim delay: {} h = {blocks} blocks mined", user.claim_delay_hours(&n_p));
    copts.send = false;
    let mut out = flow::Out::new(true);
    let dry = flow::claim(&eth_side, &user, &mut zone, &ctx, &copts, &mut out).await?;
    ensure!(
        dry.sent.is_none()
            && out.text().contains("ClaimPrivately")
            && out.text().contains("privacy of these choices: Minimal")
            && out.text().contains(", passed")
            && out.text().contains("not sent: a dry run")
            && out.text().lines().any(|l| l.starts_with("  anonymity    1 deposit of 1 ETH")
                && l.contains("its amount alone links this claim to it")),
        "the default claim's dry run, with its anonymity set (F.8): the only deposit of 1 ETH under the root"
    );
    println!("{}", out.text().lines().find(|l| l.starts_with("  anonymity")).unwrap_or_default());
    copts.send = true;
    // The private path meets the same freeze (U.08): mints of the whole configuration frozen, the private claim
    // refused before it is proven; lifted, it is proven and settled below.
    lez_freeze(&mut zone, &p, &cid, None, true, false).await?;
    let before = zone.marks();
    let e = flow::claim(&eth_side, &user, &mut zone, &ctx, &copts, &mut flow::Out::new(true))
        .await
        .err()
        .context("a private claim while mints are frozen")?;
    println!("FREEZE (LEZ, private claim): {e}");
    unmoved(&zone, &before, "the frozen private claim's refusal")?;
    ensure!(
        e.to_string().contains("exit code 242 FROZEN")
            && e.to_string().contains("the deposit stays claimable")
            && names_none_of(&format!("{e:#}"), &deposit_values(&pending), Some(leaf_p)),
        "the frozen private claim's refusal: {e}"
    );
    lez_freeze(&mut zone, &p, &cid, None, true, true).await?;
    let before = zone.marks();
    let claimed_p = flow::claim(&eth_side, &user, &mut zone, &ctx, &copts, &mut flow::Out::new(true)).await?;
    let after = zone.marks();
    ensure!(
        after.submitted == before.submitted + 1 && after != before,
        "the private claim sent one transaction (the control of the refused ones): {before:?} -> {after:?}"
    );
    println!("private claim: {:?}", claimed_p.sent);
    let held_p = flow::private_holding(&mut zone, &ctx, private0, &[0; 20]).await?;
    ensure!(held_p == ETH_1, "private /0 holds 1 ETH: {held_p}");
    ensure!(
        !zone.applied(program).await?.iter().any(|a| matches!(
            BridgeMessage::try_from_slice(&a.message),
            Ok(BridgeMessage::Claim { witness, .. }) if witness.leaf_index == leaf_p
        )),
        "no plaintext claim names the deposit"
    );
    // Again: refused before sending (250), and by the program itself when proven anyway.
    let before = zone.marks();
    let again = flow::claim(&eth_side, &user, &mut zone, &ctx, &copts, &mut flow::Out::new(true))
        .await
        .err()
        .context("claimed")?;
    unmoved(&zone, &before, "the repeated private claim")?;
    ensure!(
        again.to_string().contains("250 ALREADY_CLAIMED")
            && names_none_of(&format!("{again:#}"), &deposit_values(&pending), Some(leaf_p)),
        "{again:#}"
    );
    let Some(Mention::PrivateSelf(me)) = claimed_p.plan.call.mentions.first().cloned() else {
        bail!("a private claim")
    };
    let replay = zone.settle_private(&claimed_p.plan.call, me).expect_err("a replayed private claim");
    ensure!(replay.to_string().contains("250"), "the program refuses the replay: {replay}");
    // 4. The burn on the defaults: from private /0, to a fresh recipient; a dry run, then sent.
    let fresh = PrivateKeySigner::random().address();
    let mut bopts = flow::BurnOpts {
        public: false,
        accept_public: false,
        account: 0,
        token: eth::ETH,
        amount: "1".into(),
        recipient: fresh,
        send: false,
        yes: false,
        journal: None,
        gas_drops: None,
        network: None,
    };
    let mut out = flow::Out::new(true);
    flow::burn(&eth_side, &user, &mut zone, &ctx, &bopts, &mut out).await?;
    ensure!(
        out.text().contains("Burn from a private account: the holder stays private.")
            && out.text().contains("privacy of these choices: Minimal")
            && out.text().contains("  cost ")
            && out.text().contains("not sent: a dry run"),
        "the default burn's dry run"
    );
    bopts.send = true;
    bopts.yes = true;
    let burned = flow::burn(&eth_side, &user, &mut zone, &ctx, &bopts, &mut flow::Out::new(true)).await?;
    ensure!(flow::private_holding(&mut zone, &ctx, private0, &[0; 20]).await? == 0, "the private holding is burned");
    // Refusals: more than the holding (254), not a denomination (251), more decimals than the token's.
    for (amount, want) in
        [("1", "254 INSUFFICIENT_BALANCE"), ("0.5", "251 NOT_REGISTERED"), ("0.1000000000000000001", "decimal")]
    {
        let o = flow::BurnOpts { amount: amount.into(), ..bopts.clone() };
        let before = zone.marks();
        let e = flow::burn(&eth_side, &user, &mut zone, &ctx, &o, &mut flow::Out::new(false))
            .await
            .err()
            .context("refused")?;
        println!("burn {amount}: {e:#}");
        // Not the holder, not the recipient: the only values of a burn that was never made.
        let named = [private0.value().to_vec(), fresh.as_slice().to_vec()];
        unmoved(&zone, &before, &format!("burn {amount}"))?;
        ensure!(
            format!("{e:#}").contains(want) && names_none_of(&format!("{e:#}"), &named, None),
            "burn {amount}: {e:#}"
        );
    }
    // 5. The checkpoint of all twelve burns, finalised but in its release delay: the private burn is
    //    refused (only that root covers it); burn 1 is planned against the older root meanwhile.
    let history = lez::history_of(&zone, &ctx).await?;
    ensure!(history.burns.len() == 12, "the private burn's note is on the zone: {} burns", history.burns.len());
    let burn_leaves2: Vec<Hash> = history
        .burns
        .iter()
        .map(|b| burn_leaf(&cid, b.burn_index, &b.note.token, &b.note.amount, &b.note.c_out, &b.note.r))
        .collect();
    let burn_root2 = Tree::from_leaves(burn_leaves2).map_err(|_| anyhow!("tree"))?.root();
    // Insufficient finality on the way out (U.08): with the checkpoint of the twelve burns a candidate in its wait, not
    // final, the release of the private burn (only that root covers it), asked of the tool with a signer and --send
    // --yes, is refused before anything is signed with the vault's error and what to do, naming no burn: the signer's
    // nonce is unchanged. Then the candidate is finalised.
    let candidate = propose_burn_root(&p, vault, B256::from(cid), burn_root2, 12).await?;
    // The private burn as the wallet finds it from the seed: its values, which no refusal may name.
    let private_burn = scan::burns(&user, &ctx, &history, &eth::released_nullifiers(&p, vault, 0, 10_000).await?)
        .0
        .into_iter()
        .find(|b| b.owned.i == burned.plan.i)
        .context("the private burn, found from the seed")?
        .owned;
    let private_burn_named = burn_values(&private_burn);
    // The tool's own self-sent release of it, with a signer, a seal and --send --yes.
    let send_myself = |i: u64| flow::ReleaseOpts {
        burn: Some(i),
        journal: None,
        via: flow::Via::Myself,
        seal: Some(Bytes::from(SEAL.to_vec())),
        job_out: None,
        input_out: None,
        program: None,
        prove: false,
        send: true,
        yes: true,
        confirmations: 1,
    };
    let nonce = p.get_transaction_count(keystores[1].1).await?;
    let e = flow::release_burn(
        &eth_side,
        &user,
        &mut zone,
        &ctx,
        Some(&sender),
        &send_myself(burned.plan.i),
        &mut flow::Out::new(true),
    )
    .await
    .err()
    .context("a release before its checkpoint finalises")?;
    println!("NOT FINAL (release, its checkpoint a candidate): {e}");
    ensure!(
        e.to_string().contains("refused: vault error BurnRootNotAccepted()")
            && e.to_string().contains(
                "checked before sending: no finalized checkpoint covers this burn yet; wait for the next checkpoint"
            )
            && names_none_of(&format!("{e:#}"), &private_burn_named, Some(private_burn.i))
            && p.get_transaction_count(keystores[1].1).await? == nonce,
        "the release before its checkpoint finalises: {e:#}"
    );
    finalize_burn_root(&p, vault, B256::from(cid), candidate, false).await?;
    let job_path = dir.join("release-job.json");
    let _ = std::fs::remove_file(&job_path);
    let ropts = |burn: u64, seal: Option<Bytes>, job: Option<PathBuf>| flow::ReleaseOpts {
        burn: Some(burn),
        journal: None,
        via: flow::Via::Relayer(None),
        seal,
        job_out: job,
        input_out: None,
        program: None,
        prove: false,
        send: false,
        yes: false,
        confirmations: 1,
    };
    let early = flow::release_burn(
        &eth_side,
        &user,
        &mut zone,
        &ctx,
        None,
        &ropts(burned.plan.i, None, None),
        &mut flow::Out::new(true),
    )
    .await
    .err()
    .context("in its release delay")?;
    println!("release in the delay: {early}");
    ensure!(
        early.to_string().contains("ReleaseDelayNotElapsed")
            && names_none_of(&format!("{early:#}"), &private_burn_named, Some(private_burn.i)),
        "{early:#}"
    );
    let older =
        flow::release_burn(&eth_side, &user, &mut zone, &ctx, None, &ropts(1, None, None), &mut flow::Out::new(true))
            .await?;
    ensure!(older.root.count == 11, "burn 1 goes against the older root while the newer is in its delay");
    pass_release_delay(&p).await?;
    // 6. The release on the defaults: through any relayer, paid the registered relay fee; the tool
    //    writes the relayer's job (the journal and the proof, nothing secret).
    let planned = flow::release_burn(
        &eth_side,
        &user,
        &mut zone,
        &ctx,
        None,
        &ropts(burned.plan.i, None, None),
        &mut flow::Out::new(true),
    )
    .await?;
    ensure!(planned.root.count == 12, "the newest root, once its delay has passed");
    // Verification failure on the way out (U.08; an internal review's M1): the self-sent release with a seal the verifier rejects
    // (no proof of its journal: the mock verifier was told of none, as with a seal made for another recipient, relay
    // fee or root) is refused by the vault's eth_call before anything is signed, as the verifier's VerificationFailed
    // with what to do, naming no burn; the signer's nonce is unchanged.
    let nonce = p.get_transaction_count(keystores[1].1).await?;
    let e = flow::release_burn(
        &eth_side,
        &user,
        &mut zone,
        &ctx,
        Some(&sender),
        &send_myself(burned.plan.i),
        &mut flow::Out::new(true),
    )
    .await
    .err()
    .context("a release whose proof the verifier rejects")?;
    println!("VERIFICATION (release, proof rejected by the verifier): {e}");
    ensure!(
        e.to_string().contains("refused: vault error VerificationFailed() (eth_call, nothing sent)")
            && e.to_string().contains("the RISC Zero verifier rejects the proof")
            && e.to_string().contains("prove it again for this journal (`ferrata release --prove`")
            && names_none_of(&format!("{e:#}"), &private_burn_named, Some(private_burn.i))
            && p.get_transaction_count(keystores[1].1).await? == nonce,
        "the release whose proof the verifier rejects: {e:#}"
    );
    anvil_send(
        &p,
        verifier,
        IMock::acceptCall { imageId: release_image, journalDigest: B256::from(sha256(&planned.plan.journal)) }
            .abi_encode(),
        U256::ZERO,
    )
    .await?;
    let mut out = flow::Out::new(true);
    let rel = flow::release_burn(
        &eth_side,
        &user,
        &mut zone,
        &ctx,
        None,
        &ropts(burned.plan.i, Some(Bytes::from(SEAL.to_vec())), Some(job_path.clone())),
        &mut out,
    )
    .await?;
    ensure!(
        rel.job.is_some()
            && out.text().contains("submitted by any relayer")
            && out.text().contains("privacy of these choices: Minimal"),
        "the default release goes through a relayer"
    );
    // The relayer takes the job from its inbox, checks it, sends it, and is paid the relay fee.
    use ferrata_cli::relayer::{self as rl, Job, Relayer};
    let job: Job = serde_json::from_slice(&std::fs::read(&job_path)?)?;
    let Job::Release { journal, seal } = job else { bail!("a release job") };
    let relayer_eth = keystores[1].1;
    let relayer = Relayer { vault, me: relayer_eth, accept_unpaid: false };
    let view = relayer.view(&sp).await?;
    let (_, input) = relayer.plan_release(&sp, &view, &rl::unhex(&journal)?, Bytes::from(rl::unhex(&seal)?)).await?;
    let before = p.get_balance(fresh).await?;
    let rc = relayer.submit(&sp, input).await?;
    let (_, paid) = rl::relay_fee_paid(&rc, relayer_eth).context("a Release paying the relayer")?;
    let relay_fee = flow::release_fees(&eth_side, eth::ETH, ETH_1).await?.3;
    let got = p.get_balance(fresh).await? - before;
    ensure!(
        paid == U256::from(relay_fee) && got == U256::from(ETH_1 - relay_fee),
        "the fresh recipient received {got}, the relayer {paid} (relay fee {relay_fee})"
    );
    println!("PRIVATE ROUND TRIP: 1 ETH deposited to private /0, claimed privately after its delay, burned from private /0, released by a relayer to a fresh address ({got} wei; relay fee {paid})");

    // ---- Recovery from the seed alone ------------------------------------------------------------
    // A fresh client with only the seed file: every deposit and its state, every burn and whether it
    // is released, the holdings. (The private holdings are the LEZ wallet's view: the harness stands in
    // for its sync, keeping the states the chain committed.)
    let wiped = Seed::from_phrase(&std::fs::read_to_string(&seed_user)?)?;
    let mut out = flow::Out::new(true);
    let rec = flow::recover(&eth_side, &wiped, 8, Some((&mut zone, &ctx)), true, None, &mut out).await?;
    let want: Vec<(DestKind, u32)> = expected_unclaimed
        .iter()
        .map(|(k, a)| (if *k == "private" { DestKind::Private } else { DestKind::Public }, *a))
        .collect();
    println!(
        "RECOVERY: {} deposits ({} claimed, {} unclaimed), {} burns ({} released), holdings {:?}; {} deposits in the vault",
        rec.deposits,
        rec.claimed,
        rec.unclaimed.len(),
        rec.burns,
        rec.released,
        rec.holdings,
        n2
    );
    ensure!(rec.unclaimed == want, "the unclaimed deposits found are the ten made: {:?}", rec.unclaimed);
    ensure!(
        rec.deposits == 12 && rec.claimed == 2 && rec.burns == 12 && rec.released == 2 && rec.next_burn_counter == 12,
        "recovery: deposits, claims (one private), burns (one private), releases"
    );
    ensure!(
        rec.holdings == vec![(DestKind::Public, 0, eth::ETH, 10 * ETH_1 - 11 * ETH_1 / 10)],
        "holdings: 8.9 ETH at public /0, nothing left at private /0: {:?}",
        rec.holdings
    );
    let history = lez::history_of(&zone, &ctx).await?;
    let n = n2;

    // Status, the zone's part (the library function `ferrata status --sequencer` prints).
    let zone_lines = lez::zone_status(&zone, &ctx, &history).await?;
    for line in &zone_lines {
        println!("{line}");
    }
    ensure!(
        zone_lines
            .iter()
            .any(|l| l.contains(&format!("newest imported root: root(cid, {n})")) && l.contains("from header(id, ")),
        "status names the newest import and the record it read"
    );
    ensure!(zone_lines.iter().any(|l| l == "  burns published: 12"), "status counts the burns");

    // ---- An ERC-20 shaped like USDT (its transfers return no boolean): the deposit's dry run asks for
    //      the allowance; `--approve` sends the approve, then the deposit, through the binary.
    // Deployed from the vault's build artifact through this provider (one nonce manager for anvil's account).
    let artifact: serde_json::Value = serde_json::from_slice(&std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../vault/out/MockTokens.sol/MockUSDT.json"),
    )?)?;
    let code = hex::decode(
        artifact["bytecode"]["object"].as_str().context("the artifact's bytecode")?.trim_start_matches("0x"),
    )?;
    let ctor = <(sol_data::String, sol_data::String, sol_data::Uint<8>) as SolType>::abi_encode_params(&(
        "Tether USD".to_string(),
        "USDT".to_string(),
        6u8,
    ));
    let deploy = TransactionRequest::default().with_from(ANVIL0).with_deploy_code([code, ctor].concat());
    let usdt = p.send_transaction(deploy).await?.get_receipt().await?.contract_address.context("MockUSDT's address")?;
    ensure!(!p.get_code_at(usdt).await?.is_empty(), "MockUSDT deployed");
    anvil_send(
        &p,
        usdt,
        IAdmin::mintCall { to: keystores[0].1, amount: U256::from(10_000_000u64) }.abi_encode(),
        U256::ZERO,
    )
    .await?;
    let unlimited = IAdmin::CapParams { capacity: U256::MAX, refillPerSecond: U256::ZERO };
    let params = IAdmin::TokenParams {
        denominations: vec![U256::from(1_000_000u64)],
        relayFees: vec![U256::from(10_000u64)],
        lezUnit: U256::from(1u64),
        unitWeight: U256::from(1u64),
        depositCap: unlimited.clone(),
        releaseCap: unlimited,
        releaseDelay: U256::from(120u64),
        lezDefinition: B256::repeat_byte(0x7e),
        decimals: 6,
        checkpointFees: vec![U256::ZERO],
    };
    anvil_send(&p, vault, IAdmin::registerTokenCall { token: usdt, p: params }.abi_encode(), U256::ZERO).await?;
    let usdt_arg = usdt.to_string();
    let dry = dry_deposit(&["--token", &usdt_arg, "--denomination", "0"])?;
    print!("{dry}");
    ensure!(
        dry.contains("1 USDT (1000000 base units, 6 decimals)") && dry.contains("add --approve"),
        "the ERC-20 deposit's dry run asks for the allowance"
    );
    let mut args = common(&seed_user);
    args.extend(
        [
            "deposit",
            "--token",
            &usdt_arg,
            "--denomination",
            "0",
            "--approve",
            "--keystore",
            &user_ks.display().to_string(),
            "--password-file",
            &password.display().to_string(),
            "--send",
            "--yes",
        ]
        .map(String::from),
    );
    let sent = ferrata(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
    print!("{sent}");
    let held = eth::view(&p, usdt, IAdmin::balanceOfCall { who: vault }, None).await?;
    ensure!(
        sent.contains("would send first: approve(")
            && sent.contains("approved: ")
            && sent.contains("sent: ")
            && held == U256::from(1_010_000u64),
        "approve, then deposit: the vault holds {held} (1 USDT and the 0.01 USDT relay fee)"
    );
    let scan_out =
        ferrata(&[common(&seed_user), vec!["scan".into()]].concat().iter().map(String::as_str).collect::<Vec<_>>())?;
    ensure!(
        scan_out.contains("deposits of this seed: 13") && scan_out.contains("1 USDT (1000000 base units, 6 decimals)"),
        "the scan finds the USDT deposit"
    );
    println!(
        "ERC-20: a USDT-shaped token approved and deposited through the binary; the vault holds {held} base units"
    );

    // The relayer's release (cli/src/relayer.rs), with `FERRATA_GASDROP_ELF`: the holder of burn 1 opts in
    // with a release that pays the relayer the registered relay fee; the relayer checks and sends it
    // and is paid; its own changed calls (recipient redirected) are refused by the vault, and another
    // relayer refuses the job.
    if gas_drop.is_some() {
        use ferrata_cli::relayer::{self as rl, Relayer};
        let relayer_eth = keystores[1].1;
        let fee = eth::deposit_price(&p, vault, eth::ETH, 0).await?.relay_fee;
        let plan1 =
            release::plan(&ctx, &history, &burn_rows[1].owned, root_b.0, count, 0, relayer_eth.into_array(), fee)?;
        let verifier: Address = std::env::var("FERRATA_E2E_VERIFIER")?.parse()?;
        anvil_send(
            &p,
            verifier,
            IMock::acceptCall { imageId: release_image, journalDigest: B256::from(sha256(&plan1.journal)) }
                .abi_encode(),
            U256::ZERO,
        )
        .await?;
        let sp = eth::signing_provider(&rpc, eth::keystore_signer(other_ks, &password)?)?;
        let r = Relayer { vault, me: relayer_eth, accept_unpaid: false };
        let view = r.view(&sp).await?;
        let (req, input) = r.plan_release(&sp, &view, &plan1.journal, Bytes::from(SEAL.to_vec())).await?;
        let mut redirected = req.clone();
        redirected.recipient = relayer_eth;
        let refused = eth::send(
            &sp,
            relayer_eth,
            vault,
            eth::release_calldata(redirected, Bytes::from(SEAL.to_vec())),
            U256::ZERO,
            1,
        )
        .await
        .expect_err("a redirected release");
        let other = Relayer { vault, me: ANVIL0, accept_unpaid: false };
        let not_ours = other
            .plan_release(&p, &view, &plan1.journal, Bytes::from(SEAL.to_vec()))
            .await
            .expect_err("another relayer's");
        let before = p.get_balance(relayer_eth).await?;
        let rc = r.submit(&sp, input).await?;
        let (_, paid) = rl::relay_fee_paid(&rc, relayer_eth).context("a Release paying the relayer")?;
        let gas = U256::from(rc.gas_used) * U256::from(rc.effective_gas_price);
        let gained = p.get_balance(relayer_eth).await? + gas - before;
        ensure!(
            fee > 0 && paid == U256::from(fee) && gained == U256::from(fee),
            "the relayer was paid {gained}, the fee is {fee}"
        );
        println!("RELAYER: burn 1 released by the relayer ({} gas), relay fee {fee} wei paid to it; redirected recipient refused: {refused}; another relayer: {not_ours}", rc.gas_used);
    }
    counters_pairing_and_quiet_dry_runs(&eth_side, &user, &mut zone, &ctx, &dir).await?;
    Ok(())
}

// ---- burn counters, the pairing check, quiet dry runs (the reviews of 22 November 2026) ----------

/// A zone whose sequencer takes a transaction and settles it later ([`Deferred::flush`]): burns in flight.
/// `kill_next`: the next transaction reaches the sequencer, but the tool never learns it (killed after sending). Every
/// actor read is recorded.
struct Deferred<'a> {
    inner: &'a mut HarnessLedger,
    queue: Vec<LezCall>,
    kill_next: bool,
    reads: std::cell::RefCell<Vec<Actor>>,
}

impl Deferred<'_> {
    async fn flush(&mut self) -> Vec<Result<String>> {
        let mut out = Vec::new();
        for call in std::mem::take(&mut self.queue) {
            out.push(self.inner.submit(&call).await);
        }
        out
    }
}

impl Ledger for Deferred<'_> {
    async fn applied(&self, program: AccountId) -> Result<Vec<Applied>> {
        self.inner.applied(program).await
    }

    async fn included(&self, program: AccountId) -> Result<lez::Included> {
        self.inner.included(program).await
    }

    async fn private_account(&mut self, id: AccountId) -> Result<Option<lee::Account>> {
        self.inner.private_account(id).await
    }

    async fn actor_state(&self, actor: Actor) -> Result<Vec<u8>> {
        self.reads.borrow_mut().push(actor);
        self.inner.actor_state(actor).await
    }

    async fn submit(&mut self, call: &LezCall) -> Result<String> {
        self.queue.push(call.clone());
        if std::mem::take(&mut self.kill_next) {
            bail!("killed: the transaction reached the sequencer, the tool never learnt it");
        }
        Ok(format!("{} (queued)", call.what))
    }
}

/// The tags of the zone's applied burns of `ctx`, and of its refused ones.
async fn tags(zone: &HarnessLedger, ctx: &Context) -> Result<(Vec<Hash>, Vec<Hash>)> {
    let h = lez::history_of(zone, ctx).await?;
    Ok((h.burns.iter().map(|b| b.note.r).collect(), h.refused_burns.iter().map(|b| b.note.r).collect()))
}

/// The reviews of 22 November 2026 (the final review's H1; the bridge code review's H-1, M-1 and M-2), through the tool's
/// own functions on the zone of this test:
/// - two burns in flight from two holders of the seed take two counters (H-1), and both are applied;
/// - after a burn killed with its transaction at the sequencer, another burn takes a fresh counter (H1), never the
///   killed one's, and both are applied; the killed burn asked again once applied burns nothing; the same burn asked
///   again while unseen takes its counter again (the same note: one of the two is applied, the other refused, one
///   denomination burnt); a dry run reserves nothing; a burn refused on the zone uses its counter;
/// - a recovery from the seed alone, with no journal, finds every burn, each with its own `NOut`;
/// - `ferrata recover` with a burn journal lists only the burns reserved there that the zone does not show (review
///   N1), and a damaged journal makes `recover` and `release` fail at once with an error naming it (review L-C);
/// - a burn under an Ethereum RPC of another chain than the configuration's is refused before anything is planned
///   (M-1);
/// - a claim's dry run, and a claim held by its delay, never ask the zone for the deposit's nullifier account (M-2).
async fn counters_pairing_and_quiet_dry_runs(
    eth_side: &flow::Eth,
    user: &Seed,
    zone: &mut HarnessLedger,
    ctx: &Context,
    dir: &Path,
) -> Result<()> {
    let program = ctx.program;
    let journal = dir.join("burn-journal");
    let _ = std::fs::remove_dir_all(&journal);
    std::fs::create_dir_all(&journal)?;
    let journal_file = journal.join(ferrata_cli::burns::FILE);
    // Public /1 gets 0.1 ETH: the plaintext claim of its unclaimed deposit (a second holder of the seed beside /0).
    let list = eth::deposit_list(&eth_side.p, eth_side.vault, 0, 10_000).await?;
    let history = lez::history_of(&*zone, ctx).await?;
    let rows = scan::deposits(user, ctx, &list, 8, &history, &HashSet::new())?;
    let pick = |kind: DestKind, account: u32| {
        rows.iter()
            .find(|r| {
                r.owned.kind == kind && r.owned.account_index == account && r.state != scan::DepositState::Claimed
            })
            .map(|r| r.owned.deposit.leaf_index)
            .ok_or_else(|| anyhow!("an unclaimed deposit to {kind:?} /{account}"))
    };
    let leaf1 = pick(DestKind::Public, 1)?;
    let copts = flow::ClaimOpts {
        leaf: Some(leaf1),
        plaintext: true,
        accept_plaintext: true,
        no_delay: true,
        relayer: None,
        gap: 8,
        send: true,
        yes: true,
        gas_drop: None,
    };
    flow::claim(eth_side, user, zone, ctx, &copts, &mut flow::Out::new(false)).await?;
    let publics = user.public_accounts(2)?;
    let tenth = 100_000_000_000_000_000u128;
    ensure!(zone.world.balance(publics[1].id, &ctx.cid) == tenth, "public /1 holds 0.1 ETH");

    let bopts = |account: u32, to: Address, send: bool| flow::BurnOpts {
        public: true,
        accept_public: true,
        account,
        token: eth::ETH,
        amount: "0.1".into(),
        recipient: to,
        send,
        yes: send,
        journal: Some(journal.clone()),
        gas_drops: None,
        network: None,
    };
    let (r1, r2, r3, r4) = (
        Address::repeat_byte(0xe1),
        Address::repeat_byte(0xe2),
        Address::repeat_byte(0xe3),
        Address::repeat_byte(0xe4),
    );
    let (applied0, refused0) = tags(zone, ctx).await?;
    let k = lez::history_of(&*zone, ctx).await?.next_burn_counter(&user.bridge_keys(), ctx);

    // H-1: two burns in flight from two holders (public /0 and public /1), neither applied when the second is planned.
    let mut z = Deferred { inner: &mut *zone, queue: Vec::new(), kill_next: false, reads: Default::default() };
    let b0 = flow::burn(eth_side, user, &mut z, ctx, &bopts(0, r1, true), &mut flow::Out::new(false)).await?;
    let mut out = flow::Out::new(true);
    let b1 = flow::burn(eth_side, user, &mut z, ctx, &bopts(1, r2, true), &mut out).await?;
    println!(
        "COUNTERS in flight: public /0 -> {}, public /1 -> {} (the zone's next counter {k})",
        b0.plan.i, b1.plan.i
    );
    ensure!(b0.plan.i == k && b1.plan.i == k + 1, "two burns in flight take two counters");
    ensure!(
        out.text().contains(&format!("counter {k}")) && out.text().contains("is not on the zone yet"),
        "the second names the first"
    );
    let settled = z.flush().await;
    ensure!(settled.iter().all(|r| r.is_ok()), "both burns in flight applied: {settled:?}");

    // H1: a burn killed with its transaction at the sequencer (the tool never learnt it); then a burn with another
    // recipient takes a fresh counter, never the killed one's.
    let mut z = Deferred { inner: &mut *zone, queue: Vec::new(), kill_next: true, reads: Default::default() };
    let killed = flow::burn(eth_side, user, &mut z, ctx, &bopts(0, r3, true), &mut flow::Out::new(false)).await;
    ensure!(killed.is_err(), "the killed run fails for the tool");
    let mut out = flow::Out::new(true);
    let other = flow::burn(eth_side, user, &mut z, ctx, &bopts(0, r4, true), &mut out).await?;
    println!(
        "COUNTERS after a kill: the killed burn holds {}, the burn to another recipient takes {}",
        k + 2,
        other.plan.i
    );
    ensure!(other.plan.i == k + 3, "a fresh counter after the killed burn's");
    ensure!(out.text().contains("interrupted before it was known to be sent"), "it names the killed burn");
    let settled = z.flush().await;
    ensure!(settled.iter().all(|r| r.is_ok()), "the killed burn (late) and the other both applied: {settled:?}");
    // The killed burn asked again, now that its transaction is applied: nothing more is burnt.
    let mut out = flow::Out::new(true);
    let done = flow::burn(eth_side, user, zone, ctx, &bopts(0, r3, true), &mut out).await?;
    ensure!(
        done.already && done.sent.is_none() && done.plan.i == k + 2 && out.text().contains("not burning again"),
        "the killed burn, found applied, is not burnt again"
    );

    // The same burn as a killed one, asked again: its counter again, the same note: one applied, one refused.
    let before = zone.world.balance(publics[0].id, &ctx.cid);
    let mut z = Deferred { inner: &mut *zone, queue: Vec::new(), kill_next: true, reads: Default::default() };
    let _ = flow::burn(eth_side, user, &mut z, ctx, &bopts(0, r1, true), &mut flow::Out::new(false)).await;
    let mut out = flow::Out::new(true);
    let again = flow::burn(eth_side, user, &mut z, ctx, &bopts(0, r1, true), &mut out).await?;
    ensure!(
        again.plan.i == k + 4 && out.text().contains("takes its counter again"),
        "the same burn resumes its counter"
    );
    let settled = z.flush().await;
    ensure!(settled[0].is_ok(), "the killed run's transaction applied: {settled:?}");
    let refusal = settled[1].as_ref().err().map(|e| e.to_string()).unwrap_or_default();
    ensure!(refusal.contains("244 ALREADY_WRITTEN"), "the same note again refused by the program: {settled:?}");
    ensure!(zone.world.balance(publics[0].id, &ctx.cid) == before - tenth, "one denomination burnt, once");

    // A dry run reserves nothing.
    let kept = std::fs::read(&journal_file)?;
    flow::burn(eth_side, user, zone, ctx, &bopts(0, r2, false), &mut flow::Out::new(false)).await?;
    ensure!(std::fs::read(&journal_file)? == kept, "a dry run leaves the journal as it was");

    // A burn refused on the zone uses its counter: sent without the tool's checks (0.1 ETH more than public /1 holds).
    let keys = user.bridge_keys();
    let history = lez::history_of(&*zone, ctx).await?;
    let c = history.next_burn_counter(&keys, ctx);
    let valid = lez::plan_burn_at(
        &*zone,
        ctx,
        &keys,
        BurnFrom::Public(publics[0].id),
        [0; 20],
        tenth,
        r3.into_array(),
        &history,
        Some(c),
    )
    .await?;
    // The same note, signed at public /1, whose holding is empty: the program refuses it (254) in its block.
    let mut call = valid.call.clone();
    call.mentions[0] = Mention::Signer(ferrata_lez_bridge_wire::own_actor(program, publics[1].id));
    call.mentions[1] = Mention::Pda {
        actor: ferrata_lez_bridge_wire::holder_actor(program, &ctx.cid, publics[1].id),
        seed: ferrata_lez_bridge_wire::holder_seed(&ctx.cid, publics[1].id.value()),
    };
    let report = zone.settle(&call);
    ensure!(report.exit_code == Some(254), "public /1 holds nothing more: {:?}", report.exit_code);
    let mut out = flow::Out::new(true);
    let after_refused = flow::burn(eth_side, user, zone, ctx, &bopts(0, r2, true), &mut out).await?;
    ensure!(
        after_refused.plan.i == c + 1,
        "the refused burn's counter {c} is used: the next burn takes {}",
        after_refused.plan.i
    );

    // Recovery from the seed alone (no journal): every burn of these scenarios, one NOut each.
    let (_, refused) = tags(zone, ctx).await?;
    let new_refused: Vec<&Hash> = refused.iter().filter(|t| !refused0.contains(t)).collect();
    ensure!(new_refused.len() == 2, "two burns refused on the zone: the same note again, and the empty holder's");
    let history = lez::history_of(&*zone, ctx).await?;
    let (burn_rows, next) = scan::burns(user, ctx, &history, &HashSet::new());
    let new_rows: Vec<_> = burn_rows.iter().filter(|r| !applied0.contains(&r.owned.burn.note.r)).collect();
    let nouts: HashSet<Hash> = new_rows.iter().map(|r| r.owned.nullifier).collect();
    let counters: Vec<u64> = new_rows.iter().map(|r| r.owned.i).collect();
    println!("COUNTERS recovered from the seed: {counters:?}, {} distinct NOut, next counter {next}", nouts.len());
    ensure!(
        counters == vec![k, k + 1, k + 2, k + 3, k + 4, c + 1],
        "the burns of these scenarios, by counter: {counters:?}"
    );
    ensure!(
        nouts.len() == counters.len() && burn_rows.iter().all(|r| r.twin.is_none()),
        "one NOut per burn, no tag shared"
    );
    ensure!(next == c + 2, "the next counter after the refused one and the last burn");

    // Review N1: `ferrata recover` with the burn journal lists only the burns reserved there that the zone does not
    // show. An entry stays open until the next burn resolves it, so the journal still holds the last burn's counter
    // (c + 1), which the zone shows applied: no "burn journal:" line.
    let key = burns::key(&ferrata_bridge_core::outbound::burn_tag(&keys.view, &ctx.cid, 0));
    let open = burns::Store::new(&journal).counters(&key)?.context("this seed's counters in the journal")?.open;
    ensure!(open.iter().map(|e| e.i).collect::<Vec<_>>() == vec![c + 1], "the last burn's counter is open: {open:?}");
    let journal_lines = |out: &flow::Out| -> Vec<String> {
        out.lines.iter().filter(|l| l.starts_with("burn journal:")).cloned().collect()
    };
    let mut out = flow::Out::new(false);
    let rec = flow::recover(eth_side, user, 8, Some((&mut *zone, ctx)), true, Some(journal.as_path()), &mut out).await?;
    let listed = journal_lines(&out);
    ensure!(listed.is_empty(), "a burn the zone shows, listed as not on the zone: {listed:?}");
    ensure!(rec.burns == burn_rows.len() && rec.next_burn_counter == c + 2, "the same burns as from the seed alone");
    // A journal of this seed made with the tool's own store (another device's, say) holding counters c, c + 1 and c + 2
    // open: refused on the zone, applied, and not on the zone. Only c + 2 is listed.
    let other_home = dir.join("burn-journal-other");
    let _ = std::fs::remove_dir_all(&other_home);
    std::fs::create_dir_all(&other_home)?;
    let store = burns::Store::new(&other_home);
    let tag_of = |i: u64| ferrata_bridge_core::outbound::burn_tag(&keys.view, &ctx.cid, i);
    // The store's note hash (no zone note has it): only the tags are compared here.
    let note_of = |i: u64| sha256(&i.to_be_bytes());
    for i in [c, c + 1, c + 2] {
        let seen = burns::Seen { next: i, ..burns::Seen::default() };
        let ask = burns::Ask { from: "public /0".into(), token: eth::ETH, amount: i.into(), recipient: r4 };
        let plan = store.peek(&key, &ctx.cid, &seen, &ask, 0, None, &tag_of, &note_of)?;
        ensure!(plan.counter == burns::Counter::Fresh(i), "counter {i} reserved: {:?}", plan.counter);
        store.reserve(&key, &ctx.cid, &seen, &ask, 0, None, &tag_of, &note_of, &plan.counter)?;
    }
    let mut out = flow::Out::new(false);
    flow::recover(eth_side, user, 8, Some((&mut *zone, ctx)), true, Some(other_home.as_path()), &mut out).await?;
    let listed = journal_lines(&out);
    ensure!(
        listed.len() == 1 && listed[0].starts_with(&format!("burn journal: counter {}, ", c + 2)),
        "only the counter the zone does not show (c + 2 = {}): {listed:?}",
        c + 2
    );
    println!(
        "RECOVER (N1): the tool's journal (counter {} open, applied): no line; a journal with {c} refused, {} \
         applied, {} not on the zone: {listed:?}",
        c + 1,
        c + 1,
        c + 2
    );

    // Review L-C: that journal damaged (its next counter u64::MAX): `ferrata recover` and `ferrata release` fail at
    // once with an error naming it, where they searched 2^64 counters.
    let mut j: burns::Journal = serde_json::from_slice(&std::fs::read(store.path())?)?;
    j.wallets.get_mut(&key).context("this seed's counters in the other journal")?.next = u64::MAX;
    std::fs::write(store.path(), serde_json::to_vec(&j)?)?;
    let started = std::time::Instant::now();
    let rec = flow::recover(eth_side, user, 8, Some((&mut *zone, ctx)), true, Some(other_home.as_path()), &mut out)
        .await
        .err()
        .context("recover with a damaged journal")?;
    let ropts = flow::ReleaseOpts {
        burn: None,
        journal: Some(other_home.clone()),
        via: flow::Via::Relayer(None),
        seal: None,
        job_out: None,
        input_out: None,
        program: None,
        prove: false,
        send: false,
        yes: false,
        confirmations: 1,
    };
    let rel = flow::release_burn(eth_side, user, &mut *zone, ctx, None, &ropts, &mut flow::Out::new(false))
        .await
        .err()
        .context("release with a damaged journal")?;
    let took = started.elapsed();
    for e in [&rec, &rel] {
        let e = format!("{e:#}");
        ensure!(
            e.contains(&format!("the burn journal {} is damaged", store.path().display()))
                && e.contains(&format!("names {} as this seed's next burn counter", u64::MAX)),
            "{e}"
        );
    }
    println!("JOURNAL (L-C): a damaged journal: recover and release refused in {} ms: {rec:#}", took.as_millis());

    // M-1: an Ethereum RPC of another chain than the configuration's (a local anvil's 31337; a fork started with another
    // `--chain-id`) is refused before anything is planned or sent.
    lez::check_pairing(&*zone, ctx).await?;
    let forked = Context { chain_id: 31_337, ..ctx.clone() };
    let e = lez::check_pairing(&*zone, &forked)
        .await
        .expect_err("chain 31337 under a configuration of chain 1")
        .to_string();
    ensure!(e.contains("pairs with chain 1") && e.contains("serves chain 31337"), "{e}");
    let eth_fork =
        flow::Eth { p: eth_side.p.clone(), vault: eth_side.vault, chain_id: 31_337, from_block: 0, chunk: 10_000 };
    let mut z = Deferred { inner: &mut *zone, queue: Vec::new(), kill_next: false, reads: Default::default() };
    let e = flow::burn(&eth_fork, user, &mut z, &forked, &bopts(0, r2, true), &mut flow::Out::new(false))
        .await
        .err()
        .context("a burn under the forked RPC")?
        .to_string();
    ensure!(e.contains("serves chain 31337") && z.queue.is_empty(), "refused before sending: {e}");
    println!("PAIRING (M-1): {e}");

    // M-2: a claim's dry run, and a claim held by its delay, read no nullifier account of the zone.
    let list = eth::deposit_list(&eth_side.p, eth_side.vault, 0, 10_000).await?;
    let history = lez::history_of(&*zone, ctx).await?;
    let rows = scan::deposits(user, ctx, &list, 8, &history, &HashSet::new())?;
    let row = rows
        .iter()
        .find(|r| r.owned.kind == DestKind::Private && r.state != scan::DepositState::Claimed)
        .context("an unclaimed private deposit")?;
    let n_actor =
        ferrata_lez_bridge_wire::nullifier_actor(program, &ctx.cid, ctx.chain_id, &ctx.vault, &row.owned.nullifier);
    for (send, no_delay) in [(false, true), (false, false)] {
        let mut z = Deferred { inner: &mut *zone, queue: Vec::new(), kill_next: false, reads: Default::default() };
        let o = flow::ClaimOpts {
            leaf: Some(row.owned.deposit.leaf_index),
            plaintext: false,
            accept_plaintext: false,
            no_delay,
            relayer: None,
            gap: 8,
            send,
            yes: send,
            gas_drop: None,
        };
        let r = flow::claim(eth_side, user, &mut z, ctx, &o, &mut flow::Out::new(false)).await;
        let held = r.as_ref().err().is_some_and(|e| e.to_string().contains("held by your claim delay"));
        // Not vacuous (review L7): the dry run went as far as the claim's plan (`lez::plan_claim`), which it either
        // completed or, without --no-delay, followed with the claim delay's hold (whether the delay still holds depends
        // on the blocks' time).
        ensure!(
            r.is_ok() || (held && !no_delay),
            "the claim's dry run (no_delay {no_delay}) did not reach the plan: {:?}",
            r.as_ref().err().map(|e| format!("{e:#}"))
        );
        ensure!(
            !z.reads.borrow().contains(&n_actor),
            "a claim (send {send}, no_delay {no_delay}) read the nullifier account"
        );
        ensure!(z.queue.is_empty(), "nothing sent");
        println!(
            "QUIET (M-2): claim send {send} no_delay {no_delay}: {} actor reads, none the nullifier's{}",
            z.reads.borrow().len(),
            if held { "; held by its delay" } else { "" }
        );
    }
    Ok(())
}
