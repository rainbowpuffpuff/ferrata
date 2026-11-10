# The Ferrata bridge program on LEZ v0.4.0-rc1, measured through LEZ's state machine

The bridge program of RFP-022 (`guests/bridge`, one RISC Zero guest; image
`c0c80a6434cd0bcdaf4b32b71d4f71bfb28e5e23564fa23dfa4686792b48f1ed`), with its wire crate
(`bridge-wire/`) and the harness that drives it (`harness/src/bridge.rs`,
`harness/tests/bridge_{inbound,private,burn}.rs`). The plan, the actors and the messages are in
[BRIDGE.md](BRIDGE.md). Every number below is LEZ's own accounting of a transaction that ran:
`lee`'s state transition functions and the copy of LEZ's fee settlement that Stages 1–4 use
([RESULTS.md](RESULTS.md) §1), each transaction in a block of its own, the attestation program
`2f3566d4…` (Stage 4) answering the bridge's `Attest` calls. They ran on this worktree's Modal
build box (`spike/modal-build/mrun`), not on the laptop; the executor is deterministic, so the
cycle counts do not depend on the machine. Private executions ran in RISC Zero dev mode: executed,
not proven. The tables are `results/bridge-inbound.md`, `results/bridge-private-claim.md` and
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
| `Init`: attest the vault's slot 2 = `configId`, write `config(cid)` | Init 39,167 → Attest 82,188 → reply 33,174 | 154,529 | 1,726 |
| `ImportRoot`: attest slots 0 and 1 under one record, write `root(cid, n)` | ImportRoot 39,151 → Attest 101,934 → reply 44,676 → WriteRoot 17,859 | 203,620 | 1,703 |
| plaintext claim: path, opening of `C`, `N`, mint to a public `dest` | Claim 109,283 → Spend 17,756 → Mint 15,405 | 142,444 | 1,740 |
| private claim, private destination (one privacy-preserving transaction) | private: ClaimPrivately 39,314, reply 113,417 (user cycles); public: Read, Spend | 439,815 | 224,063 |
| private claim, public destination | private: the same two; public: Read, Spend, Mint | 457,925 | 224,063 |
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
bridge's own handlers (39,151 + 44,676 + 17,859) do not depend on it.

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
| `Init` of another configuration of vault A (other denominations): slot 2 holds `configId(A)` | 246 `PAIRING_MISMATCH` | 149,644 |
| `Init` of an existing configuration | 244 `ALREADY_WRITTEN` | 30,282 |
| import at a count already imported (refused by `WriteRoot`, after the attestation) | 244 `ALREADY_WRITTEN` | 204,728 |
| import whose message claims count 7, or the 5-leaf root, at a block holding 8 | 247 `IMPORT_MISMATCH` | 173,340; 172,858 |
| import for configuration A with vault B's proofs (the request names vault A) | 110 `ProofNodeMismatch` (the core's, from `Attest`) | 80,042 |
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
destroyed without a burn leaf. NOut (D13) is wallet and release side, `H("ferrata/nf-out/v2" ‖
sOut)`, here `bridge-wire::nf_out_v2`: the test shows that v1 (`H(… ‖ sOut ‖ burnId)`) changes with
the index a burn is collected at and v2 does not. The bridge program never sees `sOut` or `kView`.

## 5. What the checkpoint collects, and the mismatches

The test puts the public transactions (as settled) and the two privacy-preserving ones into one
LEZ block, the repeated burn marked reverted, and reads it with `checkpoint-core`'s own
`parse_block` and `BridgeMessage::decode` (branch `e2e-outbound` has the same `checkpoint/crates`
as this branch: `git diff --stat` shows no change there):

| transaction | origin | bytes | the checkpoint's decoder | collected |
|---|---|---:|---|---|
| `ImportRoot` (n = 8) | public root | 1,177 | refused: `TrailingBytes { at: 81 }` | **no** |
| plaintext claim | public root | 1,214 | ignored (variant 4 and up) | no |
| public burn, A | public root | 185 | `Burn` | yes |
| the same note again | public root | 185 | `Burn` | no (reverted) |
| public burn, B | public root | 185 | `Burn` | yes |
| `Echo` | public root | 33 | `Echo` | yes |
| private claim: `Read`, `Spend` | private → public | 1; 186 | ignored | no |
| private claim: the root's reply | public → private | 189 | — | no (not public execution) |
| private burn | private → public | 185 | `Burn` | yes |

It collects exactly the three burns that happened, in order, each with its note and
configuration; its burn leaf equals the bridge core's; its `configId`, computed from the inputs it
pins (with this prototype's `attestationRootId` and `salt`, §6), equals the program's; and it
re-encodes the program's `Burn` byte for byte (185 bytes, its `BURN_LEN`).

**Mismatches, and which side should change.**
1. **`ImportRoot` is not collected** (severe if left: `importedLeafCount` would never advance, and
   after a sunset the vault would refund deposits that are still claimable on LEZ). The
   checkpoint's variant 1 is exactly its four fields; the program needs the account and two
   storage proofs in the same transaction, and a public transaction's only visible data is its
   root message, so they follow the four fields (`ImportProof`: three borsh `Vec<Vec<u8>>`). The
   checkpoint should change: decode variant 1 as the four fields followed by `ImportProof`, walk
   its borsh lengths strictly and ignore it. Nothing else in the program's encoding needs it.
2. **Replies are never visible calls, and the checkpoint should rely on that only knowingly.**
   The checkpoint decodes every visible message to a bridge actor, while the program first treats
   a message from the actor it waits for as a reply (an `Attestation`, a `StateReply`), not as a
   bridge message. Today no reply to a bridge actor is a visible call (the attestation reply is
   nested in public execution; the root's reply to a private claimant is public→private), so the
   two decoders never disagree. A future handler that waits for a reply from a private actor
   would break that; the program side should keep it so, and the checkpoint's README should state
   it.
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

## 7. What is not built

Fees, relay fees and the fee recipient (D13); caps and freezes; `Sunset` (its variant is reserved
and refused); the registry beyond one token and its attestation from the vault; the gas drop; a
transfer between holders; the switch to a token program; real proofs of the private claim and
burn (dev mode only); a real vault (Sepolia) and records written by the light client in the same
run; the checkpoint program running over these blocks end to end (only its parser and decoder
ran); the off-chain importer, wallet and relayer; caps on mints and burns with a caller-supplied
time.

## 8. Reproduce

```sh
cd lez
export RISC0_HOME=$PWD/../../risc0-home-lez        # guest Rust 1.91.1, as RESULTS.md §7
cargo test -p harness --test bridge_inbound -- --nocapture          # writes results/bridge-inbound.md
RISC0_DEV_MODE=1 cargo test -p harness --test bridge_private --test bridge_burn -- --nocapture
                                                    # write results/bridge-private-claim.md, bridge-burn.md
```
On this worktree they ran through `../../modal-build/mrun` (a cloud container), the first build
without `RISC0_BUILD_LOCKED` because the new guest's lockfile had to be written; with the
lockfiles committed, `--locked` and `RISC0_BUILD_LOCKED=1` apply as in `RESULTS.md` §7.
