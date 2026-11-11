# The Ferrata bridge program on LEZ v0.4.0-rc1, measured through LEZ's state machine

The bridge program of RFP-022 (`guests/bridge`, one RISC Zero guest; image
`c0c80a6434cd0bcdaf4b32b71d4f71bfb28e5e23564fa23dfa4686792b48f1ed` on the branch that built it, and
`25dfd9b283ba14b0b83a522d7d307de57288c4730f538d8862b53ad73a4a8c49` built from the same sources under
the integration branch's checkout path: the ELF holds its build paths. The tests re-run there gave
the same figures, apart from the image, the ids derived from it and the import of §5; the
`results/bridge-*.md` files are from that re-run, and §7's real proofs are of that image), with its wire crate
(`bridge-wire/`) and the harness that drives it (`harness/src/bridge.rs`,
`harness/tests/bridge_{inbound,private,burn}.rs`). The plan, the actors and the messages are in
[BRIDGE.md](BRIDGE.md). Every number below is LEZ's own accounting of a transaction that ran:
`lee`'s state transition functions and the copy of LEZ's fee settlement that Stages 1–4 use
([RESULTS.md](RESULTS.md) §1), each transaction in a block of its own, the attestation program answering the bridge's
`Attest` calls (built from this branch's unchanged sources; `RESULTS.md` records its image as
`2f3566d4…`, which these runs did not re-check). They ran on this worktree's Modal
build box (`spike/modal-build/mrun`), not on the laptop; the executor is deterministic, so the
cycle counts do not depend on the machine. Private executions ran in RISC Zero dev mode in the
tests; the private claim and the private burn were then proven for real (§7). The tables are `results/bridge-inbound.md`, `results/bridge-private-claim.md` and
`results/bridge-burn.md`, written by the tests.

**What is synthetic.** The Ethereum side. Two vaults' storage (slot 0 the deposit root, slot 1 the
leaf count, slot 2 `lezConfigId`) sits in the state tries of three synthetic execution blocks built
with the reference trie of `ferrata-spectests`, and those blocks' records are placed in the
attestation program's `header(id, slot)` actors at genesis instead of being written by the light
client after a finality update (Stages 1–4 measured that path on mainnet data). From there on
everything runs through LEZ: the bridge program attests the vault's slots by calling the
attestation program's `Attest` inside its own transactions. Vault A's first five leaves are the
shared vector's claim tree (`../bridge/vectors/inbound-v2.json`, `claim`), so `root(A, 5)` is the
vector's root and the plaintext claim of its leaf 2 spends the vector's `N`
(`57f4ad38…e547`); leaves 5–7 are the harness's own deposits. The real Sepolia vault comes later.

## 1. The operations

| operation | handlers run (cycles each) | execution gas | storage gas (tx bytes) |
|---|---|---:|---:|
| `Init`: attest the vault's slot 2 = `configId`, write `config(cid)` | Init 39,150 → Attest 83,231 → reply 33,174 | 155,555 | 1,726 |
| `ImportRoot`: attest slots 0 and 1 under one record, write `root(cid, n)` | ImportRoot 38,976 → Attest 103,817 → reply 44,676 → WriteRoot 17,859 | 205,328 | 1,703 |
| plaintext claim: path, opening of `C`, `N`, mint to a public `dest` | Claim 109,283 → Spend 17,756 → Mint 15,405 | 142,444 | 1,740 |
| private claim, private destination (one privacy-preserving transaction) | private: ClaimPrivately 39,314, reply 113,417 (user cycles); public: Read, Spend | 439,815 | 224,063 |
| private claim, public destination | private: the same two; public: Read, Spend, Mint | 457,925 | 224,063 |

Measured in RISC Zero dev mode by `harness/tests/bridge_inbound.rs`, `bridge_private.rs` and
`bridge_burn.rs` on the Modal build box (branch `inbound-live`, 10 Nov 2026), after the merge of the
attestation program that attests under Gloas records (`attest` 4c2df47) and the change that lets an
account proof carry the block's header (0ce0aa7): against the figures before, `Attest` costs 1,043
more cycles in `Init` (82,188 → 83,231) and 1,883 more in `ImportRoot` (101,934 → 103,817), the
bridge's `Init` 17 fewer (39,167 → 39,150) and its `ImportRoot` 175 fewer (39,151 → 38,976); the
claims, the private claim and the burns are unchanged. Under a Gloas record, see §2a.
| public burn of one denomination, top-level and signed by the holder | Burn (holder) 27,672 → Burn (record) 18,195 | 45,867 | 647 |
| private burn of one denomination (one privacy-preserving transaction) | private: Burn 23,799 (user cycles); public: Burn (record) 18,195 | 427,959 | 224,063 |

Every transaction is under `TARGET_GAS_EXEC` (5,000,000); the largest public one, the import, at
4%. The private transactions' execution gas is LEZ's `PRIVATE_VERIFY_GAS` (409,764) plus the
cycles of their public part (derived: 30,051 for the private claim, 48,161 with the public mint,
18,195 for the private burn — the last is exactly the public burn's recording handler). Their
storage gas is LEZ's fixed charge for a privacy-preserving transaction (224,063), whatever the dev
mode transaction weighs (8,815 bytes for the private claim, 7,817 for the private burn, with a
4,464-byte fake receipt; a real succinct receipt is about 227 KB, `RESULTS.md` §5).

**What a prover proves for the bridge** (executor, `results/bridge-private-claim.md`,
`results/bridge-burn.md`): the private claim's two transitions take 39,314 and 113,417 user
cycles, one segment each (131,072 and 262,144 with padding); the private burn's one transition
23,799 (one segment, 131,072). LEZ's privacy-preserving circuit over them is proven as well; `lee`
does not report its cycles. Neither was proven for real here; the private attestation of Stage 4
(382,336 user cycles, two segments) was, in 13.5 s on one NVIDIA L4 (`RESULTS.md` §5), which
bounds what to expect.

**The attestation inside an import costs less here than it would on mainnet.** The synthetic state
trie holds 42 accounts and the vault's storage trie 3 slots, so each proof is 2–3 nodes; a
storage slot under a mainnet state root is 9 account-proof nodes plus about 7 storage-proof nodes,
and Stage 4 measured 273,717 cycles for one account and one slot (`RESULTS.md` §5). A real
vault's import (one account, two slots, and a storage trie that grows with its depositors and its
frontier) should therefore cost roughly that plus a second slot proof [E: not measured]; the
bridge's own handlers (38,976 + 44,676 + 17,859) do not depend on it.

## 2. Inbound: configuration, import, claims (`results/bridge-inbound.md`, 28 transactions)

`Init` computes `configId` on chain with the bridge core (`config_id_preimage`), taking
`bridgeHeader` from the program's own account, and writes `config(cid)` only if the vault's slot 2
holds it: the reciprocal pairing of B5, through the same direct path as the import. `ImportRoot`
names `(record_slot, root, leaf_count)` in its message — the checkpoint reads them there — and the
bridge compares them with what the attestation proves; `root(cid, n)` is keyed by the leaf count,
since the vault's tree is append-only and one count has one root.

The plaintext claim of the shared vector's deposit mints 1 ETH to the vector's destination and
spends exactly the vector's `N`: the program's claim statement is the bridge core's
`verify_claim`, reused, not re-implemented. Refusals, each with the program's own code (the
blocks of `wire::CODE_BLOCKS` leave 240–255 unassigned; `bridge-wire` owns them and a test checks
the overlap):

| case | code | execution gas |
|---|---|---:|
| `Init` of another configuration of vault A (other denominations): slot 2 holds `configId(A)` | 246 `PAIRING_MISMATCH` | 150,670 |
| `Init` of an existing configuration | 244 `ALREADY_WRITTEN` | 30,282 |
| import at a count already imported (refused by `WriteRoot`, after the attestation) | 244 `ALREADY_WRITTEN` | 206,436 |
| import whose message claims count 7, or the 5-leaf root, at a block holding 8 | 247 `IMPORT_MISMATCH` | 175,048; 174,566 |
| import for configuration A with vault B's proofs (the request names vault A) | 110 `ProofNodeMismatch` (the core's, from `Attest`) | 80,491 |
| import for a configuration never initialised | 245 `NOT_FOUND` | 14,442 |
| the same claim again (replay) | 250 `ALREADY_CLAIMED` | 124,493 |
| the same deposit under a second root, `root(A, 8)` | 250 `ALREADY_CLAIMED` | 124,493 |
| leaf 6 under `root(A, 6)`, imported from a block whose count lags its root | 248 `LEAF_NOT_IMPORTED` | 30,946 |
| one sibling changed; the leaf as 1 ETH instead of 10; its preimage at index 4 | 249 `CLAIM_INVALID` | 93,788; 93,776; 93,788 |
| the destination redirected to another account | 249 `CLAIM_INVALID` | 93,788 |
| a vault-A deposit under `root(B, 8)` (another configuration's vault) | 249 `CLAIM_INVALID` | 93,788 |
| a deposit of 0.5 ETH (not a denomination) | 251 `NOT_REGISTERED` | 31,302 |
| a private-kind deposit by the plaintext path | 252 `WRONG_DESTINATION` | 97,336 |
| under a root never imported | 245 `NOT_FOUND` | 26,640 |
| `Mint` sent to a holder directly | 243 `WRONG_SENDER` | 10,731 |

A refusal reverts the whole transaction, so nothing is written: the test checks the balances and
nullifier accounts after every case. The lagging-count case is the only one where the index check
does work the path check would not: block 102's vault holds the root of eight leaves with a count
of six, so leaf 6's path is valid under `root(A, 6)` and only `leafIndex < count` refuses it (D13).

## 2a. Under a record in the Gloas layout (`results/bridge-gloas.md`)

Every Sepolia record since Gloas binds the block hash only; the attestation program then takes the
state root from the block's RLP header, which the bridge's `Init` and `ImportRoot` carry in front of
the vault's account proof (`bridge-wire` `HEADER_MARKER`). `harness/tests/bridge_inbound.rs`
`inbound_under_a_gloas_record`: a record binding the hash of a real Sepolia header (block
12,103,422, the replay fixture's bound block) whose state root is replaced by the synthetic state's,
placed after genesis; RISC Zero dev mode, the Modal build box, commit 5b75f64.

| transaction | handlers run (cycles each) | execution gas | storage gas (tx bytes) | result |
|---|---|---:|---:|---|
| `Init`, no header | Init 40,700 → Attest 19,520 | 60,220 | 1,780 | 215 `HEADER_REQUIRED` |
| `Init`, a header that does not hash to the record (last byte changed) | Init 48,775 → Attest 35,677 | 84,452 | 2,507 | 120 `HeaderHashMismatch` |
| `Init`, header in front of the account proof | Init 48,775 → Attest 110,361 → reply 33,174 | 192,310 | 2,507 | applied |
| `ImportRoot`, no header | ImportRoot 38,847 → Attest 21,092 | 59,939 | 1,757 | 215 `HEADER_REQUIRED` |
| `ImportRoot` (2 leaves), with the header | ImportRoot 47,321 → Attest 130,799 → reply 44,676 → WriteRoot 17,859 | 240,655 | 2,484 | applied |
| plaintext claim under `root(C, 2)` | Claim 109,271 → Spend 17,756 → Mint 15,405 | 142,432 | 1,740 | applied |
| the same claim again | Claim 109,271 → Spend 15,210 | 124,481 | 1,740 | 250 `ALREADY_CLAIMED` |

Against the payload-layout record of §1, the header adds 781 bytes to each transaction and about
27,000 cycles to `Attest` in `Init` (decoding the header and hashing it) and 27,000 in
`ImportRoot`; the bridge's own handlers carry the header (+9,625 in `Init`, +8,345 in
`ImportRoot`). Both stay under 5% of LEZ's 5M execution target.

## 3. The private claim (`results/bridge-private-claim.md`)

`ClaimPrivately` at the destination's own private account sends `Read` to `root(A, 8)`, keeps the
witness in its private state until the root comes back, verifies the claim with the same function
as the plaintext path, credits its own state with 1 ETH, and sends `Spend` to
`nullifier(chain, vault, N)`. The transaction's public part, as `describe_public_outputs` decodes
it, is: three public actors (the root, the nullifier, the fee inbox); the 1-byte `Read`; the
189-byte `StateReply` (the root's 185-byte state); the 186-byte `Spend` with one PDA seed; the fee
`Credit(100,000,000)` from a fresh alias; the program image claim (the bridge, disclosed); one
private action (1,419 bytes: nullifier, commitment root, commitment, encrypted note). `Spend`
carries `N`, `configId`, the configuration's terms, the root read (`n = 8`) and the denomination,
and no destination.

That is §B6's list for a claim: `N`, token, denomination, the root read, the bridge program, a
one-time fee alias with a fixed reserve. All 8,815 bytes of the transaction were searched for the
claim's private inputs: the destination (the private account's id), `s`, `C`, the deposit leaf,
the depositor, the borsh witness and each of the 32 path siblings: all absent; `N`, `configId` and
the root are present. No public state exists at the private account afterwards; its new
commitment is in the state (the harness rebuilds the account with its 1 ETH holding and finds the
commitment, so the private balance is checked, not assumed).

The same path with a public-kind deposit, run from a private account of the claimant's own, mints
10 ETH publicly to the payer (457,925 execution gas): the destination is visible, the deposit's
leaf and secret are not. A replay cannot be proven: the public part refuses in simulation
("Program exited with non-zero code 250 after 15210 cycles"). A private-kind deposit claimed by a
private account that is not its destination ends the private part with 252 after 99,388 cycles,
so LEZ's prover makes no receipt and nothing is spent.

## 4. Burns (`results/bridge-burn.md`)

The note is made as a wallet makes it: `r`, `sOut`, `COut` and `memo` with the bridge core, and
D12's `mac` (over `memo` as well). A public holder's `Burn` is a top-level call it signs, at its
own holder actor: it debits one denomination and sends the same `Burn` message to
`burn(cid, H(note))`, which accepts it from an actor of the bridge only and writes the note once.
A private holder's `Burn` is the root of its own private execution; its delivery to
`burn(cid, H(note))` is a private→public boundary message, so the note is public and the holder
appears as an alias.

| case | code | execution gas |
|---|---|---:|
| burn 1 ETH (configuration A), then 1 ETH (B), by the payer | applied | 45,867; 45,180 |
| the same note again (the holder still holds 9 ETH) | 255 `BURN_EXISTS` | 46,524 |
| 0.5 ETH; 1 unit of another token | 251 `NOT_REGISTERED` | 19,322; 18,958 |
| 10 ETH with 9 held; under a configuration the holder holds nothing of | 254 `INSUFFICIENT_BALANCE` | 19,349; 18,605 |
| at another holder's actor, not signed by it | 253 `NOT_AUTHORIZED` | 16,605 |
| as a nested call from LEZ's `scripted` program, in a transaction the holder signed (`is_authorized` holds) | 253 `NOT_AUTHORIZED` | 29,810 |
| sent to the burn account itself | 243 `WRONG_SENDER` | 13,652 |
| `Echo` (rule (A)'s nonce): accepted anywhere, no state | applied | 11,559 |
| `Sunset` (reserved, not built) | 240 `BAD_MESSAGE` | 10,050 |

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
| `ImportRoot` (n = 8) | public root | 1,177 | `ImportRoot`, its proofs skipped (before the fix: refused, `TrailingBytes { at: 81 }`) | yes |
| plaintext claim | public root | 1,214 | ignored (variant 4 and up) | no |
| public burn, A | public root | 185 | `Burn` | yes |
| the same note again | public root | 185 | `Burn` | no (reverted) |
| public burn, B | public root | 185 | `Burn` | yes |
| `Echo` | public root | 33 | `Echo` | yes |
| private claim: `Read`, `Spend` | private → public | 1; 186 | ignored | no |
| private claim: the root's reply | public → private | 189 | — | no (not public execution) |
| private burn | private → public | 185 | `Burn` | yes |

It collects exactly the import and the three burns that happened, in order, each burn with its
note and configuration; from the import's bytes the program's decoder and the checkpoint's read the
same four fields; its burn leaf equals the bridge core's; its `configId`, computed from the inputs it
pins (with this prototype's `attestationRootId` and `salt`, §6), equals the program's; and it
re-encodes the program's `Burn` byte for byte (185 bytes, its `BURN_LEN`).

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
   bridge keeps wrapped balances as its own actor state at the holder's account, in the token's
   base units (no LEZ unit conversion), always whole denominations. The switch to a token program
   replaces `Mint` and the holder's debit by synchronous calls of that program; every encoding the
   checkpoint reads and every refusal stay.
2. **`attestationRootId = H("ferrata-lez/v1/bridge/attestation-root" ‖ attestation program ‖
   instance id)`.** An instance id alone does not name the program whose records it trusts; LEZ
   v0.4 deploys one image at many addresses.
3. **The registry is the configuration's, fixed at `Init`** (one token, three denominations), and
   `salt = H("ferrata-lez/v1/bridge/terms" ‖ token ‖ denominations)`, so `configId` — which the
   vault pins and holds in slot 2 — commits to it, and a front-run `Init` cannot install other
   terms. The specified registry, attested from the vault's own, is not built.
4. **`root(cid, n)` is keyed by the leaf count** and a second import at the same count is refused;
   `root(n)` in the proposal does not say what `n` is.
5. **`ImportRoot` carries its proofs after the checkpoint's four fields** (§5, mismatch 1).
6. **One configuration router.** `Init` and `ImportRoot` run at `config(cid)`, which keeps the
   outstanding request in its state for the length of the transaction and clears it on the reply,
   so `config(cid)` is not strictly write-once; its configuration never changes.
7. **Records at genesis** (above): the light client's path to a record is Stages 1–4's.
8. **Exit codes share where 16 do not suffice**: `CLAIM_INVALID` covers a wrong path, leaf,
   index, destination or vault (they cannot be told apart without the deposit, and a code that
   told them apart would say more than a refusal should, B6); `NOT_FOUND` covers a missing
   configuration and a missing root.

## 7. Real proofs of the private claim and the private burn

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

## 8. What is not built

Fees, relay fees and the fee recipient (D13); caps and freezes; `Sunset` (its variant is reserved
and refused); the registry beyond one token and its attestation from the vault; the gas drop; a
transfer between holders; the switch to a token program; a real proof of a private claim to a
public destination (§3's second path; only the private-destination claim and the private burn are
proven, §7); a real vault (Sepolia) and records written by the light client in the same
run; the checkpoint program running over these blocks end to end (only its parser and decoder
ran); the off-chain importer, wallet and relayer; caps on mints and burns with a caller-supplied
time.

## 9. Reproduce

```sh
cd lez
export RISC0_HOME=$PWD/../../risc0-home-lez        # guest Rust 1.91.1, as RESULTS.md §8
cargo test -p harness --test bridge_inbound -- --nocapture          # writes results/bridge-inbound.md
RISC0_DEV_MODE=1 cargo test -p harness --test bridge_private --test bridge_burn -- --nocapture
                                                    # write results/bridge-private-claim.md, bridge-burn.md
cargo build -p harness --locked --bin prove_bridge   # §7
RISC0_DEV_MODE=1 target/debug/prove_bridge --allow-dev-mode   # dry run: the same commands, fake receipts
scripts/stage-prove-bridge.sh ../gpu-prover/inputs-lez-bridge   # then ../gpu-prover/README.md, job lez-bridge
RISC0_DEV_MODE=0 target/debug/prove_bridge --settle results/bridge-private-claim-tx.borsh \
  results/bridge-private-burn-tx.borsh               # settle the proven pair again, receipts verified
```
On this worktree they ran through `../../modal-build/mrun` (a cloud container), the first build
without `RISC0_BUILD_LOCKED` because the new guest's lockfile had to be written; with the
lockfiles committed, `--locked` and `RISC0_BUILD_LOCKED=1` apply as in `RESULTS.md` §8.

## 10. On a live sequencer: the inbound crossing (stand-in, then Sepolia)

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

