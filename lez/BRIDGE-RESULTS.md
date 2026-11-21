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
those marked *before* are the program as first integrated (a66a486), re-run on `bridge-complete`'s
build box before any change: image `64faa2e3…7267` at that path, every gas figure equal to the
tables the integration branch committed for `25dfd9b2…8c49` (only the image and the ids derived
from it differ). The inbound crossing on a live sequencer (§11) deploys neither: it uses the frozen
program of `inbound-live` (`66722fc7…f4fa`). §7's real proofs are of `25dfd9b2…8c49`, the
program before this extension. **Since branch `lez-time-fix`** (§12) the program's freezes and caps
run on attested Ethereum time instead of a caller's, and **since the security fixes of 11 November**
(§13) public balances sit at `holder(account)` PDAs and `ImportRoot` is accepted only as a visible
top-level call. The merged program (both changes) was image
`cf41b7e5c0583686a273b5114368f4873bbd614249647f43604a6caf8e675390` (541,464 ELF bytes) at the `integration5` worktree's path. Every `results/bridge-*.md` is of the program the live runs deployed, `ccde6171…` (592,840 ELF bytes, the same built at any path from `f13faee2`: [`../docs/image-ids.md`](../docs/image-ids.md)), recorded on 12 November 2026 (since milestone B1 this tree builds `ac1fb59b…`, whose records are [`results/b1/`](results/b1/README.md)); it adds §14's controls and answers through the attestation program of milestones A1 and A2.
§12 gives each operation before and after the time fix (that branch alone: `d0c11833…23cd` at its
worktree's path), §13 before and after the security fixes (that branch alone: `fe2f2487…946d`); the text
of §1–§5 and §8a keeps the figures of the program before both (`f11b1aba…58eb`), and §8's rows on caps,
freezes and `Sunset` are the time fix's run: both are superseded by the tables in `results/`,
whose figures §1's last column gives (§12 and §13 keep each fix branch's own run as history). Every number below is LEZ's own accounting of a transaction that
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

| operation | handlers run, after (cycles each) | execution gas, before → after | storage gas (tx bytes), before → after | the deployed program as recorded (`results/`, `ccde6171…`, 12 November): execution gas; storage gas |
|---|---|---:|---:|---|
| `Init`: attest the vault's slot 2 = `configId`, write `config(cid)` | Init 43,668 → Attest 85,725 → reply 39,214 | 154,529 → 168,607 | 1,726 → 1,946 | 171,027; 1,946 |
| `ImportRoot` (8 leaves): attest slots 0 and 1, write `root(cid, n)` | ImportRoot 49,536 → Attest 112,749 → reply 52,945 → WriteRoot 16,328 | 203,620 → 231,558 | 1,703 → 2,025 | 236,224; 2,025 |
| `Register` a token: attest its entry, fee slots 0 and 1, the count | Register 72,084 → Attest 150,208 → reply 73,421 → WriteToken 29,035 | new: 324,748 | 2,747 | 331,816; 2,747 |
| `ImportFees`: a schedule; the absence of the next one | ImportFees 54,087 → Attest 112,749 → reply 57,503 → ExtendFees 38,134; 48,326 → 107,315 → 51,842 → 36,134 | new: 262,473; 243,617 | 2,206; 1,945 | 269,684 and 250,335; 2,206 and 1,945 |
| plaintext claim, 1 ETH under fee schedule 0 (relay fee only) | Claim 107,526 → Issue 47,239 → Admit 34,291 → Spend 21,901 → Mint 14,237 → Mint 14,237 | 142,444 → 239,431 | 1,740 → 2,061 | 261,671; 2,053 |
| plaintext claim, 10 ETH under schedule 1 (mint and relay fee) | Claim 107,526 → Issue 47,250 → Admit 34,003 → Spend 24,145 → three Mints (14,237; 14,237; 19,181) | 142,456 → 260,579 | 1,740 → 2,061 | 290,962; 2,053 |
| private claim, private destination (one privacy-preserving transaction) | private: ClaimPrivately 41,133, reply 111,838 (user cycles); public: Read, Issue, Admit, Spend, Mint (the relay fee) | 439,815 → 539,060 | 224,063 | 553,641; 224,063 |
| private claim, public destination | private: the same two; public: Read, Issue, Admit, Spend, three Mints | 457,925 → 573,154 | 224,063 | 603,981 (the test's output, 12 November); 224,063 |
| public burn of one denomination, top-level, signed by the holder | Burn (holder) 26,461 → Burn (token) 49,322 → Admit 34,250 → Burn (record) 20,835 | 45,867 → 130,868 | 647 → 775 | 166,477; 839 |
| private burn of one denomination | private: Burn 24,397 (user cycles); public: Burn (token), Admit, Burn (record) | 427,959 → 513,593 | 224,063 | 519,678; 224,063 |
| `SetCap` (admin), a token's mint bucket | SetCap (admin's actor) 12,790 → SetCap (token) 36,839 | new: 49,629 | 733 | 91,019; 789 (through `config(cid)`, §14) |
| `Freeze` (freeze authority): a token's mints; all mints | 12,483 → 34,060; 11,871 → 31,380 | new: 46,543; 43,251 | 710; 690 | 140,609 and 95,939; 839 and 755 (read the newest record) |
| `Sunset` (freeze authority) | Sunset (its actor) 11,140 → Sunset (`config(cid)`) 30,844 | new: 41,984 | 671 | 44,193; 671 |
| `Tick` at a token; at the configuration | 32,821; 27,213 | new: 32,821; 27,213 | 460; 440 | 95,555 and 80,628; 525 and 505 (read the newest record) |

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
target. Before the merge (branch `inbound-live`, commit 335814f, the program the live crossing of
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
10. **No message carries a time** (since §12). Freezes and buckets use each actor's clock: the
    newest execution timestamp it has read from a record of the configuration's attestation
    instance, moved only by `Tick` and `Freeze`. Conservative: a bucket never refills beyond
    attested Ethereum time and a lapsed freeze holds until something reads a later record (§8 and
    §12 show both). Before §12 claims and role messages carried a caller's time under a validity
    window, which one block producer could move (review r3, attack A).
11. **Roles fixed per configuration**: the freeze authority, the admin and the fee recipient are LEZ
    accounts in the configuration; replacing the freeze authority after a 14-day timelock, the
    watchers, and a permissionless `Sunset` after 30 days without a finalised checkpoint are not
    built (the program now has an attested clock, §12, but the vault has no slot recording its last
    finalised checkpoint). The
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

## 8. Registry, fees, caps, freezes and `Sunset` (`results/bridge-controls.md`, 78 transactions)

`bridge_controls.rs` starts from the two configurations, registers tokens from the vaults'
storage, imports roots and fee schedules, then exercises the caps, the freezes and the roles, and
retires configuration B. Every case below ran through LEZ; a refusal reverts its whole
transaction (the test checks the balances, nullifiers and token states after them). Since §12 time
moves only when a later block's record exists and a `Tick` or `Freeze` reads it: the test ticks
both configurations with block 102's record before registering (tokens start at their
configuration's clock), and every "later" below is a record that much later. The registry and fee
rows are the program before §12 (their figures moved by +90 to +3,500 gas, §12); the rows from
**Caps** on are the new run's.

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
| **Caps.** `SetCap` by the admin: ETH (A) mints 11 ETH, 0.01 ETH/s | applied | 55,903 |
| below the floor; by the freeze authority; by a signer with no role; sent to the token's account itself | 240; 253 `NOT_AUTHORIZED` (three) | 43,818; 43,347; 43,347; 28,620 |
| leaf 6: 10 ETH to the payer, 0.05 to the fee recipient, 0.01 to the relayer (10.06 ETH of the bucket) | applied | 262,601 |
| leaf 2 (1.001 ETH with its relay fee) with 0.94 ETH left; again once a record 10 s later exists, before any `Tick`; `Tick` ETH (A) with that record; leaf 2 (0.1 ETH refilled) | 255 `CAPPED`; 255; applied; applied | 137,575; 137,575; 94,979; 250,645 |
| B's global mint cap at the floor (1 ETH): B's 1 ETH (1.001 ETH with its fee); B's 1,000 T2 (1 ETH of value units) | 255 `CAPPED`; applied | 183,295; 227,692 |
| ETH (A) burns capped at 1 ETH: a burn; a second; the second once a record 100 s later exists, before `Tick`; `Tick`; the burn | applied; 255; 255; applied; applied | 135,847; 61,110; 61,110; 94,979; 136,264 |
| **Freezes and roles.** The freeze authority freezes ETH (B) mints for one hour (the target first reads the newest record); a claim of B's 1 ETH | applied; 242 `FROZEN` | 104,274; 136,099 |
| the admin unfreezes it; a freeze of 30 days and 1 ms after the record's time; a freeze until the record's time itself | 253 `NOT_AUTHORIZED`; 240 `BAD_MESSAGE` (two) | 41,945; 94,141; 94,130 |
| the freeze authority unfreezes ETH (B), freezes all mints of B for 30 days; the claim | applied; 242 `FROZEN` (at `config(B)`) | 104,262, 93,524; 183,680 |
| ETH (A) burns frozen: a burn; unfrozen, all burns of A frozen: a burn; once a record an hour later exists, before `Tick`; `Tick A`; the burn | 242; 242; 242; applied; applied | 61,081; 107,958; 107,958; 76,898; 136,142 |
| **`Sunset`.** By a signer with no role; by the admin; sent to `config(B)` itself | 253 `NOT_AUTHORIZED` | 37,029; 37,036; 23,861 |
| by the freeze authority; again | applied; 244 `ALREADY_WRITTEN` | 44,104; 37,489 |
| `ImportRoot` B afterwards (refused before the attestation) | 242 `FROZEN` | 27,040 |
| `Tick B` with a record 30 days later; B's 1 ETH claimed after `Sunset` (the 30-day freeze lapsed); 1 ETH (B) burned after it | applied | 76,899; 254,248; 133,744 |

The global cap counts value units across tokens (`amount / lezUnit × unitWeight`): B's T2 claim fit
the 1-ETH global bucket that B's ETH claim (1.001 ETH with its relay fee) did not. A capped or frozen
claim leaves its deposit claimable: leaf 2 and B's 1 ETH were each claimed once, later. The
earlier row of a claim whose caller time lay an hour ahead of its block (refused by LEZ's validity
window, charged the gas limit) is gone: no message carries a time now (§12).

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

Since branch `bridge-controls` (§14) the program has the keyless `Sunset`, the freeze authority's
14-day handover, a multisig (M-of-N) freeze authority, the watchers on LEZ and transfers between
holders. Not built: the gas drop in the relayer daemon (this program's private claim swept a relayer's sealed credit
and paid its fee from it on LEZ v0.4.0-rc3, real proofs, `lez/results/v0.4.0-rc3/gas-drop/`); a transfer that credits a private recipient privately (a private
holder's transfer reaches a public `holder(account)`, §14); the switch to a token program; a real proof of
the extended program's burn (§7 is of the earlier image; §14 proves the private claim and
private transfer of `bridge-controls`' image, `43615dc4…`) and of a private claim to a public destination; a run of this program on a sequencer (§11's crossing deploys the frozen earlier
program); a vault on Sepolia (§8a's vault ran in a local EVM) and records written by the light client
in the same run; the checkpoint program running over these blocks end to end (only its parser and
decoder ran); the off-chain importer, wallet and relayer; the admin's own handover on LEZ (the admin is
fixed per configuration; the vault has `transferAdmin`).

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
  results/bridge-private-burn-tx.borsh   # the proven pair of §7: only on the tree that proved it (a66a486)
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
blocks, re-checked by the full replay. That run's deposit 1 was never sent; the private claim of a real Sepolia
deposit by this tree's program is the crown run's ([`e2e/runs/crown-live-1/`](../e2e/runs/crown-live-1/README.md)).


## 12. Attested time: the clock of freezes and caps (branch `lez-time-fix`)

*History: the figures below are this branch's own run, before the merge; the merged program's (both fixes)
are in `results/` and in §1's last column, and supersede them.*

**Why.** Review r3 (attack A): LEZ v0.4.0-rc1 neither bounds nor orders block timestamps, and the
program's time was the caller's, under a validity window around the block's timestamp and advanced
by an open `Tick`. One block producer could stamp its block a year ahead, carry a `Tick`, end every
freeze, refill every bucket and leave the freeze authority unable to freeze again. **What changed**
(`BRIDGE.md` §6): each gate's clock is the newest execution timestamp it has read from a record of
the configuration's attestation instance; `Tick` (still open) and `Freeze` read that record
(`Read` of `header(instance, slot)`; a Gloas record with its block's RLP header); the clock only
moves forward; the transaction is valid only in a block stamped at or after the record's time; no
message carries a time (`Claim`, `ClaimPrivately`, `Issue`, `Admit`, `SetCap` lost theirs).

**Where and what ran.** The Modal build box through `mrun` (2 vCPU), 11 Nov 2026, this
worktree at commit `5b27f08` plus rustfmt: `RISC0_DEV_MODE=1 cargo test -p harness --locked`, 37
tests, all pass (the 36 of `integration4` and `bridge_time.rs`); clippy `-D warnings` and rustfmt
clean on the LEZ crates; `prove_bridge`'s dry run (claim and burn settled with fake receipts); the
CLI's tests build and its unit tests pass. Image `d0c11833f2944d1d928bb4a40ffe9300591561dba0c7ca20f1c66265258b23cd`
(535,576 ELF bytes) at this worktree's path (`prove_bridge --image-ids`). Not proven for real; no run
on a sequencer.

**`results/bridge-time.md`** (`harness/tests/bridge_time.rs`, 34 transactions):

| case | result | execution gas |
|---|---|---:|
| `Tick` at a configuration; at a token (the newest record, 40 min behind the LEZ clock) | applied | 76,899; 94,980 |
| freeze B's mints for one hour (reads the record) | applied | 93,525 |
| a block stamped **a year ahead**: `Tick B` with the newest record (clock unchanged); B's claim in it | applied; 242 `FROZEN` | 76,900; 183,266 |
| in that block, the freeze authority freezes B's mints for 30 days (bounded by attested time) | applied | 93,526 |
| ETH (A) mints capped at 11 ETH, 10.06 taken; in a block a year ahead: `Tick` with the newest record, `Tick` with an older record (clock unchanged, monotone), leaf 2's claim | applied; applied; 255 `CAPPED` | 94,981; 94,981; 137,575 |
| a record 10 s later: `Tick`, leaf 2's claim (exactly 0.1 ETH refilled: 0.039 ETH left, asserted) | applied | 94,979; 250,645 |
| a record 30 days after the freeze's start: `Tick B`, B's claim (the freeze lapsed) | applied | 76,899; 251,894 |
| **stalled light client** (no newer record, a year of LEZ blocks): `Tick A`; a burn under A's burn freeze; the freeze authority unfreezes; a burn; `Tick`; a second burn (bucket not refilled); the admin raises the burn cap to 100 ETH; a burn (raising refills nothing) | applied; 242; applied; applied; applied; 255; applied; 255 | 76,900; 108,251; 93,511; 136,140; 94,981; 61,110; 55,906; 61,110 |
| a record **an hour ahead of the block** (what only a captured sync committee could have written): `Tick` | reverted by LEZ (validity window, no program code), clock unchanged | 10,000,000 (the gas limit) |
| the same `Tick` in a block its producer also stamps an hour ahead | applied | 94,980 |
| Gloas record: no header; a header that does not hash to it; its own header | 247; 247; applied | 67,124; 181,187; 194,197 |
| a payload record with a header; a slot with no record | 247 `IMPORT_MISMATCH`; 245 `NOT_FOUND` | 78,779; 61,695 |

The refused far-ahead `Tick` costs its sender LEZ's whole gas limit (an uncoded failure, as before for a
caller time outside its window); an honest relayer never sends one, because an honest record is at
least two epochs behind real time.

**Each operation, before → after** (execution gas, storage gas in brackets; before = §1–§8's
figures of `f11b1aba…58eb`, after = this run):

| operation | before | after |
|---|---:|---:|
| `Tick` at the configuration | 27,213 (440) | 76,899 (505): `Tick` 32,485 → `Read` 11,877 → reply 32,537 |
| `Tick` at a token | 32,821 (460) | 94,979 (525) |
| `Tick` with a Gloas record (the header hashed in the guest, keccak in software) | — | 194,197 (1,196) |
| `Freeze` (freeze authority): a token's mints; all mints of a configuration | 46,543 (710); 43,251 (690) | 104,274 (775); 93,524 (755) |
| `SetCap` (admin), a token's mint bucket | 49,629 (733) | 55,903 (725) |
| `Sunset` (freeze authority) | 41,984 (671) | 44,104 (671) |
| plaintext claim, 1 ETH (schedule 0) | 239,431 (2,061) | 245,325 (2,053) |
| plaintext claim, 10 ETH (schedule 1) | 260,579 (2,061) | 267,540 (2,053) |
| private claim, private destination | 539,060 (224,063; 8,953 B) | 544,841 (224,063; 8,937 B) |
| public burn, 1 ETH | 130,868 (775) | 135,849 (775) |
| private burn | 513,593 (224,063) | 518,891 (224,063) |
| `Register` a token | 324,748 (2,747) | 327,760 (2,747) |
| `Init`; `ImportRoot` (8 leaves) | 168,607; 231,558 | 169,018; 231,648 |
| `Echo` (runs no changed code) | 11,947 | 12,231 |

Reading a record makes `Tick` and `Freeze` about 2.3 to 2.9 times dearer (one `Read` of the
attestation program and a second handler of the bridge); claims gain 2.5–2.7% and burns 3.8%,
without any time in their messages (claims 8 bytes shorter, private claims 16). Part of every
difference is the new image's layout: `Echo`, which runs none of the changed code, moved by +284
and the import by +90; the rest is the token state's 65 more bytes (its attestation program, instance
and pending read), decoded and written by every claim and burn. The private claim's two transitions
take 41,242 and 111,694 user cycles (before 41,133 and 111,838), the private burn's 24,654 (24,397).

**What it does not change.** The vault (its caps and freezes run on Ethereum's clock), the checkpoint
(variants 0–3 are byte for byte the same; `Tick` and `Freeze` are not among them) and the attestation
program (not edited; its `Read` serves the record). **What ran before it used the earlier program.**
The inbound crossing on a live sequencer and on Sepolia (§11) used the frozen program of
`inbound-live` (`66722fc7…f4fa`, with a caller's time), §7's real proofs are of `25dfd9b2…8c49`, and
every figure in §1–§5 and §8a of `f11b1aba…58eb`: none of them has this clock.

## 13. Security fixes of 11 November 2026 (branch `secfix-lez`)

*History: the figures below are this branch's own run, before the merge; the merged program's (both fixes)
are in `results/` and in §1's last column, and supersede them.*

The review `docs/reviews/security/review-2026-11-11.md` (of integration4 at `bc4a2a6`) found, on this
side: **C-1** a claim could write a holder state at any bridge PDA not written yet (as its public
destination, its relayer, or its configuration's fee recipient), blocking `Init`, `Register` or an import
for good, destroying a pending claim (its nullifier) or blocking a burn (its record); **H-1** `ImportRoot`
ran as a nested call or an asynchronous message, which the checkpoint cannot count. Fixes:

- **C-1.** Public wrapped balances moved from the actor state at the holder's own account to
  `holder(account)`, a PDA whose seed (`…/holder ‖ account`) names its role, so it can never be another
  actor's address. `Mint` names its holder and is accepted only at `holder(holder)`, seeded by the
  nullifier (LEZ admits a new holder PDA on the seed, no evidence needed). A public holder still burns by
  a top-level call it signs at its own actor; that actor debits its own state if it holds the amount
  (a private claimant's private balance) or forwards the note to `holder(account)`, which accepts it only
  from that own actor. Roles act through their own actors as before. Every other write is at a PDA whose
  handler checks its receiver, or at the own actor of an account that authorised the call.
- **H-1.** `ImportRoot` is refused (253 `NOT_AUTHORIZED`) unless `from = None`, as `Burn` already was.

**Tests** (`harness/tests/bridge_secfix.rs`, dev mode): (a) a claim whose destination is `config(H)` and
whose relay fee goes to the attacker configuration's fee recipient `config(H2)`, then `Init` of both;
(b) a claim with `relayer = token(H, ETH)`, then `Register`; (c) a claim to the nullifier account of a
pending claim, then that claim; (d) a claim to `root(H, 1)`, then the import; (e) a claim with the pending
burn's record as relayer, then the burn; (f) `Mint` delivered at `config(cid)` (refused) and at
`holder(config(cid))` (accepted), in the executor; H-1: `ImportRoot` forwarded by LEZ's `scripted`
program (refused, nothing written; the top-level import of the same count then succeeds), and
delivered with a sender, as LEZ presents an asynchronous message (executor: refused; without a sender:
accepted). **Before** (the new tests against `bc4a2a6`'s program, build box, `RUN=baseline`): all seven
of (a)–(e) and both H-1 tests fail, each because the attack worked: the claims wrote a holder state at
`config(H)`, `token(H, ETH)`, the nullifier, `root(H, 1)` and the burn record; the nested import and the
import with a sender were accepted. ((f) uses the new `Mint` field and has no before.) **After**
(`RUN=after1`): 8 of 8 pass; the whole harness 44 passed, 0 failed (36 before the new file), including the
checkpoint's collection in `bridge_burn.rs` (exactly the three burns, byte for byte); clippy clean.

**Cost** (`results/`, execution gas): a public burn now runs one more handler (the forward):
130,868 → 158,905 (`Burn 1 ETH (A) by the payer`), 775 → 839 transaction bytes (one more declared
actor); a plaintext claim 239,431 → 252,941 (`the shared vector's deposit`: the seeded mints);
`ImportRoot` 226,091 → 225,793; the private claim's two transitions 41,133 → 41,052 and
111,838 → 111,740 cycles. Refusals before any write cost less or about the same.

**Not changed here.** L-4 (the relay fee of a plaintext claim goes to whoever gets a copy included
first): the review's fix, `relayer == from`, does not stop a copier that names itself; the private claim
binds its relayer inside the proof (`docs/trust.md` §7). I-5 (time) is the `lez-time-fix` branch's.
I-9 is recorded in `BRIDGE.md` §3 (one mint authority per `(cid, token)` when a token program exists).
The live sequencer's frozen program (`live/frozen`) is not changed: it is what §11 ran, and it has C-1
and H-1; a new deployment must use this program, whose new image changes every `configId`.

## 14. The controls of milestone B2 (branch `bridge-controls`, `results/bridge-roles.md`, 78 transactions)

The keyless `Sunset`, the freeze authority's 14-day handover, a multisig (M-of-N) freeze authority,
watchers and transfers between holders, as `BRIDGE.md` §2 and §5 now specify them.
`harness/tests/bridge_roles.rs` (dev mode, build box `ws-8`, 16 cores) starts from the two
configurations (both ticked to block 102's record, ETH registered on both, `root(A, 8)` with ETH's
schedule 1, `root(B, 8)`, the payer's claim of A's leaf 6: 10 ETH) and runs every accepted path and
every refusal below; a refusal reverts its whole transaction, and the test checks the states after
each step (balances, the live roles, the gates' freezes, `sunset`). Image
`43615dc48098b3731990de7700204d2a1bea0e9f963ee7c4dd629048dd5ef19c` (591,084 ELF bytes) at this
worktree's path (the unchanged program builds to `0d3552601f93…70ed` there; `cf41b7e5…5390` at
`integration5`'s path). The table below is `results/bridge-roles.md` as re-recorded on 12 November 2026 with the deployed program (`ccde6171…`): every result and code as on `43615dc4…`; the steps that read or attest a record cost 837 to 1,117 more execution gas, the attestation program now being that of milestones A1 and A2.

**Design, in short** (the reasons are in `BRIDGE.md` §5):
- *Keyless `Sunset`.* Anyone sends `AttestRetired`: `config(cid)` attests the vault's slot 37 (the
  word FerrataVault v5 packs `freezeAuthority` and `retired` in; slot 35 is `admin`, as the vault's
  own storage export `vault/test/vectors/registry-storage.json` shows) under a record and requires
  byte 11, `retired`, to be 1. The vault sets it only in `retire()`, which anyone may call once D13's
  30 days without checkpoint progress have passed (`retireOpensAt`), so the vault evaluates the
  condition on Ethereum's own clock with the freeze time excluded, as designed. Afterwards anyone sends
  the checkpoint's own 33-byte `Sunset` as a top-level call at `config(cid)`.
- *Handover.* The live roles sit in `config(cid)` (`Roles`), not in `Controls` (which `configId`
  commits to). The admin's `ProposeRoles` (freeze authority and up to three watchers) reads a record
  and is valid only in a block stamped within a day after it; anyone's `ApplyRoles` applies it once a
  record 14 days later has been read; the admin's `CancelRoles` withdraws it. Every role message now
  passes `config(cid)`, which checks the live role and forwards a token's to `token(cid, token)`.
- *Multisig.* `Cosigned { signers, action }` is one transaction all signers sign: a top-level call
  at the first signer's own actor, forwarded along the signers' own actors (each checks that its
  account signed: LEZ marks an account authorised for every call in a transaction it signed, nested
  ones included) to `config(cid)`, which requires distinct members, at least the threshold. A
  multisig cannot send the checkpoint's 33-byte `Sunset` as its root call, so its `Sunset` approves;
  anyone then sends `Sunset` at `config(cid)`.
- *Watchers.* `Pause` reads a record and freezes mints and/or burns until 12 hours after it, never
  shortening a longer freeze; a watcher cannot unfreeze.
- *Transfers.* `Transfer` at the holder's own actor (a call it authorised) debits its own state
  (a private holder's balance) or forwards to its `holder(account)`, then `token(cid, token)` checks a
  multiple of the LEZ unit and sends `Credit` to `holder(to)`, accepted only from that token actor.

| case | code | execution gas |
|---|---|---:|
| **Transfers.** The payer moves 1 ETH (A) to a holder; the holder, signing, moves 0.1 ETH back; 1 ETH after `Sunset` | applied | 94,664; 99,086; 99,108 |
| 0; 1.5 gwei (not a multiple of the LEZ unit) | 240 `BAD_MESSAGE` | 67,492; 67,668 |
| 100 ETH (more than held); B's ETH (no holding) | 254 `INSUFFICIENT_BALANCE` | 37,024; 36,640 |
| from the holder's actor without its signature; sent to `holder(recipient)` itself | 253 `NOT_AUTHORIZED` | 10,606; 14,220 |
| sent to `token(A, ETH)` itself (a credit from nothing); `Credit` sent to `holder(payer)` itself | 243 `WRONG_SENDER` | 24,386; 14,059 |
| **Handover.** `ProposeRoles` by a signer with no role, by the freeze authority, sent to `config(A)` itself | 253 | 43,265; 43,487; 24,975 |
| threshold 0; 4 of 3; the admin as a member; a member twice; four watchers | 240 | 42,274; 42,275; 41,345; 41,383; 47,447 |
| `ApplyRoles`, `CancelRoles` with nothing proposed | 245 `NOT_FOUND` | 21,491; 37,664 |
| the admin proposes 2 of 3 and one watcher (block 102's record); cancels; proposes again | applied | 105,533; 47,216; 105,533 |
| `CancelRoles` by the freeze authority; `Pause` by the proposed watcher before the handover | 253 | 39,716; 40,396 |
| `ApplyRoles` at the proposal's own record; with a record 13 days later | 242 `FROZEN` | 76,502; 76,501 |
| `ApplyRoles` with a record 14 days later (anyone); again | applied; 245 | 84,935; 22,897 |
| **Multisig (2 of 3).** `Freeze` by the replaced key; by one member alone | 253 | 39,926; 39,926 |
| `Cosigned` by one member; naming two, signed by one; with a non-member | 253 | 42,480; 26,798; 60,369 |
| `Cosigned` freeze of A's mints for an hour; of ETH (A) burns (through `config(A)` to the token); unfreeze of A | applied | 121,465; 169,507; 121,487 |
| `Cosigned` freeze for 31 days | 240 | 112,732 |
| **Watchers.** `Pause` by a signer with no role; by a member of the freeze authority; of neither mints nor burns | 253; 253; 240 | 39,741; 39,741; 91,292 |
| `Pause` of A's mints and burns (12 h); of ETH (A) mints; again under a two-day freeze (kept: 2 days) | applied | 100,037; 149,936; 100,040 |
| an unfreeze by the watcher | 253 | 40,143 |
| `SetCap` ETH (A) by the admin, through `config(A)`; below the floor | applied; 240 | 92,984; 80,834 |
| **Sunset of A (multisig).** Sent at `config(A)` by anyone before approval; by the replaced key; by one member alone; `Cosigned` by one member | 253 | 24,224; 38,847; 38,847; 41,779 |
| `Cosigned` approval by two members; again | applied; 244 `ALREADY_WRITTEN` | 67,291; 59,448 |
| `Sunset` sent at `config(A)` by anyone, approved; again; `ImportRoot` A afterwards | applied; 244; 242 | 31,151; 24,224; 29,544 |
| **Keyless Sunset of B.** `Sunset` at `config(B)` by anyone, vault live | 253 | 22,818 |
| `AttestRetired` with slot 37 empty (block 100); with a freeze authority there but `retired` 0 | 247 `IMPORT_MISMATCH` | 163,972; 175,105 |
| `AttestRetired` with `retired` set (anyone); again | applied; 244 | 182,281; 26,394 |
| `Sunset` B forwarded by a signer's own actor (not a top-level call at `config(B)`) | 253 | 37,443 |
| `Sunset` at `config(B)` by anyone; again; `ImportRoot` B afterwards | applied; 244; 242 | 29,079; 22,818; 28,339 |

**What the checkpoint collects.** One LEZ block holding eight of the transactions above (the
refused ones marked reverted), read with `checkpoint-core`'s `parse_block` and
`BridgeMessage::decode`: it collects exactly the two `Sunset`s that executed, A's (after the
multisig's approval) and B's (after `AttestRetired`), each the checkpoint's own 33 bytes at the root;
the `Cosigned` approval, `AttestRetired` and `Transfer` decode as variants it ignores (20 and up).

**A private holder's transfer** (`results/bridge-transfer-private.md`, dev mode). The holder of the
private claim of A's leaf 5 (1 ETH) moves 0.1 ETH to a public holder in a private execution: one
private transition (19,113 user cycles, one segment, 131,072 padded); the transaction shows two
private-to-public deliveries from an alias, the 101-byte `Transfer` to `token(A, ETH)` (configuration,
token, amount, recipient) and the fee; the private account's id occurs nowhere in its bytes, and its
0.9 ETH is found only in its new commitment. A private recipient is not credited privately (not
built: the recipient's private state would have to be in the same private execution).

**Real proof of the private transfer** (`results/prove-bridge-transfer.log`,
`results/prove-bridge-transfer-settle.log`). `prove_bridge --transfer` makes §3's private claim of leaf
5 and then this transfer on the tests' state (`World::with_roots`), each proven by `r0vm` 3.0.5 on the
CPU of the build box (`RISC0_DEV_MODE=0`, wallet shape: seven private actions, 512-byte notes, as LEZ
v0.4.0-rc1's wallet pads every private transaction) and settled through LEZ's private settlement with
the receipt verified. Image `43615dc4…f19c`; the whole run 9 min 23 s wall clock, 9.8 GB peak memory,
3.77 CPU-hours in the 649-s command (`mrun`'s accounting, build and settlement included):

| transaction | `execute_and_prove` | receipt | transaction | execution gas | storage gas |
|---|---:|---:|---:|---:|---:|
| private claim (leaf 5, 1 ETH) | 295.0 s | 239,628 B | 256,130 B | 553,641 | 224,063 |
| private transfer (0.1 ETH to `holder(account)`) | 258.7 s | 238,898 B | 254,670 B | 463,434 | 224,063 |

Settled again from the two files with the receipts verified (`prove_bridge --transfer --settle`): both
settle (0.6 s and 0.3 s), the recipient's `holder(account)` holds 0.1 ETH, the holder's 0.9 ETH is found
in its new commitment, and the transfer's 254,670 bytes hold the `Transfer` to the token's account from
an alias and not the private account's id. With bit 0 of byte 253,670 flipped (inside the receipt,
bytes 15,772–254,670), settlement refuses it: "Invalid privacy preserving execution circuit proof".
An earlier attempt of the same run, beside the dev-mode suites on the same box, proved the claim in
413.4 s before the box was recycled. The transactions: `results/bridge-transfer-claim-tx.borsh`,
`results/bridge-private-transfer-tx.borsh` (they settle only against this image, built at this path).

**Effect on the earlier suites.** Every bridge suite passes on the new program (`bridge_roles` 2,
`bridge_controls` 1, `bridge_burn` 1, `bridge_private` 2, `bridge_inbound` 3, `bridge_time` 1,
`bridge_secfix` 8, `bridge_vault_storage` 1; the whole harness, 49 tests, passed on the image before
`rustfmt`, `25b3cc90…e776`), and `results/bridge-*.md` were of it until their re-recording with this tree's program (§1's last column). `bridge_controls.rs`'s 78
transactions keep every label and every result (applied or refused, with the same code) of
integration6's table; only gas moves. A token's `Freeze`, `SetCap` (and now `Pause`) pass
`config(cid)` first, one more handler: `SetCap` ETH (A) 55,805 → 91,019, `Freeze` ETH (B) for an hour
104,176 → 139,607, a refused one below the floor 43,720 → 79,206. Everything that reads or writes
`config(cid)` carries its `Roles`: `Init` 168,776 → 169,962, `Tick` at a configuration 76,845 →
79,627, `Register` 327,269 → 330,744, the claim of leaf 6 283,221 → 286,625, a global `Freeze`
93,426 → 95,102, `Sunset` by the freeze authority 44,002 → 44,193.
