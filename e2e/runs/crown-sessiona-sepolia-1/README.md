# crown-sessiona-sepolia-1: session A of the crown run on the fresh Sepolia checkpoint (fork run, 12 Nov 2026)

Why: `crown-live-2` stopped at its first stage on 12 Nov 2026 (nothing sent to Sepolia): on LEZ, `crown sepolia Init`
reverted with "Execution outside of the validity window". The final attestation program bounds every transaction that
creates or extends an instance by the weak-subjectivity period of the instance's finalized header (3,532 epochs,
`sepolia_config`), by the sequencer's clock, and the run's checkpoint was the period-1378 bootstrap of 5 Oct 2026,
outside that period since 21 Oct 03:26 UTC. The fix is a fresh checkpoint:
[`crates/ferrata-spectests/fixtures/sepolia-2026-11-12`](../../../crates/ferrata-spectests/fixtures/sepolia-2026-11-12/SOURCE.md)
(Gloas bootstrap at slot 11,562,112, period 1411, root `0x3b901f5e…ccf7c`; the period-1411 update), read by
`crown-driver` from the bundle's fixture.

What this run is: `e2e/crown.sh start --fork --lez-chain sepolia --halt-after identity crown-sessiona-sepolia-1` on the
our Modal workspace, commit 5975127: the live run's own LEZ session (`lez/crown/session.sh a` through
`e2e/crown/cloud`, a fresh LEZ v0.4.0-rc1 devnet on a 16-core container, the sequencer on its real clock) on the
**Sepolia chain**, with the LEZ bundle `setup` uploaded at 10:56Z (138,872,352 bytes, sha256 `0013bab125141c9d…`:
`lez/crown/build.sh repack` of `crown-live-2`'s bundle `4dd813e2…` with crown-driver `44024e7e…` and the new fixture;
the programs byte for byte). Fork mode: the identity stage read the deployer's nonce from an anvil fork of Sepolia
(block 12,118,394); no transaction was sent to Sepolia. It halted after identity, as `--lez-chain sepolia` requires.

| | |
|---|---|
| session A | 335 s (`stages.done`); 23 transactions sent, **23 applied**, 0 reverted; blocks 1..=24, all finalized on Bedrock (`lez/session-a/export/summary.txt`) |
| programs (computed by the sequencer's loader) | attestation `816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df`, bridge `ccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd` = [`docs/image-ids.md`](../../../docs/image-ids.md) (binary sha256 `ddfde69b…`, `11d83248…` as there) |
| `crown sepolia Init` | **applied**, block 17 (sequencer timestamp 1,794,481,349,799 ms = 2026-11-12T11:02:29Z; the window ends 2026-11-28T03:13:36Z), then adopt 1/2, 2/2 applied (blocks 18, 19) |
| period 1411 update | Open, Miller, adopt 1/2, adopt 2/2, Finish: **applied** (blocks 20..=24); advanced false, adopted true: the update finalizes the period's first checkpoint (slot 11,558,912, older than the bootstrap) and gives the fresh store the next committee, as the spec's `update_has_next_sync_committee` allows |
| instance | `0xa5fb2591db162d5a1a8dd56c83c18323a84cb80764fdef585ac0afbb1b9af2b5`, active, 2 jobs applied, finalized slot 11,562,112, root `0x3b901f5e…ccf7c` (`export/instances.tsv`) |
| T0 | block 24 `0x4b43b1c2…63d6` (`lez/session-a/session-a.json`, chain `sepolia`) |
| identity (fork) | deployer nonce 23, vault `0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c` (CREATE n+2), configId `0x10837afe…5553` (`identity.json`; its label says "stand-in beacon committee", written before drive.py labelled such runs by their chain) |

From `lez/session-a/logs/driver.log`:

```
[crown-a] crown sepolia: attestation program A5E62aMHKFUPwtuMZRUTSPsGwkpzeGBWmbVzD8F5tKcX, bridge AfrE4T3U17eZYUH3KMGDgeE6A4jnpDGum151T3VtddCp, instance 0xa5fb2591db162d5a1a8dd56c83c18323a84cb80764fdef585ac0afbb1b9af2b5
[crown-a] crown sepolia Init Init                                    applied        gas    114811 block   17 + 10.1s
[crown-a] crown sepolia Init adopt 1/2                               applied        gas   3397076 block   18 + 10.0s
[crown-a] crown sepolia Init adopt 2/2                               applied        gas   3480132 block   19 + 10.4s
[crown-a] period 1411 update Open (step 1)                           applied        gas   2096392 block   20 +  9.6s
[crown-a] period 1411 update Miller (step 2)                         applied        gas   3310470 block   21 + 10.0s
[crown-a] period 1411 update adopt 1/2                               applied        gas   3408111 block   22 +  9.8s
[crown-a] period 1411 update adopt 2/2                               applied        gas   3415303 block   23 + 10.0s
[crown-a] period 1411 update Finish (step 3)                         applied        gas   3696140 block   24 + 10.1s
[crown-a] period 1411 update applied: advanced false, adopted true, rotated false
```

For comparison, `crown-live-2` (same programs, the period-1378 bootstrap, instance `0x88df8428…c358`): `crown sepolia
Init Init reverted ... block 17 ... Execution outside of the validity window`.
