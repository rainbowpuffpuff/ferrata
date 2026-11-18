# LEZ v0.4.0-rc3: what it changes for Ferrata, what ran on it, what moving to it would take

LEZ tagged `v0.4.0-rc3` (commit `bafd07be0701398f9bc435f73ab05e9a7936c301`) on 11 November 2026, eight
commits after rc2: one clock account that an actor can read (LEZ #988), every `logos-blockchain`
dependency moved from `d480efc6` to the 0.3.5-rc.1 release (#1095), and a CI change (#1093). `lee/`
(`lee_core`, the state machine, the privacy circuit) and the wallet are unchanged since rc2, so rc2's
findings stand ([`docs/design/lez-rc2-changes.md`](../../../docs/design/lez-rc2-changes.md)). Ferrata's
programs target v0.4.0-rc1 and were checked on rc2 (EVIDENCE row 22, [`../../RESULTS.md`](../../RESULTS.md)
§10); this directory does the same for rc3. Everything below ran on 13–14 November 2026 (`environment.md`: 14 Nov 09:42
UTC) on a Modal build box (16 physical cores, under gVisor), from this tree.

## What ran on rc3

| what | on rc3 | files |
|---|---|---|
| the programs, rebuilt | **every image id changes** (7 of 7: the attestation program `0xefd9aef7…` for the final `0x816d1fef…`, the bridge `0x8f8efd14…` for `0xccde6171…`, the consumer `0x9dc21f01…`, LEZ's `scripted` `0x3af31a8d…`, …), though no source of ours changed. Two causes, both measured. The **tree's name**: the LEZ sources enter a guest under `/upstream/lez-<tag>/`, and `scripts/guest-rustc.py` hashes that path into every crate's `-C metadata`, so every symbol name changes and rustc orders the functions anew: 50,248 to 246,463 bytes of each user ELF differ (sizes equal). With rc3's tree compiled **under rc1's name** (`FERRATA_GUEST_REMAP_EXTRA`, an experiment's switch), 707 to 797 bytes differ: the symbol string table, which is not loaded, and in the loaded image **1 byte** (4 for `scripted`): the line number of a panic location in `lee_core`'s `src/program/mod.rs` (`vec![0; len]`, line 729 on rc1, 755 on rc3, moved by rc2's `LoaderEntry`, LEZ #943). The code (`.text`) is byte for byte the final programs'. The control: the same box rebuilt all 7 final ids from rc1 (from a copy of the tree at another path) | `image-ids.md`, `image-ids-rc3-as-rc1.md`, `elf-sections-rc3-as-rc1.txt` |
| the harness, dev mode | **56 of 56 tests pass**; in the 25 result files with tables, **1,119 of 1,120 rows equal the rc1 run's** (the same source on rc1, on the same box: `rc1-rerun/`, 56 of 56 tests): handler cycles, execution and storage gas, base fees, results. The other row is an identifier derived from the bridge's image id (`bridge-transfer-private.md`), and `images.md` lists the rebuilt ids. `bridge-controls.md`, `bridge-time.md` and `bridge-vault-storage.md` are written by their tests into `../` whatever the tag (a harness defect, not fixed here): their rc3 versions are copied here and differ from rc1's in the image id line only | `harness-tests-dev-mode.log`, `compare-v0.4.0-rc1.md`, `rc1-rerun/`, the tables (`*.md`) |
| the provers' dry runs | the private update 9,049,362 user cycles, a 4,107-byte fake receipt in an 8,101-byte transaction; the same on rc1 on the same box (`rc1-rerun/`) | `prove-*-dry-run.log` |
| **the final programs on rc3's sequencer** | one session of 51 minutes (`run.sh run`, the phases of row 22's but the real proof): the final binaries (`816d1fef…`, `8cb0e3f5…`, checked byte for byte) deployed through rc3's loader, **182 blocks, all final on Bedrock; 99 sent: 76 applied, 19 refused by the program with its exit code, 2 reverted otherwise, 2 dropped** (the stored GPU-proven transaction and the fake receipt, as on rc1 and rc2); of the 65 with a harness row (`stage1-*.md` here), the same execution gas in 64, the other the validity-window case (as on rc1 and rc2). Deployment pays execution gas as on rc2: the program 8 transactions, 1,091,406 execution gas, 599,500 storage gas, 13,527,248 in base fees; `scripted` 373,430, 238,216, 4,893,168. Base fee 8 to 9. `verify-run.sh`: every block hash holds, the 182 blocks replay through rc3's `chain_state`, 97 rows equal LEZ's settlement. New on 0.3.5-rc.1: 61 blocks were final 14–24 s after publication (rc2: 14–25 s), but **120 blocks, in two runs of 60, only after 1,214–1,216 s** (about 300 slots of 4 s; one public transaction waited 1,219.6 s, the other 76 settled in 19.1–36.8 s), not traced: the sequencer logged no rewind and one skipped turn; the storage gas price rose 1 → 7 | [`../../live/results/v0.4.0-rc3/`](../../live/results/v0.4.0-rc3/tables.md), [`../../live/README.md`](../../live/README.md) "LEZ v0.4.0-rc3" |
| checkpoint-core and Bedrock 0.3.5-rc.1 | the deployed reading still refuses the 0.3.5-rc.1 framing (3 of 3 tests that record it pass); a **second build** of the two checkpoint guests reads it (`checkpoint_0_3_5`, `checkpoint_b_0_3_5`; the default ids unmoved, 14 of 14), and the return leg ran over an rc3 zone (one sequencer, one validator): a rule (B) checkpoint over its real blocks proven for real (18,971,326 cycles, 1,488.1 s on the CPU), wrapped to Groth16 and accepted by vault v5 on a local anvil, both burns released (18 Nov; section below) | `checkpoint-core-bedrock-0.3.5.log`, [`return-leg/`](return-leg/README.md) |
| **real proofs (M0 check 4), 16 Nov** | the private update, a private attestation, a private claim and a private burn, each proven for real by rc3's wallet on the box's CPU (887.3, 279.9, 264.3, 302.2 s; transactions 251,630–255,031 B) and applied by rc3's sequencer (no `RISC0_DEV_MODE`; a fake receipt dropped) in blocks final on the 0.3.5-rc.1 node 14 s after publication; the final programs (rc1 builds), not the rebuilds above. A first attempt met a 20-minute stall of the node, after which the sequencer rewound its head to the last final block | [`real-proofs/`](real-proofs/README.md) |


## The clock (LEZ #988) and the weak-subjectivity bound

**What rc3 adds.** One clock account instead of rc1's and rc2's three (ticked every 1, 10 and 50
blocks), and a read in place of the assertion: on rc1 and rc2 a program could only send
`AssertTimestamp { at_least, at_most }`, which fails outside the range; on rc3 an actor sends
`ReadTimestamp { granularity }` (or `ReadBlockId`) to the clock's actor and receives, as a
synchronous message back, `Reply::Timestamp(t − t mod granularity)`
([`clock/core/src/lib.rs`](https://github.com/logos-blockchain/logos-execution-zone/blob/v0.4.0-rc3/lez/programs/clock/core/src/lib.rs),
[`clock/src/main.rs`](https://github.com/logos-blockchain/logos-execution-zone/blob/v0.4.0-rc3/lez/programs/clock/src/main.rs)).
The sequencer ticks the clock as the last transaction of every block, so a transaction in block N
reads block N−1's timestamp. Only an actor can read it: a transaction sent to the clock directly
fails. In a private transaction the read crosses into the public part, which LEZ executes again
when the transaction is included, so the transaction "stays valid only while the rounded value is
the one it read". The timestamp is the one the block's producer sets, as before
([`docs/design/lez-v0.4-findings.md`](../../../docs/design/lez-v0.4-findings.md) §7).

**What the bound does now.** The program makes every transaction that extends an instance valid
only inside a timestamp validity window: `Open` and `VerifyAll` from the start of the update's
signature slot to the first slot after the weak-subjectivity period of the instance's finalized
header (`update_window`, [`lez/guests/program/src/main.rs`](../../guests/program/src/main.rs));
`Init` and `Succeed` up to the end of their checkpoint's period (`period_window`). LEZ's state
machine checks the window against the timestamp of the block that includes the transaction, for
public and private transactions alike (the window of a private transaction is part of its proof's
output; `lee/state_machine/src/validated_state_diff/mod.rs`, "Verify validity window"). Nothing of
this changed in rc3: `lee/` is untouched since rc2.

**Would a clock read be better for this bound? No.**

1. *The same clock, a block later and coarser.* Both are the producer's timestamp. The window is
   checked against the including block's own timestamp; a read returns the previous block's,
   rounded down.
2. *A read bounds the present from below, not from above.* The block's time is at least the
   previous block's, which is at least the value read; nothing bounds it from above. A read can
   enforce "not before" (a timelock) but not "not after", and the weak-subjectivity bound is a "not
   after": with a read, an update would pass in the first block after a pause that crossed the end
   of the period, however long the pause. Making it sound needs a margin of one granularity step
   plus an assumed longest gap between blocks; the window needs neither.
3. *A private update would expire with the granularity.* With the window, a proven update stays
   valid until its period ends (at least 256 epochs, 27.3 h; about 3,500 epochs, 15.6 days, on
   mainnet: `crates/ferrata-core/src/ws.rs`). With a read it is valid only until the clock passes
   the next multiple of the granularity: a proof of 49.5 s on an L4 GPU or 895.6 s on 32 CPU threads
   (`../../RESULTS.md` §3 and §9) races that boundary, and a coarse granularity makes the bound late
   by as much.
4. *It is a program change for nothing in return.* The read is a round trip (the clock's handler,
   then the program again with the reply; the bridge's existing read of a record costs 12,268 +
   33,846 cycles, `../bridge-time.md`, about 2% of an `Open`'s 2,170,217, `../stage1-refusals.md`);
   the program must keep the pending update across the reply or do the check there; and its image id
   changes: a new deployment, and every consumer re-pins the program.
5. *The one thing a read would improve.* A late transaction would be refused with the program's
   own code and pay the cycles it used; a window miss reverts as `OutOfValidityWindow` and pays its
   declared gas limit (`../stage1-refusals.md` row 17: 10,000,000). LEZ's wallet simulates every
   public send since rc2 and does not send one that would revert (LEZ #1035), so an honest client
   does not pay that.

So the bound stays a validity window on rc3, and moving to rc3 changes nothing in it.

**Where a read would serve Ferrata.** Only where a program must hold or compare a LEZ time value.
None of Ferrata's programs does: the bridge's caps, freezes and timelocks run on attested Ethereum
time ([`../bridge-time.md`](../bridge-time.md): a block stamped a year ahead moves no bridge clock),
which a block producer cannot move; a LEZ clock read could at most be a fallback for liveness while
attestations stall, and it would let a producer end a freeze early, so it is not proposed. What does
change is a sentence: "LEZ programs cannot read a clock" (proposal §B4 "Caps",
[`docs/traceability.md`](../../../docs/traceability.md) F.11,
[`docs/guides/deployer-guide.md`](../../../docs/guides/deployer-guide.md) "Caps",
[`docs/design/lez-v0.4-findings.md`](../../../docs/design/lez-v0.4-findings.md) §7) is true of rc1 and
rc2 and false of rc3; the reason Ferrata keeps attested time is that one producer sets the block's
timestamp, which rc3 does not change.


## Logos Blockchain 0.3.5-rc.1 (LEZ #1095) and the checkpoint guests

rc3 moves every `logos-blockchain` dependency from `d480efc6` to the 0.3.5-rc.1 tag (`f6d4267f`),
"the release the devnet runs. LEZ built against d480efc6b could not talk to it: the node rejects
transactions without a `fork_digest`". A Mantle transaction now starts with a 32-byte fork digest,
and its hash, which every operation's proof signs, covers it:
`BLAKE2b-256("MANTLE_TXHASH_V1" ‖ fork_digest ‖ ops)` (logos-blockchain #3954,
`core/src/mantle/transactions/tx_list/op_refs.rs`); a channel configuration with an unreachable threshold no longer
decodes (logos-blockchain #3781); an inscription's size is bounded per era (`/mantle/limits`; 7/8 of 2 MiB, as before).
The deployed checkpoint guests (`checkpoint`, `checkpoint_b` of [`docs/image-ids.md`](../../../docs/image-ids.md),
which the vault v5 on Sepolia pins, `e2e/runs/crown-live-3/`) read the `d480efc6` layout, the operations alone
([`checkpoint-core/src/mantle.rs`](../../../checkpoint/crates/checkpoint-core/src/mantle.rs)), and refuse the new one:
measured on upstream's own codec fixtures ([`checkpoint/crates/checkpoint-core/tests/bedrock_0_3_5.rs`](../../../checkpoint/crates/checkpoint-core/tests/bedrock_0_3_5.rs),
3 of 3 still pass), and on this zone's real blocks (the deployed `checkpoint_b` refuses the input below,
`UnsupportedOpcode`). Their ids cannot change under the Sepolia vault, so the 0.3.5-rc.1 reading is a **second
build** of the two guests, beside the first ([`return-leg/`](return-leg/README.md), 18 November):

- checkpoint-core's cargo feature `bedrock-0-3-5` (off by default, new files only,
  [`src/bedrock_0_3_5/`](../../../checkpoint/crates/checkpoint-core/src/bedrock_0_3_5/mod.rs)) reads each transaction
  as `fork_digest ‖ ops`, hashes the digest with the operations, requires it to be the deployment's, and refuses an
  unreachable configuration threshold; the guests `checkpoint_0_3_5` `0x259cd89e…` and `checkpoint_b_0_3_5`
  `0xafbf4023…` are built from it ([`docs/image-ids.md`](../../../docs/image-ids.md), their own heading; not deployed on
  Sepolia). The default build's 14 ids rebuilt unchanged at that branch (14 of 14, and 6 of 6 against the Sepolia
  vault). The deployment pins the digest through `lezConfigId`: for such a zone `bedrockGenesisId` is its fork digest,
  so vault v5's code, its journal and the final bridge program serve it unchanged; one digest is pinned, so a Bedrock
  era change stops the checkpoints (fails closed) and the retirement plan applies.
- On a zone of LEZ v0.4.0-rc3's own sequencer (one) and its 0.3.5-rc.1 node (one validator, dev-mode leader proofs)
  with the final programs: crown-driver's sessions (a public and a private claim, a public and a private burn, the
  private ones proven for real), an attestor beside the sequencer, and the rule (B) checkpoint over the zone's real
  blocks 25..=125 built from the node's answers alone: every inscription checked with its digest; in the executor
  18,971,326 user cycles, the journal equal to the native run, the burn root equal to the bridge core's recomputation from the bridge program's two burns.
  Proven for real on the CPU (1,488.1 s), wrapped to Groth16 and accepted by FerrataVault v5 in mode (B) on a local
  anvil behind RISC Zero's real verifier, pinning the second build's `checkpoint_b_0_3_5`: both burns released with real
  release proofs; the same seal under the deployed `checkpoint_b`'s id is refused, and so is a replayed release.

The rule (A) guest of the second build is built and tested, not run on this zone; a rotation on 0.3.5-rc.1 is tested
on fixtures only; Logos's devnet itself was not used.

## What moving to rc3 would take, and in which milestone

| part | what it takes | milestone of the proposal (draft 14) |
|---|---|---|
| the LEZ programs (attestation program, consumers, bridge, holdings, LEZ's `scripted`) | no source change. A rebuild against rc3 gives new image ids (7 of 7, above; the code is unchanged but for one panic location's line number), so the final programs stay the rc1 builds of [`docs/image-ids.md`](../../../docs/image-ids.md), and they run on rc3's sequencer as they are (the session above). Rebuilding is for a release that redeploys anyway, with `scripts/guest-rustc.py` naming the LEZ tree without its tag, so that a later tag that leaves `lee_core` alone leaves the ids alone too | A1, "the immutable deployment and its check on the newest LEZ tag" (this directory is that check for rc3, as a prototype); A4/B4 for a redeployment |
| the harness, the lockfiles, the live driver and scripts | done here: `lez/scripts/lez-select.sh v0.4.0-rc3` (rc3's commit, the feature `lez-rc3`, which implies `lez-rc2`, `lockfiles/v0.4.0-rc3.patch`: LEZ's own path crates as on rc2, `cross_zone_emission_core`, and for the driver the 42 `logos-blockchain` crates at `f6d4267f`; no other crate changes version, the guests' lockfiles do not change), the harness's clock guard on rc3's one clock account (`harness/src/settle.rs`, one function), `lez/advance` added to the script's list (it had been left out since the rc2 move, so no tag but rc1 could be selected), `live/env.sh` and `scripts/fetch-upstream.sh` | A1 (done as a prototype) |
| the crates that still link LEZ rc1 outside `lez-select.sh` (`cli`, `cli/wallet`, `fuzz/programs`, `idl/spel-check`, `lez/crown/driver`, `lez/lab/driver`, `lez/updater/sequencer`, the attestation and updater modules' Rust libraries, `checkpoint/fidelity`) | the same switch of path dependencies; those that link `logos-blockchain` move to 0.3.5-rc.1 with it (`ChannelConfigOp`'s fields are private there: built with `ChannelConfigOp::new`; `Ops` carry a fork digest); none uses the clock accounts | A3 (modules, CLI, updater), B3 (bridge module, CLI, relayer) |
| the checkpoint guests (`checkpoint`, `checkpoint_b`), `checkpoint-core`, the attestor, `realblocks` | done as a prototype (18 November, [`return-leg/`](return-leg/README.md)): a second build of the two guests (checkpoint-core's feature `bedrock-0-3-5`) reads, hashes and checks the 32-byte fork digest and refuses an unreachable threshold; the attestor and `realblocks-crown` read a 0.3.5-rc.1 node's answers; crown-driver pins the digest through `lezConfigId`. What remains: a vault deployed with the new ids where the zone runs (vault v5's immutables on Sepolia cannot follow: the retirement plan the proposal commits to for "any change the deployed checkpoint program cannot follow"), Logos's devnet itself, a zone of several sequencers on 0.3.5-rc.1, rule (A) live, and a Bedrock era change (one digest is pinned) | B1, "on testnet 0.4, Logos's devnet or one I run … the newest images proven end to end" (Logos's devnet runs 0.3.5-rc.1) |
| the zones that run today on rc1 and node `d480efc6` (the crown sessions, the lab, the updater module live) | rebuilt from rc3: LEZ's sequencer and its node, as the session above did | A4, B4 (testnet 0.4) |
| real proofs on the newest tag | done on rc3 (16 November): the private update, a private attestation, a private claim and a private burn proven for real by rc3's wallet and applied by rc3's sequencer in final blocks, on the final programs ([`real-proofs/`](real-proofs/README.md)); none on rc2 (its attempt stopped at its cap) | M0, check 4: "Real proofs on the newest LEZ tag … settled by its sequencer" (GO on rc3) |
| the weak-subjectivity bound and the bridge's clock | nothing: the validity window stays (section above); the sentence "LEZ programs cannot read a clock" is now dated to rc1 and rc2 in this tree's documents, and in the proposal's §B4 "Caps" it is the owner's to change | — |

## Files and re-checks

- `environment.md` (the box), `image-ids.md` (rebuilt ids against the final ones,
  `lez/scripts/compare-guest-builds.py`), `image-ids-rc3-as-rc1.md` and `elf-sections-rc3-as-rc1.txt`
  (`lez/scripts/elf-diff-sections.py`), `harness-tests-dev-mode.log`, the tables of `cargo run -p harness --bin tables`
  (the same file names as `../`), `compare-v0.4.0-rc1.md` (`lez/scripts/compare-tables.py`, against `rc1-rerun/`: the
  harness on rc1, same source, same box), `prove-*-dry-run.log`, `checkpoint-core-bedrock-0.3.5.log`
  (`cargo test -p checkpoint-core --test bedrock_0_3_5` in `checkpoint/`).
- The box job that made them: `spike/tmp/lez-rc3/boxC.sh` beside the repository (not in it). The same by hand:

```sh
scripts/fetch-upstream.sh                     # also upstream/lez-v0.4.0-rc3 (commit bafd07b)
lez/scripts/lez-select.sh v0.4.0-rc3          # manifests, feature lez-rc3, lockfiles/v0.4.0-rc3.patch
cd lez && export RISC0_HOME=$PWD/../../risc0-home-lez CARGO_TARGET_DIR=$PWD/target/lez-v0.4.0-rc3 RISC0_BUILD_LOCKED=1
RISC0_DEV_MODE=1 cargo test -p harness --locked -- --test-threads 8 > results/v0.4.0-rc3/harness-tests-dev-mode.log
cargo run -p harness --locked --bin tables    # results/v0.4.0-rc3/*.md
# the same under rc1's name (the image experiment); unset, the wrapper maps the tree as always
FERRATA_GUEST_REMAP_EXTRA="$(cd ../../upstream/lez-v0.4.0-rc3 && pwd -P)=/upstream/lez-v0.4.0-rc1" \
  CARGO_TARGET_DIR=$PWD/target/rc3-as-rc1 cargo build -p methods --locked
scripts/lez-select.sh v0.4.0-rc1              # back to the committed state
```
