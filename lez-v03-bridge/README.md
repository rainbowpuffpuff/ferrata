# lez-v03-bridge: the bridge's inbound leg and burn as a LEZ v0.3.1 program

**Experimental, unaudited: a port for the public LEZ testnet, which runs v0.3.1** (`https://testnet.lez.logos.co`).
The deliverable remains the LEZ v0.4 programs (`../lez/`, image ids in `../docs/image-ids.md`); nothing under
`../lez/`, `../crates/`, `../checkpoint/`, `../bridge/`, `../vault/` or `../bench/` changed for this port, so no
existing image id moved. **What ran where**: the program through LEZ v0.3.1's own state machine (synthetic vault
storage, and the real Sepolia storage proofs of the crown run's live vault); the whole round to the burns on a local
LEZ v0.3.1 standalone sequencer and a local anvil, with the private claim and the private burn proven for real by
LEZ's wallet ([`results/local.md`](results/local.md)). **What it does not show**: any transaction on the public
testnet or on Sepolia (prepared, not sent: `testnet.sh`, `sepolia.sh`, the owner's commands in the agent's report);
a release on Ethereum (the return leg is a design note, [`RETURN.md`](RETURN.md)); the bridge reading records the v0.3.1 light client wrote *on the same chain*
(its port, branch `testnet-v03`, is merged here in `lez-v03/`; one of its records is read in `results/sepolia.md`; the
local round's records are written by a stand-in program, see below).

## What the program is

One RISC Zero guest (`guests/bridge`, image id in `results/synthetic.md`) with the bridge core
(`ferrata-bridge-core`) for every hash and statement, and the light client's core (`ferrata-core`) for the
Merkle-Patricia proofs. `core/` holds its state, instructions, effects and checks (shared with the host).

| instruction | does | v0.4 counterpart (`../lez/BRIDGE.md` §2) |
|---|---|---|
| `Init { config, record, proof }` | the vault's slot 2 holds `configId` under a light client record: `config(cid)` written once | `Init` (4) |
| `ImportRoot { config, record, root, n, proof }` | top-level only (D10); the vault's slots 0 and 1 hold the root and `n` under a record; `config(cid)` exists; `root(cid, n)` written once | `ImportRoot` (1) |
| `Claim { config, root, witness }` | the claim statement under an imported root (guarded at `root(cid, n)`), public kind; `nullifier(cid, chain, vault, N)` spent once; the denomination credited at `holder(cid, dest)` | `Claim` (5) → `Issue` → `Spend` → `Mint` |
| `ClaimPrivately { config, root, witness }` | the same for a private-kind deposit, run by its destination, a private account, inside a privacy-preserving transaction: its own shard credited; the root guard and the spend are the transaction's public part | `ClaimPrivately` (6) |
| `Burn { config, note, from }` | top-level only, signed by the holder; one denomination debited from the holder's own shard (a private account) or its `holder(cid, account)`; the note written once at `burn(cid, H(note))`; its leaf (`burn_leaf(cid, index, token, amount, COut, r)`) appended to the configuration's burn tree at `burns(cid)` | `Burn` (0) |

**The v0.3.1 model, and why the program looks different.** A v0.3.1 program has a *plan* that sees the
instruction and its input accounts' metadata, never their data, and an *apply* that sees one effect and one
shard (`../../upstream/lez-v0.3.1/lee/state_machine/core/src/program/mod.rs`; LEZ v0.4 has actors and messages).
So each account a v0.4 handler read is, here, state the instruction carries, checked by the plan, plus a guard
effect whose apply refuses unless the account holds exactly that state: `config(cid)` for an import, `root(cid, n)`
for a claim, the light client's `header(instance, slot)` for `Init` and `ImportRoot`. In a privacy-preserving
transaction the plan and the private account's effects run inside the user's proof; the effects on public accounts
are deferred and applied by the sequencer at settlement, where a failing guard rejects the transaction. The same
checks run in the same order as on v0.4, except that a guard's refusal (for example `NOT_FOUND`, no configuration)
comes after the plan's checks, and a write-once refusal after the attestation's.

**The light client.** The bridge verifies the vault's storage proofs itself, with the light client's core, under the
record the instruction carries, and guards the light client's `header(instance, slot)` account to hold exactly that
record (the layout of `lez-v03/wire`, branch `testnet-v03`: seed `H("ferrata-lez-v03/v1/header" ‖ instance ‖ slot)`,
`RecordState { instance_id, chain_id, record }`). This is the brief's fallback to a chained `Attest`, and it is
equivalent: a chained call cannot return data on v0.3.1 (an apply cannot emit calls, a plan sees no state), so a
chained `Attest` could only assert facts the caller states, checked by the light client's apply against the same
record with the same proof functions of `ferrata-core` (`attest_storage` per slot there, `attest_batch` here); the
bridge's own apply checks that the record is the light client's, and its plan does the proof check, so the trust is
the same: the record the light client wrote after verifying a finality update (or, at its `Init`, its checkpoint's). The light client's own `Attest` (`lez-v03/wire`: a `Check` of a storage query at
`header(instance, slot)`, then one write-once `statement(instance, s)` account per statement) could be chained the
same way; it is not, because it would add a written account per attested fact to every import and change nothing in
what is checked. `guests/records` is a **STAND-IN** that writes any record it is given at the light client's layout,
for the local round only (the light client needs a beacon chain, which a local anvil has not).

**Balances.** The testnet ships no token program: wrapped balances stay in the bridge's own accounts, as the v0.4
program does today (`../lez/BRIDGE.md` §3): `holder(cid, account)` for public ones (a PDA whose seed names the
configuration and the account, review C-1), the private account's own shard of the bridge for private ones.

## Where the port departs from the v0.4 program

Two changes after review-lez-v03-bridge (18 Nov 2026), both in this port only:

1. **Nullifiers are keyed by the configuration**: `nullifier(cid, chain, vault, N)`. A configuration's light client
   and instance are free fields, so anyone can make a second configuration of the same vault and chain whose
   records it writes itself (any copy of `guests/records` will do). With v0.4's key `(chain, vault, N)`, such a
   configuration could spend the nullifier of any deposit whose `s` it knows, and the configuration the vault pins
   was then refused (250): the deposit frozen. Now the paired configuration's claim applies whatever another
   configuration did; the rogue one only mints into its own holdings and burns into `burns(cid_rogue)`, which no
   vault pins, so it gains nothing a vault would pay. `harness/tests/review_regressions.rs` R1 shows both.
2. **Holdings are one account per configuration and destination**: `holder(cid, account)` holds only that
   configuration's registered tokens (at most two), so no other configuration can fill a destination's account; and a
   state over LEZ's shard limit is refused with 255 `CAPPED` instead of panicking and being charged the whole gas
   limit (`apply`'s size check; the guest's write has no `expect` left). R2 shows both.

**The v0.4 program still has both properties** (`../lez/guests/bridge`, the image id the live Sepolia vault pins:
`nullifier(chain, vault, N)` with no configuration id, and one `holder(account)` for every configuration): a known
issue recorded by this review on 18 Nov 2026, to be fixed in the proposal's milestone B1/B2 with the same two changes.
`../lez/` is not changed here.

## Burn accumulator account

What a release leg reads over RPC (`getAccount`), the layout the stand-in attestor of branch `testnet-return`
(`lez-v03-return/INTERFACE.md`) takes as its layout file; `bridge03 <run> layout` prints it with a run's ids, and
`bridge03 <run> burns` the run's `burns.json`.

- **Account**: `burns(cid)` = `AccountId::for_public_pda(bridge, H("ferrata-lez/v1/bridge/burns" ‖ cid))`
  (`core::burns_account`); **program shard**: the bridge program's id (where it is deployed).
- **Shard bytes**: borsh `BridgeState::Burns(BurnTree { config_id, count, frontier })`, 1,069 bytes, fixed:

| field | offset | length | content |
|---|---:|---:|---|
| tag | 0 | 1 | `5` (`BridgeState::Burns`) |
| `config_id` | 1 | 32 | the configuration's `configId` |
| `count` | 33 | 8 | burns appended, u64 little-endian |
| frontier length | 41 | 4 | `32`, u32 little-endian |
| `frontier` | 45 | 1,024 | the canonical frontier, `f[0]` first (`ferrata-bridge-core`'s `IncrementalTree::frontier`) |

  No root is stored: `IncrementalTree::from_frontier(count, frontier).root()` gives it. Leaf `k` is the bridge core's
  `burn_leaf(configId, k, token, amount, COut, r)`; `k` is assigned when the burn is applied (for a private burn, at
  settlement), in the order the sequencer applies them.
- **Each note**: `burn(cid, H(note))` holds `BridgeState::Burn(BurnRecord { config_id, note })` (tag 4, then
  `config_id`, `r`, `memo` (20), `mac` (16), `token` (20), `amount` (32, big-endian), `COut`).
- **The imported count** is not in this account: each import writes `root(cid, n)` (tag 1, then `config_id`, the
  chain id (u64), the vault (20), `n` (u64 at offset 61), the root, the record's slot, the block number).

## Left out (each a v0.4 feature)

Fees (mint, relay, burn and checkpoint fees; the fee schedules and `ImportFees`), caps and buckets, freezes, pauses,
roles and their handover (`Freeze`, `SetCap`, `Pause`, `ProposeRoles`, `Cosigned`, `Sunset`, `AttestRetired`,
`Tick` and the attested clock), the registry beyond ETH in one denomination and one ERC-20 in one denomination
(and `Register`: here the registry is part of the configuration, which `configId` commits to; v0.4 attests it
from the vault's registry slots), transfers between holders, the gas drop, a private claim of a public-kind deposit
(the public kind goes through `Claim`), the checkpoint's view (variants 0–3 of v0.4's encoding: v0.3.1 instructions
and effects are this program's own), `Echo`. `configId`'s salt is `H(…/registry ‖ registry)` instead of v0.4's
controls.

## Results

- [`results/synthetic.md`](results/synthetic.md): 31 transactions through `lee::V03State` (dev-mode proofs for the
  private ones): `Init` (Gloas record with and without the header, a wrong pairing, twice), imports (another root, a
  record not keyed by its slot, twice, before `Init`), claims (replay, another destination, an unknown root, a made-up
  root, a leaf index at the count, a private-kind deposit by the plaintext path, an unregistered amount; ETH and the
  ERC-20), the private claim (by another private account: no proof; its replay: refused at settlement) with its
  transaction searched for its private inputs, burns (the same note twice, not a denomination, nothing held, not
  signed; the private holder's), the burn tree against the bridge core's. Gas per transaction.
- [`results/sepolia.md`](results/sepolia.md): the crown run's live vault `0x04eE8FF1…e44c` under Sepolia's own
  finalized blocks 12,119,428 and 12,119,519 (records placed) and 12,160,907 (**a record the v0.3.1 light client wrote
  itself**, from `lez-v03`'s local run): the program's check of the real storage proofs, `Init` refused (246: the vault
  pins the v0.4 configuration, the pairing working), each import's plan verifying the real proofs and then refused for
  want of a configuration (245), the claim refused (245).
- [`results/local.md`](results/local.md): **the round to the burns on a local LEZ v0.3.1 standalone sequencer and
  anvil**: the vault deployed by `vault/script/deploy.sh`, two deposits, the stand-in's record, `Init`, `ImportRoot`,
  the public claim and two refusals, the private claim and the private burn **proven for real by LEZ's wallet**
  (169.5 s and 157.8 s, 247,826 and 248,346 bytes, 9.3 GiB peak), the public burn; every verdict read back over RPC
  (24 checks, 0 fail).
- `results/fork/`: `sepolia.sh --fork` on a fork of Sepolia at block 12,161,092: the inbound-only vault paired with a
  v0.3.1 configuration (the crown run's verifier, library and token reused), its two deposits, slot 2 = `configId`.
- `harness/tests/light_client_layout.rs`: the records the bridge reads are where, and as, the merged light client's
  wire crate (`lez-v03/wire`) writes them.

## Reproduce

From this directory, with LEZ v0.3.1 cloned at `../../upstream/lez-v0.3.1` (tag `v0.3.1`, commit `1a003bfb`):

```sh
RISC0_HOME=<RISC Zero home with Rust 1.91.1> cargo test -p ferrata-lez03-bridge-core
RISC0_DEV_MODE=1 cargo test -p harness03 --test synthetic      # writes results/synthetic.md
cargo test -p harness03 --test sepolia                          # writes results/sepolia.md
cargo build --release -p driver03                               # bridge03, the driver (LEZ's wallet inside)
(cd ../../upstream/lez-v0.3.1 && cargo build --release --features standalone -p sequencer_service)
./local.sh <run dir outside the repository>                     # the local round, real proofs; then results/local
python3 -I verify.py --lez http://127.0.0.1:3040 --eth http://127.0.0.1:8545 results/local   # while they run
./testnet.sh plan                                               # FERRATA_NET=testnet ETH_RPC=<Sepolia>: read-only
```

The public run (prepared, not sent): `testnet.sh prepare` (this run's LEZ accounts in the queued sender's wallet
home, the configuration id, the deposits' commitments), the vault and its two deposits on Sepolia, then
`testnet.sh run` (refused unless `FERRATA_TESTNET_SEND=yes`): the public steps as **one batch for one block** in
the format of testnet-ops's queued sender (`bridge03 <run> batch`; `ferrata_ops batch` sends it with explicit nonces
and tight declared gas: 400,000 for `Init`, 450,000 for `ImportRoot`, 250,000 for the claim, 420,000 for the burn,
about 1.5 times the measured cycles), then the private claim and the private burn, proven by the wallet; then
`testnet.sh verify`, which reports a check whose block is not final yet as PENDING. `sepolia.sh` deploys an
inbound-only vault (mode B, RISC Zero's verifier: nothing releases without a real checkpoint proof) and rehearses on
a fork (`--fork`); for the live run, branch `testnet-return`'s Sepolia vault (with a stand-in checkpoint verifier,
so that testnet burns can be released) supersedes it; `sepolia.sh` stays as the inbound-only rehearsal.
