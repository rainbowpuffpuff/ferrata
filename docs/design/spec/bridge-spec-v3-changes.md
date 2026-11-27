# Ferrata bridge: specification v3, the changes to v2

10 Nov 2026. Changes to [specification v2](bridge-spec-v2.md) (SPEC-v2); where the two differ,
this file wins. Sources: the finality-rule analysis ([LEMMA.md](../finality-rule/LEMMA.md),
[MODEL.md](../finality-rule/MODEL.md), [RESULTS.md](../finality-rule/RESULTS.md)) and 13 spec
problems found while building the v2 vault. Everything in SPEC-v2 not mentioned here stands.

## D1. Why

SPEC-v2 §4.6 claimed finality from (i)–(iv). That claim is false: with all honest sequencers
stalled for about Δ − 1 h and two short private Bedrock forks shown to two honest sequencer
nodes, the vault finalises a block LEZ later drops, with fewer than `t` corrupted keys (none
at n = 2). Validity is unaffected. Also, §4.2(4) never checked the excluded producer's
inscription, so the exclusion was vacuous and `t` keys could get an invalid block accepted.

## D2. §1 Identities: deployment order

The vault address is predicted from the deployer's address and nonce (plain CREATE). CREATE2
is not usable: the init code contains `lezConfigId`, which contains the vault address.

## D3. §4.2 The checkpoint program

Item 4 (support) is replaced by:

4. **Producer and support.** The program checks one inscription of `T` exactly like a counted
   one (`channelId` pinned, payload = `T`'s borsh bytes, Ed25519 signature over the Mantle
   transaction hash under the claimed key). Its signer `D` must be in the key set `K`. Let
   `Y = T` if `echoNonce = 0`, else the first block after `T` containing the 32-byte
   `echoNonce` in a transaction addressed to `bridgeHeader`. The blocks in `(Y, E]` include
   blocks with checked inscriptions by at least `t` distinct keys of `K` other than `D`.
   With `allowSingleKey` and `n = 1`, the one key is `D` and the supporter.

Item 5 (journal), 17 words:

`abi.encode(CHECKPOINT_DOMAIN, evmChainId, vault, lezConfigId, nMin, tMin, parentState,
newState, burnRoot, burnCount, importedLeafCount, sunset, echoNonce, tipId, tipHash, n, t)`

- `CHECKPOINT_DOMAIN = H("ferrata/vault/checkpoint/v3")`.
- `tipId:u64`, `tipHash:bytes32` are `T`'s; `n:u32`, `t:u32` are `|K|` and `t(n)` for the key
  set in `newState`. They let a monitor compare a candidate with an honest LEZ node, and let
  the vault size the wait by committee.

Item 3 (key-set changes) gains a rule, and the key-set hash binds the turn length:

- `keySetHash = H("ferrata/keyset/v2" ‖ n:u32 ‖ postingTimeframe:u32 ‖ key_1:32 ‖ … ‖ key_n:32)`.
- The program refuses a `ChannelConfigOp` whose `posting_timeframe` differs from the one in
  the current key-set hash, or is 0 (with 0 a key that keeps posting holds its turn forever).
  A zone that changes its turn length is a change the configuration cannot follow: retire it.
- The deployment check verifies that the vault's `turnSeconds` equals that `posting_timeframe`
  times the slot duration. Without this pin the wait's turn term would be unsound.

## D4. §4.3 Vault: candidates and finalisation

- New immutable `turnSeconds` (the channel's `posting_timeframe` in seconds; 900 on the
  repository's testnet values). The wait of a candidate is
  `W = deltaAtSubmit + (t − 1) × turnSeconds`, with `t` from its journal.
  `finalize` requires `beacons[i].timestamp >= submittedAt + W`.
  Reason: `t − 1` corrupted keys can each hold a full turn with empty inscriptions, during
  which no honest block can land.
- `deltaFloor` (immutable) is a budget, not "twice the expected time":
  `deltaFloor >= Δ_g + S + ε`. Δ_g: a window in which Logos Blockchain produces `k` blocks
  except with negligible probability (2 h on the template values: 1.3e-18). S: the longest
  stall of honest sequencers the deployment tolerates without relying on the guard below
  (default 1 h). ε: how early the beacon value can be known (1 min). Default floor: 3 h 01 min.
- The vault also checks `n >= nMin` and `t >= tMin` itself, at candidate and at finalisation.
- **Cancelling a candidate.** `cancelCandidate(newState)`: freeze authority, reject-only.
  Deletes a pending candidate; the same state can be submitted again and waits again.
  `monitor` calls it when a candidate's `(tipId, tipHash)` is not in an honest LEZ node's
  final tier by `submittedAt + Δ_g`. In the simulation this stopped every modelled attack,
  including the long eclipse; a freeze that is lifted once the tip is overtaken did not, and
  a freeze held until expiry halts the vault.
- **Checkpoint freeze.** One reject-only flag of the freeze authority blocks both
  `submitCandidate` and `finalize`; it expires like the other freezes. It is the general
  pause. `monitor` sets it only when an honest node's final tier has not advanced for
  `(t − 1) × turnSeconds + S/2` (a shorter trigger pauses the vault for hours a day when a
  minority merely holds its turns), and flags candidates whose key set lags the channel's
  live one.
- Events `CandidateSubmitted` and `CheckpointFinalized` carry `tipId`, `tipHash`, `n`, `t`.
- Monotonicity, checked by the vault: `importedLeafCount` never decreases along finalised
  checkpoints; once a finalised checkpoint has `sunset = true`, every later one has
  `sunset = true` and the same `importedLeafCount`.
- A revocation invalidates every pending candidate (`revocationEpoch`). A finalisation does
  not (candidates on the old `finalState` are unusable anyway).
- `delta` has a ceiling of 30 days.

## D5. §4.4–4.5 Revocation, retirement, refunds

- `revokeFrom` refuses an index that is already revoked.
- The sunset boundary is set by the first non-revoked sunset checkpoint and never lowered by a
  later one. Revoking that checkpoint clears it.
- `refund` additionally requires: the vault is `retired` (so the deposit root no longer
  moves and paths cannot go stale); `block.timestamp >= sunsetFinalizedAt + revokeWindow`
  (a forged sunset checkpoint can be revoked before any refund); releases are not globally
  frozen (the release freeze covers refunds).
- Constructor: `minReleaseDelay` immutable; a token's release delay must satisfy
  `minReleaseDelay <= delay <= revokeWindow`, so a checkpoint can be revoked for as long as
  its releases are delayed.

## D6. §4.6 Assumptions (replaces the v2 paragraph)

**Validity.** A burn root the vault accepts comes from valid LEZ blocks, provided:
(V1) fewer than `t` keys of every key set the checkpoint program has used are controlled by
an attacker, at any time, including keys that have since left the committee; (V2) an honest
sequencer extends only blocks its own node re-executed, and execution is deterministic;
(V3) the proof system is sound.

**Finality.** Those blocks are also final on LEZ (in every honest node's final tier, never
reverted), provided in addition:
- (F1) on the Logos Blockchain node of every honest sequencer, as its LEZ node has processed
  it, no reorganisation goes deeper than `k`, and at least `k` blocks arrive in every window
  Δ_g (no eclipse, no lag, no slowdown);
- (F2) whenever an honest sequencer holds a LEZ block of height `h`, a block of height at
  least `h` enters the canonical chain's fold within `(t − 1) × turn + S`. This needs: at most
  `t − 1` adversarial keys in the channel's live accredited set, not only in `K`; `turn` equal
  to the live `posting_timeframe` and non-zero; honest sequencers up except for stalls of at
  most S; their inscriptions neither censored nor kept off the chain by eclipse;
- (F3) nobody learns a beacon value more than ε before the Ethereum block that records it;
- (F4) `W >= Δ_g + (t − 1) × turn + S + ε`, which the vault enforces together with D3's rule
  that the program refuses a change of `posting_timeframe`.

**What breaks it.**
- `t` keys can sign any history. The candidate is public for `W` before it can finalise;
  `monitor`'s tip check cancels it, and `revokeFrom` un-accepts it within the window.
- Turn hogging by the adversary's own keys does not break the rule: `W` absorbs it.
- A never-final block can be finalised with fewer than `t` keys, even with none, if F2 fails
  for more than S beyond the hog and the attacker can twice show two honest sequencer nodes a
  short private fork. F2 fails if honest sequencers stall or their inscriptions are kept off
  the chain.
- Two honest sequencer nodes eclipsed for about `W` defeat the wait whatever the other honest
  sequencers do.
- `monitor`'s tip check stops all of these, provided its own node is not eclipsed, it and the
  freeze authority are live, and the offending candidate is cancelled.

**Floors.** Outside testnets `nMin >= 3` and `tMin >= 2`. The rule tolerates `n − 1 − t`
silent keys: none for `n <= 5`.

**Chains.** The beacon is `prevrandao`; a deployment on an EVM chain whose `prevrandao` is
constant or sequencer-chosen needs another beacon source and is out of scope.

## D7. Mode (B): final-tier attestations (specified; built if Logos chooses it)

Each sequencer signs, with its channel key under a separate domain,
`("ferrata/final-tier/v1", evmChainId, vault, lezConfigId, channelId, tipId, tipHash, finalMsgId)`
for its node's final tip. The program accepts `T` if at least `t` keys of `K` signed final
tips that are `T` or descend from `T` on the hash-linked path. One proof per checkpoint, no
beacon, no producer exclusion; F2, F3 and the chain-growth half of F1 drop out. The vault
keeps a wait (default 1 h) only as the reaction window against `t` stolen keys. Needs a
signer next to every sequencer.

## D8. Known limits kept (documented, not fixed in this version)

- A tripped verifier stop strands all escrow except owed balances, unless a sunset checkpoint
  was finalised first.
- The stop wrapper's owner is not moved by a freeze-authority handover, and the upstream
  wrapper lets its owner renounce.
- A compromised freeze key can renew freezes, revoke honest checkpoints within the window and
  retire the vault. Replacing it through the admin under a timelock is an open decision for
  whoever holds the keys.
- A payee that can never receive keeps its `owed` balance forever.
- Retired keys stay counted until a finalised checkpoint carries the rotation; LEZ's
  `exit_delay` (about 40 min on testnet values) is shorter than `W`.

## D9. §3.2–3.3 Leaf index range

`leafIndex < 2^32` everywhere: the vault refuses a deposit once the tree is full, the core
refuses an accumulator-leaf fact with an index of 2^32 or more, and the claim recomputes the
leaf from a preimage that binds the claimed index. Otherwise indices `i` and `i + 2^32` would
share a path and give one leaf two identifiers and two nullifiers.

## D10. From the checkpoint prototype ([`checkpoint/`](../../../checkpoint/))

- Key-set changes: Logos Blockchain requires exactly the live `configuration_threshold`
  signatures on a `ChannelConfigOp`; the program checks that, requires the threshold to be at
  least `t(n)`, and refuses a key list with repeated keys.
- `allowSingleKey` is not a free flag: it holds only when the pinned `nMin <= 1`.
- `importedLeafCount` in the state is the maximum seen, so it cannot decrease.
- The deployment check refuses a genesis state whose `posting_timeframe` is 0, and verifies
  `turnSeconds` against the slot duration, which the program cannot see.
- Provers must read blocks from the channel's inscriptions: a block served by an RPC or
  indexer may carry a different `bedrock_status` byte, and its inscription check then fails.
- The program sees burns only as public top-level calls, as the public part of private
  transactions and as private→public boundary deliveries to `bridgeHeader`. The bridge
  program must refuse a burn arriving any other way (a nested call from a public program, an
  asynchronous message, the bridge run as a private actor), or tokens would be destroyed
  without a burn leaf. Whether a v0.4 program can tell these apart is an M0 check.
- Cost: each counted inscription is BLAKE2b over the whole block (55.9 cycles per byte, no
  accelerator): 0.89M cycles for an empty block, 54.1M for a block with four private
  transactions. Provers choose a small tip and small support blocks; mode (B) does not hash
  blocks at all.

## D11. From the vault v3 build ([`vault/`](../../../vault/)): spec fixes not yet in the prototype

- **Retirement must not depend on the freeze key.** D5 makes refunds require a retired vault,
  and only the freeze authority can `retire()`. Fix: anyone may call `retire()` once a sunset
  checkpoint has been final for longer than `revokeWindow`.
- **One-second overlap.** `revokeFrom` is allowed while `block.timestamp <= finalizedAt +
  revokeWindow`; refunds, and releases whose delay equals `revokeWindow`, open only when
  `block.timestamp > finalizedAt + revokeWindow`.
- **Cancellation is not sticky**: the same public proof can be resubmitted, so `monitor`
  cancels again within every `W`; a persistent attacker is answered with the checkpoint freeze.
- `finalize` should also compare `t` and the tip with the candidate's, and check
  `importedLeafCount <= nextLeafIndex` (cheap defence in depth).
- The vault's runtime is 23,650 bytes, 926 under the EIP-170 limit: further logic goes into a
  linked library.

## D12. From the bridge core v2 build ([`bridge/`](../../../bridge/))

- `burnFrontierHash` is the hash of a tagged, canonical frontier (unused levels zero), as the
  crate implements it; SPEC-v2 left the encoding open.
- `mac` also covers `memo`: `mac = HMAC(kView, "ferrata/burn-mac/v2" ‖ r ‖ memo ‖ token ‖ amount ‖ COut)[0..16]`.
- The witness-independence of the release and claim guests' cycle counts is a CI gate
  (64 random witnesses, one count). The first release guest leaked the leaf index's
  popcount through its cycle count; the gate found it.
- `kView` discloses the holder's burns and recipients only, not its deposits or claims.

## D13. Changes of 10 Nov 2026 (in the proposal, not in the prototypes)

**Rule (B) is the proposed rule; rule (A) is the fallback.** The vault carries both
finalisation paths and an immutable `mode` chosen at deployment.
- (B) message: `("ferrata/final-tier/v2", evmChainId, vault, lezConfigId, channelId,
  keySetHash, tipId, tipHash, finalMsgId)`, an Ed25519 signature over the whole tagged message
  (never over a bare 32-byte digest). `keySetHash` is the attestor's live key set and must
  equal the proof's `K`.
- (B) keys: each sequencer's channel key signs once a certificate
  `("ferrata/attest-key/v1", channelId, attKey)`; the proof carries the certificates; a
  certificate dies when its channel key leaves `K`. The channel key never signs bridge data.
- (B) wait: default 1 h, a reaction window only. Watchers check a candidate on submission
  (its tip must already be in their final tier), not at `submittedAt + Δ_g`.
- (B) needs no beacon, so it is the mode for EVM chains whose `prevrandao` or timestamp is
  constant or sequencer-chosen. It tolerates `n − t` silent keys (no producer exclusion).
- (A) condition F2 is read with the channel's LIVE key set (D6). Watchers CANCEL (not only
  flag) a candidate whose `keySetHash` is not the live set at its tip; otherwise a prover
  could prove against a stale, smaller set, get a shorter wait, and let newly staked keys
  hog more turns than the wait covers. Recommended committee for (A): `n >= 6`.
- (A) Δ_g = 2 h gives 1.3e-18 only at full participation (3.7e-4 per window at two thirds);
  a deployment that expects less than full participation sets Δ_g = 3 h.
- `submitCandidate` refuses a `newState` that is already pending (keyed on the state, not on
  the seal), so nobody can restart an honest candidate's wait by replaying its proof.

**Roles.**
- `watchers`: up to three hot addresses set by the admin under a timelock. Any one can
  `cancelCandidate` and pause checkpoints for at most 12 h; nothing else.
- `freezeAuthority`: a multisig. Freezes up to `maxFreeze` (default 30 days), `revokeFrom`
  within `revokeWindow` (default 7 days), `retire()`, LEZ `Sunset`. The admin replaces it
  after a 14-day public timelock (closes D8's open decision).
- The emergency stop's owner is renounced in the deployment script. Only RISC Zero's own
  `estop(receipt)` (a verifying receipt with a zero claim digest) can trip it. A bug in
  Ferrata's guests is answered by the release freeze and `revokeFrom`. After a stop nothing
  more is released or refunded.
- `retire()` and LEZ `Sunset` become permissionless once no checkpoint has finalised for
  30 days (supersedes D11's first item).
- The admin cannot set a cap below an immutable floor. An unrevoked wrong checkpoint older
  than `revokeWindow` halts the vault for good: documented.

**Inbound.**
- `ImportRoot` attests slots 0 and 1 under one record and stores the root with its leaf
  count. A claim refuses `leafIndex >= count`. `importedLeafCount` in the checkpoint state is
  that count, so "claimable" and "refundable" are complementary. Refunds draw on the release
  cap bucket.
- The gas-drop receipt's journal binds `(N, oneTimeAddress, relayer)`.

**Outbound.**
- `NOut = H("ferrata/nf-out/v2" ‖ sOut)`: independent of `burnIndex`, so a burn that is
  collected again at another index after a revocation cannot be released twice.
- The write-once burn account is keyed on the hash of the whole note
  `(r, memo, mac, token, amount, COut)`, so copying a pending burn's tag cannot make it revert.
  *Amended by D14.*
- Release uses the newest root whose release delay has passed.

**Fees.** A deposit escrows the denomination plus the registered mint and relay fees; the
claim mints the denomination to the user and the fees to the fee recipient and the relayer.
A burn of one denomination releases it less the registered burn, relay and checkpoint fees;
the checkpoint fee goes to whoever finalised the root used. LEZ balances stay whole
denominations. Default cap per token and day: the smaller of 10% of escrow and a
deployer-set ceiling. **Amended for F.13 by ADR 0046 (26 November 2026):** the depositor
still pays denomination + mint + relay and caps count that whole payment, but the vault credits
the mint fee on Ethereum and escrows only denomination + relay. The claim names no schedule
range and mints only those two amounts; refunds exclude the collected mint fee.

## D14. Changes of 22 November 2026 (the security reviews H-1 and H1; ADR 0032)

**Outbound.**
- The write-once burn account is keyed on the tag: `burn(cid, r)`. A second note with a used tag is refused
  (`ALREADY_WRITTEN`) and its whole transaction reverts. Under D13's keying, two notes with one tag (a wallet that
  reused its counter) were both applied and shared one `sOut`, hence one `NOut`: one of the two could never be
  released. The cost: whoever sees a pending burn before it is applied can make it revert by burning a denomination of
  its own under the same tag first (the owner loses the refused transaction's fee, its denomination whole, and burns
  again with its next counter; the copier can release its copy, whose `COut` is of its own secret, so the attack costs
  it fees only: ADR 0032).
- A wallet reserves its counter before it proves a burn and never gives one counter to two burns; the same burn sent
  again (the same note) is the only reuse, and the program applies at most one of the two.
