# FerrataVault: the Ethereum half of a private lock-and-mint bridge (spike)

`src/FerrataVault.sol` is the Ethereum vault of a privacy-preserving lock-and-mint bridge to LEZ (Logos
RFP-022). Users deposit fixed denominations of ETH or a registered ERC-20, with a hiding commitment to their LEZ
destination. Each deposit becomes a leaf of a SHA-256 Merkle tree, and its root sits in storage slot 0, where the
LEZ side reads it with an Ethereum storage proof. Releases pay out against a RISC Zero proof of a LEZ burn, under a
LEZ burn root that was itself accepted with a proof.

Status: prototype evidence for a grant proposal. **Not audited, not deployed anywhere.** The proofs are mocked: the
tests use a verifier that accepts exactly the (image id, journal digest) pairs a test registers. The commitment and
nullifier derivations, the LEZ programs and the zkVM guest programs are out of scope.

## Running it

```sh
export PATH="$HOME/.foundry/bin:$PATH"
forge build                                                  # no warnings, no lint findings
forge test                                                   # unit, fuzz, invariant, vector, privacy, gas tests
forge test --isolate --match-contract GasScenarios -vv       # the numbers in GAS.md
forge test --gas-report > gas-report.txt
forge coverage --report summary
forge fmt --check
FERRATA_WRITE_VECTORS=1 forge test --match-contract TreeVectorsTest   # regenerate test/vectors/tree.json
python3 -I tools/check_tree_vectors.py test/vectors/tree.json        # independent check of the vectors
python3 -I tools/mutation_spotcheck.py                               # hand-written mutants (a few minutes)
```

Toolchain: forge 1.3.6, solc 0.8.26 (pinned, `offline = true`), `evm_version = "cancun"`, optimizer at 200 runs.
Dependencies, installed with `forge install --no-git`: forge-std v1.10.0, and OpenZeppelin Contracts v5.4.0, of
which only `SafeERC20` and `ReentrancyGuardTransient` are used. `IRiscZeroVerifier` is vendored as a one-function
interface.

## How it works

**Deposit.** `deposit(token, denomIndex, commitment, index)` for ERC-20s, and
`depositETH(denomIndex, commitment, index)` for ETH (the token is `address(0)`).
- The amount is the token's registered denomination at `denomIndex`. For ETH, `msg.value` must equal it.
- `index` must equal `depositCount[msg.sender]`. That gives every deposit a unique (depositor, index) pair, from
  which a client can rederive its secrets from the seed.
- `commitment` must be non-zero, is opaque to the vault, and may repeat. The vault never sees the LEZ destination.
- ERC-20s are pulled with `safeTransferFrom`, which works for tokens that return nothing (USDT). The vault's
  balance must then have grown by exactly the amount, which rejects fee-on-transfer tokens.
- Order: checks, then pull the tokens, then caps, counters, escrow, the leaf and the event.

**Burn roots.** `acceptBurnRoot(burnRoot, lezBlockId, burnCount, depth, seal)` is permissionless. It requires
`depth >= minLezDepth` and a valid proof from the root program.
- The first acceptance records `burnRootAcceptedAt[burnRoot] = block.timestamp`.
- Replaying the proof, or proving the same root at a later LEZ block, is a no-op for the timestamp. Otherwise
  anyone could restart the release delay.
- The highest `lezBlockId` seen is kept in `latestLezBlockId`.

**Release.** `release(ReleaseRequest r, seal)` is permissionless. The request is `{token, denomIndex, recipient,
feeRecipient, relayFee, burnRoot, nullifier}`. It requires:
- an accepted root, and `releaseDelay[token]` elapsed since that root's acceptance;
- an unused nullifier;
- `relayFee` equal to either 0 or exactly `standardRelayFee[token][denomIndex]`;
- enough escrow;
- a valid proof from the release program.

It then marks the nullifier, charges the caps, decrements `escrowed`, and pays `amount - relayFee` to the
recipient and `relayFee` to `feeRecipient`. If `feeRecipient` is `address(0)`, the fee goes to `msg.sender`. If a
payout fails, the whole call reverts: the nullifier stays unused and nobody else is affected. Deposits and releases
share a transient reentrancy guard.

**Escrow.** `escrowed[token]` goes up on deposit and down on release. A release can never exceed it. Tokens or ETH
that arrive any other way (a direct transfer, forced ETH) are not escrow and cannot be released. There is no rescue
function.

## Storage-slot contract

`depositRoot` is in **slot 0** (`DEPOSIT_ROOT_SLOT`) and `nextLeafIndex` is alone in **slot 1**. Nothing is
declared before them. The only parent contract, `ReentrancyGuardTransient`, uses transient storage. Tests read
both slots with `vm.load`, including inside the invariant campaign. Full layout, from
`forge inspect FerrataVault storageLayout`:

| Slot | Variable | Slot | Variable |
|---:|---|---:|---|
| 0 | `depositRoot` (bytes32) | 41 | `tokenReleasesFrozen` |
| 1 | `nextLeafIndex` (uint64) | 42 | `standardRelayFee` |
| 2–33 | `_branch[0..31]` | 43 | `releaseDelay` |
| 34 | `admin` | 44 | `escrowed` |
| 35 | `pendingAdmin` | 45 | `depositCount` |
| 36 | `freezeAuthority` | 46 | `burnRootAcceptedAt` |
| 37 | `pendingFreezeAuthority`, `minLezDepth`, `latestLezBlockId` | 47 | `nullifierUsed` |
| 38 | `depositsFrozen`, `releasesFrozen` | 48, 49 | per-token deposit and release buckets |
| 39 | `_tokens` (registry) | 50–53 | global deposit bucket |
| 40 | `tokenDepositsFrozen` | 54–57 | global release bucket |

## Deposit tree

The tree has depth 32 (2^32 leaves) and uses the SHA-256 precompile.

```
LEAF_TAG = sha256("ferrata/vault/leaf/v1")
         = 0x084d5599858dde3bdadaec9a70979121276922b791de25071d145ac6b13c4286
leaf     = sha256(0x00 || LEAF_TAG || uint64 leafIndex (8 bytes, big-endian) || token (20 bytes)
                  || uint256 amount (32 bytes, big-endian) || commitment (32 bytes))      -- 125-byte preimage
node     = sha256(0x01 || left || right)                                                   -- 65-byte preimage
empty leaf = 32 zero bytes;  zero[h+1] = node(zero[h], zero[h])
empty-tree root = zero[32] = 0x782d35b1fdad7d54e7a1b36a2ab1021e872c7692bb80fdd12bfc321e9e420409
```

`depositRoot` starts as the empty-tree root, so it is always the root of all deposits so far. Insertion keeps one
"completed subtree" node per height (`_branch`). It walks the new leaf's path once, writes exactly one branch slot
(none when the tree becomes full), and stores the new root. Empty-subtree hashes are recomputed from the node rule,
not hard-coded.

`test/vectors/tree.json` holds 8 concrete deposits (ETH, and USDC/DAI/USDT at their mainnet addresses), with each
leaf preimage, leaf and root after insertion, the 33 zero hashes, and every leaf's Merkle path to the final root. It
is generated from the Solidity vault, and `tools/check_tree_vectors.py` recomputes all of it independently with
Python's `hashlib`.

## Proof journals

The vault passes `sha256(journal)` to `verify(seal, imageId, journalDigest)`. A guest program must commit exactly
these bytes as its journal. Both journals are `abi.encode` of static values, i.e. one 32-byte word each, in this
order:

| Journal | Image | Words |
|---|---|---|
| Burn root (**provisional**) | `rootImageId` | `ROOT_DOMAIN`, `block.chainid`, vault address, `lezConfigId`, `lezBlockId`, `burnRoot`, `burnCount`, `depth` (8 words) |
| Release | `releaseImageId` | `RELEASE_DOMAIN`, `block.chainid`, vault address, `lezConfigId`, `burnRoot`, `nullifier`, `token`, `amount`, `recipient`, `feeRecipient`, `relayFee` (11 words) |

- `ROOT_DOMAIN = sha256("ferrata/vault/burn-root/v1")`.
- `RELEASE_DOMAIN = sha256("ferrata/vault/release/v1")`.
- `amount` is the registered denomination at `denomIndex`.
- `feeRecipient` is bound as submitted, so `address(0)` means "whoever submits it".

Every proof is bound to one pairing: `block.chainid` is read at verification time, and the vault address and
`lezConfigId` are fixed. `lezConfigId` hashes the LEZ network, zone, bridge program and configuration ids, off-chain.
The root-journal encoding lives in one internal function (`_rootJournalDigest`).

## Roles

| | Can | Cannot |
|---|---|---|
| **admin** | Register a token (once); enable or disable its deposits (delisting); set per-token and global caps, unit weights, standard relay fees, per-token release delays (≤ 30 days), and `minLezDepth`; hand the role on in two steps | Freeze or unfreeze; appoint the freeze authority; change the verifier, image ids or `lezConfigId` (immutable); change a token's denominations or `lezUnit`; move escrowed funds |
| **freeze authority** | Freeze or unfreeze deposits and releases, globally or per registered token; hand the role on in two steps | Touch the registry, caps, fees, delays or depth; move funds |
| **anyone** | Deposit; accept a proven burn root; submit a proven release (as a relayer) | |

Each role is handed on only by its current holder (`transferX`, then `acceptX` by the new address; proposing
`address(0)` cancels). One address cannot hold both roles. A global release freeze also stops `acceptBurnRoot`, so a
root cannot age through its release delay while releases are frozen. A freeze only delays: claims stay valid.

## Rate limits

Every token has a token bucket per direction, in token base units, and there is one global bucket per direction,
in value units. A token's value is `amount / lezUnit * unitWeight`, where `unitWeight` is set by the admin. The
tests use 1 value unit = 1e-6 USD.

- A bucket refills at `refillPerSecond` up to `capacity`. A rejection reverts and consumes nothing in either bucket.
- `capacity = type(uint256).max` disables a bucket.
- New buckets (at registration or in the constructor) start full.
- On an update, the refill earned under the old parameters is kept, then clamped to the new capacity. So lowering a
  cap takes effect at once, and raising one adds no instant headroom. A bucket that was unlimited starts full.
- Refill saturates without overflow for any parameters.

Registration requires:
- 1 to 8 strictly increasing, non-zero denominations, each at most 2^128 − 1;
- `lezUnit` a power of ten;
- every denomination an exact multiple of `lezUnit` (the "representable exactly on LEZ" rule);
- `0 < unitWeight ≤ 2^128 − 1`.

## Choices made where the brief was open

- `minLezDepth` and the two global buckets are constructor parameters, so a deployment never starts without a
  finality depth or global caps.
- A global release freeze blocks `acceptBurnRoot` (see Roles). Per-token release freezes do not, because roots are
  not per token.
- Re-accepting a root is a no-op rather than a revert. `latestLezBlockId` can still rise.
- `releaseDelay` is read at release time, so raising it also delays releases against already-accepted roots. It is
  capped at 30 days.
- A token is registered once. Its denominations and `lezUnit` are fixed, because in-flight claims refer to them.
- `recipient == address(0)` is rejected. The two roles must be distinct addresses.
- The constructor requires a verifier with code, non-zero and distinct image ids, and a non-zero `lezConfigId`.

## Known limitations

- **The root-proof journal is provisional.** The vault trusts the root program to establish that `burnRoot` is the
  LEZ burn-tree root at `lezBlockId` and that this block is final at `depth`. How LEZ consensus and finality are
  proven to Ethereum is not specified here, and `burnCount` is not used on-chain.
- **Rebasing tokens are unsupported.** A negative rebase breaks `balance ≥ escrowed`. Fee-on-transfer tokens are
  rejected at deposit. But a token that switches a fee on later (USDT has such a switch) would deliver less than
  `amount - relayFee` on release.
- **The global cap uses admin-set weights, not an oracle.** Weights are integers, so the value unit must be small
  enough to price the cheapest `lezUnit`.
- **Changing `standardRelayFee` invalidates in-flight proofs that bound the old fee.** The user must re-prove (with
  fee 0 or the new fee). The entitlement survives because the nullifier is unused, but this is friction against
  "claims stay valid".
- **Some payouts can get permanently stuck.** If the recipient cannot receive ETH, or the token blacklists the
  recipient, that release reverts every time, because the recipient is bound in the proof. Funds stay escrowed;
  there is no pull-payment fallback.
- **If the freeze key is lost while frozen, the vault stays frozen**, because by design the admin cannot unfreeze.
- **The admin can stall all releases** (caps set to 0, delays up to 30 days), but cannot single out a user.
- **No migration or rescue path.** A new verifier means a new vault. The old vault must stay live to release what
  it escrows.
- **Deposits must come from the depositor's own address**, because the recovery index is keyed on `msg.sender`.
  Routers and permit-relayed deposits would break seed-based recovery.
- **The tree holds 2^32 leaves**, after which a new vault is needed.
- **Gas figures exclude the real verifier** (see GAS.md).

## Tests

All tests are in `test/`. The mocks are `test/mocks/`. `test/utils/ReferenceMerkle.sol` is a plain,
non-incremental restatement of the tree, used as the oracle. `test/utils/VaultTestBase.sol` rebuilds both journals
word by word from the table above, rather than calling the vault's own encoder.

| File | What it checks |
|---|---|
| `Deposit.t.sol` | Deposits of ETH, a 6-decimal token, an 18-decimal token and a USDT-style token. Rejects fee-on-transfer and false-returning tokens, a wrong denomination index, a wrong per-depositor index, a zero commitment, unregistered and delisted tokens, ETH through the ERC-20 path, and a full tree. Allows a repeated commitment. Plain ETH and unknown calls are refused. |
| `Release.t.sol` | Root acceptance: first acceptance only, replay, depth below the minimum, a later depth increase. Releases of ETH, 6- and 18-decimal and USDT-style tokens; delisted tokens still release. Double release, unknown root, release delay, the relay-fee rule, fee to `msg.sender` versus a bound fee recipient, escrow limits, image-id separation, the always-rejecting verifier, a recipient that rejects ETH, and re-entry through the recipient and through the token. |
| `JournalBinding.t.sol` | One test per field of each journal: a proof whose journal differs from the vault's in that field alone is rejected, with a positive control in the same test. Also attack scenarios: redirecting funds, adding or redirecting a fee, and replaying a proof on another vault, chain or LEZ configuration. |
| `Admin.t.sol` | Constructor checks, domain tags, registry rules (including fuzzed `lezUnit` powers of ten), setters and events, role separation, and two-step transfers. |
| `Freeze.t.sol` | All 16 combinations of the four freeze flags, against a frozen token and a bystander token. A freeze delays but never invalidates a claim. The release delay restarts after an unfreeze. |
| `Caps.t.sol` | Exhaustion and refill over time, the global value-unit bucket across tokens, no partial consumption, UNLIMITED, update semantics, overflow-free refill, and a fuzzed throughput bound (accepted if and only if it fits; total ≤ capacity + refill × elapsed). |
| `Merkle.t.sol` | The slot 0 and slot 1 contract (`vm.load`), the empty root, a fuzzed incremental root versus the reference after every insertion, 130 deterministic insertions, exactly one branch write per insertion, fuzzed Merkle paths against `depositRoot`, and the high-bit paths set up with `vm.store`, including the last leaf and then `TreeFull`. |
| `invariant/` | A handler that drives deposits, root acceptances, releases, nullifier replays, donations, time, freezes, and cap and delay changes. Invariants: `balance ≥ escrowed` and `escrowed == deposits − releases` for every token; `nextLeafIndex ==` the number of deposits; the root equals the reference root; a used nullifier is never accepted again. 256 runs × 100 calls. |
| `TreeVectors.t.sol` | Builds `test/vectors/tree.json` from the vault and compares it byte for byte with the committed file. |
| `Privacy.t.sol` | Over the full state diff and logs of a deposit and of a release: LEZ-side values never appear in calldata, events or storage writes; the exact set of storage slots written and every event field are checked; failures carry only a 4-byte selector. These pass by construction today. They exist to catch a change that adds a destination-bearing field. |
| `GasScenarios.t.sol` | The scenarios in GAS.md. |

Results of the final runs, on 2026-11-09:

```
forge build --force    Compiler run successful!   (no compiler warnings, no lint findings)
forge test             Ran 15 test suites in 10.85s (31.42s CPU time): 159 tests passed, 0 failed, 0 skipped (159 total tests)

forge coverage --report summary
| File                 | % Lines           | % Statements      | % Branches      | % Funcs         |
| src/FerrataVault.sol | 100.00% (273/273) | 100.00% (322/322) | 100.00% (63/63) | 100.00% (54/54) |
| Total (incl. tests)  | 97.26% (568/584)  | 97.67% (628/643)  | 95.83% (92/96)  | 95.04% (115/121)|

tools/check_tree_vectors.py   ok: 8 leaves, 33 zero hashes, roots and paths match
tools/mutation_spotcheck.py   38/38 mutants killed
```

Fuzz tests use 256 runs. Invariants use 256 runs × 100 calls each.

Coverage shows only that code ran, not that a test would notice it being wrong. The mutation spot-check addresses
that in part. It makes 38 hand-picked one-line bugs in a scratch copy of the contract, and each is caught by at
least one named test. The list is in the script, and it includes subtle ones: an off-by-one in refill saturation,
a branch-node write at every left step (roots stay correct, but the gas property breaks), and a release journal
with recipient and fee recipient swapped. It is not an exhaustive mutation run.
