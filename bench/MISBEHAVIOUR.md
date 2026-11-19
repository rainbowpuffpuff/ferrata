# Freezing a light-client instance on proof that a sync committee signed conflicting finality

**Experimental and unaudited. The deployed program does not have this.** Written on 18 Nov 2026. The check lives in
a bench crate (`bench/misbehaviour`) and in the LEZ v0.3.1 port (`lez-v03/`), which nothing deploys with this
change; no program or guest with an image id in [`docs/image-ids.md`](../docs/image-ids.md) changed, and
`crates/ferrata-core` is unchanged (its public API is used as it is).

Ferrata's light client trusts that at least two thirds of each sync committee are honest
([`docs/trust.md`](../docs/trust.md) §2.2). Until now nothing on chain noticed when that assumption had visibly failed.
This note defines a piece of evidence that it has failed, checkable by the instance itself from what it stores, and a
freeze that follows. An honest network can never produce such evidence for an instance started from a finalized
checkpoint (the assumption of [`docs/trust.md`](../docs/trust.md) §2.1, which `Init` cannot check; §3).

## 1. What counts as evidence

A piece of evidence is one light-client update, valid under the instance's own checks (§4), that contradicts a
finality fact the instance holds. There are two kinds:

* **(A) A recorded finalized header contradicted.** The update's finalized header has the same slot as a header the
  instance has recorded as finalized (a record `header(id, slot)` in the v0.3.1 port) and another root
  (`hash_tree_root` of the beacon block header).
* **(B) An adopted next committee contradicted.** The instance's store is in period `Q` (the period of its finalized
  header) and knows its next committee `N`. The update's attested header and finalized header are both in period
  `Q`, and its next-committee branch proves a next committee `N' ≠ N` (the whole `SyncCommittee`: key root and
  aggregate field, so `CommitteeSummary::root` differs).

What is **not** evidence:

* **Two attested heads on two forks.** A sync-committee member signs the root of the block it holds as its head
  (`get_sync_committee_message`, `specs/altair/validator.md:363-376`, on "the head block" of its fork choice,
  l. 354-360). Forks happen and heads are reorganised, so honest members sign heads of different branches at
  different slots. Attested headers are never compared.
* **A next committee that differs between two states of different periods,** or between an attested state of period
  `Q` and one of another period: each period's next committee is another committee.
* **A next committee proven in an attested state of period `Q` whose finalized header is in an earlier period.** Two
  branches that split before period `Q` began may hold different next committees, and neither branch need be final.
  Kind (B) requires the finalized header in `Q` too (the specification's "sync committee finality",
  `specs/altair/light-client/sync-protocol.md:281-290`).

### 1.1 What "a different chain" can be checked from the data at hand

The instance stores, per record, the beacon header of a finalized block (slot, proposer, parent root, state root,
body root) and its execution summary, and in its store one finalized header and the two committees as summaries. An
update carries an attested header, a finalized header and its branch, and possibly a next committee and its branch.
From these alone:

* **Checkable: the same slot, another root** (kind (A)). Two distinct blocks with the same slot are never on one
  chain: every ancestor of a block has a smaller slot (a block's slot is above its parent's,
  `process_block_header`, `specs/phase0/beacon-chain.md:2350-2354`).
* **Not checkable: a finalized header at a slot with no record** (below, between or above the records). Whether it is
  an ancestor of a recorded header needs the recorded block's state: `state.block_roots[s mod 8192]` holds the root of
  the latest block at or before slot `s` of that block's chain (`process_slot`, `specs/phase0/beacon-chain.md:1858-1869`),
  for the last `SLOTS_PER_HISTORICAL_ROOT` slots, and `historical_summaries` before. A Merkle proof from a record's
  `state_root` to one of those entries would decide it; the records keep `state_root`, so such evidence could be added
  later, with a state proof fetched from an archive node. **Not built.**
* **Not detected at all: a conflicting header finalized above every record.** The instance accepts it as an ordinary
  update: the light client checks no ancestry between the finalized header it holds and the next one (the
  specification's `apply_light_client_update` replaces `finalized_header` without one, `sync-protocol.md:494-509`;
  Ferrata's policy is the same). If the instance later receives an update finalizing a recorded slot of the other
  branch, kind (A) catches it.

**When kind (A) applies.** A conflict lands on a recorded slot when the other branch finalizes a block at the slot of
a header the instance recorded. The finalized header of an update is the block of a finalized checkpoint (the block at
an epoch's first slot, or the latest one before it), and the instance records its checkpoint and the finalized header
of every update that advances its store (an update that only adopts the next committee writes no record). Two
branches that both finalized the same epoch, each with a block at its first slot, finalize two blocks with the same
slot: the shape of Ethereum's finality failing. Nothing here measures how often a conflict takes that shape. A
deliberate forger holding two thirds of a committee chooses the slots of its fabricated finalized headers: at a slot
where the canonical chain has no checkpoint block no honest update ever finalizes a header, so (A) never fires against
those records, and honest updates finalizing later canonical slots are then applied as ordinary updates (no ancestry
check), leaving the forged records attestable. Kind (B) still catches a forged next committee adopted while the
instance's next committee was unknown (honest updates of that period prove the real one); with the next committee
known, a forged one is refused as an update (`NextCommitteeMismatch`, 28), and that signed update is itself evidence
of kind (B). The submitter may choose any update of the other branch whose finalized header is at a recorded slot,
but it must be signed by a committee the instance holds when it lands (§4, §5).

## 2. What the instance does with it

**The freeze.** Evidence that passes the check of §4 freezes the instance: it records the evidence (the kind, the
slot or period, the recorded root or the adopted committee's root, the conflicting root, the evidence's attested
header root, its signature slot and the digest of the update) in the instance's own state, and from then on refuses,
with a code of its own (`FROZEN`, 222 in the v0.3.1 port):

* every `Update` and `Continue` (the store never moves again);
* every `Attest` (no new statement is written under any record);
* every further piece of evidence (the first one stays recorded).

It keeps every record, statement and the store as they were, so that what was attested can still be read and
audited. **Anyone can submit evidence**: no key, no role; the submitter only pays the transaction.

**What the freeze does not do.**

* It does not prove **who** misbehaved. The evidence shows that two finality claims, each carried by a state that at
  least two thirds of a committee the instance trusted signed, contradict each other. Which side is the canonical
  chain, and whether the signers of the record or of the evidence are dishonest (or Ethereum's finality itself
  failed, §3), is outside what the instance can decide.
* **Nothing is slashed.** A sync-committee signature on a wrong state is not slashable in the protocol (the block
  operations slash proposers and attesters only: `process_proposer_slashing` and `process_attester_slashing`,
  `specs/phase0/beacon-chain.md:2422` and `:2449`), and the instance holds no stake.
* It does not revoke what consumers already did with earlier records or statements. A consumer that reads a record
  or a statement directly must read the instance's frozen flag too. The repository's own v0.3.1 bridge port does not
  yet: its `ImportRoot` reads `header(attestation, instance_id, slot)` directly
  (`lez-v03-bridge/guests/bridge/src/main.rs:133`), so paired with this image it would keep importing roots under a
  frozen instance's records, the contradicted one included, until it reads the flag (planned with A1/B1).
* It does not unfreeze. Resuming is a new instance from a new trusted checkpoint, a new trust decision (as after the
  weak-subjectivity bound, `docs/trust.md` §2.1).

## 3. Why an honest network never freezes an instance started from a finalized checkpoint

Assume (H1) that every state whose attested header a supermajority signed, for every update the instance applied and
for the evidence, is a state the beacon chain's state transition produced on a branch honest nodes followed, and
(H2) that the finality of the beacon chain holds: no two conflicting checkpoints are both finalized. (H2) is Casper
FFG's accountable safety: two conflicting finalized checkpoints need validators holding at least a third of the stake
to have signed pairs of attestations that are slashable (`is_slashable_attestation_data`,
`specs/phase0/beacon-chain.md:1141-1151`, the double and surround votes; V. Buterin and V. Griffith, "Casper the
Friendly Finality Gadget", arXiv:1710.09437). A node never imports a block that does not descend from its finalized
checkpoint (`on_block`, `specs/phase0/fork-choice.md:981-987`). Assume also (H3) that the instance's trusted
checkpoint is a finalized block, which the instance already assumes (`docs/trust.md` §2.1).

**(A) is impossible.** A record holds the finalized header of an update the instance applied, whose finality branch
proves its root as `finalized_checkpoint.root` in a signed attested state (`validate_light_client_update`,
`sync-protocol.md:437-456`), or the checkpoint (H3). The evidence's finalized header is proven the same way. Under
(H1) both are blocks of finalized checkpoints of honest branches, so under (H2) one is an ancestor of the other or
they are equal; with equal slots they are equal, and their roots are equal.

**(B) is impossible.** A state's `next_sync_committee` changes only at the last epoch of a period
(`process_sync_committee_updates`, `specs/altair/beacon-chain.md:838-843`), so all blocks of one chain within period
`Q` share it. The store's next committee `N` was adopted from an update whose attested and finalized headers are both
in period `Q`: in Ferrata's policy (as in the specification, `apply_light_client_update`, `sync-protocol.md:494-509`)
the next committee is taken either while it is unknown, from an update whose finalized header is in the store's
period and whose signature (so attested) period is that period too (`sync-protocol.md:423-426`), or at the rotation
into `Q`, from an update finalized in `Q` and signed in `Q` at the latest (`validate_compact`, `steps.rs`). Its
attested block descends from its finalized block `F` (its finality branch), so `N` is `F`'s next committee; the
evidence's `N'` is likewise the next committee of its finalized block `F'` in `Q`. Under (H1)-(H2) `F` and `F'` are on
one chain, both in period `Q`, so `N' = N`.

So, under (H1)-(H3), every update an honest network produces is refused as evidence: either it names no recorded slot
(kind (A)) or no committee of the store's period (kind (B)), or it agrees with what the instance holds. Evidence that
passes therefore shows that (H1), (H2) or (H3) failed: a committee's supermajority signed a state off every honest
branch (the light client's assumption failed), Ethereum's finality failed, or the checkpoint was not final. Each
breaks what the instance's records mean, which is why freezing is the right response to any of them.

## 4. The check

`misbehaviour::check_evidence` ([`misbehaviour/src/lib.rs`](misbehaviour/src/lib.rs)), on the instance's compact store
(`steps::store_for_update` of the stored one, as an update):

1. **The policy:** a finality update (`NotFinalized` otherwise), signed by at least two thirds of the committee
   (`BelowSupermajority`), with a finalized header that is not the genesis placeholder (`FinalizedHeaderInvalid`).
2. **Every check of `validate_light_client_update`** as the core's `steps::validate_compact` transcribes it (shape,
   participation, the attested header's execution branch, the slot order with the slot the caller vouches for, the
   finalized header's execution branch and the finality branch, the next committee's branch), **except two**: the
   relevance check (an update whose attested slot is not above the store's finalized slot is irrelevant to
   advancing, not invalid; evidence is about the past), and `NextCommitteeMismatch` (that mismatch is kind (B)).
3. **A committee the instance holds:** the signature period must be the store's period (the current committee) or the
   next one with the next committee known, exactly as for an update (`PeriodSkipped` otherwise). An older
   committee's keys may have been sold since its members exited; the instance never trusts them again.
4. **The contradiction** of kind (A) against the claimed record (its slot and root; the v0.3.1 port checks the claim
   against the record account in the same transaction) or of kind (B) against the store's next committee
   (`NoContradiction`, 85, otherwise).
5. **The signature:** the signing key `aggregate − Σ non-signers` with the non-signers' keys proven by one multiproof
   against the committee's key root (`residue_agg::signing_key`, `bench/RESIDUE.md` §9), the signature decoded and
   checked in G2, the Miller loop (the core's `Job::step2`), then the final exponentiation (the core's `Job::step3`) or
   the residue-witness check (`bench/RESIDUE.md` §2-§3).

The program also sets the transaction's validity window as for an update: from the start of the signature slot to
the end of the weak-subjectivity period of the stored finalized header, so evidence cannot claim a future slot and an
instance past its period (whose committee may no longer be bonded) cannot be frozen by its old committee.

## 5. Replay and griefing

* **Freezing an honest instance** (one started from a finalized checkpoint) needs a supermajority signature of a
  committee the instance holds on a state that contradicts its records: exactly the failure the light client assumes
  away, or a break of BLS or SHA-256 (§3).
* **A committee the instance does not hold** (an earlier period's, a later one's, or a set of keys of the right period
  that is not the stored committee): `PeriodSkipped`, or `SignatureInvalid` (40).
* **An unfinalized update, a stale one, an honest one replayed:** `NotFinalized`; a stale update is signed by a
  committee no longer held (`PeriodSkipped`) or, if the committee is still held, names a record it agrees with
  (`NoContradiction`, 85); an update with no record at its finalized slot is refused by the record account's check
  (`NO_RECORD`, 213, in the port; the bench's instance model `NoRecord`, 87).
* **Evidence against a frozen instance:** `FROZEN` (222); the first evidence stays recorded.
* **Front-running.** Evidence carries the window an update would have, computed from the store's finalized slot when
  it is planned (`update_window`). Any `Update` that advances the store before the evidence lands (anyone may relay
  one) makes it refuse `WRONG_WINDOW` (218), nothing changed; planned again on the new store, it freezes (§7).
* **Expiry.** Evidence must be signed by a committee the instance holds when it lands. Evidence signed in period Q can
  therefore freeze the instance only until the instance applies its first update finalized in period Q+1 (an ordinary
  handoff, which anyone may relay); from then on it is refused `PeriodSkipped` (23) for good, and the record it
  contradicts stays attestable (§7). Kind (B) likewise: after the rotation the committee adopted in Q is the store's
  current committee, which kind (B) does not compare.
* **What the attestation side's `monitor` (A1) must therefore do:** submit evidence as soon as it sees a conflict,
  planned on the store as it is then; watch for an `Update` that lands first and plan the evidence again at once (it
  stays valid while the instance is in the signing committee's period); and raise a conflict it can no longer submit
  (the instance has left that period) as an alarm for the consumers' freeze authorities, since the instance can no
  longer freeze itself on it.
* **Cost to the submitter:** one transaction, about the gas of a finality update (§8). A refusal before the signature
  is cheaper (at most 306,206 gas); one at the signature or at the record account costs about as much as the evidence
  (up to 9,066,448 gas: keys the instance does not hold, with the final exponentiation; §8).

## 6. What was built

* **`bench/misbehaviour`** (new; [`src/lib.rs`](misbehaviour/src/lib.rs)): `validate_evidence`, `contradiction`,
  `signature_job` and `verify_signature`, `check_evidence` (§4), and `Instance`, a model of an instance (its compact
  store, the root of every header it recorded by slot, the freeze) whose updates run as the v0.3.1 port's
  one-transaction `Update` with `Verify::ResidueSummary`, and which refuses updates, attestations and evidence once
  frozen (`FROZEN`, 86 in the model). `ferrata-core` is used through its public API, unchanged.
* **The LEZ v0.3.1 port** (`lez-v03/`, deployed nowhere with this change): `Instruction::Evidence { instance_id,
  update, signers, verify, against, window }`, `against` either `Against::Record { slot, root }` (the account
  `header(id, slot)` follows the instance's, and an apply there checks that it holds a record of the instance whose
  beacon header has `root`: `NO_RECORD`, 213, or `WRONG_RECORD`, 219, otherwise) or `Against::NextCommittee`; one
  transaction, `verify` `Core` or `Residue`, the non-signers as one multiproof (`EVIDENCE_FORM`, 223, otherwise).
  `InstanceState` gains `frozen: Option<Freeze>`; `Update`, `Continue`, `Attest` and `Evidence` refuse a frozen
  instance with `FROZEN` (222). `Attest` now names the instance's account first (an apply there checks it is not
  frozen and writes nothing), so its account list changed; the harness's and the client's builder follow. The wire,
  the guest, the harness and the program's image id changed (§8); the reviewed image `8d3b59ff…` that the
  public-testnet run deployed (EVIDENCE row 150) is a recorded binary of its own, unchanged.
* **Not built:** the v0.4 experimental copy (`lez/guests/program-residue-experimental`) shares its state layout
  with the deployed program through `ferrata-lez-wire`, whose change would move the deployed program's image id; a
  frozen flag there needs a wire of its own. Evidence of a conflict at an unrecorded slot (§1.1). A monitor that
  watches for evidence and sends it.

## 7. Tests

**Bench** (`cargo test --release -p misbehaviour`, Modal build box, 18 Nov 2026, re-run after an independent review
with the same figures; output [`results/misbehaviour/tests.txt`](results/misbehaviour/tests.txt)): the 3 tests of
`tests/evidence.rs` pass in 7.8 s, and the review's `tests/forks.rs` in 0.07 s.

| test | what | result |
|---|---|---|
| `honest_data_never_freezes` | at every state of five replays (the official `sync` vectors, 52 cases; the mainnet replay `mainnet-p1899`; Sepolia through Gloas, `sepolia-gloas-2026-11-10`; **the lab's record**, every update the lab's mainnet and Sepolia instances applied on its LEZ sequencer, `lez/lab/results/record/net/`), every update of that replay submitted as evidence against the record at its finalized slot (if the instance holds one) and against the adopted next committee | **40,313 submitted, 0 accepted**: 32,711 refused `PeriodSkipped` (23, a committee the instance does not hold), 232 `FinalizedHeaderInvalid` (25, the vectors' genesis placeholder), 189 `NotFinalized` (50), **7,181 `NoContradiction` (85)**. Of the (state, update) pairs, **5,487 pass every check of §4 and their signature verifies under the stored committee** (1,694 of them at a recorded slot, the same root): honest updates are refused for contradicting nothing, not for a failing signature. `validate_evidence` equals the core's `validate_compact` on all 26,635 pairs where the update passes the policy, but where `validate_compact` refuses for one of the two checks evidence drops. 418 updates, 349 of them applied by the replays' policy stores (every one of the real chains') |
| `real_contradictions_freeze` | a chain that never existed (32 members whose keys the test holds, the Electra minimal preset of the `sync` vectors, a fabricated checkpoint): an update that finalizes slot 136 and adopts a next committee; another finalizing slot 136 with another root | refused as an update (`NotNewer`, 52), refused against the adopted committee (85), **accepted as evidence (A)**; the frozen instance refuses an update, an attestation and the evidence again (86), its records and store unchanged (a live copy applies the same update and attests the record). Also accepted: the checkpoint's own record contradicted (22 of 32 signers); **(B)** a next committee other than the adopted one, both headers in the store's period (refused as an update, `NextCommitteeMismatch`, 28); (B) with only the aggregate field changed (the same key root); (A) signed by the held next committee in the next period, through the residue-witness pairing |
| `griefing_and_replay` | 21 submissions on the instance after its first update | refused: signed two periods ahead, or in the previous period (23: committees the instance does not hold); the right period or the next with other keys (40); no finality branch (50); 21 of 32 signers (51); the update that wrote the record replayed against it, and against the adopted committee (85); a contradicting update against a record at another slot (85); a slot with no record (87); a finality branch node altered (26); a signature slot after the slot the caller vouches for (22); a signature over another attested header (40); another next committee whose finalized header is in the previous period, the adopted committee again, a next committee while the instance knows none (85); the genesis placeholder as the finalized header (87: the model finds no record at slot 0; the genesis check itself, 25, is exercised by the honest data's 232 cases); past the weak-subjectivity period (53); a residue witness with one bit flipped (40): 19 refused. Accepted: 22 of 32 signers (two thirds), the honest residue witness. Every refused submission left the instance unchanged |
| `honest_forks_and_reorgs_are_not_evidence` (`tests/forks.rs`, written by the independent review) | honest forks and reorgs on the fabricated chain, each against the record and against the adopted committee: an orphaned head at the canonical head's slot; an orphaned head lagging finality (it finalizes the checkpoint); a head reorg across the period boundary, the orphaned branch carrying another committee for the period after next, signed by the held next committee; a finality update without a committee branch | 8 submitted, **all refused `NoContradiction` (85)**, the instance unchanged |

**Through LEZ v0.3.1's block application** (`lez-v03/harness/tests/misbehaviour.rs`, the harness of
[`../lez-v03/README.md`](../lez-v03/README.md): every transaction public, with a fee, settled in a block as v0.3.1's
sequencer settles it; Modal build box, 18 Nov 2026; output [`../lez-v03/results/misbehaviour-tests.txt`](../lez-v03/results/misbehaviour-tests.txt),
tables `misbehaviour.tsv` and `misbehaviour-honest.tsv` beside it; re-run after the review, byte-identical tables): the
2 tests of `misbehaviour.rs` pass in 37.4 s, and the review's 2 of `misbehaviour_window.rs` in 9.6 s.

* `contradictions_freeze_and_frozen_refuses`, on a chain that never existed (a 512-member committee whose keys the
  test holds, mainnet's preset and fork versions with Electra from genesis, a fabricated checkpoint at slot 81,920;
  each instance initialized in one transaction with the 512 keys, then one update in one transaction that finalizes
  slot 81,984 and adopts a next committee by its summary): (A) refused as an `Update` (52), then applied as
  `Evidence` at 510 and at 342 signers with the residue witness and at 510 with the final exponentiation; (B) refused
  as an `Update` (28), applied as `Evidence`. The instance's `frozen` field holds the expected `Freeze`; its store,
  its count of applied jobs and its records are unchanged. The frozen instance refuses `Update` (222), `Continue`
  (222), `Attest` under its record (222; the same attestation on a live instance is refused by the record's check,
  111) and the same `Evidence` again (222). Refused, the instance unchanged: the contradicting update as an `Update`
  (52); evidence claiming a root the record does not hold (219) and against a slot with no record (213), both
  refused at the record account after the instance's check passed; the update that wrote the record, replayed
  against it and against the adopted committee (85); signed two periods ahead (23); the right period, other keys
  (40); no finality branch (50); 341 signers (51); `Verify::Open` and a signer witness without the multiproof marker
  (223); the residue witness with one bit flipped (40); a window other than an update's (218); the record account of
  another slot (201).
* `honest_mainnet_replay_never_freezes`: the mainnet replay (`Init`, then update_0 to update_3 as one-transaction
  handoffs), and after each of the five steps each of update_0 to update_4 sent as `Evidence` against the record at
  its finalized slot (when the instance holds one) and against the adopted next committee: **36 sent, 36 refused**
  (22 `PeriodSkipped`, 14 `NoContradiction`), never frozen.
* `misbehaviour_window.rs` (written by the independent review; 2 tests, 9.6 s; tables `misbehaviour-window-front-run.tsv`
  and `misbehaviour-window-expiry.tsv`): **front-running**: evidence (A) planned on the store at slot 81,984, then an
  ordinary `Update` finalizing slot 82,176 lands first (applied, 6,621,068 gas): the evidence is refused `WRONG_WINDOW`
  (218, 121,780 gas), nothing changed; planned again on the new store it applies and freezes (6,656,022). **Expiry**:
  a conflict signed by period 10's committee; an ordinary handoff into period 11 (applied, 6,722,853); the conflict is
  then refused `PeriodSkipped` (23, 164,477) for good, the instance live, the contradicted record kept (§5).
* With this image the earlier suites pass unchanged (`v03` 5, `third_party` 4, `adversarial` 5): the same 166
  transactions with the same results and exit codes; execution gas moved by −36,048 to +25,742 (an `Attest` now
  costs about 21,000 to 26,000 gas more: its apply at the instance's account; the tables are in
  [`../lez-v03/results/misbehaviour-regression/`](../lez-v03/results/misbehaviour-regression/); the committed tables
  of EVIDENCE rows 131 and 137 stay those of the image `bac768a8…`).

## 8. Numbers

Execution gas, LEZ v0.3.1's block application (zkVM cycles of the transaction, of `MAX_GAS_EXEC` = 10,000,000;
measured, Modal build box, 18 Nov 2026):

| transaction (one, public) | execution gas | of `MAX_GAS_EXEC` | storage gas (bytes) | result |
|---|---:|---:|---:|---|
| **`Evidence` (A), 510 of 512, residue witness** | **6,656,148** | **66.6%** | 4,319 | applied, frozen |
| `Evidence` (A), 510 of 512, final exponentiation | 9,139,244 | 91.4% | 3,159 | applied, frozen |
| **`Evidence` (A), 342 of 512 (every absent index odd), residue witness** | **8,443,193** | **84.4%** | 25,759 | applied, frozen |
| **`Evidence` (B), 510 of 512, residue witness** | **6,632,378** | **66.3%** | 4,295 | applied, frozen |
| `Evidence` claiming a root the record does not hold | 6,653,851 | 66.5% | 4,319 | refused, 219 |
| `Evidence` signed by keys the instance does not hold (its period), final exponentiation | 9,066,448 | 90.7% | 3,159 | refused, 40 |
| `Evidence`, the residue witness with one bit of `c` flipped | 6,581,344 | 65.8% | 4,319 | refused, 40 |
| `Evidence`, the honest update against its own record | 273,926 | 2.7% | 3,239 | refused, 85 |
| `Evidence` signed two periods ahead | 164,477 | 1.6% | 3,159 | refused, 23 |
| `Update` of a frozen instance | 86,251 | 0.9% | 4,136 | refused, 222 |
| `Attest` under a record of a frozen instance | 46,280 | 0.5% | 772 | refused, 222 |

Evidence fits one transaction like a finality update (`bench/RESIDUE.md` §6 and §9.6: 6,629,246 and 8,318,645 on the
LEZ v0.4 copy; `lez-v03/README.md`: a finality update in one transaction on v0.3.1): the same signature check, plus
its contradiction check and, for kind (A), one apply at the record. A refusal before the signature costs at most
306,206 gas (3.1% of the limit, the 341-signer case, whose input is the largest). One at the signature or at the record
account costs about as much as the evidence: 6,581,344 for a flipped witness bit, 6,640,148 and 6,653,851 at the record
account (213, 219), and 9,066,448 for keys the instance does not hold, verified with the final exponentiation. The program's image id is now `155b7317…635f` (625,824 ELF bytes, 7 segments; `bac768a8…` before this
change; deployed nowhere; none of them is in `docs/image-ids.md`).

## 9. What an auditor must check

1. That `validate_evidence` differs from `steps::validate_compact` in the two checks of §4.2 only (the differential
   test of §7 compares them on every update of the data sets).
2. The argument of §3 and the spec lines it rests on, at the pinned consensus-specs v1.7.0-beta.4 (commit
   `aa16bb4c`, `spike/upstream/consensus-specs-v1.7.0-beta.4` beside the repository).
3. That the port's record check compares the claimed root with `hash_tree_root` of the record's beacon header at
   the account derived from the claimed slot, and that a frozen instance refuses `Update`, `Continue`, `Attest` and
   `Evidence` before any other work.
4. That every consumer of records or statements reads the instance's frozen flag (`lez-v03-bridge` does not yet, §2).
5. That whatever submits evidence (the attestation side's `monitor`, A1) does so while the instance is in the signing
   committee's period and plans it again after an advancing update (§5).

## Re-check

`cd bench && cargo test --release -p misbehaviour -- --nocapture` (the vectors first: `scripts/fetch-vectors.sh`);
`cd lez-v03 && export RISC0_HOME=$PWD/../../risc0-home-lez && cargo test --release -p harness-v03 --test misbehaviour
--test misbehaviour_window -- --nocapture --test-threads 2` (LEZ v0.3.1 checked out beside the repository: `lez-v03/README.md`, "Re-check").
