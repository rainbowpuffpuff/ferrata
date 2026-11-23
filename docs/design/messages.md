# Encrypted wallet-to-wallet messages on the Ferrata LEZ v0.4 test network

**Prototype, unaudited, test network only.** Two programs:

- **`ferrata_msg_v2`** (this design): `lez/guests/msg-v2`, its logic in `lez/msg-v2-wire`, IDL
  `idl/ferrata_msg_v2.idl.json`, image `0x480f5c4e…4f59` (`docs/image-ids.md`). Not yet deployed.
- **`ferrata_msg`** (v1): `lez/guests/msg`, `lez/msg-wire`, `idl/ferrata_msg.idl.json`, image `0x04ae1988…5277`, live
  and immutable on our test network since 23 November 2026 (header `ExZxU36vgBErfDHpnsM97euf4ubx7gL8WpX7DFsYrVTe`). An
  independent review (23 November 2026) found it sound but costly in ways that the immutable program cannot change
  (§ "v1"). v2 replaces it; the clients read v1 inboxes and no longer send there.

Clients: the JS library `deploy/testnet/web/msg/ferrata-msg.js` (keys, envelopes, borsh, PDAs, preflight),
`client.js` (RPC, signing, outcomes), the CLI `ferrata-msg.mjs`.

## What LEZ v0.4.0-rc3 already has, and why neither program uses it

rc3 has **sealed messages** (`lee/state_machine/core/src/sealing.rs`): an asynchronous message emitted by a *private*
execution is stored as a commitment plus a note encrypted to the destination private account's viewing key, so even
sender and recipient are hidden. It carries a program message, exists only for privacy-preserving transactions (one
RISC Zero proof per message) and needs a private recipient: the base for a later "sealed mode", not for a chat that
answers in seconds from a browser. Both programs are public programs whose state holds only ciphertexts.

## Keys

A recipient publishes a messaging key at its own profile: `Publish { key }`, signed by the account. LEZ public accounts
sign with BIP-340 on secp256k1, not an encryption key, so the messaging key is separate but **derived from the wallet's
account secret** (no second backup): `seed = HKDF-SHA256(ikm = account secret, salt = "ferrata-msg/v1", info = "keys"
|| epoch)`, then an X25519 key pair (`seed[0..32]`) and an **ML-KEM-768** key pair (FIPS 203, from `seed[32..96]`).
Rotation publishes the next epoch; old messages still decrypt with the old epoch's keys, re-derived from the same
secret. Every epoch derives from that one secret, so **rotation bounds a leaked epoch key, not a leaked wallet**: whoever
holds the account secret opens every message the account ever received.

## Envelope (fresh key per message, authenticated encryption, hybrid post-quantum)

```
v2: 0x03 | epoch u32 | epk 32 | nonce 24 | XChaCha20-Poly1305(k, nonce, plaintext, aad)                (X25519)
    0x04 | epoch u32 | epk 32 | ML-KEM-768 ciphertext 1088 | nonce 24 | XChaCha20-Poly1305(...)       (hybrid)
k   = HKDF-SHA256(ikm = X25519(esk, rx) [|| ML-KEM shared secret], salt = "ferrata-msg/v1",
                  info = "envelope" || SHA-256(header || rx || sender id || recipient id))
aad = header || sender id || recipient id          plaintext = kind u8 | sent_ms u64 | len u16 | UTF-8 | zeros
```
The sender draws a fresh ephemeral X25519 key and a fresh ML-KEM encapsulation per message: a later leak of the
sender's keys reveals nothing it sent. Hybrid whenever the recipient published an ML-KEM key (our clients always do;
the program refuses an X25519-only envelope to such a key, `DOWNGRADE`): ciphertexts stay public on chain, so "record
now, decrypt later" is the threat, and the hybrid holds while either X25519 or ML-KEM holds. The KDF binds the
ephemeral key, the ML-KEM ciphertext, the recipient key and both accounts; the AEAD binds the envelope to sender and
recipient, so an envelope copied and re-sent by another account does not open. The sender is authenticated by LEZ:
the program records the account whose signed `Send` made the delivery. v1's envelopes (versions 1 and 2) put the whole
transcript in `info`: 1,253 bytes for a hybrid one, over the 1,024 bytes OpenSSL's HKDF (and so node:crypto's) accepts,
so a second client on those stacks could not implement it (review finding L1). v2 hashes it (40 bytes); the library
still opens v1's envelopes. `deploy/testnet/web/msg/test/independent.test.mjs` checks the library against an
implementation sharing none of its code (node:crypto, a hand-written HChaCha20) and against OpenSSL 4.0.3's ML-KEM-768
values (`test/kat.json`).

## Storage and delivery (v2): one account per message

Three kinds of actor of the program `P`, each written only by what changes it:

- **profile** `(account, P)`: key, policy, contacts and block lists (at most 64 each), and the sending record: the
  declared times of the last sends within an hour and a send counter. Only the owner's own signed transactions write it.
- **inbox** `pda(P, SHA-256("ferrata-msg/v2/" || "inbox" || owner))`: the index (number, sender, sender's counter,
  time, length, contact or not), opened by the owner's first `Publish`.
- **envelope** `pda(P, SHA-256("ferrata-msg/v2/" || "envelope" || recipient || sender || counter u64))`: one
  ciphertext, written once by delivery, emptied when the owner deletes it (the empty account itself stays: below).

A send is one transaction signed by the sender: `Send { to, now_ms, seq, body }` at the sender's profile (`seq` must be
its counter, so a concurrent send is refused cheaply, `STALE_SEQ`; rate; clock; size), then `Deliver` at the recipient's
profile (key, block list, policy, version, epoch; read, not written), then `Admit` at the recipient's inbox (capacity,
stranger limits; the entry; event `Delivered`) and `Store` at the new envelope. The program creates both PDAs by naming
their seeds (rc3's `admit` takes them), so the transaction declares the four actors and carries no evidence; the sender
knows the envelope's address beforehand from its own counter. Every refusal is a coded exit and writes nothing. A send
addressed to one of the program's own accounts (an inbox or an envelope) is refused `BAD_STATE`: their state is not a
profile (review N3; the clients refuse such an address before sending).

What this changes (review findings M2, M3; measured in `lez/results/messages-findings.md`, the same scenario on both
programs): the owner's own actions read and write only the owner's profile, so strangers' messages no longer change
what they cost (SetPolicy 9,092 cycles with or without 32 KiB of strangers' messages; v1: 9,292 → 338,022, 36.4x;
the owner's own send 74,798 either way; v1: 34,758 → 388,388, 11.2x); a send touches two profiles, the index and one
new envelope, so its cost is bounded by the lists' and the index's limits, not by two inboxes (worst case 407,277 cycles:
both profiles at full lists, the recipient's inbox one envelope from full; v1: 1,550,595 for a send between two
near-full inboxes). The price: a send to an empty inbox runs four guests instead of two (74,798 cycles against v1's
34,758). Deleting costs about 30,000-45,000 cycles per envelope (the envelope's state is read to empty it): the owner
pays to clear spam, strangers can refill up to their limits, and the owner's way out is `Block` or contacts-only.
**Every delivered message leaves an account behind for good** (review N4): deleting empties the envelope's state, which
LEZ prunes, but not the account entry (rc3 as rc1; `lez/harness/tests/messages_v2.rs` asserts it), so the network's
state grows by one empty account per message ever delivered. The sender's fee pays for it; nothing reclaims it.

## Spam limits (enforced by the program)

- No key published: refused (`NO_KEY`), so no state is created at an account that did not ask for messages.
- Envelope length one of 512, 1,024, 2,048, 4,096 bytes, a hybrid one at least 2,048 (a hybrid header is 1,149 bytes;
  v1 stored hybrid-labelled junk in 512-byte buckets, review L6), version 3 or 4, the recipient's current epoch.
- An inbox holds at most 32 messages and 64 KiB. A non-contact holds at most **1** unread message (v1: 2), and
  non-contacts at most **8** together, so strangers can never take more than 8 slots and 32 KiB: contacts always keep at
  least 24 slots and 32 KiB that no stranger can fill. Contacts themselves can fill the inbox (`INBOX_FULL` applies to
  them too). The stranger limit stands at delivery: a sender later made a contact keeps the slot it took as a stranger
  until the owner deletes the message.
- **Eight funded accounts can occupy every stranger slot** (v1: four) and keep any other stranger out
  (`STRANGERS_FULL`) until the owner deletes or blocks them; they can refill as often as they pay fees. A bounded open
  inbox cannot avoid this; the owner's tools are `Block`, `Delete`/`Clear` and contacts-only.
- A sender declares `now_ms`; its transaction is valid only in blocks stamped `[now_ms, now_ms + 10 min)`, and declared
  times never go backwards. **At most 30 sends whose declared times lie within any hour**: a sliding window over the
  last 30 declared times. Declared times may trail the block by up to 10 minutes, so at most 30 sends land in any 50
  minutes of block time. v1 used a fixed window that reset an hour after its first send: 59 sends landed within 90
  seconds across that boundary (review L7).
- Block list and contacts-only bind public senders. **A private sender is not bound by them**: in rc3 a private actor's
  message is presented under an alias the prover chooses (`execution_state.rs`, `sender()`), so a privacy-preserving
  `Send` could arrive from a fresh id each time; the stranger limits and contacts-only still bind it, `Block` does not
  (review L11; not demonstrated: it needs a proof).
- Every refused transaction still pays its fee (the CLI and the wallet check every rule first and send nothing that
  would be refused); a transaction outside its validity window is charged its whole gas limit (LEZ's rule), so clients
  set `now_ms` a few seconds behind their clock and gas limits near the measured maxima (`client.js` `GAS_V2`: a send
  600,000; an edit 200,000 plus 18,000 per id it names, review N2; checked by `lez/harness/tests/messages_v2.rs`
  against every applied transaction of its scenario, the worst cases included).
- A contacts or block edit compares each id with up to 64 list entries, byte by byte until they differ. Account ids
  (hashes) differ at once: removing 64 and adding 64 cost up to 307,004 cycles. Ids sharing 31 bytes, which only an owner can
  put in its own lists, cost up to 1,633,147 for the same edit; the limit covers both. The clients refuse an edit that
  would overflow a list (`LIST_FULL`) before sending: the limit does not bound a refused edit's comparisons.

## What is public, and what padding hides

Readable on chain: the sender (it signs and pays) and the recipient (the actors written), the block and its time, the
envelope's version and bucket; the recipient's published key, policy, contacts and block lists; the inbox's entries
(numbers, senders, their counters, declared times, lengths, contact flags) and so its size and every deletion (an
emptied envelope account); each sender's recent declared send times and its send count. A `Delivered` event repeats
sender, recipient, number and bucket. Hidden: the text and its exact length (padding: a 1-byte and a 380-byte text are
the same 512-byte X25519 envelope; a hybrid envelope's smallest bucket, 2,048 bytes, holds up to 872 bytes of text, its
largest 2,920), the sending time inside the plaintext, and the content type. Contacts and block lists are public by
design (the program must read them); a user who wants them hidden keeps the policy open and blocks.

## Threat model of the clients

The RPC is trusted for reads. It learns which inboxes a client reads, and **it can substitute a recipient's published
key** (the client encrypts to whatever key the RPC returns). Fingerprints counter it: `ferrata-msg keygen` prints the
account's (first 10 bytes of SHA-256 of the published key, base58), `send` prints the recipient's before sending and
refuses on mismatch with `--expect-key FINGERPRINT`; the wallet should pin the first fingerprint it sees per contact
(NEEDS for the wallet). The program checks nothing about keys beyond their form, so this stays a client duty.

## v1 (`ferrata_msg`, live and immutable)

v1 keeps a whole inbox, the owner's lists and its counters in one actor state that every action decodes and re-encodes.
So, as measured on the same scenarios (`lez/results/messages-findings.md`): strangers raise the recipient's own costs
(36.4x for SetPolicy, 11.2x for a send, with 32 KiB of strangers' messages); a send between two near-full inboxes costs
1,550,595 cycles, over the 1,500,000 the CLI and the wallet used at the review (clients still sending there need at least
2,000,000: `client.js` `GAS_V1`); hybrid-labelled 512-byte junk is stored; its rate limit is a fixed window; four
accounts fill its stranger slots. Its code reaches 19 of its 20 exit codes in `lez/harness/tests/messages.rs`
(`BAD_STATE` is defensive). Its inboxes stay readable: `ferrata-msg inbox --legacy`.

## Measured

- `lez/results/messages-v2.md` (`harness/tests/messages_v2.rs`, LEZ's state machine and fee settlement, RISC0 dev
  mode): 284 transactions, 248 applied, 34 refused by the program with a code, 2 reverted by LEZ (validity window),
  none of the refused or reverted changing any state; 21 of 21 exit codes (`BAD_STATE` by a send addressed to one of
  the program's own accounts). Largest applied: send 407,277 cycles; an edit of 128 ids 307,004 with account ids and
  1,633,147 with ids sharing 31 bytes; publish 135,667 (a first key on full lists, opening the inbox); clear of 32
  envelopes 1,148,014 (by an owner with full lists); clear of 16 envelopes of 4,096 B 718,983.
- `lez/results/messages-findings.md` (`harness/tests/messages_findings.rs`): each review finding measured on both
  programs; every check fails on v1 and passes on v2.
- `deploy/testnet/web/msg/test/results/local-2/record.txt` (`test/box.sh e2e`, LEZ v0.4.0-rc3's own sequencer,
  2-second blocks): 27 of 27 checks; deployed by LEZ's CLI in 123 s (3 segments and a header); a send to an inbox
  account refused before sending; a hybrid 2,048-byte send in its block 3.6 s after the CLI started, fee 1,022,264
  units; a stranger's second message forced past the client's check reverted in its block, fee 724,328 units, nothing
  stored; bulk block-list edits through the library (63 ids, then 64 removed and 64 added) applied within its limits. LEZ's RPC serves which transactions of a block
  reverted, not their exit codes: the codes are the harness's.

## CROPS review (ethskills `crops`)

**Censorship resistance:** no server, relay or indexer in the path; the program has no admin key, pause or allowlist,
and is deployed immutable (both e2e runs did, and the public deployment is asked to); the gatekeeper is the test
network's single sequencer (as for every program on it). **Open source:** program, IDL, library, CLI and tests in this
repository; the crypto from `@noble` packages pinned by integrity hash, audited except `@noble/post-quantum` (ML-KEM),
which calls itself auditable, not audited (`vendor/NOTICE` carries their licences). **Privacy:** contents and exact
length hidden; the metadata above public; reads go to an RPC the user chooses, which learns which inbox is read and is
trusted for keys (fingerprints). **Security:** keys never leave the client; nothing to lose but messages; the walkaway
test passes: with a LEZ RPC and this library anyone reads and sends.

## Next

A sealed mode on rc3's sealed async messages (sender and recipient hidden) once private proving runs on the test
network's prover; postage paid to the recipient by strangers (spam that pays the one it costs); first-seen key pinning
in every client.
