# FerrataVault v2: the Ethereum half of a private lock-and-mint bridge (spike)

`src/FerrataVault.sol` is the Ethereum vault of a privacy-preserving lock-and-mint bridge to LEZ (Logos RFP-022). It
implements `notes/design/SPEC-v2.md` (in the parent repository).

**Inbound.** Users deposit fixed denominations of ETH or a registered ERC-20, with a hiding commitment to their LEZ
destination. Each deposit becomes a leaf of a SHA-256 Merkle tree, and its root sits in storage slot 0, where the
LEZ side reads it with an Ethereum storage proof.

**Outbound.** The vault follows LEZ through proven checkpoints. A checkpoint is first a candidate, then must wait
at least Δ, and is then finalised with a second proof that echoes an Ethereum beacon. Releases pay out against a
RISC Zero proof of a burn under a finalised, unrevoked burn root.

Status: prototype evidence for a grant proposal. **Not audited, not deployed anywhere.** Proofs are mocked: the
tests' base verifier accepts exactly the (image id, journal digest) pairs a test registers. It sits behind RISC
Zero's unmodified `RiscZeroVerifierEmergencyStop`, as a deployment's would. The commitment and nullifier
derivations, the LEZ programs and the zkVM guests are out of scope.

v1 (the first review round's vault) is the commit tagged `v1`.

## Running it

```sh
export PATH="$HOME/.foundry/bin:$PATH"
# Dependencies (lib/ is not committed):
forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.0.0
forge build                                              # no compiler warnings, no lint findings
forge test                                               # everything, including the invariant campaign (about a minute)
forge test --isolate --match-contract GasScenarios -vv   # the numbers in GAS.md
forge test --gas-report --no-match-path 'test/invariant/*' > gas-report.txt
forge coverage --report summary                          # about 3 minutes
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

The vault takes the vendored one-function `IRiscZeroVerifier`, whose selector is the same as RISC Zero's.

## How it works

### Deposits

`deposit(token, denomIndex, C, index)` takes an ERC-20; `depositETH(denomIndex, C, index)` takes ETH, whose token
is `address(0)`.
- The amount is the registered denomination at `denomIndex`.
- `index` must equal `depositCount[msg.sender]`.
- `C` must be non-zero, is opaque to the vault, and may repeat.
- ERC-20s are pulled with `safeTransferFrom`, which works for tokens like USDT that return nothing. The vault's
  balance must then grow by exactly the amount, which rejects fee-on-transfer tokens.
- The leaf binds the depositor (`msg.sender`).
- After `retire()`, deposits revert forever.

### LEZ checkpoints: the finality rule in plain words

The vault keeps `finalState`, the latest finalised LEZ checkpoint state, which starts at the immutable
`genesisState`. Moving it forward takes three steps.

1. **Candidate.** Anyone submits a checkpoint proof whose `parentState` is `finalState`, whose `echoNonce` is 0,
   and which states the deployment's `nMin`/`tMin`. The program behind that proof shows that at least `t`
   sequencers, other than the one that produced the new tip, built on that tip. The vault records the candidate,
   the time, and the current Δ (`deltaAtSubmit`). It also records a beacon: the block time plus
   `keccak256(prevrandao, block.number, vault)`.
2. **Wait Δ.** Nothing can happen to the candidate until Ethereum time has moved Δ past its submission.
3. **Finalisation.** Anyone takes a beacon recorded at least Δ after the candidate. Its nonce could not be known
   before then. Someone posts that nonce on LEZ, and a second proof shows `t` other sequencers building on blocks
   that contain it. The vault checks that the second proof echoes exactly that beacon's nonce and carries the
   candidate's fields. It then makes the candidate final: its burn root becomes accepted, and a sunset checkpoint
   sets the refund boundary.

Unless at least `t` keys are compromised, the first proof shows that an honest sequencer re-executed the tip. The
second shows that an honest sequencer was still building on it Δ of Ethereum time later. Candidates that are never
finalised are harmless, because everything starts from `finalState`. The assumptions are those of spec §4.6.

### Revocation, retirement, refunds (reject-only)

- **`revokeFrom(index)`** (freeze authority). It must be called within `revokeWindow` of that checkpoint's
  finalisation. It marks checkpoint `index` and every later one revoked, so their burn roots stop counting. It
  restores `finalState` to the state `index` was built on, and clears the refund boundary if a revoked checkpoint
  set it. Used nullifiers stay used. It accepts nothing: the restored state needs a fresh candidate and a full Δ.
  Pending candidates submitted before a revocation can never be finalised.
- **`retire()`** (freeze authority, one-way). It stops deposits forever. It cannot touch escrow, checkpoints or
  releases.
- **`refund(leafIndex, depositor, token, denomIndex, C, path)`** (anyone). It works once a finalised checkpoint
  carries `sunset`, for leaves at or above that checkpoint's `importedLeafCount`. Those deposits LEZ never imported.
  The vault rebuilds the leaf and checks its 32-step path against the current `depositRoot`, then pays the
  depositor named in the leaf, once. Retire before sunset: open deposits keep changing the root, which makes
  in-flight refund paths stale.

### Releases and pull payments

`release(r, seal)` takes the request `{token, denomIndex, recipient, relayer, relayFee, burnRoot, nullifier}`. It
requires:
- a burn root that is accepted, meaning finalised and not revoked;
- the token's release delay elapsed since that root's finalisation. The delay is the one in force when the root
  was finalised: a change applies only to roots finalised afterwards;
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
later call that needs a proof reverts: `submitCandidate`, `finalize` and `release`. Refunds, `claimOwed`, revocation
and role handovers still work. Nothing can change what the vault accepts.

### Freezes

The freeze authority can freeze deposits, releases (globally or per token) and candidate submission. A freeze lasts
`maxFreeze` (immutable) unless renewed; freezing again restarts it, and unfreezing ends it at once. A lost freeze key
therefore cannot lock the vault for good.

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
- 39–41: the three global freeze deadlines;
- 42–51: checkpoint state (`finalState`, `delta`, `revocationEpoch`, beacons, candidates, history, burn-root index,
  refund boundary);
- 52–63: registry, per-token freezes, delay history, escrow, owed, counters, nullifiers, refunded leaves, per-token
  buckets;
- 64–67 and 68–71: the global buckets.

## Encodings

All hashes are SHA-256 unless stated otherwise. `||` is concatenation; integers are big-endian.

```
LEAF_TAG          = sha256("ferrata/vault/leaf/v2")       = 0x17b9508a0d0cbf61207c0fcb66f06ed92a6aeec110a54860479f0bf3cd6aff0b
leaf              = sha256(0x00 || LEAF_TAG || leafIndex:u64 || depositor:20 || token:20 || amount:u256 || C:32)   (145 bytes)
node              = sha256(0x01 || left || right); empty leaf = 32 zero bytes; zero[h+1] = node(zero[h], zero[h])
empty-tree root   = zero[32] = 0x782d35b1fdad7d54e7a1b36a2ab1021e872c7692bb80fdd12bfc321e9e420409
CHECKPOINT_DOMAIN = sha256("ferrata/vault/checkpoint/v2") = 0xe99926553e520475aa7b6407f28efcca58fa91851d8476395f692eb871457ce1
RELEASE_DOMAIN    = sha256("ferrata/vault/release/v2")    = 0x6e04f4b1fa66535e182a1ef2e4f5bfdfe44f7257f1663ef81ffa4f38d7696e51
beacon nonce      = keccak256(abi.encode(block.prevrandao, block.number, vault))
```

Both journals are `abi.encode` of static values, one 32-byte word each, and the vault passes `sha256(journal)` to
`verify(seal, imageId, digest)`:

| Journal | Image | Words, in order |
|---|---|---|
| Checkpoint (spec §4.2.5) | `checkpointImageId` | `CHECKPOINT_DOMAIN`, `block.chainid`, vault, `lezConfigId`, `nMin`, `tMin`, `parentState`, `newState`, `burnRoot`, `burnCount`, `importedLeafCount`, `sunset` (0/1), `echoNonce` (13 words) |
| Release (spec §5.2) | `releaseImageId` | `RELEASE_DOMAIN`, `block.chainid`, vault, `lezConfigId`, `burnRoot`, `NOut`, `token`, `amount`, `recipient`, `relayer`, `relayFee` (11 words) |

- A candidate's checkpoint journal has `echoNonce = 0`; a finalisation's has the named beacon's nonce.
- `amount` is the registered denomination at `denomIndex`.
- `relayer` is bound as submitted, so `address(0)` means "whoever submits", which can be front-run.
- `test/vectors/tree-v2.json` holds 8 concrete deposits from three depositors. For each it gives the leaf
  preimage, the leaf, the root after insertion and the Merkle path, plus the 33 zero hashes. It is generated from
  the vault and rechecked independently by `tools/check_tree_vectors.py`.

## Roles

| | Can | Cannot |
|---|---|---|
| **admin** | Register a token once: denominations, relay fees (≤ 1% each), `lezUnit` and `unitWeight` are then fixed. Enable or disable deposits per token. Set caps. Set release delays (≤ 30 days, for roots finalised later). Raise Δ (never below `deltaFloor`, at most 30 days). Hand the role on in two steps. | Freeze or unfreeze, revoke, retire, stop the verifier. Change a registered token's denominations, fees, `lezUnit` or weight. Lower Δ. Touch escrow. Change the verifier, image ids, `lezConfigId`, `genesisState`, `nMin`, `tMin`, `deltaFloor`, `revokeWindow` or `maxFreeze` (all immutable). |
| **freeze authority** | Freeze and unfreeze (each freeze lapses after `maxFreeze` unless renewed). `revokeFrom` within the window. `retire()`. As the stop's guardian, stop the verifier. Hand the role on in two steps. | Touch the registry, caps, delays, Δ or escrow. Accept anything. Undo a stop or a retirement. |
| **anyone** | Deposit. `recordBeacon`. Submit and finalise checkpoints with proofs. Release with a proof (as relayer). Refund after sunset. Stop the verifier with a proof of exploit. | |
| **payee** | `claimOwed` what it is owed, to itself. | |

Each role is handed on only by its current holder. One address cannot hold both roles. The stop's guardian is the
wrapper's own `Ownable2Step` owner, so it has to be handed over separately.

## Where I interpreted or departed from the spec

- **Revocation invalidates pending candidates** (`revocationEpoch`). This is my reading of §4.4's "must again
  satisfy §4.3". Otherwise a revocation would revive candidates whose Δ had already run out, the revoked
  candidate's state included, for instant re-finalisation. Finalisation itself does not invalidate candidates
  (§4.3's parent check already does), so a provable no-op checkpoint cannot be used to make honest candidates
  restart.
- **`finalize` also requires the deployed `nMin`/`tMin`.** §4.3 lists this check for candidates only.
- **`revokeFrom` refuses an index that is already revoked.** Re-revoking it would restore a revoked state.
- **Δ ceiling.** `raiseDelta` is capped at 30 days. This is not in the spec. Without it, a raise-only Δ could be
  pushed out of reach for good, and `submittedAt + Δ` could overflow. `deltaFloor` must be non-zero.
- **`finalize(beaconIndex, journalFields, seal)`.** The spec's separate `newState` argument is taken from the
  journal fields.
- **Release delays as a history.** The delay is stored as a history per token (`fromCheckpoint`, `delay`). It is
  ordered by checkpoint index, which is exact even when a change and a finalisation share a block. Registration
  sets the initial delay.
- **Sunset boundary, literally.** Every sunset checkpoint sets the refund boundary, and revoking the one that set it
  last clears it (even if an earlier valid one exists: refunds wait for the next sunset checkpoint).
- **Spec as written: freezes do not cover `finalize`, refunds or claims.** The candidate freeze stops submissions
  only. Refunds are not capped, frozen or delayed.

## Residual risks, stated plainly

- **Escrow backing tokens that are never burned before the checkpoint program can no longer follow LEZ stays in the
  vault.** No party can move it. Nobody can release it, and deposits LEZ already imported cannot be refunded.
- **A stopped verifier means nothing more is released from this vault.** No new checkpoints follow either. If no
  sunset checkpoint was finalised before the stop, no refunds become possible.
- **`t` compromised keys** can sign a fabricated LEZ history. It still has to be published as a candidate and
  wait Δ; the defences are monitoring that window, the freeze, revocation within `revokeWindow`, release delays and
  caps.
- **The freeze key is powerful.** A compromised freeze authority can keep renewing freezes, revoke honest
  checkpoints within the window, or retire the vault: a denial of service, not theft. The admin can stall exits by
  setting caps to 0.
- **Payouts can still be lost.** A payee that can never receive (a contract without `receive`, a permanently
  blacklisted address) keeps an `owed` balance it cannot collect.
- **Issuer risk is inherited.** A token can blacklist the vault or switch on transfer fees (USDT has such a switch).
  Rebasing tokens are unsupported.
- **The global cap uses admin-set integer weights, not an oracle.**
- **No real proofs.** All verification here is mocked. The Groth16 verifier's gas and behaviour are not measured.

## Tests

All tests are in `test/`. The mocks are in `test/mocks/`. `test/utils/ReferenceMerkle.sol` is a plain,
non-incremental restatement of the tree. `test/utils/VaultTestBase.sol` rebuilds both journals word by word from
the table above and drives real candidate → beacon → finalisation sequences.

| File | What it checks |
|---|---|
| `Deposit.t.sol` | ETH, 6- and 18-decimal, USDT-style; fee-on-transfer and false-returning tokens rejected; denomination, index, commitment rules; unregistered and delisted tokens; full tree; plain ETH and unknown calls refused |
| `Checkpoint.t.sol` | Beacons (one per block, nonce from prevrandao). Candidate rules. The finality rule: no beacon old enough, too-early beacon, nonce not matching the named beacon, stale parent, competing candidates (the loser never finalises, even after a revocation), a Δ raise applying only to later candidates. A no-op checkpoint doesn't stale others. Sunset boundary. Δ only up and capped. |
| `Revocation.t.sol` | Inside and outside the window. Release against a revoked root reverts. Nullifiers used before a revocation stay used. A re-proven checkpoint is accepted again, but the same nullifier never pays twice. Pending candidates are invalidated. |
| `Refund.t.sol` | Retirement is one-way. Refunds are impossible before sunset, below the boundary, twice, with a wrong path, or to anyone but the leaf's depositor. Refunds reduce escrow. Revoking the sunset checkpoint disables refunds. Paths are against the current root. |
| `Release.t.sol` | Releases of every token kind, delisted tokens, nullifiers, relay fees, the relayer, escrow limits, image-id separation, the always-rejecting verifier, re-entry through recipient and token |
| `ReleaseDelay.t.sol` | Delay changes apply only to later roots (raising and lowering), per-token delays, the registration delay; a fuzzed check of the history lookup against a linear scan |
| `PullPayment.t.sol` | ETH recipient that rejects then claims; blacklisted ERC-20 recipient; rejecting relayer; 600 kB revert bomb; gas burner; refund to a rejecting depositor; `claimOwed` only by the payee, not re-enterable; one stuck payee blocks nobody |
| `JournalBinding.t.sol` | One test per field of the release journal, and per field of the checkpoint journal at both `submitCandidate` and `finalize` (a proof differing in exactly that field is rejected, with a positive control in the same test); relayer redirection, fee and replay attacks |
| `EmergencyStop.t.sol` | After a guardian stop, `submitCandidate`, `finalize` and `release` revert, while refund, `claimOwed`, revocation and both role handovers work; the proof-of-exploit stop; no undo |
| `Freeze.t.sol` | All 32 combinations of the five freezes; every freeze lapses after `maxFreeze`; renewal; a lost key cannot lock the vault |
| `Admin.t.sol` | Constructor checks; write-once registry; relay-fee ceiling (fuzzed); role separation; two-step handovers |
| `Caps.t.sol`, `Merkle.t.sol`, `Privacy.t.sol`, `TreeVectors.t.sol`, `GasScenarios.t.sol` | Rate limits; slots 0, 1 and 2 and tree algebra; LEZ-side data never appears in calldata, events or storage writes; vectors; gas |
| `invariant/` | 7 invariants over 256 runs × 100 calls, listed below |

The invariant handler also finalises, revokes, retires, refunds and claims. Its 7 invariants:
- `balance ≥ escrowed + totalOwed`, and `escrowed == deposits − releases − refunds`, for every token;
- `totalOwed` equals the sum over all payees;
- a used nullifier or refunded leaf is never paid again;
- `finalState` is genesis or a state that went through `finalize`, and it is the newest non-revoked checkpoint's
  state;
- the leaf count, root and slot 2 match the reference.

Results of the final runs, on 2026-11-10:

```
forge build --force    Compiler run successful!   (no compiler warnings, no lint findings)
forge test             Ran 21 test suites in 28.89s (111.90s CPU time): 248 tests passed, 0 failed, 0 skipped (248 total tests)

forge coverage --report summary      (all 248 tests, including the invariant campaign)
| File                 | % Lines           | % Statements      | % Branches        | % Funcs         |
| src/FerrataVault.sol | 100.00% (420/420) | 100.00% (533/533) | 100.00% (100/100) | 100.00% (77/77) |
| Total (incl. tests)  | 96.24% (845/878)  | 96.80% (998/1031) | 96.62% (143/148)  | 94.35% (167/177)|

tools/check_tree_vectors.py   ok: 8 leaves, 33 zero hashes, roots and paths match
tools/mutation_spotcheck.py   92/92 mutants killed
```

Fuzz tests use 256 runs. Invariants use 256 runs × 100 calls each. In one invariant's campaign, about 1,300
deposits, 1,500 finalisations, 900 revocations, 640 releases and 350 refunds succeed, and replays stopped by the
nullifier check appear in every sampled run.

Coverage shows only that code ran. The mutation spot-check plants 92 hand-written one-line bugs in a scratch copy
of the contract, across deposits, the tree, the registry, candidates and beacons, finalisation, revocation, refunds,
releases, pull payments, freezes, roles and caps. Each is caught by named failing tests; for example, the
return-data-copying ETH `call` is caught by the 600 kB revert bomb. It is not an exhaustive mutation run.
