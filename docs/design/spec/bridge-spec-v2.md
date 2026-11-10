# Ferrata bridge: specification v2

9 Nov 2026. Supersedes the encodings of the first design and the v1 vault. Where this file and older text disagree, this file wins. The changes made after it, including the correction of the finality claim in §4.6, are in [bridge-spec-v3-changes.md](bridge-spec-v3-changes.md), which wins where the two differ.

All hashes are SHA-256. `‖` is byte concatenation. Integers are big-endian at the stated width. `HMAC` is HMAC-SHA-256. Tags are the ASCII bytes of the quoted string. "abi.encode" means Solidity's standard encoding of static types (32-byte words).

## 1. Identities and pairing

- `evmChainId`: `uint256` (32 bytes in every preimage).
- `vault`: 20-byte address.
- `channelId`: 32 bytes, the zone's Logos Blockchain channel id. `bedrockGenesisId`: 32 bytes.
- `bridgeHeader`: 32 bytes, the bridge program's header account id on LEZ. `bridgeImageId`: 32 bytes.
- `attestationRootId`: 32 bytes, the root id of the RFP-021 instance lineage the configuration reads Ethereum through.
- `configId = H("ferrata/config/v1" ‖ bedrockGenesisId ‖ channelId ‖ bridgeHeader ‖ bridgeImageId ‖ evmChainId ‖ vault ‖ attestationRootId ‖ salt:32)`.
- The vault holds `lezConfigId` as an immutable AND copies it into storage slot 2, so that the LEZ configuration's `Init` can attest it through the RFP-021 primitive and refuse to initialise unless it equals its own `configId` (reciprocal pairing). The vault address is therefore computed before deployment (CREATE2 or deployer nonce).
- Inbound nullifier accounts on LEZ are keyed by `(evmChainId, vault, N)` in one namespace for the whole bridge program, not per configuration, so two configurations cannot both mint against one vault deposit.

## 2. Keys (wallet side)

From the LEZ wallet seed, under labels disjoint from LEZ's own `/LEE-Keys/v1/…` tree:
- `kAuth = HMAC(bridgeSeed, "ferrata/auth/v1")`: derives the secrets that authorise claims and releases.
- `kView = HMAC(bridgeSeed, "ferrata/view/v1")`: derives burn tags, memo pads and MACs. Sharing `kView` discloses a user's burns and recipients without letting the viewer claim or release.

## 3. Deposit side

### 3.1 Vault storage contract
- slot 0: `depositRoot` (bytes32); slot 1: `nextLeafIndex` (uint64, alone in its slot); slot 2: `lezConfigId` (bytes32).

### 3.2 Deposit tree (depth 32)
- `LEAF_TAG = H("ferrata/vault/leaf/v2")`
- `leaf = H(0x00 ‖ LEAF_TAG ‖ leafIndex:u64 ‖ depositor:20 ‖ token:20 ‖ amount:u256 ‖ C:32)`
- `node = H(0x01 ‖ left ‖ right)`; empty leaf = 32 zero bytes.

### 3.3 Commitment and nullifier
- `s = HMAC(kAuth, "ferrata/s/v1" ‖ evmChainId ‖ vault ‖ depositor ‖ j:u64)` where `j = depositCount[depositor]` at deposit time.
- `C = H("ferrata/C/v1" ‖ s ‖ kind:u8 ‖ dest:32)`; `kind = 0x01` private LEZ account, `0x02` public LEZ account. `dest` is always an account derived from the same seed (account index `a`, gap limit `g`). A private-kind claim must execute as the destination account itself.
- `fid = H("ferrata/fact/acc-leaf/v1" ‖ evmChainId ‖ vault ‖ slot:u256 (= 0) ‖ leafIndex:u64 ‖ leaf)`: the statement identifier of "leaf `leafIndex` of the accumulator at slot 0 of `vault`". It does not depend on the root, so it is the same under every later root.
- `N = H("ferrata/nf/v1" ‖ s ‖ fid)`.
- A claim (public or private execution) checks: the Merkle path from `leaf` to an imported root, with the path's direction bits equal to `leafIndex`; the opening of `C`; then publishes `N`, the token and the amount.

### 3.4 Vault functions (changes from v1)
- `deposit` / `depositETH`: unchanged except for the leaf (now binds `depositor`) and `retired`.
- `retire()`: freeze authority, one-way. Deposits revert forever afterwards. It cannot touch escrow, checkpoints or releases.
- Registry entries are write-once for `(token, lezUnit, denominations, relayFee per denomination, unitWeight)`. Only caps, `depositsEnabled` and release delays remain adjustable. The relay fee per denomination is fixed at registration and must be ≤ 1% of the denomination.
- `refund(leafIndex, depositor, token, denomIndex, C, path[32])`: permissionless. Requires `sunsetLeafCountSet` and `leafIndex >= sunsetLeafCount`, not already refunded, and the path to hash to the current `depositRoot`. Pays the amount to `depositor` (pull-payment fallback as in §5.4). This is how deposits that LEZ can never import are returned once the configuration has been sunset (§4.5).

## 4. LEZ → Ethereum checkpoints

### 4.1 LEZ checkpoint state
`S = H("ferrata/lez-state/v1" ‖ tipId:u64 ‖ tipHash:32 ‖ keySetHash:32 ‖ configTipHash:32 ‖ burnCount:u64 ‖ burnFrontierHash:32 ‖ importedLeafCount:u64 ‖ sunset:u8)`
- `tipId`, `tipHash`: the LEZ block id and hash up to and including which burns have been collected.
- `keySetHash = H("ferrata/keyset/v1" ‖ n:u32 ‖ key_1:32 ‖ … ‖ key_n:32)`, keys in the channel's `accredited_keys` order. `configTipHash`: the channel's configuration-chain tip, so key rotations are followed in order.
- `burnFrontierHash`: hash of the incremental burn tree's frontier (32 nodes) so a prover cannot reset the tree.
- `importedLeafCount`: the vault leaf count carried by the configuration's last successful `ImportRoot`; `sunset`: whether the configuration's one-way `Sunset` has executed.

### 4.2 What the checkpoint program proves
Given a parent state `P` and a chain of LEZ blocks `P.tip → … → T → … → E`:
1. Every block's hash is recomputed from its bytes (`SHA256(prefix ‖ borsh(id, prev, timestamp, txs, reverted))`) and links by `prev_block_hash`; ids increase by one. This is the LEZ chain, not the channel's message chain: an inscription that honest nodes skipped is simply not on it.
2. From every non-reverted transaction in `(P.tip, T]` addressed to `bridgeHeader`, the program collects, for this `configId` only: burns (appended to the burn tree in order), successful `ImportRoot` (updates `importedLeafCount`), and `Sunset`. Burns of other configurations are ignored.
3. Key-set changes: `ChannelConfigOp`s are supplied as witness and applied in `configTipHash` order; each must carry signatures of at least `t(current n)` current keys. `t(n) = max(1, min(⌈2n/3⌉, n − 1))`, LEZ's own threshold. The program refuses to advance if `n < nMin` or `t < tMin` (deployment constants, in the journal).
4. **Support.** Let `Y = T` if `echoNonce = 0`, else the first block after `T` that contains the 32-byte `echoNonce` in a transaction addressed to `bridgeHeader`. The blocks in `(Y, E]` include blocks inscribed by at least `t` distinct current accredited keys other than the key that inscribed `T`. For each counted block the program checks the inscription: `channelId` equals the pinned one, the payload is the block's borsh bytes, and the Ed25519 signature over the Mantle transaction hash verifies under the claimed key. Only these counted inscriptions are signature-checked and Blake2b-hashed.
5. Journal: `abi.encode(CHECKPOINT_DOMAIN, evmChainId, vault, lezConfigId, nMin, tMin, parentState, newState, burnRoot, burnCount, importedLeafCount, sunset, echoNonce)` where `newState` is `S` at tip `T` and `CHECKPOINT_DOMAIN = H("ferrata/vault/checkpoint/v2")`.

Single-sequencer zones (`n = 1`): "other than the key that inscribed `T`" cannot be met. A deployment constant `allowSingleKey` (testnets only) relaxes it to that one key; the documentation states that the vault is then secured by one hot key.

### 4.3 Vault: candidates, beacons, finalisation
State: `finalState` (starts at the immutable `genesisState`), `delta` (seconds; `>= deltaFloor`, immutable floor; the admin can only raise it; a change applies to candidates submitted afterwards), `beacons[]` of `(timestamp, nonce)`, `candidates[newState]`, `checkpoints[]` (history of finalised checkpoints).

- `recordBeacon()`: permissionless. Appends `(block.timestamp, keccak256(abi.encode(block.prevrandao, block.number, address(this))))`. At most one per block.
- `submitCandidate(journal fields, seal)`: permissionless. Requires `parentState == finalState`, `echoNonce == 0`, `nMin`/`tMin` equal the immutables, not frozen, proof valid. Stores the candidate with `submittedAt = block.timestamp` and `deltaAtSubmit = delta`. Also records a beacon.
- `finalize(newState, beaconIndex, journal fields, seal)`: permissionless. Requires the candidate to exist with `parentState == finalState`; `beacons[beaconIndex].timestamp >= submittedAt + deltaAtSubmit`; the journal's `echoNonce == beacons[beaconIndex].nonce`; same `parentState`, `newState`, `burnRoot`, `burnCount`, `importedLeafCount`, `sunset` as the candidate; proof valid. Then `finalState = newState`; the checkpoint is appended to history with `finalizedAt`; `burnRoot` becomes accepted; if `sunset`, `sunsetLeafCount = importedLeafCount`.
- Candidates that are never finalised are harmless: every candidate and every finalisation starts from the current `finalState`.

Why this gives validity and finality (assumptions in §4.6): the candidate proof shows that a sequencer other than `T`'s producer built on `T` before Ethereum time `a = submittedAt`. The finalisation proof shows that a sequencer other than `T`'s producer built on a block that contains a value nobody knew before Ethereum time `a + Δ`. With fewer than `t` malicious keys, both groups contain an honest sequencer; the first re-executed `T`, and the second was still building on `T` at least Δ of Ethereum time later.

### 4.4 Revocation (reject-only)
- `revokeFrom(index)`: freeze authority. Requires `checkpoints[index].finalizedAt >= block.timestamp - revokeWindow` (immutable window). Marks every checkpoint from `index` on as revoked (their burn roots are no longer accepted), sets `finalState` to the state before `index`, and clears `sunsetLeafCount` if it was set at or after `index`. Used nullifiers stay used. It cannot accept anything: a chain must again satisfy §4.3 from the restored state.

### 4.5 Retirement of a configuration (the migration mechanism)
1. Freeze authority calls `retire()` on the vault (no new deposits) and `Sunset` on the LEZ configuration (one-way: no further `ImportRoot`; claims against imported roots and burns continue).
2. Holders burn and release as usual.
3. Once a finalised checkpoint carries `sunset = true`, every vault leaf with `leafIndex >= importedLeafCount` can be refunded to its depositor on Ethereum (§3.4). Leaves below it remain claimable on LEZ forever.
No party moves escrow. Escrow backing tokens that are never burned before the checkpoint program can no longer follow LEZ stays in the vault; the documentation says so.

### 4.6 Assumptions of the checkpoint rule
(i) Fewer than `t` keys of every key set the program has accepted are controlled by an attacker, by collusion or theft. (ii) An honest sequencer extends, by `prev_block_hash`, only blocks its node has re-executed, and another sequencer's block enters its view only after its own Logos Blockchain node processed that block's inscription on chain. (iii) Logos Blockchain is safe and live: no reorganisation deeper than `k`, and at least `k` blocks in any window of length Δ. (iv) Ethereum's `prevrandao` is unpredictable Δ ahead, and the proof system is sound. With `t` keys an attacker can sign a fabricated history; it must still publish a candidate on Ethereum and wait Δ before finalising, which is the window in which `monitor` compares the candidate with an honest LEZ node and the freeze authority freezes.

## 5. Burn and release

### 5.1 Burn note (published on LEZ) and burn leaf
- `r_i = HMAC(kView, "ferrata/burn-tag/v1" ‖ configId ‖ i:u64)`; the wallet advances `i` whenever a burn is published, even if it reverts. The bridge program keeps a write-once account per `r`: a repeated `r` is refused.
- `sOut = HMAC(kAuth, "ferrata/s-out/v1" ‖ configId ‖ r)`.
- `COut = H("ferrata/Cout/v1" ‖ sOut ‖ evmChainId ‖ vault ‖ recipient:20)`.
- `memo = recipient XOR HMAC(kView, "ferrata/memo/v1" ‖ r)[0..20]`.
- `mac = HMAC(kView, "ferrata/burn-mac/v1" ‖ r ‖ token:20 ‖ amount:u256 ‖ COut)[0..16]`. The wallet releases only notes whose `mac` verifies.
- Note = `(r, memo, mac, token, amount, COut)`.
- `BURN_TAG = H("ferrata/burn/leaf/v2")`; `burnLeaf = H(0x02 ‖ BURN_TAG ‖ configId ‖ burnIndex:u64 ‖ token:20 ‖ amount:u256 ‖ COut ‖ r)`; same node rule and depth as the deposit tree.
- The burn handler debits the wrapped token through synchronous messages only, so a non-reverted burn transaction means the tokens were destroyed.

### 5.2 Release statement (user-side proof)
- `burnId = H("ferrata/burn/v1" ‖ configId ‖ burnIndex:u64 ‖ burnLeaf)`; `NOut = H("ferrata/nf-out/v1" ‖ sOut ‖ burnId)`.
- Private inputs: `burnIndex`, the Merkle path, `sOut`, `r`. The proof checks the path (direction bits equal `burnIndex`) to `burnRoot`, the opening of `COut`, and outputs the journal.
- Journal: `abi.encode(RELEASE_DOMAIN, evmChainId, vault, lezConfigId, burnRoot, NOut, token, amount, recipient, relayer, relayFee)`, `RELEASE_DOMAIN = H("ferrata/vault/release/v2")`.
- The guest's cycle count must not depend on the witness. Only succinct (recursion) receipts are handed to a wrapper.

### 5.3 Vault release
- Requires: `burnRoot` accepted (finalised, not revoked) and `block.timestamp >= finalizedAt + releaseDelay(token)` (a delay change applies only to roots finalised after the change); `NOut` unused; `amount` is the token's denomination at `denomIndex`; `relayFee` is 0 or the registered fee for that denomination; if `relayer == address(0)` the fee goes to `msg.sender`; proof valid; not frozen; cap available.
- Replay protection is keyed only on `NOut`, never on proof bytes (Groth16 seals can be re-randomised).

### 5.4 Pull-payment fallback
If the transfer to the recipient (or relayer) fails, the vault credits `owed[payee][token]` and the nullifier stays used; `claimOwed(token)` lets only that payee withdraw later. A recipient that cannot receive therefore never blocks or loses the release, and nobody can redirect it.

## 6. Verifier and failsafe (stop-only)
- The vault's `verifier` immutable is a deployer-owned `RiscZeroVerifierEmergencyStop` (from risc0-ethereum, unmodified) that wraps the base `RiscZeroGroth16Verifier`. Its guardian is the freeze authority; anyone holding a proof of exploit can also stop it. It has no unpause: once stopped, nothing more is accepted by this vault.
- No function can change what the vault accepts. `revokeFrom` and the stop are reject-only.

## 7. Freeze authority, admin
- Freeze authority: can freeze and unfreeze deposits / releases (global or per token), freeze candidate submission, `retire()`, `revokeFrom`, and stop the verifier. Freezes expire after `maxFreeze` (immutable, e.g. 30 days) unless renewed, so a lost key cannot lock the vault forever.
- Admin: registry (write-once entries), caps, `delta` (raise only), release delays (≤ 30 days). Two-step handover for both roles; the two must differ.

## 8. Fees (LEZ side)
- Protocol fees are charged on LEZ in the wrapped token: out of the minted amount on a claim, on top of the burned amount on a burn. They are bounded by an immutable ceiling per denomination (1%), change only after a 7-day timelock measured in Ethereum time (records), and a deposit pays the schedule in force for its leaf index, so a later change cannot confiscate an in-flight deposit.
- Relay fees: inbound, a fixed amount out of the minted amount, to the relayer the claim names; outbound, the fixed registered fee out of the released amount on Ethereum, to the relayer the proof names.
