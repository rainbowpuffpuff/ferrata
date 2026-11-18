# Ferrata bridge: design note (RFP-022)

This note describes Ferrata's bridge between Ethereum and the Logos Execution Zone (LEZ): an immutable vault on Ethereum, an immutable bridge program on LEZ, and the proofs between them. The inbound leg reads Ethereum through the RFP-021 attestation primitive, which has its own design note and is not repeated here. LEZ and Logos Blockchain facts are cited from the repository's LEZ v0.4 findings note, `docs/design/lez-v0.4-findings.md`, by section ("findings §n"). The analysis of finality rule (A) is in `docs/design/finality-rule/MODEL.md` (the behaviour of the v0.4 code, with source citations), `docs/design/finality-rule/LEMMA.md` (proofs and the counterexample) and `docs/design/finality-rule/RESULTS.md` (simulations, including the v3 experiments (i) to (k)).

The design is specification v2 ([`spec/bridge-spec-v2.md`](spec/bridge-spec-v2.md), cited as SPEC-v2) as amended by specification v3 ([`spec/bridge-spec-v3-changes.md`](spec/bridge-spec-v3-changes.md), changes D1 to D13; D13, of 10 Nov 2026, is the newest and overrides earlier items where they differ). Requirement IDs are RFP-022's (F.x, U.x, R.x, P.x, Security #n, Privacy #n, soft #n); `021:` marks RFP-021's.

**Labels.** Every number carries one:
- **measured**: from a run reproducible from the repository, with where it ran (Python simulator, local EVM under Foundry, RISC Zero executor or prover on a laptop CPU, mock or real verifier);
- **derived**: computed from measured numbers, with the inputs given;
- **estimated**: from a model or a component measurement, not from a run of the thing itself;
- **not built**: specified, not implemented.

All hashes are SHA-256 unless stated. `‖` is byte concatenation; integers are big-endian at the stated width; `HMAC` is HMAC-SHA-256; tags are the ASCII bytes of the quoted string; `abi.encode` is Solidity's encoding of static types, one 32-byte word per field.

## 1. Purpose, scope, and what is built

The bridge moves registered ERC-20 tokens and ETH to LEZ and back in fixed denominations, so that an observer of both chains cannot tell which deposit paid which mint, or which burn which release.

- **Inbound** (deposit to mint) rests on Ethereum's sync committee, through the attestation primitive, and nothing else.
- **Outbound** (burn to release) rests on the honesty of the zone's accredited sequencers, because LEZ v0.4 offers an outside verifier nothing stronger (§7). Two finality rules are specified: rule (B), final-tier attestations, is the proposed rule; rule (A), inferred support with a beacon and a wait, is the fallback if the sequencers will not run an attestor (§9). §11 states rule (A)'s trust exactly; §21, what would remove the trust in the sequencers altogether.

| Component | State |
|---|---|
| Vault, specification v3 (vault v5) | Prototype: D3 to D5, D9, and the vault items of D11 and D13 (both rules behind an immutable `mode`, watchers, the 14-day role change, keyless retirement, the cap floor, D13's fees) with a linked library; its own suite mocks proofs behind RISC Zero's unmodified emergency-stop wrapper; the return leg (rule (A)) and real rule (B) checkpoint proofs were verified with RISC Zero's real Groth16 verifier (§10.3, §18.7, §18.8); not audited; not deployed (the v3 vault of the end-to-end fixture is on Sepolia with one deposit, `lez/live/inbound/README.md`); gas measured (§18.1, §18.7, `vault/GAS.md`) |
| Vault, specification v2 | Prototype built; the release proof v2 was verified against it with the real Groth16 verifier (§18.2) |
| `ferrata-bridge-core` v2 (trees, commitments, nullifiers, statements, recovery) | Built and tested, SPEC-v2 encodings; without `mac` v2 and `NOut` v2 |
| Release guest v2 | Built; proved on a laptop CPU, verified on a local EVM against the v2 vault with the real verifier and, unchanged, against the v3 vault under a burn root finalised by real checkpoint proofs (§18.7) |
| Claim guest (the bridge's part of a claim) | Built for measurement; not LEZ's own claim circuit |
| Checkpoint program | Prototype: SPEC-v2 §4.1–4.2 with D3, rule (A) with cycles measured; two real proofs of a small checkpoint (§18.4); a candidate and a finalisation proof of the end-to-end fixture accepted by the v3 vault (§18.7). Rule (B) of D7 and D13 built in its own guest, with the attestor's core; cycles measured against rule (A) on the same blocks (§18.5) |
| Rule (A) analysis | Written proof and simulation (`docs/design/finality-rule/`) |
| Bridge program on LEZ | Prototype (`lez/BRIDGE-RESULTS.md`): pairing, the registry attested from the vault, `ImportRoot`, plaintext and private claims, burns, fees by leaf index, caps, freezes, roles and `Sunset`; since branch `bridge-controls` the keyless `Sunset`, the 14-day handover, a multisig freeze authority, watchers and transfers between holders (§14 there); run in the in-process harness (private paths in dev mode); real proofs of a private claim and a private burn of an earlier image (§7 there); an earlier, frozen image ran on a live LEZ sequencer, on a stand-in chain and for deposit 0 of the Sepolia vault (§11 there) |
| Command-line tool `ferrata` (`cli/`) | Prototype: keys, deposit, scan, recover, claim, burn, release, status; one end-to-end test against a local anvil vault (mock verifier) and the harness (dev mode) |
| Relayer daemon `ferrata-relayer` (`cli/`: rule (B) checkpoints, releases, wrapping, fees; the gas-drop receipt) and the watchers' `monitor` (`monitor/`) | Prototypes ([EVIDENCE](../EVIDENCE.md) rows 43–48): a local chain and an anvil fork of Sepolia, real proofs for the relayer |
| The gas drop: a private claim sweeping the relayer's sealed credit and paying its fee from it | Measured on LEZ v0.4.0-rc3 with real proofs ([EVIDENCE](../EVIDENCE.md) row 124; M0 check 2 GO); the sealed credit in the daemon `ferrata-relayer` and for burns not built |
| The relayer's root imports on a running sequencer (on the devnet: EVIDENCE rows 106–107; the attestor and `monitor` are built as programs and as Logos modules, rows 94–96) | Not built |

## 2. Overview

```
 Ethereum                                         LEZ v0.4
 FerrataVault (immutable, per configuration)      bridge program (immutable, shared by configurations)
   deposit(token, denomIndex, C, j)
     └─ leaf ─► deposit tree: root slot 0,  ───►  ImportRoot (attests slots 0 and 1 under one record)
                leaf count slot 1                    └─ root(n) with its leaf count, write-once
                                                   Claim: path to root(n), leafIndex < count, opening of C, N
                                                     └─ mint one denomination
                                                   Burn: destroy one denomination, publish burn note
   submitCandidate ◄── checkpoint proof ◄────────  LEZ blocks, channel inscriptions,
                                                   and under rule (B) final-tier attestations
   rule (A) only: recordBeacon (prevrandao) ── echo nonce ──► echo transaction to bridgeHeader
   finalize ◄── rule (A): second proof; rule (B): after the wait
     └─ burnRoot accepted
   release ◄── release proof (burn leaf under burnRoot, opening of COut, NOut)
```

Burns live only in LEZ blocks; Ethereum holds deposits, checkpoints, burn roots and used `NOut`.

## 3. Identities and pairing (F.15)

LEZ v0.4 has no chain id; a zone is its 32-byte channel id, and one program image can be deployed at several headers (findings §7). The bridge binds every identity itself:

```
configId = H("ferrata/config/v1" ‖ bedrockGenesisId:32 ‖ channelId:32 ‖ bridgeHeader:32 ‖ bridgeImageId:32
             ‖ evmChainId:u256 ‖ vault:20 ‖ attestationRootId:32 ‖ salt:32)
```

**Reciprocal check.** The vault copies `lezConfigId` into slot 2; the LEZ configuration's `Init` attests slot 2 and refuses to initialise unless it equals its own `configId`. Every journal carries `evmChainId`, the vault and `lezConfigId`.

**Deployment order (D2).** The vault address is predicted from the deployer's address and nonce (plain CREATE); CREATE2 is not usable, because the init code contains `lezConfigId`, which contains the vault address. `Init` runs once the deployment is final and recorded.

**Beacon chains.** Rule (A) needs an unpredictable `prevrandao` (Ethereum and its testnets). On an EVM chain whose `prevrandao` or timestamp is constant or sequencer-chosen, the same vault is deployed in rule (B), which needs no beacon (D13).

**One nullifier namespace.** Inbound nullifier accounts are keyed by `(evmChainId, vault, N)` across the whole bridge program, not per configuration, so two configurations importing one vault's roots cannot both mint against a deposit (R.3, Security #6). The same key lets a second configuration spend the paired one's nullifiers: see the known issues below. The program's second build (not deployed) keys them by `configId` as well: a deposit then mints at most once per configuration, and only the configuration a vault pins is ever paid by it.

### Known issues, 18 November 2026

An independent review of the bridge's LEZ v0.3.1 port found two properties that the deployed v0.4 bridge program (`0xccde6171…71dd`, the image the live Sepolia vault's `configId` commits to) shares. The malicious-tenant test `lez/harness/tests/tenant.rs` runs both through LEZ v0.4's state machine with that program and records what happens ([EVIDENCE](../EVIDENCE.md) row 136; figures in [`lez/results/tenant.md`](../../lez/results/tenant.md)).

**R1: a second configuration of the same vault can freeze a public-kind deposit.** A configuration names its attestation program and instance freely, and the nullifier account is keyed by `(evmChainId, vault, N)` without `configId`. Anyone can create an instance of the attestation program from a checkpoint of their choosing (its `Init` is open to anyone; in the test the instance's record is placed as the program writes one), and with it a configuration B that copies the paired configuration A's fields except the instance. B's records say whatever its creator's committee signs, so B pairs (slot 2 = `configId(B)` in its own records), imports a root holding the vault's real leaves, and claims any public-kind deposit whose `s` it knows. A's claim of that deposit is then refused with 250 (`ALREADY_CLAIMED`) and its sender pays its gas (213,992 execution gas in the test); A's other deposits still claim.
- *Who can do it:* whoever knows the deposit's `s`: its depositor, and anyone who sees a plaintext claim before it is included, since a plaintext claim carries `s` (the test front-runs one: B's copy is ordered first, the owner's own claim is refused with 250). A private claim does not publish `s` (`bridge_private.rs` searches its transaction for it); it is the CLI's default path, and the plaintext path needs `--i-accept-plaintext-links-deposit` (`cli/src/flow.rs`; that refusal has no test of its own). A private-kind deposit cannot be taken: without `s` no leaf opens (249), and even with it the plaintext path refuses a private-kind deposit and a private claim must run as the destination account itself (252 on both).
- *What it costs them:* B's `Init`, `Register` and `ImportRoot` once (728,087 execution gas in the test), then one claim per deposit (257,798), plus keeping their own attestation instance.
- *What is and is not at risk:* no theft and no double mint. B's mints are holdings and burn records under `configId(B)`, which no vault pins: the vault releases only against the burn root of a finalised checkpoint and a release digest that binds its own `lezConfigId` (`FerrataVault.sol`, `_checkRelease` and `_releaseJournalDigest`). The deposit is frozen: it can no longer be claimed under A, and `refund` pays only leaves at or above the sunset checkpoint's imported count (§16), which counts A's imports only (`checkpoint/crates/checkpoint-core/src/rule.rs`), so once A has imported a root covering the deposit, which every claim under A needs, its escrow stays in the vault for good. A frozen depositor gets a refund only if the vault is retired and sunset before A ever imports that leaf. Its destination holds B's tokens instead, which nothing redeems.

**R2: a destination's holder account can be filled.** `holder(dest)` keeps the public balances of `dest` under every configuration and token in one list, which every mint, burn and transfer there decodes, searches and re-encodes. LEZ v0.4 has no limit on an actor's state, so the review's v0.3.1 figure (1,505 holdings, then a panic at the 100 KiB shard limit) does not apply: in the test a claim to a holder of 1,505 other holdings applies (2,683,111 gas). The list instead grows until a transaction touching it exceeds 10,000,000 execution gas (`MAX_GAS_EXEC`, the most a block can use). Measured: a claim to a holder of 6,048 other holdings applies (9,999,010 gas); with 6,049 it fails with no exit code, charged the whole 10,000,000 (a base fee of 80,016,424 against 2,102,624 for the same claim to an empty holder), and nothing is minted. Anyone can add holdings, one per (configuration, token) pair, through claims to `dest` under configurations they create; reaching 6,049 costs about 3.3 × 10¹⁰ execution gas (derived: each claim 260,775 gas plus 1,610 per holding already there, and one `Register` of 331,816 per pair), at least 3,304 blocks at their maximum. After that no claim to `dest` applies, under any configuration, so a public-kind deposit to `dest` (its destination is fixed in its commitment) stays unclaimed while the list is that long. The owner can still burn from it (measured up to 6,109 other holdings), at about 10,000,000 gas a transaction. Not tested: whether transfers into `dest`, whose other steps cost less than a claim's, can push the list past 6,109, where the owner's burns fail too. Private accounts' balances are not in `holder(dest)`.

**F1 (the red team, 18 November): `Init` accepts a freeze authority that is the admin.** The deployed program checks
at `Init` only that the caps are above the floor, so it writes a configuration whose live freeze authority is its admin,
although SPEC-v2 §7 says the two must differ, `ProposeRoles` refuses the admin as a member and the vault refuses the
pair at construction (`RolesMustDiffer`). In such a configuration the admin can freeze and run `Sunset`
([`docs/trust.md`](../trust.md) §4). Bounded: `configId` commits to the controls, so the roles are visible before anyone
deposits ([`docs/reviews/redteam-2026-11-18.md`](../reviews/redteam-2026-11-18.md), F1, vector BR-INIT-05).

**The planned change.** The nullifier account keyed by `configId` as well, so a second configuration spends only its own nullifiers (only the paired configuration's mints are ever paid by its vault, so nothing is lost by separating them); a holder account per configuration and destination; an oversized state refused with a code. It changes the bridge program's image id, so it belongs to milestone B1, with the B2 gate's malicious-tenant test kept as its regression test. The LEZ v0.3.1 port already has this change (branch `testnet-v03-bridge` at `ff453dcc`: `lez-v03-bridge/core/src/lib.rs`, its regression tests `harness/tests/review_regressions.rs`).

**Prototyped as a second build on 18 November, not deployed; the deployed program still has both, and F1, until B1 deploys it with a new vault.** `lez/guests/bridge-next` (image `0xc5cad2cd…d6da`, [`docs/image-ids.md`](../image-ids.md), its own heading) is the deployed program with `nullifier(cid, chain, vault, N)` and `holder(cid, account)` (bridge-wire's cargo feature `config-keyed`, off by default; every other line is the deployed program's). `holder(cid, ·)` is credited only from `nullifier(cid, …)` and `token(cid, token)`, so it holds at most one holding per token registered under `cid`; the oversized-state code is not needed on v0.4, which has no state limit. Through LEZ v0.4's state machine ([`lez/results/bridge-next.md`](../../lez/results/bridge-next.md), [EVIDENCE](../EVIDENCE.md) row 143): R1, after the rogue configuration's claim of a public-kind deposit, the paired configuration's claim of it **applies** (264,728 execution gas; refused with 250 on the deployed program), a second claim of it under A is refused (250), and the rogue's mints and burns stay under its own `configId`, which no vault pins; a plaintext claim copied and ordered first no longer blocks its owner's (applied, 290,427). R2, a claim to a destination costs 260,855 gas with 0, 1,505, 6,049 and 12,098 other configurations' holdings for it (each in its own account), and its owner's burn 165,055 with 0, 6,110 and 12,098; on the deployed program the claim fails at 6,049 and the burn at 6,110. Every bridge suite passes against it (20 tests in the eight `bridge_*.rs` suites with F1's, the privacy suite's LEZ half, the three tenant tests), and the 14 deployed guests rebuild byte-identical in fresh target directories (14 of 14; 6 of 6 against the live vault's immutables). Since the evening of 18 November it also refuses F1's `Init` with 240, the code of `ProposeRoles` (the red team's BR-INIT-05 re-run: accepted by the deployed program, refused by the second build; regression test `bridge_secfix.rs`, `f1_init_refuses_the_admin_as_freeze_authority`); that check moved its image id from `0x7c7d2b3a…1e2f` and `Init`'s cost by +79 execution gas, the 14 deployed guests again rebuilt byte-identical from no target directory. B1 deploys it with a new vault, whose `configId` commits to its image id, and drives it from the CLI, the relayer and the bridge module, which still compute the deployed program's addresses.

## 4. Keys

Derived from the LEZ wallet seed under labels disjoint from LEZ's own key tree:

| Key | Derivation | Derives | Lets its holder |
|---|---|---|---|
| `kAuth` | `HMAC(bridgeSeed, "ferrata/auth/v1")` | deposit secrets `s`, burn secrets `sOut` | claim and release |
| `kView` | `HMAC(bridgeSeed, "ferrata/view/v1")` | burn tags `r_i`, memo pads, burn MACs | find a holder's burns and read their recipients |

A `kView` holder sees burns and recipients but cannot claim or release (soft #2). It does not see deposits or claims (D12), so soft #2 is met outbound only.

## 5. Inbound: deposit to mint (F.1–F.4, F.9, F.17, R.1, R.3–R.5)

### 5.1 Vault storage and deposits

One immutable contract per configuration, without proxy, settable verifier or rescue function. Slot 0 holds `depositRoot`, slot 1 `nextLeafIndex`, slot 2 `lezConfigId`.

`deposit(token, denomIndex, C, j)` and `depositETH(denomIndex, C, j)` escrow exactly one registered denomination plus the registered mint and relay fees (§13). ERC-20s move through `SafeERC20` with a balance check, which accepts tokens that return nothing (USDT, F.17) and rejects fee-on-transfer tokens. Plain ETH is refused. `j` must equal `depositCount[msg.sender]`, which gives every deposit a public `(depositor, j)` pair. After `retire()`, deposits revert forever.

### 5.2 Deposit tree and leaf

Incremental, depth 32:

```
LEAF_TAG = H("ferrata/vault/leaf/v2")
leaf     = H(0x00 ‖ LEAF_TAG ‖ leafIndex:u64 ‖ depositor:20 ‖ token:20 ‖ amount:u256 ‖ C:32)
node     = H(0x01 ‖ left ‖ right)            empty leaf = 32 zero bytes
```

**Leaf index range (D9).** `leafIndex < 2^32` everywhere: the vault refuses a deposit once the tree is full, the core refuses an accumulator-leaf fact with an index of 2^32 or more, and the claim recomputes the leaf from a preimage that binds the claimed index. Otherwise indices `i` and `i + 2^32` would share a path and give one leaf two identifiers and two nullifiers. The only thing a deposit says about LEZ is `C` (F.2).

### 5.3 Commitment and nullifier

```
s   = HMAC(kAuth, "ferrata/s/v1" ‖ evmChainId ‖ vault ‖ depositor ‖ j:u64)
C   = H("ferrata/C/v1" ‖ s ‖ kind:u8 ‖ dest:32)        kind 0x01 private account, 0x02 public account
fid = H("ferrata/fact/acc-leaf/v1" ‖ evmChainId ‖ vault ‖ slot:u256 (= 0) ‖ leafIndex:u64 ‖ leaf)
N   = H("ferrata/nf/v1" ‖ s ‖ fid)
```

`C` hides the destination and binds it and its kind, so nobody can redirect a claim (Security #3). `dest` is always an account derived from the same seed (account index `a`, gap limit `g`).

**Why `N` is keyed on `fid`.** `fid` names "leaf `leafIndex` of the accumulator at slot 0 of `vault`" and contains no root. One deposit lies under every later root, so a nullifier keyed on a root would allow one mint per root (core test `attack_same_deposit_under_two_roots_gives_same_nullifier`). `N` contains `s`, so it cannot be computed from public data. `evmChainId` and `vault` are configuration constants, never claimant inputs.

### 5.4 Root import

Anyone sends `ImportRoot`: the bridge program calls the attestation primitive's direct path for **slots 0 and 1** of the vault (root and leaf count) under one finalised record, inside its own transaction, and writes a write-once `root(n)` account that stores the root with its leaf count (D13). The writer is the bridge's own immutable code, so reading a root trusts no third party (021: F.7; F.3). Daemons import on a fixed cadence; a client never imports for itself.

The imported leaf count is what makes "claimable" and "refundable" complementary: a claim refuses `leafIndex >= count`, and `importedLeafCount` in the checkpoint state (§8.1) is that count, which is the refund boundary after retirement (§16). Inbound finality is Ethereum finality by default. A deeper setting delays honest claimants but not a captured sync committee; the defence there is caps and freezes (F.12).

### 5.5 The claim

A claim checks the Merkle path from `leaf` to an imported root (direction bits equal to `leafIndex`), that `leafIndex` is below that root's leaf count, and the opening of `C`; it then publishes `N`, token and amount, writes the nullifier account and mints. It needs no Ethereum proof, archive node or history hop.

- **Private by default (F.4).** Every claim is a privacy-preserving transaction, showing `N`, token, denomination and the root read. A public destination is visible, but not which deposit paid it. A private-kind claim executes as the destination account itself; the mint goes to a fresh alias.
- **Mint amounts.** The claim mints the denomination to the claimant and the escrowed fees to the fee recipient and the relayer it names (§13), so the claimant's LEZ balance is a whole denomination.
- **Plaintext path (P.1).** The witness is transaction data; one public transaction, for users who cannot prove. It links mint and deposit, and the client says so first.
- A rejected claim changes nothing and can be retried (R.1). A copied `C` gets its own leaf, `fid` and `N`, and cannot block the original.

### 5.6 Recovery from the seed (F.9, R.5)

The client downloads the whole deposit list, recomputes `C` for every `(depositor, j)` against a few seed-derived destinations, and keeps matches whose `N` is unused. It never queries per item; completeness is checked against `depositCount`. Measured (laptop CPU, one thread): median 128.9 ms for 100,000 deposits, 1,308.6 ms for 1,000,000 (§18.3). The window of account indices is fixed at `0..g` per kind.

## 6. Fees on LEZ v0.4 and the gas drop (F.13, U.5, Privacy #6)

The fee payer is a correlation point; v0.4 constrains the RFP's relayer mitigation (findings §6):

- **Public transactions** name any signing fee payer, so a relayer pays for root imports and checkpoints.
- **Privacy-preserving transactions** pay the native fee from a regular private account of the prover, inside the proof. Nobody else can pay for it. The payer appears as a fresh alias with a fixed reserve per operation type.
- **The gas drop.** A v0.4 proof can sweep a received native credit into the payer within the same transaction. The claimant shows a relayer a small receipt proving that some unclaimed deposit has nullifier `N`; its journal binds `(N, oneTimeAddress, relayer)` (D13), so it cannot be replayed to another relayer or address. The relayer sends one sealed credit per `N` to the one-time address; the claim sweeps it and pays the relayer the fixed relay fee in the wrapped token. Burns use the same drop. The relayer learns a time, a one-time address and the transaction, never which deposit. The receipt and the relayer's one credit per `N` are built ([EVIDENCE](../EVIDENCE.md) rows 47, 48); the sealed credit and its sweep inside the claim ran on LEZ v0.4.0-rc3 with real proofs (row 124, M0 check 2 GO): the credit from the relayer's private account sealed to a fresh one-time private account, whose public keys the claimant hands over with the receipt (they must derive its address), the claim naming that account as fee payer and paying the relayer the relay fee; in the daemon `ferrata-relayer` and for burns it is not built. If it fails, a first claim needs native LEZ from a faucet or a public purchase, and the documentation will state the correlation that leaves.
- **Throughput.** At most four privacy-preserving transactions fit in a block.

## 7. Outbound, part 1: what an executed burn does not prove

"A valid LEZ burn" is three facts, and Ethereum has to learn all three (findings §4):

| Fact | Where it lives in v0.4 | Provable today? |
|---|---|---|
| The burn is in a given LEZ block, once | the block hash is SHA-256 over all transactions and the reverted list | yes, cheaply |
| That block and all before it are valid | nowhere: no state root; nodes re-execute, and fees, the gas cap and out-of-gas reverts depend on executor cycle counts that a RISC Zero receipt claim does not contain | not without an exact metering replica or a protocol change |
| The block's inscription is final on Logos Blockchain | leader proofs are checked against two ledger roots that headers do not commit to; finality is "`k` deep" on the connected node, with no certificate | not without replaying the whole ledger in a proof |

On v0.4 the last two can only come from the accredited sequencers, as for LEZ's own cross-zone delivery (findings §5). The design is therefore a light client of the LEZ channel, proven to Ethereum: rule (B) takes finality from the sequencers' signed reading of their own nodes' final tier; rule (A) infers it from support plus time from Ethereum.

## 8. Outbound, part 2: the checkpoint program

### 8.1 Checkpoint state

```
S = H("ferrata/lez-state/v1" ‖ tipId:u64 ‖ tipHash:32 ‖ keySetHash:32 ‖ configTipHash:32
      ‖ burnCount:u64 ‖ burnFrontierHash:32 ‖ importedLeafCount:u64 ‖ sunset:u8)
keySetHash = H("ferrata/keyset/v2" ‖ n:u32 ‖ postingTimeframe:u32 ‖ key_1:32 ‖ … ‖ key_n:32)    accredited_keys order
```

- `tipId`, `tipHash`: the block `T` up to which burns are collected.
- `keySetHash` (v2, D3) binds the channel's turn length, `posting_timeframe` in slots, alongside the keys.
- `configTipHash`: the channel's configuration-chain tip, so rotations are followed in order.
- `burnFrontierHash`: the hash of a tagged, canonical frontier of the burn tree, unused levels zero (D12), so the tree cannot be reset.
- `importedLeafCount`: the leaf count stored by `ImportRoot` (§5.4); the program keeps the maximum seen, so it cannot decrease (D10), and nothing changes it after `Sunset`.
- `sunset`: whether `Sunset` has executed.

### 8.2 What the program proves, under either rule

A RISC Zero guest, wrapped to Groth16; anyone can prove it. Given parent state `P`, LEZ blocks from `P.tip` to `T` and beyond, and key set `K` (`n = |K|`, `t = t(n)`):

1. **Hash chain.** Every block hash is recomputed from its bytes, blocks link by `prev_block_hash`, ids increase by one; no block can be skipped or added.
2. **Collection.** From every non-reverted transaction in `(P.tip, T]` addressed to `bridgeHeader`, for this `configId` only: burns, appended to the burn tree in order; successful `ImportRoot`; `Sunset`. A prover cannot leave out a visible burn.
3. **Key-set changes.** `ChannelConfigOp`s are witnesses, applied in `configTipHash` order. Logos Blockchain requires exactly the live `configuration_threshold` signatures on a `ChannelConfigOp`; the program checks that, requires the threshold to be at least `t(n)`, where `t(n) = max(1, min(⌈2n/3⌉, n − 1))` is LEZ's own threshold, and refuses a key list with repeated keys (D10). It refuses a `ChannelConfigOp` whose `posting_timeframe` differs from the one in the current key-set hash, or is 0. A zone that changes its turn length is a change the configuration cannot follow: it is retired (D3). The program refuses to advance if `n < nMin` or `t < tMin`.
4. **Support** for `T`, by rule (B) (§8.3) or rule (A) (§8.4), selected by the deployment.
5. **Journal**, 17 words (D3):

```
abi.encode(CHECKPOINT_DOMAIN, evmChainId, vault, lezConfigId, nMin, tMin, parentState, newState,
           burnRoot, burnCount, importedLeafCount, sunset, echoNonce, tipId, tipHash, n, t)
CHECKPOINT_DOMAIN = H("ferrata/vault/checkpoint/v3")
```

`newState` is `S` at `T`. `tipId`, `tipHash` let a watcher compare a candidate with its node; `n`, `t` (for the key set in `newState`) size the wait. Under rule (B) `echoNonce` is 0.

**Deployment check.** The program sees the turn length in slots only, so the deployment check verifies that the vault's `turnSeconds` equals `posting_timeframe` times the slot duration, and refuses a genesis state whose `posting_timeframe` is 0 (D3, D10). Without this pin the wait's turn term (§10.2) would be unsound.

**Burn arrival paths (D10).** The program sees burns only as public top-level calls, as the public part of private transactions and as private-to-public boundary deliveries to `bridgeHeader`. The bridge program must refuse a burn arriving any other way (a nested call from a public program, an asynchronous message, the bridge run as a private actor), or tokens would be destroyed without a burn leaf. Whether a v0.4 program can tell these apart is an M0 check.

**Reading blocks.** Provers must read blocks from the channel's inscriptions: a block served by an RPC or indexer may carry a different `bedrock_status` byte, and its inscription check then fails (D10).

The program does not check transaction signatures, private-transaction proofs or LEZ's other validity rules; an invalid block is caught only through support (V2, §11.4).

### 8.3 Support under rule (B): final-tier attestations (proposed; built in the checkpoint program, not in the vault)

Each sequencer runs `ferrata_attestor`, a Logos module, which signs its node's final tip with an attestation key (D13):

```
("ferrata/final-tier/v2", evmChainId, vault, lezConfigId, channelId, keySetHash, tipId, tipHash, finalMsgId)
```

Ed25519 over the whole tagged message (241 bytes, `tipId` big-endian), never over a bare 32-byte digest. `keySetHash` is the attestor's live key set and must equal the proof's `K`.

**Attestation keys.** Each sequencer's channel key signs, once per deployment, a certificate `("ferrata/attest-key/v1", evmChainId, vault, lezConfigId, channelId, attKey)` (169 bytes). D13 names only `channelId` and `attKey`; the prototype also binds the deployment, so that a certificate cannot be carried to another vault or configuration on the same channel and a leaked attestation key is confined to one deployment. The proof carries the certificates; a certificate dies when its channel key leaves `K` (there is no other bound: a key that leaves and rejoins `K` revives its certificates). Beyond its certificate the channel key signs nothing for the bridge; its inscription of `T`, which it signs for Logos Blockchain anyway, is checked (Rule).

**Rule.** The program accepts `T` if at least `t` keys of `K`, through certified attestation keys, signed final tips that are `T` or descend from `T` on the hash-linked path. One proof per checkpoint, no beacon, no producer exclusion. The program also checks one inscription of `T` exactly as rule (A) does (§8.4: `channelId` pinned, payload = `T`'s bytes, the signer's Ed25519 signature over the Mantle transaction hash), and its signer must be in `K`: `t` attestation keys, a second key class no LEZ node trusts, cannot certify a history alone; one channel key of `K` must also have inscribed `T` (review r3 §2 C). It is the only inscription rule (B) hashes, so a prover picks a small `T` (§18.5). A repeated channel key or attestation key refuses the input. `finalMsgId` is signed for watchers and not checked: checking it would mean BLAKE2b over the tip's inscription. An attestation needs no nonce or deadline: it states a fact that does not expire, one over a tip below `T` cannot support `T`, and its key-set hash retires it at the next rotation. The journal is word for word rule (A)'s candidate journal over the same blocks; the vault tells the rules apart by the guest's image id.

**Cost** (measured, executor, §18.5): 1.69M cycles per attestor, the two Ed25519 checks; 5.1M at `t` = 3, 11.8M at `t` = 7, whatever the blocks hold. Code: `checkpoint/crates/attestor-core` (the attestor's core, with vectors checked by an independent RFC 8032 implementation), `checkpoint/crates/checkpoint-core/src/rule_b.rs`, the guest `checkpoint/guests/checkpoint-b`.

### 8.4 Support under rule (A): inferred (fallback; built in the checkpoint prototype)

**Producer and support (D3).** The program checks one inscription of `T` exactly like a counted one (`channelId` pinned, payload = `T`'s borsh bytes, Ed25519 signature over the Mantle transaction hash under the claimed key). Its signer `D` must be in the key set `K`. Let `Y = T` if `echoNonce = 0`, else the first block after `T` containing the 32-byte `echoNonce` in a transaction addressed to `bridgeHeader`. The blocks in `(Y, E]` include blocks with checked inscriptions by at least `t` distinct keys of `K` other than `D`. With `allowSingleKey` and `n = 1`, the one key is `D` and the supporter. The proof is made twice: once as the candidate (`echoNonce = 0`), once at finalisation over blocks that contain the beacon's nonce (§10.3).

**Single-key testnets.** `allowSingleKey` holds only when the pinned `nMin <= 1` (D10), so a prover cannot switch it on in production. Such a vault is secured by one hot key, and its documentation says so.

**Cost.** Each counted inscription is BLAKE2b over the whole block (55.9 cycles per byte, no accelerator): 0.89M cycles for an empty block, 54.1M for one with four private transactions (measured, executor, §18.4). Provers choose small tip and support blocks; rule (B) hashes one, `T`'s.

### 8.5 Key rotation

Rotations reach the program only through the `ChannelConfigOp`s a prover supplies. Under rule (A) both proofs reproduce the same `newState`, so they count against the same `K`; a rotation between them only stops that candidate. The safety cost is that retired keys stay counted until a finalised checkpoint carries the rotation: an attacker holding `t` keys of an old set simply never supplies the rotation that retired them, and V1 (§11.4) must hold for every key set the program ever accepted. Watchers cancel candidates whose key set lags the live one (§11.5); under rule (B) each attestation names its live key set.

**Configuration branches.** The guest cannot tell a landed `ChannelConfigOp` from one that collected valid signatures but never landed (two ops with the same parent can both carry `t` signatures); a prover can steer the vault onto such a branch, which stalls it until a revocation or a new vault (LEMMA §5.6).

## 9. The two finality rules

Each honest signature rule (A) sees certifies "`T` was in my head at some time"; none rules out "`T` left every honest chain and came back" (§11.3). A final-tier attestation does, which is why rule (B) is proposed. The vault carries both finalisation paths and an immutable `mode` chosen at deployment (D13).

| | Rule (B): final-tier attestations (proposed) | Rule (A): inferred support (fallback) |
|---|---|---|
| What `t` keys sign | their node's final tip, with certified attestation keys | channel inscriptions of blocks after `T` (after `Y`) |
| Finality needs | V1–V3 and no Logos Blockchain reorganisation deeper than `k` on the attestors' nodes | V1–V3 and F1–F4 (§11.4) |
| Beaten by | `t` attestation keys and one channel key of `K` (`T`'s inscription); enough attestors' nodes eclipsed for as long as it takes to show them `k` private blocks | `t` keys; F2 failing beyond S plus two short private forks, uncancelled; two honest nodes eclipsed for about `W`, uncancelled |
| Logos Blockchain slowdown | waits | may finalise before `k` deep (§11.6) |
| Beacon | none | `prevrandao`, recorded at least `W` after the candidate |
| Chains | any EVM chain | chains with an unpredictable `prevrandao` |
| Producer exclusion | not needed; `T`'s inscription checked | needed and checked |
| Proofs per checkpoint | 1 | 2 |
| Vault wait | default 1 h, a reaction window only | `W = deltaAtSubmit + (t − 1) × turnSeconds`, at least 3 h 16 min |
| Watcher check | tip already final on submission | tip final by `submittedAt + Δ_g`; live key set |
| Silent keys tolerated | `n − t` | `n − 1 − t`; none for `n <= 5` |
| Recommended committee | floors `nMin >= 3`, `tMin >= 2` | `n >= 6` |
| Guest cost (cycles) | 1.69M per attestor: 5.1M at `t` = 3 to 11.8M at `t` = 7, plus `T`'s inscription (0.88–0.90M for an empty `T`, more for a busy one); one proof per checkpoint (measured, executor) | 0.89M per supporter plus `T`'s own check with small blocks (3.70M at `n = 4`), up to 224.8M with busy support blocks; two proofs per checkpoint (measured, executor) |
| Outbound latency before the release delay | about 3 h (estimated) | 7–22 h (estimated); 4.00–7.68 h median from tip to finalisation in simulation (§11.7) |
| Operator duties | an attestor beside every sequencer | none |

Both rules fail equally to `t` colluding keys and to a Logos Blockchain safety failure; a false attestation is checkable but not slashable on v0.4. Under rule (B), F2, F3 and the chain-growth half of F1 drop out (D7), and off-chain roles are liveness-only (Security #7); under rule (A), outside its conditions, the watchers are trusted for safety.

**The decision asked of Logos.** Will the testnet's accredited sequencers run `ferrata_attestor`? Yes: rule (B). No, or no answer by day 5 of M0: rule (A).

## 10. Outbound, part 3: the vault side

### 10.1 Candidates

`submitCandidate(journal fields, seal)`, permissionless, requires: `parentState == finalState`; `echoNonce == 0`; `nMin`/`tMin` equal to the immutables; `n >= nMin` and `t >= tMin`, checked by the vault itself (D4); no checkpoint pause; a valid proof for the deployment's mode; and a `newState` that is not already pending (D13: keyed on the state, not the seal, so replaying a proof cannot restart an honest candidate's wait). It stores `submittedAt = block.timestamp`, `deltaAtSubmit = delta`, the tip and `t`, and the current revocation epoch; under rule (A) it records a beacon. It emits `CandidateSubmitted` with `tipId`, `tipHash`, `n`, `t`.

### 10.2 The wait

Under rule (B) the wait is a reaction window against `t` stolen keys, default 1 h (D7, D13).

Under rule (A):

```
W = deltaAtSubmit + (t − 1) × turnSeconds
```

`t` comes from the candidate's journal; `turnSeconds` is the channel's `posting_timeframe` in seconds (900 on testnet values), an immutable pinned by the deployment check (§8.2). `t − 1` corrupted keys can each hold a full turn with empty inscriptions, during which no honest block can land. `delta` is at least the immutable `deltaFloor >= Δ_g + S + ε` (§11.6); the admin can only raise it, to at most 30 days, and a change applies to later candidates (F.12).

### 10.3 Finalisation

Rule (A): `finalize(beaconIndex, journal fields, seal)`, permissionless, requires: the candidate exists with `parentState == finalState` and the current revocation epoch; `beacons[beaconIndex].timestamp >= submittedAt + W`; `echoNonce == beacons[beaconIndex].nonce`; the other fields equal the candidate's; `n >= nMin`, `t >= tMin`; no checkpoint pause; monotonicity; a valid proof.

**Beacons.** `recordBeacon()`, permissionless, appends `(block.timestamp, keccak256(abi.encode(block.prevrandao, block.number, address(this))))`, at most one per block.

Rule (B): the candidate's single proof carries the attestations; `finalizeAttested` takes no second proof and no beacon and succeeds once the wait has passed. Built in vault v4, measured with RISC Zero's real verifier on the real rule (B) proof: `submitCandidate` 347,766 gas, `finalizeAttested` 196,601, so 544,367 a checkpoint against 975,734 for rule (A) on the same vault (`vault/GAS.md`). As designed: `submitCandidate` records no beacon and stores the digest of the journal the seal was verified against; the rule (B) finalisation re-presents the journal, checks its digest (which authenticates the tip, `n` and `t` as D11 asks), the live candidate and `submittedAt + deltaAtSubmit`, then makes the same state changes as rule (A)'s; about 0.71M gas a checkpoint against 1.06M (§18.1, estimated).

On finalisation, `finalState = newState`, the checkpoint joins the history with `finalizedAt`, its `burnRoot` is accepted, and `CheckpointFinalized` is emitted with `tipId`, `tipHash`, `n`, `t`. The first non-revoked sunset checkpoint sets the refund boundary.

**Monotonicity**, checked by the vault at finalisation: `importedLeafCount` never decreases along finalised checkpoints; once a finalised checkpoint has `sunset = true`, every later one has `sunset = true` and the same `importedLeafCount`.

### 10.4 Cancelling a candidate

`cancelCandidate(newState)` deletes a pending candidate; it is reject-only. The same state can be submitted again and waits again in full. In simulation, cancelling stopped every modelled attack on rule (A), including the long eclipse; pausing did not, or halted the vault (§11.7).

- **Who.** Under D13, any one of the watchers (§14); in vault v4 also the freeze authority.
- **When.** Under rule (A), a watcher cancels a candidate whose `(tipId, tipHash)` is not in its own honest LEZ node's final tier by `submittedAt + Δ_g`, and a candidate whose `keySetHash` is not the channel's live set at its tip (§11.5). Under rule (B), a watcher checks on submission: the tip must already be in its final tier.
- **Not sticky.** The same public proof can be resubmitted, so a watcher cancels again within every wait; a persistent attacker is answered with the checkpoint pause (D11).
- In the prototype it works under the checkpoint pause and after a verifier stop, and emits `CandidateCancelled(newState, tipId, tipHash)`.

### 10.5 The checkpoint pause

One reject-only flag blocks both `submitCandidate` and `finalize`; it expires like the other freezes. It is the general pause. Watchers can set it for at most 12 h; the freeze authority for up to `maxFreeze` (D13). Under rule (A), `monitor` sets it when an honest node's final tier has not advanced for `(t − 1) × turnSeconds + S/2` (D4). A shorter trigger pauses the vault for hours a day when a minority merely holds its turns (§11.7).

### 10.6 Revocation

`revokeFrom(index)` (freeze authority) requires `checkpoints[index].finalizedAt >= block.timestamp − revokeWindow` and refuses an already-revoked index (D5). It revokes every checkpoint from `index` on, restores `finalState` (with its imported leaf count and sunset flag) to the state before `index`, clears the sunset boundary if a revoked checkpoint set it, and advances `revocationEpoch`, invalidating every pending candidate. Used nullifiers stay used; it accepts nothing. `revokeWindow` defaults to 7 days.

**The halt.** After one bad finalisation every later candidate descends from it, so no honest checkpoint can follow. A wrong checkpoint that is not revoked within `revokeWindow` halts the vault for good (D13).

## 11. Rule (A), analysed

Validity (accepted blocks executed correctly) needs a key bound and re-execution; finality (never reverted) needs more. Specification v2 claimed finality from too little; specification v3 is the repair (D1).

### 11.1 The model in brief

From `docs/design/finality-rule/MODEL.md` (LEZ `v0.4.0-rc1`, Logos Blockchain `ee5e344`, with source citations):

- A node's head is its final tier folded over the unfinalised channel lineage, each block re-executed. A block is final for a node when it is `k` deep on the chain its Logos Blockchain node follows, with no certificate. Another sequencer's block enters an honest head once its node has processed an inscription carrying it, which is not the same as final.
- A LEZ block has no producer field and no signature; any scheduled key can inscribe a copy of its bytes, and dropped inscriptions can return after a reorganisation.
- One key is scheduled per `posting_timeframe` (900 one-second slots on testnet values), or per `posting_timeout` (150) when nothing is inscribed. An empty inscription is a missed turn, not an offence. Corrupted keys can sign blocks off chain in any number.
- `k = 120`, `f = 1/30`, 1 s slots (repository templates, unconfirmed for a live network): `k` blocks take about 1 h on average.

### 11.2 Validity, and what it needs

If the vault accepts a candidate for `T`, the support has `t` keys other than `D` and fewer than `t` of `K` are corrupted, so an honest key inscribed some block after `T`. An honest key signs only its own blocks, built on its head, where every block was re-executed on its parent's state, deterministically. So `T` and its ancestors are valid. This uses neither Logos Blockchain, nor time, nor the exclusion of `D`.

Checking `D` matters. Specification v2's item never checked the excluded producer's inscription, so the exclusion was vacuous and `t` keys could get an invalid block accepted (D1): in simulation, 200 of 200 runs with the exclusion as v2 wrote it, 0 of 200 with `D` checked (experiment (b)). With `D` checked, an invalid `T` needs `t + 1` keys. The residual assumption is that an honest key never signs an inscription of a block it did not produce. The stated bound stays "fewer than `t`", because `t` keys break finality (§11.8).

The exclusion also matters when one honest producer's node is partitioned for more than Δ and `t − 1` keys sign on top of its unmined blocks: without a checked `D` the simulated vault finalised a never-published block in every run, with it in none (experiment (c)).

### 11.3 The counterexample to the previous rule

The specification v2 rule waited a fixed Δ and claimed finality from four assumptions: fewer than `t` corrupted keys; re-execution of blocks seen on the node's chain; Logos Blockchain safe and live with `k` blocks per Δ; `prevrandao` unpredictable Δ ahead. All four can hold while the vault finalises a block that never becomes final (LEMMA §3).

Smallest case with a corrupted key: `n = 3`, `t = 2`, honest `H0` and `H1`, corrupted `M`, Δ = 2 h. Twice, for a few minutes, the adversary decides which chain the nodes of `H0` and `H1` follow (a short private fork, or delayed blocks).

1. In its public turn `M` posts an empty inscription, then inscribes a valid block `T_A`, holding a burn to double-spend, only in a private block `β1`.
2. **First fork.** The two nodes follow `β1`; `H0` and `H1` build on `T_A`. A candidate is submitted at time `a` with support `{H0, H1}` and `D = M`. The nodes return to the public chain, without `T_A`.
3. **Stall.** `H0` and `H1` stay down until about `b − D_k` (`D_k`, the time for `k` blocks, about 1 h), so the canonical fold stays at `T_A`'s parent.
4. **Second fork** at the first Ethereum block `b >= a + Δ` with a beacon: `M` re-inscribes `T_A` and a block `Y` carrying the nonce in a private block `β2`; the two nodes build on `Y`. **The vault finalises `T_A`.** The fork is dropped, an honest block takes `T_A`'s height, and the burn is undone.

What fails is honest progress: no honest block reached the canonical chain for about Δ − `D_k`. At `n = 2` (`t = 1`) the same timeline works with **no** corrupted key: the adversary captures the honest producer's node, so its block is inscribed only on the private fork, and re-includes that inscription on the second.

Simulation counts against the previous rule (measured in the Python simulator, `docs/design/finality-rule/RESULTS.md` experiment (h); testnet parameters, Δ = 2 h):

| Scenario | Attack succeeded |
|---|---|
| `m = t − 1` keys, two short forks around an outage of all honest sequencers, `n` = 3, 4, 7, 10 | 380 of 400 (94, 96, 98, 92 of 100) |
| No corrupted key, `n` = 2, 3, 4 | 150 of 150 |
| Same fork attack, honest sequencers live (experiment (a)) | 0 of 400 |
| Two honest nodes eclipsed for Δ, other honest sequencers live, `n` = 3, 4, 7, 10 | 200 of 200 |
| `n = 3`, honest sequencers back 45 / 55 / 60 / 65 / 75 min before the beacon | 45 / 36 / 18 / 8 / 0 of 50 |

Each fork controlled two nodes' views for at most 317 s (`n = 3`) to 1,068 s (`n = 10`). The 20 failures in the first row were attempts started while an honest block was in flight. At Δ = 3 h the outage sweep is almost the same (45, 37, 17, 7, 0 of 50): a longer wait makes the attack costlier but never removes it.

### 11.4 The assumptions (D6)

The conditions as specification v3 states them in D6:

**Validity.** A burn root the vault accepts comes from valid LEZ blocks, provided:
(V1) fewer than `t` keys of every key set the checkpoint program has used are controlled by an attacker, at any time, including keys that have since left the committee; (V2) an honest sequencer extends only blocks its own node re-executed, and execution is deterministic; (V3) the proof system is sound.

**Finality.** Those blocks are also final on LEZ (in every honest node's final tier, never reverted), provided in addition:
- (F1) on the Logos Blockchain node of every honest sequencer, as its LEZ node has processed it, no reorganisation goes deeper than `k`, and at least `k` blocks arrive in every window Δ_g (no eclipse, no lag, no slowdown);
- (F2) whenever an honest sequencer holds a LEZ block of height `h`, a block of height at least `h` enters the canonical chain's fold within `(t − 1) × turn + S`. This needs: at most `t − 1` adversarial keys in the channel's live accredited set, not only in `K`; `turn` equal to the live `posting_timeframe` and non-zero; honest sequencers up except for stalls of at most S; their inscriptions neither censored nor kept off the chain by eclipse;
- (F3) nobody learns a beacon value more than ε before the Ethereum block that records it;
- (F4) `W >= Δ_g + (t − 1) × turn + S + ε`, which the vault enforces together with D3's rule that the program refuses a change of `posting_timeframe`.

V1–V3 are the validity conditions of both rules. Under rule (B), F2, F3 and the chain-growth half of F1 are not needed (§9).

**Proof idea** (Theorem F, `docs/design/finality-rule/LEMMA.md` §4). The candidate shows an honest node had `T` in its head before submission time `a`; by F2 a block at `T`'s height is on the canonical fold by `a + (t − 1) × turn + S`. The finalisation shows an honest node had `T` in its head at least `W − ε` after `a` (F3); by F1 and F4 that node's last irreversible block is later than the canonical block above, so `T` is in its final tier, `k` deep, and hence eventually in every honest node's. The exclusion of `D` is not used; it matters only when F1 fails for a single producer's node (§11.2).

### 11.5 F2 is read with the live key set

F2 bounds adversarial keys in the channel's **live** accredited set, not only in the `K` the proof used (D6, D13). `W` covers `t − 1` hogged turns with the proof's `t`; a prover proving against a stale, smaller set would get a shorter wait, while keys newly staked into the live set could hog more turns than it covers. So under rule (A) watchers **cancel**, not merely flag, a candidate whose `keySetHash` is not the live set at its tip (D13). For the same reason the turn length is pinned (§8.2): with a different live `posting_timeframe`, `(t − 1) × turnSeconds` would no longer bound the hog.

### 11.6 The wait budget

| Term | Meaning | Value |
|---|---|---|
| Δ_g | window in which Logos Blockchain produces `k` blocks except with negligible probability | 2 h: P(fewer than 120 blocks) = 1.27e-18 at full participation, 3.7e-4 at two thirds, so a deployment expecting less than full participation sets 3 h (D13) |
| S | longest honest stall tolerated without relying on the pause; a policy choice | 1 h default |
| ε | how early a beacon value can be known; about `12(r + 1)` s for a coalition proposing `r` consecutive slots | 1 min (covers `r <= 4`) |
| turn | `posting_timeframe` in seconds, `turnSeconds` | 900 s on testnet values |

Default `deltaFloor` = 2 h + 1 h + 1 min = 3 h 01 min (with Δ_g = 3 h it would be 4 h 01 min, derived). At the default floor (derived):

| `n` | `t` | `(t − 1)` turns | W |
|---|---|---|---|
| 3 | 2 | 15 min | 3 h 16 min |
| 4 | 3 | 30 min | 3 h 31 min |
| 7 | 5 | 1 h | 4 h 01 min |
| 10 | 7 | 1 h 30 min | 4 h 31 min |

Under the previous fixed-Δ rule, at Δ = 2 h and half block rate, 10.1 % of the fastest finalisations were under `k` deep (measured in simulation, experiment (e)); none of them was reverted in the model, which has no Logos Blockchain adversary.

### 11.7 Experiments on the v3 rule: (i), (j), (k)

Measured in the Python simulator (`docs/design/finality-rule/RESULTS.md`). Settings: `D` checked; `W = 3 h 01 min + (t − 1) × 15 min`; the tip trigger fires when a candidate's tip is not in the honest final tier by `a + 2 h`; the stall trigger fired after 30 min without progress, shorter than D4's. Adversary keys sit consecutively in the turn order (the committee is sorted by key, so key bytes can be ground).

**(i) Turn hogging.** `m = t − 1` keys hold each 900 s turn with empty inscriptions, with a fork attack around it; honest sequencers live. Attack successes:

| `n` | `t` | hog | flat Δ = 2 h | flat Δ = 3 h 01 | v3 `W` | `W` |
|---|---|---|---|---|---|---|
| 3 | 2 | 15 min | 0/100 | 0/100 | 0/100 | 3.27 h |
| 4 | 3 | 30 min | 0/100 | 0/100 | 0/100 | 3.52 h |
| 7 | 5 | 60 min | 0/100 | 0/100 | 0/100 | 4.02 h |
| 10 | 7 | 90 min | 67/100 | 0/100 | 0/100 | 4.52 h |
| 13 | 9 | 120 min | 48/50 | 0/50 | 0/50 | 5.02 h |
| 16 | 11 | 150 min | 50/50 | 16/50 | 0/50 | 5.52 h |

`W` cancels the hog term exactly: 0 successes in 500 attempts for committees of 3 to 16. When, in addition, a network adversary keeps each honest sequencer's inscriptions off the chain while it is scheduled, F2 fails, and the attack succeeds under v3 `W` in 43/50, 48/50, 50/50 and 45/50 runs (`n` = 3, 4, 7, 10).

**(j) The earlier attacks against v3.** 50 runs per cell, attack successes. "Held": pause until expiry (30 days); "lifted": pause lifted once another block is final at the tip's height; "cancel": the candidate is cancelled.

| Attack, v3 `W` | `n` | guard off | held | lifted | cancel |
|---|---|---|---|---|---|
| outage of all honest sequencers + two short forks, `m = t − 1` | 3 / 4 / 7 / 10 | 45 / 48 / 50 / 45 | 0 | 0 | 0 |
| same, no adversary key (`m = 0`) | 2 / 3 / 4 | 50 / 50 / 50 | 0 | 0 | 0 |
| two honest nodes eclipsed for `W`, `m = t − 1` | 3 / 4 / 7 / 10 | 50 / 50 / 50 / 50 | 0 | 0 / 0 / 50 / 50 | 0 |
| hogging + honest turns kept off chain | 3 / 4 / 7 / 10 | 43 / 48 / 50 / 45 | 0 | 1 / 0 / 0 / 0 | 0 |

Without the guard, 724 of these 750 attack runs succeeded; with cancellation, none, and the vault kept finalising (240 / 200 / 150 / 100 honest finalisations in the outage rows). "Held" stopped every attack and the vault with it (0 honest finalisations afterwards). "Lifted" failed against the long eclipse whenever a third honest sequencer kept LEZ live (`n` = 7, 10).

False pauses: none in 800 honest runs. Under a Logos Blockchain slowdown to half rate (`n = 4`), "held" halted the vault in 19 of 50 runs, while cancelling cost 20 cancellations and no pause. When the adversary holds every one of its turns, the 30-minute stall trigger paused every run (7.27 h of 28 at `n = 7`, 10.11 h at `n = 10`, delaying finalisations by up to 2.13 h); the tip trigger alone paused none. These are the reasons D4 gives for cancelling rather than pausing and for the longer stall trigger.

**(k) Honest-operation latency**, tip creation to finalisation, medians (p90): 4.00 h (4.21) at `n = 3`, 4.67 h (4.96) at `n = 4`, 6.11 h (6.37) at `n = 7`, 7.68 h (7.92) at `n = 10`, against 2.65 / 3.29 / 4.24 / 5.20 h under the previous flat 2 h rule. The shallowest tip at finalisation was 379 blocks deep (`n = 3`). Latency is `W` plus about two rounds of `t` posting turns; proving time and the release delay come on top.

**Not modelled:** a Logos Blockchain adversary racing honest leaders with stake (an analytic table prices that step); key rotation (argued in LEMMA §5.6); transaction contents; Ethereum reorganisations; ε (set to 0).

### 11.8 What still breaks rule (A), and what bounds the loss

From D6, "What breaks it":
- `t` keys can sign any history (200 of 200 runs in experiment (b)). The candidate is public for `W` before it can finalise; a watcher's tip check cancels it, and `revokeFrom` un-accepts it within the window.
- Turn hogging by the adversary's own keys does not break the rule: `W` absorbs it.
- A never-final block can be finalised with fewer than `t` keys, even with none, if F2 fails for more than S beyond the hog and the attacker can twice show two honest sequencer nodes a short private fork.
- Two honest sequencer nodes eclipsed for about `W` defeat the wait whatever the other honest sequencers do.
- The tip check stops all of these, provided the watcher's own node is not eclipsed, the watcher is live, and the offending candidate is cancelled. Outside F1–F4, the watchers are therefore trusted for safety.

The loss is bounded by the public wait, the release delay (24 h outside testnets, a second window for `revokeFrom`) and the caps (§13). An error not revoked within `revokeWindow` halts the vault for good (§10.6).

### 11.9 Small committees, silent keys, retired keys

With `t(n)`: `n` = 3, 4, 5, 6, 7, 10 give `t` = 2, 3, 4, 4, 5, 7, so `t − 1` = 1, 2, 3, 3, 4, 6 corrupted keys are tolerated (none for `n <= 2`). Silent keys tolerated: rule (A) `n − 1 − t` = 0, 0, 0, 1, 1, 2; rule (B) `n − t` = 1, 1, 1, 2, 2, 3.

- **Floors.** Outside testnets `nMin >= 3` and `tMin >= 2`. Committees below three keys are for testnets only. Recommended committee for rule (A): `n >= 6` (D13).
- **Silent keys.** Under rule (A), in simulation, finalisation stopped exactly when silent keys exceeded `n − 1 − t` (experiment (g)); v0.4 has no liveness fault. The exclusion of `D` costs one tolerated silent key; it stays because it is the only defence against a partitioned producer.
- **Retired keys** stay counted until a finalised checkpoint carries the rotation, while their stake is released after `exit_delay` (about 40 min on testnet values), well before `W`. V1 covers them by its wording.

## 12. Burn notes and the release proof (F.5–F.7, R.2, Security #2)

### 12.1 Burn and burn note

A burn is a top-level call (public holding) or a boundary message (private holding, F.6) to the bridge program; any other route is refused (§8.2). It destroys one denomination through synchronous messages only, so a non-reverted burn means destroyed tokens, and publishes a note:

```
r_i  = HMAC(kView, "ferrata/burn-tag/v1" ‖ configId ‖ i:u64)
sOut = HMAC(kAuth, "ferrata/s-out/v1" ‖ configId ‖ r)
COut = H("ferrata/Cout/v1" ‖ sOut ‖ evmChainId ‖ vault ‖ recipient:20)
memo = recipient XOR HMAC(kView, "ferrata/memo/v1" ‖ r)[0..20]
mac  = HMAC(kView, "ferrata/burn-mac/v2" ‖ r ‖ memo ‖ token ‖ amount ‖ COut)[0..16]
note = (r, memo, mac, token, amount, COut)
```

`COut` hides and binds the recipient (F.5). `mac` covers `memo` (D12), so a copied note with another memo cannot mislead a `kView`-only viewer.

**Write-once burn account (D13)**, keyed on the hash of the whole note `(r, memo, mac, token, amount, COut)`, so copying a pending burn's tag cannot make it revert. The wallet advances `i` on every published burn, even a reverted one, so a tag never repeats (a repeat would reuse `sOut`). Recovery (F.9): the wallet regenerates tags, unmasks `memo`, and keeps only notes whose `mac` verifies.

```
BURN_TAG = H("ferrata/burn/leaf/v2")
burnLeaf = H(0x02 ‖ BURN_TAG ‖ configId ‖ burnIndex:u64 ‖ token:20 ‖ amount:u256 ‖ COut ‖ r)
```

The leaf binds the configuration, so a tenant's burns cannot be released from another tenant's vault.

### 12.2 Release statement

```
NOut    = H("ferrata/nf-out/v2" ‖ sOut)
journal = abi.encode(RELEASE_DOMAIN, evmChainId, vault, lezConfigId, burnRoot, NOut, token, amount,
                     recipient, relayer, relayFee)
RELEASE_DOMAIN = H("ferrata/vault/release/v2")
```

The proof checks the path (direction bits equal `burnIndex`) to `burnRoot` and the opening of `COut`. `NOut` v2 is independent of `burnIndex` (D13): after a revocation a burn can be collected again at another index, and a nullifier that depended on the index would let it be released twice. `NOut` is the same under every root; the holder proves against the newest root whose release delay has passed.

**Witness-independent cycle count.** Neither the receipt nor the proving time may say which burn is released, and only succinct receipts go to a wrapper. This is a CI gate (D12): 64 random witnesses, one count. Measured: 80,501 user cycles for all 64. The first version leaked the popcount of the burn index through its cycle count (the compiler had turned a branch-free mask back into a branch); the gate found it.

### 12.3 Release on the vault

`release` requires: `burnRoot` finalised and not revoked; the token's release delay elapsed since that root's finalisation, with the delay in force when the root was finalised; `NOut` unused; `amount` equal to the denomination at `denomIndex`; `relayFee` 0 or the registered fee; no release freeze; cap available; a valid proof. The vault pays the denomination less the registered burn, relay and checkpoint fees (§13). If `relayer == address(0)` the fee goes to `msg.sender`, so wallets name a relayer whenever the fee is non-zero.

**Replay protection** is keyed on `NOut`, never on proof bytes, which can be re-randomised.

**Pull payments.** A failed transfer is credited to `owed[payee][token]`; only that payee withdraws it, with `claimOwed(token)`.

**Wrapping.** The Groth16 wrap runs only on x86 today (Docker, about 7.3 GiB). Any relayer can wrap a holder's succinct receipt and learns only what the release publishes. RISC Zero targets zero knowledge for these receipts without a written proof, as does every LEZ private transaction.

## 13. Amounts, fees, caps, freezes, registry, release delays (F.8, F.10–F.14, Security #4, #5)

- **Denominations (F.8).** Every deposit, mint, burn and release is one registered denomination, at most eight per token; the default registers three.
- **Fees (F.13), collected so that LEZ balances stay whole denominations (D13).** A deposit escrows the denomination plus the registered mint and relay fees; the claim mints the denomination to the claimant and the fees to the fee recipient and the relayer. A burn of one denomination releases it less the registered burn, relay and checkpoint fees; the **checkpoint fee** goes to whoever finalised the root used, which pays provers. Fees are fixed per token and denomination. Protocol fees start at zero under an immutable 1 % ceiling, change seven days after announcement, and a deposit keeps the schedule of its leaf index.
- **Supply (Security #6).** Wrapped supply never exceeds escrow under the stated trust; a breach creates a shortfall of at most the caps before a freeze.
- **Caps (F.11, Security #4).** Token buckets per token and globally: deposits and releases on the vault, mints and burns on LEZ; refunds draw on the release bucket (D13). On LEZ, buckets and freezes run on attested Ethereum time: each gate's clock is the newest execution timestamp it has read from a record of the configuration's attestation instance (a permissionless `Tick`, and every `Freeze`, read one), monotone, and valid only in a LEZ block stamped at or after it, so neither a block producer alone nor a captured sync committee alone can move it; when Ethereum finality stalls, LEZ buckets stop refilling and LEZ freezes cannot lapse (`lez/BRIDGE.md` §6; the earlier caller-supplied time let one producer end every LEZ freeze, review r3). The global cap uses admin-set weights, not an oracle.
- **Freezes (F.14, Security #5).** Deposits, releases, mints or burns, globally or per token, and checkpoints. Every freeze lapses after `maxFreeze` unless renewed (on LEZ, `maxFreeze` of attested Ethereum time, measured from the record the `Freeze` reads). The global release freeze also stops refunds. The admin cannot unfreeze.
- **Registry (F.10).** Write-once per token: `token`, `lezUnit`, denominations, relay fees, `unitWeight`; the LEZ entry is created by attesting the vault's, and denominations must be exact multiples of the LEZ unit. Delisting stops deposits only.
- **Release delays.** Per token, `minReleaseDelay <= delay <= revokeWindow` (D5), so a checkpoint can be revoked for as long as its releases are delayed. A change applies only to roots finalised afterwards.

| Parameter | Default | Who sets it |
|---|---|---|
| `deltaFloor` (rule (A)) | 3 h 01 min | immutable |
| `delta` | at least `deltaFloor`, at most 30 days | admin, raise only, for later candidates |
| wait under rule (B) | 1 h | specified default (D7, D13) |
| `turnSeconds` | 900 s on testnet values | immutable, checked against `posting_timeframe` |
| `revokeWindow` | 7 days | immutable |
| `maxFreeze` | 30 days | immutable |
| `minReleaseDelay` | 24 h (0 on testnets) | immutable |
| watcher pause | at most 12 h | watchers |
| freeze-authority replacement timelock | 14 days | admin |
| cap per token and day | smaller of 10 % of escrow and a deployer-set ceiling, above an immutable floor | admin |
| `nMin`, `tMin` outside testnets | at least 3 and 2 | immutable |

## 14. Roles

| Who | Provides | If it lies | If it stops |
|---|---|---|---|
| Root importer, relayer, prover, wrapper | imports, gas drops, proofs, wraps, submission | refused by the primitive or the vault; proofs bind recipient and fee; a prover cannot omit a visible burn | anyone else does it |
| Accredited sequencers | LEZ blocks; attestations under (B) | `t` can forge a burn root, bounded by the wait, watchers, revocation, delays and caps | the zone stops; finalisation needs `t` live keys under (B), `t + 1` under (A) |
| Watchers (`monitor`) | cancelling candidates; pauses of at most 12 h | can delay finalisation until the admin replaces them | a bad candidate is not cancelled; release delay and revocation remain |
| Freeze authority (multisig) | freezes up to `maxFreeze`, `revokeFrom` within `revokeWindow`, `retire()`, LEZ `Sunset` | cannot move escrow or accept anything; can halt deposits and releases while freezes last, revoke recent checkpoints, retire | freezes lapse; anyone retires after 30 days without a checkpoint |
| Admin | registry, caps above the floor, `delta` (raise only), delays, watcher set, freeze-authority replacement | cannot move escrow, exceed the fee ceiling, lower `delta`, cut a cap below the floor or unfreeze | positions stay claimable |

None of these parties learns anything private; relayers and wrappers learn a time, a one-time address and what the transaction publishes.

- **Watchers (D13).** Up to three hot addresses set by the admin under a timelock, each with its own nodes. Any one can `cancelCandidate` and pause checkpoints for at most 12 h; nothing else.
- **Freeze authority (D13).** A multisig; the admin replaces it after a 14-day public timelock (closing D8's open decision).
- **Emergency stop (D13).** The stop wrapper's owner is renounced in the deployment script. Only RISC Zero's own `estop(receipt)` (a verifying receipt with a zero claim digest) can trip it. A bug in Ferrata's own guests is answered by the release freeze and `revokeFrom`.
- **Keyless retirement (D13).** `retire()` and LEZ `Sunset` become permissionless once no checkpoint has finalised for 30 days. This supersedes D11's first item (anyone may retire once a sunset checkpoint has been final for longer than `revokeWindow`).

Vault v4 has all four on its side: watchers cancel and pause for at most 12 h; one admin proposal, applied by anyone 14 days later, changes the watchers and replaces the freeze authority (whose own handover is gone); `retire()` is open to anyone after 30 days without a finalisation; the vault requires an untripped stop and leaves the renunciation of its owner to the deployment script. A freeze key lost before retirement no longer blocks refunds.

The bridge program on LEZ has the same three roles' controls (`lez/BRIDGE.md` §5, `lez/BRIDGE-RESULTS.md` §14): the admin's proposal of a freeze authority and up to three watchers, applied by anyone after 14 days of attested Ethereum time; an M-of-N freeze authority acting by one transaction its signers all sign; watchers that pause LEZ mints or burns for 12 h; and a keyless `Sunset`: anyone attests the vault's `retired` flag, which only `retire()` sets (open to anyone after the 30 days), and then sends the checkpoint's own `Sunset`. So the 30 days are measured once, on Ethereum's clock, and LEZ never stops importing before the vault stops taking deposits.

Off-chain roles are liveness-only under rule (B); under rule (A) the watchers have a safety role outside F1–F4 (§11.8).

## 15. Tenants, immutability, verifier and failsafe (F.16, Security #8, #9)

**Tenants (F.16).** A configuration is a namespace of accounts under the one immutable bridge program, with its own vault, admin, freeze authority, registry, fees, caps, depths and wrapped tokens. Burn leaves bind the configuration and the checkpoint program collects only its own, so one tenant's burns cannot be released from another's vault. Two parts are not separated on the deployed program: the nullifier accounts and `holder(dest)` (§3, "Known issues, 18 November 2026"; fix planned for B1, prototyped as a second build of the program that is not deployed).

**Immutability (Security #8).** The vault pins RISC Zero's base Groth16 verifier, never the router, whose admin can add verifiers. The token program must be deployed immutable, or Ferrata deploys its own immutable copy; a mutable burning program could redefine a burn (findings §4).

**Stop-only failsafe (Security #9).** The `verifier` immutable is RISC Zero's unmodified `RiscZeroVerifierEmergencyStop` around the base verifier, with its owner renounced (§14), so only a proof that RISC Zero itself is unsound trips it. It has no unpause. After a stop, nothing more is released or refunded (D13). No function moves escrow or changes what the vault accepts: an escrow-moving recovery would need someone to choose new verification logic, which Security #8 forbids.

The vault prototype does this since v4: after a stop `refund` reverts (`vault/README.md`, `EmergencyStop.t.sol`).

## 16. Retirement and refunds: the migration mechanism

The vault and checkpoint program are immutable and parse v0.4 blocks, so a configuration is retired before a LEZ change it cannot follow, including a change of the channel's turn length or of Logos Blockchain's slot length (§8.2).

1. The freeze authority, or anyone once no checkpoint has finalised for 30 days, calls the one-way `retire()` on the vault (deposits stop forever, nothing else changes) and the one-way `Sunset` on LEZ (root imports stop; claims against imported roots and burns continue).
2. Holders burn and release as usual.
3. The first non-revoked finalised checkpoint with `sunset = true` sets the refund boundary to its `importedLeafCount`; a later one never lowers or moves it, and revoking that checkpoint clears it (D5).
4. `refund(leafIndex, depositor, token, denomIndex, C, path[32])`, permissionless, pays a deposit LEZ never imported back to the depositor named in the leaf. It requires: `leafIndex >= sunsetLeafCount` and below `nextLeafIndex`; no earlier refund of that leaf; the path hashing to the current `depositRoot`; the vault retired, so the root no longer moves and paths cannot go stale; the revocation window of the sunset checkpoint passed, so a forged sunset checkpoint can be revoked before any refund; no global release freeze; release cap available.

Since `importedLeafCount` is the count `ImportRoot` stored and a claim refuses `leafIndex >= count` (§5.4), every leaf is either claimable on LEZ or refundable on Ethereum, never both. No party moves escrow. Refunds need a finalised sunset checkpoint; retirement alone opens none. M0 asks Logos for a notice period before breaking changes.

**Window boundary.** D11: `revokeFrom` is allowed while `block.timestamp <= finalizedAt + revokeWindow`; refunds, and releases whose delay equals `revokeWindow`, open only when `block.timestamp > finalizedAt + revokeWindow`. The vault opens them one second after the last revocable second (since v4; §19).

## 17. What is public, and residual correlations (Privacy #1–#7)

| Step | Public | Hidden |
|---|---|---|
| Deposit | depositor, token, denomination, time, `C` | the LEZ destination |
| Root import | the vault's root and leaf count at a block | — |
| Claim | `N`, token, denomination, time, root read, bridge program, a one-time fee alias with a fixed reserve; a public destination | which deposit; a private destination |
| Claim, plaintext | everything, including which deposit | — |
| Burn, private holding | token, denomination, time, the burn note | the holder; the recipient |
| Checkpoint | LEZ tip, `n`, `t`, burn root and count | — |
| Release | recipient, token, denomination, time, `NOut`, root, relayer | which burn |

Residual correlations, each documented: amount and token; timing; the root read (clients read the newest); a retry after a cap rejection; a fee-schedule change; network metadata (out of scope); an adversary's own deposits.

Private execution, private destination, fixed denomination, delayed submission and a relayer are preselected (Privacy #7); error codes name no deposit (Privacy #5).

## 18. Measurements

CPU figures come from one laptop, AMD Ryzen 9 7940HS (8 cores, 16 threads), 54 GiB RAM, no GPU used; RISC Zero 3.0.5, risc0-ethereum v3.0.0. The machine was shared with other heavy jobs, so wall times state the load; executor cycle counts are deterministic.

### 18.1 Vault v3 gas: measured (local EVM, mock verifier)

Forge 1.3.6, solc 0.8.26 (200 runs, cancun), `forge test --isolate`: whole transactions, including base cost, calldata and cold access. State: 42 earlier deposits, escrow in every token, one finalised checkpoint, finite caps, relay fees registered; checkpoint rows use `n = 4`, `t = 3`. This is the rule (A) vault without D11 and D13; the v5 vault's figures (both rules, D13's fees) are in `vault/GAS.md`.

| Operation | Gas |
|---|---:|
| `depositETH`, first / later by the address | 173,920 / 153,922 |
| `depositETH` over 128 consecutive leaves, min / max | 147,556 / 177,388 |
| `deposit` ERC-20, first / later | 192,346 / 172,348 |
| `recordBeacon` | 77,823 |
| `submitCandidate`, with its beacon ¹ | 247,481 |
| `finalize` / first sunset checkpoint ¹ | 212,565 / 300,999 |
| `cancelCandidate` | 46,740 |
| `release` ETH, no fee / ERC-20, registered fee ¹ ² | 167,995 / 200,397 |
| `refund` ETH / ERC-20 | 127,577 / 153,965 |
| `claimOwed` ERC-20 / ETH | 60,132 / 37,713 |
| `revokeFrom`, min / median / max | 23,698 / 69,882 / 80,514 |

¹ Through the stop wrapper around a mock verifier (about 2,765 gas); the real verifier is not included. ² Includes the 25,000 new-account charge of a fresh recipient.

Derived (mock removed, the 224,320-gas Groth16 frame of §18.2 added): candidate about 470k, finalisation about 435k, so a rule (A) checkpoint with its second beacon is about 1.0M gas, paid once per finalised LEZ state; under rule (B), one proof and no beacon, measured on vault v4 with the real verifier and the real rule (B) proof at 544,367 against 975,734 for rule (A) on the same vault (`vault/GAS.md`; the earlier estimate was about 0.71M from the measured figures below: the candidate less the beacon it records, about 56.8k, plus a stored journal digest, 22.1k, is about 0.47M; the finalisation less the verifier through the stop, 230,107, the beacon reads and the seal's calldata is about 0.24M). Measured since with the real verifier (§18.7): candidate 507,370, finalisation 475,300, 1,060,493 with the second beacon. Caps add about 20,700 gas per deposit. Tests: 286 pass, including 10 invariants over 256 runs × 100 calls; 100 % line and branch coverage of the vault; 127 of 127 hand-written mutants killed. Proofs are mocked throughout this suite.

### 18.2 Release proof v2: measured (laptop CPU; real verifier, v2 vault)

| Item | Result |
|---|---|
| Release guest v2 | 80,501 user cycles, one 2^17 segment; one count over 64 random witnesses |
| Succinct proof | 37.1–48.6 s wall when the prover got 76–91 % of the threads; up to 150.0 s under load 20–40; peak RSS 1.46 GB; receipt 223,930 bytes |
| Groth16 wrap | 77.5–79.9 s wall started quiet; up to 175.7 s under load; prover about 7.3 GiB; seal 260 bytes |
| `verify` | 224,320 gas in the Groth16 verifier's frame; 230,107 through the stop |
| Release, whole transaction, real verifier: ETH / ERC-20 | 367,988 / 397,874 gas (local EVM, `forge test --isolate`) |

The checkpoint proof in that test was mocked; release proofs reached the real verifier. §18.7 is the run without that mock, on the v3 vault.

### 18.3 Core v2 and claim guest: measured (laptop CPU)

- Core v2: 54 tests pass; 529 vector values recomputed independently; 50 hand-written mutants, 49 killed, 1 equivalent.
- Recovery, one thread: median 128.9 ms for 100,000 deposits, 1,308.6 ms for 1,000,000.
- Claim guest (the bridge's part of a claim, a lower bound): 74,379 user cycles, constant over 64 witnesses; succinct proof 38.90 s; receipt verification 12.4 ms.

### 18.4 Checkpoint program, rule (A): measured (executor and prover, laptop CPU)

Real v0.4 block formats built with LEZ's own crates; 13 core and 17 fidelity tests pass. `n = 4`, `t = 3`; 360 blocks are one hour at 10 s.

Measured on one NVIDIA L4 GPU (cloud), an earlier checkpoint image ([EVIDENCE](../EVIDENCE.md) row 17), succinct receipts verified locally and journals equal to the native runs: one block 14.0 s, an idle hour 24.1 s, an hour with 5 private transactions 41.4 s, an hour with 50 private transactions 219.3 s; about 80 times faster than the laptop CPU figures below (`gpu-prover/RESULTS.md`). No Groth16 wrap was made on the GPU.

| Input | User cycles (executor) | Segments |
|---|---:|---:|
| one empty `T` + 3 support blocks (fixed part) | 3,705,001 | 4 |
| idle hour | 5,992,503 | 7 |
| hour with 5 private transactions | 8,821,992 | 12 |
| hour with 10 private burns | 11,298,256 | 16 |
| hour with 50 private transactions | 37,865,667 | 60 |
| `T` and 3 support blocks with 4 private transactions each | 224,848,717 | 232 |

Components: BLAKE2b 55.9 cycles/B (no accelerator), SHA-256 2.59 cycles/B, Ed25519 0.83M.

**Real proofs** of the fixed-part checkpoint, twice (image built just before the turn-length amendment; same input, 59 cycles fewer): succinct 1,157.5 s starting quiet, 1,758.9 s under load; Groth16 wrap 195.1–215.2 s; peak 9.62 GB; seal 260 bytes. Receipts verify locally; these two were not verified on a contract (the end-to-end fixture's checkpoint proofs were: §18.7). Extrapolated from 289 s per segment: an idle hour about 34 min plus 3.5 min to wrap; an hour with 50 private transactions about 4.8 h.

### 18.5 Rule (B) program: measured (executor, Modal build box)

`checkpoint/RESULTS.md` §4, on the blocks of the rule (A) fixtures of §18.4, with seeded keys at `n` = 3, 4, 7, 10, 15. Rule (B) costs 1.69M cycles per attestor (two Ed25519 `verify_strict`, certificate and attestation; the slope over the five committees), so the check is 3.4M at `t` = 2, 5.1M at `t` = 3, 8.5M at `t` = 5, 11.8M at `t` = 7 and 16.9M at `t` = 10: about twice the earlier estimate, which counted the attestations and not the certificates. Whole proofs, rule (B) against rule (A) on the same blocks: idle hour 7.51M against 5.98M at `n` = 4 and 19.31M against 12.20M at `n` = 15; an hour with 50 private transactions 39.39M against 37.88M at `n` = 4. Per checkpoint rule (B) is the cheaper: rule (A) proves twice and its second proof also links the blocks of the wait (derived: 20.1M against 7.5M for the idle hour at `n` = 4). 13 tests cover every refusal (§8.3). Since review r3 the program also checks `T`'s producer inscription (§8.3): measured +0.88–0.90M cycles on these fixtures (an empty `T`), so the idle hour at `n` = 4 is 8.40M; a 14th test covers its refusals (`checkpoint/RESULTS.md` §4a). The real proof below is of the guest before that check. One real proof: the idle hour at `n` = 4 on one NVIDIA L4, 9 segments, 31.9 s (rule (A)'s idle hour: 24.1 s, §18.4); the succinct receipt verifies locally against `checkpoint_b`'s image id. No Groth16 wrap.

### 18.6 Simulation, and what is not measured

The rule (A) simulation is a Python model of the v0.4 code (`docs/design/finality-rule/MODEL.md`), 12,000 runs for experiments (a) to (h) plus (i) to (k), deterministic per seed (§11). Not built or not measured (the vault's rule (B) path, the bridge program's claims and imports on LEZ and `ferrata_attestor`, also as a Logos module, are built since: [EVIDENCE](../EVIDENCE.md) rows 24, 41, 63–68, 94–95): the gas drop's sealed credit and its sweep; composed real proofs; inbound latency on a network (the outbound ran once on Sepolia, on testnet parameters, row 41); phones and a laptop at the declared minimum (cloud CPUs held to it and two Mac minis were measured, rows 32–34, 38, 88).

### 18.7 The return leg end to end: measured (real verifier, v3 vault; stand-in zone)

One fixture in which the checkpoint program, the release program and the v3 vault meet (`e2e/README.md`). Ethereum side, nothing mocked: RISC Zero's unmodified Groth16 verifier behind its unmodified emergency stop, the v3 vault with the default timers of §13, real Groth16 proofs. LEZ side, a stand-in: 16 blocks and their inscriptions built in the real formats with test sequencer keys (`n = 4`, `t = 3`), an `ImportRoot`, five burns; no node, no bridge program. Local EVM (Foundry), 15 tests.

| Item | Result |
|---|---|
| Journals | The checkpoint program's 17-word journals and the release program's 11-word journals are byte for byte the vault's own encodings; the bridge side recomputes `configId`, the burn root, both states and both checkpoint journals independently and gets the same bytes |
| Checkpoint proofs (candidate; finalisation echoing the beacon) | 3,786,158 and 3,831,033 user cycles, 5 segments each; 15.0 s and 15.4 s on one NVIDIA L4; wrapped to Groth16 (recursion 18.7 s and 18.6 s on 16 cores, Groth16 prover 14 s each) |
| Release proofs (ETH; an ERC-20 with a relayer's fee) | 80,501 user cycles each, the unchanged release guest; 15.1 s and 14.7 s on 16 cores; wrapped likewise |
| The run | deposits, `submitCandidate` with the real seal, the wait `W`, `recordBeacon`, `finalize` with the seal that echoes that beacon, the release delay, both releases paid to recipients that held nothing |
| Refused | one flipped byte in each of the four seals; a valid seal of another statement; each journal field changed under its real seal; a replayed release; a release against a root not yet finalised or still delayed; a finalisation one second early with the right nonce; any proof after the stop |
| Gas, whole transactions (`forge test --isolate`) | `submitCandidate` 507,370; `recordBeacon` 77,823; `finalize` 475,300; `release` ETH 363,169, ERC-20 388,243; a checkpoint 1,060,493 |
| Elsewhere | the release seals also verify, by read-only calls, on RISC Zero's own Groth16 verifier deployed on Sepolia; the deployment, candidate and beacon stages of the Sepolia scripts ran on a local fork with the same gas |

Found on the way: the bridge core's checkpoint encodings are still SPEC-v2 and could not equal the program's (the SPEC-v3 ones were written a second time beside it); adding code to the core, even code the release guest does not use, changes that guest's image id; and LEZ signs transactions with fresh randomness, so a prover must keep the blocks it proved a candidate for, because the finalisation has to prove the same ones.

### 18.8 The checkpoint on real LEZ blocks: measured (real verifier, v5 vault, rule (B); a devnet)

Rule (B) over blocks 8..=202 of the lab's chain (`lez/lab/`): LEZ v0.4.0-rc1's own sequencer and its Logos Blockchain node, unmodified, a devnet with one validator and leader proofs in dev mode, whose channel holds one key. The checkpoint core parses and re-hashes all 202 blocks and checks all 202 inscriptions exactly as made (no mismatch; the inscribed blocks carry status `Pending`, the RPC serves them `Finalized`; the channel's posting timeframe is 0 on Bedrock). The lab's channel key certified an attestation key, which attested block 202 final by the node's own rule (n = 1, t = 1). 195 blocks, 585 transactions, 7.66M cycles, proven in 39.5 s on one NVIDIA L4, wrapped to Groth16; the v5 vault in mode (B), with RISC Zero's verifier, accepts it, finalises it after the hour, and refuses a flipped seal byte, another key set and an early finalisation; 347,832 + 195,149 gas. No bridge program runs in the lab: no burns. `checkpoint/RESULTS.md`, "Real LEZ blocks".

## 19. Specified but not yet in the prototypes

The vault prototype (v5) implements D3 to D5, D9, the vault items of D11 and D13 with D13's fees, and both rules; the bridge program on LEZ (`lez/BRIDGE-RESULTS.md`) implements the LEZ side of D13's inbound items, fees, caps and freezes; the checkpoint prototype implements D3 with rule (A), and rule (B) in its own guest; the core and the release guest implement SPEC-v2. Specified, and missing from them:

| Source | Item | Prototype today |
|---|---|---|
| D11 | refunds, and releases whose delay equals `revokeWindow`, open only after the last revocable second | built (vault v4) |
| D11 | `finalize` also compares `t` and the tip, and checks `importedLeafCount <= nextLeafIndex` | built (vault v4: the whole journal, by its digest) |
| D11 | further logic in a linked library | built (`FerrataRegistry`; v5's runtime with the security fixes 24,254 bytes, 322 under EIP-170) |
| D12 | `mac` v2 covering `memo` | the core: MAC without `memo`; the bridge program's wire (`lez/bridge-wire`) has `mac` v2 |
| D13 | rule (B): `final-tier/v2` message, attestation-key certificates, `ferrata_attestor`, program and vault path, immutable `mode`, 1 h wait | program and vault paths built and measured; `ferrata_attestor` built, also as a Logos module (rows 94–95) |
| D13 | `submitCandidate` refuses a `newState` already pending | built (v3 and v4) |
| D13 | watchers; cancelling candidates whose key set lags | vault side built (since v4: up to three watchers); LEZ side built in the bridge program (12-hour pauses, branch `bridge-controls`); `monitor` built (`monitor/`, [EVIDENCE](../EVIDENCE.md) rows 43–45), also as a Logos module (row 96) |
| D13 | 14-day freeze-authority replacement; renounced stop owner; no refunds after a stop; keyless retirement after 30 days; cap floor | built (since vault v4); on LEZ, the 14-day handover, a multisig freeze authority and the keyless `Sunset` built in the bridge program (branch `bridge-controls`, `lez/BRIDGE-RESULTS.md` §14); the stop owner's renunciation is a step of the deployment script, run in its fork rehearsals only (`vault/README.md`, "Deployment") |
| D13 | `ImportRoot` over slots 0 and 1; claim bound `leafIndex < count`; refunds on the release bucket; gas-drop receipt binding | refunds on the release bucket built (since vault v4); `ImportRoot` over slots 0 and 1 and the claim bound built in the bridge program; the gas-drop receipt built (`bridge/`, [EVIDENCE](../EVIDENCE.md) rows 47, 48) |
| D13 | `NOut` v2; burn account keyed on the whole note; newest root past its delay | `NOut` v2 in the release guest and the core since the security fixes (review M-3) and in the bridge program's wire; the burn account keyed on the whole note built in the bridge program; the newest-root rule is a wallet rule, in the `ferrata` tool (`cli/src/flow.rs`) and in `ferrata_bridge_module`'s release (EVIDENCE row 93) |
| D13 | fees escrowed at deposit; checkpoint fee; default caps | fees escrowed at deposit and the checkpoint fee built (vault v5, `Fees.t.sol`); default caps: the rate-limit mechanism and the cap floor only |

Built already, for reference: D12's tagged canonical `burnFrontierHash` and the 64-witness cycle-count gate.

## 20. Known limits

Documented, not fixed in this version (beyond the unbuilt items of §19):

- An unrevoked wrong checkpoint older than `revokeWindow` halts the vault for good.
- A tripped verifier stop ends releases and refunds (D13); owed balances stay claimable with `claimOwed` in the prototype.
- Cancellation is not sticky; under rule (A), outside F1–F4, the watchers are trusted for safety.
- Until replaced, a compromised freeze authority can renew freezes, revoke honest checkpoints within the window and retire the vault: denial of service, not theft.
- A payee that can never receive keeps its `owed` balance forever.
- Retired keys stay counted until a finalised checkpoint carries the rotation; `exit_delay` (about 40 min on testnet values) is shorter than `W`. Configuration branches can stall the vault until a revocation or a new vault.
- One silent key stalls rule (A) for `n <= 5`.
- Until B1, a second configuration of the same vault can freeze a public-kind deposit whose `s` it knows, and a destination's holder account can be filled (§3, "Known issues, 18 November 2026"); the program's second build, not deployed, has neither.
- A change of turn length or slot length forces retirement; escrow backing tokens never burned stays in the vault.
- Busy support blocks make rule (A) checkpoints expensive (54.1M cycles per counted block with four private transactions).
- Rule (A) needs an unpredictable `prevrandao`.
- Issuer risk (blacklisting, transfer-fee switches) is inherited; rebasing tokens are unsupported.
- Image ids: since `integration7` every guest builds byte-identical at any path ([EVIDENCE](../EVIDENCE.md) row 82, [`docs/image-ids.md`](../image-ids.md)); the Dockerfile that pins the toolchains has not been built.

## 21. What a fully verified return leg would need upstream

On v0.4 the most possible is what §8 does: prove inclusion, take validity and finality from the sequencers (findings §10). Removing that trust needs:

1. **A state commitment in LEZ blocks**: a state, nullifier or receipts root inside the block hash.
2. **Metering a proof can check**: fees, the gas cap and out-of-gas reverts that do not depend on unattested r0vm cycle counts, or a specified cycle accounting a guest may emulate exactly.
3. **Ledger roots in Logos Blockchain headers**, and a light-client specification, so finality can be proven without replaying the ledger.

With these, a checkpoint program could prove validity and finality directly, and, since the release path asks only whether a burn root is accepted, replace rule (A) or (B) in a new configuration.
