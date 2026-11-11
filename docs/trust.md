# Ferrata: trust assumptions

**The trust document of RFP-021 (S.6, VS.4, VS.5) and RFP-022 (Security #1–#9, R.9, Privacy #4,
#6). First draft, 11 November 2026.** It says who is trusted for what, what each failure does, what
bounds the loss, and what an observer can still learn. It covers the light client and its
attestations (inbound) and the return leg from LEZ to Ethereum (outbound). Every statement cites its
source; paths are relative to the repository root. The code is a prototype, **not audited and not
for production** ([`README.md`](../README.md), "Not audited"): an audit of the consensus and inclusion
logic, the vault, the bridge program and the proof programs is planned and not done.

Labels as in the design notes: **measured**, **derived**, **estimated**, **not built**
([`docs/design/attestation.md`](design/attestation.md), [`docs/design/bridge.md`](design/bridge.md),
opening sections).

## 1. Summary

| You trust | For | If it fails | What bounds the loss | Section |
|---|---|---|---|---|
| The starting checkpoint of an attestation instance | that the instance follows the real Ethereum | a false chain from the start | anyone can compare the checkpoint root with public Ethereum before relying on the instance | §2.1 |
| The vault's deployment parameters: `genesisState` (the starting key set, turn length and tip), `lezConfigId`, the image ids, `turnSeconds`, `nMin`/`tMin` | that the vault follows the real LEZ channel from the right state with the right programs | a false LEZ history or program accepted from the start | anyone can check them before depositing: the channel's public state, `configId`'s preimage (it binds the vault's address), reproducible builds of the guests; the deployment script checks none of them and accepts `nMin = tMin = 1` (testnets only: it refuses every chain but anvil, Sepolia, Hoodi and Holesky) | §3.2, §4 |
| Two thirds of each 512-member sync committee | that every recorded header was finalized | forged records: facts about blocks that never existed are attested | consumer caps and freezes; `monitor`'s alarm (not built) | §2.2 |
| Fewer than `t` of the LEZ channel's accredited keys are an attacker's, in every key set the checkpoint program used | validity of every burn root the vault accepts (both rules) | `t` keys can forge a burn root | the public wait, watchers, the release delay and revocation, release caps | §3.2 |
| Logos Blockchain: no reorganisation deeper than `k` on the attestors' nodes | finality of accepted burn roots under rule (B) | a released burn is later undone on LEZ | the same | §3.3 |
| Rule (A)'s conditions F1–F4, or else a live, uneclipsed watcher | finality under rule (A) | as above, with fewer than `t` keys or none | the same; outside F1–F4 the watchers are trusted for safety | §3.4 |
| RISC Zero's proof system (and LEZ's privacy circuit) | that every accepted proof is true | anything a proof gates | the emergency stop, tripped only by RISC Zero's own proof of unsoundness | §5 |
| Ferrata's own code (core, programs, guests, vault) | everything | depends on the bug | caps, freezes, `revokeFrom`; immutability means no fix in place | §6 |
| Admin and freeze authority (multisigs) | liveness only: they cannot move escrow or accept anything | denial of service | freezes lapse after 30 days; keyless retirement; 14-day public role changes | §4 |
| Everyone else (submitters, importers, provers, relayers, wrappers, RPC providers) | liveness only | refused, or replaced by anyone else | — | §2.4, §4 |

## 2. The light client and its attestations (RFP-021)

### 2.1 The chain of trust and the checkpoint

```
checkpoint block root ─ Init: bootstrap verified; config fixed (fork schedule, genesis time, slot duration)
 └─ current committee (proven from the checkpoint state)
     └─ finality update, ≥ 2/3 of the committee, domain = (fork version at signature_slot − 1, genesis_validators_root)
         ├─ finality branch ─► finalized beacon header ─► execution binding
         │   └─► header(instance, slot): write-once record (number, hash, state root, receipts root)
         └─ next-committee branch ─► adopted at the handoff
consumer: record ─ state root ─ account ─ storage slot  |  receipts root ─ receipt ─ log  |  ─ accumulator leaf  |  ─ EIP-2935 hop
```

From [`attestation.md`](design/attestation.md) §2. Two things are trusted: the starting checkpoint and
an honest two-thirds supermajority of each sync committee. Everything below the signature is checked by
hashes and Merkle branches, and execution roots reach a consumer only from a record, never from the
caller.

**Who supplies the checkpoint.** Whoever initialises an instance chooses it; an instance's id is the
hash of its configuration and checkpoint, so an instance cannot be reconfigured and a consumer that pins
the id pins the checkpoint ([`attestation.md`](design/attestation.md) §3.4). In the lab, the Sepolia
instance starts from the period-1378 bootstrap of a pinned fixture, and the mainnet instance from the
finalized checkpoint a public beacon API reported, which a second provider reported canonical
([`lez/lab/README.md`](../lez/lab/README.md), "The two instances").

**How a verifier confirms it independently.** By comparing the checkpoint's block root with several
independent beacon providers and with its own node, and checking that the chain id and
`genesis_validators_root` belong together (the program cannot: nothing a light client verifies commits
to the execution chain id). `verify_checkpoint` will do this (at least three providers and
`verified_proxy_module`'s finalized block) and is **not built**
([`attestation.md`](design/attestation.md) §11.4; [`lez/RESULTS.md`](../lez/RESULTS.md) §5, "Decisions" 1).

**Weak subjectivity.** An instance that falls outside the weak-subjectivity period must be replaced from
a fresh checkpoint under a new root id: a new trust decision. In the design the bound is fixed per
instance (about two weeks on mainnet) and an update too far past the finalized slot is refused, with no
clock. **Not built:** neither the program nor the core enforces a bound today, so an instance can be
advanced from an old checkpoint, one period at a time, by any chain of handoffs its committees signed
([`attestation.md`](design/attestation.md) §3.6, §4.6).

### 2.2 Two thirds of each sync committee

Following the chain through committee signatures trusts that at least two thirds of the current
512-validator committee are honest: weaker than Ethereum's full economic finality. A colluding committee
can sign a header off the canonical chain.

**Capture by stake** (computed; a lower bound, because the proposers of the last slots before the seed is
fixed can bias it by withholding RANDAO reveals; [`attestation.md`](design/attestation.md) §11.1). With a
share *p* of stake, the probability of holding at least 342 of 512 seats:

| *p* | per period (27.3 h) | at least once over a year's 321 periods |
|---|---|---|
| 0.50 | 1.2 × 10⁻¹⁴ | 3.8 × 10⁻¹² |
| 0.55 | 3.6 × 10⁻⁸ | 1.1 × 10⁻⁵ |
| 0.60 | 8.9 × 10⁻⁴ | 0.25 |
| 0.65 | 0.21 | ≈ 1 |

**Capture by bribery** has no protocol cost: each committee is known a period ahead, and a bad
sync-committee signature is neither slashed nor loses a reward, so the bribe has no floor. The design
therefore **bounds the loss, not the cost**: a consumer sizes what its caps let through before a freeze to
what it can afford to lose ([`attestation.md`](design/attestation.md) §11.2).

**What a captured committee can do (VS.4).** Sign a forged handoff and hold the instance until someone
notices; sign finality updates for blocks that were never finalized, whose records then attest anything
the forger put in their state. For the bridge, a forged record whose vault slot 0 holds invented deposits
lets `ImportRoot` accept them and claims mint wrapped tokens with no escrow behind them. **What bounds it:**
the bridge's mint caps (per token and global, on LEZ) until the freeze authority freezes mints; wrapped
supply then exceeds escrow by at most what the caps let through ([`bridge.md`](design/bridge.md) §13,
"Supply"). Those caps and freezes run on attested Ethereum time, the execution timestamp of a record,
and a record moves a clock only in a LEZ block stamped at or after it: a captured committee's
far-future record refills no bucket and ends no freeze unless a block producer also stamps its block
that far ahead ([`lez/BRIDGE.md`](../lez/BRIDGE.md) §6). `monitor`, which compares records with several providers' canonical roots and feeds consumers'
freeze authorities, is **not built** ([`attestation.md`](design/attestation.md) §11.3). A deeper inbound
finality setting would not help: it delays honest claimants, not a captured committee
([`bridge.md`](design/bridge.md) §5.4).

**Evidence that a non-captured committee is required (VS.1, VS.2).** A self-consistent update about a
chain that never existed is refused on the signature alone (the fabricated-chain test, consensus half,
native); participation at 21, 22, 23 and 32 of 32 is tested; an update for another chain's instance is
refused with 40; 107 hand-picked mutants are killed; Helios is a differential oracle on 33 of the 52
official cases ([`attestation.md`](design/attestation.md) §3.4, §12). The inclusion half of VS.1 and VS.2
through the program are **not built** (milestone A2).

**The policy's liveness limit.** A header enters only through a finality update; the program keeps no
optimistic header. If Ethereum finalizes nothing through a whole sync-committee period, the instance stops
safely and must be replaced ([`attestation.md`](design/attestation.md) §4.1). The same follows if a third of
one committee (171 of 512) withholds its signatures for a whole period: without force updates the instance
cannot move to the next committee. The bridge pins the instance in `configId`, so replacing it is a full
migration (`Sunset` by the freeze authority, then a new configuration and vault); the minority's cost is its
sync rewards and penalties for about 27 hours (security review of 11 November 2026, I-6; not changed: force
updates would admit headers on weaker evidence).

### 2.3 What an attestation proves, and what it does not

**Proves:** that an account, a storage value, a log of a successful transaction, or a leaf of an
accumulator held in a slot, is in an execution block whose header was finalized, according to a chain of
committees each signed by two thirds of its predecessor, starting from the instance's checkpoint; for
the chain id in its identifier ([`attestation.md`](design/attestation.md) §2, §5).

**Does not prove** ([`attestation.md`](design/attestation.md) §4.6, §5.2, §6.2):

- that the full validator set finalized the block (only that two thirds of a sync committee signed);
- that the fact is current: a record is a snapshot of one block, and records lag the chain by the update
  cadence plus finality;
- that an event happened once, for storage facts: a value that changes and changes back has one
  identifier; once-per-event consumers key on logs or accumulator leaves;
- anything about privacy on Ethereum: Ethereum activity is public; a private read publishes which record
  was read and the consumer's program, and a private call publishes its query (RFP-021, "What the
  attestation does not do");
- anything about LEZ state on Ethereum (that is the bridge's return leg, §3);
- freshness relative to the weak-subjectivity period (not built, §2.1).

### 2.4 Off-chain participants of the light client

| Who | Provides | Can affect | If it lies | If it stops | Observes |
|---|---|---|---|---|---|
| Any submitter or prover | updates | liveness only | refused: signatures are checked | anyone else submits | public data |
| Beacon and execution providers | light-client data, proofs, receipts | liveness only | refused: checked against signatures and records | another provider, or your own node | which blocks are fetched |
| Checkpoint source | each fresh checkpoint | **safety**, until checked | a false root; `verify_checkpoint` (not built) or a manual comparison catches it | nothing to stop | nothing |
| LEZ sequencers | inclusion, block timestamps | liveness only: the bridge's LEZ-side clock is attested Ethereum time, which a producer can delay (by leaving out a `Tick`) but not move ([`lez/BRIDGE.md`](../lez/BRIDGE.md) §6) | cannot forge a record | the zone stops | public transactions |
| Deployer | the immutable deployment | nothing after deployment | — | — | nothing |

From [`attestation.md`](design/attestation.md) §11.5. An update is accepted on validity alone, never on
the submitter's identity (VS.6; §4.5 there). Apart from the checkpoint source, no participant can make
the program accept a false record (VS.5).

## 3. The return leg (RFP-022 outbound)

### 3.1 What LEZ v0.4 lets Ethereum check

A valid LEZ burn is three facts: the burn is in a given block, once (provable cheaply: the block hash is
SHA-256 over all transactions); that block and all before it are valid (not provable: v0.4 blocks carry
no state root, and fees, the gas cap and out-of-gas reverts depend on executor cycle counts that a
receipt claim does not contain); the block's inscription is final on Logos Blockchain (not provable
without replaying the ledger). The last two can only come from the zone's accredited sequencers, as for
LEZ's own cross-zone delivery ([`bridge.md`](design/bridge.md) §7). What would remove that trust upstream:
a state commitment in LEZ blocks, metering a proof can check, and ledger roots in Logos Blockchain headers
([`bridge.md`](design/bridge.md) §21).

### 3.2 Validity, under either rule

A burn root the vault accepts comes from valid LEZ blocks provided ([`bridge.md`](design/bridge.md) §11.4):

- **V1** fewer than `t` keys of every key set the checkpoint program has used are an attacker's, at any
  time, including keys that have since left the committee;
- **V2** an honest sequencer extends only blocks its own node re-executed, and execution is deterministic;
- **V3** the proof system is sound.

`t = t(n) = max(1, min(⌈2n/3⌉, n − 1))`, LEZ's own threshold: `n` = 3, 4, 7, 10 give `t` = 2, 3, 5, 7
([`bridge.md`](design/bridge.md) §8.2, §11.9). The program follows key-set changes in order, each signed by
the current set's threshold, and refuses to advance below the deployment's floors (`nMin ≥ 3`, `tMin ≥ 2`
outside testnets). **Retired keys stay counted** until a finalised checkpoint carries the rotation, while
their stake is released after `exit_delay` (about 40 min on testnet values), well before the wait: V1
covers them by its wording ([`bridge.md`](design/bridge.md) §8.5, §11.9). Measured: `t` keys can sign any
history (200 of 200 simulated runs); with the producer's inscription checked, an invalid tip needs
`t + 1` keys ([`bridge.md`](design/bridge.md) §11.2, §11.8).

### 3.3 Finality under rule (B), the proposed rule

Each sequencer runs `ferrata_attestor`, which signs its node's final tip with an attestation key that its
channel key certified once for this deployment (the certificate binds chain, vault and configuration). The
proof needs `t` such signatures over the tip or its descendants ([`bridge.md`](design/bridge.md) §8.3), and
checks the tip's own inscription as rule (A) does: a channel key of the live set must have inscribed it.
Beyond V1–V3, finality then needs only **no Logos Blockchain reorganisation deeper than `k` on the
attestors' nodes**, which every LEZ full node assumes. It is beaten by `t` attestation keys together with
one channel key of the live set (to inscribe a forged tip; `t` attestation keys alone no longer suffice),
or by eclipsing enough attestors' nodes for as long as it takes to show them `k` private blocks. It tolerates `n − t` silent sequencers, needs no beacon,
and works on any EVM chain; off-chain roles are liveness-only ([`bridge.md`](design/bridge.md) §9).
**Status:** the rule is built and measured in the checkpoint program and the vault, and one real proof was
verified on a rule (B) vault ([`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md) §4;
[`vault/README.md`](../vault/README.md)), of the guest before the tip-inscription check (that check is
built, tested and measured, §4a there, not proven for real); the attestor as a Logos module is **not built**, and whether the
zone's sequencers will run it is the decision the proposal asks of Logos ([`bridge.md`](design/bridge.md) §9).

### 3.4 Finality under rule (A), the fallback

Support is inferred: the tip's own inscription is checked, later blocks carry inscriptions by `t` keys
other than its producer's, and the proof is made twice, the second time over blocks that contain a random
value the vault recorded after a wait `W` ([`bridge.md`](design/bridge.md) §8.4, §10). Beyond V1–V3 it
needs ([`bridge.md`](design/bridge.md) §11.4; proof: `docs/design/finality-rule/LEMMA.md` §4, Theorem F):

- **F1** on every honest sequencer's Logos Blockchain node, no reorganisation deeper than `k`, and at
  least `k` blocks in every window Δ_g (no eclipse, lag or slowdown);
- **F2** honest blocks reach the canonical chain within `(t − 1)` turns plus a stall `S`: at most `t − 1`
  adversarial keys in the channel's **live** set, the live turn length, honest sequencers up except for
  stalls of at most `S`, their inscriptions not censored or eclipsed;
- **F3** nobody learns a beacon value more than ε before the Ethereum block that records it;
- **F4** `W ≥ Δ_g + (t − 1) × turn + S + ε`, enforced by the vault: 3 h 16 min for three sequencers,
  4 h 31 min for ten, at the default floor of 3 h 01 min.

**Measured in simulation** (Python model of the v0.4 code, `docs/design/finality-rule/RESULTS.md`;
[`bridge.md`](design/bridge.md) §11.3, §11.7): the previous version of the rule finalised a block LEZ later
dropped in 380 of 400 runs with `t − 1` keys, and in 150 of 150 with **no** corrupted key; the repaired
rule showed 0 violations in 500 turn-hogging runs for committees of 3 to 16; of 750 runs of the earlier
attacks against it, 724 succeeded without the watcher's cancellation and none with it. Key rotation is
argued, not simulated; a Logos Blockchain adversary racing honest leaders with stake is not modelled.

**Outside F1–F4 the watchers are trusted for safety.** A never-final block can be finalised with fewer than
`t` keys, even none, if F2 fails for longer than `S` and the attacker can twice show two honest sequencer
nodes a short private fork; two honest nodes eclipsed for about `W` defeat the wait whatever the other
sequencers do. The watcher's tip check stops all of these, provided its own node is not eclipsed, it is
live, and it cancels ([`bridge.md`](design/bridge.md) §11.8). Rule (A) also needs `t + 1` live keys, so one
silent sequencer stalls releases for `n ≤ 5`; it needs an unpredictable `prevrandao`
([`bridge.md`](design/bridge.md) §9, §11.9).

### 3.5 What bounds an outbound loss

([`bridge.md`](design/bridge.md) §10, §11.8, §13; [`vault/README.md`](../vault/README.md))

1. **The public wait.** A candidate's tip is public for the whole wait (1 h under (B), `W` under (A))
   before it can finalise; any of up to three watchers cancels it if the tip is not final in its own
   nodes (rule (A): also if the key set lags the live one). A cancelled state waits again in full.
2. **The release delay.** Once a root is final, releases against it wait the token's delay: at least
   24 h outside testnets, at most the 7-day revocation window. In it the freeze authority can
   `revokeFrom` the checkpoint and every later one; used nullifiers stay used. The release nullifier is
   D13's `NOut = H("ferrata/nf-out/v2" ‖ sOut)`, which does not depend on the burn's index, so a burn
   that a revoked checkpoint collected at one index and the canonical branch at another is released
   once (release guest since 11 November 2026; security review M-3: the guest had kept SPEC-v2's
   index-dependent `NOut`, under which such a burn could be released twice).
3. **Caps.** Release buckets per token and globally (refunds draw on them too).
4. **The halt.** A wrong checkpoint not revoked within the revocation window halts the vault for good:
   every later candidate descends from it, so no honest checkpoint can follow.

No bridge signer, relayer or prover is trusted, and a prover cannot leave out a visible burn
([`bridge.md`](design/bridge.md) §8.2, §14).

## 4. Roles and powers (RFP-022 Security #5, #7)

From [`vault/README.md`](../vault/README.md), "Roles"; [`bridge.md`](design/bridge.md) §14;
[`lez/BRIDGE.md`](../lez/BRIDGE.md) §5:

| Role | Can | Cannot | If its key is lost | If its key is stolen |
|---|---|---|---|---|
| **Admin** (multisig) | register a token once; enable or disable deposits per token; set caps above the immutable floor; set release delays within `[minReleaseDelay, revokeWindow]` for later roots; raise Δ; propose watchers and a new freeze authority, applied 14 days later; announce fee schedules (in force 7 days later, ≤ 1% each) | move escrow; change a registered token; lower Δ; cut a cap below the floor; freeze, unfreeze, cancel, revoke, retire; change any immutable (verifier, `mode`, image ids, `lezConfigId`, timers, floors) | no new tokens or caps; everything else keeps working | registry and cap churn within bounds; a public 14-day window before a role change |
| **Freeze authority** (multisig) | freeze deposits, releases, checkpoints (vault) and mints, burns (LEZ), globally or per token, each for at most `maxFreeze` (30 days; on LEZ, of attested Ethereum time, so a LEZ freeze holds while the light client stalls) unless renewed; `cancelCandidate`; `revokeFrom` within the window; `retire()`; LEZ `Sunset` | touch the registry, caps, delays, Δ or escrow; accept anything; undo a stop or a retirement; hand itself over | freezes lapse; anyone retires after 30 days without checkpoint progress (time under a freeze does not count); the admin replaces it in 14 days | **denial of service, not theft**: renew freezes, revoke honest checkpoints within the window, retire the vault |
| **Watchers** (up to three hot keys) | cancel a pending candidate; pause checkpoints for at most 12 h | anything else | — (others watch) | delay finalisation until the admin replaces them |
| **Emergency stop** (RISC Zero's `RiscZeroVerifierEmergencyStop`) | stop every proof-gated call for good | be tripped by anyone holding a key: its owner is renounced at deployment, so only `estop(receipt)`, RISC Zero's proof of unsoundness, trips it | — | — |
| **Anyone** | deposit; submit and finalise checkpoints with proofs; record beacons; release with a proof; refund once allowed; apply a role proposal after 14 days; retire after 30 days without checkpoint progress (no submission or finalisation; freezes and pauses do not count), or once a sunset checkpoint can no longer be revoked | — | — | — |

On LEZ, the roles are accounts fixed in the configuration; the 14-day replacement, watchers and the
permissionless `Sunset` are **not built** there, and neither is a multisig freeze authority
([`lez/BRIDGE.md`](../lez/BRIDGE.md) §4 item 9; [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) §9).

**Off-chain participants of the bridge** (after [`bridge.md`](design/bridge.md) §6, §12.3, §14):

| Who | Provides | If it lies | If it stops | Learns |
|---|---|---|---|---|
| Root importer | one attestation per record | refused by the primitive | anyone else imports | public data |
| Relayer | gas drops (not built), submission | cannot redirect or overcharge: proofs bind destination, recipient and fee | the user picks another or submits directly | a time, a one-time address, the transaction it submits |
| Checkpoint prover | candidate and finalisation proofs | refused by the vault | anyone with a prover and an x86 machine; a backlog is proven in pieces | public data |
| Release wrapper | the Groth16 wrap | refused by the vault | another wrapper, or the user's own x86 machine | what the release publishes |
| Accredited sequencers | LEZ blocks; attestations under (B) | `t` of them can forge a burn root (§3.5 bounds it) | the zone stops; finalisation needs `t` live keys under (B), `t + 1` under (A) | public transactions |
| RPC and LEZ nodes | chain data | refused: checked against roots and records | another node | request metadata; recovery downloads whole lists, never per item |

Under rule (B) every off-chain role is liveness-only (Security #7); under rule (A) the watchers have the
safety role of §3.4.

## 5. The proof system

- **RISC Zero zkVM 3.0.5, Groth16 via risc0-ethereum v3.0.0.** The vault pins RISC Zero's base Groth16
  verifier behind its unmodified emergency-stop wrapper, never the router, whose admin can add verifiers
  ([`bridge.md`](design/bridge.md) §15, §18; [`vault/GAS.md`](../vault/GAS.md)). Upstream BLS (`blst`, NCC
  Group audit 2021) and RISC Zero's own components are reused, not reimplemented (R.8;
  [`attestation.md`](design/attestation.md) §14).
- **Zero knowledge of succinct receipts.** RISC Zero targets it without a written proof; every LEZ private
  transaction rests on the same property. Wrappers receive only succinct receipts, and the release guest's
  cycle count is the same for 64 random witnesses (a gate that caught a first version leaking the popcount
  of the burn index) ([`bridge.md`](design/bridge.md) §12.2–§12.3).
- **LEZ's privacy-preserving circuit** gates private claims, private burns, private reads and the proven
  update path; its soundness is LEZ's, not Ferrata's.
- **If the proof system is unsound** and RISC Zero publishes the proof, anyone can trip the stop: nothing
  more is released or refunded, owed balances stay claimable, and whatever escrow remains stays in the
  vault (vault v5; [`vault/README.md`](../vault/README.md), "Verifier and stop", "Refunds"). Before the stop
  trips, an unsound proof system breaks everything a proof gates; caps and freezes bound it.
- **Image ids are reproducible only at a fixed, published build path** (a guest embeds its build paths): the
  crown run's six programs rebuild to the deployed ids at the crown build path with the pinned toolchains
  (`scripts/reproduce-image-ids.sh`, [`guides/verify.md`](guides/verify.md): 6 of 6 on the build box, the programs
  byte-identical), and another path gives other ids. An immutable production deployment needs a build independent of
  the path ([`bridge.md`](design/bridge.md) §20; [`lez/live/inbound/README.md`](../lez/live/inbound/README.md)).

## 6. Failure classes, what is lost, what bounds it (Security #9)

There is **no escrow-moving failsafe**: an escrow-moving recovery would need a party to choose new
verification logic, which Security #8 forbids. The failsafe is stop-only
([`bridge.md`](design/bridge.md) §15).

| Failure | What happens | Positions lost | Bound |
|---|---|---|---|
| Sync committee captured (inbound) | forged records; invented deposits imported and minted | wrapped supply above escrow; the shortfall falls on late redeemers | LEZ mint caps until the freeze authority freezes mints |
| `t` sequencer keys (outbound) | a forged burn root; releases of tokens never burned | escrow paid out against forged burns | wait + watchers; release delay with `revokeFrom`; release caps |
| Rule (A) outside F1–F4, watchers down or eclipsed | a released burn later dropped on LEZ (double spend) | as above | the same |
| Proof system unsound | anything a proof gates, until the stop trips | after the stop: every unreleased burn and unrefunded deposit stays in the vault | caps and freezes before; the stop after |
| A bug in Ferrata's guests | depends on the bug | depends | release freeze and `revokeFrom` ([`bridge.md`](design/bridge.md) §14) |
| A bug in the vault | depends; no upgrade | depends | freezes, retirement; caps |
| A wrong checkpoint not revoked within 7 days | the vault halts for good | every position not yet released | caps bound what the error itself released |
| A LEZ change the configuration cannot follow | retirement before it; unimported deposits refunded | tokens never burned before the change stay backed in the vault but cannot be released | notice period asked of Logos (M0) |
| Admin or freeze key compromised | denial of service within the bounds of §4 | none directly | lapsing freezes, 14-day public replacement, keyless retirement |
| Every operator stops | no imports, checkpoints or relays | none: anyone can resume; claims never expire | keyless retirement and refunds after 30 days without checkpoint progress, from deployment on (operators keep a heartbeat checkpoint) |
| A recipient that can never receive | its `owed` balance stays forever | that payee's amount | pull payments isolate it from everyone else |

Sources: [`bridge.md`](design/bridge.md) §13 ("Supply"), §15, §16, §20; [`vault/README.md`](../vault/README.md).
**Supply (Security #6):** wrapped supply never exceeds escrow under the stated trust, because supply
changes by exactly one denomination at each mint and burn; a breach creates a shortfall of at most the
caps before a freeze ([`bridge.md`](design/bridge.md) §13).

## 7. Who sends each transition, and why

Nothing runs itself; every state change is a transaction someone pays for
(ethskills `concepts`).

| Transition | Who | Why they would | Built |
|---|---|---|---|
| light-client update | anyone running the updater | they need fresh records (consumers, the bridge's importers); the bridge can pay a fixed rebate from its protocol fee (soft #5) | updates yes; the rebate is design only |
| `ImportRoot` | anyone, as a top-level call or private root (the bridge refuses a nested or asynchronous import, which the checkpoint could not count; security review H-1) | depositors want to claim; relayers earn relay fees on claims | yes |
| `submitCandidate`, `finalize` / `finalizeAttested` | anyone with a prover | the **checkpoint fee** of every release against that root goes to the candidate's prover, the sender of `submitCandidate` (not of the finalisation, which under rule (B) needs no proof) | yes (vault v5; prover since the security fixes of 11 November) |
| `recordBeacon` (rule (A)) | anyone, typically the finaliser | needed for its finalisation | yes |
| relaying claims and releases | relayers | the registered relay fee, bound by the proof (a plaintext claim's relayer is a free field: whoever gets a copy of it included first takes its relay fee, at most 1% of the denomination; the private claim binds the relayer inside the proof; security review L-4, not changed) | fee yes; relayer service no |
| `cancelCandidate`, `pauseCheckpoints` | watchers | appointed by the admin; no fee | vault side yes; `monitor` no |
| `refund` after retirement | anyone, typically the depositor | it pays the depositor named in the leaf | yes |
| `retire()` after 30 days without checkpoint progress, or after an irrevocable sunset checkpoint | anyone | to open refunds | yes (vault); LEZ `Sunset` by the freeze authority only |

Whether the fees cover the callers' gas at the deployer's chosen levels is **not measured**; the fee
levels are the deployer's choice ([`deployer-guide.md`](guides/deployer-guide.md) §2).

## 8. What an observer can still learn (Privacy #1–#7)

What each step publishes is tabulated in [`user-guide.md`](guides/user-guide.md) §10, with the measured
transaction dumps of [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) §3 and
[`lez/RESULTS.md`](../lez/RESULTS.md) §5. **Residual correlations**, none hidden by the defaults
([`bridge.md`](design/bridge.md) §17; [`lez/BRIDGE.md`](../lez/BRIDGE.md), "What each path makes public"):

| Correlation | What it reveals | Mitigation |
|---|---|---|
| amount and token | narrows a claim to deposits of that token and denomination | fixed denominations, at most eight per token, default three |
| timing | deposit-to-claim and burn-to-release intervals | the client proposes a delay (not built) |
| the root a claim reads | the deposit is at least that old | read the newest root |
| a retry after a cap rejection | the first attempt's time | the client checks caps before proving |
| a fee-schedule change | splits sets by date: the claim's fee range shows which schedule applied | schedules change rarely, with 7 days' notice |
| the fee payer of a private transaction | the payer appears as a fresh alias with its reserve in plaintext; a first claim funded from a faucet or a purchase links to that source until the gas drop exists | fixed reserve per operation type; gas drop (not built) |
| a private read of a record | which record and which consumer program | read the newest record; hop inside the proof |
| the plaintext claim | which deposit paid which mint | private by default; the client warns before allowing it |
| network metadata | IP addresses to RPC endpoints and sequencers | out of scope; editable endpoints (U.9, not built) |
| an adversary's own deposits | subtracted from any anonymity set | none |

**Who handles user data (Privacy #4).** Only the wallet holds the seed, `s`, `C`'s opening, destinations,
`sOut` and recipients; none of them is in any submitted transaction (searched byte by byte in the measured
dumps). A relayer learns a time, a one-time address and what it submits; a wrapper what the release
publishes; RPC providers which blocks are fetched (recovery downloads whole lists, never per item). The
`kView` key reveals burns and recipients but cannot claim or release ([`bridge.md`](design/bridge.md) §4, §6).

## 9. CROPS review

(Format of ethskills `crops`; applied to this design.)

- **Censorship resistance.** Risk: the zone's sequencers can exclude transactions; a freeze authority can
  pause flows for up to 30 days at a time; a single relayer could refuse users. Mitigation: every role
  except watchers and the multisigs is permissionless; freezes lapse; claims never expire. User escape:
  submit directly; pick another relayer per operation; refunds after retirement, which anyone can trigger
  after 30 days without a checkpoint.
- **Open.** Risk: guest image ids are not reproducible yet; the daemons and modules do not exist yet.
  Mitigation: the whole stack is in this repository; the deployment is scripted and rehearsed. User
  escape: run your own updater, importer, prover and wrapper from the repository.
- **Free.** The repository is licensed MIT OR Apache-2.0, at the user's option ([`README.md`](../README.md); [`LICENSE-APACHE`](../LICENSE-APACHE),
  [`LICENSE-MIT`](../LICENSE-MIT)); the licences of reused upstream components are not recorded here.
- **Privacy.** Risk: §8's residual correlations. Mitigation: commitments and nullifiers, fixed
  denominations, private execution by default, relayers. User escape: the defaults; switching relayers per
  operation.
- **Security.** Risk: §6. Mitigation: immutability, least authority (no role can move escrow), multisigs
  and a 14-day public timelock on role changes, caps, a stop-only failsafe. **Walkaway test:** if every
  operator and the team disappear, anyone can still import, prove, finalise, release, retire and refund
  from the repository; the vault needs nobody's key.
- **Accepted compromises:** trust in two thirds of each sync committee (inbound) and in fewer than `t`
  accredited sequencer keys (outbound), both forced by what Ethereum's light-client protocol and LEZ v0.4
  expose; the halt after an unrevoked wrong checkpoint; tokens never burned before a breaking LEZ change.

## 10. Not built, and therefore not yet part of the guarantee

`verify_checkpoint`; the weak-subjectivity bound; successors and root ids; `monitor`; the rule (B)
attestor module; LEZ-side watchers, multisig roles, 14-day replacement and permissionless `Sunset`; the
gas drop; reproducible guest builds; the audit. Sources: [`attestation.md`](design/attestation.md) §15;
[`bridge.md`](design/bridge.md) §19; [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) §9.
