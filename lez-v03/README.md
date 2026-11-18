# Ferrata's light client on LEZ v0.3.1, the public LEZ testnet's version

**EXPERIMENTAL AND UNAUDITED. Not the proposal's deliverable: the LEZ v0.4 programs of
[`../lez/`](../lez/README.md) are.** Its one-transaction finality update replaces the final exponentiation by
the residue-witness check of [`../bench/residue`](../bench/RESIDUE.md) and may compose the signing key from one
multiproof of the non-signers ([`../bench/residue-agg`](../bench/RESIDUE.md)); with `Verify::ResidueSummary` a
handoff is one transaction too, the next committee adopted by its summary instead of its 512 keys
([`../bench/residue-handoff`](../bench/RESIDUE.md), §10), which is sound only under the honest-supermajority
assumption the light client already makes. All of it is bench code, experimental and unaudited, which nothing else
in this repository deploys. No program or guest with an image id in [`../docs/image-ids.md`](../docs/image-ids.md)
changed; this program's own image id changed with the summary adoption (below, "Image id").

Every program of this repository targets LEZ v0.4 (the actor model), which has no public network. The public LEZ
testnet runs **v0.3.1** (`https://testnet.lez.logos.co`; its state machine, programs and wallet are the same in
v0.3.2). This directory is Ferrata's Ethereum sync-committee light client as a v0.3.1 program, its tests through
v0.3.1's own block application, a run on a v0.3.1 sequencer, and the script that would run it on the public
testnet. What v0.3.1 is, read from its source with file and line: [`NOTES.md`](NOTES.md).

## The program

v0.3.1 runs a program in two phases: a *plan* that sees the instruction and the accounts' ids but no state and
returns *effects* on the program's own shards of those accounts, then one *apply* per effect that sees one
shard and returns its new data; the transaction is atomic. So the plan here only routes (each account must be
the program's public PDA the instruction implies), and every check runs in an apply:

| account (PDA of the program) | shard | written by |
|---|---|---|
| `instance(id)`, `id = H(config ‖ checkpoint)` | config, the compact store, the job in progress | `Init`, `Update`, `Continue` |
| `header(id, slot)` | a finalized header's record, written once | the transaction that applies the job |
| `statement(id, s)` | an attested storage slot, written once | `Attest` |

An apply sees one shard, so v0.4's separate job actor is impossible: the job lives in the instance's shard. A
record or a statement is written by its own apply, which cannot see the store: the instruction *claims* it, the
apply at the instance (or at the record) checks the claim against what it verified and refuses otherwise, which
reverts the whole transaction. The timestamp validity window, which the plan sets without seeing the store, is
claimed the same way and must equal v0.4's (from the signature slot to the end of the weak-subjectivity period of
the stored finalized header). Refusals exit with a code, so v0.3.1 charges the cycles used, not the gas limit,
with one exception an independent review found in the reviewed image `8d3b59ff…` (the one the prepared
public-testnet run deploys): its `check_config` (identical to the v0.4 program's,
`../lez/guests/program/src/main.rs:129-140`) accepts `epochs_per_sync_committee_period = 0`; such an instance's `Init`
applies, and every update of it panics (a division by zero in `ferrata-core`'s `period_at_slot`) and is charged the
whole gas limit. Only that instance is affected, created with that config by whoever chose it, and no state changes.
It was not fixed in `8d3b59ff…` because the check would have moved that image's id. **This branch's image
`bac768a8…` fixes it**: its `check_config` (`guest/src/main.rs`) refuses a zero-epoch period at `Init` (`BAD_CONFIG`,
212), and [`harness/tests/adversarial.rs`](harness/tests/adversarial.rs) (`review_zero_period_config`) now asserts that
refusal. The deployed v0.4 program keeps the gap (its image id is fixed).

Instructions ([`wire/src/lib.rs`](wire/src/lib.rs), [`guest/src/main.rs`](guest/src/main.rs)):

- `Init { config, checkpoint, bootstrap, adopt, record, window }`: the same trusted checkpoint and `Config` as the
  v0.4 program; with the committee's 512 keys in `adopt` it is one transaction.
- `Update { instance_id, update, signers, verify, adopt, record, window }`, `verify` one of `Open` (step 1),
  `Miller` (steps 1-2), `Core` (steps 1-3, the core alone), `Residue { c, w }` (steps 1-2 and the residue check),
  `ResidueSummary { c, w }` (as `Residue`; a handoff's next committee then adopted by its summary, which is sound only
  under the light client's honest-supermajority assumption, [`../bench/RESIDUE.md`](../bench/RESIDUE.md) §10).
- `Continue { instance_id, steps, record }`: the job's next steps (Miller, FinalExp, Residue, Adopt).
- `Attest { instance_id, slot, query, statements }`: storage slots of one account under `header(id, slot)`
  (`ferrata_core::attest::attest_storage`, the block's RLP header for a Gloas record), each written to
  `statement(id, s)`. Its accounts start with `instance(id)` (since the evidence below: an apply there refuses a
  frozen instance and writes nothing).
- `Evidence { instance_id, update, signers, verify, against, window }` (EXPERIMENTAL, UNAUDITED,
  [`../bench/MISBEHAVIOUR.md`](../bench/MISBEHAVIOUR.md)): anyone may send an update that a committee the instance
  holds signed, as evidence that it contradicts a recorded finalized header at its slot (`Against::Record`, the
  record's account checked in the same transaction) or the next committee the instance adopted for its period
  (`Against::NextCommittee`); if it does, the instance is frozen with the evidence recorded, and refuses `Update`,
  `Continue`, `Attest` and `Evidence` from then on (`FROZEN`, 222), keeping its store and records. One transaction:
  6,656,148 gas at 510 of 512 signers, 8,443,193 at 342 (the residue witness, one multiproof). For an instance started
  from a finalized checkpoint (the assumption of [`../docs/trust.md`](../docs/trust.md) §2.1, which `Init` cannot
  check) an honest network never produces such evidence (`MISBEHAVIOUR.md` §3). It carries an update's window and
  must land while the instance is in the signing committee's period: an advancing `Update` landing first makes it
  refuse 218 (plan it again), and after the instance's first update of the next period it is refused 23 for good
  (`MISBEHAVIOUR.md` §5); [`harness/tests/misbehaviour.rs`](harness/tests/misbehaviour.rs),
  [`harness/tests/misbehaviour_window.rs`](harness/tests/misbehaviour_window.rs).

The verification is `ferrata-core`'s, unchanged (`steps::initialize`, `step1`, `Job::step2`, `step3`, `adopt`,
`apply`, `store_for_update`). **Two images.** The public-testnet run (prepared, sent by its own watcher) deploys the
reviewed image `8d3b59ff9b4bf6992f80e797e7e25fad1ed372c4b4e1386d96e6dd33ba8e1bb5` (597,336 ELF bytes, 6 segments),
the image the local sequencer run below used. This branch builds `bac768a8026f14b05dcf3736e5a2bbbe029c89224e5459d0fff9cfcc5466d36e`
(598,876 ELF bytes, 6 segments), which adds the one-transaction handoff (`Verify::ResidueSummary`) and the zero-period
check, and has not run on a sequencer. Both are built through the repository's `scripts/guest-rustc`, so an id does not depend on the build
path. Since the evidence (`Instruction::Evidence`, the frozen flag, `Attest`'s instance account) this branch builds
`155b731726cb78311814c3d773813ef8a103f57d5c65e35da5f0afb39181635f` (625,824 ELF bytes, 7 segments), deployed nowhere;
the earlier tests pass on it with the same results and codes ([`results/misbehaviour-tests.txt`](results/misbehaviour-tests.txt),
the regenerated tables in [`results/misbehaviour-regression/`](results/misbehaviour-regression/); the tables above
stay those of `bac768a8…`). **The wire changed with it:** `Attest` now names the instance's account first, so a client
built from this tree (which also deploys this tree's image) cannot attest under an instance of `8d3b59ff…` or
`bac768a8…`, which refuse that account list (`WRONG_ACCOUNT`, 201); the prepared public-testnet run, signed in advance
for `8d3b59ff…`, must keep the client and plan it was prepared with. Re-running the earlier suites rewrites the committed
tables of `results/` with this image's numbers: `git checkout -- results/` restores them (this image's are in
`results/misbehaviour-regression/`).

## Through v0.3.1's block application, with real data

[`harness/tests/v03.rs`](harness/tests/v03.rs): every transaction a public transaction with a fee, settled in a
block as v0.3.1's sequencer settles it (`chain_state::apply::derive_block_summary` and `apply_block`: the fee
reserve, the action metered at the declared limit, the refund, the block caps), from v0.3.1's testnet genesis; the
program deployed through v0.3.1's program loader. Data: the mainnet replay (`mainnet-p1899`) and Sepolia's
`sepolia-gloas-2026-11-10` (bootstrap of period 1378 and every period's update to 1409, through the Gloas fork),
`sepolia-inbound-2026-11-11-1` and `sepolia-vault-v5-2026-11-11` (two finality updates of period 1410, each with
the vault's proofs). Ran on the Modal build box, 18 November 2026; results in [`results/`](results/).

| transaction | execution gas | of the 10M cap | bytes |
|---|---:|---:|---:|
| **finality update in ONE transaction, residue witness** (mainnet update_1, 510 of 512) | **6,647,616** | 66.5% | 4,934 |
| the same, Sepolia (509 of 512, finalized slots 11,551,744 and 11,556,608) | 6,528,729 and 6,530,623 | 65.3% | 4,732 |
| finality update in one transaction by the core alone (mainnet update_4) | 9,103,198 | 91.0% | 3,390 |
| `Init` with the committee's 512 keys adopted, one transaction (mainnet; Sepolia) | 7,975,077; 7,885,683 | 79.8%; 78.9% | 51,196; 51,202 |
| **handoff (period update) in ONE transaction, the committee adopted by its summary, sound only under the light client's honest-supermajority assumption** (`Verify::ResidueSummary`, one multiproof; image `bac768a8…`; mainnet update_0 to update_3: 512, 510, 510, 512 signers) | **6,655,335; 6,726,614; 6,737,794; 6,687,532** | 66.6% to 67.4% | 3,901 to 4,964 |
| the same, Sepolia: the 32 handoffs of periods 1378 to 1409 (through the Gloas layout change) | 6,538,351 to **6,682,541** | at most 66.8% | 3,664 to 5,424 |
| handoff with the 512 keys adopted: `Update` (residue) + `Continue` (adopt 512 keys, apply) | 6,567,571 + 7,707,411 | 65.7% + 77.1% | 3,897 + 49,486 |
| handoff by the core alone, stepwise: Open, Miller, FinalExp, then the adoption | 2,232,993, 3,337,313, 3,670,720, 7,704,931 | | |
| handoff, steps 1-3 by the core in one transaction, then the adoption | 9,114,250 + 7,704,931 | 91.1% + 77.0% | |
| handoff with the multiproof of the non-signers and the residue witness, then the adoption | 6,566,232 + 7,704,931 | | |
| `Attest` of two slots (`depositRoot`, `nextLeafIndex`) under a Gloas record | 526,207 and 512,641 | 5.3% | 6,970; 6,991 |
| deployment: 6 `WriteSegment` + `CreateHeader` (the loader is native) | 0 | | 569,528 in all |

The v0.4 bench measured 6,629,246 for the same mainnet update in one transaction through LEZ v0.4's state machine
([`../bench/RESIDUE.md`](../bench/RESIDUE.md), EVIDENCE row 127); v0.3.1 charges 6,647,616. A finality update by the
core alone fits one transaction too at this participation; each non-signer adds about 25,000 cycles, so below about
475 signers it does not (derived), and the residue path with the multiproof keeps it under the cap down to the
policy's floor of 342 (row 128, measured on v0.4's state machine). With its 512 keys adopted the handoff does not fit one
transaction (about 14M): two. With the next committee adopted by its summary (experimental; sound only under the
light client's honest-supermajority assumption, [`../bench/RESIDUE.md`](../bench/RESIDUE.md) §10, EVIDENCE row 137) it
is one (`handoffs_in_one_transaction_each`, `results/mainnet-handoff-one-tx.tsv` and `sepolia-handoff-one-tx.tsv`): after
each, the instance's store is the native policy store's, whose adoption checks the 512 keys, and a header that advances
is recorded; refused, nothing changed: a next-committee branch with one node altered (27), the summary adoption with
the 512 keys sent beside it (84, `NoAdoption`), update_0 again (23). At 342 signers the handoff was measured on v0.4's
state machine only (8,431,403 gas at the costliest placement found, row 137): this harness has no way to write a
substituted committee into an instance's state. Sepolia: 76 transactions
applied, 32 handoffs from period 1378 to 1409, the store upgrading itself to the Gloas layout at 1379, then the two
finality updates and the attestations (slot 0 = `0xa5b5c43f…4875` and slot 1 = 1 of vault `0xd0b5…8af7` at block
12,107,997; slot 0 = `0xfaad1232…579d` and slot 1 = 2 of vault `0xfe31…9e54` at block 12,112,773).

Refused, each with its code and nothing changed: a replayed `Init` (203); a stale update (52, `NotNewer`); an update
replayed as sent (218, its window is the old store's); a participation bit cleared (40, `SignatureInvalid`); a
finalized header changed (26); a tampered residue witness (40); a wrong claimed record (219); a widened window
(218); an update while a verified job waits for its adoption (207); an attestation claiming a wrong value (220),
with a changed proof node (110) or without the block's header under a Gloas record (215); an `Init` whose
sync-committee period is zero epochs (212, this branch's image; 1,705,891 gas). A transaction sent before
its signature slot is outside its window: v0.3.1 includes it and charges the whole declared limit.

**Image id.** The summary adoption and the zero-period check changed the program: this branch's image is
`bac768a8026f14b05dcf3736e5a2bbbe029c89224e5459d0fff9cfcc5466d36e` (598,876 ELF bytes; its deployed segments are 1,540
bytes longer; this branch's first build, before the zero-period check and the aggregate field's decoding, was
`f8ec1b21…`), deployed nowhere and not run on a sequencer. The local sequencer run below used, and the prepared
public-testnet run deploys, the reviewed image before it, `8d3b59ff9b4bf6992f80e797e7e25fad1ed372c4b4e1386d96e6dd33ba8e1bb5`
(`results/local/txs.tsv`). Every figure of the table above is from this build, 18 November 2026; each is 69 to 89 gas
above the earlier build's for the same transaction (the earlier figures are in the history of `results/`); the local
run's figures are the earlier build's.

## On a LEZ v0.3.1 sequencer

`./testnet.sh local` on the Modal build box, 18 November 2026, 11:30-11:41 UTC (refreshed after the review, with the
state-driven script): LEZ v0.3.1's own `sequencer_service` (its standalone mode: a mock Bedrock, no Logos Blockchain
node; one sequencer, 10 s blocks), the payer funded at genesis, every transaction signed and sent with v0.3.1's wallet
(its key storage, signing and sequencer client). Fresh Sepolia data first: the finalized checkpoint at slot 11,605,568
(period 1416, block root `0xa3dbfb9d…a641`, Lodestar's bootstrap checked against publicnode's beacon header), the next
two finality updates (finalized slots 11,605,600 and 11,605,632) and `eth_getProof` of the vault
`0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c`, slots 0 and 1, at Sepolia block 12,161,152, the block the second update's
finalized (Gloas) header binds. Then the run read the chain (nothing there yet), signed all eleven transactions with
explicit nonces and declared limits of the simulated gas plus 3% and 30,000, and queued them at once (11:40:12Z). The
sequencer made exactly the three blocks the simulation predicted, deferring each update to the next block by its
declared gas (`results/local/sequencer.log.txt`):

| block | transactions | execution gas (declared) | bytes |
|---|---|---|---:|
| 4 | 6 `WriteSegment` + `CreateHeader` (no execution gas); **`Init`**, the committee's 512 keys adopted | 7,884,541 (8,160,000) | 567,988 + 50,887 |
| 5 | **finality update 1, one transaction** (residue witness) | 7,046,507 (7,300,000) | 11,260 |
| 6 | **finality update 2, one transaction**; **`Attest`** of the vault's slots 0 and 1 | 7,046,631 (7,300,000); 501,007 (550,000) | 11,260; 6,820 |

The execution gas is the simulation's (v0.3.1's RPC reports no gas and no status); the payer's balance, recorded before
queueing and after the last block (`results/local/run-log.tsv`: 1,000,000,000,000,000 then 999,999,792,831,749), fell
by 207,168,251 native units, exactly the sum of the simulated fees in `signed.tsv` at the base fees the simulation
predicted (8, 9, 10). 30 s from queueing to the third block. The goal then held in state after one round (`goal`:
`DONE`). `./testnet.sh verify` ran 29 checks, all matching: 7 compare state (the program's image id, the instance, the
records of slots 11,605,568, 11,605,600 and 11,605,632, the two statements: slot 0 (`depositRoot`) = `0xa67ef5a1…7469`,
slot 1 (`nextLeafIndex`) = 2, at block 12,161,152) with `record.json`, which `record` wrote from the same state; 11
compare the block each transaction is in, which v0.3.1 cannot tell from a refusal (a refused transaction is in a block
too); 11 compare each transaction's effect in state, in full, with what the run's data implies (each segment's chunk,
the image id, the bootstrap's record and the active instance, each update's record, the statements). The fresh updates
had fewer signers than the fixtures (their signer witnesses make them 11,260 bytes), hence 7.05M instead of 6.5M.
Record: [`results/local/`](results/local/) (`signed/` and `signed.tsv`: every signed transaction; `txs.tsv`: blocks and
verdicts; `record.json`; `run-log.tsv`; the data and `requests.tsv`; `batch.json`; the sequencer's configuration and
log).

## The public testnet: prepared, not sent

[`testnet.sh`](testnet.sh) `plan` signs the whole run for a fresh payer (keys under `spike/tmp/testnet-v03/`, never in
the repository) and prints every transaction with its block, size, declared and simulated gas and fee, and the
funds needed and how the LEZ docs say to get them; `run` fetches a fresh checkpoint, two finality updates and the
vault's proofs, reads the chain, signs what it is missing with explicit nonces and declared limits of the simulated
gas plus 3% and 30,000, queues it at once, and repeats by rounds until the goal holds in state (next section);
`resend` sends the same signed bytes again (after a rewind); `verify` compares state with the committed record and
with what the data implies, and the transactions' blocks with the explorer (PENDING, exit 3, while a block is Pending
on Bedrock); it fails if `status` fails or fewer checks than signed transactions ran. Every path that can send
(`run`, `resend`, `local`) refuses any sequencer but a loopback one unless `FERRATA_TESTNET_SEND=yes`, which the
script never sets itself; [`tests/sendguard.sh`](tests/sendguard.sh) and [`tests/verifystatus.sh`](tests/verifystatus.sh)
check both with stubs (no network, no keys).
The same sequence is written as testnet-ops's `ferrata_ops batch` file (`batch.json`). `./testnet.sh plan` on 18
November ([`results/public-plan.txt`](results/public-plan.txt)): the eleven transactions signed for a fresh payer
(`yEnfeJdX68Qrihb5yz3cxcidYZK13aupgYR1T2R3C3R`, program account `J4G2Leu5WUVu66FF23nKqQM7apUajyV5Av7mmGeKrFj1`) on the
local run's data and simulated: the same three blocks, 211,470,107 native units of fees at the simulated base fees
(8, 9, 10), the largest reserve 65,687,096 at base fee 8; the payer needs at least 514,266,448. Read-only calls only so far:
the public testnet answered `getLastBlockId` 83005 and its fee state (base fees 8 and 8, caps 10,000,000 and
1,000,000) on 18 November; on that day it made a block every few hours and rewound one. **Nothing has been sent to it.**

## What a third party can do to the public instance, and what the run does

Everything here is liveness: no sequence of transactions makes the program accept what the core refuses (the review
found no unsound path; `harness/tests/adversarial.rs`). A run is driven by state (`harness_v03::Instance::
plan_from_state`, `harness_v03::deployment`): each round reads the deployment accounts, the instance and the records,
signs only what the state is missing, and queues nothing new while earlier signed transactions are not in a block (it
sends their bytes again). Each case has a test through v0.3.1's block application
([`harness/tests/third_party.rs`](harness/tests/third_party.rs)):

| what a third party does | what a pre-signed run would get | what the run does |
|---|---|---|
| writes one of the run's segment accounts first (`WriteSegment` needs no signature) | a program with another image id at the header; every call fails | finds the account taken (`Deployment::Taken`) and deploys at fresh accounts (a fresh header key kept beside the wallet, never inside it), saying so in `state.json` (`replaced`) and `notes.txt` |
| creates the run's instance first without adopting its committee (216,479 gas) | `Init` refused (203), every update refused (206) | sends `Continue` with the 512 keys instead of `Init` (7,735,452 gas), then the updates |
| applies a newer update than the run's | the run's updates refused as stale (52); its `Attest` without its record (213) | skips what the chain already holds, rebuilds its light client from the chain's store, applies the next newer update, attests under the record the instance holds (fetching that block's proofs if needed) |
| verifies a handoff and leaves it without its adoption | every update refused (207) | pays for the adoption (`Continue`, 7,707,328 gas), then updates |

No config field takes the instance id out of anyone else's reach for free: the id is `H(config ‖ checkpoint)` and
every field of `Config` is a chain parameter or a security parameter (the adoption parts, the weak-subjectivity
period), so none was changed; anyone can create the same instance, and the run adopts it instead.

## What it does not show

- No public run yet: the owner decides it (see the main session's runbook); until then no public LEZ network runs any
  of Ferrata.
- No bridge on v0.3.1, no private path (v0.3.1's privacy circuit is not used), no successors; the LEZ v0.4 programs
  remain the proposal's deliverable.
- The one-transaction finality update rests on the experimental residue check (and, optionally, the multiproof):
  not audited; the core-only path (`Verify::Core`, stepwise `Continue`) is there for the comparison.
- The local sequencer ran with a mock Bedrock (v0.3.1's standalone mode): no Logos Blockchain finality.
- The weak-subjectivity period is the test value of the v0.4 work (3,532 epochs), not computed from Sepolia's
  balance at the checkpoint.

## Re-check

```sh
cd lez-v03
scripts/build.sh --lez                       # program, harness, client; LEZ v0.3.1's sequencer_service and wallet
cargo test --release --locked -p harness-v03 --test v03 --test third_party --test adversarial -- --nocapture --test-threads 4
cargo test --release --locked -p harness-v03 --test misbehaviour --test misbehaviour_window -- --nocapture --test-threads 2   # evidence and the freeze (EXPERIMENTAL)
tests/sendguard.sh && tests/verifystatus.sh  # the script's send guard and verify, against stubs (no network, no keys)
./testnet.sh local                           # a v0.3.1 sequencer here, the whole queued sequence, verify
./testnet.sh plan                            # the public run, signed and simulated; sends nothing
FERRATA_SEQUENCER=http://127.0.0.1:3040 ./testnet.sh verify   # against a running sequencer holding the local run
```

Files: [`NOTES.md`](NOTES.md) (v0.3.1 from source), `wire/` (instructions, effects, state, addresses), `guest/` (the
program), `methods/`, `harness/` (the chain and the relayer's side), `client/` (`ferrata-v03`, on v0.3.1's wallet),
`scripts/fetch.py` (public beacon and execution APIs; generic User-Agent; every request in `requests.tsv`),
[`testnet.sh`](testnet.sh), [`results/`](results/).
