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
| configuration | §1 `configId`; B5 reciprocal pairing | `Init` writes a write-once `config(cid)` after attesting the vault's slot 2 = `cid`; the roles and cap defaults are in `cid` (§5) |
| root import | B1, D13 inbound | `ImportRoot` attests slots 0 and 1 under one record in its own transaction, writes a write-once `root(cid, n)`; refused after `Sunset` |
| registry | F.8, F.10, §B4 | `Register` attests a token's entry hash and fee schedules 0 and 1 from the vault's storage and writes a write-once `token(cid, token)`: at most eight denominations, each an exact multiple of a power-of-ten LEZ unit (§5) |
| fees | F.13, D13, §B4 | the claim mints the denomination to the user, the mint fee of the deposit's leaf-index schedule to the fee recipient, the relay fee to the relayer the claim names; `ImportFees` imports later schedules; 1% ceiling and seven-day notice re-checked (§5) |
| caps | F.11, §B4 | token buckets per token and global, for mints and for burns, in the vault's value units, on attested Ethereum time (§6); an immutable floor (§5) |
| freezes and roles | F.14, §B4 | the freeze authority freezes mints or burns, per token or globally, for at most 30 days of attested Ethereum time; the admin sets caps (never below the floor) and cannot unfreeze (§5, §6) |
| retirement | SPEC-v2 §4.5, §B5 | `Sunset` by the freeze authority, one-way: no `ImportRoot` after it; claims and burns go on; the checkpoint collects it (§5) |
| claim, plaintext | §3.3, B1 | `Claim` at `root(cid, n)`: the deposit leaf's path, the opening of `C`, `N`; then `Issue` at `token(cid, token)`, `Spend` at the nullifier, the mints |
| claim, private | B1, B6 | `ClaimPrivately` at the destination's own private account: reads `root(cid, n)`, verifies inside its own execution, sends only `Issue` (the fee schedule's range, not the leaf index) |
| nullifier | §1, B1 | write-once `nullifier(chain, vault, N)`, one namespace for the whole program |
| wrapped balances | (M0 fallback, §3 below) | the bridge program's own actor state at the holder's account, per configuration and token |
| burn | §5.1, D13 outbound, D10 | `Burn` (the checkpoint's variant 0, byte for byte) at the holder's actor, top-level or private root only; admitted by `token(cid, token)`; a write-once `burn(cid, H(note))` |

Left out: permissionless `Sunset` (30 days without a finalised checkpoint: the program has an
attested clock (§6), but the vault keeps no slot with the time of its last finalised checkpoint), the replacement of the freeze authority after a
14-day timelock and the 12-hour watchers (vault-side roles; on LEZ the roles are fixed per
configuration), the gas drop (B2), transfers between holders, off-chain software (importer,
wallet, prover daemons) and mode (A)/(B) finality (that is the checkpoint's and the vault's). The
vault side of the registry and fee schedules is FerrataVault v5's (`vault/`), which writes the slots
of §5; the harness reads them both from its synthetic vault and from the storage v5 itself wrote
(`harness/tests/bridge_vault_storage.rs`, `BRIDGE-RESULTS.md` §8a). Private execution runs in RISC Zero dev mode in the
tests; the private claim and burn of the earlier image were proven for real (`BRIDGE-RESULTS.md`
§7), not this image's.

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
| `config(cid)` | PDA `config ‖ cid` | `Config { config }`; mutable: the global gate (freezes, mint and burn buckets, the attested clock, §6), `sunset` (one-way); transiently a pending attestation or record read inside one transaction | `Init`; attestation and record replies; `Admit`; `Freeze`, `SetCap`, `Tick`, `Sunset` |
| `token(cid, token)` | PDA `token ‖ cid ‖ token` | `Token { entry, schedules, known_until, roles, attestation program and instance, gate }`; mutable: schedules (appended), `known_until` (grows), the gate; transiently a pending record read | `WriteToken` and `ExtendFees` from `config(cid)` only; `Issue`, `Burn`, `Freeze`, `SetCap`, `Tick`; record replies |
| `root(cid, n)` | PDA `root ‖ cid ‖ n` | `Root { cid, chain, vault, n, root, record_slot, block_number }` | `WriteRoot` from `config(cid)` only |
| `nullifier(chain, vault, N)` | PDA `nullifier ‖ chain:u256 ‖ vault ‖ N` (no `cid`) | `Spent { cid, token, n, amount }` | `Spend` from `token(cid, token)` only |
| holder | the holder's own account, program `B` | `Holder { holdings: [(cid, token, amount)] }` (mutable) | `Mint` from the nullifier actor; the holder's `Burn`; its own private claim |
| `burn(cid, H(note))` | PDA `burn ‖ cid ‖ H(r ‖ memo ‖ mac ‖ token ‖ amount ‖ COut)` | `Burn { cid, note }` | `Burn` from `token(cid, token)` only |

`cid` is SPEC-v2 §1's `configId`, computed on chain with the bridge core
(`config_id_preimage`), with `bridgeHeader` taken from the program's own account. The token actor
copies the configuration's chain, vault and roles at registration, so a claim or a burn reads
neither `config(cid)` nor the registry a second time.

Messages (one borsh enum, `bridge-wire/src/lib.rs`). Variants 0–3 are the checkpoint's
(`../checkpoint/crates/checkpoint-core/src/wire.rs`): the checkpoint program decodes every call
to `B` that a block shows in the clear with that decoder, so the program must put exactly those
bytes on chain.

| # | message | to | from | does |
|---|---|---|---|---|
| 0 | `Burn { cid, r, memo, mac, token, amount, COut }` | holder; then `token(cid, token)`; then `burn(cid, H(note))` | top-level (public) or the private root; then `B` | debits one denomination of an authorised holder; the token admits it (denomination, burn freezes and buckets, `Admit`); the note is recorded once |
| 1 | `ImportRoot { cid, record_slot, root, leaf_count, proof }` | `config(cid)` | anyone | `Attest` (vault account, slots 0 and 1) at `header(instance, record_slot)`; on the reply, `WriteRoot`. Refused after `Sunset` |
| 2 | `Sunset { cid }` | the freeze authority's own actor; then `config(cid)` | top-level, signed by it | one-way: no `ImportRoot` after it |
| 3 | `Echo { nonce }` | any | anyone | no-op (rule (A)'s echo nonce) |
| 4 | `Init { config, record_slot, proof }` | `config(cid)` | anyone | `Attest` (vault account, slot 2); on the reply, writes the configuration if slot 2 = `cid` |
| 5 | `Claim { cid, n, witness, range, relayer }` | `root(cid, n)` | anyone | the plaintext claim: verify, then `Issue` with `mint_to = dest` |
| 6 | `ClaimPrivately { cid, n, witness, range, relayer }` | the claimant's (private) holder actor | its owner | `Read` of `root(cid, n)`; on the reply verify, then `Issue` (and credit itself) |
| 7 | `Read` | any | an actor | replies `StateReply` (LEZ v0.4's read idiom) |
| 8 | `WriteRoot { root }` | `root(cid, n)` | `config(cid)` only | writes the root once |
| 9 | `Spend { N, cid, chain, vault, token, n, amount, mint_to, fees }` | `nullifier(chain, vault, N)` | `token(cid, token)` only | spends `N` once; mints the denomination to a public `mint_to` and each fee to its account |
| 10 | `Mint { N, cid, chain, vault, token, amount }` | holder | `nullifier(chain, vault, N)` only | credits the amount |
| 11 | `Register { cid, entry, schedule, record_slot, proof }` | `config(cid)` | anyone | bounds; `Attest` (the entry's slot, fee slots 0 and 1, slot 1); on the reply, `WriteToken` |
| 12 | `ImportFees { cid, token, index, schedule?, record_slot, proof }` | `config(cid)` | anyone | `Attest` (fee slot `index`, slot 1); on the reply, `ExtendFees` |
| 13 | `WriteToken { state }` | `token(cid, token)` | `config(cid)` only | writes the token once |
| 14 | `ExtendFees { cid, token, index, schedule?, count }` | `token(cid, token)` | `config(cid)` only | appends a schedule (in order, bounds), or extends `known_until` to `count` |
| 15 | `Issue { cid, token, N, amount, range, n, mint_to, relayer }` | `token(cid, token)` | an actor of `B` (a claimant) | denomination, the range against the schedules, the fees; mint freeze and bucket at the token's clock; then `Admit` and `Spend` |
| 16 | `Admit { cid, token, mints, value }` | `config(cid)` | `token(cid, token)` only | the global freeze and bucket of a mint or a burn, at the configuration's clock |
| 17 | `Freeze { cid, token?, mints, burns, until, record }` | the freeze authority's actor; then the target | top-level, signed by it | the target reads `record` (as `Tick`), then freezes until `until` (after that attested time and ≤ 30 days after it; 0 unfreezes) |
| 18 | `SetCap { cid, token?, mints, cap }` | the admin's actor; then the target | top-level, signed by it | a bucket's capacity and refill, never below the floor, refilled at the target's clock first |
| 19 | `Tick { cid, token?, record }` | `config(cid)` or `token(cid, token)` | anyone | `Read` of the attestation program's `header(instance, record.slot)`; on the reply, the clock moves to that record's execution timestamp if later (§6) |

Every handler checks that its receiver is the actor the message names. Replies are accepted only
from the actor the receiver is waiting for (the consumer's rule, `guests/consumer`).

### The claim statement
`verify(root state, witness, range)`: `kind ∈ {private, public}`; `leafIndex < n` (the root's
count, D13), so also `< 2^32` (D9); `leafIndex` inside the fee schedule range the claim names;
then `ferrata_bridge_core::inbound::verify_claim` (recomputes `C` and the leaf from the witness,
checks the path with direction bits = `leafIndex` to the imported root, returns `N = H(s ‖ fid)`
with `chain` and `vault` from the root). The token actor then checks that the token is registered,
the amount one of its denominations and the range its schedule's (§5). The plaintext path requires
`kind = public`; the private path with `kind = private` requires that it runs as `dest` itself
(`receiver = dest`, authorised), and credits its own state instead of minting publicly.

### What each path makes public (B6)
- `ImportRoot`, `Claim` (plaintext), public `Burn`: the whole message.
- `ClaimPrivately`: the `Read` of `root(cid, n)` and its reply (which root was read), `Issue`
  (`N`, `cid`, the token, the amount, the fee schedule's range, `n`, the relayer, the time;
  `mint_to` only for a public destination), then `Spend` and the public mints of the fees (the
  fee recipient and the relayer), the fee alias and reserve. Not the leaf index, depositor, `s`,
  `C`, the leaf, the path, or a private `dest`. The range is a residual: the mint fee is public and
  differs between schedules, so which schedule applied is public anyway; the range says no more
  (a wallet names the widest range its state allows). The test dumps the transaction and searches
  its bytes for each private input.
- private `Burn`: the boundary `Burn` to `token(cid, token)` (the note), from an alias.
- `Sunset`, `Freeze`, `SetCap`: the whole message and the role's account (they are public acts).

### D10: burns the checkpoint cannot see
The checkpoint sees calls to `B` only as the root of a public transaction, the public root of a
privacy-preserving one and its private→public boundary deliveries. So the holder's `Burn`
requires `from = None` (a top-level call, or the root of a private execution); a nested call
from any program and an asynchronous message carry a sender and are refused (`NotAuthorized`).
The note itself is admitted by `token(cid, token)` from an actor of `B` only and recorded by
`burn(cid, H(note))` from that token actor only; in the private case the delivery to the token
actor is the boundary message the checkpoint reads. The bridge never records a burn in private
state. `Sunset` follows the same rule: the freeze authority's own actor forwards it only from a
top-level call (or private root), so the checkpoint sees every `Sunset` that executes.

## 3. The token decision (M0 fallback)

LEZ v0.4.0-rc1 has no released token program (`../docs/design/lez-v0.4-findings.md` §8: the
v0.4 port is an unmerged branch, fungible-only, pinned to another LEZ revision). So the bridge
keeps wrapped balances itself: a holder's balance is the bridge program's own actor state at the
holder's account (one actor state per program per account is LEZ v0.4's account model), per
configuration and token, in token base units: a user's balance is always a sum of whole
denominations (fees are minted beside it, never out of it); the fee recipient's and relayers'
balances also hold fees. The registry's LEZ unit divides every denomination and every fee minted
on LEZ, so the conversion to a token program's units is exact. A public holder's debit needs the holder's
signature (`is_authorized`); a private holder's balance lives in its private state, updated only
by the bridge program inside a proof.

**How it would switch.** When a v0.4 token program is released and deployed immutable (B5), each
registry entry's `lezDefinition` names the wrapped token's definition account (zero here), with the
mint authority held by a
PDA of `B` (lez-programs' mint authority, LP-0013). `Mint` becomes a synchronous `Mint` call of
the token program by that PDA, and `Burn` a synchronous `Burn`/debit of the holder's token
account authorised by the holder, both in the same transaction, so a non-reverted burn still
means the tokens are gone (§5.1). The holder actors and `Holder` state disappear; nullifiers,
roots, burn notes, every encoding the checkpoint reads and every refusal stay as they are.

## 4. Departures and open points (decided here; recorded in `BRIDGE-RESULTS.md`)

1. `attestationRootId` (§1) is taken as `H("ferrata-lez/v1/bridge/attestation-root" ‖
   attestation program account ‖ instance id)`: an instance id alone does not name the program
   whose records it trusts (LEZ v0.4 deploys one image at many addresses).
2. `salt = H("ferrata-lez/v1/bridge/controls" ‖ borsh(controls))`: `configId` — which the vault
   pins and slot 2 holds — commits to the roles and cap defaults, so a front-run `Init` cannot
   install others. Tokens are not in it: each is registered from the vault's own registry (§5).
3. Wrapped amounts are kept in the token's base units (no token program); the registry's LEZ unit
   makes the conversion exact.
4. `root(cid, n)` is keyed by the leaf count: the vault's tree is append-only, so one count has
   one root; a second import at the same count is refused.
5. The checkpoint's `ImportRoot` (variant 1) has no room for the Merkle-Patricia proofs the
   program needs inside the same transaction; the program's variant 1 is the checkpoint's fields
   followed by the proofs, which the checkpoint skips (`BRIDGE-RESULTS.md` §5).
6. Exit codes 240–255 (the unassigned block of `wire::CODE_BLOCKS`), 16 codes, so refusals share
   (listed in `bridge-wire/src/lib.rs`, `code`): 242 became `FROZEN` (the old `WRONG_RECEIVER`
   is now `WRONG_SENDER`), 255 became `CAPPED` (a repeated burn note is now `ALREADY_WRITTEN`).
   `lez/wire/src/lib.rs` is not edited: its line numbers are in the attestation guest's ELF.
7. The vault's registry and fee slots are defined here (§5), with SHA-256 slot keys rather than
   Solidity mappings; FerrataVault v5 writes them in this encoding (`BRIDGE-RESULTS.md` §8a).
8. No message carries a time. Freezes and buckets use each actor's attested clock, which only
   `Tick` and `Freeze` move, by reading a record of the configuration's attestation instance (§6).
   Conservative: a bucket never refills beyond attested Ethereum time, and a lapsed freeze still
   holds until something reads a record past it. (Until the fix of §6 a caller's time under a
   validity window was used; one block producer could move it, review r3's attack A.)
9. Roles are LEZ accounts fixed in the configuration; the 14-day replacement of the freeze
   authority, the watchers and permissionless `Sunset` are not built (§1).

## 5. Registry, fees, caps, freezes and `Sunset`

**Vault storage the LEZ side attests** (defined here; FerrataVault v5 and the harness's synthetic vault write it):
- the entry of `token`: slot `sha256("ferrata/vault/registry/v1" ‖ token)` holds
  `sha256("ferrata/registry/v1" ‖ token ‖ lezDefinition ‖ decimals:u8 ‖ lezUnit:u256 ‖
  unitWeight:u256 ‖ k:u8 ‖ denominations[k]:u256 ‖ relayFees[k]:u256)`, written once at registration;
- fee schedule `j` of `token`: slot `sha256("ferrata/vault/fees/v1" ‖ token ‖ j:u64)` holds
  `sha256("ferrata/fees/v1" ‖ token ‖ j:u64 ‖ fromLeaf:u64 ‖ announcedAt:u64 ‖ effectiveAt:u64 ‖
  k:u8 ‖ mintFees[k] ‖ burnFees[k] ‖ checkpointFees[k])`, written when it takes effect (`fromLeaf`
  = the vault's `nextLeafIndex` then); schedule 0 at registration.

**Registry.** `Register` checks the bounds before it asks anything: 1 to 8 denominations, strictly
increasing, each an exact multiple of a power-of-ten `lezUnit` (at most `10^decimals`), relay fees
at most 1% and multiples of `lezUnit`, a non-zero `unitWeight`, schedule 0 without protocol fees
("protocol fees start at zero"). Then one attestation of four slots under one record: the entry,
schedules 0 and 1, the leaf count. Delisting has no LEZ effect (the spec stops deposits on the
vault; claims and releases go on).

**Fees by leaf index.** A token keeps its schedules and `known_until`: every leaf below it falls
under a known schedule. Registration sets it to the attested count if schedule 1 does not exist
yet. `ImportFees(j, schedule)` appends schedule `j` (in order, starting at or after `known_until`
and the previous one, bounds: each fee ≤ 1% of its denomination, mint fees multiples of `lezUnit`,
`effectiveAt ≥ announcedAt + 7 days`) and sets `known_until` to its `fromLeaf`;
`ImportFees(j, None)` attests that schedule `j` does not exist at a count, and raises
`known_until` to it. A claim names a range `[from, until)` of schedule `j`; the claimant checks its
leaf index against it (privately in a private claim), the token checks that `from` is schedule
`j`'s first leaf and `until` at most the next schedule's or `known_until`. The claim mints exactly
the denomination to the user, schedule `j`'s mint fee to the fee recipient and the registered
relay fee to the relayer it names (none named: the fee recipient). Outbound fees (burn, relay,
checkpoint) are deducted from the release on Ethereum (D13); the LEZ burn debits one denomination.

**Caps.** A gate per token and one per configuration: a mint and a burn token bucket each, in the
vault's value units (`amount / lezUnit × unitWeight`), counting what a claim mints (denomination
and fees) and what a burn destroys, at the actor's attested clock (§6). The admin's `SetCap`
refills under the old parameters at that clock, then sets the new ones, never below the
configuration's immutable floor; raising a capacity does not refill. A refused claim reverts the
whole transaction, so the deposit stays claimable.

**Freezes and roles.** Each role is a LEZ account in the configuration and acts by a top-level call
it signs at its own actor of `B`, which forwards the message to `config(cid)` (global) or
`token(cid, token)`; the target accepts it only from the role's actor. The freeze authority sets
`mints_until` and/or `burns_until`: the target first reads the record the `Freeze` names, and
`until` must lie after that attested time and at most 30 days after it (0 unfreezes); the admin
only sets caps, so it cannot unfreeze. `Sunset` is the freeze authority's, one-way; `ImportRoot`
refuses afterwards (`FROZEN`), everything else goes on.

## 6. Time: attested Ethereum time (B2)

**The flaw this replaces** (review r3, `notes/reviews/draft6/r3-engineer.md` §2 A). LEZ
v0.4.0-rc1 neither bounds nor orders block timestamps, and the program took its time from the
caller, kept monotone and advanced by a `Tick` anyone could send, each under a validity window
around the block's timestamp. So the producer whose turn it was could stamp a block a year ahead
and carry its own `Tick`: every freeze of the configuration ended, every bucket refilled, and the
freeze authority could not freeze again (a new `until` had to lie within 30 days of an honest time,
which was already in the program's past). One producer is fewer than `t`.

**Where time comes from now.** Each gate (`config(cid)`, `token(cid, token)`) keeps a clock: the
newest execution timestamp, in milliseconds, it has read from a record of the configuration's own
attestation program and instance (`header(instance, slot)`, written by the light client after it
verified a finality update; the record of a payload-layout header holds the execution block's
timestamp, and one in the Gloas layout binds the block's hash, so the reader supplies the RLP header,
which must hash to it). It only moves forward (`max`), so it is monotone by construction, and an old
record moves nothing. No message carries a caller's time: `Claim`, `ClaimPrivately`, `Issue`,
`Admit` and `SetCap` lost their time fields; claims and burns use the clocks as they stand.

**`Tick` reads the record.** `Tick { cid, token?, record }`, still open to anyone: the gate sends
`Read` to the record's actor, waits for the reply within the transaction (accepted only from that
actor), checks the record's instance and chain, and advances its clock to the record's timestamp. A
`Freeze` does the same before it applies, so a freeze's 30-day bound is measured from attested time,
whatever the gate's clock was, and in one transaction (a separate `Tick` first could be ordered
after the `Freeze` by the producer). A token's clock starts at its configuration's at registration;
a gate that has read no record yet starts its buckets counting at the first time it reads.

**Not ahead of the block.** The reply also sets the transaction's validity window to blocks stamped
at or after the record's time. An honest record is always behind the block (finality takes about
two epochs, 13 minutes, before a record can be written), so honest blocks always pass. Without it, a
captured sync committee, the case LEZ's caps exist for, could have a far-future record written and
lift freezes and refill caps on its own; with it, that needs a captured committee **and** a producer
stamping its block that far ahead. A producer alone can only delay a `Tick` (other producers include
it), which holds freezes longer and refills later: it fails safe.

**When Ethereum finality stalls** (no newer record): every clock stops. Buckets stop refilling, so
claims and burns stop once their buckets are spent, and LEZ-side burns wait for the light client
too. No freeze can lapse, however long LEZ runs. The freeze authority can still freeze (a new
`until` lies after the stopped clock, so it holds at once and until records pass it), unfreeze
(`until` 0) and `Sunset`; the admin can still set caps, but no bucket refills. The vault side is
unchanged: its caps and freezes run on Ethereum's own clock.

**What stays.** A captured sync committee together with one producer that stamps a far-future
block can still move the clock; the vault's caps, on Ethereum time, still bound the escrow. Tested in
`harness/tests/bridge_time.rs` (results in `results/bridge-time.md`, `BRIDGE-RESULTS.md` §11):
a block stamped a year ahead with the newest record lifts no freeze and refills nothing, the freeze
authority can still freeze in it; newer records refill exactly and end freezes; an older record moves
nothing; a record an hour ahead of the block is refused by LEZ unless the block is stamped that far
ahead too; with no newer record a year of LEZ blocks changes nothing; Gloas records with and without
the right header; a slot without a record. The real crossing on Sepolia and every run recorded
before this change used the earlier program (its caller time); `BRIDGE-RESULTS.md` §11 has the image
ids.
