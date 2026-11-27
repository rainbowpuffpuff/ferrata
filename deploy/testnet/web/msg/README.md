# Encrypted messages on the Ferrata LEZ v0.4 test network

**Prototype, unaudited, test network only.** Design, threat model and what is public:
[`docs/design/messages.md`](../../../../docs/design/messages.md).

| piece | where |
|---|---|
| program `ferrata_msg_v2` (LEZ v0.4.0-rc3 guest) | [`lez/guests/msg-v2`](../../../../lez/guests/msg-v2), logic and types in [`lez/msg-v2-wire`](../../../../lez/msg-v2-wire/src/lib.rs) |
| program `ferrata_msg` (v1, live, read only here) | [`lez/guests/msg`](../../../../lez/guests/msg), [`lez/msg-wire`](../../../../lez/msg-wire/src/lib.rs) |
| IDLs (SPEL format) | [`idl/ferrata_msg_v2.idl.json`](../../../../idl/ferrata_msg_v2.idl.json), [`idl/ferrata_msg.idl.json`](../../../../idl/ferrata_msg.idl.json), written by [`idl/tools/msg_idl.py`](../../../../idl/tools/msg_idl.py); `python3 -I idl/check.py` decodes every vector with them |
| harness tests (LEZ state machine + fee settlement) | [`lez/harness/tests/messages_v2.rs`](../../../../lez/harness/tests/messages_v2.rs), [`messages_findings.rs`](../../../../lez/harness/tests/messages_findings.rs), [`messages.rs`](../../../../lez/harness/tests/messages.rs) (v1); records in [`lez/results/`](../../../../lez/results/messages-v2.md) (`messages-v2.md`, `messages-findings.md`, `messages.md`) |
| JS library (browser and node) | [`ferrata-msg.js`](ferrata-msg.js) (keys, envelopes, borsh, PDAs, preflight), [`client.js`](client.js) (RPC, signing, outcomes; uses `../wallet/lez.js`) |
| CLI | [`ferrata-msg.mjs`](ferrata-msg.mjs) |
| tests | `node --test deploy/testnet/web/msg/test/*.test.mjs` (library; the CLI process offline against loopback RPC stand-ins; an independent implementation against OpenSSL's ML-KEM values), `cargo test -p ferrata-lez-msg-v2-wire -p ferrata-lez-msg-wire` (logic, vectors), [`test/box.sh`](test/box.sh) (LEZ rc3's own sequencer, end to end; record [`test/results/local-2/`](test/results/local-2/record.txt)) |

## CLI

Node 20 or later (checked on 20.11 and 22.12), from this directory, nothing to install: the crypto is vendored in
`vendor/crypto.js` (built by `vendor/build.sh` from pinned `@noble` packages; licences in `vendor/NOTICE`).

```sh
export FERRATA_RPC=<the network's RPC URL>      # or --rpc, or config.json's "rpc" (the host serving this directory fills it)
export FERRATA_KEY_FILE=~/my-lez-key.hex        # 64 hex digits (a raw LEZ key) or a BIP-39 mnemonic (--path /0)
export FERRATA_MSG_PROGRAM=<ferrata_msg_v2 id>  # until config.json names it
./ferrata-msg.mjs keygen                         # account, messaging keys (derived from your key), fingerprint
./ferrata-msg.mjs publish                        # put the hybrid key on chain; this also opens your inbox
./ferrata-msg.mjs send <account> hello --expect-key <their fingerprint>
./ferrata-msg.mjs inbox                          # read and decrypt; --legacy reads your v1 inbox
./ferrata-msg.mjs status <account>               # anyone's public messaging status
./ferrata-msg.mjs delete <n>... | clear <n> | contact|uncontact|block|unblock <account> | policy open|contacts
```

`send` prints the recipient's key fingerprint before sending; with `--expect-key` it refuses any other key (the RPC
could otherwise substitute one). It checks every rule of the program first (`preflightSend`) and refuses locally with
the program's code name, so a message that would be refused costs nothing; `--force` sends anyway. Every signed command
reports what its block recorded: applied, or reverted (refused; the fee is charged, nothing changes). LEZ's RPC does not
serve exit codes. The key file never leaves the machine: only signed transactions and public reads go to the RPC.

## For the wallet

```js
import * as M from '/msg/ferrata-msg.js';
import * as C from '/msg/client.js';                                // uses /wallet/lez.js
await C.publish(seq, { node, program });                             // first time: opens the inbox too
const r = await C.send(seq, { node, program, to, text, expectKey }); // r.refused (with r.why) when nothing was sent;
                                                                     // otherwise r.block and r.reverted
const { list } = await C.messages(seq, { program, node });           // decrypted in the page
await C.remove(seq, { node, program, ns: [n] });                     // or { upto: n }; envelopes emptied
await C.contacts(seq, { node, program, add: [id] }); await C.block(seq, { node, program, add: [id] });
await C.setPolicy(seq, { node, program, policy: 'contactsOnly' });
```

`C.send` sets `now_ms` a few seconds behind the page's clock and never before the account's last send, and uses the
gas limit the harness checks (`GAS_V2`); `M.fingerprint(key)` beside the recipient, pinned the first time it is seen,
answers the RPC's power to substitute keys. v1 inboxes: `C.legacyMailbox` and `M.readLegacyInbox`.

## Deployment

Build `lez/guests/msg-v2` (`cargo build -p methods-msg --locked` in `lez/`) and check its sha256 and image id against
`docs/image-ids.md`; deploy it immutable with LEZ's CLI exactly as `test/box.sh` does (a header and three segments: the
count LEZ's CLI names from the program's user ELF); write the header account into `config.json` (`"program"`, and the
network's RPC URL into `"rpc"`); serve this directory at `/msg/` beside `/wallet/`. No server, indexer or key custody;
fees are the sender's; the faucet funds new accounts.
