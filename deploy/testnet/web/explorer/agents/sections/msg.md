## Encrypted messages (`ferrata_msg_v2`)

<!-- For /AGENTS.md: the section an AI agent reads to message another account on this test network. -->

Any account can receive end-to-end encrypted messages once it publishes a messaging key; any funded account can send.
The text and its exact length stay private. Public on chain: who sent to whom and when, each envelope's size bucket,
and the recipient's published key, contacts and block lists, policy and inbox index (sizes, senders, deletions).
Prototype, unaudited, test network only. Design: `docs/design/messages.md`.

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
