# Admin view: a deployed FerrataVault, read from the chain

`admin/index.html` is one static page that reads a deployed FerrataVault (v5) through any Ethereum JSON-RPC endpoint
you type and shows its state. It has no build step and no dependency. All of its JavaScript is inline, and its only
network traffic goes to the endpoint you type. It is read-only: it sends `eth_chainId`, `eth_blockNumber`,
`eth_call`, `eth_getLogs`, `eth_getBalance`, `eth_getCode` and `eth_getStorageAt`, and its RPC client refuses
any other method. It never signs anything and never sends a transaction. Every read is pinned to one block. It gets
that block's time from an `eth_call` with no `to`: a creation that is never mined runs a 13-byte program returning
`TIMESTAMP` and `NUMBER`, so `eth_getBlockByNumber` is not needed.

The page computes keccak-256 itself for function selectors, event topics, the error table and EIP-55 checksums, and
SHA-256 for the vault's deposit tree and its published registry and fee hashes. It encodes the calls and decodes the
results with a minimal ABI codec. All reading and formatting are pure functions on `globalThis.FerrataAdmin`:
`readVault(rpcUrl, vault, fromBlock, opts) -> state`, `renderText(state)` and `renderHtml(state)`. The script touches
the DOM only when `document` exists, so `dump.mjs` runs the same code in node.

## Opening it

Open `admin/index.html` from disk (`file://…/admin/index.html`). Type an RPC URL, for example the keyless
`https://ethereum-sepolia-rpc.publicnode.com`, and press **Read**. The vault defaults to the live Sepolia vault
`0xfE3199fa2634413CbAC317371076Cae3EB2d9E54`. The event scan defaults to block 12111048, its creation block (README.md).
For another vault, clear the from-block field or set it to that vault's creation block. If you clear it, the page
looks for the creation block by bisecting `eth_getCode`, which needs an archive node. If the endpoint refuses, the
page says so and scans only the last 50,000 blocks.

**CORS.** A page opened from `file://` sends `Origin: null`, so the endpoint must allow browser origins. If it does
not, the page reports a network error, which is how a browser shows a CORS refusal. In that case, serve the folder
locally (`python3 -m http.server -d admin 8000`, then `http://localhost:8000/`), use an endpoint that allows browser
origins, or use the node dump below, which CORS does not affect. Whether publicnode allows `Origin: null` was not
checked from a browser here: node sends no `Origin` header, and publicnode's answers to it carried no
`Access-Control-Allow-Origin`. If an URL holds an API key in its path or query, the report shows only its origin.

**Errors.** Every RPC or decoding error is listed in the last section ("RPC and decoding errors"). It also appears
where the missing value would be, as `ERROR: …`. A revert is named by its custom error, for example
`reverted with FunctionNotFound()` for an address that is not a v5 vault. `eth_getLogs` range errors are handled by
shrinking the chunk (from 50,000 blocks down to 16), and the Read section shows how many there were. A scan that
still fails is marked incomplete. Rate limits (HTTP 429/5xx) are retried with backoff and counted.

## Node dump and self-test

```
node admin/dump.mjs <rpcUrl> <vault> [fromBlock]   # prints the report as text (progress on stderr)
node admin/dump.mjs --selftest                     # no network
```

`dump.mjs` (node 22, no packages) extracts `<script id="ferrata-admin">` from `index.html`, runs it in a `node:vm`
context that receives only `fetch`, timers and `AbortController`, and prints a header and `renderText(readVault(…))`.
The header gives the endpoint (origin only), the chain, the block and the UTC time. Exit codes: 0 for a complete read,
3 if some read failed (listed in the report), 1 if fatal (no chain, no code at the address, bad input).

The self-test (232 checks) uses expected values that come from outside the page's code:
- keccak-256 of `""`, `"abc"` and inputs of 135, 136, 137 and 300 bytes, which exercise the padding and block
  edges; selectors such as `transfer(address,uint256)` = `a9059cbb`; 11 event topics. All are computed offline with
  Foundry's `cast`.
- The 40 vault errors of `cli/src/codes.rs`. The test parses the list from that file, checks each selector against
  `cast sig`, and checks that `decodeRevert` names it.
- EIP-55 checksums: the examples from the EIP.
- SHA-256: `""`, `"abc"`, the vault's `LEAF_TAG`, `CHECKPOINT_DOMAIN` and `RELEASE_DOMAIN`, 11 random inputs against
  `node:crypto`, and the empty tree root.
- The deposit tree vectors (`vault/test/vectors/tree-v2.json`: leaves, the root after each insert, the final root).
  The registry-entry and fee-schedule hashes, and their SHA-256 slots, from `vault/test/vectors/registry-storage.json`.
- The ABI decoder on inputs made by `cast abi-encode`: arrays, an empty array, a string, `address[3]`, a negative
  `int256`, `bytes4` and `bytes`. Malformed data is refused: short data, a bool of 2, a `uint8` of 256, an address
  with high bits set, an offset out of range. Call data is checked against `cast calldata`.
- Two vault logs decoded, PUSH20 scanning, URL redaction, and a bad URL refused before any request.

The test exits non-zero on any failure. Changing one keccak round constant, one SHA-256 constant or the bool check
makes it fail.

## What each section shows and where it comes from

| Section | Contract getters (all `eth_call` at the pinned block) | Events and other reads |
|---|---|---|
| Read | none | `eth_chainId`, `eth_blockNumber`, `eth_getCode` (code size and hash). The linked FerrataRegistry library is a PUSH20 constant in that code that has code itself. Event scan range and counts, RPC statistics |
| Summary | `mode`, `retired`, the stop's `paused`, `*FrozenUntil`, `nextLeafIndex`, `checkpointCount`, `candidates` | `CandidateSubmitted`, `Release`, `TokenRegistered` |
| Configuration | `verifier`, `checkpointImageId`, `releaseImageId`, `lezConfigId`, `storedLezConfigId`, `genesisState`, `nMin`, `tMin`, `deltaFloor`, `turnSeconds`, `revokeWindow`, `maxFreeze`, `minReleaseDelay`, `mode`, `releaseCapFloor`, `releaseRefillFloor`, `feeRecipient` | `eth_getStorageAt` slot 2 (= `storedLezConfigId`) |
| Roles | `admin`, `pendingAdmin`, `freezeAuthority`, `watchers(0..2)`, `feeRecipient`, `roleProposal` (freeze authority, `effectiveAt`) | Proposed watchers from the last `RolesProposed`. `eth_getCode` of each role address (EOA or contract) |
| Verifier emergency stop | On the stop: `paused`, `owner`, `pendingOwner`, `verifier`. On the wrapped verifier: `VERSION`, `SELECTOR` | Who can trip it (the owner's `estop()`, anyone's `estop(receipt)` with a proof of exploit) is from RISC Zero's source |
| Freezes | `depositsFrozenUntil`, `releasesFrozenUntil`, `checkpointsFrozenUntil`, `tokenDepositsFrozenUntil(t)`, `tokenReleasesFrozenUntil(t)`, compared with the block time | `CheckpointsPaused` (watcher pauses) |
| Checkpoints, burn roots, candidates | `finalState`, `finalImportedLeafCount`, `finalSunset`, `delta`, `revocationEpoch`, `checkpointCount`, `beaconCount`, `checkpoints(i)`, `burnRootStatus(root)`, `burnRootCheckpoint(root)`, `releaseDelayFor(t, i)`, `candidates(newState)`, `beacons(i)` | Candidates are found from `CandidateSubmitted` and their fate from `CandidateCancelled` and `CheckpointFinalized`. Also `CheckpointsRevoked`. `_stateChanges` (needed for a candidate's liveness) is read from storage slot 52 and used only if the three public getters that share the slot match it |
| Token (one section per token) | `tokenConfig(t)`, `tokenCap(t, 0/1)`, `escrowed(t)`, `totalOwed(t)`, `feeState(t)`, `fees(t, index, d)`, `releaseDelayFor(t, checkpointCount)`. On the token: `decimals`, `symbol`, `name`, `balanceOf(vault)`. ETH: `eth_getBalance` | Tokens from `TokenRegistered` (ETH is always probed). `RegistryEntryPublished` (lezDefinition, decimals, entry hash), `ReleaseDelaySet`, `FeesAnnounced`, `FeesInForce`. The published hashes are read at their SHA-256 slots with `eth_getStorageAt`, and compared with the events and with a hash recomputed from them. Also checked: balance against escrowed + owed |
| Global caps | `globalCap(0)`, `globalCap(1)` (capacity, refill, available; utilisation = capacity − available) | none |
| Deposits | `nextLeafIndex`, `depositRoot`, `depositCount(depositor)` | `Deposit` (last 20 and per depositor). Slots 0 and 1. The SHA-256 tree is rebuilt from the `Deposit` events and its root compared with `depositRoot` |
| Releases and refunds | `nullifierUsed(nullifier)`, `refunded(leaf)` | `Release` (last 20, totals), `Refund` |
| Fees and payments owed | `owed(payee, token)`, `totalOwed(token)` | Payees from `OwedCredited` and `OwedClaimed`, the `feeRecipient` and the checkpoint provers, with their credited and claimed totals |
| Retirement and sunset | `retired`, `retireOpensAt`, `keylessFrom`, `sunsetLeafCountSet`, `sunsetLeafCount`, `sunsetSetAt`, `sunsetFinalizedAt` | `Retired`. Refunds open at `sunsetFinalizedAt + revokeWindow + 1` (derived, as `_refundsOpenAt`) |
| Control history | none | The last 40 role, cap, delay, freeze, registry, fee, revocation and sunset events |
| What the contract does not expose | none | The list below |

**Not exposed by the contract, and how the page handles it.** The page says so on the page rather than guessing.
- Registered tokens, pending candidates, owed payees and depositors cannot be enumerated: they come from events.
- `tokenConfig` omits the relay fees: they are shown from `fees()` and `TokenRegistered`.
- Fee schedules' `fromLeaf`, `announcedAt` and `effectiveAt`, the registry entry's `lezDefinition` and decimals, and
  the proposed watchers have no getter: they come from events.
- Bucket levels are visible only as `available`.
- The linked library address has no getter: it is found in the code.
- `_stateChanges` and rule (A)'s `_lastBeaconBlock` are internal: read from storage with a layout check.
- Who holds the role keys is not on chain.
- The links between deposits, claims, burns and releases are on LEZ or hidden by design.

A from-block later than the vault's creation means some event-derived parts are partial. The page shows this, for
example "root recomputation not possible" or depositor counts that differ from `depositCount()`.

## The live read

`results/live-sepolia-<UTC>.txt` is the text dump of the live Sepolia vault, read through
`https://ethereum-sepolia-rpc.publicnode.com` (keyless, default headers, read-only). The file header gives the block
and the time.
