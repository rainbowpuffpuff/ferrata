# The Ferrata bridge program on LEZ v0.4.0-rc1, measured through LEZ's state machine

The bridge program of RFP-022 (`guests/bridge`, one RISC Zero guest), with its wire crate
(`bridge-wire/`) and the harness that drives it (`harness/src/bridge.rs`,
`harness/tests/bridge_{inbound,private,burn,controls,vault_storage}.rs`). The plan, the actors and
the messages are in [BRIDGE.md](BRIDGE.md). Since branch `bridge-complete` the program also has the
registry attested from the vault, fees by leaf index, token and global caps, freezes and roles,
and `Sunset` (§8); on branch `integration` it is merged with the Gloas header of `inbound-live`
(§2a) and runs against that branch's attestation program. **Images.** The merged program is
`f11b1aba55fde1ea977f74a54cc927ff50253e8bf479c89dac0b3658177c58eb` built at the integration
worktree's path (the ELF holds its build paths); `bridge-complete`'s own was `b5d177bb…1132` at its
path. The figures marked *after* are the merged program's, measured on the integration build box;
those marked *before* are the program as first integrated (136cf35), re-run on `bridge-complete`'s
build box before any change: image `64faa2e3…7267` at that path, every gas figure equal to the
tables the integration branch committed for `25dfd9b2…8c49` (only the image and the ids derived
from it differ). The inbound crossing on a live sequencer (§11) deploys neither: it uses the frozen
program of `inbound-live` (`66722fc7…f4fa`). §7's real proofs are of `25dfd9b2…8c49`, the
program before this extension. Every number below is LEZ's own accounting of a transaction that
ran: `lee`'s state transition functions and the copy of LEZ's fee settlement that Stages 1–4 use
([RESULTS.md](RESULTS.md) §1), each transaction in a block of its own, the attestation program
answering the bridge's `Attest` calls. They ran on this worktree's Modal build box
(`spike/modal-build/mrun`, 2 vCPU), not on the laptop; the executor is deterministic, so the cycle
counts do not depend on the machine. Private executions ran in RISC Zero dev mode. The tables are
`results/bridge-inbound.md`, `results/bridge-private-claim.md`, `results/bridge-burn.md` and
`results/bridge-controls.md`, written by the tests.

**What is synthetic.** The Ethereum side. Two vaults' storage (slot 0 the deposit root, slot 1 the
leaf count, slot 2 `lezConfigId`, and the registry and fee-schedule slots `BRIDGE.md` §5 defines)
sits in the state tries of three synthetic execution
blocks built with the reference trie of `ferrata-spectests`, and those blocks' records are placed in the
attestation program's `header(id, slot)` actors at genesis instead of being written by the light
client after a finality update (Stages 1–4 measured that path on mainnet data). From there on
everything runs through LEZ: the bridge program attests the vault's slots by calling the
attestation program's `Attest` inside its own transactions. Vault A's first five leaves are the
shared vector's claim tree (`../bridge/vectors/inbound-v2.json`, `claim`), so `root(A, 5)` is the
vector's root and the plaintext claim of its leaf 2 spends the vector's `N`
(`57f4ad38…e547`); leaves 5–7 are the harness's own deposits. Vault A registers ETH (and T3, whose
second fee schedule breaks the bounds), with a second ETH fee schedule from leaf 6; vault B
registers ETH and a six-decimal token T2, with a T2 deposit at its leaf 5. The real Sepolia vault
comes later.

## 1. The operations, before and after

| operation | handlers run, after (cycles each) | execution gas, before → after | storage gas (tx bytes), before → after |
|---|---|---:|---:|
| `Init`: attest the vault's slot 2 = `configId`, write `config(cid)` | Init 43,668 → Attest 85,725 → reply 39,214 | 154,529 → 168,607 | 1,726 → 1,946 |
| `ImportRoot` (8 leaves): attest slots 0 and 1, write `root(cid, n)` | ImportRoot 49,536 → Attest 112,749 → reply 52,945 → WriteRoot 16,328 | 203,620 → 231,558 | 1,703 → 2,025 |
| `Register` a token: attest its entry, fee slots 0 and 1, the count | Register 72,084 → Attest 150,208 → reply 73,421 → WriteToken 29,035 | new: 324,748 | 2,747 |
| `ImportFees`: a schedule; the absence of the next one | ImportFees 54,087 → Attest 112,749 → reply 57,503 → ExtendFees 38,134; 48,326 → 107,315 → 51,842 → 36,134 | new: 262,473; 243,617 | 2,206; 1,945 |
| plaintext claim, 1 ETH under fee schedule 0 (relay fee only) | Claim 107,526 → Issue 47,239 → Admit 34,291 → Spend 21,901 → Mint 14,237 → Mint 14,237 | 142,444 → 239,431 | 1,740 → 2,061 |
| plaintext claim, 10 ETH under schedule 1 (mint and relay fee) | Claim 107,526 → Issue 47,250 → Admit 34,003 → Spend 24,145 → three Mints (14,237; 14,237; 19,181) | 142,456 → 260,579 | 1,740 → 2,061 |
| private claim, private destination (one privacy-preserving transaction) | private: ClaimPrivately 41,133, reply 111,838 (user cycles); public: Read, Issue, Admit, Spend, Mint (the relay fee) | 439,815 → 539,060 | 224,063 |
| private claim, public destination | private: the same two; public: Read, Issue, Admit, Spend, three Mints | 457,925 → 573,154 | 224,063 |
| public burn of one denomination, top-level, signed by the holder | Burn (holder) 26,461 → Burn (token) 49,322 → Admit 34,250 → Burn (record) 20,835 | 45,867 → 130,868 | 647 → 775 |
| private burn of one denomination | private: Burn 24,397 (user cycles); public: Burn (token), Admit, Burn (record) | 427,959 → 513,593 | 224,063 |
| `SetCap` (admin), a token's mint bucket | SetCap (admin's actor) 12,790 → SetCap (token) 36,839 | new: 49,629 | 733 |
| `Freeze` (freeze authority): a token's mints; all mints | 12,483 → 34,060; 11,871 → 31,380 | new: 46,543; 43,251 | 710; 690 |
| `Sunset` (freeze authority) | Sunset (its actor) 11,140 → Sunset (`config(cid)`) 30,844 | new: 41,984 | 671 |
| `Tick` at a token; at the configuration | 32,821; 27,213 | new: 32,821; 27,213 | 460; 440 |

Every transaction is under `TARGET_GAS_EXEC` (5,000,000); the largest applied public one is a
`Register` (360,698 in §8, 7.2%; 448,038 on the vault's own storage, §8a). **What the merge
changed.** Against `bridge-complete`'s own run only the operations that attest move, because the
attestation program of `inbound-live` (which attests under Gloas records) answers a little more
expensively: `Init` +782 execution gas, `ImportRoot` +1,815 (8 leaves), `Register` +2,454,
`ImportFees` +1,822; claims, burns, roles and the private transitions are unchanged. **What the
controls cost.** A claim now runs two more handlers (`Issue` at the token: the
denomination, the fee schedule, the token's freeze and bucket; `Admit` at the configuration: the
global ones) and one `Mint` per fee: +96,987 execution gas for a plaintext claim of 1 ETH
(×1.68), +118,123 with a mint fee. A burn runs the token's handler and `Admit`: +85,001 (×2.85).
`Init`, the import and the claim's own verification grew a little with larger states (the gate
and `sunset` in `config(cid)`; the import's `Attest` also proves a vault storage trie that now holds
the registry slots, so its proofs are longer: the import message is 1,499 bytes, 1,177 before).
The private transactions' execution gas is LEZ's `PRIVATE_VERIFY_GAS` (409,764) plus the cycles of
their public part (derived: 129,296 for the private claim, 163,390 with the public mint, 103,829 for
the private burn); their storage gas is LEZ's fixed 224,063 whatever the dev-mode transaction
weighs (8,953 bytes for the private claim, 7,857 for the private burn).

**What a prover proves for the bridge** is almost unchanged, because the controls run in public
execution: the private claim's two transitions take 41,133 and 111,838 user cycles (before 39,314
and 113,417), one segment each (131,072 and 262,144 with padding); the private burn's one
transition 24,397 (before 23,799; 131,072). They were not proven for real for this image; §7's
proofs of the earlier image (15.3 s and 12.4 s on one NVIDIA L4) are the expectation.

**The attestation inside an import costs less here than it would on mainnet.** The synthetic state
trie holds 42 accounts and each vault's storage trie at most nine slots, so each proof is 2–3 nodes; a
storage slot under a mainnet state root is 9 account-proof nodes plus about 7 storage-proof nodes,
and Stage 4 measured 274,842 cycles for one account and one slot (`RESULTS.md` §5). A real
vault's import (one account, two slots) and a registration (four slots) should therefore cost
roughly that plus one or three more slot proofs [E: not measured]; the bridge's own handlers do not
depend on it. §8a runs on the storage a real vault wrote (139 and 149 slots), still under a
synthetic state trie of 41 accounts.

## 2. Inbound: configuration, import, claims (`results/bridge-inbound.md`, 32 transactions)

`Init` computes `configId` on chain with the bridge core (`config_id_preimage`), taking
`bridgeHeader` from the program's own account, and writes `config(cid)` only if the vault's slot 2
holds it: the reciprocal pairing of B5, through the same direct path as the import. `ImportRoot`
names `(record_slot, root, leaf_count)` in its message — the checkpoint reads them there — and the
bridge compares them with what the attestation proves; `root(cid, n)` is keyed by the leaf count,
since the vault's tree is append-only and one count has one root.

The plaintext claim of the shared vector's deposit mints 1 ETH to the vector's destination, the
registered relay fee (0.001 ETH, schedule 0 has no mint fee) to the relayer the claim names, and
spends exactly the vector's `N`: the program's claim statement is the bridge core's
`verify_claim`, reused, not re-implemented. The payer's 10-ETH deposit at leaf 6 falls under
vault A's second fee schedule: the payer gets exactly 10 ETH, the fee recipient the 0.05-ETH mint
fee, the relayer 0.01 ETH. Refusals, each with the program's own code (the
blocks of `wire::CODE_BLOCKS` leave 240–255 unassigned; `bridge-wire` owns them and a test checks
the overlap):

| case | code | execution gas |
|---|---|---:|
| `Init` of another configuration of vault A (another fee recipient): slot 2 holds `configId(A)` | 246 `PAIRING_MISMATCH` | 161,761 |
| `Init` of an existing configuration | 244 `ALREADY_WRITTEN` | 38,101 |
| import at a count already imported (refused by `WriteRoot`, after the attestation) | 244 `ALREADY_WRITTEN` | 233,436 |
| import whose message claims count 7, or the 5-leaf root, at a block holding 8 | 247 `IMPORT_MISMATCH` | 200,503; 200,023 |
| import for configuration A with vault B's proofs (the request names vault A) | 110 `ProofNodeMismatch` (the core's, from `Attest`) | 90,829 |
| import for a configuration never initialised | 245 `NOT_FOUND` | 15,124 |
| the same claim again (replay), refused by the nullifier after `Issue` and `Admit` | 250 `ALREADY_CLAIMED` | 207,707 |
| the same deposit under a second root, `root(A, 8)` | 250 `ALREADY_CLAIMED` | 207,707 |
| leaf 6 under `root(A, 6)`, imported from a block whose count lags its root | 248 `LEAF_NOT_IMPORTED` | 31,738 |
| one sibling changed; the leaf as 1 ETH instead of 10; its preimage at index 4 | 249 `CLAIM_INVALID` | 94,494 each |
| the destination redirected to another account | 249 `CLAIM_INVALID` | 94,494 |
| a vault-A deposit under `root(B, 8)` (another configuration's vault) | 249 `CLAIM_INVALID` | 94,494 |
| a deposit of 0.5 ETH (not a denomination), refused by the token after the path | 251 `NOT_REGISTERED` | 133,412 |
| a private-kind deposit by the plaintext path | 252 `WRONG_DESTINATION` | 98,120 |
| under a root never imported | 245 `NOT_FOUND` | 27,303 |
| `Mint` sent to a holder directly | 243 `WRONG_SENDER` | 10,237 |

A refusal reverts the whole transaction, so nothing is written: the test checks the balances and
nullifier accounts after every case. The test registers ETH for both vaults from their storage and
imports vault A's second fee schedule before the claims (`bridge_controls.rs` has the registry's
own cases, §8). The lagging-count case is the only one where the index check
does work the path check would not: block 102's vault holds the root of eight leaves with a count
of six, so leaf 6's path is valid under `root(A, 6)` and only `leafIndex < count` refuses it (D13).

## 2a. Under a record in the Gloas layout (`results/bridge-gloas.md`)

Every Sepolia record since Gloas binds the block hash only; the attestation program then takes the
state root from the block's RLP header, which the bridge's messages carry in front of the vault's
account proof (`bridge-wire` `HEADER_MARKER`, from branch `inbound-live`): `Init`, `ImportRoot` and,
since the merge with `bridge-complete`, `Register` and `ImportFees`, which share one request builder
(`vault_query`). `harness/tests/bridge_inbound.rs` `inbound_under_a_gloas_record`: a record binding
the hash of a real Sepolia header (block 12,103,422, the replay fixture's bound block) whose state
root is replaced by the synthetic state's, placed after genesis; RISC Zero dev mode, the Modal build
box, the merged program (`f11b1aba…`).

| transaction | handlers run (cycles each) | execution gas | storage gas (tx bytes) | result |
|---|---|---:|---:|---|
| `Init`, no header | Init 44,953 → Attest 19,716 | 64,669 | 1,968 | 215 `HEADER_REQUIRED` |
| `Init`, a header that does not hash to the record (last byte changed) | Init 52,893 → Attest 35,873 | 88,766 | 2,695 | 120 `HeaderHashMismatch` |
| `Init`, header in front of the account proof | Init 52,893 → Attest 112,452 → reply 39,214 | 204,559 | 2,695 | applied |
| `Register` ETH, no header | Register 71,768 → Attest 24,707 | 96,475 | 2,673 | 215 `HEADER_REQUIRED` |
| `Register` ETH, with the header | Register 82,628 → Attest 175,195 → reply 73,421 → WriteToken 29,035 | 360,279 | 3,400 | applied |
| `ImportRoot`, no header | ImportRoot 48,884 → Attest 21,484 | 70,368 | 1,885 | 215 `HEADER_REQUIRED` |
| `ImportRoot` (2 leaves), with the header | ImportRoot 57,941 → Attest 134,981 → reply 52,945 → WriteRoot 16,328 | 262,195 | 2,612 | applied |
| plaintext claim under `root(C, 2)` (relay fee to the relayer) | Claim 107,526 → Issue 44,380 → Admit 34,291 → Spend 21,901 → Mint 14,237 → Mint 14,237 | 236,572 | 2,061 | applied |
| the same claim again | Claim 107,526 → Issue 44,092 → Admit 34,003 → Spend 19,227 | 204,848 | 2,061 | 250 `ALREADY_CLAIMED` |

Against the payload-layout `Init` of §1, the header adds 749 bytes to the transaction, 26,727
cycles to `Attest` (decoding and hashing the header) and 9,225 to the bridge's own `Init`, which
carries it; `Register` grows alike. The largest, `Register`, stays at 7.2% of LEZ's 5M execution
target. Before the merge (branch `inbound-live`, commit 5b75f64, the program the live crossing of
§11 deploys) the same test had no `Register` and its applied `Init` cost 192,310.

## 3. The private claim (`results/bridge-private-claim.md`)

`ClaimPrivately` at the destination's own private account sends `Read` to `root(A, 8)`, keeps the
claim in its private state until the root comes back, verifies it with the same function as the
plaintext path (including the leaf index against the fee schedule range it names), credits its own
state with 1 ETH, and sends `Issue` to `token(A, ETH)`, which applies the fees, freezes and caps and
sends `Admit` and `Spend`. The transaction's public part, as `describe_public_outputs` decodes it,
is: seven public actors (the root, the token, the configuration, the nullifier, the fee recipient,
the relayer, the fee inbox); the 1-byte `Read`; the 121-byte `StateReply` (the root's 117-byte
state); the 175-byte `Issue`; the fee `Credit(100,000,000)` from a fresh alias; the program image
claim (the bridge, disclosed); one private action (1,343 bytes: nullifier, commitment root,
commitment, encrypted note). `Issue` carries `N`, `configId`, the token, the denomination, the fee
schedule's range (`[0, 6)`: schedule 0), the root read (`n = 8`), the relayer and the time, and no
destination and no leaf index; the relay fee is then minted publicly to the relayer.

That is §B6's list for a claim (`N`, token, denomination, the root read, the bridge program, a
one-time fee alias with a fixed reserve), plus the fee schedule's range and the relayer: the mint
fee is public and differs between schedules, so which schedule paid is public in any case
(`BRIDGE.md` §2). All 8,953 bytes of the transaction were searched for the claim's private inputs:
the destination (the private account's id), `s`, `C`, the deposit leaf, the depositor, the borsh
witness and each of the 32 path siblings: all absent; `N`, `configId` and the root are present. No
public state exists at the private account afterwards; its new commitment is in the state (the
harness rebuilds the account with its 1 ETH holding and finds the commitment, so the private
balance is checked, not assumed).

The same path with a public-kind deposit, run from a private account of the claimant's own, mints
10 ETH publicly to the payer, the mint fee of schedule 1 to the fee recipient and the relay fee to
the relayer (573,154 execution gas): the destination is visible, the deposit's
leaf and secret are not. A replay cannot be proven: the public part refuses in simulation
("Program exited with non-zero code 250 after 19790 cycles"). A private-kind deposit claimed by a
private account that is not its destination ends the private part with 252 after 101,110 cycles,
so LEZ's prover makes no receipt and nothing is spent.

## 4. Burns (`results/bridge-burn.md`)

The note is made as a wallet makes it: `r`, `sOut`, `COut` and `memo` with the bridge core, and
D12's `mac` (over `memo` as well). A public holder's `Burn` is a top-level call it signs, at its
own holder actor: it debits one denomination and sends the same `Burn` message to
`token(cid, token)`, which accepts it from an actor of the bridge only, checks the denomination and
the burn freezes and buckets (its own, then the configuration's through `Admit`) and sends it to
`burn(cid, H(note))`, which accepts it from that token actor only and writes the note once. A
private holder's `Burn` is the root of its own private execution; its delivery to
`token(cid, token)` is a private→public boundary message, so the note is public and the holder
appears as an alias.

| case | code | execution gas |
|---|---|---:|
| burn 1 ETH (configuration A), then 1 ETH (B), by the payer | applied | 130,868; 127,348 |
| the same note again (the holder still holds 9 ETH) | 244 `ALREADY_WRITTEN` | 132,113 |
| 0.5 ETH (refused by the token after the debit) | 251 `NOT_REGISTERED` | 56,918 |
| 1 unit of another token (the holder holds none of it) | 254 `INSUFFICIENT_BALANCE` | 20,758 |
| 10 ETH with 9 held; under a configuration the holder holds nothing of | 254 `INSUFFICIENT_BALANCE` | 21,059; 20,525 |
| at another holder's actor, not signed by it | 253 `NOT_AUTHORIZED` | 19,575 |
| as a nested call from LEZ's `scripted` program, in a transaction the holder signed (`is_authorized` holds) | 253 `NOT_AUTHORIZED` | 31,891 |
| sent to the burn account itself; to the token's account itself | 243 `WRONG_SENDER` | 16,224; 28,347 |
| `Echo` (rule (A)'s nonce): accepted anywhere, no state | applied | 11,947 |
| `Sunset` by the payer, which is not the freeze authority (forwarded, refused by `config(A)`) | 253 `NOT_AUTHORIZED` | 39,277 |

The nested case is D10's: a holder's signature authorises every delivery to its account in the
transaction, at any depth, so the bridge cannot rely on `is_authorized` alone; it refuses a burn
that carries a sender, because the checkpoint would not see a nested call and the tokens would be
destroyed without a burn leaf. An asynchronous message is delivered as a transaction's root with its
sender as `from` (`lee/state_machine/core/src/execution_state.rs`, lines 1090–1113 of the tag), so
it is refused the same way; that was read in the source, not run (the harness delivers no
asynchronous messages). NOut (D13) is wallet and release side, `H("ferrata/nf-out/v2" ‖
sOut)`, here `bridge-wire::nf_out_v2`: the test shows that v1 (`H(… ‖ sOut ‖ burnId)`) changes with
the index a burn is collected at and v2 does not. The bridge program never sees `sOut` or `kView`.

## 5. What the checkpoint collects, and the mismatches

The test puts the public transactions (as settled) and the two privacy-preserving ones into one
LEZ block, the repeated burn marked reverted, and reads it with `checkpoint-core`'s own
`parse_block` and `BridgeMessage::decode`, as fixed on the integration branch (mismatch 1 below):

| transaction | origin | bytes | the checkpoint's decoder | collected |
|---|---|---:|---|---|
| `ImportRoot` (n = 8) | public root | 1,499 | `ImportRoot`, its proofs skipped (before the fix: refused, `TrailingBytes { at: 81 }`) | yes |
| plaintext claim | public root | 1,279 | ignored (variant 4 and up) | no |
| public burn, A | public root | 185 | `Burn` | yes |
| the same note again | public root | 185 | `Burn` | no (reverted) |
| public burn, B | public root | 185 | `Burn` | yes |
| `Echo` | public root | 33 | `Echo` | yes |
| private claim: `Read`, `Issue` | private → public | 1; 175 | ignored | no |
| private claim: the root's reply | public → private | 121 | — | no (not public execution) |
| private burn (its delivery to the token's account) | private → public | 185 | `Burn` | yes |

It collects exactly the import and the three burns that happened, in order, each burn with its
note and configuration; from the import's bytes the program's decoder and the checkpoint's read the
same four fields; its burn leaf equals the bridge core's; its `configId`, computed from the inputs it
pins (with this prototype's `attestationRootId` and `salt`, §6), equals the program's; and it
re-encodes the program's `Burn` byte for byte (185 bytes, its `BURN_LEN`). The extension moved the
burn's second hop (now holder → token → record) and added `Issue`, `Admit` and the fee mints, all
nested or private and so invisible to the checkpoint; what it collects is unchanged. `Sunset` is
now built; §8 shows the checkpoint collecting it once, from the freeze authority's visible call.

**Mismatches, and which side should change.**
1. **`ImportRoot` was not collected; fixed on the checkpoint side** (severe if left:
   `importedLeafCount` would never advance, and after a sunset the vault would refund deposits
   that are still claimable on LEZ). The checkpoint's variant 1 was exactly its four fields; the
   program needs the account and two storage proofs in the same transaction, and a public
   transaction's only visible data is its root message, so they follow the four fields
   (`ImportProof`: three borsh `Vec<Vec<u8>>`). The checkpoint now decodes variant 1 as the four
   fields followed by `ImportProof`, walks its borsh lengths strictly and skips it
   (`checkpoint-core`'s `wire.rs`; `wire_import_root_skips_its_proof_strictly`, and this test on
   the program's own transaction). The bare four fields are refused. The checkpoint's two guests
   changed with it (`../checkpoint/RESULTS.md`), and the end-to-end fixture, whose import has the
   old form, is now refused by the fixed program (`../e2e/README.md`).
2. **Replies are never visible calls, and the checkpoint should rely on that only knowingly.**
   The checkpoint decodes every visible message to a bridge actor, while the program first treats
   a message from the actor it waits for as a reply (an `Attestation`, a `StateReply`), not as a
   bridge message. Today no reply to a bridge actor is a visible call (the attestation reply is
   nested in public execution; the root's reply to a private claimant is public→private), so the
   two decoders never disagree. A future handler that waits for a reply from a private actor
   would break that; the program side should keep it so. The checkpoint's README states the
   dependency ("A dependency on the bridge program").
3. **Configuration identity.** The checkpoint takes `attestationRootId` and `salt` as opaque
   inputs; this prototype defines them (§6). No change, but a deployment must pin the same values.

## 6. Departures from the specification, and why

1. **The token: the M0 fallback** (`BRIDGE.md` §3). No v0.4 token program is released, so the
   bridge keeps wrapped balances as its own actor state at the holder's account, per configuration
   and token, in the token's base units; the registry's LEZ unit divides every denomination and
   every fee minted on LEZ, so a switch to a token program converts exactly. Users' balances stay
   whole denominations; the fee recipient's and relayers' hold fees.
2. **`attestationRootId = H("ferrata-lez/v1/bridge/attestation-root" ‖ attestation program ‖
   instance id)`.** An instance id alone does not name the program whose records it trusts; LEZ
   v0.4 deploys one image at many addresses.
3. **`salt = H("ferrata-lez/v1/bridge/controls" ‖ borsh(controls))`**: `configId` — which the vault
   pins and holds in slot 2 — commits to the roles (admin, freeze authority, fee recipient) and the
   cap defaults and floor, so a front-run `Init` cannot install others. The registry is no longer in
   the configuration: each token is registered from the vault's own registry (§8).
4. **`root(cid, n)` is keyed by the leaf count** and a second import at the same count is refused;
   `root(n)` in the proposal does not say what `n` is.
5. **`ImportRoot` carries its proofs after the checkpoint's four fields** (§5, mismatch 1).
6. **One configuration router.** `Init`, `ImportRoot`, `Register` and `ImportFees` run at
   `config(cid)`, which keeps the outstanding request in its state for the length of the
   transaction and clears it on the reply; its configuration never changes, its gate and `sunset`
   do.
7. **Records at genesis** (above): the light client's path to a record is Stages 1–4's.
8. **Exit codes share where 16 do not suffice** (`bridge-wire`, `code`): `CLAIM_INVALID` covers a
   wrong path, leaf, index, destination or vault (they cannot be told apart without the deposit,
   and a code that told them apart would say more than a refusal should, B6); `NOT_FOUND` a missing
   configuration and a missing root; `BAD_MESSAGE` also every value outside the bounds of §B4;
   `IMPORT_MISMATCH` every attested fact that is not what the message says; `LEAF_NOT_IMPORTED`
   also a leaf outside the fee schedules known; `FROZEN` also an import after `Sunset`. To make
   room, 242 (`WRONG_RECEIVER`, now `WRONG_SENDER`) became `FROZEN` and 255 (`BURN_EXISTS`, now
   `ALREADY_WRITTEN`) became `CAPPED`. `lez/wire/src/lib.rs` (`CODE_BLOCKS`) is not edited: its
   line numbers are in the attestation guest's ELF.
9. **The vault's registry and fee slots are defined here** (`BRIDGE.md` §5): SHA-256 slot keys,
   so the guest needs no keccak and the slots do not depend on the vault's layout. FerrataVault v5
   writes them; §8a runs the program on the storage it wrote.
10. **Burns carry no time.** The 185-byte `Burn` is the checkpoint's, with no room for one; a
    burn's freezes and buckets use the latest time its token and configuration actors accepted
    (from claims, role messages and `Tick`). Conservative: a burn bucket never refills beyond a
    validated time and a lapsed freeze holds until the time advances (§8 shows both).
11. **Roles fixed per configuration**: the freeze authority, the admin and the fee recipient are LEZ
    accounts in the configuration; replacing the freeze authority after a 14-day timelock, the
    watchers, and a permissionless `Sunset` after 30 days without a finalised checkpoint are not
    built (the attestation carries no block timestamp, and the vault has no slot for it). The
    freeze authority acts through one LEZ account's signature, not a multisig (no v0.4 multisig
    program was looked for).
12. **Each fee is held to the 1% ceiling separately** (mint, burn, checkpoint, relay), and schedule 0
    must charge no protocol fee; "protocol fees start at zero" read that way.

## 7. Real proofs of the private claim and the private burn (the program before this extension)

These proofs are of image `25dfd9b2…8c49`, the program as integrated, before §8's controls. The
extension changes the private claim's and burn's code and messages, so the recorded receipts do not
settle against the current program (its image claim differs); the extended program's private claim
and burn ran in dev mode in the tests and in `prove_bridge`'s dry run (settled with fake receipts,
this worktree's box), not proven for real. §1 gives their user cycles: the same segment counts as
below.

`harness/src/bin/prove_bridge.rs` makes the private claim of §3 (leaf 5, 1 ETH, to its private
destination) and then that holder's private burn of §4 on the state of the tests
(`World::with_roots`), each proven by `r0vm` and settled through LEZ's private settlement with the
receipt verified. Proven on one NVIDIA L4 (`../gpu-prover/RESULTS.md`, job `lez-bridge`, run
`gpu-l4-bridge`), bridge image `25dfd9b2…8c49`:

| transaction | private transitions (user cycles, padded) | `execute_and_prove` | receipt | transaction | execution gas | storage gas |
|---|---|---:|---:|---:|---:|---:|
| private claim | 39,314 (131,072) + 113,417 (262,144) | 15.3 s | 227,477 B | 231,828 B | 439,815 | 224,063 |
| private burn | 23,799 (131,072) | 12.4 s | 226,978 B | 230,830 B | 427,959 | 224,063 |

The run took 33.0 s for both (0.78 GiB peak host memory, 11,384 MiB peak GPU memory). The charges are
the dev-mode figures of §1: the private part's cost is LEZ's fixed `PRIVATE_VERIFY_GAS`, whatever the
receipt. Settled again on the build box from the two files (`results/prove-bridge-settle.log`):
both receipts verify, the claimed holding, the nullifier, the recorded note and the emptied holding
are found, and the claim's 231,828 bytes hold none of its private inputs (the search of §3); with
bit 0 of one byte inside either receipt flipped, settlement refuses it ("Invalid privacy preserving
execution circuit proof"). The transactions: `results/bridge-private-claim-tx.borsh`,
`results/bridge-private-burn-tx.borsh`; the GPU log `results/prove-bridge.log`.

## 8. Registry, fees, caps, freezes and `Sunset` (`results/bridge-controls.md`, 72 transactions)

`bridge_controls.rs` starts from the two configurations, registers tokens from the vaults'
storage, imports roots and fee schedules, then exercises the caps, the freezes and the roles, and
retires configuration B. Every case below ran through LEZ; a refusal reverts its whole
transaction (the test checks the balances, nullifiers and token states after them).

| case | code | execution gas |
|---|---|---:|
| **Registry.** `Register` ETH (A): entry, schedules 0 and 1, count attested at block 100 (schedule 1 absent: covers 5 leaves) | applied | 324,748 |
| ETH (A) again (refused by `WriteToken`, after the attestation) | 244 `ALREADY_WRITTEN` | 348,012 |
| T2 at vault A, which holds no entry for it; ETH (B) with relay fees other than the vault's | 247 `IMPORT_MISMATCH` | 260,219; 280,007 |
| ETH for configuration A', never initialised | 245 `NOT_FOUND` | 20,064 |
| nine denominations; a denomination not a multiple of the LEZ unit; a 2% relay fee; a LEZ unit that is not a power of ten; schedule 0 with a mint fee (refused before any attestation) | 240 `BAD_MESSAGE` | 34,232; 35,556; 35,760; 34,985; 39,673 |
| ETH (B), then T2 (B): a second token of one configuration | applied | 332,606; 337,987 |
| **Fees by leaf index.** A claim of leaf 6 naming the range `[0, 8)` before schedule 1 is known (`known_until` = 5) | 248 `LEAF_NOT_IMPORTED` | 131,996 |
| `ImportFees` ETH (A) schedule 1 at block 100, where it is not yet; schedule 2 before 1; schedule 1 twice | 247 `IMPORT_MISMATCH` | 196,979; 232,089; 251,643 |
| schedule 1 at block 101 (`known_until` → 6), then its absence of a schedule 2 at 8 leaves (→ 8) | applied | 262,473; 243,617 |
| leaf 6 in schedule 1's range `[6, 8)` before the range was closed at 8 | 248 `LEAF_NOT_IMPORTED` | 133,437 |
| T3's schedule 1 as its vault wrote it: one day of notice; a 2% mint fee (refused by the token) | 240 `BAD_MESSAGE` | 252,281; 250,496 |
| leaf 6 under schedule 0's range `[0, 6)`; leaf 2 under schedule 1's `[6, 8)` (refused by the claimant) | 248 `LEAF_NOT_IMPORTED` | 31,744; 31,741 |
| **Caps.** `SetCap` by the admin: ETH (A) mints 11 ETH, 0.01 ETH/s | applied | 49,629 |
| below the floor; by the freeze authority; by a signer with no role; sent to the token's account itself | 240; 253 `NOT_AUTHORIZED` (three) | 39,385; 38,917; 38,917; 25,079 |
| leaf 6: 10 ETH to the payer, 0.05 to the fee recipient, 0.01 to the relayer (10.06 ETH of the bucket) | applied | 255,922 |
| leaf 2 (1.001 ETH with its relay fee) with 0.94 ETH left; ten seconds later (0.1 ETH refilled) | 255 `CAPPED`; applied | 134,180; 244,554 |
| B's global mint cap at the floor (1 ETH): B's 1 ETH (1.001 ETH with its fee); B's 1,000 T2 (1 ETH of value units) | 255 `CAPPED`; applied | 179,068; 222,998 |
| the T2 claim with a time one hour ahead of the block (LEZ's validity window, no program code; charged the gas limit) | reverted | 10,000,000 |
| ETH (A) burns capped at 1 ETH: a burn; a second; the second again 100 s later without `Tick`; after `Tick` | applied; 255; 255; applied | 130,578; 57,531; 57,531; 130,701 |
| **Freezes and roles.** The freeze authority freezes ETH (B) mints for one hour; a claim of B's 1 ETH | applied; 242 `FROZEN` | 46,543; 132,706 |
| the admin unfreezes it; a freeze of 30 days and 1 ms | 253 `NOT_AUTHORIZED`; 240 `BAD_MESSAGE` | 37,050; 37,524 |
| the freeze authority unfreezes ETH (B), freezes all mints of B for 30 days; the claim | applied; 242 `FROZEN` (at `config(B)`) | 46,525, 43,251; 179,041 |
| ETH (A) burns frozen: a burn; unfrozen, all burns of A frozen: a burn; an hour later without `Tick`; after `Tick A` | 242; 242; 242; applied | 57,504; 102,555; 102,555; 130,579 |
| **`Sunset`.** By a signer with no role; by the admin; sent to `config(B)` itself | 253 `NOT_AUTHORIZED` | 34,908; 34,908; 22,584 |
| by the freeze authority; again | applied; 244 `ALREADY_WRITTEN` | 41,984; 35,367 |
| `ImportRoot` B afterwards (refused before the attestation) | 242 `FROZEN` | 26,764 |
| B's 1 ETH claimed after `Sunset` (the 30-day freeze lapsed); 1 ETH (B) burned after it | applied | 248,683; 129,237 |

The global cap counts value units across tokens (`amount / lezUnit × unitWeight`): B's T2 claim fit
the 1-ETH global bucket that B's ETH claim (1.001 ETH with its relay fee) did not. A capped or frozen
claim leaves its deposit claimable: leaf 2 and B's 1 ETH were each claimed once, later. The
validity window is LEZ's: the program asks for `[t, t + 10 min)`, settlement refuses a block outside
it, and LEZ charges an uncoded failure the whole gas limit.

**What the checkpoint collects of a `Sunset`.** One LEZ block with five of the transactions above,
the refused ones marked reverted, read with `checkpoint-core`'s `parse_block` and
`BridgeMessage::decode`:

| transaction | origin | bytes | the checkpoint's decoder | collected |
|---|---|---:|---|---|
| `Sunset` B by a signer with no role | public root | 33 | `Sunset` | no (reverted) |
| `Sunset` B by the freeze authority | public root | 33 | `Sunset` | yes |
| `Sunset` B again | public root | 33 | `Sunset` | no (reverted) |
| `ImportRoot` B after `Sunset` | public root | 1,392 | `ImportRoot` | no (reverted) |
| a burn of 1 ETH (B) after `Sunset` | public root | 185 | `Burn` | yes |

It collects exactly one `Sunset` (byte for byte its variant 2, 33 bytes) and the burn. The role
acts through its own actor of the bridge at the top level, so the `Sunset` that executes is a
visible call; the program refuses a `Sunset` that is not (D10's rule, as for burns).

## 8a. On FerrataVault's own storage (`results/bridge-vault-storage.md`, 23 transactions)

The vault storage of §1–§8 is synthetic. `vault/test/RegistryStorage.t.sol` runs FerrataVault v5
itself in a local EVM (Foundry): the vault suite's four default tokens and a six-decimal token with
checkpoint fees registered, three deposits (moment P); then ETH's schedule 1 (mint 0.5%, burn 0.1%,
checkpoint 0.05% of each denomination) announced and in force seven days later from the next
deposit, and three more deposits (moment Q). It exports every slot the vault and its library wrote
(139 at P, 149 at Q, read with `vm.record`/`vm.accesses`) and what the vault's events say (entries,
schedules, deposits), with the depositors' secrets, to `vault/test/vectors/registry-storage.json`,
which every `forge test` compares with a fresh export. Nothing there computes a LEZ-side slot or
hash. `harness/tests/bridge_vault_storage.rs` builds an execution block holding exactly that storage
for each moment (`Block::with_storage`) and runs the program on it (dev mode, build box):

| step | execution gas |
|---|---:|
| `Register` of each of the five tokens: the vault's entry and schedule 0 at P | 424,496–448,038 |
| `ImportRoot` of the vault's own slots 0 and 1, at P (3 leaves) and at Q (6 leaves) | 277,456; 278,292 |
| `ImportFees`: ETH's schedule 1 at Q; the absence of each token's next schedule | 306,172; 277,600–288,838 |
| a claim of each of the six deposits | 236,493–266,875 |
| refused with 247 `IMPORT_MISMATCH`: an entry with another `lezUnit`; schedule 0 a second later; schedule 1 with another mint fee | 374,134; 376,671; 252,988 |

Each claim mints exactly the denomination to its destination, the registered relay fee to the
relayer and, for ETH's leaves 3 and 5, schedule 1's mint fee to the fee recipient; the roots are
the bridge core's tree over the leaves recomputed from the `Deposit` events. So the encodings of
`BRIDGE.md` §5 agree on both sides (the entry, the schedules, the slot keys, slot 1 holding the
leaf count alone), and neither side was changed. The vault's real storage trie gives longer proofs
than the synthetic one: `Register` costs about 425,000 here against 324,748 in §8 (`Attest`
219,872–244,382 cycles). Two parts are not the vault's: `Init` runs under a synthetic block whose
slot 2 holds the configuration's id, since a vault built in Foundry cannot hold a `configId` that
depends on this program's image (the pairing is §2's subject); and the depositors' destinations are
added as LEZ accounts, as the world's genesis adds its own (a public account must exist to be
admitted).

## 9. What is not built

A permissionless `Sunset` (30 days without a finalised checkpoint), the 14-day
replacement of the freeze authority and the watchers on LEZ; a multisig freeze authority; the gas
drop; a transfer between holders; the switch to a token program; real proofs of the extended
program (§7 is of the earlier image) and of a private claim to a public destination; a run of this
program on a sequencer (§11's crossing deploys the frozen earlier program); a vault on Sepolia (§8a's
vault ran in a local EVM) and records written by the light client in the same run; the checkpoint program running
over these blocks end to end (only its parser and decoder ran); the off-chain importer, wallet and
relayer.

## 10. Reproduce

```sh
cd lez
export RISC0_HOME=$PWD/../../risc0-home-lez        # guest Rust 1.91.1, as RESULTS.md §8
RISC0_DEV_MODE=1 cargo test -p harness --test bridge_inbound --test bridge_private --test bridge_burn \
  --test bridge_controls --test bridge_vault_storage -- --nocapture
                    # write results/bridge-{inbound,gloas,private-claim,burn,controls,vault-storage}.md
(cd ../vault && forge test --match-contract RegistryStorageTest)   # §8a: the vault's export, compared
cargo build -p harness --locked --bin prove_bridge   # §7
RISC0_DEV_MODE=1 target/debug/prove_bridge --allow-dev-mode   # dry run: the same commands, fake receipts
scripts/stage-prove-bridge.sh ../gpu-prover/inputs-lez-bridge   # then ../gpu-prover/README.md, job lez-bridge
RISC0_DEV_MODE=0 target/debug/prove_bridge --settle results/bridge-private-claim-tx.borsh \
  results/bridge-private-burn-tx.borsh   # the proven pair of §7: only on the tree that proved it (136cf35)
```
On the integration worktree they ran through `../../modal-build/mrun` (a cloud container, 2 vCPU)
with `--locked` and `RISC0_BUILD_LOCKED=1`: the whole harness (35 tests) in 290 s, `prove_bridge`'s
dry run (claim and burn settled with fake receipts), rustfmt and clippy (`-D warnings`) clean.

## 11. On a live sequencer: the inbound crossing (stand-in, then Sepolia)

[`live/inbound/README.md`](live/inbound/README.md): the same operations on a running LEZ
v0.4.0-rc1 sequencer and Logos Blockchain node (the Modal build box, 10 Nov 2026), on a stand-in
beacon chain (a committee whose keys we hold, Gloas-layout records binding blocks made from a real
Sepolia header with a synthetic vault's state root): the programs deployed through LEZ's loader
from the frozen binaries whose ids the Sepolia vault will pin; `Init` refused without the block's
header (215) and applied with it (181,965 execution gas); `ImportRoot` n = 1 and n = 2 (232,278
each); the plaintext claim (142,432), its replay and the same deposit under the later root refused
(250), another destination refused (249); the private claim at LEZ's private test account 0 with a
real proof made by LEZ's wallet on the box's CPU in 254.8 s, 252,185 bytes, 439,815 execution gas
and 224,063 storage gas, included by the sequencer, which verifies receipts. Measured (the
sequencer's own charges, re-checked by replaying the recorded chain through LEZ's `chain_state`).
On public Sepolia (11 Nov 2026, [`live/inbound/README.md`](live/inbound/README.md) "The real
session"): deposit 0 into the live vault `0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7` (creation tx
`0x0a5c3a64…9f12`, deposit tx `0xf3235de7…89a0`), captured after its finality, claimed on the same
kind of zone: the instance from the period-1378 bootstrap through Gloas (32 period updates, 160
transactions) to finalized slot 11551744 (block 12107997); `Init` on `configId` `a2e880cb…d3b8` (=
the vault's `storedLezConfigId`, 355,935 execution gas) after the refusal without the header (215);
`ImportRoot` n = 1 (426,479); a leaf index at the count refused (248), another destination refused
(249), the plaintext claim to LEZ's public test account 0 (142,432), its replay refused (250). 186
blocks, re-checked by the full replay. Deposit 1 (private kind, real proof) waits for capture 2.

