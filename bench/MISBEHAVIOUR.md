# Freezing a light-client instance on proof that a sync committee signed conflicting finality

**Experimental and unaudited. The deployed program does not have this.** Written on 18 Nov 2026. The check lives in
a bench crate (`bench/misbehaviour`) and in the LEZ v0.3.1 port (`lez-v03/`), which nothing deploys with this
change; no program or guest with an image id in [`docs/image-ids.md`](../docs/image-ids.md) changed, and
`crates/ferrata-core` is unchanged (its public API is used as it is).

Ferrata's light client trusts that at least two thirds of each sync committee are honest
([`docs/trust.md`](../docs/trust.md) §2.2). Until now nothing on chain noticed when that assumption had visibly failed.
This note defines a piece of evidence that it has failed, checkable by the instance itself from what it stores, and a
freeze that follows. An honest network can never produce such evidence (§3).

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

In practice a real conflict is likely to land on a recorded slot: the finalized header of an update is the block of a
finalized checkpoint, the block at an epoch's first slot or the latest one before it, and the instance records the
finalized header of every update it applies. Two branches that both finalized the same epoch, with a block at its
first slot on each, finalize two blocks with the same slot. The submitter may choose any update of the other branch
whose finalized header is at a recorded slot, but it must be signed by a committee the instance still holds (§4).

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
* **Nothing is slashed.** A sync-committee signature on a wrong state is not slashable in the protocol (only
  proposer and attester offences are, `specs/phase0/beacon-chain.md`), and the instance holds no stake.
* It does not revoke what consumers already did with earlier records or statements. A consumer that reads a record
  or a statement directly must read the instance's frozen flag too.
* It does not unfreeze. Resuming is a new instance from a new trusted checkpoint, a new trust decision (as after the
  weak-subjectivity bound, `docs/trust.md` §2.1).

## 3. Why an honest network never freezes an instance

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
period and whose signature (so attested) period is that period too (`sync-protocol.md:422-426`), or at the rotation
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

* **Freezing an honest instance** needs a supermajority signature of a committee the instance holds on a state that
  contradicts its records: exactly the failure the light client assumes away, or a break of BLS or SHA-256 (§3).
* **A committee the instance does not hold** (an earlier period's, a later one's, or a set of keys of the right period
  that is not the stored committee): `PeriodSkipped`, or `SignatureInvalid` (40).
* **An unfinalized update, a stale one, an honest one replayed:** `NotFinalized`; a stale update is signed by a
  committee no longer held (`PeriodSkipped`) or, if the committee is still held, names a record it agrees with
  (`NoContradiction`, 85); an update with no record at its finalized slot is refused by the record account's check
  (`NO_RECORD`, 213, in the port; the bench's instance model `NoRecord`, 87).
* **Evidence against a frozen instance:** `FROZEN` (222); the first evidence stays recorded.
* **Cost to the submitter:** one transaction, about the gas of a finality update (§8); a refusal is cheaper.

## 6. What an auditor must check

1. That `validate_evidence` differs from `steps::validate_compact` in the two checks of §4.2 only (the differential
   test of §7 compares them on every update of the data sets).
2. The argument of §3 and the spec lines it rests on, at the pinned consensus-specs v1.7.0-beta.4 (commit
   `aa16bb4c`, `spike/upstream/consensus-specs-v1.7.0-beta.4` beside the repository).
3. That the port's record check compares the claimed root with `hash_tree_root` of the record's beacon header at
   the account derived from the claimed slot, and that a frozen instance refuses `Update`, `Continue`, `Attest` and
   `Evidence` before any other work.
4. That every consumer of records or statements reads the instance's frozen flag.
