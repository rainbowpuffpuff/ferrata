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
| Two thirds of each 512-member sync committee | that every recorded header was finalized | forged records: facts about blocks that never existed are attested; the signers lose no stake (none of it is slashable for this) | consumer caps and freezes; the attestation side's `monitor` alarm (not built); an instance freeze on evidence of conflicting finality (experimental, the v0.3.1 port only, not deployed) | §2.2 |
| Fewer than `t` of the LEZ channel's accredited keys are an attacker's, in every key set the checkpoint program used | validity of every burn root the vault accepts (both rules) | `t` keys can forge a burn root | the public wait, watchers, the release delay and revocation, release caps | §3.2 |
| Logos Blockchain: no reorganisation deeper than `k` on the attestors' nodes | finality of accepted burn roots under rule (B) | a released burn is later undone on LEZ | the same | §3.3 |
| Rule (A)'s conditions F1–F4, or else a live, uneclipsed watcher | finality under rule (A) | as above, with fewer than `t` keys or none | the same; outside F1–F4 the watchers are trusted for safety | §3.4 |
| RISC Zero's proof system (and LEZ's privacy circuit) | that every accepted proof is true | anything a proof gates | the emergency stop, tripped only by RISC Zero's own proof of unsoundness | §5 |
| Ferrata's own code (core, programs, guests, vault) | everything | depends on the bug | caps, freezes, `revokeFrom`; immutability means no fix in place | §6 |
| Admin and freeze authority (multisigs) | liveness only: they cannot move escrow or accept anything | denial of service | freezes lapse after 30 days; keyless retirement; 14-day public role changes | §4 |
| Until milestone B1: nobody who knows a public-kind deposit's `s` creates a second configuration of the same vault and chain, and nobody fills a destination's `holder(dest)` | that every public-kind deposit can be claimed under the paired configuration, and that a public destination can receive | that deposit is frozen: the paired configuration's claim is refused (250) and, once it has imported the deposit, the leaf is below the refund boundary, so its escrow stays in the vault; a filled destination can no longer be credited | no theft and no double mint: the second configuration's mints and burns are under its own id, which no vault pays; only the depositor or whoever sees a plaintext claim before inclusion knows `s` (a private claim, the CLI's default path, does not publish it; a private-kind deposit cannot be taken); the fix is planned for B1 ([`lez/results/tenant.md`](../lez/results/tenant.md)) | [`bridge.md`](design/bridge.md) §3 |
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
the program applies the bound by the sequencer's clock. The alarm before an instance runs out (the
attestation side's `monitor`) is not built ([`attestation.md`](design/attestation.md) §3.6, §4.6).

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
  (`minReleaseDelay` 24 h, at most `revokeWindow`, 7 days), within which the freeze authority freezes releases or
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
that far ahead ([`lez/BRIDGE.md`](../lez/BRIDGE.md) §6). The attestation side's `monitor`, which would compare records with several providers' canonical roots
and feed consumers' freeze authorities, is **not built** (the watchers' `monitor` of the return leg is:
EVIDENCE rows 43–45) ([`attestation.md`](design/attestation.md) §11.3). A deeper inbound
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
- freshness relative to the weak-subjectivity period, beyond what the bound enforces (a store past its
  period refuses every update; a record's own age is not bounded: §2.1).

### 2.4 Off-chain participants of the light client

| Who | Provides | Can affect | If it lies | If it stops | Observes |
|---|---|---|---|---|---|
| Any submitter or prover | updates | liveness only | refused: signatures are checked | anyone else submits | public data |
| Beacon and execution providers | light-client data, proofs, receipts | liveness only | refused: checked against signatures and records | another provider, or your own node | which blocks are fetched |
| Checkpoint source | each fresh checkpoint | **safety**, until checked | a false root; `verify_checkpoint` (built, with its provider checks: §2.1) or a manual comparison catches it | nothing to stop | nothing |
| LEZ sequencers | inclusion, block timestamps | liveness only: the bridge's LEZ-side clock is attested Ethereum time, which a producer can delay (by leaving out a `Tick`) but not move ([`lez/BRIDGE.md`](../lez/BRIDGE.md) §6) | cannot forge a record | the zone stops | public transactions |
| Deployer | the immutable deployment | nothing after deployment | — | — | nothing |

From [`attestation.md`](design/attestation.md) §11.5. An update is accepted on validity alone, never on
the submitter's identity (VS.6; §4.5 there). Apart from the checkpoint source, no participant can make
the program accept a false record (VS.5).

**The updater's sources.** `ferrata-updater` runs the program's own checks on every candidate before it pays for a
step, demotes a source that serves what the program would refuse, and with `min_agreeing` waits until enough sources
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

From [`vault/README.md`](../vault/README.md), "Roles"; [`bridge.md`](design/bridge.md) §14;
[`lez/BRIDGE.md`](../lez/BRIDGE.md) §5:

| Role | Can | Cannot | If its key is lost | If its key is stolen |
|---|---|---|---|---|
| **Admin** (multisig) | register a token once; enable or disable deposits per token; set caps above the immutable floor; set release delays within `[minReleaseDelay, revokeWindow]` for later roots; raise Δ; propose watchers and a new freeze authority, applied 14 days later; announce fee schedules (in force 7 days later, ≤ 1% each) | move escrow; change a registered token; lower Δ; cut a cap below the floor; freeze, unfreeze, cancel, revoke, retire; change any immutable (verifier, `mode`, image ids, `lezConfigId`, timers, floors) | no new tokens or caps; everything else keeps working | registry and cap churn within bounds; a public 14-day window before a role change |
| **Freeze authority** (multisig) | freeze deposits, releases, checkpoints (vault) and mints, burns (LEZ), globally or per token, each for at most `maxFreeze` (30 days; on LEZ, of attested Ethereum time, so a LEZ freeze holds while the light client stalls) unless renewed; `cancelCandidate`; `revokeFrom` within the window; `retire()`; LEZ `Sunset` | touch the registry, caps, delays, Δ or escrow; accept anything; undo a stop or a retirement; hand itself over | freezes lapse; anyone retires after 30 days without checkpoint progress (time under a freeze does not count); the admin replaces it in 14 days | **denial of service, not theft**: renew freezes, revoke honest checkpoints within the window, retire the vault |
| **Watchers** (up to three hot keys) | cancel a pending candidate; pause checkpoints for at most 12 h | anything else | — (others watch) | delay finalisation until the admin replaces them |
| **Emergency stop** (RISC Zero's `RiscZeroVerifierEmergencyStop`) | stop every proof-gated call for good | be tripped by anyone holding a key: its owner is renounced at deployment, so only `estop(receipt)`, RISC Zero's proof of unsoundness, trips it | — | — |
| **Anyone** | deposit; submit and finalise checkpoints with proofs; record beacons; release with a proof; refund once allowed; apply a role proposal after 14 days; retire after 30 days without checkpoint progress (no submission or finalisation; freezes and pauses do not count), or once a sunset checkpoint can no longer be revoked | — | — | — |

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

**Off-chain participants of the bridge** (after [`bridge.md`](design/bridge.md) §6, §12.3, §14):

| Who | Provides | If it lies | If it stops | Learns |
|---|---|---|---|---|
| Root importer | one attestation per record | refused by the primitive | anyone else imports | public data |
| Relayer | gas drops (the receipt and one credit per `N` built, EVIDENCE rows 47, 48; the sealed credit's sweep not built), submission | cannot redirect or overcharge: proofs bind destination, recipient and fee; but a plaintext claim it is given carries `s`, with which it can freeze that deposit through a second configuration until B1 (EVIDENCE row 136) | the user picks another or submits directly | a time, a one-time address, the transaction it submits |
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
- **Image ids are reproducible at any build path** since `integration7` (`scripts/guest-rustc`: source paths
  remapped and cargo's path-dependent `-C metadata` replaced in guest builds): the same commit built at two paths with
  two `$HOME`s gives byte-identical guests and ids, 14 of 14 ([`image-ids.md`](image-ids.md),
  [`guides/verify.md`](guides/verify.md) §2); what must match is the pinned toolchains. The first live run's programs
  (`crown-live-1`) were built before this, at the maintainer's checkout path, and reproduce only there (6 of 6 on the
  build box, [`guides/verify.md`](guides/verify.md)); the final programs are the ones a production deployment uses, and
  the ones the live run `crown-live-3` deployed: its vault's image ids equal this tree's rebuilt ones (6 of 6 at
  `f13faee2`, [`guides/verify.md`](guides/verify.md) §2; since milestone B1 this tree's bridge program is B1's, which
  no live vault pins, and the deployed one rebuilds from `f13faee2`: [`image-ids.md`](image-ids.md)).

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

| Failure | What happens | Positions lost | Bound |
|---|---|---|---|
| Sync committee captured (inbound) | forged records; invented deposits imported and minted | wrapped supply above escrow; the shortfall falls on late redeemers | LEZ mint caps until the freeze authority freezes mints |
| `t` sequencer keys (outbound) | a forged burn root; releases of tokens never burned | escrow paid out against forged burns | wait + watchers; release delay with `revokeFrom`; release caps |
| Rule (A) outside F1–F4, watchers down or eclipsed | a released burn later dropped on LEZ (double spend) | as above | the same |
| Proof system unsound (the verifier accepts any seal) | forged checkpoints and releases, a forged sunset's refunds (to the leaves' depositors) | escrow, token by token, any depositor's | per-token escrow; the wait, cancel and revoke; the release delay; release caps; freezes; the stop, which anyone can then trip with a junk proof of exploit ([`vault/README.md`](../vault/README.md) "Under a broken verifier") |
| Proof system unsound | anything a proof gates, until the stop trips | after the stop: every unreleased burn and unrefunded deposit stays in the vault | caps and freezes before; the stop after |
| A bug in Ferrata's guests | depends on the bug | depends | release freeze and `revokeFrom` ([`bridge.md`](design/bridge.md) §14) |
| A bug in the vault | depends; no upgrade | depends | freezes, retirement; caps |
| A wrong checkpoint not revoked within 7 days | the vault halts for good | every position not yet released | caps bound what the error itself released |
| A LEZ change the configuration cannot follow | retirement before it; unimported deposits refunded | tokens never burned before the change stay backed in the vault but cannot be released | notice period asked of Logos (M0) |
| A Bedrock era change under a zone on Logos Blockchain 0.3.5-rc.1 (its transactions then carry another fork digest) | the checkpoints stop: the second build of the checkpoint guests pins one digest through `lezConfigId` and refuses the new one ([`lez/results/v0.4.0-rc3/return-leg/`](../lez/results/v0.4.0-rc3/return-leg/README.md)); retirement as for a LEZ change | as the row above | the same notice period, asked for Bedrock's eras too |
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
| relaying claims and releases | relayers | the registered relay fee, bound by the proof (a plaintext claim's relayer is a free field: whoever gets a copy of it included first takes its relay fee, at most 1% of the denomination; the private claim binds the relayer inside the proof; security review L-4, not changed) | fee yes; the relayer daemon `ferrata-relayer` yes (EVIDENCE row 46: on a Sepolia fork; row 48: against the LEZ harness) |
| `cancelCandidate`, `pauseCheckpoints` | watchers | appointed by the admin; no fee | vault side yes; `monitor` yes (EVIDENCE rows 43–45: a local chain and a Sepolia fork), also as a Logos module (row 96) |
| `refund` after retirement | anyone, typically the depositor | it pays the depositor named in the leaf | yes |
| `retire()` after 30 days without checkpoint progress, or after an irrevocable sunset checkpoint | anyone | to open refunds | yes (vault); LEZ `Sunset` by the freeze authority, or by anyone once the vault's `retired` flag is attested (EVIDENCE row 74) |

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
| a fee-schedule change | splits sets by date: the claim's fee range shows which schedule applied | schedules change rarely, with 7 days' notice |
| the fee payer of a private transaction | the payer appears as a fresh alias with its reserve in plaintext; a first claim funded from a faucet or a purchase links to that source until the gas drop exists | fixed reserve per operation type; gas drop (its receipt and the relayer's credit built, EVIDENCE rows 47, 48; the sealed credit to a private one-time account and its sweep not built) |
| a private read of a record | which record and which consumer program | read the newest record; hop inside the proof |
| the plaintext claim | which deposit paid which mint | private by default; the client warns before allowing it |
| network metadata | IP addresses to RPC endpoints and sequencers | out of scope; editable endpoints (U.9: the mini-app's, EVIDENCE row 62) |
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
- **Open.** Risk: the daemons' Logos modules do not exist yet (guest image ids reproduce at any path since
  `integration7`, §5).
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

`verify_checkpoint` as an API of Logos Core's Ethereum module (its execution check runs through `verified_proxy_module`;
the upstream pull request is not opened); the attestation side's `monitor` (records against
several providers, the weak-subjectivity alarm); `monitor`'s client of the LEZ and Logos Blockchain nodes' RPCs;
consumers bound to the root id (the bridge, `ferrata_holdings` and the reference consumer still pin an
instance); on LEZ, the admin's own handover; the gas drop's sealed credit to a private one-time account
and its sweep inside the private claim; a rule (B) zone of Logos's sequencers with the attestor module beside
each, its checkpoints on Sepolia, and a rotation that removes a key; in the deployed light-client program, the freeze
of an instance on evidence of conflicting finality (an experimental, unaudited version exists in the bench and the
v0.3.1 port, [EVIDENCE](EVIDENCE.md) row 144); a cost for forging a record: bonded or restaked backing that a consumer
can require, or anchoring to Ethereum's FFG finality (§2.2, design only); the audit. Sources:
[README](../README.md#specified-but-not-built), "Specified but not built"; [`attestation.md`](design/attestation.md) §15; [`bridge.md`](design/bridge.md) §19;
[`lez/BRIDGE-RESULTS.md`](../lez/BRIDGE-RESULTS.md) §9.

Built since this list's first draft, with their evidence: the weak-subjectivity bound, successors and root
ids ([EVIDENCE](EVIDENCE.md) rows 69, 70); the watchers' `monitor` (rows 43–45); the rule (B) attestor
program (rows 63–68); the attestor and the watchers' `monitor`, also as Logos modules (rows 94–96); on LEZ,
watchers, an M-of-N freeze authority, the 14-day handover and the keyless `Sunset` (row 74); the gas-drop receipt
(rows 47, 48); a zone of four sequencers running the bridge program, its checkpoint proven and both burns
released, and a rotation its committee proposed (rows 100, 101); the relayer's root imports and its Logos module
(rows 106, 107).
