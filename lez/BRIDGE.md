# The Ferrata bridge program on LEZ v0.4.0-rc1: plan

What this file maps: the inbound and outbound legs of the bridge specification
(`../docs/design/spec/bridge-spec-v3-changes.md` D1–D13, which wins over
`bridge-spec-v2.md`, which wins over `../docs/design/bridge.md`; proposal §B1–§B6) onto LEZ v0.4
actors, accounts and messages, and what the prototype in this directory builds of it. Results are
in `BRIDGE-RESULTS.md`.

## 1. What the prototype builds and leaves out

Built, as one RISC Zero guest (`guests/bridge`) driven through LEZ's own state transition and fee
settlement by the harness (`harness/src/bridge.rs`, `harness/tests/bridge_*.rs`):

| piece | spec | here |
|---|---|---|
| configuration | §1 `configId`; B5 reciprocal pairing | `Init` writes a write-once `config(cid)` after attesting the vault's slot 2 = `cid` |
| root import | B1, D13 inbound | `ImportRoot` attests slots 0 and 1 under one record in its own transaction, writes a write-once `root(cid, n)` |
| claim, plaintext | §3.3, B1 | `Claim` at `root(cid, n)`: the deposit leaf's path, the opening of `C`, `N`, one denomination minted to a public `dest` |
| claim, private | B1, B6 | `ClaimPrivately` at the destination's own private account: reads `root(cid, n)`, verifies inside its own execution, sends only `Spend` |
| nullifier | §1, B1 | write-once `nullifier(chain, vault, N)`, one namespace for the whole program |
| wrapped balances | (M0 fallback, §3 below) | the bridge program's own actor state at the holder's account |
| burn | §5.1, D13 outbound, D10 | `Burn` (the checkpoint's variant 0, byte for byte) at the holder's actor, top-level or private root only; a write-once `burn(cid, H(note))` |

Left out (out of scope of this piece of work): fees and relay fees (D13 "Fees"), caps, freezes,
`Sunset` (its message index is reserved and refused), the registry beyond one token (no
registry attestation: see §4), the gas drop (B2), transfers between holders, off-chain software
(importer, wallet, prover daemons), mode (A)/(B) finality (that is the checkpoint's and the
vault's). Real proofs: none; private execution runs in RISC Zero dev mode (executed, not
proven), as Stage 3 and 4 did before their GPU proofs.

The Ethereum side is synthetic: the vault's state (slots 0, 1, 2) is built with the reference
trie of `../crates/ferrata-spectests` in a synthetic execution block, and the record of that
block is placed in the attestation program's `header(id, slot)` actor at genesis, standing in for
the record the light client writes after verifying a finality update (Stages 1–4 measured that
part on mainnet data). The first five leaves of vault A are the shared vector's claim tree
(`../bridge/vectors/inbound-v2.json`, `claim`): `root(5)` is the vector's root, and the plaintext
claim of its leaf 2 must produce the vector's `N`. The real Sepolia vault comes later.

## 2. Actors, accounts and messages

All actors are of the bridge program `B` (deployed at genesis like the attestation program; its
header account is `bridgeHeader`). PDAs are public PDAs of `B`; seeds are
`H("ferrata-lez/v1/bridge/<role>" ‖ …)` with the wire crate's tagged hash.

| actor | address | state (write-once unless said) | written by |
|---|---|---|---|
| `config(cid)` | PDA `config ‖ cid` | `Config { config }`; transiently `Pairing`/`pending` inside one transaction | `Init`; its attestation reply |
| `root(cid, n)` | PDA `root ‖ cid ‖ n` | `Root { cid, terms, n, root, record_slot, block_number }` | `WriteRoot` from `config(cid)` only |
| `nullifier(chain, vault, N)` | PDA `nullifier ‖ chain:u256 ‖ vault ‖ N` (no `cid`) | `Spent { cid, n, amount }` | `Spend` from an actor of `B` only |
| holder | the holder's own account, program `B` | `Holder { holdings: [(cid, terms, amount)] }` (mutable) | `Mint` from the nullifier actor; the holder's `Burn`; its own private claim |
| `burn(cid, H(note))` | PDA `burn ‖ cid ‖ H(r ‖ memo ‖ mac ‖ token ‖ amount ‖ COut)` | `Burn { cid, note }` | `Burn` from an actor of `B` only |

`cid` is SPEC-v2 §1's `configId`, computed on chain with the bridge core
(`config_id_preimage`), with `bridgeHeader` taken from the program's own account. `terms` =
`(evmChainId, vault, token, denominations)` is copied from the configuration into every root
and holding, so a claim or a burn needs no second read.

Messages (one borsh enum, `wire/src/bridge.rs`). Variants 0–3 are the checkpoint's
(`../checkpoint/crates/checkpoint-core/src/wire.rs`): the checkpoint program decodes every call
to `B` that a block shows in the clear with that decoder, so the program must put exactly those
bytes on chain.

| # | message | to | from | does |
|---|---|---|---|---|
| 0 | `Burn { cid, r, memo, mac, token, amount, COut }` | holder; then `burn(cid, H(note))` | top-level (public) or the private root; then `B` | debits one denomination of an authorised holder, records the note once |
| 1 | `ImportRoot { cid, record_slot, root, leaf_count, proof }` | `config(cid)` | anyone | `Attest` (vault account, slots 0 and 1) at `header(instance, record_slot)`; on the reply, `WriteRoot` |
| 2 | `Sunset { cid }` | — | — | reserved; refused (not built) |
| 3 | `Echo { nonce }` | any | anyone | no-op (rule (A)'s echo nonce) |
| 4 | `Init { config, record_slot, proof }` | `config(cid)` | anyone | `Attest` (vault account, slot 2); on the reply, writes the configuration if slot 2 = `cid` |
| 5 | `Claim { cid, n, witness }` | `root(cid, n)` | anyone | the plaintext claim: verify, then `Spend` with `mint_to = dest` |
| 6 | `ClaimPrivately { cid, n, witness }` | the claimant's (private) holder actor | its owner | `Read` of `root(cid, n)`; on the reply verify, then `Spend` (and credit itself) |
| 7 | `Read` | any | an actor | replies `StateReply` (LEZ v0.4's read idiom) |
| 8 | `WriteRoot { root }` | `root(cid, n)` | `config(cid)` only | writes the root once |
| 9 | `Spend { N, cid, terms, n, amount, mint_to }` | `nullifier(chain, vault, N)` | an actor of `B` only | spends `N` once; mints to a public `mint_to` |
| 10 | `Mint { N, cid, terms, amount }` | holder `mint_to` | `nullifier(chain, vault, N)` only | credits one denomination |

Every handler checks that its receiver is the actor the message names. Replies are accepted only
from the actor the receiver is waiting for (the consumer's rule, `guests/consumer`).

### The claim statement
`verify(root state, witness)`: `kind ∈ {private, public}`; `leafIndex < n` (the root's count,
D13), so also `< 2^32` (D9); `token` is the configuration's and `amount` one of its
denominations; then `ferrata_bridge_core::inbound::verify_claim` (recomputes `C` and the leaf
from the witness, checks the path with direction bits = `leafIndex` to the imported root,
returns `N = H(s ‖ fid)` with `chain` and `vault` from the configuration). The plaintext path
requires `kind = public`; the private path with `kind = private` requires that it runs as `dest`
itself (`receiver = dest`, authorised), and credits its own state instead of minting publicly.

### What each path makes public (B6)
- `ImportRoot`, `Claim` (plaintext), public `Burn`: the whole message.
- `ClaimPrivately`: the `Read` of `root(cid, n)` and its reply (which root was read), `Spend`
  (`N`, `cid`, the terms, `n`, the amount; `mint_to` only for a public destination), the fee
  alias and reserve. Not the leaf index, depositor, `s`, `C`, the leaf, the path, or a private
  `dest`. The test dumps the transaction and searches its bytes for each of these.
- private `Burn`: the boundary `Burn` to `burn(cid, H(note))` (the note), from an alias.

### D10: burns the checkpoint cannot see
The checkpoint sees calls to `B` only as the root of a public transaction, the public root of a
privacy-preserving one and its private→public boundary deliveries. So the holder's `Burn`
requires `from = None` (a top-level call, or the root of a private execution); a nested call
from any program and an asynchronous message carry a sender and are refused (`NotAuthorized`).
The note itself is recorded only by `burn(cid, H(note))`, a public PDA, which accepts it only
from an actor of `B`: in the private case that delivery is the boundary message the checkpoint
reads. The bridge never records a burn in private state.

## 3. The token decision (M0 fallback)

LEZ v0.4.0-rc1 has no released token program (`../docs/design/lez-v0.4-findings.md` §8: the
v0.4 port is an unmerged branch, fungible-only, pinned to another LEZ revision). So the bridge
keeps wrapped balances itself: a holder's balance is the bridge program's own actor state at the
holder's account (one actor state per program per account is LEZ v0.4's account model), in token
base units, always a sum of whole denominations. A public holder's debit needs the holder's
signature (`is_authorized`); a private holder's balance lives in its private state, updated only
by the bridge program inside a proof.

**How it would switch.** When a v0.4 token program is released and deployed immutable (B5), the
configuration gains the wrapped token's definition account, with the mint authority held by a
PDA of `B` (lez-programs' mint authority, LP-0013). `Mint` becomes a synchronous `Mint` call of
the token program by that PDA, and `Burn` a synchronous `Burn`/debit of the holder's token
account authorised by the holder, both in the same transaction, so a non-reverted burn still
means the tokens are gone (§5.1). The holder actors and `Holder` state disappear; nullifiers,
roots, burn notes, every encoding the checkpoint reads and every refusal stay as they are.

## 4. Departures and open points (decided here; recorded in `BRIDGE-RESULTS.md`)

1. `attestationRootId` (§1) is taken as `H("ferrata-lez/v1/bridge/attestation-root" ‖
   attestation program account ‖ instance id)`: an instance id alone does not name the program
   whose records it trusts (LEZ v0.4 deploys one image at many addresses).
2. The registry (one token, three denominations) is fixed in the configuration, and `salt =
   H("ferrata-lez/v1/bridge/terms" ‖ token ‖ denominations)`, so `configId` — which the vault
   pins and slot 2 holds — commits to it and a front-run `Init` cannot install other terms. The
   specified registry, attested from the vault, is not built.
3. Wrapped amounts are kept in the token's base units (no LEZ unit conversion: no token program).
4. `root(cid, n)` is keyed by the leaf count: the vault's tree is append-only, so one count has
   one root; a second import at the same count is refused.
5. The checkpoint's `ImportRoot` (variant 1) has no room for the Merkle-Patricia proofs the
   program needs inside the same transaction; the program's variant 1 is the checkpoint's fields
   followed by the proofs. Which side should change is recorded with the burn check (piece 5).
6. Exit codes 240–255 (the unassigned block of `wire::CODE_BLOCKS`), 16 codes, so some refusals
   share one (listed in `wire/src/bridge.rs`).
