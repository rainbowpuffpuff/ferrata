# AGENTS.md: the Ferrata LEZ test network

For AI agents and the people who run them. Reading needs no key and no account: every request below is a plain JSON-RPC call you can paste into a shell. Each response shown was recorded from this network's public RPC on 2026-11-23 09:32Z (head block 36); run the request again for today's values.

## Rules

1. **Test network only.** Its tokens have no value, and it may be reset (block numbers then start again).
2. **Never put a private key in a prompt.** Do not paste, print, log, commit or send a private key, seed phrase or wallet password: not to this site, not to any agent, chat or issue. Generate keys with LEZ's wallet on the machine that signs and keep them there. Nothing here asks for a key; anything that does is not us.
3. **Be gentle.** At most about 10 requests a second; read history with `getBlockRange` (at most 50 blocks per call).
4. **Verify, do not trust**, the sequencer included: recompute block hashes, check the light clients' headers against Ethereum's beacon APIs, re-verify attested facts. The scripts at the end do the first two; the explorer does all three in your browser.
5. **No receipts on LEZ v0.4.0-rc3.** A transaction's outcome is: included in a block (`getTransaction`) and not in that block's `reverted` list. Events are not served by the sequencer's RPC.

## The network

| | |
|---|---|
| JSON-RPC endpoint | `https://{{HOST}}/rpc` (POST, JSON-RPC 2.0, CORS open) |
| Software | LEZ v0.4.0-rc3 (Logos Execution Zone) sequencer |
| Anchored to | its own Logos Blockchain node (0.3.5-rc.1): every block is inscribed into the zone's Bedrock channel; a block's `bedrock_status` (Pending, Safe, Finalized) says how far |
| Chain id | LEZ transactions carry no chain id; the zone is named by its Bedrock channel id (`getChannelId`): `0101010101010101010101010101010101010101010101010101010101010101`. The Ethereum chain the light clients follow: Sepolia (11155111) |
| Blocks | every 10 s; at most 20 user transactions and 1 MiB per block |
| Native token | LGO, 9 decimals: balances and fees are integers of 10^-9 LGO |
| Explorer, status, programs | https://{{HOST}}/explorer/ · https://{{HOST}}/status/ · https://{{HOST}}/registry/ |
| Faucet | announced on the landing page when it opens |
| Faucet API | `POST https://{{HOST}}/faucet with {"account": "<your LEZ public account id, base58>"}: 10 LGO, once a day per account (GET /faucet: its terms, account and balance)` |
| Web wallet | https://{{HOST}}/wallet/ |

### Ferrata's programs

| Program | Account (base58) | Image id | IDL |
|---|---|---|---|
| Ferrata attestation (RFP-021) | `not deployed yet` | `0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df` | https://{{HOST}}/idl/ferrata_attestation.idl.json |
| Ferrata bridge (RFP-022) | `not deployed yet` | `0x847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515` | https://{{HOST}}/idl/ferrata_bridge.idl.json |
| Reference consumer (RFP-021) | `not deployed yet` | `0x8464603bcf9ea9c69c0eac100605deb9c8ef666a56f964cceba66947958e4b96` | — |
| Holdings (worked example) (RFP-021) | `not deployed yet` | `0xa79e3f5c4496f2707ab6f55a980efb00dd80aaf562b51f5a504349e9a97c5591` | — |
| Names (test network) | `not deployed yet` | `0xee9578cab413451e1f0a0cf267a7874ca3231ed0a30907f5d793648ac4e6f790` | https://{{HOST}}/idl/ferrata_names.idl.json |
| Encrypted messages (test network) | `not deployed yet` | `0x04ae1988fe84eef6da472152a75b17e6c7453e6e4775264f12af1a750b945277` | https://{{HOST}}/idl/ferrata_msg.idl.json |
| Recorded statements (RFP-021) | `not deployed yet` | `0x86acef86dc73be13b76de63d21d41ac928639f3be7144132f22f14831d5bc663` | — |
| Statements reader (RFP-021) | `not deployed yet` | `0xd2a8b082f1e4326daad8aad58c2a525f76f17001bb362040fec67260d4d3e73d` | — |
| Operator rebate (RFP-021) | `not deployed yet` | `0xe8f1c1c1381a08faf284fd33226ae11fcb4b7b7d3c397faa113b76d9c7e6d902` | — |

Each image id is the hash of the exact binary, which builds reproducibly from the repository (https://github.com/rainbowpuffpuff/ferrata: `scripts/reproduce-image-ids.sh build`). A program's account holds a loader header naming its image id: `getAccountView` with `program_account_id` `JAQ5MVHbCkSYRzXunsrNuM2m1LS859PGveHfoYPAmcvZ` (LEZ's program loader).

### Light-client instances

Listed here once created; meanwhile the status page finds them from their `init` transactions.

## Encodings

- **Account ids**: 32 bytes in base58, as the RPC takes them (LEZ's wallet shows them as `Public/<base58>`).
- **Hashes**: 64 hex digits, no `0x`.
- **Blocks and transactions**: base64 of borsh. A block is `header {block_id u64, prev_block_hash [32], hash [32], timestamp u64 (ms)}`, `body {transactions Vec<LeeTransaction>, reverted Vec<u64>}`, then `bedrock_status u8` (0 Pending, 1 Safe, 2 Finalized), which is outside the hash.
- **Block hash** = SHA-256(`/LEE/v0.3/Message/Block/` zero-padded to 32 bytes ‖ bytes 0..40 ‖ bytes 72..80 ‖ the body).
- **Transaction hash** = SHA-256 of the inner transaction's borsh: the `LeeTransaction` without its first (variant) byte; variant 0 public, 1 privacy-preserving.
- **A signer's account** = SHA-256(`/LEE/v0.3/AccountId/Public/` zero-padded to 32 ‖ its 32-byte public key).
- **Big integers** (u128 balances and nonces) are bare JSON numbers: parse them without rounding (Python's `json` is exact; in JavaScript use a lossless parser).
- **Account state**: `{"nonce": n, "data": {"actor_states": {"<program account>": [bytes]}}}`. The native balance is the little-endian u128 under the all-zero program `11111111111111111111111111111111`.
- **A full decoder** (blocks, public and private transactions, messages through an IDL) in plain ES modules: https://{{HOST}}/assets/lez.js, https://{{HOST}}/assets/idl.js.

## Every method

### `sendTransaction`

Submit a signed transaction. Not called here: build and sign with LEZ's wallet library or CLI; the answer is the hash, not the outcome. Parameters: tx: LeeTransaction (base64 of borsh). Returns: the transaction's hash (hex).

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' --data '{"jsonrpc": "2.0", "id": 1, "method": "sendTransaction", "params": ["<base64 of borsh(LeeTransaction)>"]}'
```

### `getFeeState`

Base fees (execution per cycle, storage per byte), the band the next block's can move in, the per-block gas caps: size `max_fee` from this. Parameters: none. Returns: the fee market at the head.

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getFeeState","params":[]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": {
  "height": 36,
  "base_fee_exec": 8,
  "base_fee_stor": 8,
  "next_base_fee_exec_floor": 8,
  "next_base_fee_exec_ceiling": 9,
  "next_base_fee_stor_floor": 8,
  "next_base_fee_stor_ceiling": 9,
  "max_gas_exec": 10000000,
  "max_gas_stor": 1000000
 }
}
```

### `checkHealth`

Answers null when the sequencer is up. Parameters: none. Returns: null.

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"checkHealth","params":[]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": null
}
```

### `getBlock`

One block, with its Bedrock status as the last byte. Parameters: block_id: u64. Returns: base64(borsh(Block)) or null.

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getBlock","params":[36]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": "JAAAAAAAAAC9kn42ZthftfinOiZyCDDG668wViyhqDwz+JD6ylvT0DUMrbZJ0uxyXweOtJqREAx/6gDLM0s86wNyrTgAE96jF9G/B6IBAAACAAAAAAQAAAAZ…(948 chars)"
}
```

### `getBlockRange`

Blocks start..=end; ask for at most 100 at a time. Parameters: start_block_id: u64, end_block_id: u64. Returns: [base64(borsh(Block))].

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getBlockRange","params":[35,36]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": ["IwAAAAAAAAC68CbnGhuBaq1WGzFoprJQnt6zpwvtHCrX+pEzpc2v6b2SfjZm2F+1+Kc6JnIIMMbrrzBWLKGoPDP4kPrKW9PQB6q/B6IBAAACAAAAAAQAAAAZ…(948 chars)", "JAAAAAAAAAC9kn42ZthftfinOiZyCDDG668wViyhqDwz+JD6ylvT0DUMrbZJ0uxyXweOtJqREAx/6gDLM0s86wNyrTgAE96jF9G/B6IBAAACAAAAAAQAAAAZ…(948 chars)"]
}
```

### `getLastBlockId`

The head. Parameters: none. Returns: u64.

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":[]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": 36
}
```

### `getAccountBalance`

The native balance. Parameters: account_id: base58. Returns: u128.

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getAccountBalance","params":["ob15kRVNHFC2VxygSRqk55tLDeQ5bwTrvrbSYWkN1Gs"]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": 1000000000000000
}
```

### `getTransaction`

An included transaction and its block; null for one not (yet) in a block. No outcome: see the block's `reverted`. Parameters: tx_hash: hex. Returns: [base64(borsh(LeeTransaction)), block_id] or null.

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getTransaction","params":["a04d987623c39dd9a7a2beb4d41983f761f2641fd5cc8e354b48d24571b60636"]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": ["AAMAAAAL7ympEyXTDpyj3DVTn+7B9NdrVNDoY81EX70AOLMt8AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAjK2oJ40ott2uc2JYF+vAToipaZae…(612 chars)", 1]
}
```

### `getAccountsNonces`

The accounts' nonces, in order (sign the next transaction with these). Parameters: account_ids: [base58]. Returns: [u128].

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getAccountsNonces","params":[["ob15kRVNHFC2VxygSRqk55tLDeQ5bwTrvrbSYWkN1Gs"]]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": [0]
}
```

### `getAccountView`

One actor state of an account: the account's state under one program. Parameters: actor: {account_id, program_account_id}. Returns: {nonce, data: {actor_states: {program: [bytes]}}} or null.

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getAccountView","params":[{"account_id":"ob15kRVNHFC2VxygSRqk55tLDeQ5bwTrvrbSYWkN1Gs","program_account_id":"11111111111111111111111111111111"}]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": {
  "nonce": 0,
  "data": {
   "actor_states": {
    "11111111111111111111111111111111": [0, 128, 198, 164, 126, 141, "…(10 more)"]
   }
  }
 }
}
```

### `getPublications`

Messages published into the commitment tree (cross-actor messages, clear or sealed). Parameters: from_position: u64, limit: u32. Returns: [[position, Publication]].

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getPublications","params":[0,2]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": []
}
```

### `getMessagePath`

The membership path of a published message. Parameters: position: u64. Returns: [position, [hash]] or null.

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getMessagePath","params":[0]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": [
  0,
  [
   [41, 207, 4, 189, 189, 211, 156, 29, 219, 43, 135, 229, 58, 9, 222, 254, "…(32 bytes)"],
   [172, 1, 168, 146, 218, 154, 228, 10, 203, 64, 96, 51, 70, 50, 201, 105, "…(32 bytes)"],
   [73, 42, 230, 37, 7, 188, 231, 240, 27, 168, 5, 24, 231, 83, 165, 161, "…(32 bytes)"]
  ]
 ]
}
```

### `getRecoveryBinding`

The recovery note bound to a private address. Parameters: address: base58. Returns: EncryptedNote or null.

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getRecoveryBinding","params":["ob15kRVNHFC2VxygSRqk55tLDeQ5bwTrvrbSYWkN1Gs"]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": null
}
```

### `getProofsAndRoot`

Membership paths of commitments and message leaves under one root (private transactions use these). Parameters: commitments: [[u8;32]], message_positions: [u64]. Returns: [[path or null], [path or null], root].

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getProofsAndRoot","params":[[],[]]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": [
  [],
  [],
  [220, 95, 36, 186, 85, 131, 158, 108, 156, 77, 98, 20, 163, 236, 12, 106, "…(32 bytes)"]
 ]
}
```

### `getAccount`

Every actor state of an account. Parameters: account_id: base58. Returns: {nonce, data: {actor_states}} or null.

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getAccount","params":["ob15kRVNHFC2VxygSRqk55tLDeQ5bwTrvrbSYWkN1Gs"]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": {
  "nonce": 0,
  "data": {
   "actor_states": {
    "11111111111111111111111111111111": [0, 128, 198, 164, 126, 141, "…(10 more)"]
   }
  }
 }
}
```

### `getProgramIds`

Image ids the node reports (on rc3: the privacy-preserving circuit's). Parameters: none. Returns: {name: [u32; 8]}.

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getProgramIds","params":[]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": {
  "privacy_preserving_circuit": [2517376867, 3464899159, 3300512099, 2206436585, 1795275722, 388007228, "…(2 more)"]
 }
}
```

### `getChannelId`

The zone's Bedrock channel id: what names this network. Parameters: none. Returns: hex.

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getChannelId","params":[]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": "0101010101010101010101010101010101010101010101010101010101010101"
}
```

### `getCrossZoneDeadLetters`

Cross-zone deliveries this sequencer gave up on. Parameters: none. Returns: {total_retired, retained}.

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getCrossZoneDeadLetters","params":[]}'
```
```json
{
 "jsonrpc": "2.0",
 "id": 1,
 "result": {
  "total_retired": 0,
  "retained": []
 }
}
```

### `requeueCrossZoneDeadLetter`

Operator action, unauthenticated in LEZ; this network's gateway may refuse it. Not called here. Parameters: message_key: hex. Returns: requeued | already_pending | not_found | not_retained.

```sh
curl -s https://{{HOST}}/rpc -H 'content-type: application/json' --data '{"jsonrpc": "2.0", "id": 1, "method": "requeueCrossZoneDeadLetter", "params": ["<message_key hex>"]}'
```


## Read Ferrata's state

- **Decode with the IDLs**: https://{{HOST}}/idl/ferrata_attestation.idl.json and https://{{HOST}}/idl/ferrata_bridge.idl.json (SPEL's IDL
  format). A message is its instruction's one-byte discriminator followed by the borsh of its arguments; an actor state
  decodes as one of the IDL's `accounts` types.
- **Ferrata's accounts** are PDAs: seed = SHA-256(`ferrata-lez/v1/` ‖ tag ‖ parts) (the bridge: `ferrata-lez/v1/bridge/`),
  account = SHA-256(`/LEE/v0.2/AccountId/PDA/` zero-padded to 32 ‖ program account ‖ seed). Tags of the attestation
  program: `instance` (instance id), `job` (instance id, job id), `header` (instance id, slot as u64 little-endian).
- **An instance** (`InstanceState`): its configuration, `active`, `applied`, and `store`, which is ferrata-core's
  `CompactStore`: fork byte (0 Altair … 4 Gloas), the finalized beacon header (112 bytes of SSZ), the current sync
  committee's summary (80 bytes), a flag and the next committee's summary.
- **A record** (`RecordState`) at `header(instance id, slot)`: written once for each finalized header; its `record` is
  `HeaderRecord`: the beacon header (112 bytes), the execution root, a layout byte (0 none; 1 payload: block number u64,
  block hash, parent hash, state root, receipts root, timestamp u64; 2 Gloas: the block hash only).
- **Check a record yourself**: hash its beacon header (SSZ `hash_tree_root`) and compare with
  `GET <beacon API>/eth/v1/beacon/headers/<slot>` → `data.root`; compare the execution block hash with
  `eth_getBlockByNumber` on any Ethereum RPC. The status page (https://{{HOST}}/status/) does both for every instance.

## Attest an Ethereum fact, and read the statement

1. **Choose the record**: the instance's newest, `header(instance id, finalized slot)` (read the slot from the
   instance's store). Its execution block is the block your proofs must be for.
2. **Fetch proofs** at that block from any Ethereum RPC: `eth_getProof(address, [slots], block)` for accounts and
   storage slots; the block's receipts for logs. For a Gloas record (block hash only) include the block's RLP header,
   which must hash to the record's block hash. The program checks every proof against the record, so the RPC need not
   be trusted.
3. **Build the message**: `attest` (discriminator 7) with a `Request {instance_id, chain_id, header, hops, accounts,
   logs, leaves, relations}` (IDL). Each `AccountQuery` carries its account proof, optional conditions (`field`, `cmp`,
   32-byte `rhs`) and `SlotQuery`s; each `LogQuery` a receipt proof and conditions on the emitter, a topic or a data word.
4. **Send** a public transaction whose root call goes to actor (the record account, the attestation program), signed
   with LEZ's wallet library, fee declared from `getFeeState`. The repository's `ferrata attest`
   (https://github.com/rainbowpuffpuff/ferrata/blob/main/lez/ATTEST-CLI.md) builds them: `ferrata attest --lez-rpc https://{{HOST}}/rpc --program <account> …
   attest --instance <id> --record-slot <slot> --eth <Ethereum RPC> --block <n> --storage ADDRESS:SLOT[:eq:VALUE]`
   computes the program's own verdict first and sends only with `--send --yes`. (It is built against LEZ
   v0.4.0-rc1's libraries and has not yet been run against this rc3 network.)
5. **Read the outcome**: included, and not in the block's `reverted` list, means the program verified every proof and
   condition and emitted the statements (event `FRT-ATT1`); reverted means it refused (exit codes in the IDL, e.g. 130
   `PredicateFailed`). The rc3 RPC does not serve events, so read the statement from what you proved: the explorer's
   transaction page re-verifies the request in your browser (Keccak, RLP, the state, storage and receipt tries, the
   conditions) and shows each fact's value and whether its verdict agrees with the chain's.
6. **A program that needs the statement on chain** sends `attest` as a message instead and receives an `Attestation`
   reply (`instance_id, chain_id, slot, block_number, block_hash, hops, statements[{id, fact}]`): see the reference
   consumer (https://github.com/rainbowpuffpuff/ferrata/tree/main/lez/guests/consumer) and the consumer guide (https://github.com/rainbowpuffpuff/ferrata/blob/main/docs/guides/consumer-guide.md).

## Use the bridge (Sepolia ⇄ LEZ)

The bridge moves ETH and registered ERC-20s from a vault on Sepolia to private (default) or public holdings on LEZ and
back. Vault on Sepolia: `to be announced`. The steps, each with the `ferrata` tool (https://github.com/rainbowpuffpuff/ferrata/tree/main/cli; the user guide:
https://github.com/rainbowpuffpuff/ferrata/blob/main/docs/guides/user-guide.md):

1. `ferrata keys` — your bridge seed, kept locally (never share it).
2. `ferrata deposit` — on Sepolia, into the vault, committed to your LEZ destination (or the web depositor page,
   https://github.com/rainbowpuffpuff/ferrata/tree/main/web/depositor, from MetaMask).
3. Root import — anyone imports the vault's deposit root to LEZ through the attestation program.
4. `ferrata claim` — on LEZ, private by default: mints your holding without linking it to the deposit.
5. `ferrata burn` — on LEZ, to an Ethereum address; then the checkpoint is proven and, after its delay,
   `ferrata release` (or a relayer) pays out on Sepolia.
6. `ferrata recover` — rebuild everything from the seed alone.

## Names (`ferrata_names` v2) — for agents

Human-readable names for LEZ accounts on our v0.4 test network: `alice` → an account (and optionally a private
receiver). Program `lez/guests/names` (all logic in `lez/names-wire/src/lib.rs`), IDL `idl/ferrata_names.idl.json`,
JS `names/js/names.js`, CLI `names/js/cli.mjs` (Node 22.7+ or 20.19+). The program id is published with the network's
other programs; v2 is a new program at a new address (v1 stays live and immutable at its own; names do not carry over).

**The web wallet is still on v1.** `/wallet/` registers and resolves names with the first version (`ferrata_names`
in `/programs.json`) until its bundled client is updated: a name a wallet user registered is on v1, not here, and the
same name can belong to another account on v2. Ask which version the user means, and show the account before sending.

**Resolve before you send.** Never send to a name without resolving it on chain at that moment, and show the
resolved account to the user first. The RPC is trusted for what it returns (LEZ serves public state without proofs):
pass several independent RPCs as `rpc: [url1, url2]` to refuse an answer they disagree on.

```js
import { Names } from './names.js';
const names = new Names({ rpc: 'https://<host>/rpc', program: '<names program id, base58>' });
const r = await names.resolve('alice');        // null if free, released, expired, or only a look-alike is registered
// r.target: the public account (base58) or null; r.receiver: { npk, vpk } for a private send, or null
// names.js finalAt(r): before it the registration is provisional (an older commitment can still take it)
const who = await names.reverse('<account>');  // the account's name, already checked against the forward record
```

From a shell: `node names/js/cli.mjs --rpc URL --program ID resolve alice` (exit 0 found, 1 not), `reverse ACCOUNT`,
`whois NAME` (record, status `live|grace|lapsed|free`, provisional until when, a look-alike if one holds the address),
`check NAME` (the rules, offline), `rules`.

**Act on names** (signed by the account that acts, which also pays the fee; `--key FILE` holds a test key in hex):
`register NAME [--target ID|--no-target] [--receiver-keys FILE]` (commit, wait about 4 minutes, reveal),
`set-target`, `set-primary NAME|--clear`, `renew`, `transfer NAME TO` (an offer: the account `TO` takes it with
`accept NAME`; `transfer NAME <yourself>` withdraws it), `release`, `forget` (drop names that left you). Each prints
the transaction and whether it applied or reverted (exit 0 / 1). Send each action with `gasFor(msg)` from names.js
(its worst case measured, with a margin): out of gas, a transaction is charged its whole limit.

**Never send a reveal that can fail.** A reveal that is refused or reverts is still in a block: it publishes the name.
The CLI checks everything first (`preflightReveal` in names.js: the commitment, its window, the name, the list of
names, the balance) and sends nothing early; a wallet must do the same. The program protects the account whose
commitment is oldest: if a copier reveals first, a later reveal of the older commitment, within its hour, takes the
name back (event `Displaced`). Registrations are provisional for an hour.

**Rules** (the program refuses anything else, with the exit code in brackets): 3–32 characters of `a-z 0-9 -`, no
leading/trailing `-`, no `--` [245]; reserved names such as `faucet`, `wallet`, `admin` and their variants
(`faucet1`, `wa11et`, `ferrata-team`) [246]; one name per look-alike: `0`=`o`, `1`=`l`, `rn`=`m`, `vv`=`w` [234];
commit first [230–233]; oldest commitment first [247]; at most 5 names per account, names that left you counting
until you `forget` them [251]; 180-day term, renewable by the owner until 30 days after expiry [250], after which
anyone may register it; only the owner offers, updates, renews or releases [248], only the account offered to
accepts [254]; an expired name does not resolve and cannot be transferred or updated [249]. A deadline holds for the
block: send a deadline-bound action at least 10 minutes before it. No fee on the test network.

**Pitfalls.** Lower-case what a user typed (`normalize`): `Alice` is refused, not folded, by the program. Do not
trust a stored primary name on its own: `reverse()` re-checks it. `resolve('a1ice')` is null while `alice` holds the
address: compare the name you show with the record's. Names are a test-network convenience, unaudited.

## Encrypted messages (`ferrata_msg_v2`)

<!-- For /AGENTS.md: the section an AI agent reads to message another account on this test network. -->

Any account can receive end-to-end encrypted messages once it publishes a messaging key; any funded account can send.
The text and its exact length stay private. Public on chain: who sent to whom and when, each envelope's size bucket,
and the recipient's published key, contacts and block lists, policy and inbox index (sizes, senders, deletions).
Prototype, unaudited, test network only. Design: `docs/design/messages.md`.

**The web wallet is still on v1.** `/wallet/`'s inbox uses the first version (`ferrata_msg` in `/programs.json`)
until its bundled client is updated: wallet users publish their messaging keys there, not here, so a v2 send to
them is refused for want of a key.

**As an agent, with the CLI** (Node 20 or later, in `deploy/testnet/web/msg/` of the repository, or `/msg/` on this
host, whose `config.json` names the program and the RPC):

```sh
export FERRATA_KEY_FILE=agent.key               # 64 hex digits: a funded account's key, kept local
node ferrata-msg.mjs publish                    # once: your hybrid X25519 + ML-KEM-768 key (derived from your key)
node ferrata-msg.mjs keygen                     # your fingerprint: give it to whoever writes to you
node ferrata-msg.mjs send <account> '<text>' --expect-key <their fingerprint>   # at most 2,920 bytes of UTF-8 text
node ferrata-msg.mjs inbox --json               # [{n, from, atMs, text | error}, ...]
node ferrata-msg.mjs clear <last n>             # free your inbox (32 messages, 64 KiB at most)
```

Exit status: 0 done (applied in its block), 1 refused (before sending, at no cost, or reverted in its block, fee
charged), 2 usage, 3 error.

**From code** (browser or node): `import * as M from '/msg/ferrata-msg.js'` (keys, `seal`, `open`, `openInbox`,
`preflightSend`, `fingerprint`, the borsh encoders and PDAs) and `/msg/client.js` (`send`, `publish`, `messages`,
`remove`, `contacts`, `block`, `setPolicy`, `outcome`). The interface is `idl/ferrata_msg_v2.idl.json`.

**Rules the program enforces** (the CLI and `preflightSend` check them first): the recipient must have published a
key; envelopes are exactly 512, 1,024, 2,048 or 4,096 bytes, a hybrid one at least 2,048, hybrid whenever the
recipient published ML-KEM, for its current key epoch; at most 30 sends whose declared times lie within any hour; a
non-contact holds at most 1 unread message in an inbox and non-contacts at most 8 together; recipients may block
senders or accept contacts only. Exit codes 0xA1 to 0xB5 are named in the IDL's `errors`.

**Etiquette for agents:** publish a key before you expect replies; read with `inbox --json`, act, then `clear`; do
not send to accounts without a key (refused, and pointless); compare fingerprints out of band and send with
`--expect-key`; say in your first message how to reach you otherwise.

## Minimal scripts

Both read the head block and recompute its hash, read each light-client instance's finalized Ethereum header from its account, and compare its block root with public beacon APIs. Read-only; no key.

### Python 3 (standard library): https://{{HOST}}/agents/ferrata_check.py

```python
#!/usr/bin/env python3
"""Read the Ferrata LEZ test network and check it yourself. Python 3.8+, standard library only, read-only.

    python3 ferrata_check.py https://<site>/          # the site serves /config.json and /rpc

1. The head block: getLastBlockId, getBlock; its hash recomputed from its bytes (SHA-256 over LEZ's encoding).
2. Each light-client instance in the site's config.json: its finalized Ethereum beacon header, read from the
   instance's account (getAccountView) and hashed to its block root (SSZ hash_tree_root).
3. That root compared with a public beacon API's root for the same slot (the providers in config.json).
Exit 0 if every check holds, 1 if one fails, 2 if something could not be read.
"""
import base64
import hashlib
import json
import struct
import sys
import urllib.request
from urllib.parse import urljoin

B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
UA = {"User-Agent": "ferrata-check/1", "Content-Type": "application/json", "Accept": "application/json"}


def b58encode(b):
    n = int.from_bytes(b, "big")
    s = ""
    while n:
        n, r = divmod(n, 58)
        s = B58[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + s


def b58decode(s):
    n = 0
    for c in s:
        n = n * 58 + B58.index(c)
    body = n.to_bytes((n.bit_length() + 7) // 8, "big") if n else b""
    return b"\0" * (len(s) - len(s.lstrip("1"))) + body


def http_json(url, body=None):
    req = urllib.request.Request(url, data=json.dumps(body).encode() if body is not None else None, headers=UA)
    with urllib.request.urlopen(req, timeout=20) as r:
        return json.loads(r.read())


def rpc(url, method, params=()):
    r = http_json(url, {"jsonrpc": "2.0", "id": 1, "method": method, "params": list(params)})
    if "error" in r:
        raise RuntimeError(f"{method}: {r['error']}")
    return r["result"]


def pad32(s):
    return s.encode().ljust(32, b"\0")


def block_hash(raw):
    """LEZ v0.4: SHA-256("/LEE/v0.3/Message/Block/" padded to 32 || id || prev || timestamp || body);
    the header is id(8) prev(32) hash(32) timestamp(8), the body follows, the last byte is the Bedrock status."""
    return hashlib.sha256(pad32("/LEE/v0.3/Message/Block/") + raw[0:40] + raw[72:80] + raw[80:-1]).digest()


def pda(program, tag, *parts, domain="ferrata-lez/v1/"):
    seed = hashlib.sha256(domain.encode() + tag.encode() + b"".join(parts)).digest()
    return hashlib.sha256(pad32("/LEE/v0.2/AccountId/PDA/") + program + seed).digest()


def beacon_root(ssz):
    """hash_tree_root of a BeaconBlockHeader given as its 112 SSZ bytes."""
    leaves = [ssz[0:8].ljust(32, b"\0"), ssz[8:16].ljust(32, b"\0"), ssz[16:48], ssz[48:80], ssz[80:112]] + [b"\0" * 32] * 3
    while len(leaves) > 1:
        leaves = [hashlib.sha256(leaves[i] + leaves[i + 1]).digest() for i in range(0, len(leaves), 2)]
    return leaves[0]


PRESET_CHAIN = {"mainnet": 1, "sepolia": 11155111, "holesky": 17000, "hoodi": 560048}


def account_bytes(s):
    t = s[7:] if s.startswith("Public/") else s
    h = t[2:] if t[:2] in ("0x", "0X") else t
    if len(h) == 64 and all(c in "0123456789abcdefABCDEF" for c in h):
        return bytes.fromhex(h)
    return b58decode(t)


def instances(site, cfg):
    """The site's config.json instances, else those of the deployment's programs.json."""
    if cfg.get("instances"):
        return cfg["instances"]
    try:
        dep = http_json(urljoin(site, "programs.json"))
    except Exception:  # noqa: BLE001 - no deployment published yet
        return []
    out = []
    for name, i in (dep.get("instances") or {}).items():
        if i.get("program") and i.get("instance_id"):
            out.append({"label": name.capitalize(), "program": b58encode(account_bytes(i["program"])), "instance_id": i["instance_id"][2:] if i["instance_id"].startswith("0x") else i["instance_id"],
                        "account": b58encode(account_bytes(i["instance_account"])) if i.get("instance_account") else None})
    return out


def main():
    site = (sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:18750/").rstrip("/") + "/"
    cfg = http_json(urljoin(site, "config.json"))
    rpc_url = urljoin(site, cfg.get("rpc") or "rpc")
    ok = True

    head = rpc(rpc_url, "getLastBlockId")
    raw = base64.b64decode(rpc(rpc_url, "getBlock", [head]))
    bid, = struct.unpack_from("<Q", raw, 0)
    ts, = struct.unpack_from("<Q", raw, 72)
    ntx, = struct.unpack_from("<I", raw, 80)
    status = ["Pending", "Safe", "Finalized"][raw[-1]]
    same = block_hash(raw) == raw[40:72]
    ok &= same
    print(f"head block #{bid}: {ntx} transaction(s), {status} on Bedrock, timestamp {ts} ms; "
          f"hash {raw[40:72].hex()[:16]}… recomputed: {'equal' if same else 'DIFFERENT'}")

    for inst in instances(site, cfg):
        program = b58decode(inst["program"])
        account = b58decode(inst["account"]) if inst.get("account") else pda(program, "instance", bytes.fromhex(inst["instance_id"]))
        view = rpc(rpc_url, "getAccountView", [{"account_id": b58encode(account), "program_account_id": inst["program"]}])
        state = bytes((view or {}).get("data", {}).get("actor_states", {}).get(inst["program"], []))
        if not state:
            print(f"{inst['label']}: no state at {b58encode(account)}")
            ok = False
            continue
        # InstanceState (borsh): id[32] config[202] active u8 store Vec<u8> applied u64 root_id[32] parent_id[32]
        chain_id, = struct.unpack_from("<Q", state, 32)
        genesis_time, seconds_per_slot = struct.unpack_from("<QQ", state, 32 + 176)
        (n,) = struct.unpack_from("<I", state, 32 + 202 + 1)
        store = state[32 + 202 + 1 + 4: 32 + 202 + 1 + 4 + n]
        # CompactStore: fork u8, then the finalized BeaconBlockHeader (112 bytes of SSZ)
        header = store[1:113]
        slot, = struct.unpack_from("<Q", header, 0)
        root = beacon_root(header)
        print(f"{inst['label']}: chain {chain_id}, finalized slot {slot} (Ethereum time {genesis_time + slot * seconds_per_slot}), "
              f"block root 0x{root.hex()}")
        for api in cfg.get("beacon_apis", {}).get(str(chain_id), []):
            url = urljoin(site, api).rstrip("/") + f"/eth/v1/beacon/headers/{slot}"
            try:
                theirs = http_json(url)["data"]["root"][2:]
                same = theirs == root.hex()
                ok &= same
                print(f"  {urllib.request.urlparse(url).netloc}: {'equal' if same else 'DIFFERENT: 0x' + theirs}")
            except Exception as e:  # noqa: BLE001 - a provider that does not answer is reported, not fatal
                print(f"  {url}: could not read ({e})")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    try:
        main()
    except Exception as e:  # noqa: BLE001
        print(f"error: {e}", file=sys.stderr)
        sys.exit(2)
```

### JavaScript (Node 18+): https://{{HOST}}/agents/ferrata_check.mjs

```js
#!/usr/bin/env node
// Read the Ferrata LEZ test network and check it yourself. Node 18+ (global fetch), no dependencies, read-only.
//
//   node ferrata_check.mjs https://<site>/        // the site serves /config.json and /rpc
//
// 1. The head block: getLastBlockId, getBlock; its hash recomputed from its bytes (SHA-256 over LEZ's encoding).
// 2. Each light-client instance in the site's config.json: its finalized Ethereum beacon header, read from the
//    instance's account (getAccountView) and hashed to its block root (SSZ hash_tree_root).
// 3. That root compared with a public beacon API's root for the same slot.
// Exit 0 if every check holds, 1 if one fails, 2 if something could not be read.

import { createHash } from "node:crypto";

const B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const sha256 = (...parts) => createHash("sha256").update(Buffer.concat(parts.map((p) => Buffer.from(p)))).digest();
const pad32 = (s) => Buffer.concat([Buffer.from(s), Buffer.alloc(32 - s.length)]);
const b58encode = (b) => {
  let n = BigInt("0x" + (Buffer.from(b).toString("hex") || "0"));
  let s = "";
  while (n > 0n) {
    s = B58[Number(n % 58n)] + s;
    n /= 58n;
  }
  let z = 0;
  while (z < b.length && b[z] === 0) z++;
  return "1".repeat(z) + s;
};
const b58decode = (s) => {
  let n = 0n;
  for (const c of s) n = n * 58n + BigInt(B58.indexOf(c));
  let h = n.toString(16);
  if (h.length % 2) h = "0" + h;
  const body = n === 0n ? Buffer.alloc(0) : Buffer.from(h, "hex");
  let z = 0;
  while (z < s.length && s[z] === "1") z++;
  return Buffer.concat([Buffer.alloc(z), body]);
};

async function getJson(url, body) {
  const r = await fetch(url, body ? { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) } : {});
  if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
  return r.json();
}
async function rpc(url, method, params = []) {
  const r = await getJson(url, { jsonrpc: "2.0", id: 1, method, params });
  if (r.error) throw new Error(`${method}: ${JSON.stringify(r.error)}`);
  return r.result;
}

// LEZ v0.4: SHA-256("/LEE/v0.3/Message/Block/" padded to 32 ‖ id ‖ prev ‖ timestamp ‖ body); the last byte is the
// Bedrock status, outside the hash.
const blockHash = (raw) => sha256(pad32("/LEE/v0.3/Message/Block/"), raw.subarray(0, 40), raw.subarray(72, 80), raw.subarray(80, raw.length - 1));
const pda = (program, tag, ...parts) => sha256(pad32("/LEE/v0.2/AccountId/PDA/"), program, sha256(Buffer.from("ferrata-lez/v1/"), Buffer.from(tag), ...parts));
function beaconRoot(ssz) {
  const leaf = (b) => Buffer.concat([b, Buffer.alloc(32 - b.length)]);
  let l = [leaf(ssz.subarray(0, 8)), leaf(ssz.subarray(8, 16)), ssz.subarray(16, 48), ssz.subarray(48, 80), ssz.subarray(80, 112), Buffer.alloc(32), Buffer.alloc(32), Buffer.alloc(32)];
  while (l.length > 1) l = Array.from({ length: l.length / 2 }, (_, i) => sha256(l[2 * i], l[2 * i + 1]));
  return l[0];
}

const PRESET_CHAIN = { mainnet: 1, sepolia: 11155111, holesky: 17000, hoodi: 560048 };
const accountBytes = (s) => {
  const t = String(s).replace(/^Public\//, "");
  return /^(0x)?[0-9a-fA-F]{64}$/.test(t) ? Buffer.from(t.replace(/^0x/, ""), "hex") : b58decode(t);
};

// The site's config.json instances, else those of the deployment's programs.json.
async function instances(site, cfg) {
  if ((cfg.instances || []).length) return cfg.instances;
  let dep;
  try {
    dep = await getJson(new URL("programs.json", site));
  } catch {
    return [];
  }
  return Object.entries(dep.instances || {})
    .filter(([, i]) => i.program && i.instance_id)
    .map(([name, i]) => ({ label: name[0].toUpperCase() + name.slice(1), program: b58encode(accountBytes(i.program)), instance_id: i.instance_id.replace(/^0x/, ""), account: i.instance_account ? b58encode(accountBytes(i.instance_account)) : undefined }));
}

async function main() {
  const site = new URL((process.argv[2] || "http://127.0.0.1:18750/").replace(/\/?$/, "/"));
  const cfg = await getJson(new URL("config.json", site));
  const rpcUrl = new URL(cfg.rpc || "rpc", site).href;
  let ok = true;

  const head = await rpc(rpcUrl, "getLastBlockId");
  const raw = Buffer.from(await rpc(rpcUrl, "getBlock", [Number(head)]), "base64");
  const same = blockHash(raw).equals(raw.subarray(40, 72));
  ok &&= same;
  console.log(`head block #${raw.readBigUInt64LE(0)}: ${raw.readUInt32LE(80)} transaction(s), ${["Pending", "Safe", "Finalized"][raw[raw.length - 1]]} on Bedrock; hash ${raw.subarray(40, 72).toString("hex").slice(0, 16)}… recomputed: ${same ? "equal" : "DIFFERENT"}`);

  for (const inst of await instances(site, cfg)) {
    const program = b58decode(inst.program);
    const account = inst.account ? b58decode(inst.account) : pda(program, "instance", Buffer.from(inst.instance_id, "hex"));
    const view = await rpc(rpcUrl, "getAccountView", [{ account_id: b58encode(account), program_account_id: inst.program }]);
    const state = Buffer.from((view && view.data.actor_states[inst.program]) || []);
    if (!state.length) {
      console.log(`${inst.label}: no state at ${b58encode(account)}`);
      ok = false;
      continue;
    }
    // InstanceState (borsh): id[32] config[202] active u8 store Vec<u8> ...; CompactStore: fork u8, header[112]
    const chainId = state.readBigUInt64LE(32);
    const header = state.subarray(32 + 202 + 1 + 4 + 1, 32 + 202 + 1 + 4 + 1 + 112);
    const slot = header.readBigUInt64LE(0);
    const root = beaconRoot(header).toString("hex");
    console.log(`${inst.label}: chain ${chainId}, finalized slot ${slot}, block root 0x${root}`);
    for (const api of (cfg.beacon_apis || {})[String(chainId)] || []) {
      const url = new URL(`${api.replace(/\/$/, "")}/eth/v1/beacon/headers/${slot}`, site);
      try {
        const theirs = (await getJson(url)).data.root.slice(2);
        ok &&= theirs === root;
        console.log(`  ${url.host}: ${theirs === root ? "equal" : `DIFFERENT: 0x${theirs}`}`);
      } catch (e) {
        console.log(`  ${url.host}: could not read (${e.message})`);
      }
    }
  }
  process.exit(ok ? 0 : 1);
}

main().catch((e) => {
  console.error(`error: ${e.message}`);
  process.exit(2);
});
```

## Errors

| Where | Code | Meaning |
|---|---|---|
| JSON-RPC | -32601 | method not found (or not allowed by this network's gateway) |
| JSON-RPC | -32602 | invalid params (wrong count or type; account ids are base58, hashes hex) |
| JSON-RPC | -31900 | the mempool is full: retry later |
| JSON-RPC | -31901 | a private transaction's fee reserve is below the head's: price it again from `getFeeState` |
| Attestation program | 1-61, 80-99, 100-199, 200-219 | its exit codes, each named in the IDL's `errors` (e.g. 130 `PredicateFailed`, 211 `WRONG_INSTANCE`) |
| Bridge program | 240-255 | its exit codes, in the bridge IDL's `errors` |

A refused transaction is still included and charged: it appears in its block's `reverted` list.
