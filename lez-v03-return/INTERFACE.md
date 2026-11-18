# What the stand-in attestor reads from the LEZ v0.3.1 bridge

`standin.py` signs a burn root only after comparing it with what a LEZ v0.3.1 node holds. The bridge program for
v0.3.1 is another part of this work (`lez-v03-bridge/`); until its account layout is final, the three things the
attestor needs from it are described here as an interface, and given to the tool as files.

Unchanged from the v0.4 bridge, and not part of this interface: the burn leaf and the burn tree. They are
`ferrata-bridge-core`'s ([`bridge/crates/ferrata-bridge-core/src/outbound.rs`](../bridge/crates/ferrata-bridge-core/src/outbound.rs),
[`tree.rs`](../bridge/crates/ferrata-bridge-core/src/tree.rs)):

- leaf `k` = `sha256(0x02 ‖ sha256("ferrata/burn/leaf/v2") ‖ configId ‖ k:u64 ‖ token ‖ amount:u256 ‖ COut ‖ r)`;
- a depth-32 SHA-256 tree, node = `sha256(0x01 ‖ left ‖ right)`, empty leaf = 32 zero bytes, leaves appended in burn
  order; the canonical frontier `f[h]` is the completed subtree of height `h` where bit `h` of the count is 1, and 32
  zero bytes elsewhere.

One difference from v0.4 matters. On v0.4 the bridge program keeps no burn tree: the checkpoint program counts the
`Burn` calls it reads in the blocks ([`lez/bridge-wire/src/lib.rs`](../lez/bridge-wire/src/lib.rs), the table at the
top). No program can read v0.3.1 blocks for us today, so the v0.3.1 bridge must keep the accumulator in its own state,
where a node's RPC shows it.

## 1. The accumulator: a layout file (`FERRATA_LEZ_LAYOUT`, `--layout`)

LEZ v0.3.1 keeps, in every account, one byte string ("shard") per program
(`lee/state_machine/core/src/account.rs:106-109, 142-144`), and its sequencer RPC returns them with `getAccount`
(`lez/sequencer/service/rpc/src/lib.rs:98`). The layout file says where in which shard each field is:

```json
{
  "program": "<the bridge program's id: base58, or 0x + 64 hex>",
  "account": "<the account that holds the accumulator: base58, or 0x + 64 hex>",
  "count_endian": "little",
  "burn_count":          {"offset": 0,  "length": 8},
  "burn_root":           {"offset": 8,  "length": 32},
  "burn_frontier":       {"offset": 40, "length": 1024},
  "imported_leaf_count": {"offset": 1064, "length": 8},
  "config_id":           {"offset": 1072, "length": 32}
}
```

- `burn_count` is required, and at least one of `burn_root` and `burn_frontier` (32 nodes of 32 bytes, `f[0]` first,
  canonical).
- `imported_leaf_count` (the vault leaf count of the configuration's last import) and `config_id` are optional. A
  field may name its own `account` and `program` when it lives elsewhere. Without `imported_leaf_count` the count is
  given with `--imported` (`FERRATA_IMPORTED`), and the record says it was not read from the node.
- The offsets above are an example, not the bridge's layout: they are whatever the v0.3.1 bridge program writes.

The tool reads the fields between two reads of the head block id that agree, so they are the state after that block.
It refuses unless the count equals the number of listed burns, the root (and the frontier, if read) equal the ones it
recomputes from the listed burns, and `config_id` (if read) equals the vault's `lezConfigId`.

## 2. The burns: `burns.json` (`FERRATA_BURNS`), or the wallet's notes (`FERRATA_BURN_NOTES`)

A release proof needs a Merkle path, so every leaf of the accumulator must be known, in order. Either file gives them.

`burns.json` (every burn of the configuration, as [`bridge/e2e`](../bridge/e2e/src/main.rs)'s `crown-release` reads
it):

```json
{"config_id": "0x…", "burn_count": 2, "burn_root": "0x…",
 "burns": [{"burn_index": 0, "token": "0x… (20 bytes)", "amount": "0x… (32 bytes)", "c_out": "0x…", "r": "0x…",
            "leaf": "0x…", "block": 83010, "origin": "…"}]}
```

`burn-notes.json` (the LEZ wallet's own burns, the crown run's format): a list of
`{"i", "kind": "public" | "private", "r", "sOut", "cOut", "recipient", "token", "amount" (decimal), "denomIndex"}`.
`standin.py burns-from-notes` turns it into `burns.json` when these burns, in this order, are all the accumulator
holds; the comparison with the node's root then decides whether that was true. The notes are also what the release
proofs are made from (`sOut` is the release secret: the file is the wallet's, and is not committed).

## 3. The tip: the node's own block and finality flag

Nothing from the bridge: `getBlock` (`lez/sequencer/service/rpc/src/lib.rs:56`). See "What final means" in
[`README.md`](README.md).

## 4. As `lez-v03-bridge` has it (read in its source on 18 November 2026)

The bridge port keeps the accumulator as this interface needs it, in its own layout, and `standin.py` derives the
layout file from two values instead of asking for offsets (`standin.py layout --program <id> --config-id 0x<cid>
--imported <n>`; `e2e/testnet-return.sh` does it when `FERRATA_LEZ_PROGRAM` is set):

| What | Where (`lez-v03-bridge/core/src/lib.rs`) | As bytes |
|---|---|---|
| the account `burns(cid)` | `AccountId::for_public_pda(program, sha256("ferrata-lez/v1/bridge/" ‖ "burns" ‖ cid))`; LEZ v0.3.1's public PDA is `sha256("/LEE/v0.2/AccountId/PDA/" ‖ 8 zero bytes ‖ program ‖ seed)` (`lee/state_machine/core/src/program/mod.rs:242-257`) | 32 bytes, base58 for the RPC |
| its shard under the bridge program | borsh of `BridgeState::Burns(BurnTree { config_id, count, frontier })` | `05` ‖ `config_id` (32, offset 1) ‖ `count` u64 little-endian (offset 33) ‖ `20 00 00 00` (offset 41) ‖ 32 frontier nodes (offset 45): 1,069 bytes |
| the account `root(cid, n)` | the same PDA over `sha256("ferrata-lez/v1/bridge/" ‖ "root" ‖ cid ‖ n as u64 little-endian)` | |
| its shard | borsh of `BridgeState::Root(RootState { config_id, evm_chain_id, vault, leaf_count, root, .. })` | `01` ‖ `config_id` (offset 1) ‖ chain id u64 (33) ‖ vault (41) ‖ `leaf_count` u64 (61) ‖ `root` (69) ‖ … |

The frontier is the bridge core's canonical one, so it compares byte for byte. The derived layout also pins the two
variant bytes, the frontier's length and `root(cid, n)`'s configuration, and the attestor compares `root(cid, n).root`
with the vault's `depositRoot()` when LEZ imported every leaf. These offsets were read from the port's source, not
checked against its binary; a wrong derivation finds an empty shard or a wrong byte, and the attestor then refuses.

The wallet's notes: the port's driver makes burn `i` with `BurnNote::create` from keys
`BridgeKeys::from_bridge_seed(sha256(<seed text>))` and records `{r, memo, mac, c_out, hash}` per burn, without the
release secret. `standin.py wallet-burns --seed-text … --check <the driver's record>` recomputes the notes with their
secrets from the same seed text and refuses unless they are the recorded ones.

**One vault for both directions.** The vault that takes the testnet run's deposits must be the one
`e2e/testnet-return.sh` deploys (verifier = the stand-in). The port's own `sepolia.sh` deploys a vault behind RISC
Zero's verifier alone, with `checkpoint_b`'s image id: nothing can leave that vault until a checkpoint proof for the
zone exists, and the stand-in cannot be added to it afterwards (the verifier is immutable). For a round trip the
order is therefore: `e2e/testnet-return.sh plan` (the vault's address) → the port's configuration id and deposit
commitments for that address → `plan` again with `FERRATA_LEZ_CONFIG_ID` and `FERRATA_DEPOSITS` → deploy and deposit
here → the port's LEZ steps → `attest` here.
