# FerrataVault v5: the Ethereum half of a private lock-and-mint bridge (spike)

`src/FerrataVault.sol` is the Ethereum vault of a privacy-preserving lock-and-mint bridge to LEZ (Logos RFP-022). It
implements the bridge specification v2 as amended by the v3 changes: D3 to D5 and D9 of the delta, and the vault
items of D11 and D13: both finality rules behind an immutable `mode` (rule (B) of D7 and D13 beside rule (A)),
watchers, the admin's 14-day role change, keyless retirement, the floor under the release buckets, refunds drawing
on the release buckets and stopping with the verifier, and D11's boundary second and journal comparison; v5 adds
D13's fees. D8 lists limits that stay as they are. `src/FerrataRegistry.sol` is its linked library (registration,
cap settings, role proposals, the checks of a revocation, fee schedules and their publication for LEZ, the refund's
leaf checks), which keeps the vault under EIP-170.

**Specification.** "SPEC-v2 §x" is [`docs/design/spec/bridge-spec-v2.md`](../docs/design/spec/bridge-spec-v2.md)
and "SPEC-v3-delta Dn" (or "v3 Dn") is
[`docs/design/spec/bridge-spec-v3-changes.md`](../docs/design/spec/bridge-spec-v3-changes.md). Source comments
cite the same two files by their working names, `notes/design/SPEC-v2.md` and `notes/design/SPEC-v3-delta.md`.

This directory was the separate `bridge-evm` repository, imported with its history; its versions are commits of
that history:

| Version | Commit | Implements |
|---|---|---|
| v1 (tag `v1` there) | `c417dc2` | the first design |
| v2 (branch `v2` there) | `d64845b` | SPEC-v2 |
| v3 (branch `v3` there) | `2f40b2f` | SPEC-v2 + SPEC-v3-delta D3–D5, D9 |
| v4 (branch `vault-v4` of this repository) | `3b0f059` to `c7703f6` | + rule (B), the vault items of D11 and D13 |
| v5 (branch `vault-fees`; this one) | `ebc0e72` and later | + D13's fees; the deployment script |

**Inbound.** Users deposit fixed denominations of ETH or a registered ERC-20, with a hiding commitment to their LEZ
destination. Each deposit becomes a leaf of a SHA-256 Merkle tree, and its root sits in storage slot 0, where the
LEZ side reads it with an Ethereum storage proof.

**Outbound.** The vault follows LEZ through proven checkpoints, under one of two rules fixed at deployment
(`mode`). Rule (A): a candidate waits `W = Δ + (t − 1) × turnSeconds` and is finalised with a second proof that
echoes an Ethereum beacon. Rule (B): one proof that `t` sequencers attested the tip as final; the candidate is final
once `W = Δ` (1 h by default) has passed, unless a watcher cancels it. Releases pay out against a RISC Zero proof of
a burn under a finalised, unrevoked burn root.

Status: prototype evidence for a grant proposal. **Not audited.** This v5 vault is deployed on Sepolia at
`0xfE3199fa2634413CbAC317371076Cae3EB2d9E54` (rule (B), testnet parameters) by the crown run, which made the whole round
trip through it on 11 November 2026 ([`../e2e/runs/crown-live-1/`](../e2e/runs/crown-live-1/README.md)); the v3 vault of the end-to-end
fixture is on Sepolia with one deposit (`../lez/live/inbound/README.md`, "On Sepolia"). In the vault's own tests (376 of the 407; the other 31, in `test/real/`, use real proofs) proofs are mocked: the base verifier accepts exactly the (image id, journal digest) pairs a test registers.
It sits behind RISC Zero's unmodified `RiscZeroVerifierEmergencyStop`, as a deployment's would. `test/real/` runs
the return leg once with no mock on the Ethereum side: RISC Zero's real Groth16 verifier behind the same stop, a
real proof of the checkpoint program for the candidate, a second one for the finalisation, and real proofs of the
release program ([`../e2e/README.md`](../e2e/README.md)), and `test/real/RuleBReal.t.sol` verifies the real rule (B)
checkpoint proof on a rule (B) vault. The LEZ side of those runs is a stand-in: blocks built with
test sequencer keys, no node, no bridge program. The commitment and nullifier derivations, the LEZ programs and
the zkVM guests are out of this directory's scope.

## Running it

```sh
export PATH="$HOME/.foundry/bin:$PATH"
# Dependencies (lib/ is not committed):
forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.0.0
FOUNDRY_OFFLINE=false forge build                        # once: forge is offline here (below); this fetches solc 0.8.26
forge build --sizes                                      # no compiler warnings, no lint findings; exits 1, see below
forge test                                               # everything, including the invariant campaign:
                                                         # 451 passed, 1 skipped (PrivacySuite, without its LEZ half)
forge test --isolate --match-contract GasScenarios -vv   # the numbers in GAS.md (mock verifier)
forge test --match-path 'test/real/*' -vv                # the return leg with RISC Zero's real verifier and real proofs
forge test --isolate --match-contract ReturnLegGas -vv   # its gas (GAS.md, "With the real verifier")
forge test --gas-report --no-match-path 'test/invariant/*' > gas-report.txt
forge coverage --report summary
forge fmt --check
FERRATA_WRITE_VECTORS=1 forge test --match-contract TreeVectorsTest   # regenerate test/vectors/tree-v2.json
python3 -I tools/check_tree_vectors.py test/vectors/tree-v2.json     # independent check of the vectors
python3 -I tools/mutation_spotcheck.py [start stop]                   # hand-written mutants, sliceable
```

Toolchain and dependencies:
- forge 1.3.6, solc 0.8.26 (pinned, `offline = true`: forge never downloads by itself, so on a machine without
  solc 0.8.26 the first build needs `FOUNDRY_OFFLINE=false`, or forge stops at "can't install missing solc 0.8.26
  in offline mode"), `evm_version = "cancun"`, optimizer at 200 runs.
- forge-std v1.10.0.
- OpenZeppelin Contracts v5.4.0. The vault uses only `SafeERC20` and `ReentrancyGuardTransient`.
- risc0-ethereum v3.0.0. The vault's own tests use only `RiscZeroVerifierEmergencyStop` and the verifier
  interface; `test/real/` and `script/` also deploy its `RiscZeroGroth16Verifier` with its `ControlID`. All are
  compiled unmodified against the OpenZeppelin v5.2.0 that risc0-ethereum itself pins.

The vault takes the vendored one-function `IRiscZeroVerifier`, whose selector is the same as RISC Zero's. Its
runtime is 24,254 bytes, 322 below the EIP-170 limit, with the library (12,504 bytes) holding the cold admin code;
further features need more of it moved to the library. `forge build --sizes` prints these and exits 1 with "some
contracts exceed the runtime size limit": the one over it is `VaultHandler` (37,962 bytes), the invariant campaign's
test-only handler, which is never deployed (forge does not apply EIP-170 to tests).

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
- After `retire()`, deposits revert forever. They also revert (`VaultRetired`) while a finalised sunset checkpoint
  is in force: LEZ imports nothing after `Sunset`, so such a deposit could be neither claimed nor refunded until
  retirement (review 2026-11-11 L-3).

### LEZ checkpoints: the finality rules in plain words

The vault keeps `finalState`, the latest finalised LEZ checkpoint state, which starts at the immutable
`genesisState`. Under rule (A) moving it forward takes three steps.

1. **Candidate.** Anyone submits a checkpoint proof whose `parentState` is `finalState` and whose `echoNonce` is 0.
   Its journal states the deployment's `nMin`/`tMin`, the tip `T` the checkpoint was cut at (`tipId`, `tipHash`),
   and the size `n` and threshold `t` of the key set in `newState`. The vault itself requires `n ≥ nMin` and
   `t ≥ tMin`, and refuses a `newState` that is already pending (D13). The program behind the proof checks one
   inscription of `T` by its producer `D` and shows that at least `t` keys other than `D` built on `T`. The vault
   keeps the SHA-256 digest of the journal the proof was verified against, the tip, `t`, the time, the current Δ
   (`deltaAtSubmit`) and the revocation epoch, in three storage slots. Under rule (A) it also records a beacon: the
   block time plus `keccak256(prevrandao, block.number, vault)`.
2. **Wait `W = deltaAtSubmit + (t − 1) × turnSeconds`.** `turnSeconds` is the channel's posting timeframe, an
   immutable. The turn term covers `t − 1` corrupted keys each holding a full turn with empty inscriptions. In
   this window the candidate's tip is public, and a watcher compares it with an honest LEZ node's final tier.
3. **Finalisation** (`finalize`). Anyone takes a beacon recorded at least `W` after the candidate. Its nonce could
   not be known before then. Someone posts that nonce on LEZ, and a second proof shows `t` other sequencers building
   on blocks that contain it. The vault checks that the second proof echoes exactly that beacon's nonce and that its
   journal is the candidate's word for word apart from `echoNonce` (the digest; so the tip, `n` and `t` are compared
   too, D11). It also checks monotonicity: `importedLeafCount` never decreases along finalised checkpoints, and once
   one is a sunset checkpoint, every later one is a sunset checkpoint with the same count; and `importedLeafCount ≤
   nextLeafIndex` (D11). The candidate then becomes final: its burn root is accepted, and the first sunset checkpoint
   sets the refund boundary.

**Rule (B)** (`mode = B`; the checkpoint image is `checkpoint_b`'s). Its journal is word for word rule (A)'s
candidate journal, but the program has checked that at least `t` keys of the key set signed, through certified
attestation keys, final tips that are `T` or descend from `T`. One proof suffices:
1. `submitCandidate` as above, without a beacon.
2. Wait `W = deltaAtSubmit`: `turnSeconds` is 0 under rule (B), and Δ (default 1 h) is a reaction window against
   `t` stolen keys. A watcher checks on submission that the tip is already in its own node's final tier, and
   cancels the candidate otherwise.
3. `finalizeAttested(journal fields)`, anyone: the fields must be the journal the candidate's proof was verified
   against, word for word (its digest), the wait over, monotonicity and the leaf bound as above, no checkpoint
   freeze and no verifier stop. No second proof and no beacon.

`recordBeacon` and `finalize` revert `WrongMode` under rule (B), `finalizeAttested` under rule (A). Candidates that
are never finalised are harmless, because everything starts from `finalState`. A revocation invalidates every
pending candidate; a finalisation does not. The assumptions are stated below.

### Reject-only powers: watchers and the freeze authority

- **`cancelCandidate(newState)`**, by any watcher or the freeze authority. Deletes a pending (not yet finalised)
  candidate and emits `CandidateCancelled(newState, tipId, tipHash)`. Unknown and already finalised states are
  refused. Finalised checkpoints are untouched, and the same state can be submitted again as a fresh candidate,
  which waits a full `W` from its new submission time: a cancellation costs an honest prover one wait, and the
  watcher must cancel again (D11: not sticky). Rule (A): when the tip is not in an honest node's final tier by
  `submittedAt + Δ_g`, or the key set is not the live one; rule (B): when the tip is not already final.
- **`pauseCheckpoints()`**, by a watcher: stops `submitCandidate` and finalisation until 12 h from now. It never
  shortens a longer freeze, and a watcher cannot lift it; the freeze authority can.
- **The checkpoint freeze** (`setCheckpointsFrozen`, freeze authority) stops both submission and finalisation. It is
  the general pause and lapses like every freeze.
- **`revokeFrom(index)`** (freeze authority), within `revokeWindow` of that checkpoint's finalisation, and only for a
  checkpoint not already revoked. It marks checkpoint `index` and every later one revoked, so their burn roots stop
  counting. It restores `finalState`, and that state's imported leaf count and sunset flag, to those `index` was
  built on. If a revoked checkpoint set the refund boundary, the boundary is cleared. Used nullifiers stay used. It
  accepts nothing: the restored state needs a fresh candidate and a full wait.
- **`retire()`**, one-way: the freeze authority at any time; anyone from `retireOpensAt()` on, so that refunds never
  depend on the freeze key (D13). It stops deposits forever. It cannot touch escrow, checkpoints or releases.
  `retireOpensAt()` is the earlier of two moments (review 2026-11-11 M-1, L-3):
  - **keyless**: 30 days after the first moment a checkpoint could be finalised since the clock last started. The
    clock (`keylessFrom`) starts at deployment and restarts at every candidate submission (a verified proof on
    `finalState`) and every finalisation; the shortest wait `delta + (tMin − 1) × turnSeconds` (today's `delta`) is
    added. A checkpoint freeze or a watcher pause in force moves the start to its end, and a freeze the freeze
    authority lifts or cuts short restarts the clock when it ends. So no time under a freeze or pause counts, a
    watcher cannot run the clock out (pauses stop it; cancelled candidates are resubmitted, which restarts it), and
    operators must submit or finalise a checkpoint at least every 30 days, from deployment on (a heartbeat);
  - **sunset**: the second after the first non-revoked sunset checkpoint stops being revocable
    (`sunsetFinalizedAt + revokeWindow + 1`, when refunds open: D11's first item).

### Refunds

`refund(leafIndex, depositor, token, denomIndex, C, path)` (anyone) returns a deposit LEZ never imported. It needs:
- a retired vault, so the deposit root no longer moves and a refund's path cannot go stale;
- a refund boundary: the first non-revoked sunset checkpoint, whose `importedLeafCount` is the boundary. A later
  sunset checkpoint can neither lower it nor move it;
- `block.timestamp > sunsetFinalizedAt + revokeWindow`: the second after the last one in which the sunset checkpoint
  can be revoked (D5, D11), so that a forged sunset checkpoint can be revoked before any refund;
- no global release freeze in force, and no verifier stop (D13: after a stop nothing more is refunded);
- room in the token's and the global release bucket, on which a refund draws (D13);
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
  releases are delayed; a delay equal to `revokeWindow` opens only the second after the last revocable one (D11);
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

### Fees (v5, D13)

Every fee is a fixed amount per token and denomination, at most 1% of that denomination (a constant ceiling, each
fee on its own), in the token itself:
- the **relay fee**, registered with the token and write-once (as in v4). A deposit escrows it and the claim on LEZ
  pays it to the relayer; a release pays it to its relayer when the proof names it (0 or exactly the registered fee);
- the **mint fee** (protocol), escrowed by a deposit beside the denomination, in whole LEZ units;
- the **burn fee** (protocol), kept from a release and credited to the immutable `feeRecipient`;
- the **checkpoint fee**, kept from a release and credited to the prover of the checkpoint that accepted the root
  the release is proven against: the sender of its candidate (`submitCandidate`, whose proof the vault verified),
  under both rules, not the sender of the finalisation (review 2026-11-11 M-2).

A deposit escrows the denomination plus the mint and relay fees of the schedule in force; its leaf and its `Deposit`
event keep the denomination. A release pays the denomination less the relay fee its proof names and the burn and
checkpoint fees of the schedule in force at the release. Burn and checkpoint fees are credited to `owed` and claimed
with `claimOwed`, never pushed, so no prover can make releases costly. A refund returns the leaf's whole escrow:
the denomination and the mint and relay fees of the schedule of its leaf index. The caps count what moves: a
deposit's total, a release's denomination, a refund's total.

**Schedules.** Registration sets schedule 0: no mint or burn fee (protocol fees start at zero) and the checkpoint
fees given. The admin announces schedule `current + 1` with `announceFees(token, mintFees, burnFees, checkpointFees)`.
It takes effect seven days of Ethereum time later, at the token's first deposit or release from then on or at
anyone's `applyFees(token)`, and applies to the deposits from the leaf count at that moment (`fromLeaf`); a deposit
keeps the schedule of its leaf index. One change can be pending per token: a new announcement replaces it and
restarts the seven days, announcing the fees in force withdraws it, and a change already due is put in force before
a new announcement is recorded. `feeState(token)` and `fees(token, index, denomIndex)` read them.

**Published for the LEZ side.** Slots 81 and 82 hold the fee state and the schedules. The registry entry and every
schedule in force are published as SHA-256 hashes at SHA-256 slots, which LEZ reads with a storage proof as it reads
slot 0: at `sha256("ferrata/vault/registry/v1" ‖ token)` the hash `sha256("ferrata/registry/v1" ‖ token ‖
lezDefinition ‖ decimals:u8 ‖ lezUnit ‖ unitWeight ‖ k:u8 ‖ denominations[k] ‖ relayFees[k])`, and at
`sha256("ferrata/vault/fees/v1" ‖ token ‖ index:u64)` the hash `sha256("ferrata/fees/v1" ‖ token ‖ index:u64 ‖
fromLeaf:u64 ‖ announcedAt:u64 ‖ effectiveAt:u64 ‖ k:u8 ‖ mintFees[k] ‖ burnFees[k] ‖ checkpointFees[k])`, zero until
the schedule is in force (u256 words unless marked). These are the encodings of `lez/bridge-wire` on the branch
`bridge-complete` (a9c676b, work in progress), and the vault refuses what that code refuses: a fee above 1%, mint
and relay fees that are not whole LEZ units, a mint or burn fee in schedule 0, a `lezUnit` above 10^decimals, a
largest denomination whose double does not fit in 128 bits. If the LEZ side changes them, `_publishEntry` and
`_publishSchedule` in `FerrataRegistry` are the one place to follow. `test/Fees.t.sol` pins known answers of both.

### Verifier and stop

`verifier` is immutable. A deployment points it at a `RiscZeroVerifierEmergencyStop` wrapping the Groth16 verifier
and renounces the stop's owner (D13), so that only RISC Zero's own proof of exploit (`estop(receipt)`, a verifying
receipt for the zero claim digest) can trip it. The constructor refuses a verifier that does not answer `paused()`
or is already stopped. A stop is one-way. After it, every call that needs a proof reverts (`submitCandidate`,
`finalize`, `release`), and so do rule (B) finalisations and refunds (D13). Cancellation, `claimOwed`, revocation
and role changes still work. Nothing can change what the vault accepts.

### Freezes

The freeze authority can freeze deposits, releases (globally or per token) and checkpoints; a watcher can pause
checkpoints for at most 12 h. The global release freeze also stops refunds; the checkpoint freeze stops submission
and finalisation. A freeze lasts `maxFreeze`
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
- 35–37: the roles, with `retired` packed beside the freeze authority;
- 38–40: the deposit, release and checkpoint freeze deadlines;
- 41–52: checkpoint state (`finalState`, `delta`, `revocationEpoch`, beacons, candidates, history, burn-root index,
  refund boundary and its finalisation time, and the imported leaf count and sunset flag of `finalState` with the
  keyless-retirement clock `keylessFrom` in one slot);
- 53–64: registry, per-token freezes, delay history, escrow, owed, counters, nullifiers, refunded leaves, per-token
  buckets;
- 65–68 and 69–72: the global buckets;
- 73–75: the watchers; 76–80: the pending role proposal (v4);
- 81–82: the fee state and the fee schedules (v5); the published registry and fee hashes sit at SHA-256 slots
  ("Fees").

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
| **admin** | Register a token once: denominations, relay fees (≤ 1% each), `lezUnit` and `unitWeight` are then fixed. Enable or disable deposits per token. Set caps, a release bucket never below the immutable floor. Set release delays, for roots finalised later, always with `minReleaseDelay ≤ delay ≤ revokeWindow` (and ≤ 30 days). Raise Δ (never below `deltaFloor`, at most 30 days). Propose a watcher set and a new freeze authority, applied 14 days later (`proposeRoles`). Hand its own role on in two steps. | Freeze or unfreeze, cancel, pause, revoke, retire, stop the verifier. Change a registered token's denominations, fees, `lezUnit` or weight. Lower Δ. Set a release bucket below the floor. Change the roles faster than 14 days. Touch escrow. Change the verifier, `mode`, image ids, `lezConfigId`, `genesisState`, `nMin`, `tMin`, `deltaFloor`, `revokeWindow`, `maxFreeze`, `turnSeconds`, `minReleaseDelay` or the floor (all immutable). |
| **watchers** (up to three, D13) | `cancelCandidate` a pending candidate. `pauseCheckpoints()` until 12 h from now. | Anything else: lift a pause, shorten a freeze, freeze anything else, revoke, retire. |
| **freeze authority** (a multisig) | Freeze and unfreeze deposits, releases and checkpoints (each freeze lapses after `maxFreeze` unless renewed). `cancelCandidate`. `revokeFrom` within the window. `retire()` at any time. | Touch the registry, caps, delays, Δ or escrow. Accept anything. Cancel a finalised checkpoint (only revoke it, within the window). Undo a stop or a retirement. Hand itself over: only the admin's proposal changes it. |
| **anyone** | Deposit. `recordBeacon` (rule (A)). Submit and finalise checkpoints (`finalize` with a proof under (A), `finalizeAttested` after the wait under (B)), and submit a cancelled state again. Release with a proof (as relayer). Refund once the rules above allow it. `applyRoles` once a proposal is 14 days old. `retire()` once no checkpoint has been finalised for 30 days. Stop the verifier with a proof of exploit. | |
| **payee** | `claimOwed` what it is owed, to itself. | |

The admin hands its role on in two steps. The freeze authority and the watchers change only through the admin's
proposal, which anyone applies 14 days later (D13: a lost or compromised freeze key is replaced in public). One
address cannot be both admin and freeze authority. The stop's owner is renounced at deployment, so the stop has no
guardian.

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

v5, D13's fees: where the specification leaves a choice (the LEZ side, built at the same time on `bridge-complete`,
has to agree on each):
- **Which fees change by announcement.** The mint and burn fees (protocol fees, zero at registration) and the
  checkpoint fee (set at registration). The relay fee stays write-once.
- **The 1% ceiling** applies to each fee on its own, not to their sum.
- **Units.** Mint and relay fees are whole LEZ units, since LEZ mints them; burn and checkpoint fees may be any base
  unit, since Ethereum keeps them.
- **"Seven days of Ethereum time after they are announced"** is a lower bound: the change takes effect at the token's
  first deposit or release, or anyone's `applyFees`, at or after announcement + 7 days; `fromLeaf` is the leaf count
  then, and the published `effectiveAt` is that block's time.
- **One pending change per token**: a new announcement replaces it and restarts the notice; announcing the fees in
  force withdraws it.
- **A release pays the fees in force at the release**, not those of the root's checkpoint. Rejected: the root's
  schedule, inexact in time under lazy activation and a search on the hot path.
- **"Whoever finalised the root used"** is read as the candidate's prover: the sender of `submitCandidate`, stored
  in the candidate (one more slot, measured in GAS.md) and copied into the checkpoint's history entry. v5 credited
  the sender of `finalize` (A) or `finalizeAttested` (B): under rule (B) that call needs no proof, so anyone took the
  fee without proving, and under (A) a copied seal redirected it (review 2026-11-11 M-2). Still open: copying a
  pending `submitCandidate` from the mempool takes the credit; closing it needs the prover's address in the checkpoint
  journal (a new checkpoint program).
- **Who receives the burn fee**: an immutable `feeRecipient` set at deployment. Burn and checkpoint fees are
  credited, never pushed.
- **Relay fee**: a release's stays 0 or the registered fee (v4); a deposit always escrows it, since the claim on LEZ
  pays it.
- **A refund returns the leaf's whole escrow** (denomination, mint and relay fee of its schedule); the caps count
  escrow movements (a deposit's total, a release's denomination, a refund's total).
- **Schedule 0** starts at the leaf count at registration. **Events**: `Release` is unchanged, fee credits are
  `OwedCredited`, and `Refund.amount` is the total refunded.
- **Encodings and checks** of the registry entry and the schedules: those of `lez/bridge-wire` at a9c676b ("Fees").

Kept from v2:
- **`finalize(beaconIndex, journalFields, seal)`.** The spec's separate `newState` argument is taken from the
  journal fields; rule (B)'s `finalizeAttested(journalFields)` likewise.
- **Release delays as a history.** The delay is stored as a history per token (`fromCheckpoint`, `delay`). It is
  ordered by checkpoint index, which is exact even when a change and a finalisation share a block. Registration
  sets the initial delay.
- **Bounds the spec does not state:** `deltaFloor` non-zero; release delays at most 30 days (on top of D5's
  `≤ revokeWindow`).

Kept from v3:
- **Monotonicity is checked at finalisation only**, as D4 says ("along finalised checkpoints"). A candidate that
  breaks it can be submitted, but never finalised.
- **Only the global release freeze stops refunds**, as D5 words it. A per-token release freeze stops that token's
  releases, not its refunds.
- **Constructor bounds:** `minReleaseDelay ≤ revokeWindow` and `≤ 30 days`; `revokeWindow / deltaFloor ≤ 1,000`, so
  `revokeFrom` marks at most 1,001 checkpoints (two finalisations are at least `deltaFloor` apart; review I-7).
- **Stale candidates** (submitted before a finalisation or a revocation, so built on an earlier `finalState`) are
  dead: they cannot be finalised and a new submission of the same `newState` replaces them (review L-1). A candidate
  records `epoch = revocationEpoch + checkpointCount()` at submission and is live only while that sum is unchanged.
- **`cancelCandidate`** refuses a state that was never submitted and one already finalised alike
  (`UnknownCandidate`). It also deletes a stale candidate, which is harmless. It works under the checkpoint freeze
  and after a verifier stop, since it needs no proof. Every candidate keeps its tip for `CandidateCancelled`.
- The vault checks `n` and `t` against the floors, not that `t = t(n)` (the program's rule).

New in v4:
- **The candidate keeps the digest of its journal**, not its fields, and a finalisation must re-present the journal
  word for word (under (A) apart from `echoNonce`). This is D11's comparison of `t` and the tip, extended to every
  word, and it makes a candidate three storage slots instead of seven.
- **Rule (B) needs `turnSeconds = 0`**, so that one formula, `W = deltaAtSubmit + (t − 1) × turnSeconds`, serves
  both rules; rule (A) keeps `0 < turnSeconds ≤ 30 days`.
- **Nothing is finalised after a verifier stop under rule (B) either** (D13 says only that nothing more is released
  or refunded; under rule (A) the second proof cannot verify anyway).
- **The D11 boundary second only where D11 names it:** refunds, and releases whose delay equals `revokeWindow`, open
  at `finalizedAt + revokeWindow + 1`; a shorter delay still opens at `finalizedAt + delay`.
- **One role proposal, one timelock.** `proposeRoles(watchers[3], freezeAuthority)` (address(0): unchanged) takes
  effect 14 days later, applied by anyone; a new proposal replaces a pending one. D13 gives the 14 days for the
  freeze authority and "a timelock" for the watchers; the watchers matter for safety under rule (A), so they get
  the same 14 days. Two separate proposals cost 1.1 kB of bytecode the vault does not have.
- **The freeze authority keeps `cancelCandidate`** (D13 names the watchers): it is reject-only, weaker than the
  checkpoint freeze it holds anyway, and a backstop when no watcher is set.
- **The freeze authority no longer hands itself over** (v3's two-step `transferFreezeAuthority`). D13's admin
  replacement is the one way it changes; a multisig rotates its own signers. Removed also for bytecode.
- **The floor covers the release buckets only** (per token and global, capacity and refill, in value units; a token's
  bucket is converted at its weight, saturating instead of overflowing). Deposit buckets have no floor: an admin
  who closes deposits stalls no exit.
- **"Release uses the newest root whose release delay has passed"** (D13) is the holder's rule in the wallet
  (`docs/design/bridge.md` §12), not a vault check: `NOut` v2 is the same under every root, and a vault check would
  make a release revert whenever a newer root matured between proving and inclusion.
- **The stop's owner is renounced by the deployment, not checked by the vault**; the vault checks only that the
  stop answers `paused()` and is not tripped.
- **D13's fees are built** as "Fees (v5, D13)" above describes: deposits escrow the mint and relay fees,
  releases pay less the relay, burn and checkpoint fees, the checkpoint fee goes to the candidate's prover (review
  M-2), schedules take effect seven days after announcement. As D13 specifies, a release pays the outbound fees in
  force at the release, so a fee announced after a burn applies to it (review L-2, not fixed:
  `../notes/reviews/security/fixes-2026-11-11.md`).

## Known limits (SPEC-v3 D8, kept) and residual risks

- **A tripped verifier stop strands all escrow except owed balances.** No new checkpoints follow, and nothing more
  is released or refunded (D13).
- **The freeze key is powerful until replaced.** A compromised freeze authority can keep renewing freezes, cancel
  every candidate, revoke honest checkpoints within the window, or retire the vault: a denial of service, not theft.
  The admin replaces it after 14 days (D13). A compromised admin can, after the same 14 days, install its own freeze
  authority and remove the watchers; the public proposal is the warning.
- **A watcher can delay checkpoints** by cancelling every candidate and renewing 12-hour pauses, until the admin
  replaces it 14 days after proposing to. Under rule (A) the watchers are also trusted for safety outside F1–F4.
- **Keyless retirement follows 30 days without checkpoint progress** (after one shortest wait; freezes and pauses do
  not count). A quiet month with no prover, from deployment on, lets anyone retire the vault, which stops deposits
  for good: operators must submit or finalise a checkpoint at least every 30 days. A watcher cannot cause it, but a
  rogue watcher can delay it (renewing pauses, or submitting and cancelling candidates) until the admin replaces it;
  it can already stop checkpoints, and with them refunds, for as long. Anyone with a valid proof can delay it too,
  by submitting a heartbeat candidate (as a finalisation always could).
- **An unrevoked wrong checkpoint older than `revokeWindow` halts the vault for good** (D13): every later candidate
  descends from it.
- **A payee that can never receive** (a contract without `receive`, a permanently blacklisted address) keeps an
  `owed` balance it cannot collect.
- **Retired keys stay counted** until a finalised checkpoint carries the rotation; LEZ's `exit_delay` (about 40 min
  on testnet values) is shorter than `W`.
- **Escrow backing tokens that are never burned before the checkpoint program can no longer follow LEZ stays in the
  vault.** No party can move it.
- **Issuer risk is inherited.** A token can blacklist the vault or switch on transfer fees (USDT has such a switch).
  Rebasing tokens are unsupported.
- **The global cap uses admin-set integer weights, not an oracle**; the floor is in the same value units.
- **The library is a second contract.** `FerrataRegistry` is linked at its deployed address, fixed in the vault's
  bytecode; it has no state, and its functions run in the vault's storage only through the vault's own functions. A
  forge script deploys a linked library itself before the vault, which shifts the deployer's nonces: a v4 deployment
  whose addresses are predicted (the proofs bind the vault's address) must deploy the library first, or from
  another account.
- **Real proofs in two runs only.** `test/real/` verifies real Groth16 proofs of both checkpoint rules and of the
  release program; every other test mocks the verifier. Those proofs are about a stand-in zone built with test keys,
  except `RealBlocks.t.sol`'s: a rule (B) checkpoint over 195 real blocks of LEZ's sequencer, from the lab's devnet
  (one validator, dev-mode leader proofs, one channel key: n = 1), with no burns.

## Deployment (v5): `script/deploy.sh` (rehearsed on a local fork of Sepolia; run on Sepolia by the crown run)

`script/DeployVault.s.sol` deploys a test vault under rule (A) or (B), **one transaction per run**; `script/deploy.sh`
runs it until its record says complete and stops at the first run that fails. Each run reads the record
(`deployments/<chain id>-<mode>.json`, or `FERRATA_OUT`), counts a recorded address only if it has code, and does the
next step:
1. RISC Zero's `RiscZeroGroth16Verifier` (or the one `FERRATA_GROTH16_VERIFIER` names);
2. `RiscZeroVerifierEmergencyStop` around it, owned by the deployer;
3. the library `FerrataRegistry`;
4. `FerrataVault`: the artifact's creation code with its 11 call sites of the library linked to that address (solc's
   placeholder, `__$` + 17 bytes of `keccak256("src/FerrataRegistry.sol:FerrataRegistry")` + `$__`); never
   `new FerrataVault`, which would make forge deploy a library of its own. Before it, the recorded library's runtime
   code must be FerrataRegistry's exactly, since the vault DELEGATECALLs it;
5. the test token;
6. `registerToken` for ETH (0.001, 0.01, 0.1 ETH; relay fee 0.2%, checkpoint fee 0.1%) and 7. for the token (100,
   1,000, 10,000; 0.25% and 0.1%): schedule 0, protocol fees at zero;
8. `renounceOwnership()` of the stop: nobody holds its guardian key (proposal §B5), and only RISC Zero's proof of
   an exploit can trip it;
9. no transaction: the checks (the stop has no owner, is not tripped and wraps the verifier; the vault's verifier is
   the stop; its rule, admin and freeze authority; both registrations, schedule 0, the published registry and fee
   hashes), then `complete: true`.

The record holds every address, the deployer, the rule, the gas rule and margin, and per transaction the node's
estimate and the limit.

**Gas.** Before it sends, a run asks the node for `eth_estimateGas` of that transaction on the chain as the earlier
runs left it (`vm.rpc`), and refuses, sending nothing, unless estimate × `FERRATA_GAS_MARGIN_PCT` / 100 (default 125)
is within the cap of `FERRATA_GAS_RULE`: `eip7825` (the default) caps the limit at 2^24; `eip8037` (Sepolia since
6 October 2026) caps it at the block gas limit and 2^32 − 1, the node's estimate having already refused anything
whose execution gas exceeds 2^24. `FERRATA_TX_GAS_CAP` lowers the cap further. forge then sends the transaction with
`--skip-simulation --gas-estimate-multiplier` at the same margin, so its limit is the node's estimate times the
margin; `deploy.sh` reads the sent limit back from the node and stops if it differs. One transaction per run is what
makes every estimate real: a transaction that depends on the previous one can only be estimated once that one is
mined. On today's Sepolia the creations cost 7.80 M (verifier), 2.98 M (stop), 18.30 M (library), 39.54 M (vault) and
3.34 M (token) gas: `GAS.md`, "Creating the contracts on today's Sepolia"; the receipts of the crown run's deployment
(mode B, registrations included) are in `GAS.md`, "The v5 round trip, live on Sepolia".

**Keys.** Neither the script nor the driver reads one. forge signs with Foundry's own keystore variables
(`ETH_KEYSTORE`, `ETH_PASSWORD`: a keystore file and a file holding its password) or, against a local fork, with
`--unlocked`. The script refuses chain 1, forge's default sender (no key given) and, except on chain 31337, the
public anvil accounts.

```
cd vault && forge build
# Sepolia (the owner's command; the crown run ran it in mode B, e2e/runs/crown-live-1/):
export ETH_KEYSTORE=<keystore file> ETH_PASSWORD=<file holding its password> ETH_FROM=<its address>
FERRATA_MODE=A FERRATA_GAS_RULE=eip8037 FERRATA_RPC_URL=<Sepolia RPC> script/deploy.sh
# the rehearsal on a local anvil fork of Sepolia (no key; nothing reaches Sepolia):
script/fork-rehearsal.sh [<log directory>]
```

Testnet defaults: `FERRATA_DELTA` 600 s, `FERRATA_REVOKE_WINDOW` 1 day, `FERRATA_MAX_FREEZE` 30 days,
`FERRATA_MIN_RELEASE_DELAY` 0, `FERRATA_RELEASE_DELAY` 120 s, a freeze authority nobody holds a key for
(`FERRATA_FREEZE_AUTHORITY`), the deployer as admin and fee recipient (`FERRATA_FEE_RECIPIENT`), no watchers
(`FERRATA_WATCHERS`), caps UNLIMITED. `script/params/sepolia.json` gives both rules' image ids, the release image id,
the LEZ configuration and genesis of the Sepolia run, and the LEZ token definitions, which are zero placeholders:
registration is write-once, so set them before a deployment the LEZ side will read.

**The rehearsal** (`script/fork-rehearsal.sh`, 2026-11-10, on the Modal build box: anvil 1.3.6 forking Sepolia at
block 12,107,296 with Foundry's gas rules, accounts impersonated):

| Step | Node estimate (anvil) | Limit sent (× 1.25) | Gas used |
|---|---:|---:|---:|
| Groth16 verifier | 1,179,986 | 1,474,982 | 1,179,986 |
| emergency stop | 451,756 | 564,695 | 451,756 |
| `FerrataRegistry` | 2,563,131 | 3,203,913 | 2,563,131 |
| `FerrataVault`, rule (A) / rule (B) | 5,681,056 / 5,681,009 | 7,101,320 / 7,101,261 | 5,681,056 / 5,681,009 |
| test token | 508,036 | 635,045 | 508,036 |
| `registerToken` ETH / token | 717,698 / 718,489 | 897,122 / 898,111 | 707,052 / 707,830 |
| `renounceOwnership` of the stop | 30,324 | 37,905 | 25,524 |

- Rule (A), RISC Zero's real verifier, `eip7825`: complete; deposits of 0.001 ETH and 100 tokens escrowed
  1,002,000,000,000,000 wei and 100,250,000 (the denomination and the relay fee; mint fee 0), and the deposit root
  is the reference's.
- A refusal: the same script with `FERRATA_TX_GAS_CAP=100000` refused the first creation (estimate 1,179,986) and sent
  nothing; the deployer's nonce did not move.
- Rule (B), `eip8037`, the stop around an accept-all **test** verifier (`RehearsalAcceptAllVerifier`; no proof exists
  for a vault at a fresh address without a GPU prover): complete; the same deposits; the freeze authority retired the
  vault and submitted a sunset candidate that imported no leaf; `finalizeAttested` 601 s later; after the revoke
  window (86,401 s) both refunds paid the depositor its 1,002,000,000,000,000 wei and 100,250,000 back and left no
  escrow.
- Read-only against the public Sepolia node itself, step 1 without `--broadcast`: estimate 7,795,331, limit
  9,744,163, cap 199,609,187 (the block gas limit).

## Sepolia round trip (rehearsed end to end on forks; the first attempt ran out of gas)

**Which vault.** The rehearsals below and the owner's run of them deploy the v3 vault of the end-to-end fixture,
from the tree of the branch `sepolia-cloud` (that run stopped after deposit 0). In this tree `script/RoundTrip.s.sol`
constructs the v5 vault (the v5 constructor, with that branch's stages); that combination has not been rehearsed. The
v5 vault made the whole round trip on Sepolia through the crown run instead (`script/deploy.sh`, `script/Crown.s.sol`;
[`../e2e/runs/crown-live-1/`](../e2e/runs/crown-live-1/README.md)).

`script/RoundTrip.s.sol` and `script/roundtrip.sh` run the return leg on Sepolia in stages:
RISC Zero's Groth16 verifier, its emergency stop, a test token and the vault are created and both
assets registered; the deposits follow one stage each (after each of the two that LEZ claims, the run
waits for the deposit's block to be finalized and captures what LEZ needs to attest the vault's root);
the candidate is submitted with its real seal, the beacon is recorded after the wait, the finalisation
is proven for that beacon and submitted, and both releases are paid. `script/verify-sepolia.sh`
re-checks the manifest against any endpoint.

**Gas: the node's, never Foundry's.** Sepolia runs Glamsterdam since 6 October 2026, whose state gas
(EIP-8037: 1,530 gas per byte of new code, per new account and per new storage slot) Foundry's EVM does
not price. The first attempt (10 November, [`../e2e/runs/sepolia-live/`](../e2e/runs/sepolia-live/README.md))
sent the verifier's creation with forge's limit of 1,533,981 and ran out of gas; the node asks 7,795,331.
Now every gas limit is the node's own `eth_estimateGas`, taken by forge right before the transaction is
signed (`forge script --skip-simulation --gas-estimate-multiplier 125`, with `--slow`: after the previous
receipt), times 1.25. Before a stage sends anything, [`../e2e/gasplan.py`](../e2e/gasplan.py) plays it on
a fork and has the node price each of its transactions on that state (`eth_estimateGas` with the fork's
state and the block's time as overrides; the node refuses an estimate when execution gas is over the 2^24
cap); a transaction without an estimate, or whose limit is over the block's gas limit, stops the stage with
nothing sent (`gas/<stage>.json` in the run directory is the record). The vault's creation needs 38.5M:
over 2^24, and valid, because the cap binds execution gas only (GAS.md, "On Sepolia since Glamsterdam").

**The proofs bind the deployment.** Both journals carry the chain id and the vault address, and
`lezConfigId` commits to them too, so a proof made for one vault is worthless for another. The vault's
address is the sender's plain CREATE address (SPEC-v3 D2), so the fixture is made for one sender and
one nonce: [`../e2e/runs/sepolia-live-2/`](../e2e/runs/sepolia-live-2/README.md) is for
`0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07` from nonce 1 (verifier, stop, token and vault at nonces 1 to 4;
the vault `0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7`), and its `lezConfigId` is the configuration id of the
LEZ deployment that will claim its deposits (the inbound plan, recorded in `lez-inputs.json`). `deploy` stops
before it broadcasts anything if the sender or its nonce would put a contract anywhere else. For another sender, make a new run
(`e2e/run.sh params <run> 11155111 <sender> <nonce>`, then every step of `e2e/README.md`); the address
is known before deployment, so the candidate and the releases can be proven first.

**The finalisation proof cannot be made in advance.** It echoes the nonce of a beacon recorded after
the wait, which is `keccak256(prevrandao, block number, vault)` of the block that mines `recordBeacon`.
The candidate and release seals of the committed fixture are reused as they are; the finalisation is
proven again for the real beacon.

**One command.** [`../e2e/sepolia-live.sh`](../e2e/sepolia-live.sh) runs the whole round trip unattended, from
the repository root, with its settings in `e2e/sepolia-live.env` (not committed; `e2e/sepolia-live.env.example`
lists them: the endpoint, the sender, the paths of the keystore file and of its password file, the profile of
the GPU app):

```sh
e2e/sepolia-live.sh          # about 1 h 40 min plus the queue for a GPU; ends with ALL CHECKS PASSED and a DONE line
```

Before the first transaction it checks the chain id, the sender's nonce and balance, and that what the
finalisation proof will need forty minutes later is at hand (the recorded guest programs, the GPU app's profile,
the build wrapper or cargo and r0vm, the prover image's wrapper or Docker), and that the sender can pay 65M gas
at twice the gas price of the moment. It then sends the sixteen transactions in their nonce order, each stage
priced by the node first, waits for the finality of each deposit LEZ attests and captures it
(`lez-capture-1/`, `lez-capture-2/`), sleeps through the vault's wait and the release delay, builds and proves
the finalisation for the beacon it recorded, and runs `verify-sepolia.sh`. Everything it prints also goes to
`e2e/runs/sepolia-live-2/sepolia-live.log`. To run it with the laptop switched
off, [`../e2e/sepolia-cloud.sh`](../e2e/sepolia-cloud.sh) runs the same script in the cloud (`e2e/README.md`,
"Running it in the cloud").

It is resumable: started again, it lets a transaction that is still in the mempool land, reads the chain, sees
which stages are complete and continues after the last one. A stage that was interrupted while it was sending
(the deployment, the two releases) is finished with forge's own record of it (`--resume`), never sent anew. A GPU
job that an earlier start left queued or running is waited for, not started a second time. The manifest takes
every receipt from the chain, so it is whole even where forge was stopped before it had saved one. It stops with
a message at the first thing that is not as planned, for instance a nonce that a transaction from outside the run
has moved, and sends nothing after that.

**The stages by hand**, which is what the driver does:

```sh
export SEPOLIA_RPC_URL=<endpoint>
export ETH_FROM=0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07
export ETH_KEYSTORE=<keystore file> ETH_PASSWORD=<file holding its password>   # or ETH_KEYSTORE_ACCOUNT=<name>
export FERRATA_FIXTURE=$PWD/../e2e/runs/sepolia-live-2/fixture.json   # the prepared run, without its finalisation,
                                                                      # which is proven after the beacon

script/roundtrip.sh status       # before the run: the vault is not deployed and the sender's nonce is 1
script/roundtrip.sh deploy       # nonces 1-8: 4 creations, 2 registrations, mint, approve
script/roundtrip.sh deposit0     # nonce 9: 0.001 ETH to LEZ's public test account 0
#   wait until its block is finalized (cast block finalized), then capture for LEZ (the driver's capture()):
#     lez/scripts/fetch-attest-fixture.py --network sepolia --storage <vault>:0=depositRoot(),1=nextLeafIndex(),
#       2=storedLezConfigId() --deposits <vault>:<block of the vault's creation> --updates-from 1410 <run>/lez-capture-1
script/roundtrip.sh deposit1     # nonce 10: 0.001 ETH to LEZ's private test account 0; wait, capture lez-capture-2
script/roundtrip.sh deposit2     # nonce 11: 100 test tokens (leaf 2; it funds the ERC-20 release; LEZ never sees it)
script/roundtrip.sh submit       # nonce 12: the candidate, with its real seal
#   wait W = FERRATA_DELTA + (t - 1) x turnSeconds = 600 s + 2 x 900 s = 40 min   (`status` counts down)
script/roundtrip.sh beacon       # nonce 13; writes beacon.json beside the fixture
#   prove the finalisation for that beacon, from the repository root (e2e/README.md, "Steps"):
#     e2e/run.sh inputs sepolia-live-2 && e2e/run.sh stage-gpu sepolia-live-2
#     the GPU proof of cp-e2e-finalize -> e2e/runs/sepolia-live-2/receipts/cp-e2e-finalize/receipt-succinct.bin
#     e2e/run.sh p254 sepolia-live-2 cp-e2e-finalize; the prover image; e2e/run.sh groth16 sepolia-live-2 cp-e2e-finalize
#     e2e/run.sh fixture sepolia-live-2
script/roundtrip.sh finalize     # nonce 14: the finalisation, with the new seal
#   wait the release delay: FERRATA_RELEASE_DELAY = 120 s
script/roundtrip.sh release      # nonces 15-16: ETH to a fresh recipient, the token with a relayer's fee
script/verify-sepolia.sh ../e2e/runs/sepolia-live-2/manifest.json
```

The key never appears on a command line: forge reads the keystore file and the password file from
Foundry's own variables. On a local fork that impersonates the sender, `FERRATA_UNLOCKED=1` replaces
the keystore.

**Rehearsal.** The stages ran by hand on `anvil --fork-url <a public Sepolia endpoint>` with the sender
impersonated ([`../e2e/runs/fork/`](../e2e/runs/fork/README.md)): 15 transactions, the finalisation proven
after the fork's beacon existed, both releases paid, and `verify-sepolia.sh` passing on the manifest,
including what must be refused (a seal with a changed byte, the candidate's seal offered as the
finalisation, each release replayed). The gas of every transaction is the Foundry figure of GAS.md, to the
unit except where calldata differs. On a fork the wait is passed with `cast rpc evm_increaseTime <seconds>`
and `cast rpc anvil_mine 1`; the beacon there is anvil's random `prevrandao`, not Ethereum's. The driver was
then rehearsed on a second fork from a clean run directory (`FERRATA_FORK=1`;
[`../e2e/runs/fork-driver/`](../e2e/runs/fork-driver/README.md)), with interruptions: see there.

**Source verification** is a separate step, [`script/sourcify.sh`](script/sourcify.sh), and was not run.
`script/sourcify.sh risc0` verifies RISC Zero's two contracts on Sourcify and sends only files that are public
already. `script/sourcify.sh vault` verifies the vault and the test token, which publishes
`src/FerrataVault.sol`, `src/interfaces/IRiscZeroVerifier.sol` and `script/TestToken.sol` of this repository
for good; it runs only with `FERRATA_PUBLISH_SOURCES=yes`.

The testnet configuration (`deploy` only; each from the environment): `FERRATA_DELTA` 600 s as both
`deltaFloor` and the initial `delta`, `turnSeconds` 900 s (the fixture's posting timeframe of 900
one-second slots), `FERRATA_RELEASE_DELAY` 120 s, `FERRATA_MIN_RELEASE_DELAY` 0,
`FERRATA_REVOKE_WINDOW` 1 day, `FERRATA_MAX_FREEZE` 30 days; denominations of 0.001, 0.01 and 0.1 ETH
and 100, 1,000 and 10,000 test tokens; the sender is the admin and the freeze authority is an address
without a key unless `FERRATA_FREEZE_AUTHORITY` names one. The design's defaults (3 h 01 min, 24 h,
7 days) are what `test/real/` uses; the constructor enforces none of them.

## Tests

All tests are in `test/`. The mocks are in `test/mocks/`. `test/utils/ReferenceMerkle.sol` is a plain,
non-incremental restatement of the tree. `test/utils/VaultTestBase.sol` rebuilds both journals word by word from
the table above and drives real candidate → beacon → finalisation sequences with the test committee (n = 4, t = 3,
`turnSeconds` = 900 s, Δ = 4 h, so `W` = 4 h 30 min); rule (B) vaults use `turnSeconds` = 0 and Δ = 1 h.

| File | What it checks |
|---|---|
| `Deposit.t.sol` | ETH, 6- and 18-decimal, USDT-style; fee-on-transfer and false-returning tokens rejected; denomination, index, commitment rules; unregistered and delisted tokens; a full tree refuses both deposit kinds and changes nothing (D9); plain ETH and unknown calls refused |
| `Checkpoint.t.sol` | Beacons (one per block, nonce from prevrandao). Candidate rules. The finality rule: no beacon old enough, too-early beacon, nonce not matching the named beacon, stale parent, competing candidates, a Δ raise applying only to later candidates. The committee-sized wait, refused one second before `W` and accepted at it for t = 2, 3, 5, 7; the wait is the candidate's. Key-set floors at submission and finalisation. The checkpoint freeze stops finalisation until it lapses. Monotonicity: a lower count and a dropped sunset are refused; a later sunset checkpoint cannot lower or move the boundary. `cancelCandidate`: only the freeze authority; unknown or finalised states refused; a cancelled candidate cannot be finalised; resubmission works and restarts the wait; works under the freeze. Δ only up and capped. v4: a finalisation journal must equal the candidate's in every word (D11: the tip, n and t too); a checkpoint claiming more imported leaves than the tree is refused, the bound inclusive (D11); a watcher cancels too. |
| `Revocation.t.sol` | Inside and outside the window. Release against a revoked root reverts. Nullifiers used before a revocation stay used. A re-proven checkpoint is accepted again, but the same nullifier never pays twice. Pending candidates are invalidated. A revocation restores the imported leaf count and the sunset flag. |
| `Refund.t.sol` | Retirement is one-way. Refunds are refused before retirement, before sunset, below the boundary, before `revokeWindow` has passed (refused one second before, paid at it), under the global release freeze (not under a per-token one), twice, with a wrong path, to anyone but the leaf's depositor, for an index of 2^32 or more, and when releases took the escrow. A forged sunset checkpoint revoked inside the window pays nothing, then or later. The first sunset checkpoint fixes the boundary and the opening time. v4: the second in which the sunset checkpoint can still be revoked pays no refund, the next one does, and no second allows both (D11); refunds draw on the token's and the global release bucket (D13); others retire only after 30 days without a finalisation. |
| `Release.t.sol` | Releases of every token kind, delisted tokens, nullifiers, relay fees, the relayer, escrow limits, image-id separation, the always-rejecting verifier, re-entry through recipient and token |
| `ReleaseDelay.t.sol` | Delay changes apply only to later roots (raising and lowering), per-token delays, the registration delay; every delay setting between `minReleaseDelay` and `revokeWindow`; a fuzzed check of the history lookup against a linear scan; v4: a delay equal to `revokeWindow` opens the second after the last revocable one, a shorter delay at its own second (D11) |
| `PullPayment.t.sol` | ETH recipient that rejects then claims; blacklisted ERC-20 recipient; rejecting relayer; 600 kB revert bomb; gas burner; refund to a rejecting depositor; `claimOwed` only by the payee, not re-enterable; one stuck payee blocks nobody |
| `JournalBinding.t.sol` | One test per field of the release journal, and per field of the 17-word checkpoint journal at both `submitCandidate` and `finalize` (a proof differing in exactly that field is rejected, with a positive control in the same test); relayer redirection, fee and replay attacks |
| `EmergencyStop.t.sol` | After a guardian stop, `submitCandidate`, `finalize`, `release` and (v4, D13) `refund` revert, while cancellation, `claimOwed`, revocation, the admin handover and the timelocked role change work; the proof-of-exploit stop; no undo |
| `Freeze.t.sol` | All 32 combinations of the five freezes; every freeze lapses after `maxFreeze`; renewal; a lost key cannot lock the vault |
| `Admin.t.sol` | Constructor checks, including `turnSeconds` and `minReleaseDelay`; the v3 domain tag; write-once registry; relay-fee ceiling (fuzzed); role separation; two-step handovers (v4: the admin's own; the freeze authority changes only through `proposeRoles`) |
| `Caps.t.sol`, `Merkle.t.sol`, `Privacy.t.sol`, `TreeVectors.t.sol`, `GasScenarios.t.sol` | Rate limits; slots 0, 1 and 2 and tree algebra; LEZ-side data never appears in calldata, events or storage writes; vectors; gas |
| `Fees.t.sol` (v5) | D13's fees, 22 tests: registration publishes the registry entry and schedule 0 at their slots (known answers, recomputed byte by byte) and emits them; the fee rules (1% ceiling, LEZ units, schedule 0 without mint or burn fee, twice the largest denomination in 128 bits); `announceFees` only by the admin and only for a registered token (fuzzed ceiling); a change in force seven days after its announcement and not a second earlier, a deposit after it sending the new total, a new announcement replacing the pending one, announcing the fees in force withdrawing it, a due change taking effect before the next announcement; `applyFees` permissionless, acting only when due; a refund returning the escrow of its leaf's schedule; releases paying the denomination less the relay, burn and checkpoint fees, with and without a relay fee, putting a due change in force, crediting nothing for zero fees, the checkpoint fee credited to the candidate's prover and never pushed, under rule (B) to the prover and not to the sender of `finalizeAttested` (review 2026-11-11 M-2); deposit buckets counting the whole escrow; fuzzed fee arithmetic |
| `invariant/` | 14 invariants over 256 runs × 100 calls, on a rule (A) vault (`VaultInvariants`) and on a rule (B) vault (`VaultInvariantsB`), listed below |
| `ModeB.t.sol` (v4) | Rule (B): the constructor (turnSeconds 0 under B, non-zero under A); a candidate records no beacon and keeps its journal digest; final at its wait and not a second before, whatever its t; every journal word (the 11 the caller can change, and echoNonce) must be the candidate's; `recordBeacon` and `finalize` are rule (A) only, `finalizeAttested` rule (B) only; a watcher cancels within the wait and the same proof waits again in full; revocation, a competing finalisation, a pending duplicate, the checkpoint freeze, the verifier stop; monotonicity and the leaf bound; a Δ raise only for later candidates; the program's image id pinned; a release against a rule (B) root |
| `Roles.t.sol` (v4) | Watchers: a 12-hour pause that never shortens a freeze and that the freeze authority can lift, renewable only from now, no other power, every slot counts, nobody else pauses. The admin's role change: admin only, applied by anyone 14 days later and not a second earlier, the old roles holding until then, nothing to apply, address(0) keeping the freeze authority, an empty set removing the watchers, a new proposal restarting the clock, the admin never becoming freeze authority. Keyless retirement 30 days after deployment or the last finalisation, the freeze authority at any time. The release floor: the deployment's global bucket, the global and the per-token bucket in value units, saturation, registration, a fuzz test against hand-worked thresholds; deposits not floored. A verifier that is not an untripped stop refused; a renounced stop owner cannot stop it, a proof of exploit still can |
| `SecurityReview.t.sol` | The vault findings of the security review of 11 November 2026, 12 tests that fail on `integration4`'s vault (1bb20d5) and pass here: keyless retirement counted from the first finalisable moment, not shortened by a checkpoint freeze (lifted early or run to its end), not caused by one watcher pausing or cancelling (M-1); the checkpoint fee to the candidate's prover under both rules, nothing to a copied or proof-less finalisation (M-2); a stale candidate replaced (L-1); no deposit after a finalised sunset checkpoint, anyone retiring once it is irrevocable (L-3); the constructor's bound on `revokeWindow / deltaFloor` (I-7); the deployment script refusing mainnets (I-2) |
| `real/ReturnLeg.t.sol` | No mock: RISC Zero's Groth16 verifier behind its emergency stop, the vault at the addresses the proofs bind, the design's default timers. Before any proof: the checkpoint program's two journals and the release program's two journals are byte for byte the vault's encodings, and the deposit root is the one the bridge core computed. The run: deposits, `submitCandidate` with the real checkpoint seal, the wait, `recordBeacon`, `finalize` with the seal that echoes that beacon, the release delay, `release` of ETH and of the token to recipients that held nothing. Refused: one flipped byte in each of the four seals (selector and each proof word), a valid seal of another statement, each journal field changed under its real seal (candidate, finalisation, release), a replayed release, a release against a root that is not finalised or still delayed, a finalisation one second early with the right nonce, any proof after the stop. `ReturnLegGas` measures every step. v4 runs it unchanged except where v4 refuses earlier (a finalisation journal with another tip, n or t is refused by the vault before the proof, D11). |
| `real/RuleBReal.t.sol` (v4) | No mock, rule (B): the real `checkpoint_b` proof of `gpu-prover/results/ruleb-l4b/` wrapped to Groth16 (fixture `test/real/fixtures/rule-b.json`), RISC Zero's verifier behind its stop, a rule (B) vault constructed at the address the journal binds on chain id 1. Accepted; its journal digest kept; final after one hour and not a second earlier; refused with one flipped byte in the selector or any proof word, with each journal field changed, and by a rule (A) vault (rule (A)'s image id); a watcher cancels it. `RuleBRealGas` measures it |
| `real/RealBlocks.t.sol` (v5) | No mock, rule (B), **real LEZ blocks**: the `checkpoint_b` proof of blocks 8..=202 of the lab's LEZ v0.4.0-rc1 sequencer chain (one validator, dev-mode leader proofs), tip 202 attested by a key the lab's channel key certified (n = 1, t = 1), proven on one NVIDIA L4 (`gpu-prover/results/realblocks-l4/`) and wrapped to Groth16 (fixture `test/real/fixtures/real-blocks.json`). The vault's genesis state is recomputed from the lab channel's key and equals the proof's parent. Accepted; final after one hour and not at once or a second earlier; refused with one flipped byte in the selector or any proof word, with another tip, tip id, state or `n`, by vaults of another key set (`ParentNotFinal`), with larger floors (`WrongCommitteeFloor`) and of rule (A). `RealBlocksGas`: 347,832 + 195,149 gas (`checkpoint/RESULTS.md`, "Real LEZ blocks") |

The invariant handler deposits, submits candidates with committees of four sizes (n, t) = (3, 2), (4, 3), (7, 5),
(10, 7) and finalises them after their own `W`, submits candidates that break monotonicity and cancels candidates,
then tries to finalise both kinds after their full wait with valid proofs, and the pending one before its wait.
It finalises under the vault's rule (`finalizeAttested` under rule (B)). It also revokes, retires, refunds,
releases (some to payees that cannot receive), claims, replays nullifiers and refunds, donates, moves time, and
changes freezes, caps and delays; watchers pause and cancel, and the admin's role proposals are applied on time
and early; since v5 it also announces and applies fees, and models the LEZ side's supply (claims of settled
deposits, burns, releases of burns). Its 14 invariants:
- `balance == escrowed + totalOwed + donations` (an equality, so value stranded in the vault fails it too), and
  `escrowed == deposits − releases − refunds`, for every token;
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
- the leaf count, root and slot 2 match the reference;
- (v4) the freeze authority and the watchers are the handler's model, changed only by a proposal applied after its
  14 days, and `checkpointsFrozenUntil` is the model's (watcher pauses reach 12 h ahead and never shorten a freeze);
- (v4) no candidate is finalised before its wait, and under rule (B) no beacon is ever recorded;
- (v5) supply: for every token, the wrapped supply on LEZ (the handler's model: a claim mints what the leaf's attested
  schedule and the registered relay fee say, a burn takes one denomination) equals the escrow less the deposits not
  yet minted and the burns not yet released;
- (v5) every deposit escrowed, every release paid and credited, every refund returned exactly what the fee model
  says; every published schedule hash is the model's preimage; no claimed deposit is refunded.

Results of the final v5 runs, on 2026-11-10, on the Modal build box (`spike/modal-build/mrun`), forge 1.3.6, solc
0.8.26:

```
forge build --sizes           FerrataVault runtime 24,242 bytes (334 under EIP-170); FerrataRegistry 11,606
forge fmt --check             ok
forge test                    Ran 30 test suites: 395 tests passed, 0 failed, 0 skipped (395 total tests)
                              (including the 14 invariants twice and the 21 tests of test/real/ with real proofs)

forge coverage --report summary      (all 395 tests)
| File                    | % Lines           | % Statements      | % Branches        | % Funcs          |
| src/FerrataRegistry.sol | 100.00% (227/227) | 100.00% (316/316) | 100.00% (41/41)   | 100.00% (31/31)  |
| src/FerrataVault.sol    | 100.00% (426/426) | 100.00% (522/522) | 100.00% (102/102) | 100.00% (80/80)  |

tools/mutation_spotcheck.py   in eight slices of 28 (the last 27): 223/223 killed, every one by the unit suites
                              (the invariant campaign was never needed)
```

On branch `integration` (the same `src/`, plus `test/RegistryStorage.t.sol`, which exports the
vault's own storage for the LEZ side's test, `../lez/BRIDGE-RESULTS.md` §8a), measured on its build
box: `forge test` 31 suites, 396 tests passed; coverage of both files as above; the mutation run
223/223 killed again; `forge fmt --check` ok. `forge build --sizes` gives the same sizes but exits 1,
because a test contract (`VaultHandler`, 37,928 bytes) is above EIP-170, which forge does not enforce
in tests. On branches `integration3` and `integration4` (the same `src/`, plus `test/real/RealBlocks.t.sol`,
the real-block checkpoint's 10 tests, and `test/PrivacySuite.t.sol`, the privacy suite's Ethereum half):
`forge test` 35 suites, 407 tests passed, 31 of them in `test/real/`; coverage of both files as above
(`../docs/evidence-suites.txt`). With the fixes of the security review of 11 November 2026 (branch
`secfix-vault`, on the build box): `forge test` 36 suites, 421 tests passed, 1 skipped (`PrivacySuite`, which needs
its LEZ half's output), 0 failed; the 12 tests of `test/SecurityReview.t.sol` fail on `integration4`'s vault;
`forge coverage` 100% of lines, statements, branches and functions of both files (vault 431, 537, 102, 82; library
245, 338, 45, 33); `tools/mutation_spotcheck.py` 254/254 mutants killed, every one by the unit suites (the tool now
lets the privacy suite skip in its workspaces; before, it failed there and would have hidden a survivor). On
`integration5`, the merged tree (the same `src/`, build box, 11 November 2026): `forge test` 38 suites, 433 passed,
1 skipped (`PrivacySuite`, run by `scripts/privacy/run.sh`), 43 of them in `test/real/` (10 suites); invariants 28
passed; coverage 100% as above; FerrataVault runtime 24,254 bytes (322 under EIP-170), FerrataRegistry 12,504
(`../docs/evidence-suites.txt`).

The deployment and rehearsal scripts (`script/DeployVault.s.sol`, `script/ForkRehearsal.s.sol`) are not run by
`forge test`; `forge coverage` lists them at 0%. `script/fork-rehearsal.sh` runs every step of the first under both
rules, its refusal, and every function of the second ("Deployment").

v4's final runs: 364 tests, 100% coverage of the vault and its library, 175/175 mutants.
v3's final runs (same commands, before v4): 286 tests on the laptop, then 301 with `test/real/` on the box, 100%
coverage of the vault, 127/127 mutants. In v4 the invariant handler donates ETH by self-destruct instead of
`vm.deal`: a balance set with `vm.deal` was lost once a later call made the vault DELEGATECALL its library (forge
1.3.6, seen in a failing run's trace). Test contracts are not held to EIP-170 (the handler is 30,261 bytes).

Fuzz tests use 256 runs. Invariants use 256 runs × 100 calls each. Handler calls with nothing to do are discarded,
so the run table counts exactly what happened. Averaged over the ten campaigns of the final run, one campaign has
about 1,050 deposits, 1,680 candidates, 670 finalisations, 470 candidates that break monotonicity, 200
cancellations, 280 refused finalisations of those two kinds, 365 revocations, 140 releases and 17 refunds (174 over
the ten campaigns); the vault is retired in about 160 of the 256 runs. Refunds stay rare because one 100-call run
must retire the vault, finalise a sunset checkpoint and wait `revokeWindow` first. v2's README counted handler calls
that did not revert, including calls that had nothing to do, so its figures (1,500 finalisations, 900
revocations, 640 releases, 350 refunds per campaign) overstated what happened.

Coverage shows only that code ran. The mutation spot-check plants 223 hand-written one-line bugs in a scratch copy
of the vault and its library (v3's 127, 30 of them rewritten where v4 moved or changed the text, and 48 for v4: the
two rules, the journal digest, D11, watchers, role proposals, keyless retirement, the floor, refunds on the release
buckets and the stop; v5 retargets 14 of those where the code moved into the library and adds 48 for the fees:
schedules, notice, replacement, ceilings and units, the escrowed and paid amounts, the credits, the publication's
slots and hashes, refunds by schedule), across deposits, the tree, the registry and delay bounds, the constructor, candidates, the
committee checks and the wait, beacons, finalisation and monotonicity, the journal layout, cancellation,
revocation, refunds and their opening rules, releases, pull payments, freezes, roles and caps. Each is caught by
named failing tests; for example, dropping the release-freeze check from `refund` is caught by
`test_refund_refusedUnderTheReleaseFreeze`. It is not an exhaustive mutation run.
