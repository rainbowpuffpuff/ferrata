# FerrataVault v3: the Ethereum half of a private lock-and-mint bridge (spike)

`src/FerrataVault.sol` is the Ethereum vault of a privacy-preserving lock-and-mint bridge to LEZ (Logos RFP-022). It
implements the bridge specification v2 as amended by the v3 changes: D3 to D5 and D9 of the delta, including
`cancelCandidate`. D7 (mode B) is specified, not built; D8 lists limits that stay as they are.

**Specification.** "SPEC-v2 §x" is [`docs/design/spec/bridge-spec-v2.md`](../docs/design/spec/bridge-spec-v2.md)
and "SPEC-v3-delta Dn" (or "v3 Dn") is
[`docs/design/spec/bridge-spec-v3-changes.md`](../docs/design/spec/bridge-spec-v3-changes.md). Source comments
cite the same two files by their working names, `notes/design/SPEC-v2.md` and `notes/design/SPEC-v3-delta.md`.

This directory was the separate `bridge-evm` repository, imported with its history; its versions are commits of
that history:

| Version | Commit | Implements |
|---|---|---|
| v1 (tag `v1` there) | `a01fcd7` | the first design |
| v2 (branch `v2` there) | `1d61b32` | SPEC-v2 |
| v3 (branch `v3` there; this one) | `b631b23` | SPEC-v2 + SPEC-v3-delta |

**Inbound.** Users deposit fixed denominations of ETH or a registered ERC-20, with a hiding commitment to their LEZ
destination. Each deposit becomes a leaf of a SHA-256 Merkle tree, and its root sits in storage slot 0, where the
LEZ side reads it with an Ethereum storage proof.

**Outbound.** The vault follows LEZ through proven checkpoints. A checkpoint is first a candidate, then must wait
`W = Δ + (t − 1) × turnSeconds`, and is then finalised with a second proof that echoes an Ethereum beacon. Releases
pay out against a RISC Zero proof of a burn under a finalised, unrevoked burn root.

Status: prototype evidence for a grant proposal. **Not audited, not deployed anywhere.** Proofs are mocked: the
tests' base verifier accepts exactly the (image id, journal digest) pairs a test registers. It sits behind RISC
Zero's unmodified `RiscZeroVerifierEmergencyStop`, as a deployment's would. The commitment and nullifier
derivations, the LEZ programs and the zkVM guests are out of scope.

## Running it

```sh
export PATH="$HOME/.foundry/bin:$PATH"
# Dependencies (lib/ is not committed):
forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.0.0
forge build --sizes                                      # no compiler warnings, no lint findings, all under EIP-170
forge test                                               # everything, including the invariant campaign
forge test --isolate --match-contract GasScenarios -vv   # the numbers in GAS.md
forge test --gas-report --no-match-path 'test/invariant/*' > gas-report.txt
forge coverage --report summary
forge fmt --check
FERRATA_WRITE_VECTORS=1 forge test --match-contract TreeVectorsTest   # regenerate test/vectors/tree-v2.json
python3 -I tools/check_tree_vectors.py test/vectors/tree-v2.json     # independent check of the vectors
python3 -I tools/mutation_spotcheck.py [start stop]                   # hand-written mutants, sliceable
```

Toolchain and dependencies:
- forge 1.3.6, solc 0.8.26 (pinned, `offline = true`), `evm_version = "cancun"`, optimizer at 200 runs.
- forge-std v1.10.0.
- OpenZeppelin Contracts v5.4.0. The vault uses only `SafeERC20` and `ReentrancyGuardTransient`.
- risc0-ethereum v3.0.0. The tests use only `RiscZeroVerifierEmergencyStop` and the verifier interface, compiled
  against the OpenZeppelin v5.2.0 that risc0-ethereum itself pins.

The vault takes the vendored one-function `IRiscZeroVerifier`, whose selector is the same as RISC Zero's. Its
runtime is 23,650 bytes, 926 below the EIP-170 limit, so there is little room left for more features.

## How it works

### Deposits

`deposit(token, denomIndex, C, index)` takes an ERC-20; `depositETH(denomIndex, C, index)` takes ETH, whose token
is `address(0)`.
- The amount is the registered denomination at `denomIndex`.
- `index` must equal `depositCount[msg.sender]`.
- `C` must be non-zero, is opaque to the vault, and may repeat.
- ERC-20s are pulled with `safeTransferFrom`, which works for tokens like USDT that return nothing. The vault's
  balance must then grow by exactly the amount, which rejects fee-on-transfer tokens.
- The leaf binds the depositor (`msg.sender`) and its 64-bit index.
- Leaf indices stay below 2^32 (D9): once the tree holds 2^32 leaves, both deposit functions revert, and a refund
  for an index of 2^32 or more is refused. Indices `i` and `i + 2^32` would otherwise share a path.
- After `retire()`, deposits revert forever.

### LEZ checkpoints: the finality rule in plain words

The vault keeps `finalState`, the latest finalised LEZ checkpoint state, which starts at the immutable
`genesisState`. Moving it forward takes three steps.

1. **Candidate.** Anyone submits a checkpoint proof whose `parentState` is `finalState` and whose `echoNonce` is 0.
   Its journal states the deployment's `nMin`/`tMin`, the tip `T` the checkpoint was cut at (`tipId`, `tipHash`),
   and the size `n` and threshold `t` of the key set in `newState`. The vault itself requires `n ≥ nMin` and
   `t ≥ tMin`. The program behind the proof checks one inscription of `T` by its producer `D` and shows that at
   least `t` keys other than `D` built on `T`. The vault records the candidate with its tip and `t`, the time, and
   the current Δ (`deltaAtSubmit`). It also records a beacon: the block time plus
   `keccak256(prevrandao, block.number, vault)`.
2. **Wait `W = deltaAtSubmit + (t − 1) × turnSeconds`.** `turnSeconds` is the channel's posting timeframe, an
   immutable. The turn term covers `t − 1` corrupted keys each holding a full turn with empty inscriptions. In
   this window the candidate's tip is public, and a monitor compares it with an honest LEZ node's final tier.
3. **Finalisation.** Anyone takes a beacon recorded at least `W` after the candidate. Its nonce could not be known
   before then. Someone posts that nonce on LEZ, and a second proof shows `t` other sequencers building on blocks
   that contain it. The vault checks that the second proof echoes exactly that beacon's nonce, carries the
   candidate's fields, and names a key set that meets the floors. It also checks monotonicity: `importedLeafCount`
   never decreases along finalised checkpoints, and once one is a sunset checkpoint, every later one is a sunset
   checkpoint with the same count. The candidate then becomes final: its burn root is accepted, and the first
   sunset checkpoint sets the refund boundary.

Candidates that are never finalised are harmless, because everything starts from `finalState`. A revocation
invalidates every pending candidate; a finalisation does not. The assumptions are stated below.

### Reject-only powers of the freeze authority

- **`cancelCandidate(newState)`.** Deletes a pending (not yet finalised) candidate and emits
  `CandidateCancelled(newState, tipId, tipHash)`. Unknown and already finalised states are refused. Finalised
  checkpoints are untouched, and the same state can be submitted again as a fresh candidate, which waits a full
  `W` from its new submission time: a cancellation costs an honest prover one wait. The monitor calls it when a
  candidate's tip is not in an honest node's final tier by `submittedAt + Δ_g`.
- **The checkpoint freeze** (`setCheckpointsFrozen`) stops both `submitCandidate` and `finalize`. It is the general
  pause and lapses like every freeze.
- **`revokeFrom(index)`**, within `revokeWindow` of that checkpoint's finalisation, and only for a checkpoint not
  already revoked. It marks checkpoint `index` and every later one revoked, so their burn roots stop counting. It
  restores `finalState`, and that state's imported leaf count and sunset flag, to those `index` was built on. If a
  revoked checkpoint set the refund boundary, the boundary is cleared. Used nullifiers stay used. It accepts
  nothing: the restored state needs a fresh candidate and a full wait.
- **`retire()`**, one-way. It stops deposits forever. It cannot touch escrow, checkpoints or releases.

### Refunds

`refund(leafIndex, depositor, token, denomIndex, C, path)` (anyone) returns a deposit LEZ never imported. It needs:
- a retired vault, so the deposit root no longer moves and a refund's path cannot go stale;
- a refund boundary: the first non-revoked sunset checkpoint, whose `importedLeafCount` is the boundary. A later
  sunset checkpoint can neither lower it nor move it;
- `block.timestamp ≥ sunsetFinalizedAt + revokeWindow`, so that a forged sunset checkpoint can be revoked before any
  refund;
- no global release freeze in force;
- a leaf at or above the boundary, below `nextLeafIndex`, not refunded before, with a 32-step path to the current
  `depositRoot`.

It pays the depositor named in the leaf.

### Releases and pull payments

`release(r, seal)` takes the request `{token, denomIndex, recipient, relayer, relayFee, burnRoot, nullifier}`. It
requires:
- a burn root that is accepted, meaning finalised and not revoked;
- the token's release delay elapsed since that root's finalisation. The delay is the one in force when the root
  was finalised: a change applies only to roots finalised afterwards. Every delay setting, at registration
  included, satisfies `minReleaseDelay ≤ delay ≤ revokeWindow`, so a checkpoint stays revocable for as long as its
  releases are delayed;
- an unused nullifier;
- `relayFee` equal to 0 or exactly the registered fee for that denomination;
- enough escrow and cap;
- no freeze in force;
- a valid release proof.

If `relayer` is `address(0)`, the fee goes to `msg.sender`.

Any payout that fails (recipient, relayer, or a refunded depositor) is credited to `owed[payee][token]`. The
release still completes and the nullifier is used. Only the payee can withdraw, with `claimOwed(token)`, and only to
itself. ETH is sent with a `call` that copies no return data, so a hostile recipient cannot run the vault out of gas
with a huge revert.

### Verifier and stop

`verifier` is immutable. A deployment points it at a `RiscZeroVerifierEmergencyStop` wrapping the Groth16 verifier,
with the freeze authority as guardian; anyone with a proof of exploit can also stop it. A stop is one-way. Every
later call that needs a proof reverts: `submitCandidate`, `finalize` and `release`. Cancellation, refunds,
`claimOwed`, revocation and role handovers still work. Nothing can change what the vault accepts.

### Freezes

The freeze authority can freeze deposits, releases (globally or per token) and checkpoints. The global release
freeze also stops refunds; the checkpoint freeze stops submission and finalisation. A freeze lasts `maxFreeze`
(immutable) unless renewed; freezing again restarts it, and unfreezing ends it at once. A lost freeze key therefore
cannot lock the vault for good.

## Storage-slot contract

| Slot | Variable | Notes |
|---:|---|---|
| 0 | `depositRoot` (bytes32) | proven on LEZ by an Ethereum storage proof |
| 1 | `nextLeafIndex` (uint64) | alone in its slot |
| 2 | `storedLezConfigId` (bytes32) | copy of the immutable `lezConfigId`; LEZ `Init` attests it (reciprocal pairing) |
| 3–34 | `_branch[0..31]` | incremental-tree frontier |

Nothing is declared before these, and the only parent contract (`ReentrancyGuardTransient`) uses transient
storage. Tests read slots 0–2 with `vm.load`, including in every step of the invariant campaign. The rest of the
layout (`forge inspect FerrataVault storageLayout`):
- 35–38: the roles, and `retired`;
- 39–41: the deposit, release and checkpoint freeze deadlines;
- 42–53: checkpoint state (`finalState`, `delta`, `revocationEpoch`, beacons, candidates, history, burn-root index,
  refund boundary and its finalisation time, and the imported leaf count and sunset flag of `finalState`);
- 54–65: registry, per-token freezes, delay history, escrow, owed, counters, nullifiers, refunded leaves, per-token
  buckets;
- 66–69 and 70–73: the global buckets.

## Encodings

All hashes are SHA-256 unless stated otherwise. `||` is concatenation; integers are big-endian.

```
LEAF_TAG          = sha256("ferrata/vault/leaf/v2")       = 0x17b9508a0d0cbf61207c0fcb66f06ed92a6aeec110a54860479f0bf3cd6aff0b
leaf              = sha256(0x00 || LEAF_TAG || leafIndex:u64 || depositor:20 || token:20 || amount:u256 || C:32)   (145 bytes)
node              = sha256(0x01 || left || right); empty leaf = 32 zero bytes; zero[h+1] = node(zero[h], zero[h])
empty-tree root   = zero[32] = 0x782d35b1fdad7d54e7a1b36a2ab1021e872c7692bb80fdd12bfc321e9e420409
CHECKPOINT_DOMAIN = sha256("ferrata/vault/checkpoint/v3") = 0xbdb3b52df781536a6bcce7088d156b3951a00a45ec8bcf2805ba8406fbeb5ec7
RELEASE_DOMAIN    = sha256("ferrata/vault/release/v2")    = 0x6e04f4b1fa66535e182a1ef2e4f5bfdfe44f7257f1663ef81ffa4f38d7696e51
beacon nonce      = keccak256(abi.encode(block.prevrandao, block.number, vault))
```

Both journals are `abi.encode` of static values, one 32-byte word each, and the vault passes `sha256(journal)` to
`verify(seal, imageId, digest)`:

| Journal | Image | Words, in order |
|---|---|---|
| Checkpoint (SPEC-v3 D3) | `checkpointImageId` | `CHECKPOINT_DOMAIN`, `block.chainid`, vault, `lezConfigId`, `nMin`, `tMin`, `parentState`, `newState`, `burnRoot`, `burnCount`, `importedLeafCount`, `sunset` (0/1), `echoNonce`, `tipId`, `tipHash`, `n`, `t` (17 words) |
| Release (spec §5.2) | `releaseImageId` | `RELEASE_DOMAIN`, `block.chainid`, vault, `lezConfigId`, `burnRoot`, `NOut`, `token`, `amount`, `recipient`, `relayer`, `relayFee` (11 words) |

- A candidate's checkpoint journal has `echoNonce = 0`; a finalisation's has the named beacon's nonce.
- `CandidateSubmitted` and `CheckpointFinalized` carry `tipId`, `tipHash`, `n` and `t`.
- `amount` is the registered denomination at `denomIndex`.
- `relayer` is bound as submitted, so `address(0)` means "whoever submits", which can be front-run.
- `test/vectors/tree-v2.json` holds 8 concrete deposits from three depositors. For each it gives the leaf
  preimage, the leaf, the root after insertion and the Merkle path, plus the 33 zero hashes. It is generated from
  the vault and rechecked independently by `tools/check_tree_vectors.py`.

## Roles

| | Can | Cannot |
|---|---|---|
| **admin** | Register a token once: denominations, relay fees (≤ 1% each), `lezUnit` and `unitWeight` are then fixed. Enable or disable deposits per token. Set caps. Set release delays, for roots finalised later, always with `minReleaseDelay ≤ delay ≤ revokeWindow` (and ≤ 30 days). Raise Δ (never below `deltaFloor`, at most 30 days). Hand the role on in two steps. | Freeze or unfreeze, cancel, revoke, retire, stop the verifier. Change a registered token's denominations, fees, `lezUnit` or weight. Lower Δ. Touch escrow. Change the verifier, image ids, `lezConfigId`, `genesisState`, `nMin`, `tMin`, `deltaFloor`, `revokeWindow`, `maxFreeze`, `turnSeconds` or `minReleaseDelay` (all immutable). |
| **freeze authority** | Freeze and unfreeze deposits, releases and checkpoints (each freeze lapses after `maxFreeze` unless renewed). `cancelCandidate` a pending candidate. `revokeFrom` within the window. `retire()`. As the stop's guardian, stop the verifier. Hand the role on in two steps. | Touch the registry, caps, delays, Δ or escrow. Accept anything. Cancel a finalised checkpoint (only revoke it, within the window). Undo a stop or a retirement. |
| **anyone** | Deposit. `recordBeacon`. Submit and finalise checkpoints with proofs, and submit a cancelled state again. Release with a proof (as relayer). Refund once the rules above allow it. Stop the verifier with a proof of exploit. | |
| **payee** | `claimOwed` what it is owed, to itself. | |

Each role is handed on only by its current holder. One address cannot hold both roles. The stop's guardian is the
wrapper's own `Ownable2Step` owner, so it has to be handed over separately.

## Assumptions (SPEC-v3 D6, verbatim)

**Validity.** A burn root the vault accepts comes from valid LEZ blocks, provided:
(V1) fewer than `t` keys of every key set the checkpoint program has used are controlled by
an attacker, at any time, including keys that have since left the committee; (V2) an honest
sequencer extends only blocks its own node re-executed, and execution is deterministic;
(V3) the proof system is sound.

**Finality.** Those blocks are also final on LEZ (in every honest node's final tier, never
reverted), provided in addition:
- (F1) on the Logos Blockchain node of every honest sequencer, as its LEZ node has processed
  it, no reorganisation goes deeper than `k`, and at least `k` blocks arrive in every window
  Δ_g (no eclipse, no lag, no slowdown);
- (F2) whenever an honest sequencer holds a LEZ block of height `h`, a block of height at
  least `h` enters the canonical chain's fold within `(t − 1) × turn + S`. This needs: at most
  `t − 1` adversarial keys in the channel's live accredited set, not only in `K`; `turn` equal
  to the live `posting_timeframe` and non-zero; honest sequencers up except for stalls of at
  most S; their inscriptions neither censored nor kept off the chain by eclipse;
- (F3) nobody learns a beacon value more than ε before the Ethereum block that records it;
- (F4) `W >= Δ_g + (t − 1) × turn + S + ε`, which the vault enforces together with D3's rule
  that the program refuses a change of `posting_timeframe`.

**What breaks it.**
- `t` keys can sign any history. The candidate is public for `W` before it can finalise;
  `monitor`'s tip check cancels it, and `revokeFrom` un-accepts it within the window.
- Turn hogging by the adversary's own keys does not break the rule: `W` absorbs it.
- A never-final block can be finalised with fewer than `t` keys, even with none, if F2 fails
  for more than S beyond the hog and the attacker can twice show two honest sequencer nodes a
  short private fork. F2 fails if honest sequencers stall or their inscriptions are kept off
  the chain.
- Two honest sequencer nodes eclipsed for about `W` defeat the wait whatever the other honest
  sequencers do.
- `monitor`'s tip check stops all of these, provided its own node is not eclipsed, it and the
  freeze authority are live, and the offending candidate is cancelled.

**Floors.** Outside testnets `nMin >= 3` and `tMin >= 2`. The rule tolerates `n − 1 − t`
silent keys: none for `n <= 5`.

**Chains.** The beacon is `prevrandao`; a deployment on an EVM chain whose `prevrandao` is
constant or sequencer-chosen needs another beacon source and is out of scope.

What the vault enforces of this: `W` from the candidate's own `t` and the immutable `turnSeconds` (F4 holds if the
deployment sets `deltaFloor ≥ Δ_g + S + ε`, 3 h 01 min by default, and the deployment check pins `turnSeconds`);
`n ≥ nMin` and `t ≥ tMin` on every checkpoint journal. The rest is the program's, the monitor's and the deployment's.

## Where I interpreted or departed from the spec

Kept from v2:
- **`finalize` also requires the deployed `nMin`/`tMin`** in its journal. §4.3 lists this check for candidates only.
- **`finalize(beaconIndex, journalFields, seal)`.** The spec's separate `newState` argument is taken from the
  journal fields.
- **Release delays as a history.** The delay is stored as a history per token (`fromCheckpoint`, `delay`). It is
  ordered by checkpoint index, which is exact even when a change and a finalisation share a block. Registration
  sets the initial delay.
- **Bounds the spec does not state:** `deltaFloor` non-zero; release delays at most 30 days (on top of D5's
  `≤ revokeWindow`).

(v2's other readings are now spec: revocation invalidates pending candidates, an already revoked checkpoint cannot
be revoked again, Δ has a 30-day ceiling.)

New in v3:
- **What `finalize` compares.** It compares the v2 fields with the candidate (`parentState`, `burnRoot`,
  `burnCount`, `importedLeafCount`, `sunset`, and `newState` as the key), not `tipId`, `tipHash`, `n` or `t`.
  `newState` commits to the tip and the key set (SPEC-v2 §4.1), so a sound proof of the same state carries the same
  values. The wait always uses the candidate's stored `t` (D4: "with `t` from its journal"); the floors are checked
  on each journal.
- **Monotonicity is checked at finalisation only**, as D4 says ("along finalised checkpoints"). A candidate that
  breaks it can be submitted, but never finalised.
- **Only the global release freeze stops refunds**, as D5 words it. A per-token release freeze stops that token's
  releases, not its refunds.
- **Constructor bounds:** `0 < turnSeconds ≤ 30 days`; `minReleaseDelay ≤ revokeWindow` and `≤ 30 days`.
- **`cancelCandidate`** refuses a state that was never submitted and one already finalised alike
  (`UnknownCandidate`; the vault does not tell them apart). It also deletes a stale candidate, which is harmless.
  It works under the checkpoint freeze and after a verifier stop, since it needs no proof. To put the tip in
  `CandidateCancelled`, every candidate stores `tipId` and `tipHash`, which costs about 22k gas per submission.
- **The boundary second.** `revokeFrom` is allowed up to and including `finalizedAt + revokeWindow`, and refunds
  (and releases under a delay equal to `revokeWindow`) open at that same second, as D5's `>=` says. Within that one
  second a refund and a revocation can land in either order.
- The vault checks `n` and `t` against the floors, not that `t = t(n)` (the program's rule), and it does not
  compare `importedLeafCount` with its own `nextLeafIndex`.

## Known limits (SPEC-v3 D8, kept) and residual risks

- **A tripped verifier stop strands all escrow except owed balances**, unless a sunset checkpoint was finalised
  first. No new checkpoints follow, and nothing more is released.
- **The stop wrapper's owner is not moved by a freeze-authority handover**, and the upstream wrapper lets its owner
  renounce.
- **The freeze key is powerful.** A compromised freeze authority can keep renewing freezes, cancel every candidate,
  revoke honest checkpoints within the window, or retire the vault: a denial of service, not theft. Replacing it
  through the admin under a timelock is an open decision. The admin can stall exits by setting caps to 0.
- **Refunds now depend on the freeze authority.** Only it can `retire()`, and refunds need a retired vault. A freeze
  key lost before retirement means no refunds, ever.
- **A payee that can never receive** (a contract without `receive`, a permanently blacklisted address) keeps an
  `owed` balance it cannot collect.
- **Retired keys stay counted** until a finalised checkpoint carries the rotation; LEZ's `exit_delay` (about 40 min
  on testnet values) is shorter than `W`.
- **Escrow backing tokens that are never burned before the checkpoint program can no longer follow LEZ stays in the
  vault.** No party can move it.
- **Issuer risk is inherited.** A token can blacklist the vault or switch on transfer fees (USDT has such a switch).
  Rebasing tokens are unsupported.
- **The global cap uses admin-set integer weights, not an oracle.**
- **No real proofs.** All verification here is mocked. The Groth16 verifier's gas and behaviour are not measured.

## Tests

All tests are in `test/`. The mocks are in `test/mocks/`. `test/utils/ReferenceMerkle.sol` is a plain,
non-incremental restatement of the tree. `test/utils/VaultTestBase.sol` rebuilds both journals word by word from
the table above and drives real candidate → beacon → finalisation sequences with the test committee (n = 4, t = 3,
`turnSeconds` = 900 s, Δ = 4 h, so `W` = 4 h 30 min).

| File | What it checks |
|---|---|
| `Deposit.t.sol` | ETH, 6- and 18-decimal, USDT-style; fee-on-transfer and false-returning tokens rejected; denomination, index, commitment rules; unregistered and delisted tokens; a full tree refuses both deposit kinds and changes nothing (D9); plain ETH and unknown calls refused |
| `Checkpoint.t.sol` | Beacons (one per block, nonce from prevrandao). Candidate rules. The finality rule: no beacon old enough, too-early beacon, nonce not matching the named beacon, stale parent, competing candidates, a Δ raise applying only to later candidates. The committee-sized wait, refused one second before `W` and accepted at it for t = 2, 3, 5, 7; the wait is the candidate's. Key-set floors at submission and finalisation. The checkpoint freeze stops finalisation until it lapses. Monotonicity: a lower count and a dropped sunset are refused; a later sunset checkpoint cannot lower or move the boundary. `cancelCandidate`: only the freeze authority; unknown or finalised states refused; a cancelled candidate cannot be finalised; resubmission works and restarts the wait; works under the freeze. Δ only up and capped. |
| `Revocation.t.sol` | Inside and outside the window. Release against a revoked root reverts. Nullifiers used before a revocation stay used. A re-proven checkpoint is accepted again, but the same nullifier never pays twice. Pending candidates are invalidated. A revocation restores the imported leaf count and the sunset flag. |
| `Refund.t.sol` | Retirement is one-way. Refunds are refused before retirement, before sunset, below the boundary, before `revokeWindow` has passed (refused one second before, paid at it), under the global release freeze (not under a per-token one), twice, with a wrong path, to anyone but the leaf's depositor, for an index of 2^32 or more, and when releases took the escrow. A forged sunset checkpoint revoked inside the window pays nothing, then or later. The first sunset checkpoint fixes the boundary and the opening time. |
| `Release.t.sol` | Releases of every token kind, delisted tokens, nullifiers, relay fees, the relayer, escrow limits, image-id separation, the always-rejecting verifier, re-entry through recipient and token |
| `ReleaseDelay.t.sol` | Delay changes apply only to later roots (raising and lowering), per-token delays, the registration delay; every delay setting between `minReleaseDelay` and `revokeWindow`; a fuzzed check of the history lookup against a linear scan |
| `PullPayment.t.sol` | ETH recipient that rejects then claims; blacklisted ERC-20 recipient; rejecting relayer; 600 kB revert bomb; gas burner; refund to a rejecting depositor; `claimOwed` only by the payee, not re-enterable; one stuck payee blocks nobody |
| `JournalBinding.t.sol` | One test per field of the release journal, and per field of the 17-word checkpoint journal at both `submitCandidate` and `finalize` (a proof differing in exactly that field is rejected, with a positive control in the same test); relayer redirection, fee and replay attacks |
| `EmergencyStop.t.sol` | After a guardian stop, `submitCandidate`, `finalize` and `release` revert, while cancellation, refund, `claimOwed`, revocation and both role handovers work; the proof-of-exploit stop; no undo |
| `Freeze.t.sol` | All 32 combinations of the five freezes; every freeze lapses after `maxFreeze`; renewal; a lost key cannot lock the vault |
| `Admin.t.sol` | Constructor checks, including `turnSeconds` and `minReleaseDelay`; the v3 domain tag; write-once registry; relay-fee ceiling (fuzzed); role separation; two-step handovers |
| `Caps.t.sol`, `Merkle.t.sol`, `Privacy.t.sol`, `TreeVectors.t.sol`, `GasScenarios.t.sol` | Rate limits; slots 0, 1 and 2 and tree algebra; LEZ-side data never appears in calldata, events or storage writes; vectors; gas |
| `invariant/` | 10 invariants over 256 runs × 100 calls, listed below |

The invariant handler deposits, submits candidates with committees of four sizes (n, t) = (3, 2), (4, 3), (7, 5),
(10, 7) and finalises them after their own `W`, submits candidates that break monotonicity and cancels candidates,
then tries to finalise both kinds after their full wait with valid proofs. It also revokes, retires, refunds,
releases (some to payees that cannot receive), claims, replays nullifiers and refunds, donates, moves time, and
changes freezes, caps and delays. Its 10 invariants:
- `balance ≥ escrowed + totalOwed`, and `escrowed == deposits − releases − refunds`, for every token;
- `totalOwed` equals the sum over all payees;
- a used nullifier or refunded leaf is never paid again;
- `finalState` is genesis or a state that went through `finalize`, and it is the newest non-revoked checkpoint's
  state;
- the valid (non-revoked) checkpoints form one chain from genesis along which the imported leaf count never
  decreases and, after the first sunset checkpoint, stays fixed with every checkpoint a sunset one; each records
  its parent's count and flag, and the vault's current ones are the chain's last;
- the refund boundary, and the time refunds open from, are those of the first valid sunset checkpoint;
- no candidate that breaks monotonicity or was cancelled is ever finalised; nothing is submitted or finalised under
  a checkpoint freeze; and, judged from the handler's own records, a refund is paid exactly when the rules allow it
  (refused although allowed counts as a failure too);
- the leaf count, root and slot 2 match the reference.

Results of the final runs, on 2026-11-10:

```
forge build --force --sizes   Compiler run successful!   (no compiler warnings, no lint findings; all contracts under EIP-170)
forge test             Ran 21 test suites in 17.02s (75.05s CPU time): 286 tests passed, 0 failed, 0 skipped (286 total tests)

forge coverage --report summary      (all 286 tests, including the invariant campaign)
| File                 | % Lines           | % Statements       | % Branches        | % Funcs          |
| src/FerrataVault.sol | 100.00% (456/456) | 100.00% (584/584)  | 100.00% (108/108) | 100.00% (83/83)  |
| Total (incl. tests)  | 95.96% (974/1015) | 96.60% (1164/1205) | 94.44% (170/180)  | 94.33% (183/194) |

tools/check_tree_vectors.py   ok: 8 leaves, 33 zero hashes, roots and paths match
tools/mutation_spotcheck.py   127/127 mutants killed
```

The unit tests alone (without the invariant campaign) also cover 100% of the vault's lines, statements, branches and
functions.

Fuzz tests use 256 runs. Invariants use 256 runs × 100 calls each. Handler calls with nothing to do are discarded,
so the run table counts exactly what happened. Averaged over the ten campaigns of the final run, one campaign has
about 1,050 deposits, 1,680 candidates, 670 finalisations, 470 candidates that break monotonicity, 200
cancellations, 280 refused finalisations of those two kinds, 365 revocations, 140 releases and 17 refunds (174 over
the ten campaigns); the vault is retired in about 160 of the 256 runs. Refunds stay rare because one 100-call run
must retire the vault, finalise a sunset checkpoint and wait `revokeWindow` first. v2's README counted handler calls
that did not revert, including calls that had nothing to do, so its figures (1,500 finalisations, 900
revocations, 640 releases, 350 refunds per campaign) overstated what happened.

Coverage shows only that code ran. The mutation spot-check plants 127 hand-written one-line bugs in a scratch copy
of the contract, across deposits, the tree, the registry and delay bounds, the constructor, candidates, the
committee checks and the wait, beacons, finalisation and monotonicity, the journal layout, cancellation,
revocation, refunds and their opening rules, releases, pull payments, freezes, roles and caps. Each is caught by
named failing tests; for example, dropping the release-freeze check from `refund` is caught by
`test_refund_refusedUnderTheReleaseFreeze`. It is not an exhaustive mutation run.
