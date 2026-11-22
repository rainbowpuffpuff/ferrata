//! Probe H1 (22 November 2026): the `ferrata` tool's library at a given tree; copied into cli/tests on a checkout, run
//! and deleted (docs/reviews/lost-burn-2026-11-22/README.md). Two burns of one seed planned from the zone's history
//! while neither is applied (two holders of the seed, in flight) take one counter; both are sent; what the bridge
//! program does with the second, and what the wallet's scan shows.

use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use ferrata_cli::{
    lez::{self, Applied, BurnFrom, Context, Ledger, LezCall, Mention},
    scan,
    seed::Seed,
};
use harness::bridge as hb;
use lee::{AccountId, PrivateKey};
use lee_core::account::Actor;

/// BIP-39 test phrase (a published test value, never a wallet).
const USER: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

struct Zone {
    world: hb::World,
    applied: Vec<Applied>,
    refused: Vec<Applied>,
    keys: HashMap<AccountId, PrivateKey>,
}

impl Ledger for Zone {
    async fn applied(&self, program: AccountId) -> Result<Vec<Applied>> {
        Ok(self.applied.iter().filter(|a| a.to.program_account_id == program).cloned().collect())
    }

    async fn included(&self, program: AccountId) -> Result<lez::Included> {
        let refused = self.refused.iter().filter(|a| a.to.program_account_id == program).cloned().collect();
        Ok(lez::Included { applied: self.applied(program).await?, refused, touched: HashSet::new() })
    }

    async fn actor_state(&self, actor: Actor) -> Result<Vec<u8>> {
        Ok(self.world.chain.actor_state(actor).to_vec())
    }

    async fn submit(&mut self, call: &LezCall) -> Result<String> {
        let actors: Vec<Actor> = call.mentions.iter().map(Mention::actor).collect();
        let root = actors[call.root_index];
        let signers: Vec<PrivateKey> = call
            .mentions
            .iter()
            .filter_map(|m| match m {
                Mention::Signer(a) => self.keys.get(&a.account_id).cloned(),
                _ => None,
            })
            .collect();
        let refs: Vec<&PrivateKey> = signers.iter().collect();
        let report = self.world.submit_signed(&call.what, root, &call.message, actors, vec![], &refs);
        let a = Applied { to: root, message: borsh::to_vec(&call.message)? };
        if report.ok() {
            self.applied.push(a);
            Ok(format!("{}: applied ({} execution gas)", call.what, report.charge.gas_exec))
        } else {
            self.refused.push(a);
            Err(anyhow!("{}: refused, exit {:?}", call.what, report.exit_code))
        }
    }
}

#[tokio::test]
async fn h1_two_burns_planned_while_neither_is_applied() -> Result<()> {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run with RISC0_DEV_MODE=1");
    let mut world = hb::World::with_roots();
    let (cid, vault, program) = (world.cid_a, world.vault_a.address, world.program);
    let witness = world.vault_a.witness(&world.payer_deposit.clone(), 8);
    assert!(world.claim("setup: claim A's leaf 6 (10 ETH to the payer)", &cid, 8, &vault, witness).ok());
    let user = Seed::from_phrase(USER)?;
    let accounts = user.public_accounts(2)?;
    let d1 = hb::DENOMINATIONS[1];
    let mut keys = HashMap::new();
    for a in &accounts {
        world.chain.state.force_insert_account(a.id, lee::Account::default());
        assert!(world.transfer("setup: 1 ETH to a public account of the seed", &cid, hb::ETH, d1, a.id).ok());
        keys.insert(a.id, a.key.clone());
    }
    let mut zone = Zone { world, applied: Vec::new(), refused: Vec::new(), keys };
    let ctx = Context { program, cid, chain_id: hb::CHAIN_ID, vault };
    let bk = user.bridge_keys();
    // Both planned from one history: the first is in flight (sent, not yet in a block) when the second is planned.
    let history = lez::history(&ctx, &zone.applied(program).await?);
    let p0 =
        lez::plan_burn(&zone, &ctx, &bk, BurnFrom::Public(accounts[0].id), hb::ETH, d1, [0xe1; 20], &history).await?;
    let p1 =
        lez::plan_burn(&zone, &ctx, &bk, BurnFrom::Public(accounts[1].id), hb::ETH, d1, [0xe2; 20], &history).await?;
    println!("PROBE-H1 counters: public /0 -> {}, public /1 -> {}; one tag: {}", p0.i, p1.i, p0.note.r == p1.note.r);
    let s0 = zone.submit(&p0.call).await;
    let s1 = zone.submit(&p1.call).await;
    println!("PROBE-H1 first burn: {s0:?}");
    println!("PROBE-H1 second burn: {s1:?}");
    let history = lez::history(&ctx, &zone.applied(program).await?);
    let (rows, next) = scan::burns(&user, &ctx, &history, &HashSet::new());
    let nouts: HashSet<_> = rows.iter().map(|r| r.owned.nullifier).collect();
    println!(
        "PROBE-H1 scan: {} burns of the seed (counters {:?}), {} distinct NOut, next counter {next}; holdings after: /0 {}, /1 {}",
        rows.len(),
        rows.iter().map(|r| r.owned.i).collect::<Vec<_>>(),
        nouts.len(),
        zone.world.balance(accounts[0].id, &cid),
        zone.world.balance(accounts[1].id, &cid)
    );
    let released: HashSet<_> = rows.first().map(|r| r.owned.nullifier).into_iter().collect();
    let (after, _) = scan::burns(&user, &ctx, &history, &released);
    println!("PROBE-H1 after one release, the scan's released flags: {:?}", after.iter().map(|r| r.released).collect::<Vec<_>>());
    Ok(())
}
