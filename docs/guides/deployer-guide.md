# Deploying Ferrata: the programs on LEZ and the vault on an EVM chain

**Deployer guide (RFP-022 R.14, F.15, F.16, Security #8; RFP-021 VS.3). Revised for delivery on 21 November 2026,
against the tree at this commit; it replaces the first draft of 11 November.** For an entity standing up its own, independently configured deployment: which decisions to
take before anything is sent, in which order the pieces are deployed and paired, how gas limits are
set on a chain with Glamsterdam's state gas, how tokens, fees, the admin and freeze authorities and
the finality depth are configured, and how to rehearse. Every statement cites its source; paths are
relative to the repository root.

**Status.** Nothing has been deployed for production, and the code is not audited
([`vault/README.md`](../../vault/README.md); [`README.md`](../../README.md), "Not audited"). What was run: the vault's
deployment script on local chains and forks of Sepolia (one transaction per run, limits from the node), and by the
owner on public Sepolia for the live round trips (EVIDENCE rows 41, 91, 142; the sources verified, row 122); the LEZ
programs through LEZ's own program loader on zones we ran; the one-command deployment of a release, tested on anvil
(row 53). The walkthrough of §11 was followed from a fresh clone, step 0 rebuilding every Ferrata program from source
and building LEZ's sequencer and the Logos Blockchain node from their pinned sources (what their builds download: §11
step 0), and stood up a configuration of its own, with its own roles, fees, token, caps and finality settings, on anvil and a local
LEZ zone: recorded there. It was run on our build box, so it shows that the steps work as written, not that a stranger
found them clear.

**Where each clause is answered (RFP-022 R.14).** Registering tokens: §3 steps 5 and 7, §5 ("Registry bounds"), §11
step 3, items 3 to 5. Setting fees: §2, §5 ("Fee schedules"), §11 step 3, item 5. Configuring the admin and the freeze
authorities: §2, §5 ("Roles"), §11 steps 1 and 3 (items 2, 3, 5). The finality depth: §2 ("Finality depth"), §11
step 1. An independently configured
deployment (F.16): §7 and §11. Pairing (F.15): §6. Migration (Security #8): §9.

## 1. What a deployment consists of

| Piece | Where | Shared or per configuration | Mutable after deployment |
|---|---|---|---|
| Attestation program (RFP-021) | LEZ | shared by every consumer and configuration | no: immutable header, enforced by the v0.4 loader |
| An attestation **instance** | LEZ, actor `instance(id)` of that program | shared; `id = H(config ‖ checkpoint)` | only by verified updates |
| Bridge program | LEZ | shared by every configuration ("tenant") | no |
| A bridge **configuration** `config(cid)` | LEZ, actors of the bridge program | per configuration | roles act within fixed bounds; tokens write-once |
| `FerrataVault` + its library `FerrataRegistry` | the EVM chain | per configuration | no proxy, no settable verifier, no rescue function |
| RISC Zero's `RiscZeroGroth16Verifier` behind its `RiscZeroVerifierEmergencyStop` | the EVM chain | may be shared; the vault pins the stop's address | stop owner renounced at deployment |
| Checkpoint program (rule (A) or rule (B) image), release program | off chain; their image ids are vault immutables | per vault | no |

Sources: [`docs/design/attestation.md`](../design/attestation.md) §3.3–§3.4;
[`docs/design/bridge.md`](../design/bridge.md) §1–§3, §15; [`lez/BRIDGE.md`](../../lez/BRIDGE.md) §2;
[`vault/README.md`](../../vault/README.md), opening and "Deployment".

## 2. Decide before deploying

Everything below is fixed at deployment, either as a vault immutable, inside `configId`, or in the LEZ
configuration. The vault's constructor enforces none of the design's defaults; the deployer must
choose them ([`vault/README.md`](../../vault/README.md), last paragraph of "Sepolia round trip"; "Roles").

| Decision | Design default | Testnet value used by the scripts | Fixed in |
|---|---|---|---|
| Outbound finality rule, `mode` | (B) if the zone's sequencers run `ferrata_attestor`; otherwise (A) | (A) for the Sepolia runs; (B) rehearsed | vault immutable |
| Committee floors `nMin`, `tMin` | at least 3 and 2 outside testnets; (A) recommends `n ≥ 6` | 3 and 2 (`script/params/sepolia.json`) | vault immutable |
| Wait: `deltaFloor` and initial `delta` (Δ) | (A): 3 h 01 min; (B): 1 h | the design default for the rule since milestone B1 (`script/DeployVault.s.sol`, `test/DeployDefaults.t.sol`); `FERRATA_DELTA` sets another for a test run (the testnet runs of November 2026: 600 s; the crown runs: 3,600 s); `script/RoundTrip.s.sol`: 600 s | `deltaFloor` immutable; `delta` raise-only (`raiseDelta`), ≤ 30 days |
| `turnSeconds` (the channel's posting timeframe) | 900 s on testnet values | 900 | vault immutable |
| `revokeWindow` | 7 days | 1 day | immutable |
| `maxFreeze` | 30 days | 30 days | immutable |
| `minReleaseDelay`; per-token release delay | 24 h (0 on testnets); delay between it and `revokeWindow` | 0; 120 s | immutable; admin per token, for roots finalised later |
| Admin | a multisig of the deployer | the deployer | two-step handover |
| Freeze authority | a multisig, never the admin | an address without a key unless `FERRATA_FREEZE_AUTHORITY` names one | replaced only by the admin's proposal, 14 days later |
| Watchers | up to three hot addresses, each with its own nodes | none | admin's proposal, 14 days later |
| Fee recipient | the deployer's choice | the deployer | vault immutable |
| Tokens, denominations, relay and checkpoint fees, `lezUnit`, `unitWeight` | three denominations per token; fees ≤ 1% each | ETH 0.001/0.01/0.1, relay 0.2%, checkpoint 0.1%; test token 100/1,000/10,000, 0.25% and 0.1% | write-once per token |
| Caps per token and day | the smaller of 10% of escrow and a deployer-set ceiling, above an immutable floor | capped by default since milestone B1: per token and day 100 times its largest denomination (10 ETH, 1,000,000 test-token units; `ethDailyCap`, `tokenDailyCap` in the params), globally their sum, an immutable release floor of one largest denomination a day (`script/DeployVault.s.sol`, `test/DeployDefaults.t.sol`); `FERRATA_UNCAPPED=1` deploys without caps, as the live runs of November 2026 did; `script/RoundTrip.s.sol` (the earlier runs' single-script round trip) deploys none. Why 100 times: below | admin, never below the floor |
| Attestation instance (its root id) | an instance whose checkpoint you verified | the Sepolia period-1378 bootstrap | inside `configId` |
| Inbound finality depth | Ethereum finality, no extra epochs (depth 0) | 0 | the admin's `SetDepth` on LEZ, in beacon slots beyond finality, at most 216,000; it applies to later imports only (since B1) |

Sources: [`bridge.md`](../design/bridge.md) §9, §13; [`vault/README.md`](../../vault/README.md),
"Deployment (v5)", "Roles", "Sepolia round trip"; [`lez/live/inbound/README.md`](../../lez/live/inbound/README.md).

**Finality depth (R.14, F.12).** Inbound, the default is Ethereum finality with no extra epochs; a
deeper setting delays honest claimants but not a captured sync committee, whose defence is caps and
freezes ([`bridge.md`](../design/bridge.md) §5.4). The admin sets it with `SetDepth`; roots already imported stay
imported, so no valid claim is lost (`lez/harness/tests/bridge_depth.rs`). Outbound, the depth is the vault's wait (Δ, plus
`(t − 1) × turnSeconds` under (A)), by default the design's for the rule, 3 h 01 min under (A) and 1 h under (B), which
`script/DeployVault.s.sol` deploys as `deltaFloor` and initial `delta` unless `FERRATA_DELTA` names another
(`test/DeployDefaults.t.sol`). The admin can only raise it (`raiseDelta`, at most 30 days): a lower wait would shorten
the window to react to a wrong candidate ([`bridge.md`](../design/bridge.md) §9), so the floor is immutable; a raise
applies only to candidates submitted after it. The per-token release delay holds a finalised root's releases while the checkpoint can still be revoked
(between `minReleaseDelay` and `revokeWindow`; a change applies to roots finalised after it); the scripts deploy the
testnet 120 s, not the design's 24 h or more ([`vault/README.md`](../../vault/README.md), "LEZ checkpoints",
"Releases and pull payments").

**Why the caps are 100 largest denominations a day.** The design bounds a day's loss by the smaller of 10 % of escrow
and a ceiling the deployer sets, above an immutable release floor. A vault's buckets hold fixed capacities the admin
sets (`setTokenCap`, `setGlobalCap`, never a release cap below the floor), so a deployment can only start from the
ceiling: 10 % of the zero escrow of a new vault would refuse its first deposit. The script's ceiling, 100 times a
token's largest denomination a day, lets a day of deposits and releases through at the largest size (99 of them once
each counts its fees: `test/DeployDefaults.t.sol`) and is the most a broken verifier or a stolen key can take in a day
before a freeze. 10 % of escrow is the smaller bound until escrow passes 1,000 largest denominations; until then the
admin should lower the caps toward it as escrow grows (derived: 100 = 10 % of 1,000). The release floor, one largest
denomination a day, is what no admin can throttle releases below.

**Choosing the rule.** Rule (A) needs an unpredictable `prevrandao` (Ethereum and its testnets); on a
chain whose `prevrandao` or timestamp is constant or sequencer-chosen, deploy rule (B). Rule (B) also
needs an attestor beside every sequencer (`ferrata_attestor`, a program and a Logos module), tolerates
`n − t` silent sequencers against `n − 1 − t` for (A), and costs one proof per checkpoint against two
([`bridge.md`](../design/bridge.md) §3, §9). Trust consequences are in [`docs/trust.md`](../trust.md).

## 3. Order of deployment

The vault pins the LEZ configuration id, and the LEZ configuration refuses to initialise unless the
vault's storage holds that same id, so the order is fixed (D2; [`bridge.md`](../design/bridge.md) §3):

1. **LEZ: deploy the attestation program and the bridge program** through LEZ's `program_loader`,
   immutable. Measured on a local v0.4.0-rc1 sequencer: 6 transactions per 508 KB image (storage gas
   479,496, execution gas 0), the loader recomputing the image id ([`lez/RESULTS.md`](../../lez/RESULTS.md)
   §9); the lab deployed its 602,888-byte image in 7 ([`lez/lab/README.md`](../../lez/lab/README.md)). **Image ids do not
   depend on the build path** since `integration7` (`scripts/guest-rustc`; [`docs/image-ids.md`](../image-ids.md)): the
   same commit and toolchains give the same ids anywhere (`scripts/reproduce-image-ids.sh`, [`verify.md`](verify.md));
   the live crossing still deploys frozen binaries and refuses any other file
   ([`lez/live/inbound/README.md`](../../lez/live/inbound/README.md); [`bridge.md`](../design/bridge.md)
   §20). A reproducible build is a precondition of an immutable production deployment.
2. **LEZ: initialise an attestation instance** from a checkpoint you verified (`Init`, three
   transactions with the committee's adoption; [`attestation.md`](../design/attestation.md) §4.2), or
   choose an existing one. `verify_checkpoint` in `ferrata_attest_module` recomputes the instance id, runs the program's
   `Init` checks and the full `KeyValidate` on the bootstrap, compares the roots you give it, and asks the
   beacon providers you name (at least three hosts must agree, none disagree) and an execution endpoint
   (EVIDENCE rows 59, 105; [`attestation.md`](../design/attestation.md) §11.4). Name providers you consider
   independent: the method counts hosts, it cannot tell who runs them.
3. **Compute `configId`** ([`bridge.md`](../design/bridge.md) §3; [`lez/BRIDGE.md`](../../lez/BRIDGE.md) §4):
   ```
   configId = H("ferrata/config/v1" ‖ bedrockGenesisId ‖ channelId ‖ bridgeHeader ‖ bridgeImageId
                ‖ evmChainId ‖ vault ‖ attestationRootId ‖ salt)
   attestationRootId = H("ferrata-lez/v1/bridge/attestation-root" ‖ attestation program account ‖ instance id)
   salt = H("ferrata-lez/v1/bridge/controls" ‖ borsh(controls))
   ```
   `salt` commits to the LEZ roles and cap defaults, so a front-run `Init` cannot install others. The
   vault's address is not known yet: predict it from the deployer's address and nonce (plain CREATE).
   CREATE2 is not usable, because the vault's init code contains `configId`, which contains the vault's
   address. `live-driver inbound-plan --attest-bin … --bridge-bin … --vault ADDR` prints every part and
   the id ([`lez/live/inbound/README.md`](../../lez/live/inbound/README.md), "The configuration's identity").
4. **EVM: deploy the vault** with `vault/script/deploy.sh` (§4), giving `lezConfigId`, the checkpoint
   images (`checkpointImageIdA` or `B`), the release image, `genesisState` (the checkpoint state the
   vault starts from, [`bridge.md`](../design/bridge.md) §8.1), `nMin`, `tMin` and the timers. A run
   that would put the vault anywhere but the predicted address stops before broadcasting
   ([`vault/README.md`](../../vault/README.md), "Sepolia round trip").
5. **EVM: register tokens** on the vault (`registerToken`: denominations, relay fees, checkpoint fees,
   `lezUnit`, `unitWeight`, LEZ definition; write-once) and **renounce the stop's owner**, so that only
   RISC Zero's own proof of an exploit can trip it.
6. **LEZ: `Init` the configuration** once the vault's deployment is final and recorded in an Ethereum
   record of the instance: `Init` computes `configId` on chain (taking `bridgeHeader` from the program's
   own account), attests the vault's slot 2 and refuses with 246 `PAIRING_MISMATCH` unless it holds the
   id ([`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §2). Under a Gloas record the transaction
   carries the block's RLP header (215 without it; §2a there).
7. **LEZ: `Register` each token** from the vault's own registry slot (an attestation of four slots: the
   entry, fee schedules 0 and 1, the leaf count) and `ImportFees` as schedules take effect; `SetCap` by
   the admin. The LEZ entry is created only by attesting the vault's, so the two chains cannot disagree
   ([`lez/BRIDGE.md`](../../lez/BRIDGE.md) §5).
8. **Start the operators**: root importer, checkpoint prover, watchers (operator guide).

## 4. The vault: `script/deploy.sh`, one transaction per run

`script/DeployVault.s.sol` deploys under rule (A) or (B), **one transaction per run**; `script/deploy.sh`
repeats it until its record says complete ([`vault/README.md`](../../vault/README.md), "Deployment (v5)"):

1. `RiscZeroGroth16Verifier` (or the one `FERRATA_GROTH16_VERIFIER` names);
2. `RiscZeroVerifierEmergencyStop` around it;
3. the library `FerrataRegistry`;
4. `FerrataVault`, its 11 library call sites linked to that address (never `new FerrataVault`, which
   would deploy a library of its own); the recorded library's runtime code must be FerrataRegistry's
   exactly, since the vault DELEGATECALLs it;
5. the test token (testnets);
6. and 7. `registerToken` for ETH and the token;
8. `renounceOwnership()` of the stop;
9. no transaction: the checks (the stop has no owner, is not tripped and wraps the verifier; the vault's
   verifier is the stop; rule, admin and freeze authority; both registrations, schedule 0, the published
   registry and fee hashes), then `complete: true`.

The record (`deployments/<chain id>-<mode>.json`) holds every address, the deployer, the rule, the gas
rule and margin, and per transaction the node's estimate and the limit. **Keys:** neither the script nor
the driver reads one; forge signs with Foundry's keystore variables (`ETH_KEYSTORE`, `ETH_PASSWORD`), or
`--unlocked` on a local fork. The script refuses chain 1, forge's default sender and, except on chain
31337, the public anvil accounts.

### 4.1 Gas limits come from the node (Glamsterdam)

Sepolia has run **Glamsterdam** since 6 October 2026 (epoch 353,024). Its state gas (EIP-8037) is not in
Foundry's EVM: 1,530 gas per byte of new code, 183,600 per new account, 97,920 per new storage slot.
EIP-7825's per-transaction cap of 2^24 = 16,777,216 now bounds **execution** gas only; the whole limit is
capped at 2^32 − 1 and by the block gas limit. The first live attempt sent the Groth16 verifier's
creation with forge's limit of 1,533,981 and it **ran out of gas**; Sepolia's node asks 7,795,331 for it
([`vault/GAS.md`](../../vault/GAS.md), "On Sepolia since Glamsterdam" and
"Creating the contracts on today's Sepolia").

So before it sends, a run asks the node for `eth_estimateGas` of that transaction on the chain as the
earlier runs left it, and refuses, sending nothing, unless estimate × `FERRATA_GAS_MARGIN_PCT` / 100
(default 125) is within the cap of `FERRATA_GAS_RULE`: `eip7825` (default: limit ≤ 2^24) or `eip8037`
(Sepolia: limit ≤ block gas limit and 2^32 − 1). forge then sends with the same margin, and `deploy.sh`
reads the sent limit back from the node and stops if it differs. One transaction per run is what makes
every estimate real: a transaction that depends on the previous one can only be estimated once that one
is mined ([`vault/README.md`](../../vault/README.md), "Deployment (v5)"). Measured refusal: with
`FERRATA_TX_GAS_CAP=100000` the first creation (estimate 1,179,986) was refused and nothing sent.

**Creation costs** (measured; [`vault/GAS.md`](../../vault/GAS.md), "Creating the contracts on today's
Sepolia": `eth_estimateGas` against a public Sepolia node at block 12,107,166, read-only; beside it anvil's
receipts on a fork with Foundry's rules):

| Creation | Runtime bytes | Sepolia (Glamsterdam) | Foundry / anvil |
|---|---:|---:|---:|
| `RiscZeroGroth16Verifier` | 4,810 | 7,795,331 | 1,179,986 |
| `RiscZeroVerifierEmergencyStop` | 1,698 | 2,977,644 | 451,756 |
| `FerrataRegistry` (library) | 11,606 | 18,301,151 | 2,563,131 |
| `FerrataVault`, rule (A) / (B) | 24,242 | 39,538,106 / 39,538,070 | 5,681,056 / 5,681,009 |
| test token | 1,862 | 3,342,971 | 508,036 |

These are v5 before the security fixes (11 November); the fixed vault's runtime is 24,254 bytes and the
library's 12,504 (`vault/GAS.md`), not yet estimated on Sepolia. The library and the vault exceed 2^24 in
total but almost all of it is state gas (the vault's code alone
is 24,242 × 1,530 = 37,090,260); their execution gas fits the cap, and with the 1.25 margin their limits
(22,876,438 and 49,422,632) are under the block gas limit: **both fit, no split is needed**. On a chain
that capped the whole transaction at 2^24 at this price, a creation could deposit about 10.6 kB of code,
and the vault and library together (35,848 bytes) would have to become at least five contracts (derived,
[`vault/GAS.md`](../../vault/GAS.md)).

**Registrations on Sepolia** were priced by the node only for the v3 vault: `registerToken` 2,136,791
(ETH) and 2,138,129 (token), against 472,211 and 473,049 on Foundry ([`e2e/README.md`](../../e2e/README.md),
"What the round trip costs on today's Sepolia"). The v5 vault's registration also stores fee schedule 0
and publishes the registry and schedule hashes (647,208 on Foundry, [`vault/GAS.md`](../../vault/GAS.md));
its Sepolia figure was not measured, and since it writes new slots at 97,920 each it will be higher than
the v3 figure (estimated, not measured).

### 4.2 Commands

```sh
cd vault && forge build
# a real network (the deployer's own command; keys only through Foundry's variables):
export ETH_KEYSTORE=<keystore file> ETH_PASSWORD=<file holding its password> ETH_FROM=<its address>
FERRATA_MODE=A FERRATA_GAS_RULE=eip8037 FERRATA_RPC_URL=<Sepolia RPC> script/deploy.sh
# the rehearsal on a local anvil fork of Sepolia (no key; nothing reaches Sepolia):
script/fork-rehearsal.sh [<log directory>]
```

From [`vault/README.md`](../../vault/README.md), "Deployment (v5)". Environment: `FERRATA_DELTA`,
`FERRATA_REVOKE_WINDOW`, `FERRATA_MAX_FREEZE`, `FERRATA_MIN_RELEASE_DELAY`, `FERRATA_RELEASE_DELAY`,
`FERRATA_FREEZE_AUTHORITY`, `FERRATA_FEE_RECIPIENT`, `FERRATA_WATCHERS`, `FERRATA_GAS_RULE`,
`FERRATA_GAS_MARGIN_PCT`, `FERRATA_TX_GAS_CAP`, `FERRATA_OUT`. `script/params/sepolia.json` gives both
rules' checkpoint image ids, the release image id, `lezConfigId`, `genesisState`, `nMin`, `tMin`, the
posting timeframe and the LEZ token definitions, which are **zero placeholders: registration is
write-once, so set them before a deployment the LEZ side will read**.

### 4.3 What the rehearsal showed

`script/fork-rehearsal.sh`, 10 November 2026, anvil 1.3.6 forking Sepolia at block 12,107,296 with
Foundry's gas rules, accounts impersonated ([`vault/README.md`](../../vault/README.md), "The rehearsal"):

- **Rule (A)**, RISC Zero's real verifier, `eip7825`: complete; deposits of 0.001 ETH and 100 tokens
  escrowed the denomination plus the relay fee (mint fee 0), and the deposit root is the reference's.
- **Rule (B)**, `eip8037`, the stop around an accept-all **test** verifier (no proof exists for a vault
  at a fresh address without a GPU prover): complete; the freeze authority retired the vault and
  submitted a sunset candidate; `finalizeAttested` 601 s later; after the 86,401-s revoke window both
  refunds paid the depositor back and left no escrow.
- Fork gas per step: verifier 1,179,986; stop 451,756; library 2,563,131; vault 5,681,056 (A) /
  5,681,009 (B); token 508,036; `registerToken` 707,052 / 707,830; `renounceOwnership` 25,524.

To check a deployment afterwards, open [`admin/index.html`](../../admin/index.html) (or run
`node admin/dump.mjs <rpc> <vault> <deployment block>`): it reads the vault through any JSON-RPC
endpoint, read-only, and shows its configuration and image ids, roles and watchers, the emergency
stop and its owner (renounced or not), tokens with their caps and fee schedules, and freezes
([`admin/README.md`](../../admin/README.md); a read of the live Sepolia vault is in
[`admin/results/`](../../admin/results/)).

## 5. The LEZ configuration

([`lez/BRIDGE.md`](../../lez/BRIDGE.md) §2, §4, §5; [`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §2, §8)

- **Roles** are LEZ accounts fixed in the configuration (inside `salt`, so inside `configId`): the
  admin and the freeze authority. Each acts by a top-level call it signs at its own actor of the bridge
  program, which forwards to `config(cid)` or `token(cid, token)`; the target accepts only from the
  role's actor. The freeze authority's 14-day handover, an M-of-N freeze authority (`Cosigned`), LEZ-side
  watchers' 12-hour pauses and the keyless `Sunset` are built on LEZ
  ([`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §14, EVIDENCE row 74); the admin's own handover
  is not. **Put accounts only you control into `controls`.** `crown-driver` takes them as `CROWN_ADMIN` and
  `CROWN_FREEZE_AUTHORITY` (account ids), and `crown-driver new-role-keys --out DIR` makes two from fresh random
  keys, which only `DIR` holds (§11 does this by default). Its other way, `CROWN_ROLES_LABEL`, derives both keys from
  a label as SHA-256 of a public prefix, the label and a counter ([`keys.rs`](../../lez/live/driver/src/keys.rs)):
  anyone who reads the label has the keys, and with them `SetCap`, `Freeze` and `Sunset`. Use a label only on a
  local zone, never for a configuration that holds value ([`lez/testnet04/NOTES.md`](../../lez/testnet04/NOTES.md)
  says the same of the label keys).
- **Registry bounds** checked by `Register` before any attestation: 1 to 8 denominations, strictly
  increasing, each an exact multiple of a power-of-ten `lezUnit` (at most `10^decimals`); relay fees at
  most 1% and multiples of `lezUnit`; non-zero `unitWeight`; schedule 0 without protocol fees. Refused
  with 240 otherwise (measured: nine denominations, a non-multiple, a 2% relay fee, a non-power-of-ten
  unit, a mint fee in schedule 0).
- **Fee schedules.** The admin announces schedule `current + 1` on the vault (`announceFees`); it takes
  effect seven days of Ethereum time later and applies to deposits from the leaf count at that moment; a
  deposit keeps its leaf's schedule. On LEZ, `ImportFees` appends it once in force (attested), and
  `ImportFees(j, None)` attests that schedule `j` does not exist yet at a count, extending `known_until`.
  Protocol fees start at zero under an immutable 1% ceiling ([`vault/README.md`](../../vault/README.md),
  "Fees (v5, D13)").
- **Caps.** A mint and a burn token bucket per token and one per configuration on LEZ, in value units
  (`amount / lezUnit × unitWeight`); deposit and release buckets per token and globally on the vault;
  refunds draw on the release buckets. `SetCap` never goes below the configuration's immutable floor.
  LEZ programs cannot read a clock on v0.4.0-rc1 and rc2 (rc3 adds a read of the producer's timestamp, which Ferrata does
  not use: `lez/results/v0.4.0-rc3/README.md`): freezes and caps run on the newest Ethereum execution timestamp the
  attestation program has recorded, never on a caller's time. `Tick` and `Freeze` read a record; claims and `SetCap`
  carry no time ([EVIDENCE](../EVIDENCE.md) row 28). A freeze therefore holds while the light client stalls.
- **Freezes.** The freeze authority sets mint and burn freezes, globally or per token, at most 30 days
  after the newest attested Ethereum time (a constant of the program; on the vault the bound is its
  `maxFreeze`, §2); the admin cannot unfreeze. `Sunset` is the freeze authority's, one-way; imports stop after it.
- **Inbound depth** (since B1). The admin's `SetDepth` holds every later `ImportRoot` until the configuration's
  clock has read a record that many beacon slots newer than the import's (242 until then); at most 216,000 slots
  (`lez/harness/tests/bridge_depth.rs`).
- **Delisting** (since B1). The admin delists a token on the vault (`setDepositsEnabled(token, false)`): its
  deposits stop, and the vault writes 1 at `sha256("ferrata/vault/listing/v1" ‖ token)` (0 when relisted). Anyone
  then attests that slot on LEZ (`AttestListing`), and `token(cid, token)` records the delisting and the record it
  came from, which a LEZ-side client can read (none does yet: the `ferrata` tool reads the vault's
`depositsEnabled`); an older record never undoes a newer listing. Claims of deposits made
  before it, burns, transfers and releases are untouched, so users lose nothing and need do nothing; to end a token
  for good, delist it and let holders burn (`vault/test/Listing.t.sol`, `lez/harness/tests/bridge_delist.rs`).

Measured costs of these steps (in-process harness, the bridge program since milestone B1, `lez/results/b1/`):
`Init` 171,418 execution gas; `Register` 333,303 (433,020–456,729 on the vault's real storage); `ImportFees` 271,623;
`SetCap` 91,833; `Freeze` 140,383 (it reads the newest attested record); `Sunset` 44,695 (`bridge-inbound.md`,
`bridge-controls.md`, `bridge-vault-storage.md` there; the deployed program's, within 2,000 of these:
[`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §1, last column). All under LEZ's 5M execution target.

## 6. Pairing (F.15)

- A zone is its 32-byte channel id; LEZ v0.4 has no chain id. `configId` hashes the Logos Blockchain
  genesis, the channel id, the bridge program's header and image, the EVM chain id, the vault, the
  attestation root id and the salt ([`bridge.md`](../design/bridge.md) §3).
- **Reciprocal check**: the vault copies `lezConfigId` into slot 2; LEZ `Init` attests it. Every proof
  journal carries the EVM chain id, the vault and `lezConfigId`, so a proof made for one vault is
  worthless for another ([`vault/README.md`](../../vault/README.md), "Sepolia round trip").
- **Nullifiers per configuration** (since B1; the program the live runs of November 2026 deployed keyed them by
  `(evmChainId, vault, N)` across the whole program): inbound nullifier accounts are `nullifier(configId, evmChainId,
  vault, N)` and holdings `holder(configId, account)`, so no configuration can spend another's nullifier or fill its
  holders' accounts. A second configuration of a vault can mint only its own tokens, which no vault releases: a
  release binds the vault's own `lezConfigId` ([`bridge.md`](../design/bridge.md) §3,
  `lez/harness/tests/tenant.rs`).
- Measured refusals: `Init` of another configuration of the same vault (246); a vault-A deposit under a
  vault-B root (249); vault B's proofs for configuration A (110) ([`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §2).
- **Run once against Sepolia:** `Init` against the vault deployed there, which pins the `configId` of the
  frozen LEZ binaries: slot 2 attested at block 12,107,997, 355,935 execution gas, after the refusal
  without the block's header (215) ([`lez/live/inbound/README.md`](../../lez/live/inbound/README.md),
  "The real session"). This tree's bridge program has run `Init` only with slot 2 in a synthetic block,
  because a vault built in Foundry cannot hold a `configId` that depends on the bridge program's image
  ([`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §8a, §11).

## 7. Tenants (F.16)

Another entity deploys its own configuration under the same immutable bridge program: its own vault,
admin, freeze authority, registry, fees, caps, depths and wrapped tokens. Burn leaves bind the
configuration and the checkpoint program collects only its own, so a tenant's burns cannot be released
from another tenant's vault ([`bridge.md`](../design/bridge.md) §12.1, §15). A client selects a
configuration by its id. Evidence: the release test
`attack_burn_from_another_config_cannot_satisfy_a_release` (native); two configurations with their own vaults side by
side in the harness ([`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §2, §8); the malicious-tenant test,
`lez/harness/tests/tenant.rs`: on the bridge program since B1 a tenant's claims, holdings, roles and internal messages
reach nothing of another configuration (`lez/results/b1/tenant/`), while against the program the live runs of November
2026 deployed its two known issues stand (EVIDENCE row 136, `lez/results/tenant.md`: a plaintext claim's `s` lets its
holder freeze that deposit through a second configuration, and a filled public destination); and a second deployer's
own configuration, §11.

**Identifying and switching.** A vault pins exactly one configuration (`lezConfigId`, slot 2), so a client
identifies a configuration by its EVM chain id and vault: the `ferrata` tool reads `lezConfigId` from the vault it
is given (`--chain-id`, `--vault`, with `--bridge-account` for the LEZ program) and acts under that id only; to switch
configuration, give it another vault ([`cli/README.md`](../../cli/README.md), options).

## 8. Rehearse the whole round trip

- **By stages**: `script/roundtrip.sh status | deploy | submit | beacon | finalize | release`, then
  `script/verify-sepolia.sh <manifest>`, which also checks what must be refused (a seal with a changed
  byte, the candidate's seal as the finalisation, each release replayed). The finalisation proof cannot
  be made in advance: it echoes the nonce of a beacon recorded after the wait
  ([`vault/README.md`](../../vault/README.md), "Sepolia round trip").
- **Unattended**: `e2e/sepolia-live.sh`, resumable (a stage interrupted while sending is finished with
  forge's own record, never sent anew), stopping at the first thing not as planned
  ([`vault/README.md`](../../vault/README.md)). The same driver also runs on a
  cloud runner with a GPU function and a Groth16-wrap function (`e2e/sepolia-cloud.sh setup | start |
  status | fetch | stop`; the key lives in a cloud secret only during the run and must be a throwaway
  testnet key), and its rehearsal on a fork reached ALL CHECKS PASSED with 16 transactions
  ([`e2e/README.md`](../../e2e/README.md), "Running it in the cloud"; commit `f7b1aa7`).
- **Source verification** (`script/sourcify.sh`) is a separate step. It was run on 16 November 2026 for the three
  Sepolia deployments: every contract is an exact match on Sourcify ([`docs/sepolia.md`](../sepolia.md), EVIDENCE row
  122). `vault` mode publishes this repository's vault sources for good and runs only with
  `FERRATA_PUBLISH_SOURCES=yes`; `status` sends nothing.

## 9. Retirement is the migration mechanism (Security #8)

The vault and the checkpoint program are immutable and parse LEZ v0.4 blocks, so a configuration is
retired before a LEZ change it cannot follow (including a change of turn or slot length). The freeze
authority, or anyone 30 days plus one candidate wait after the last checkpoint progress (the deployment, the
last candidate submitted or checkpoint finalised, or the end of the last checkpoint freeze or watchers' pause,
whichever is latest: `retireOpensAt()`), calls `retire()` on the vault
(deposits stop forever) and `Sunset` on LEZ (imports stop). Holders burn and release as usual; deposits
LEZ never imported become refundable after a final sunset checkpoint and its revocation window. A new
version is a new configuration, deployed as above, that holders move to by burning and depositing
again ([`bridge.md`](../design/bridge.md) §16; [`vault/README.md`](../../vault/README.md), "Refunds").
Rehearsed over both halves (the vault on anvil, the bridge program in the harness, the guests in dev mode):
`cli/tests/both_halves.rs`, `migration_rehearsal_honours_positions_in_flight`
([`e2e/both-halves/results/migration.md`](../../e2e/both-halves/results/migration.md)).
The attestation side's migration (successor and replacement instances) is in
[`consumer-guide.md`](consumer-guide.md) §9.

## 10. What is not built or not run

- a deployment with production parameters: the round trip ran on public Sepolia on the final programs (EVIDENCE
  row 91, `crown-live-3`; the first live run, row 41, on `integration5`'s) through FerrataVault v5 with testnet
  parameters and a LEZ devnet we ran; a deployment on LEZ testnet 0.4 (which does not exist);
- `verify_checkpoint` as an API of Logos Core's Ethereum module (its execution check runs through
  `verified_proxy_module`, its beacon checks read plain HTTP);
- a deployment of the bridge program since milestone B1 (`ferrata_bridge_b1` in `docs/image-ids.md`: the nullifier
  and holder accounts keyed by the configuration, `SetDepth`, `AttestListing`): it runs in the harness and in CI's
  round trip and needs a new vault; the live runs' program keeps its two known issues (EVIDENCE row 136);
- on LEZ: the admin's own handover (an M-of-N freeze authority, its 14-day handover, watchers and the
  keyless `Sunset` are built: EVIDENCE row 74);
- a tool for the LEZ configuration's role messages: `crown-driver` puts your own accounts into `controls` (§5), but
  no shipped tool sends the admin's `ProposeRoles` (to hand the freeze authority to an M-of-N set) or a multisig's
  `Cosigned` (both built and tested in the harness, EVIDENCE row 74);
- published build outputs: the zone's bundle and the R0BF programs of §11 step 0 are built by each deployer; publishing
  them (as release assets, with their sha256) would save a deployer that hour of building and is the maintainer's
  step, not done;
- the v5 vault's registrations and deposits priced on Sepolia (only the v3 vault's were);
- `ferrata_bridge_module`'s administration methods (the read-only admin view, EVIDENCE row 57, and the
  mini-app's `SetCap` and `Freeze` on a local devnet, row 62, are built).

Sources: [`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §9; [`bridge.md`](../design/bridge.md)
§19–§20; [`attestation.md`](../design/attestation.md) §15; [`lez/BRIDGE.md`](../../lez/BRIDGE.md) §4.

## 11. Walkthrough: a second deployer's own configuration, on anvil and a local LEZ zone

This is the guide followed end to end from a fresh clone: the clone rebuilds every Ferrata program from source and
builds LEZ's sequencer and the Logos Blockchain node from their pinned sources (step 0), stands up its own, independently configured deployment, choosing its own roles, fees, token, caps and finality depth, and
reads them back from both chains (what is read from where: step 3, item 6). Everything runs on one machine: a local anvil chain (chain id 11155111, the
light client's configuration being Sepolia's) and a local LEZ v0.4.0-rc1 zone (LEZ's own sequencer and a Logos
Blockchain node, one validator). One stand-in, named: anvil has no beacon chain, so a committee whose keys the run
holds finalizes anvil's blocks for LEZ's light client (`CROWN_CHAIN=stand-in`); everything else is the deployment as
§3 describes it. Nothing is sent to any public network.

**Step 0. The tree and what it builds.** A Linux x86-64 machine with about 20 GB of free disk (on our build box,
which had 64 GB of memory and reports 2 CPUs to `nproc`, the steps below took 62 minutes and left 14 GB beside and in
the clone), Rust ([`rustup`](https://rustup.rs): each
directory's `rust-toolchain.toml` picks its version), Foundry 1.3.6, `git`, `jq`, `curl`, `zstd`, `python3`, a C and
C++ compiler with `make` (`build-essential`), `clang`, `cmake`, `protobuf-compiler`, `pkg-config`, `libssl-dev`, and
libpcsclite's development files (`sudo apt-get install libpcsclite-dev` on Debian or Ubuntu, `pcsc-lite-devel` on
Fedora): LEZ's wallet, which the `ferrata` tool links, builds keycard support (an earlier walkthrough from a fresh
clone stopped there without them).

*What was tested.* Both recorded runs used our build box, not a bare machine: Ubuntu 24.04 with Python 3.12 and the
Debian packages `curl`, `ca-certificates`, `build-essential`, `pkg-config`, `libssl-dev`, `git`, `protobuf-compiler`,
`libprotobuf-dev`, `clang`, `libclang-dev`, `llvm`, `lld`, `cmake`, `ninja-build`, `time`, `zstd`, `rsync`, `jq`,
`unzip`, `xz-utils`, `file`, `libudev-dev`, `netcat-openbsd`, `procps`, `psmisc`, `iproute2`, `bc`, `m4`, `nasm`,
`libgmp-dev`, `perl`, `sudo`, `wget`, `sqlite3`, `libsqlite3-dev` and `libpcsclite-dev`; RISC Zero 3.0.5 already in
`~/.risc0` (so the install step below added only `../risc0-home-lez`), Foundry 1.3.6 with solc 0.8.26, and a cargo
registry already filled. The list above is what the steps are known to need; on a machine with fewer packages,
install from this list what a build reports missing.

*What is built, and what is downloaded.* Beside the clone (`../upstream`, `../risc0-home-lez`, `../tmp`): every
Ferrata program, rebuilt from source and compared with [`image-ids.md`](../image-ids.md); LEZ's sequencer and the
Logos Blockchain node, built from their pinned sources. Their builds are not from source all the way down: the
Logos Blockchain crates they link take logos-blockchain-circuits v0.5.7's `-sys` crates with the `prebuilt` feature,
whose build script downloads prebuilt zk-circuit libraries (with `libgmp` and rapidsnark) from that repository's
GitHub release and, as its source says, skips checksum verification. Nothing is sent to any network but the
downloads of sources, toolchains and those prebuilt libraries:

```sh
git clone https://github.com/rainbowpuffpuff/ferrata.git && cd ferrata
scripts/fetch-upstream.sh                     # LEZ v0.4.0-rc1, rc2 and rc3 and the Logos Blockchain revision rc1 pins
RISC0_HOME_LEZ=$PWD/../risc0-home-lez scripts/ci/install-risc0.sh    # RISC Zero 3.0.5 and both guest toolchains
export PATH=$HOME/.risc0/bin:$PATH                                     # r0vm, as rzup installs it
(cd vault && forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 \
  risc0/risc0-ethereum@v3.1.0 && FOUNDRY_OFFLINE=false forge build)   # the vault's pinned libraries and solc 0.8.26
# The final programs, rebuilt and compared with docs/image-ids.md (any path gives the same ids):
scripts/reproduce-image-ids.sh build --out ../tmp/programs && scripts/reproduce-image-ids.sh compare --out ../tmp/programs
# LEZ's sequencer and the Logos Blockchain node (into ../tmp/crown/live), then the zone's bundle (lez/crown/build.sh):
FERRATA_LIVE_JOBS=$(nproc) FERRATA_LIVE_NICE=0 FERRATA_LIVE_TMP=$PWD/../tmp/crown/live bash lez/live/run.sh build-sequencer
FERRATA_LIVE_JOBS=$(nproc) FERRATA_LIVE_NICE=0 FERRATA_LIVE_TMP=$PWD/../tmp/crown/live bash lez/live/run.sh build-bedrock
CROWN_PROGRAMS=$PWD/../tmp/programs/programs bash lez/crown/build.sh all
export SEQNET_BUNDLE=$PWD/../tmp/crown-lez/bundle.tar.zst SEQNET_R0BF=$PWD/../tmp/programs/r0bf
```

`compare` must print a match for every program: the zone then runs exactly the programs of
[`image-ids.md`](../image-ids.md) (`ferrata_lez.bin` `0x816d1fef…`; `ferrata_bridge.bin` since milestone B1 is
`ferrata_bridge_b1`, `0x847a4336…` since ADR 0032, `0xac1fb59b…` before; the checkpoint and release programs the vault pins). The recorded runs, at
`6a191ce0` and `26ad3564`, predate B1 and ran the deployed `0xccde6171…`. The sequencer and the node need not match anyone's bytes: no image
id names them. (The recorded runs before 21 November used the live run's bundle instead, which is not published:
§10.)

**Step 1. Your settings.** The run below used these; choose your own.

| Setting | Variable | In the recorded run | Fixed in |
|---|---|---|---|
| The vault's admin, after the handover | `SD_ADMIN` | `0x5ec0d000000000000000000000000000000ad001` | two-step handover (`transferAdmin`, `acceptAdmin`) |
| The vault's freeze authority (never the admin) | `FERRATA_FREEZE_AUTHORITY` | `0x5ec0d000000000000000000000000000000f2001` | replaced only by the admin's proposal, 14 days later |
| Who receives protocol fees | `FERRATA_FEE_RECIPIENT` | `0x5ec0d000000000000000000000000000000fee01` | vault immutable |
| Watchers (up to three) | `FERRATA_WATCHERS` | `0x5ec0d000000000000000000000000000000a7001,0x5ec0d000000000000000000000000000000a7002` | admin's proposal, 14 days later |
| Finality depth outbound: the candidate's wait Δ (rule B), s | `SEQNET_DELTA` (passed to `FERRATA_DELTA`) | 5,400 | `deltaFloor` immutable; `delta` raise-only |
| Release delay per token, its floor, the revocation window, s | `FERRATA_RELEASE_DELAY`, `FERRATA_MIN_RELEASE_DELAY`, `FERRATA_REVOKE_WINDOW` | 600; 300; 172,800 | immutable floor and window; the admin sets the delay within them |
| The LEZ configuration's admin and freeze authority | `CROWN_ADMIN`, `CROWN_FREEZE_AUTHORITY` (account ids of keys you hold); unset: two fresh random keys, made by the run and kept in its work directory (a rerun under the same name deletes that directory and makes new ones: copy the keys out first) | unset: fresh random keys (the walkthrough before 21 November used `CROWN_ROLES_LABEL=deployer-two`, keys anyone can derive: §5) | inside `configId` |
| LEZ mint and burn caps, per token and global, value units a day | `CROWN_TOKEN_CAP`, `CROWN_GLOBAL_CAP` | 3,000,000,000; 9,000,000,000 | inside `configId`; the LEZ admin sets caps above the floor |
| Inbound finality depth | — (the walkthrough keeps the default) | 0: Ethereum finality | the admin's `SetDepth` on LEZ, for later imports only (§2; since B1) |

```sh
export SD_ADMIN=0x5ec0d000000000000000000000000000000ad001
export FERRATA_FREEZE_AUTHORITY=0x5ec0d000000000000000000000000000000f2001
export FERRATA_FEE_RECIPIENT=0x5ec0d000000000000000000000000000000fee01
export FERRATA_WATCHERS=0x5ec0d000000000000000000000000000000a7001,0x5ec0d000000000000000000000000000000a7002
export SEQNET_DELTA=5400 FERRATA_RELEASE_DELAY=600 FERRATA_MIN_RELEASE_DELAY=300 FERRATA_REVOKE_WINDOW=172800
export CROWN_TOKEN_CAP=3000000000 CROWN_GLOBAL_CAP=9000000000
# Your own LEZ admin and freeze authority, if you hold their keys elsewhere (otherwise the run makes two fresh ones):
# export CROWN_ADMIN=0x<64 hex digits> CROWN_FREEZE_AUTHORITY=0x<64 hex digits>
```

**Step 2. Run it.** One command builds the tools from this clone (the vault, `ferrata`, `crown-driver` and the
checkpoint tools) and stands up the deployment; its records go to `e2e/runs/seqnet-NAME/`:

```sh
bash e2e/seqnet/run.sh deployer second-deployer-2
```

**Step 3. What it does, in §3's order** ([`drive.sh`](../../e2e/seqnet/drive.sh), mode `deployer`, then
[`hook-deployer.sh`](../../e2e/seqnet/hook-deployer.sh)):

1. **LEZ: the programs and an instance.** A new zone; the attestation and bridge programs deployed through LEZ's
   program loader, immutable, checked to be the final images; an attestation instance (`lez/crown/session.sh a`).
2. **`configId`**, from the zone's identity, the predicted vault address and your LEZ roles and caps
   (`crown-driver config-id`; the roles are `CROWN_ADMIN` and `CROWN_FREEZE_AUTHORITY`, or the accounts of two fresh
   random keys, `crown-driver new-role-keys`, kept in the run's work directory and never in its records; a rerun
   under the same name replaces them).
3. **EVM: the vault**, by `vault/script/deploy.sh` (§4) in mode (B), one transaction per run: RISC Zero's Groth16
   verifier, the emergency stop, the library, the vault with your freeze authority, fee recipient, watchers, Δ,
   revocation window and release delays; ETH and the test token registered; the stop's owner renounced; the checks.
4. **A smoke test and the LEZ configuration**: one deposit with the `ferrata` tool, the vault captured, then on the
   zone `Init` of your configuration (it attests that the vault's slot 2 holds your `configId`, so it succeeds only
   if the LEZ roles and caps are the ones the vault pinned), ETH registered from the vault's registry, the deposit's
   root imported (`crown-driver crown-b --operator-only`). These come before step 5 because the LEZ `Register`
   attests the vault's fee schedules as they stand at the record; a schedule announced later reaches LEZ through
   `ImportFees` once it is in force.
5. **Your token, caps and fees, as the vault's admin**: a token of your own deployed and registered with your
   denominations (50 and 500, 6 decimals), relay fees (0.5%), checkpoint fees (0.2%), caps (5,000 a day each way) and
   release delay; ETH's deposit and release caps (1 ETH a day) and a global deposit cap; a fee schedule for ETH
   announced (mint and burn fees 0.1%, checkpoint fees 0.2%), seven days of anvil's clock let pass, applied; the admin
   role handed to your address in two steps.
6. **Everything read back** and compared with what you chose: `checks.txt`, one PASS or FAIL per setting; the run
   fails on any FAIL. The vault's settings are read from anvil (`cast call` on its getters, `feeState` for the
   schedule in force). The LEZ controls are compared as `crown-driver config-id` computes them: they reach the zone
   as `configId`, a hash over them that the vault pins (`lezConfigId`, read from anvil) and under which the zone
   accepted `Init` (`ferrata status` on the zone); the global cap is also read from the zone itself.

**Step 4. Look at your deployment** with the admin view ([`admin/index.html`](../../admin/index.html), or
`node admin/dump.mjs http://127.0.0.1:8545 <vault> <deployment block>`) while anvil runs, and `ferrata status`.

**Recorded:** [`runs/2026-11-21-second-deployer/`](runs/2026-11-21-second-deployer/README.md): fresh clones of the
repository on the build box (the machine as *What was tested* says); the first with the live run's bundle placed beside the clone and label-derived LEZ roles,
the second with step 0 as written here and fresh random LEZ roles.

**What this walkthrough does not show:** a public network (Sepolia and LEZ testnet 0.4: the live round trips of
EVIDENCE rows 41, 91, 142 were the owner's deployments); real proofs (none is needed to stand a deployment up); your
own token registered on the LEZ side (the same `Register` message attests its vault entry as it does ETH's: §3 step 7);
the LEZ import of the new fee schedule (`ImportFees`, once in force on the record's clock); the LEZ admin's own
`SetCap` and the freeze authority's `Freeze` (built and tested in the harness, EVIDENCE row 74).
