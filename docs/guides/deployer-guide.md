# Deploying Ferrata: the programs on LEZ and the vault on an EVM chain

**Deployer guide (RFP-022 R.14, F.15, F.16, Security #8; RFP-021 VS.3). First draft, 11 November
2026.** For an entity standing up its own, independently configured deployment: which decisions to
take before anything is sent, in which order the pieces are deployed and paired, how gas limits are
set on a chain with Glamsterdam's state gas, how tokens, fees, the admin and freeze authorities and
the finality depth are configured, and how to rehearse. Every statement cites its source; paths are
relative to the repository root.

**Status.** Nothing has been deployed for production. What was run: the vault's deployment script, on
local forks of Sepolia (one transaction per run, limits from the node); the whole return leg, on forks
and as a cloud rehearsal; the LEZ programs, through LEZ's own program loader on a local LEZ v0.4.0-rc1
sequencer. Sent to public Sepolia: the first attempt's first transaction, which ran out of gas (§4); then,
on 11 November 2026, the second run's deployment of the v3 vault of the end-to-end fixture and its
deposit 0, and nothing after it ([`e2e/runs/sepolia-live-2/README.md`](../../e2e/runs/sepolia-live-2/README.md);
the hashes in [`lez/live/inbound/README.md`](../../lez/live/inbound/README.md), "On Sepolia"). The code is not audited
([`vault/README.md`](../../vault/README.md); [`README.md`](../../README.md), "Not audited").

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
| Wait: `deltaFloor` and initial `delta` (Δ) | (A): 3 h 01 min; (B): 1 h | 600 s (`FERRATA_DELTA`) | `deltaFloor` immutable; `delta` raise-only, ≤ 30 days |
| `turnSeconds` (the channel's posting timeframe) | 900 s on testnet values | 900 | vault immutable |
| `revokeWindow` | 7 days | 1 day | immutable |
| `maxFreeze` | 30 days | 30 days | immutable |
| `minReleaseDelay`; per-token release delay | 24 h (0 on testnets); delay between it and `revokeWindow` | 0; 120 s | immutable; admin per token, for roots finalised later |
| Admin | a multisig of the deployer | the deployer | two-step handover |
| Freeze authority | a multisig, never the admin | an address without a key unless `FERRATA_FREEZE_AUTHORITY` names one | replaced only by the admin's proposal, 14 days later |
| Watchers | up to three hot addresses, each with its own nodes | none | admin's proposal, 14 days later |
| Fee recipient | the deployer's choice | the deployer | vault immutable |
| Tokens, denominations, relay and checkpoint fees, `lezUnit`, `unitWeight` | three denominations per token; fees ≤ 1% each | ETH 0.001/0.01/0.1, relay 0.2%, checkpoint 0.1%; test token 100/1,000/10,000, 0.25% and 0.1% | write-once per token |
| Caps per token and day | the smaller of 10% of escrow and a deployer-set ceiling, above an immutable floor | UNLIMITED | admin, never below the floor |
| Attestation instance (its root id) | an instance whose checkpoint you verified | the Sepolia period-1378 bootstrap | inside `configId` |
| Inbound finality depth | Ethereum finality, no extra epochs | — | — (deeper settings: not built) |

Sources: [`bridge.md`](../design/bridge.md) §9, §13; [`vault/README.md`](../../vault/README.md),
"Deployment (v5)", "Roles", "Sepolia round trip"; [`lez/live/inbound/README.md`](../../lez/live/inbound/README.md).

**Finality depth (R.14, F.12).** Inbound, the default is Ethereum finality with no extra epochs; a
deeper setting delays honest claimants but not a captured sync committee, whose defence is caps and
freezes ([`bridge.md`](../design/bridge.md) §5.4). Outbound, the depth is the vault's wait (Δ, plus
`(t − 1) × turnSeconds` under (A)) and the per-token release delay; a change applies only to candidates
submitted, or roots finalised, after it ([`vault/README.md`](../../vault/README.md), "LEZ checkpoints",
"Releases and pull payments").

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
  is not.
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
  LEZ programs cannot read a clock: claims, `Freeze`, `SetCap` and `Tick` carry the caller's time `t`
  under a validity window `[t, t + 10 min)`, which a sequencer can stretch within its own limits.
- **Freezes.** The freeze authority sets mint and burn freezes, globally or per token, at most 30 days
  ahead; the admin cannot unfreeze. `Sunset` is the freeze authority's, one-way; imports stop after it.

Measured costs of these steps (in-process harness, this tree's program): `Init` 171,027 execution gas;
`Register` 331,816 (431,512–455,116 on the vault's real storage); `ImportFees` 269,684; `SetCap` 91,019;
`Freeze` 140,609 (it reads the newest attested record); `Sunset` 44,193
([`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §1, last column; `lez/results/bridge-*.md`). All under LEZ's 5M
execution target.

## 6. Pairing (F.15)

- A zone is its 32-byte channel id; LEZ v0.4 has no chain id. `configId` hashes the Logos Blockchain
  genesis, the channel id, the bridge program's header and image, the EVM chain id, the vault, the
  attestation root id and the salt ([`bridge.md`](../design/bridge.md) §3).
- **Reciprocal check**: the vault copies `lezConfigId` into slot 2; LEZ `Init` attests it. Every proof
  journal carries the EVM chain id, the vault and `lezConfigId`, so a proof made for one vault is
  worthless for another ([`vault/README.md`](../../vault/README.md), "Sepolia round trip").
- **One nullifier namespace per vault**: inbound nullifier accounts are keyed by `(evmChainId, vault, N)`
  across the whole bridge program, so two configurations importing one vault's roots cannot both mint
  against a deposit ([`bridge.md`](../design/bridge.md) §3).
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
configuration by its id. Evidence today: the release test
`attack_burn_from_another_config_cannot_satisfy_a_release` (native), and two configurations with their
own vaults side by side in the harness ([`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §2, §8).
The malicious-tenant test (milestone B2's gate) is **not built** ([`docs/traceability.md`](../traceability.md),
row F.16).

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
- **Source verification** (`script/sourcify.sh`) is a separate step and was **not run**; `vault` mode
  publishes this repository's vault sources for good and runs only with `FERRATA_PUBLISH_SOURCES=yes`.

## 9. Retirement is the migration mechanism (Security #8)

The vault and the checkpoint program are immutable and parse LEZ v0.4 blocks, so a configuration is
retired before a LEZ change it cannot follow (including a change of turn or slot length). The freeze
authority, or anyone once no checkpoint has finalised for 30 days, calls `retire()` on the vault
(deposits stop forever) and `Sunset` on LEZ (imports stop). Holders burn and release as usual; deposits
LEZ never imported become refundable after a final sunset checkpoint and its revocation window. A new
version is a new configuration, deployed as above, that holders move to by burning and depositing
again ([`bridge.md`](../design/bridge.md) §16; [`vault/README.md`](../../vault/README.md), "Refunds").
The attestation side's migration (successor and replacement instances) is in
[`consumer-guide.md`](consumer-guide.md) §9.

## 10. What is not built or not run

- a deployment with production parameters: the round trip ran on public Sepolia on the final programs (EVIDENCE
  row 91, `crown-live-3`; the first live run, row 41, on `integration5`'s) through FerrataVault v5 with testnet
  parameters and a LEZ devnet we ran; a deployment on LEZ testnet 0.4 (which does not exist);
- `verify_checkpoint` through Logos Core's `verified_proxy_module` (it reads plain HTTP); the `ferrata_attestor`
  module (rule (B)'s sequencer side; the program is built);
- on LEZ: the admin's own handover (an M-of-N freeze authority, its 14-day handover, watchers and the
  keyless `Sunset` are built: EVIDENCE row 74);
- deeper inbound finality settings; the malicious-tenant test;
- the v5 vault's registrations and deposits priced on Sepolia (only the v3 vault's were);
- source verification on Sourcify;
- `ferrata_bridge_module`'s administration methods (the read-only admin view, EVIDENCE row 57, and the
  mini-app's `SetCap` and `Freeze` on a local devnet, row 62, are built).

Sources: [`lez/BRIDGE-RESULTS.md`](../../lez/BRIDGE-RESULTS.md) §9; [`bridge.md`](../design/bridge.md)
§19–§20; [`attestation.md`](../design/attestation.md) §15; [`lez/BRIDGE.md`](../../lez/BRIDGE.md) §4.
