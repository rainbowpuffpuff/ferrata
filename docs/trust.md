# Ferrata: trust assumptions

**The trust document of RFP-021 (S.6, F.12, VS.4, VS.5) and RFP-022 (Security #1–#9, R.9, Privacy #4,
#6). Revised for delivery on 21 November 2026, against the tree at this commit; it replaces the first draft of
11 November.** It says who is trusted for what, what each failure does, what
bounds the loss, and what an observer can still learn. It covers the light client and its
attestations (inbound) and the return leg from LEZ to Ethereum (outbound). Every statement cites its
source; paths are relative to the repository root. The code is a prototype, **not audited and not
for production** ([`README.md`](../README.md), "Not audited"): an audit of the consensus and inclusion
logic, the vault, the bridge program and the proof programs is planned and not done.

Labels as in the design notes: **measured**, **derived**, **estimated**, **not built**
([`docs/design/attestation.md`](design/attestation.md), [`docs/design/bridge.md`](design/bridge.md),
opening sections).

## 0. The verification model in one paragraph (RFP-021 VS.5, RFP-022 Security #7)

Nothing in Ferrata takes a party's word for a fact. An attestation is accepted because the LEZ program itself
verifies Ethereum's sync-committee signatures, from a checkpoint the consumer pinned, and the Merkle proofs under
the record it wrote; a claim, because the bridge program proves the deposit under a root the attestation program
attested; a release, because the vault verifies a zero-knowledge proof of the burn under a checkpoint it accepted.
Every off-chain participant (updaters, provers, root importers, relayers, wrappers, RPC providers) only carries data
that these checks accept or refuse: if it lies it is refused, and if it stops anyone else, the user included, can
take its place. Its role is **liveness only**. Four assumptions remain, stated rather than hidden: the starting
checkpoint, until it is checked (§2.1); two thirds of each sync committee, for the inbound leg (§2.2); fewer than `t` of the zone's accredited sequencers, for the return leg: their keys, or the LEZ and Logos Blockchain nodes their attestors read (each attestor must use its operator's own), where under rule (A) the watchers also hold a safety
role outside its conditions (§3); and the soundness of the proofs the system accepts: the vault's verifier, and LEZ's
check of privacy-preserving transactions, on which every Ferrata program's public state rests, an instance's
included, because LEZ applies a private transaction's calls to public actors on the strength of its proof alone (§5;
RISC Zero 3.0.x is under advisory: the [security note](security/risc0-groth16-advisory-2026-11-21/README.md)).

**Where each clause is answered.**

| RFP clause | Section |
|---|---|
| RFP-021 S.6: the honest supermajority and the cost of the attack it admits | §2.2 |
| RFP-021 S.6: the weak-subjectivity checkpoint, who supplies it, how a verifier confirms it | §2.1 |
| RFP-021 S.6: what an attestation does and does not prove | §2.3 |
| RFP-021 S.6: what every off-chain participant can observe, and whether it affects safety or only liveness | §2.4 (light client), §4.2 (bridge) |
| RFP-021 S.6: when the guarantees degrade or fail | §2.5, §6 |
| RFP-021 F.12: every off-chain participant, and why none of them, alone or as a class, blocks an attestation | §2.4, each row walked by a test |
| RFP-021 VS.4: a compromised committee, and how a consumer bounds its exposure | §2.2 |
| RFP-021 VS.5, RFP-022 Security #7: the trustless model and liveness-only roles | §0, §2.4, §4.2 |
| RFP-022 R.9: outages and malfunctions of third-party components, their effect and mitigation | §4.3 |
| RFP-022 Security #8, #9: no swap of verification logic; the failsafe | §6; the [deployer guide](guides/deployer-guide.md) §9 |
| RFP-022 Privacy #4: every component that handles user data | §8.1 |
| RFP-022 Privacy #6: what a party submitting for the user learns, and switching per operation | §8.2 |

## 1. Summary

| You trust | For | If it fails | What bounds the loss | Section |
|---|---|---|---|---|
| The starting checkpoint of an attestation instance | that the instance follows the real Ethereum | a false chain from the start | anyone can compare the checkpoint root with public Ethereum before relying on the instance | §2.1 |
| The vault's deployment parameters: `genesisState` (the starting key set, turn length and tip), `lezConfigId`, the image ids, `turnSeconds`, `nMin`/`tMin` | that the vault follows the real LEZ channel from the right state with the right programs | a false LEZ history or program accepted from the start | anyone can check them before depositing: the channel's public state, `configId`'s preimage (it binds the vault's address), reproducible builds of the guests; the deployment script checks none of them and accepts `nMin = tMin = 1` (testnets only: it refuses every chain but anvil, Sepolia, Hoodi and Holesky) | §3.2, §4 |
| Two thirds of each 512-member sync committee | that every recorded header was finalized | forged records: facts about blocks that never existed are attested; the signers lose no stake (none of it is slashable for this) | consumer caps and freezes; the attestation side's monitor's alarms (§2.2); an instance freeze on evidence of conflicting finality (experimental, the v0.3.1 port only, not deployed) | §2.2 |
| Fewer than `t` of the LEZ channel's accredited keys are an attacker's, in every key set the checkpoint program used | validity of every burn root the vault accepts (both rules) | `t` keys can forge a burn root | the public wait, watchers, the release delay and revocation, release caps | §3.2 |
| Logos Blockchain: no reorganisation deeper than `k` on the attestors' nodes | finality of accepted burn roots under rule (B) | a released burn is later undone on LEZ | the same | §3.3 |
| Rule (A)'s conditions F1–F4, or else a live, uneclipsed watcher | finality under rule (A) | as above, with fewer than `t` keys or none | the same; outside F1–F4 the watchers are trusted for safety | §3.4 |
| RISC Zero's proof system (and LEZ's privacy circuit) | that every accepted proof is true | anything a proof gates | caps and freezes; the emergency stop once a proof of unsoundness for the all-zero claim is published, which did not happen under RISC Zero's advisory of 11 November 2026 (§5) | §5 |
| Ferrata's own code (core, programs, guests, vault) | everything | depends on the bug | caps, freezes, `revokeFrom`; immutability means no fix in place | §6 |
| Admin and freeze authority (multisigs) | liveness only: they cannot move escrow or accept anything | denial of service | freezes lapse after `maxFreeze` (immutable: 30 days by default on the vault, a 30-day constant on LEZ); keyless retirement; 14-day public role changes | §4 |
| Until milestone B1: nobody who knows a public-kind deposit's `s` creates a second configuration of the same vault and chain, and nobody fills a destination's `holder(dest)` | that every public-kind deposit can be claimed under the paired configuration, and that a public destination can receive | that deposit is frozen: the paired configuration's claim is refused (250) and, once it has imported the deposit, the leaf is below the refund boundary, so its escrow stays in the vault; a filled destination can no longer be credited | no theft and no double mint: the second configuration's mints and burns are under its own id, which no vault pays; only the depositor or whoever sees a plaintext claim before inclusion knows `s` (a private claim, the CLI's default path, does not publish it; a private-kind deposit cannot be taken); the fix is planned for B1 ([`lez/results/tenant.md`](../lez/results/tenant.md)) | [`bridge.md`](design/bridge.md) §3 |
| Everyone else (submitters, importers, provers, relayers, wrappers, RPC providers) | liveness only | refused, or replaced by anyone else | — | §2.4, §4.2 |

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
to the execution chain id). `verify_checkpoint`, a method of `ferrata_attest_module`, recomputes the instance id, runs the
program's `Init` checks and the full `KeyValidate` on the bootstrap, compares the roots it is given, and asks
the beacon providers named in the call itself: at least three hosts must hold the root as the canonical,
finalized block at its slot and serve the instance's genesis and spec (their `DEPOSIT_CHAIN_ID` ties the chain id
to `genesis_validators_root`), and none may disagree; an execution endpoint is asked for the checkpoint's execution
block ([EVIDENCE](EVIDENCE.md) rows 59, 105, [`modules/README.md`](../modules/README.md),
[`modules/results/verify-checkpoint-2026-11-13/`](../modules/results/verify-checkpoint-2026-11-13/README.md)). The
endpoint meant is `verified_proxy_module`: on Sepolia the execution check ran through Logos Core's `eth_rpc_module` with
the chain in `required` mode, answered by that module's light client
([`modules/results/verified-proxy-2026-11-14/`](../modules/results/verified-proxy-2026-11-14/README.md)); the beacon providers are still read over plain
HTTP by the attestation module itself, and the pull request that offers the check to that module is prepared, not opened
([`attestation.md`](design/attestation.md) §11.4; [`lez/RESULTS.md`](../lez/RESULTS.md) §5, "Decisions" 1).

**Weak subjectivity.** An instance that falls outside the weak-subjectivity period must be replaced from
a fresh checkpoint under a new root id: a new trust decision. In the design the bound is fixed per
instance (about two weeks on mainnet) and an update too far past the finalized slot is refused, with no
clock. **Built** ([EVIDENCE](EVIDENCE.md) rows 69, 70): the core implements consensus-specs'
`compute_weak_subjectivity_period` and `is_within_weak_subjectivity_period` against the store's finalized
header, a store past its period refuses every update (53) and is recovered only by a new bootstrap, and
the program applies the bound by the sequencer's clock. The alarm before an instance runs out is the
attestation side's monitor's (`attest-monitor`: `ws-half` past half the period, `ws-expired` past it; §2.2,
[`attestation.md`](design/attestation.md) §3.6, §4.6).

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

**What a forger loses** (measured on mainnet on 18 November 2026, the rules from the consensus specifications v1.6.1
and EIP-7657, fetched read-only that day; method, sources and the re-check: [`forgery-cost/`](forgery-cost/README.md)).
A forged finality update needs 342 of the 512 seats: Ferrata's step 1 refuses `participants * 3 < 512 * 2`
([`steps.rs`](../crates/ferrata-core/src/steps.rs), the specifications' supermajority; 342 × 3 = 1,026 ≥ 1,024, 341 × 3 =
1,023). What stands behind those signatures, and what the signers lose:

| | The sync committee, as Ferrata follows it | With EIP-7657 (sync-committee slashing) | Casper FFG over the whole validator set |
|---|---|---|---|
| Signatures a forgery needs | 342 of 512 seats | the same | two thirds of the active stake: 29,562,407 ETH (derived) |
| The stake behind them (measured) | the current committee (period 1914): 512 members, 409,755 ETH; its cheapest 342 seats 76,714 ETH (period 1915's: 390,013 and 58,397 ETH) | the same | the active stake, 44,343,611 ETH |
| Slashable for a forgery | **nothing**: the specifications slash a proposer (two blocks for one slot) and an attester (FFG double and surround votes); a block carries `proposer_slashings` and `attester_slashings` and no sync-committee evidence (also in Gloas), so a member signs the canonical block for its reward and the forgery beside it | the 342 signers, those not yet withdrawable (`is_slashable_validator`; one member of the current committee already is, with a 32 ETH seat) | at least a third of the active stake, 14,781,204 ETH (derived), whenever Ethereum finalizes the conflicting canonical checkpoint (accountable safety: two conflicting finalized checkpoints mean a third double- or surround-voted) |
| Lost by the forger (derived, Electra's penalties) | **0** | about 536 ETH for the cheapest 342 seats: 417 in slashing penalties (an immediate 1/4,096 of the balance plus the correlation penalty, three times the share of the stake slashed together: 0.52 % of 76,714 ETH) and 119 in the source and target penalties a slashed validator pays every epoch until it is withdrawable (8,192 epochs, about 36 days, at 40/64 of a base reward of 303 Gwei per ETH); about 12,095 ETH if all 512 are slashed together (11,459 + 636). An approximation over the totals: effective balances held at today's, no inactivity leak | everything: from a third slashed together the correlation penalty is the whole effective balance, at least 14,781,204 ETH |
| Status | live since Altair | EIP-7657 is **Stagnant** (created 21 March 2024); its own text: use cases securing more than 512 × 32 = 16,384 ETH should combine the light client with other methods | Ethereum's own finality; nothing of Ferrata verifies it |

The "342 × 32 = 10,944 ETH" sometimes quoted is a nominal floor: since Electra seats are drawn in proportion to
effective balance up to 2,048 ETH, so the current committee's 512 members are 276 of at most 32 ETH, 32 of 33–1,023, 161 of
1,024–2,047 and 43 of 2,048, and its 342 cheapest seats hold 76,714 ETH, none of which a forgery puts at risk. Four
of its members are exiting, two have exited and one is withdrawable; their seats still sign. The active stake is Lighthouse's count for
epoch 490,090 (publicnode's node); the sync reward per seat in the same block (25,627 Gwei) agrees through the reward
formula. *Measured*: balances, counts, statuses; *derived*: the thirds and the losses.

**What this means for the bridge.** A forgery costs bribes with no floor and no stake, so the bridge cannot price it;
it bounds what one can take before someone freezes, as row 138's invariants measure under a stronger attacker:

- **In:** a forged record's invented deposits mint wrapped tokens only up to LEZ's mint caps (per token and global
  buckets, refilled on attested Ethereum time, which a captured committee alone does not advance: above), until the
  freeze authority freezes mints.
- **Out, the escrow:** the unbacked tokens can be burned on LEZ, and the burns are real, so their checkpoint is
  honest: what protects the escrow is the vault's release caps (per token and global; the design's default at most
  10 % of a token's escrow a day above an immutable floor, [`bridge.md`](design/bridge.md) §13) and the release delay
  (at least `minReleaseDelay`, at most `revokeWindow`: immutable deployment parameters, 24 h and 7 days by the
  design's defaults), within which the freeze authority freezes releases or
  revokes the root. [EVIDENCE](EVIDENCE.md) row 138 ran the vault under a verifier that accepts every seal (any
  checkpoint, any release, at the earliest second allowed): releases and refunds stayed within the cap bound in every
  window (the fullest 74.00 % of the ETH bucket's bound under rule (A), 90.09 % under rule (B)) and none came before the
  delay; the uncapped USDT lost up to 94.4 % (rule (A)) and 99.4 % (rule (B)) of a run's deposits, and with the
  deployment script's testnet defaults of the
  live runs of November 2026 (caps UNLIMITED; since milestone B1 only with `FERRATA_UNCAPPED=1`, the script's default
  being capped: `vault/test/DeployDefaults.t.sol`) only the clocks and the emergency stop remain.
- **Sizing:** a production deployment's caps × (the time to notice and freeze, at least the release delay) is what
  a captured committee can take; set it to what the deployment accepts to lose. The live round trips of rows 41 and 91
  ran with no caps and a 120 s delay (testnet values).
- **The updater's client check does not change this** (§2.4): the program accepts an update from anyone on its
  signatures (VS.6), so a forger submits it; an honest updater with `min_distinct_clients` only refuses to pay for and
  relay one, and names the disagreeing sources in its `/health`.

**Options, design only (not built).** (1) *Bonded backing, a consumer's choice:* a consumer (the bridge's
configuration, or any other) accepts a record only when a bonded set co-signs its finalized header as canonical, the
bond (an Ethereum contract, or restaked collateral) slashed on Ethereum on proof that a co-signed header is not the
canonical block at its slot (EIP-4788 gives contracts the beacon block roots); a forgery then costs at least the bond,
which the consumer sizes to its caps over the reaction window. It adds a bonded, trusted-for-cost party and a
challenge path; nothing of it exists. (2) *EIP-7657, if Ethereum adopts it:* the signers' stake becomes slashable, but
by the table about 536 ETH for today's cheapest supermajority, because the correlation penalty is small when few are
slashed (and the source and target penalties until withdrawable add about 119 ETH); Ferrata can only use it, not build
it. (The instance freeze on evidence of conflicting finality, below, stops an instance but takes nothing from the
signers either.) (3) *FFG-based anchoring (future work):* follow Ethereum's own
finality, two thirds of the active stake's attestations, beside or instead of the sync committee, so a forgery means a
third of the stake slashed (about 14.8 million ETH today); the guest would check the whole validator set's
attestations instead of 512 keys; not designed in detail, not measured.

**What a captured committee can do (VS.4).** Sign a forged handoff and hold the instance until someone
notices; sign finality updates for blocks that were never finalized, whose records then attest anything
the forger put in their state. For the bridge, a forged record whose vault slot 0 holds invented deposits
lets `ImportRoot` accept them and claims mint wrapped tokens with no escrow behind them. **What bounds it:**
the bridge's mint caps (per token and global, on LEZ) until the freeze authority freezes mints; wrapped
supply then exceeds escrow by at most what the caps let through ([`bridge.md`](design/bridge.md) §13,
"Supply"). Those caps and freezes run on attested Ethereum time, the execution timestamp of a record,
and a record moves a clock only in a LEZ block stamped at or after it: a captured committee's
far-future record refills no bucket and ends no freeze unless a block producer also stamps its block
that far ahead ([`lez/BRIDGE.md`](../lez/BRIDGE.md) §6). The attestation side's monitor (`monitor/attest`,
`attest-monitor`; [EVIDENCE](EVIDENCE.md) row 193) compares the instance's finalized header with several beacon providers
and raises two critical alarms that a consumer wires (`--alert-cmd`, level critical) to its freeze authority:
`instance-diverged` when a provider (by default any one; `--diverge-at N` for N) holds another block at that slot, final
for it or not yet, or no block at a slot it has finalized, whatever the others say (a provider that agrees with a forged
header may be colluding with the forger); and `instance-ahead` when the header is younger than the chain's finality
delay (two epochs), which no honest instance can hold, or when no provider has finalized it or holds it although it is
older than that delay plus a grace. A consumer runs it with three or more providers of different clients, operated by
different parties. One dishonest provider can then raise a false alarm, which costs a pause, but cannot hide a forged
header; only every provider counted together can. A forged header the providers cannot judge yet (ahead of their
finality, at a slot where they hold no block) is judged again once they finalize its slot, or come back: at most three
epochs after it on a chain that finalizes normally (about 19 minutes on Ethereum; an epoch more if every slot from it to
the next epoch boundary is empty), at the next poll unless more than eight headers become judgeable at once (then within
⌈n/8⌉ polls, with a warning). The two alarms clear only when every configured provider confirms the header; the
monitor's clock is assumed right within 12 s. What can pass before the freeze is the consumer's caps over that time, plus the poll interval and
the freeze's own; size the caps to what it can afford to lose. The monitor is a prototype, tested on altered states
against loopback providers serving recorded answers, not run against public providers or a real captured committee
(the watchers' `monitor` of the return leg is another program: EVIDENCE rows 43–45; [`attestation.md`](design/attestation.md)
§11.3). A deeper inbound
finality setting would not help: it delays honest claimants, not a captured committee
([`bridge.md`](design/bridge.md) §5.4).

**What is now detected (experimental, unaudited; the deployed program does not have it).** A committee that signs
finality contradicting what an instance recorded can be caught by the instance itself, in the LEZ v0.3.1 port
(`lez-v03/`, deployed nowhere with this change) and the bench ([`bench/MISBEHAVIOUR.md`](../bench/MISBEHAVIOUR.md)):
anyone may send `Evidence`, one update valid under a committee the instance holds (two thirds signed, the finality
branch, the policy's checks, in the window an update would have) whose finalized header has the slot of a recorded
header and another root, or whose next committee differs from the one the instance adopted for that period (both
headers of the update in that period). The instance then refuses every update, attestation and further evidence
(`FROZEN`), keeps its store and records, and records the evidence. For an instance started from a finalized
checkpoint (the assumption of §2.1, which `Init` cannot check) an honest network never produces such evidence
(`MISBEHAVIOUR.md` §3: finalized checkpoints lie on one chain unless a third of the stake is slashable, and a
period's next committee is fixed on a chain). Not detected: a conflicting header finalized at a slot the instance
holds no record of (deciding its ancestry needs a proof into a recorded state's `block_roots`, not built), and one
finalized above every record, which the instance accepts as an update (the light client checks no ancestry, as the
specification's does not). Evidence must be signed by a committee the instance holds when it lands, so it expires: a
conflict signed in period Q can freeze the instance only until the instance's first update finalized in Q+1, which
anyone may relay (and an `Update` that advances the store first makes the evidence be planned again); the
attestation side's `monitor` (A1) must submit it as soon as it sees a conflict, re-plan it when an update lands first,
and raise a conflict it can no longer submit as an alarm (`MISBEHAVIOUR.md` §5). It does not say who
misbehaved, nothing is slashed, and a consumer that reads records or statements directly must read the instance's
frozen flag ([EVIDENCE](EVIDENCE.md) row 144).

**Evidence that a non-captured committee is required (VS.1, VS.2).** A self-consistent update about a
chain that never existed is refused on the signature alone (the fabricated-chain test, consensus half,
native); participation at 21, 22, 23 and 32 of 32 is tested; an update for another chain's instance is
refused with 40; 107 hand-picked mutants are killed; Helios is a differential oracle on 33 of the 52
official cases ([`attestation.md`](design/attestation.md) §3.4, §12). Through the program, at this commit: a
forged signature is refused from any sender (VS.6), and forged state and receipts roots, another chain and a
predicate over uncovered state are refused (`lez/results/stage4-attest-refusals.md`); the fabricated chain itself,
and committee, participation and finality forgeries, are refused in the native core's tests
([`requirements-status.md`](requirements-status.md), VS.1 and VS.2, says what is shown where).

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
- freshness relative to the weak-subjectivity period, beyond what the bound enforces (a store past its
  period refuses every update; a record's own age is not bounded: §2.1).

### 2.4 Off-chain participants of the light client (RFP-021 F.12)

Every off-chain participant the attestation side requires, what it can affect, and why none of them, alone or as a
class, can stop an attestation from eventually being produced. Each row is walked by one test of
[`lez/updater/tests/every_party.rs`](../lez/updater/tests/every_party.rs), named in the last column: the party lies,
lags, stops or censors, and the test still ends with `Attest` applied on a record the instance wrote (the updater
daemon against LEZ v0.4.0-rc1's state machine in process, the mainnet fixture of 10 November 2026, dev mode). A
further test fails if a row is added here without its scenario, or the reverse.

| Who | Provides | Can affect | If it lies | If it stops | Observes | Why it cannot block an attestation | Walked by |
|---|---|---|---|---|---|---|---|
| Submitters (updater operators) | light-client updates, as the public job's transactions | liveness only | refused: the program checks every update's signatures and branches, whoever sends it (VS.6); a forged update moves no store (the test sends two with the signer witness right: one signature byte flipped, refused at `Open` with 41; the committee's genuine signature over a false header whose branches were recomputed to verify, refused at `Finish` with 40) | anyone else submits; an account with no prior relationship to the instance advances it | public chain data | submitting is permissionless and needs only LEZ gas: when every operator stops, the consumer runs the same daemon with its own key | `submitters_any_account_advances_none_is_needed` |
| Provers (the proven path only) | a proof of an update's verification (`VerifyAll`, then `ApplyVerified`) | liveness only, while LEZ's check of private-transaction receipts is sound (§5) | refused: LEZ verifies the receipt; a forged receipt LEZ accepted would write the instance's store (§5) | nobody needs one | the update, which is public | not required: the public job (`Init`, `Adopt`, `Open`, `Miller`, `Finish`) advances the instance with no proof | `provers_none_is_needed_on_the_public_job` |
| Beacon API providers | bootstraps and updates | liveness only | refused by the updater before it pays (the source is demoted), and by the program if sent anyway; a real update re-dated to another `signature_slot` of its period (the signature does not cover it) verifies, and the program refuses its `Open` until that slot: the updater refuses an update signed more than 60 s past the chain's clock and demotes its source, and ranks an update whose window has not opened by the chain's clock after every one that can be sent, so a source re-dating real updates within the 60 s (a copy, or another real update of one finalized header with more signers), even again every round, holds back only an update no other source serves ([`lez/updater/tests/refusals.rs`](../lez/updater/tests/refusals.rs)); one re-dated to a slot that has started can be chosen, a new job with each copy, which the source may open and take on a step itself (anyone may), but every step the updater sends of a job it does not carry waits a turn counted by the instance's state, one per kind of step, not by the job: such a source delays the instance, by at most 68 rounds on each state of the instance at the default `defer_window`, 68 + 85 = 153 with join or wait (ADR 0047, on by default; `lez/updater/README.md` item 8) (measured 66 in the committed test; 67 in an independent review run, not reproduced by a committed test), and does not stop it ([`lez/updater/tests/redating.rs`](../lez/updater/tests/redating.rs)); the turns start again with each new state, so a party that also applies older real updates itself (anyone may, paying their jobs) can postpone the next committee's adoption, and keep the instance behind the newest update, for as long as it has such updates to apply, the instance moving at each so that the stall alarm need not fire (measured in an independent review run, not reproduced by a committed test: adopted 475 rounds after the rotation over seven such updates with the source using up the turns on each state, 165 with no lying source, the same before the per-state turns); a chain clock more than 60 s behind Ethereum's makes honest sources look so, and they are demoted (`/health` names the gap); with a sequencer whose clock lags the updater's, a source re-dating to the slot that has just started can make `Open`s the chain refuses, and pause sending, while that lasts | another provider, or the consumer's own beacon node | which light-client data is fetched, and when; the updater's IP address | light-client data is public: any beacon node serves it, the consumer's own included | `beacon_providers_lie_lag_or_stop` |
| Execution RPC providers | `eth_getProof`, receipts and headers for a request | liveness only | refused: a proof is checked against the record's state or receipts root (110 and the other codes of the [consumer guide](guides/consumer-guide.md) §8) | another provider, or the consumer's own node | which accounts, slots and logs a consumer asks about; its IP address | proofs are public data: any full or archive node serves them | `execution_providers_cannot_forge_a_fact` |
| Checkpoint source | each fresh checkpoint | **safety**, until checked | a false root: `verify_checkpoint` (built, with its provider checks: §2.1) or a manual comparison catches it; a root no honest node serves a bootstrap for gives no instance; an instance from another checkpoint has another id, which a consumer pinned to the instance it checked refuses (211) | nothing to stop: a checkpoint is public | nothing | a finalized block root is public: anyone initialises an instance from one it checked | `checkpoint_source_misleads_only_until_checked` |
| LEZ sequencers, and the LEZ nodes clients read from | inclusion, ordering, block timestamps; the instance's state and records, read through a node's RPC by the updater and the consumer's client | liveness only, for Ferrata: the zone's execution is LEZ's own trust, as for every program on it; the bridge's LEZ-side clock is attested Ethereum time, which a producer can delay (by leaving out a `Tick`) but not move ([`lez/BRIDGE.md`](../lez/BRIDGE.md) §6) | cannot make the program accept an invalid update; can leave transactions out; a node that misreports the instance makes the updater plan a step the program refuses, or none, and the consumer's `Attest` still runs on the zone's real state | the zone stops for every program; the instance resumes from its own state when the zone does (R.7) | public transactions; senders' IP addresses | a censored submitter is replaced by another (the test leaves one submitter's transactions out); a node is replaced by another or the consumer's own; a zone that includes nothing is an outage of LEZ itself; producers that together leave out every transaction to the program while including others censor it, which only LEZ's own censorship resistance answers, as for every program on the zone | `sequencers_censor_or_stall_the_instance_still_advances` |
| Deployer (the program and an instance's `Init`) | the immutable deployment | nothing after deployment | — | — | nothing | it keeps no power: the program is immutable (LEZ v0.4's loader) and no instruction rewrites an instance (F.13); its own `Init` sent again, or with another chain id, is refused | `deployer_holds_nothing_after_deployment` |

From [`attestation.md`](design/attestation.md) §11.5. An update is accepted on validity alone, never on
the submitter's identity (VS.6; §4.5 there). Apart from the checkpoint source, no participant can make
the program accept a false record (VS.5). The consumer's own client (the wallet, the `ferrata-attest` tool, a
module) is the user, not a third party; the attestation side's monitor (`attest-monitor`), which compares records with several
providers and warns before the weak-subjectivity bound, is not required for an attestation (§2.2).

**The updater's sources.** `ferrata-updater` runs the program's own checks on every candidate before it pays for a
step, demotes a source that serves what the program would refuse (or an update signed in the future by the chain's
clock), and with `min_agreeing` waits until enough sources
serve, or hold canonical, the update's finalized header ([`lez/updater/README.md`](../lez/updater/README.md), EVIDENCE
rows 79–81). With `min_distinct_clients = k` (built 18 November 2026, row 147) it also sends only an update that
sources reporting `k` different consensus clients at `/eth/v1/node/version` served byte for byte (`client_match =
"bytes"`, the default) or agree on (`"finalized-header"`), from `k` different sources, and logs and names in `/health`
two sources serving different bytes for the same update. An honest node of any client derives light-client data only
from blocks its own fork choice imported, so **with `client_match = "bytes"`** (the default, and the setting to use) a
forged update that one client's node serves (a bug, or a captured committee's accomplice) is not sent, as long as the
accomplice holds fewer than `k` of the configured sources. **With `"finalized-header"` it is**: that mode compares only
the finalized header, and a `headers` node confirms a block canonical, not finalized, so an update one light-client
source alone serves is sent when its finalized header is canonical, a forged next committee among them (a captured
committee's forged handoff, §2.2). What it does not do in either mode: the client is the provider's own claim, which
nothing authenticates (a provider behind a load balancer can answer from several clients); the updater counts `k`
configured sources and cannot tell whether two of them are one provider (the configuration refuses the same host twice
under the policy, not two hosts of one provider); and it binds only this updater, since the program accepts the same
update from anyone (§2.2, "What this means for the bridge"). While it waits for clients and its sources serve updates
more than `max_lag_slots` ahead, its `/health` says so. Measured live on 18
November ([`lez/updater/results/client-diversity-2026-11-18/`](../lez/updater/results/client-diversity-2026-11-18/README.md)):
Nimbus and Lodestar served the same bytes for every period update asked (five periods each on mainnet and Sepolia),
but rarely the same finality update at the same moment, because they keep different ones (Lodestar the first of a
newly finalized checkpoint for the whole epoch, Nimbus the newest each slot); the updater remembers which clients
served which bytes, so it matches them across rounds. Live for ten minutes on mainnet, on the in-process chain: with
`k = 2` byte for byte (Nimbus, Lodestar) it applied two period updates and one finality update, each from both
clients, and skipped one finalized checkpoint no two clients served alike; with `k = 3` on the finalized header
(Lighthouse's node the third) it skipped none.

### 2.5 When the light client's guarantees degrade or fail

| Condition | What happens | What a consumer does | Where |
|---|---|---|---|
| A captured sync committee (342 of 512 seats signing a header off the canonical chain) | forged records; facts about blocks that never existed are attested | size caps and freezes to what it can lose; the instance freeze on conflicting finality exists only in the experimental v0.3.1 port | §2.2 |
| A false checkpoint, not checked before use | the instance follows a false chain from its start | check the checkpoint (`verify_checkpoint`, §2.1) before pinning an instance; a pinned instance id pins its checkpoint | §2.1, §2.4 |
| The weak-subjectivity bound passed; a whole sync-committee period without Ethereum finality; a third of one committee withholding its signatures for a period | the instance refuses every update (53, or no update to apply) and stops safely; records already written stay valid | move to a replacement instance from a fresh checkpoint, a new trust decision ([consumer guide](guides/consumer-guide.md) §9) | §2.1, §2.2 |
| A fork that is not in the schedule fixed at `Init` | the instance cannot follow it; a successor instance under the same root id can | accept the successor ([consumer guide](guides/consumer-guide.md) §9) | [`attestation.md`](design/attestation.md) §3.5 |
| A consumer holding the program's id in mutable state | whoever controls that pointer substitutes a program that attests to anything | pin the program; derive every actor from constants ([consumer guide](guides/consumer-guide.md) §2) | VS.3 |
| An unsound proof system (RISC Zero, LEZ's privacy circuit), or LEZ accepting a forged receipt (LEZ v0.4.0-rc1 and rc3 verify with risc0-zkvm 3.0.5, under advisory) | private reads and private consumers can be forged, and, through the calls LEZ replays on a receipt's word, any instance's store and records (`ApplyVerified`, or `Apply` and `Record` sent in a job's or the instance's name), which public consumers read too: no check of the public job guards against it | caps and freezes | §5 |
| A bug in Ferrata's core or program | depends on the bug; the deployed program cannot be patched | move to a new deployment ([consumer guide](guides/consumer-guide.md) §9) | §6 |
| A LEZ zone that includes nothing | no update, no attestation, for every program on the zone | wait; the instance resumes from its own state | §2.4 |

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
attestors' nodes**, which every LEZ full node assumes. It is beaten by `t` attestors (their attestation keys, or the LEZ and Logos Blockchain nodes they trust) together with one channel key of the live set (to inscribe a forged tip; `t` attestation keys alone no longer suffice), or by eclipsing enough attestors' nodes for as long as it takes to show them `k` private blocks. Each attestor signs a tip only when its own LEZ node holds it in LEZ's final tier (applied, not parked). A LEZ node may show an adopted head block `Finalized` early (LEZ #1053); the attestor also requires the block's inscription to be final on its Logos Blockchain node. `lez` must be the attestor's own LEZ node, and its Logos Blockchain node its own too: attestors that share a node share its lies. It tolerates `n − t` silent sequencers, needs no beacon, and works on any
EVM chain; the attestors and the LEZ and Logos Blockchain nodes they trust are safety roles, every other off-chain role is liveness-only
([`bridge.md`](design/bridge.md) §9).
**Status:** the rule is built and measured in the checkpoint program and the vault, and one real proof was
verified on a rule (B) vault ([`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md) §4;
[`vault/README.md`](../vault/README.md)), of the guest before the tip-inscription check (that check is
built, tested and measured, §4a there, not proven for real); the attestor is built as a program and as a Logos
module (EVIDENCE rows 63–66, 94–95), and whether the zone's sequencers will run it is the decision the proposal
asks of Logos ([`bridge.md`](design/bridge.md) §9).

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
   at least `minReleaseDelay` and at most the revocation window `revokeWindow` (immutable deployment parameters:
   24 h and 7 days by the design's defaults, 0 and 1 day in the deploy script's testnet configuration). In it
   the freeze authority can
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

### 3.6 A testnet stand-in for the checkpoint proof (public LEZ testnet, v0.3.1)

**Not part of the design above, and not for production.** On the public LEZ testnet neither rule can run:
its blocks are LEZ v0.3.1 (the checkpoint programs read v0.4), and its sequencer, which Logos runs, runs
no attestor and has certified no key for us. To show a burn made there released on Sepolia, one test
deployment replaces the checkpoint proof and nothing else
([`vault/src/standin/README.md`](../vault/src/standin/README.md),
[`lez-v03-return/README.md`](../lez-v03-return/README.md)):

- **Real:** the unchanged vault under rule (B) with all its rules, the deposits, and every release, which
  still needs a zero-knowledge proof of the unchanged `release` program, checked by RISC Zero's Groth16
  verifier, that the burn is under the accepted root.
- **Stand-in:** the burn root. The vault's verifier is `StandInCheckpointVerifier`, which for one constant
  id that is no program's (`0x66ff80f9…dc38`, the vault's `checkpointImageId`) accepts the signatures of 2
  of 3 keys **that the operator of the run holds**, over the vault's own journal digest, and sends every
  other id to RISC Zero's verifier. The keys sign only through an attestor that has compared the burns with
  the bridge's state on Logos's public node and cut the checkpoint at a block that node reports final, the
  blocks after it read and none touching the bridge (`standin.py sign` refuses a checkpoint without both
  checks). The vault refuses equal checkpoint and release ids, so the stand-in id can never be the release
  id; the live run's keys are fresh ones that have signed nothing else.

**You trust the operator's keys for everything outbound in that deployment:** two of them can make the
vault accept any burn root, then prove releases of burns that never happened under it, and take the escrow
(two 0.001 ETH test deposits). Nothing checks LEZ blocks, their validity or their inscriptions; V1–V3 and
the finality assumptions of §3.3 and §3.4 are replaced by the operator's honesty and by one node's answer.
What still holds: a release needs a real proof for a burn leaf of this configuration, token, amount and
recipient; a signature is good for one journal, vault, verifier and chain; the wait, the release delay,
nullifiers and RISC Zero's stop work as in §3.5. Status: rehearsed end to end on a fork of Sepolia
([`e2e/runs/testnet-return-fork-1/`](../e2e/runs/testnet-return-fork-1/README.md)); nothing of it had
been sent to Sepolia or to the LEZ testnet when this was written (18 November 2026). What a real
checkpoint for that network would need, and what its public data already lets anyone check, is measured in
[`lez-v03-return/STAGE2.md`](../lez-v03-return/STAGE2.md).

## 4. Roles and powers (RFP-022 Security #5, #7)

### 4.1 Roles

From [`vault/README.md`](../vault/README.md), "Roles"; [`bridge.md`](design/bridge.md) §14;
[`lez/BRIDGE.md`](../lez/BRIDGE.md) §5:

| Role | Can | Cannot | If its key is lost | If its key is stolen |
|---|---|---|---|---|
| **Admin** (multisig) | register a token once; enable or disable deposits per token; set caps above the immutable floor; set release delays within `[minReleaseDelay, revokeWindow]` for later roots; raise Δ; propose watchers and a new freeze authority, applied 14 days later; announce fee schedules (in force 7 days later, ≤ 1% each) | move escrow; change a registered token; lower Δ; cut a cap below the floor; freeze, unfreeze, cancel, revoke, retire; change any immutable (verifier, `mode`, image ids, `lezConfigId`, timers, floors) | no new tokens or caps; everything else keeps working | registry and cap churn within bounds; a public 14-day window before a role change |
| **Freeze authority** (multisig) | freeze deposits, releases, checkpoints (vault) and mints, burns (LEZ), globally or per token, each for at most `maxFreeze` (on the vault an immutable deployment parameter, 30 days by default, which the constructor does not bound; on LEZ a 30-day constant, of attested Ethereum time, so a LEZ freeze holds while the light client stalls) unless renewed; `cancelCandidate`; `revokeFrom` within the window; `retire()`; LEZ `Sunset` | touch the registry, caps, delays, Δ or escrow; accept anything; undo a stop or a retirement; hand itself over | freezes lapse; anyone retires once 30 days plus one candidate wait pass without checkpoint progress (counted from the end of the last checkpoint freeze, if later); the admin replaces it in 14 days | **denial of service, not theft**: renew freezes, revoke honest checkpoints within the window, retire the vault |
| **Watchers** (up to three hot keys) | cancel a pending candidate; pause checkpoints for at most 12 h | anything else | — (others watch) | delay finalisation until the admin replaces them |
| **Emergency stop** (RISC Zero's `RiscZeroVerifierEmergencyStop`) | stop every proof-gated call for good | be tripped by anyone holding a key: its owner is renounced at deployment, so only `estop(receipt)`, RISC Zero's proof of unsoundness, trips it (a receipt that verifies for the all-zero claim digest; RISC Zero's advisory of 11 November 2026 came with none, and the stops of the Sepolia vaults did not trip) | — | — |
| **Anyone** | deposit; submit and finalise checkpoints with proofs; record beacons; release with a proof; refund once allowed; apply a role proposal after 14 days; retire after 30 days without checkpoint progress (no submission or finalisation, counted from the end of the last checkpoint freeze or pause if later, plus one candidate wait), or once a sunset checkpoint can no longer be revoked | — | — | — |

On LEZ the same controls are built ([EVIDENCE](EVIDENCE.md) row 74; harness, dev mode): the freeze
authority's 14-day handover (proposed, applied by anyone once 14 days of attested Ethereum time have
passed, cancellable), an M-of-N freeze authority (`Cosigned`: one transaction all signers sign), watchers'
12-hour pauses and the keyless `Sunset` (anyone attests the vault's `retired` flag)
([`lez/BRIDGE.md`](../lez/BRIDGE.md) §2, §5; [`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) §14). The admin's own
handover on LEZ is not built ([README](../README.md#specified-but-not-built), "Specified but not built").
**One exception on the deployed LEZ program** (the red team's F1,
[`reviews/redteam-2026-11-18.md`](reviews/redteam-2026-11-18.md)): its `Init` does not refuse controls whose freeze
authority is the admin, so in such a configuration the admin holds both roles and can freeze and run `Sunset`, which the
table says it cannot. `configId` commits to the controls, so this is visible before anyone deposits; the vault refuses
the same pair at construction (`RolesMustDiffer`). The program's second build (not deployed) refuses such an `Init`
(240; [`lez/results/bridge-next.md`](../lez/results/bridge-next.md), "F1"); milestone B1 deploys it.

### 4.2 Off-chain participants of the bridge (RFP-022 Security #7)

(After [`bridge.md`](design/bridge.md) §6, §12.3, §14.)

| Who | Provides | Can affect | If it lies | If it stops | Learns |
|---|---|---|---|---|---|
| Root importer | one attestation per record | liveness only | refused by the primitive | anyone else imports | public data |
| Relayer | gas drops, submission | liveness only | cannot redirect or overcharge: proofs bind destination, recipient and fee; but a plaintext claim it is given carries `s`, with which it can freeze that deposit through a second configuration (the deployed program; fixed in the second build, not deployed: EVIDENCE rows 136, 143) | the user picks another or submits directly | §8.2 |
| Checkpoint prover | candidate and finalisation proofs | liveness only | refused by the vault | anyone with a prover and an x86 machine; a backlog is proven in pieces | public data |
| Release wrapper | the Groth16 wrap | liveness only | refused by the vault | another wrapper, or the user's own x86 machine | what the release publishes |
| Watchers (`monitor`) | cancellations, 12-hour pauses | liveness under rule (B); **safety** under rule (A) outside its conditions F1–F4 (§3.4) | delay finalisation until the admin replaces them | rule (B): the release delay and revocation still catch a bad candidate | public data |
| Accredited sequencers, their attestors and the LEZ and Logos Blockchain nodes each attestor reads (its operator's own) | LEZ blocks; attestations under (B) | **safety** for the return leg: `t` of them can forge a burn root (§3.2; §3.5 bounds it) | as said | the zone stops; finalisation needs `t` live keys under (B), `t + 1` under (A) | public transactions |
| RPC and LEZ nodes (other than an attestor's own) | chain data | liveness only | refused: checked against roots and records | another node | request metadata; recovery downloads whole lists, never per item |

Under rule (B) every off-chain role apart from the accredited sequencers, their attestors and the LEZ and Logos Blockchain nodes each attestor reads is liveness-only (Security #7); under rule
(A) the watchers also have the safety role of §3.4.

### 4.3 Outages and malfunctions of third-party components (RFP-022 R.9)

Each component a third party is expected to run, what its outage and its malfunction do, the mitigation that is
built, and the test or recorded run that shows the mitigation.

| Component | Outage: effect | Malfunction: effect | Mitigation (built) | Shown by |
|---|---|---|---|---|
| Light-client updater | no fresh records: claims of newer deposits wait | a wrong update is refused by the program | anyone runs one (no permission); the user runs its own | §2.4's tests; EVIDENCE rows 79, 80 |
| Root importer | deposits after the last import cannot be claimed yet | an import the attestation does not prove is refused (110, 244, 247), so nothing wrong is written | `ImportRoot` is permissionless: `ferrata-relayer --import-roots`, `ferrata_bridge_module.import_root`, or the user (who then dates its deposit, as the user guide warns) | `lez/harness/tests/bridge_inbound.rs`; EVIDENCE row 106 (refused before sending, nothing spent) |
| Relayer, inbound (gas drop) | a first private claim has no native LEZ for its fee | it cannot redirect: the receipt binds `N`, the one-time address and the relayer; one credit per `N` | another relayer per claim; or native LEZ from elsewhere, a correlation the client states | EVIDENCE rows 47, 48, 124 (M0 check 2 on LEZ v0.4.0-rc3: the credit sealed to a one-time private account and swept by the claim) |
| Relayer, outbound (release submission) | the release is not sent | a release with the recipient, the relayer or the fee changed is refused by RISC Zero's verifier; a replay by `NullifierAlreadyUsed` | any relayer can send the job (relayer address 0 by default), or `ferrata release --self-send` | EVIDENCE row 46 (`e2e/runs/relayer-fork/`); `cli/tests/e2e.rs` (`--self-send`, row 85) |
| Release wrapper | no Groth16 seal | a wrong seal is refused by the vault | another wrapper, or the user's own x86 machine (`ferrata release --prove`, then any wrapper) | `cli/tests/relayer.sh` (a succinct receipt wrapped by the daemon, row 46) |
| Checkpoint prover | releases wait; 30 days plus one candidate wait after the last checkpoint progress anyone may retire the vault, and refunds open after a sunset checkpoint and its revocation window | an invalid proof is refused; a prover cannot leave out a burn | anyone with a prover; a backlog proven in pieces; keyless retirement and refunds | `vault/test/` (refunds, retirement: row 12); `checkpoint/RESULTS.md` §3, §5 |
| Watchers (`monitor`) | rule (B): a bad candidate is still caught by the release delay and revocation; rule (A): §3.4 | cancels honest candidates: a delay, until the admin replaces it (14 days) | up to three watchers; restart from the chain; a failed cancellation retried | EVIDENCE rows 43–45 (`monitor/tests/anvil-e2e.sh`: kill and restart, lost state, no gas money, a forked node) |
| Rule (B) attestors (beside each sequencer) | up to `n − t` silent sequencers are tolerated | fewer than `t` keys cannot forge a checkpoint; an uncertified key or another deployment's is refused | the threshold | EVIDENCE rows 66 (a silent sequencer on a four-sequencer zone), 68 (refusals in the guest) |
| Ethereum RPC and LEZ nodes | the client cannot read or send | data checked against the vault's roots and the zone's records; a wrong chain or a missing vault refused | any endpoint, by option or in the view | `cli/tests/e2e.rs` (an endpoint that does not answer, another chain, no vault: row 85); the mini-app's editable endpoints (row 62) |
| Admin and freeze authority (multisigs) | no new tokens or caps; no freeze | denial of service within §4's bounds | freezes lapse after `maxFreeze` (30 days by default on the vault, a constant on LEZ); 14-day public role changes; keyless retirement | `vault/test/Freeze.t.sol`, `vault/test/Roles.t.sol` (row 12); `lez/harness/tests/bridge_controls.rs` (row 74) |


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
- **LEZ's check of private-transaction receipts, and why the public path rests on it too** (added 23 November
  2026). LEZ v0.4.0-rc1 and rc3 accept a privacy-preserving transaction when each disclosed program's image is the one
  its account holds and its proof, decoded as any RISC Zero `InnerReceipt`, passes `Receipt::verify` against the
  privacy circuit's image id (LEZ's `lee/state_machine/src/validated_state_diff/mod.rs:655-688` at v0.4.0-rc3,
  `:618-651` at rc1; `lee/state_machine/src/privacy_preserving_transaction/circuit/mod.rs:51-65`, the same in both). Both lock
  risc0-zkvm 3.0.5, risc0-groth16 3.0.4 and risc0-circuit-recursion 4.0.4, in whose `Receipt::verify` a composite,
  succinct or Groth16 receipt is accepted alike (risc0-zkvm 3.0.5 `src/receipt.rs:344-354`; a fake one only in dev
  mode). LEZ's own prover makes succinct receipts (`circuit/mod.rs:658` at rc3, `:655` at rc1) and its fee model assumes one
  (LEZ's `lez/programs/fee/core/src/market.rs:45-51`, rc1 and rc3); we found no check that refuses another kind. These versions are in
  the ranges of RISC Zero's advisory GHSA-g6qg-9j8p-7j87, whose Groth16 verifier accepts a receipt with a substituted
  claim digest. Whether a forged receipt passes LEZ's check has not been tested: we built none, and it is for Logos to
  confirm ([security note](security/risc0-groth16-advisory-2026-11-21/README.md) §3 and its correction of 23
  November 2026). GHSA-mm6c-35cm-g4x2, about the Solidity verifiers, does not reach LEZ.
  A private transaction acts on public actors through the calls its journal lists as crossing to the public side
  (`BoundaryStep::PrivateToPublic`, LEZ's `lee/state_machine/core/src/execution_state.rs:132-137`). LEZ replays each
  of them on the public side as the journal states it, its receiver, message and sender and the authorizations and
  PDA seeds it carries, with no check of its own (`replay_proven_calls`, `:842-872` at v0.4.0-rc1 and rc3:
  `proven.with_from(Sender::Call(proven.envelope.from))` at `:849`; LEZ's own comment at `:733-734`: a proven call is
  "replayed publicly unchecked"); the receipt check is what binds these calls to a
  real private run. A receiving program sees a replayed call as it sees any call from that sender (`ReceiveInput`,
  `lee/state_machine/core/src/program/mod.rs:373-385`, has no field that tells them apart), and the delivery's
  inherited authorizations and PDA seeds decide whether it is authorized (`authorize`, `execution_state.rs:1431-1470`).
  So, by our reading of LEZ's source (not tested), every check of who sent a call (a named sender, an actor of the
  receiver's own program: `ReceiveInput::from_own_program`, `program/mod.rs:390-393`) or of its authorization is met by
  a journal that names it; the receiver's own checks of the call's content still run, since LEZ executes the receiver
  on the replayed call (`deliver_to_public`, `execution_state.rs:910-917`). A forged receipt LEZ accepted could reach:
  - in the attestation program, `ApplyVerified` from any actor of the program (`lez/guests/program/src/main.rs:288-305`),
    `Apply` in a job actor's name (`:269-286`) and `Record` in the instance actor's name (`:405-425`; `attest` trusts a
    record because only `Record`, accepted from `instance(id)` alone, writes one: `:429-430`). A job is applied after
    a check of its phase, its adoption and its starting store (`crates/ferrata-core/src/steps.rs:605-623`) and the
    period and slot-order rules of `apply_core_compact` (`:304-335`), which bound each job, not where a run of them
    ends: over a few jobs a forger installs a committee of its own. So it could write any instance's store and
    records, from which every attestation follows, public consumers' included;
  - in the bridge program, `issue`, which mints (`lez/guests/bridge/src/main.rs:639`), the burn delivery a checkpoint
    reads (`:775`) and the transfer delivery (`:852`), and the handlers that accept a configuration's or a nullifier's
    actor as sender (`:180`, `:202`, `:448`, `:473`);
  - in the reference consumer, `spend` (`lez/guests/consumer/src/main.rs:185`) and its credits (`:219`, `:285`); and in
    `ferrata_holdings`, `ferrata_statements`, the statements reader and `ferrata_rebate`, the replies they accept by
    their sender, a record's, an entry's or an instance's (`lez/guests/holdings/src/main.rs:40`,
    `lez/guests/statements/src/main.rs:41`, `lez/guests/statements-reader/src/main.rs:43`,
    `lez/guests/rebate/src/main.rs:59`, `:89`).
  No Ferrata program is outside this, and nothing in these programs removes it. A program cannot tell a replayed call
  from a call another actor makes. A replayed call always names a sender (LEZ's `Sender::source`,
  `execution_state.rs:346-351`), so only handlers that take top-level calls alone are out of its reach, such as the
  bridge's role actors (`lez/guests/bridge/src/main.rs:979`). Every Ferrata program accepts calls from other actors
  somewhere, and no flag of ours separates them: an instance refusing `ApplyVerified` would still accept `Apply` and
  `Record` in its own actors' names. The fix is LEZ's: a LEZ whose receipt check is sound, a LEZ on
  risc0 3.1.0 (LEZ `dev` moved at `6aaabe5`). No live exposure follows today: no v0.4 program of ours runs on a public
  network, and every v0.4 sequencer named in this repository is one we ran.
  **Corrected 23 November 2026:** the first half of that sentence stopped holding on 23 November 2026, when the
  project's own public test network (LEZ v0.4.0-rc3 on RISC Zero 3.0.5, its one sequencer ours, test value only)
  started running these programs; until LEZ moves to RISC Zero 3.1 that network's program state is exposed as this
  paragraph describes, if LEZ accepts a forged receipt, which has not been tested.
- **If the proof system is unsound** and RISC Zero publishes the proof, anyone can trip the stop: nothing
  more is released or refunded, owed balances stay claimable, and whatever escrow remains stays in the
  vault (vault v5; [`vault/README.md`](../vault/README.md), "Verifier and stop", "Refunds"). Before the stop
  trips, an unsound proof system breaks everything a proof gates; caps and freezes bound it.
- **21 November 2026: RISC Zero's advisory GHSA-mm6c-35cm-g4x2 is this case, and the stop did not trip.** RISC Zero
  published no proof of exploit; the forgery makes the verifier accept a chosen claim, not any seal; and whether it
  reaches the all-zero claim digest that `estop(receipt)` requires is not established. On the four Sepolia vaults only
  the admin's caps stop it ([security note](security/risc0-groth16-advisory-2026-11-21/README.md)); decision D1 of
  [risc0-3.1-migration.md](design/risc0-3.1-migration.md) §4 changes this for new vaults.
- **Image ids are reproducible at any build path** since `integration7` (`scripts/guest-rustc`: source paths
  remapped and cargo's path-dependent `-C metadata` replaced in guest builds): the same commit built at two paths with
  two `$HOME`s gives byte-identical guests and ids, 14 of 14 ([`image-ids.md`](image-ids.md),
  [`guides/verify.md`](guides/verify.md) §2); what must match is the pinned toolchains. The first live run's programs
  (`crown-live-1`) were built before this, at the maintainer's checkout path, and reproduce only there (6 of 6 on the
  build box, [`guides/verify.md`](guides/verify.md)); the final programs are the ones a production deployment uses, and
  the ones the live run `crown-live-3` deployed: its vault's image ids equal this tree's rebuilt ones (6 of 6 at
  `f13faee2`, [`guides/verify.md`](guides/verify.md) §2; since milestone B1 the bridge program was B1's
  (`847a4336…`, the one our test network runs), ADR 0046's was `ferrata_bridge_f13` (`ffb47f6e…`, never deployed), and
  since ADR 0048 this tree's is `ferrata_bridge_lezfee` (`2ba8dc18…`), which no deployed vault pins; crown-live-3's rebuilds from `f13faee2`: [`image-ids.md`](image-ids.md)).

## 6. Failure classes, what is lost, what bounds it (Security #9)

There is **no escrow-moving failsafe**: an escrow-moving recovery would need a party to choose new
verification logic, which Security #8 forbids. The failsafe is stop-only
([`bridge.md`](design/bridge.md) §15).

**If the verifier accepted any seal** (the proof system unsound), the vault's own checks still bound the loss, as
measured with a verifier that accepts everything behind RISC Zero's real emergency stop, its owner renounced
([`vault/README.md`](../vault/README.md) "Under a broken verifier": 8 invariants on both rules, a seven-step
walkthrough, [EVIDENCE](EVIDENCE.md) row 138): a token pays only from its own escrow; releases and refunds in any
window stay within the token's and the global release bucket (with caps UNLIMITED, as the deployment script's testnet
defaults leave them, only the escrow bounds it); nothing pays before the candidate's wait and the token's release
delay; a revoked root pays nothing, and with the release delay equal to the revocation window the revocation always
comes first; freezes hold; and since the broken verifier also accepts a junk proof of exploit, anyone can trip the
stop, after which nothing proven is accepted. Which depositors' escrow of a token goes is not bounded.

This is the model of a verifier that accepts every seal. RISC Zero's advisory of 11 November 2026 (GHSA-mm6c-35cm-g4x2)
is narrower: the verifier accepts a chosen claim, no junk proof of exploit is known to verify, and on 21 November 2026
the stops of the four Sepolia vaults had not tripped. Until a stop trips, the bounds are the ones above but the stop:
the token's escrow, the clocks, revocation and freezes where a freeze authority holds a key, and the caps. On the
Sepolia vaults (no freeze-authority key) that left the escrow and the clocks until the admin set every release and
deposit cap to zero on 21 November 2026, which is what stops a forgery there
([security note](security/risc0-groth16-advisory-2026-11-21/README.md), [EVIDENCE](EVIDENCE.md) row 154). How the
remaining escrow leaves such a vault is decided by
[ADR 0033](decisions/0033-broken-verifier-without-proof-of-exploit.md): it does not. No escrow-moving exit meets
Security #9's constraints without a sound verifier, so the freeze authority's runbook contains instead (releases frozen
globally, so refunds too; checkpoints and deposits frozen; each freeze renewed before it lapses; the caps; retirement),
the affected funds and in-flight positions stay in the vault while it holds, and only `owed` balances still move, to
their own payees ([EVIDENCE](EVIDENCE.md) row 269). The runbook is procedure, not enforced by the vault: a lapsed
freeze reopens the forged path.

| Failure | What happens | Positions lost | Bound |
|---|---|---|---|
| Sync committee captured (inbound) | forged records; invented deposits imported and minted | wrapped supply above escrow; the shortfall falls on late redeemers | LEZ mint caps until the freeze authority freezes mints |
| `t` sequencer keys (outbound) | a forged burn root; releases of tokens never burned | escrow paid out against forged burns | wait + watchers; release delay with `revokeFrom`; release caps |
| Rule (A) outside F1–F4, watchers down or eclipsed | a released burn later dropped on LEZ (double spend) | as above | the same |
| Proof system unsound (the verifier accepts any seal) | forged checkpoints and releases, a forged sunset's refunds (to the leaves' depositors) | escrow, token by token, any depositor's | per-token escrow; the wait, cancel and revoke; the release delay; release caps; freezes; the stop, which anyone can then trip with a junk proof of exploit (with an accept-all verifier; under GHSA-mm6c-35cm-g4x2 no such proof was known and the stop did not trip) ([`vault/README.md`](../vault/README.md) "Under a broken verifier") |
| Proof system unsound | anything a proof gates, until the stop trips | after the stop: every unreleased burn and unrefunded deposit stays in the vault | caps and freezes before; the stop after; the stop may never trip (§5) |
| LEZ's check of private-transaction receipts unsound (LEZ v0.4.0-rc1 and rc3 on risc0-zkvm 3.0.5; GHSA-g6qg-9j8p-7j87, for Logos to confirm; §5) | a forged receipt delivers the bridge program's `Mint` from a nullifier's actor and writes a burn record from a token's actor, neither through the gate that holds LEZ's mint and burn caps and freezes (`lez/guests/bridge/src/main.rs:179-193`, `:761-771`); it can also write the attestation program's records (§5) | wrapped supply above escrow, without bound on LEZ; a forged burn record is LEZ state, so an honest checkpoint carries it and its release pays out of escrow, token by token; the shortfall falls on late redeemers. What happens to each in-flight position is not analysed for this case | on Ethereum only: the release caps; the release and checkpoint freezes while renewed; `revokeFrom` inside the window, at the freeze authority's judgement (the checkpoint is true to LEZ's state, so the wait and the watchers do not refuse it); retirement. Nothing on LEZ. The fix is LEZ's (a LEZ on RISC Zero 3.1) |
| A bug in Ferrata's guests | depends on the bug | depends | release freeze and `revokeFrom` ([`bridge.md`](design/bridge.md) §14) |
| A bug in the vault | depends; no upgrade | depends | freezes, retirement; caps |
| A wrong checkpoint not revoked within 7 days | the vault halts for good | every position not yet released | caps bound what the error itself released |
| A LEZ change the configuration cannot follow | retirement before it; unimported deposits refunded | tokens never burned before the change stay backed in the vault but cannot be released | notice period asked of Logos (M0) |
| A Bedrock era change under a zone on Logos Blockchain 0.3.5-rc.1 (its transactions then carry another fork digest) | the checkpoints stop: the second build of the checkpoint guests pins one digest through `lezConfigId` and refuses the new one ([`lez/results/v0.4.0-rc3/return-leg/`](../lez/results/v0.4.0-rc3/return-leg/README.md)); retirement as for a LEZ change | as the row above | the same notice period, asked for Bedrock's eras too |
| Admin or freeze key compromised | denial of service within the bounds of §4 | none directly | lapsing freezes, 14-day public replacement, keyless retirement |
| Every operator stops | no imports, checkpoints or relays | none: anyone can resume; claims never expire | keyless retirement 30 days plus one candidate wait after the last checkpoint progress, from deployment on, and refunds after a sunset checkpoint and its revocation window (operators keep a heartbeat checkpoint) |
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
| light-client update | anyone running the updater | they need fresh records (consumers, the bridge's importers); a rebate pool anyone funds pays whoever lands an advance through it (soft #5, `ferrata_rebate`) | updates yes; the rebate pool built and tested in the harness, not deployed; funding it from the bridge's fees needs a conversion that is not built |
| `ImportRoot` | anyone, as a top-level call or private root (the bridge refuses a nested or asynchronous import, which the checkpoint could not count; security review H-1) | depositors want to claim; relayers earn relay fees on claims | yes |
| `submitCandidate`, `finalize` / `finalizeAttested` | anyone with a prover | the **checkpoint fee** of every release against that root goes to the candidate's prover, the sender of `submitCandidate` (not of the finalisation, which under rule (B) needs no proof) | yes (vault v5; prover since the security fixes of 11 November) |
| `recordBeacon` (rule (A)) | anyone, typically the finaliser | needed for its finalisation | yes |
| relaying claims and releases | relayers | the registered relay fee, bound by the proof (a plaintext claim's relayer is a free field: whoever gets a copy of it included first takes its relay fee, at most 1% of the denomination; the private claim binds the relayer inside the proof; security review L-4, not changed) | fee yes; the relayer daemon `ferrata-relayer` yes (EVIDENCE row 46: on a Sepolia fork; row 48: against the LEZ harness) |
| `cancelCandidate`, `pauseCheckpoints` | watchers | appointed by the admin; no fee | vault side yes; `monitor` yes (EVIDENCE rows 43–45: a local chain and a Sepolia fork), also as a Logos module (row 96) |
| `refund` after retirement | anyone, typically the depositor | it pays the depositor named in the leaf | yes |
| `retire()` 30 days plus one candidate wait after the last checkpoint progress (`retireOpensAt()`), or after an irrevocable sunset checkpoint | anyone | to open refunds | yes (vault); LEZ `Sunset` by the freeze authority, or by anyone once the vault's `retired` flag is attested (EVIDENCE row 74) |

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
| timing | deposit-to-claim and burn-to-release intervals | the client proposes a delay (the `ferrata` tool and the mini-app: a claim delay of 1–12 h from the seed, a random release delay; EVIDENCE rows 84, 86) |
| the root a claim reads | the deposit is at least that old | read the newest root |
| a retry after a cap rejection | the first attempt's time | the client checks caps before proving |
| a fee-schedule change | before ADR 0046 it split sets by date through the claim's public fee range, and on our test network, which still runs that program (B1), it still does; since ADR 0046 the paired claim has no schedule range or mint-fee amount, and since ADR 0048 a burn has no fee either (it burns one denomination), though timing/root correlations remain; a deposit's payment and a release's payout depend on the schedule in force at their own (public) block | the vault escrows the mint and burn fees and `CollectFees` mints them on LEZ in totals over many deposits and releases (ADR 0048); schedules change rarely, with 7 days' notice |
| the fee payer of a private transaction | the payer appears as a fresh alias with its reserve in plaintext; a first claim funded from a faucet or a purchase links to that source until the gas drop exists | fixed reserve per operation type; the gas drop: its receipt and the relayer's one credit per `N` (EVIDENCE rows 47, 48), and the credit sealed to a one-time private account and swept by the claim, with LEZ's wallet on v0.4.0-rc3 (row 124; in the relayer daemon and the `ferrata` tool on LEZ v0.4.0-rc1's sequencer with dev-mode proofs, row 205) |
| a private read of a record | which record and which consumer program | read the newest record; hop inside the proof |
| the plaintext claim | which deposit paid which mint | private by default; the client warns before allowing it |
| network metadata | IP addresses to RPC endpoints and sequencers | out of scope; editable endpoints (U.9: the mini-app's, EVIDENCE row 62) |
| an adversary's own deposits | subtracted from any anonymity set | none |

### 8.1 Every component that handles user data (Privacy #4)

The values that would connect the two legs are the seed and what derives from it: `kAuth`, `kView`, a deposit's
secret `s` and the opening of its commitment `C` (`s`, the destination's kind, the destination), a burn's `r`,
`sOut` and recipient, and the burn counter. **On the default, private path, none of them leaves the user's machine, and
none is in any submitted transaction** (a plaintext claim, which the user must choose explicitly, publishes its whole
witness: the leaf index, the depositor, `j`, `s` and the destination): the privacy suite searched every public byte of both chains over 20 round trips for 320 such values
and found none, in dev mode on this tree's programs and on real proofs of the deployed ones (the bridge program `0xccde6171…`, before milestone B1; EVIDENCE rows 23, 89;
[`privacy/RESULTS.md`](privacy/RESULTS.md); every public field, step by step:
[`privacy/generated/fields.md`](privacy/generated/fields.md)).

| Component | Who runs it | What of the user's it handles | Where it stays | In a submitted transaction |
|---|---|---|---|---|
| The `ferrata` tool, the mini-app and `ferrata_bridge_module` | the user | the seed (a file, `chmod 600`), every derived key and secret above, the deposit tree and burn notes it downloads | the user's machine; the module returns no `kAuth`, `kView`, `s` or `sOut`, except `s` inside a claim message (plaintext: by the user's explicit choice; private: as the input of the private execution, never sent) ([`modules/README.md`](../modules/README.md)) | no linking value (rows 23, 89) |
| LEZ's wallet (the prover of private claims and burns) | the user | the witness of a private claim or burn: the leaf, its path, `s`, the destination | on the user's machine; only the proof and the transaction's public parts leave | the nullifier `N`, the root read, the fee alias: §10 of the [user guide](guides/user-guide.md) |
| Relayer (gas drop; release submission) | any third party, per operation | the gas-drop receipt; a release job | its state directory | only what the step publishes anyway: §8.2 |
| Release wrapper | any x86 machine, the user's included | a succinct release receipt | its machine | what the release publishes (the journal) |
| Root importer, checkpoint prover, updater | any third party | nothing of the user's | — | — |
| Ethereum RPC providers, LEZ nodes | any provider; the user may run its own | request metadata: the IP address, the time, which blocks and lists are read (recovery downloads whole lists, never one item) | the provider | — |
| Ethereum (the deposit) | public chain | the depositor, the token, the denomination, `C`, the pair (depositor, `j`) | public by design | yes, by design: the deposit is the public leg |

The `kView` key reveals burns and recipients but cannot claim or release ([`bridge.md`](design/bridge.md) §4, §6).

### 8.2 What a party submitting for the user learns, and switching it per operation (Privacy #6)

| Party | Receives | Learns beyond what the chain then publishes | Never learns | Switched per operation by | Shown by |
|---|---|---|---|---|---|
| Gas-drop relayer (a first private claim's fee) | the gas-drop receipt: its journal `DROP_DOMAIN ‖ root ‖ evmChainId ‖ vault ‖ N ‖ oneTimeAddress ‖ relayer` (212 bytes) | that a claim spending `N` under that root will follow, the one-time address, the time of the request, the requester's IP address | which deposit, its depositor, its destination | the receipt names one relayer and the claimant picks it per claim; another relayer refuses a receipt naming someone else | EVIDENCE rows 47, 48, 124 |
| The relayer a claim names (relay fee) | nothing: it is named inside the claim the user proves and sends | nothing beyond the published claim, which names it and mints it the relay fee (the user sends the claim; a relayer asked to submit it would also learn the time of the request and the user's IP address, as the gas-drop relayer does) | which deposit | `ferrata claim --relayer`, per claim | `cli/tests/e2e.rs` (row 84) |
| Release relayer | the job: the release journal and its seal, or the succinct receipt | the job's fields are exactly what the release publishes (chain, vault, configuration, root, `NOut`, token, amount, recipient, relayer, fee); in addition the time it received the job and the sender's IP address | `sOut`, `r`, the burn's index, the seed: which burn | a job naming relayer address 0 (the default) can be sent by any relayer, which the vault pays; `--relayer` names one; `--self-send` needs none | EVIDENCE row 46 (`e2e/runs/relayer-fork/`: another relayer's daemon refuses a job naming a different relayer); `cli/tests/e2e.rs` (rows 84, 85) |
| Release wrapper | a succinct receipt | the journal, as above | the witness (succinct receipts are zero-knowledge, §5) | any wrapper, or the user's own x86 machine | `cli/tests/relayer.sh` (row 46) |

What every submitting party appears as on chain, measured: a claim names its relayer and mints it the relay fee
publicly; a release names its relayer; every private claim and burn names the account the unspent fee reserve is
refunded to ([`privacy/generated/fields.md`](privacy/generated/fields.md)).

## 9. CROPS review

(Format of ethskills `crops`; applied to this design.)

- **Censorship resistance.** Risk: the zone's sequencers can exclude transactions; a freeze authority can
  pause flows for up to `maxFreeze` at a time (30 days by default; a checkpoint freeze also holds back keyless retirement); a single relayer could refuse users. Mitigation: every role
  except watchers and the multisigs is permissionless; freezes lapse; claims never expire. User escape:
  submit directly; pick another relayer per operation; refunds after retirement (which anyone can
  trigger 30 days plus one candidate wait after the last checkpoint progress) and a sunset checkpoint.
- **Open.** Risk: no module catalog is published yet, and an operator builds from source. Mitigation: the whole
  stack is in this repository, every daemon also as a Logos module ([`modules/`](../modules/README.md)), guest image
  ids reproduce at any path (§5), the deployment is scripted and rehearsed. User escape: run your own updater,
  importer, prover, relayer and wrapper from the repository.
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

`verify_checkpoint` as an API of Logos Core's Ethereum module (its execution check runs through `verified_proxy_module`;
the upstream pull request is not opened); the attestation side's monitor against public providers (built and
tested on recorded answers: `monitor/attest/`); `monitor`'s client of the LEZ and Logos Blockchain nodes' RPCs;
consumers bound to the root id (the bridge, `ferrata_holdings` and the reference consumer still pin an
instance); on LEZ, the admin's own handover; the gas drop inside the relayer daemon (the sealed credit and its sweep
by the claim ran with LEZ's wallet on v0.4.0-rc3, EVIDENCE row 124; the daemon's own credit is a public transfer that
no private claim can sweep); a rule (B) zone of Logos's sequencers with the attestor module beside
each, its checkpoints on Sepolia, and a rotation that removes a key; in the deployed light-client program, the freeze
of an instance on evidence of conflicting finality (an experimental, unaudited version exists in the bench and the
v0.3.1 port, [EVIDENCE](EVIDENCE.md) row 144); a cost for forging a record: bonded or restaked backing that a consumer
can require, or anchoring to Ethereum's FFG finality (§2.2, design only); the audit. Sources:
[README](../README.md#specified-but-not-built), "Specified but not built"; [`attestation.md`](design/attestation.md) §15; [`bridge.md`](design/bridge.md) §19;
[`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) §9.

Built since the first draft of 11 November, with their evidence: the weak-subjectivity bound, successors and root
ids ([EVIDENCE](EVIDENCE.md) rows 69, 70); the watchers' `monitor` (rows 43–45); the rule (B) attestor
program (rows 63–68); the attestor and the watchers' `monitor`, also as Logos modules (rows 94–96); on LEZ,
watchers, an M-of-N freeze authority, the 14-day handover and the keyless `Sunset` (row 74); the gas-drop receipt
(rows 47, 48); a zone of four sequencers running the bridge program, its checkpoint proven and both burns
released, and a rotation its committee proposed (rows 100, 101); the relayer's root imports and its Logos module
(rows 106, 107); the gas drop's sealed credit swept by a private claim, on LEZ v0.4.0-rc3 (row 124); the test that
walks every off-chain party of the attestation side (§2.4).
