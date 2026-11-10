# Privacy suite: what is public at each step (generated)

Every field the collectors can name, across the 20 records of each step: constant, or how many distinct values; its class when it varies; the item of the proposal's section B6 it falls under (`-`: not in B6).

## deposit (A: private destinations)

| field | bytes | across records | class | what | §B6 |
|---|---:|---|---|---|---|
| `tx.from` | 20 | 20 distinct | the user's own public address | the depositor (or the submitting relayer) | depositor |
| `tx.value` | 32 | constant | constant |  | denomination |
| `calldata.selector` | 4 | constant | constant |  | - |
| `calldata.denomIndex` | 32 | constant | constant |  | denomination |
| `calldata.commitment` | 32 | 20 distinct | fresh randomness | LEZ: the private account's new commitment | C |
| `calldata.index` | 32 | constant | constant |  | - |
| `event.Deposit.sig` | 32 | constant | constant |  | - |
| `event.Deposit.depositor` | 32 | 20 distinct | the user's own public address | the depositor (or the submitting relayer) | depositor |
| `event.Deposit.token` | 32 | constant | constant |  | token |
| `event.Deposit.index` | 32 | constant | constant |  | - |
| `event.Deposit.leafIndex` | 32 | 20 distinct | time / order | the deposit's position (its order) | - |
| `event.Deposit.amount` | 32 | constant | constant |  | denomination |
| `event.Deposit.commitment` | 32 | 20 distinct | fresh randomness | LEZ: the private account's new commitment | C |
| `block.number` | 32 | 20 distinct | time / order | time | time |
| `block.timestamp` | 32 | 20 distinct | time / order | time | time |

## claim, private (default)

| field | bytes | across records | class | what | §B6 |
|---|---:|---|---|---|---|
| `public_context.actors[] (members in every record)` | 64 | constant (6) | constant |  | - |
| `public_context.actors[] (other members)` | 64 | 20 distinct | nullifier | = `derived.nullifier_account`: the nullifier account, a PDA of N | - |
| `boundary[].out.from` | 32 | 20 distinct | fresh randomness | LEZ: the sender's one-time alias (where it varies) | a one-time fee alias with a fixed reserve |
| `boundary[].out.to (members in every record)` | 32 | constant (3) | constant |  | a one-time fee alias with a fixed reserve |
| `boundary[].out.message.Read` | 0 | constant | constant |  | the root read |
| `boundary[].in.from` | 32 | constant | constant | LEZ: the sender's one-time alias (where it varies) | a one-time fee alias with a fixed reserve |
| `boundary[].in.to` | 32 | 20 distinct | fresh randomness | LEZ: the sender's one-time alias (where it varies) | a one-time fee alias with a fixed reserve |
| `boundary[].in.message.StateReply.state` | 117 | constant | constant |  | the root read |
| `boundary[].out.message.Issue.config_id` | 32 | constant | constant |  | token |
| `boundary[].out.message.Issue.token` | 20 | constant | constant |  | token |
| `boundary[].out.message.Issue.nullifier` | 32 | 20 distinct | nullifier | N | N |
| `boundary[].out.message.Issue.amount` | 16 | constant | constant |  | denomination |
| `boundary[].out.message.Issue.range` | 24 | constant | constant |  | - |
| `boundary[].out.message.Issue.leaf_count` | 8 | constant | constant |  | the root read |
| `boundary[].out.message.Issue.relayer` | 32 | constant | constant |  | - |
| `boundary[].out.message.Issue.time_ms` | 8 | 20 distinct | time / order | time | time |
| `boundary[].out.message.native.Credit` | 16 | constant | constant |  | denomination |
| `private_actions[].nullifier` | 32 | 20 distinct | nullifier | LEZ: the private account's nullifier (main action; fee fallback) | - |
| `private_actions[].commitment_root` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |
| `private_actions[].commitment` | 32 | 20 distinct | fresh randomness | LEZ: the private account's new commitment | - |
| `private_actions[].note.ciphertext` | 247 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `private_actions[].note.epk` | 1088 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `fee.refund_to` | 32 | constant | constant |  | - |
| `fee.fallback.action.nullifier` | 32 | 20 distinct | nullifier | LEZ: the private account's nullifier (main action; fee fallback) | - |
| `fee.fallback.action.commitment_root` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |
| `fee.fallback.action.commitment` | 32 | 20 distinct | fresh randomness | LEZ: the private account's new commitment | - |
| `fee.fallback.action.note.ciphertext` | 137 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `fee.fallback.action.note.epk` | 1088 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `fee.fallback.boundary[].out.from` | 32 | 20 distinct | fresh randomness | LEZ: the sender's one-time alias (where it varies) | a one-time fee alias with a fixed reserve |
| `fee.fallback.boundary[].out.to` | 32 | constant | constant | LEZ: the sender's one-time alias (where it varies) | a one-time fee alias with a fixed reserve |
| `fee.fallback.boundary[].out.message.native.Credit` | 16 | constant | constant |  | denomination |
| `validity` | 4 | constant | constant | time | time |
| `witness_set` | 4541 | 20 distinct | fresh randomness | the proof (dev mode: a fake receipt) or the submitter's signature | - |
| `derived.nullifier_account` | 32 | 20 distinct | nullifier | the nullifier account, a PDA of N | N |
| `block.id` | 8 | 20 distinct | time / order | time | time |
| `block.timestamp` | 8 | 20 distinct | time / order | time | time |
| `commitment_root.before` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |
| `commitment_root.after` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |

## claim, plaintext

| field | bytes | across records | class | what | §B6 |
|---|---:|---|---|---|---|
| `public_context.actors[] (members in every record)` | 64 | constant (5) | constant |  | - |
| `public_context.actors[] (other members)` | 64 | 20 distinct | UNCLASSIFIED |  | - |
| `root.to` | 32 | constant | constant |  | - |
| `root.message.Claim.config_id` | 32 | constant | constant |  | - |
| `root.message.Claim.leaf_count` | 8 | constant | constant |  | - |
| `root.message.Claim.witness.depositor` | 20 | 20 distinct | UNCLASSIFIED |  | - |
| `root.message.Claim.witness.token` | 20 | constant | constant |  | - |
| `root.message.Claim.witness.amount` | 32 | constant | constant |  | - |
| `root.message.Claim.witness.leaf_index` | 8 | 20 distinct | UNCLASSIFIED |  | - |
| `root.message.Claim.witness.path[] (members in every record)` | 32 | constant (27) | constant |  | - |
| `root.message.Claim.witness.path[] (other members)` | 32 | 20 distinct | UNCLASSIFIED |  | - |
| `root.message.Claim.witness.s` | 32 | 20 distinct | UNCLASSIFIED |  | - |
| `root.message.Claim.witness.kind` | 1 | constant | constant |  | - |
| `root.message.Claim.witness.dest` | 32 | 20 distinct | UNCLASSIFIED |  | - |
| `root.message.Claim.range` | 24 | constant | constant |  | - |
| `root.message.Claim.relayer` | 32 | constant | constant |  | - |
| `root.message.Claim.time_ms` | 8 | 20 distinct | time / order | time | - |
| `fee` | 65 | constant | constant |  | - |
| `nonces` | 52 | 20 distinct | time / order | time | - |
| `witness_set` | 100 | 20 distinct | fresh randomness | the proof (dev mode: a fake receipt) or the submitter's signature | - |
| `derived.nullifier_account` | 32 | 20 distinct | nullifier | the nullifier account, a PDA of N | - |
| `block.id` | 8 | 20 distinct | time / order | time | - |
| `block.timestamp` | 8 | 20 distinct | time / order | time | - |
| `commitment_root.before` | 32 | constant | constant | LEZ: the commitment-set root (state the proof read) | - |
| `commitment_root.after` | 32 | constant | constant | LEZ: the commitment-set root (state the proof read) | - |

## burn, private holding

| field | bytes | across records | class | what | §B6 |
|---|---:|---|---|---|---|
| `public_context.actors[] (members in every record)` | 64 | constant (3) | constant |  | - |
| `public_context.actors[] (other members)` | 64 | 20 distinct | fresh randomness | = `derived.burn_record_account`: the burn record account, a PDA of H(note) | - |
| `boundary[].out.from` | 32 | 20 distinct | fresh randomness | LEZ: the sender's one-time alias (where it varies) | a one-time fee alias with a fixed reserve |
| `boundary[].out.to (members in every record)` | 32 | constant (2) | constant |  | a one-time fee alias with a fixed reserve |
| `boundary[].out.message.Burn.config_id` | 32 | constant | constant |  | token |
| `boundary[].out.message.Burn.r` | 32 | 20 distinct | fresh randomness | the burn note (tag, encrypted recipient, mac, COut) | the burn note |
| `boundary[].out.message.Burn.memo` | 20 | 20 distinct | fresh randomness | the burn note (tag, encrypted recipient, mac, COut) | the burn note |
| `boundary[].out.message.Burn.mac` | 16 | 20 distinct | fresh randomness | the burn note (tag, encrypted recipient, mac, COut) | the burn note |
| `boundary[].out.message.Burn.token` | 20 | constant | constant |  | token |
| `boundary[].out.message.Burn.amount` | 32 | constant | constant |  | denomination |
| `boundary[].out.message.Burn.c_out` | 32 | 20 distinct | fresh randomness | the burn note (tag, encrypted recipient, mac, COut) | the burn note |
| `boundary[].out.message.native.Credit` | 16 | constant | constant |  | denomination |
| `private_actions[].nullifier` | 32 | 20 distinct | nullifier | LEZ: the private account's nullifier (main action; fee fallback) | - |
| `private_actions[].commitment_root` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |
| `private_actions[].commitment` | 32 | 20 distinct | fresh randomness | LEZ: the private account's new commitment | - |
| `private_actions[].note.ciphertext` | 179 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `private_actions[].note.epk` | 1088 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `fee.refund_to` | 32 | constant | constant |  | - |
| `fee.fallback.action.nullifier` | 32 | 20 distinct | nullifier | LEZ: the private account's nullifier (main action; fee fallback) | - |
| `fee.fallback.action.commitment_root` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |
| `fee.fallback.action.commitment` | 32 | 20 distinct | fresh randomness | LEZ: the private account's new commitment | - |
| `fee.fallback.action.note.ciphertext` | 247 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `fee.fallback.action.note.epk` | 1088 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `fee.fallback.boundary[].out.from` | 32 | 20 distinct | fresh randomness | LEZ: the sender's one-time alias (where it varies) | a one-time fee alias with a fixed reserve |
| `fee.fallback.boundary[].out.to` | 32 | constant | constant | LEZ: the sender's one-time alias (where it varies) | a one-time fee alias with a fixed reserve |
| `fee.fallback.boundary[].out.message.native.Credit` | 16 | constant | constant |  | denomination |
| `validity` | 4 | constant | constant | time | time |
| `witness_set` | 3993 | 20 distinct | fresh randomness | the proof (dev mode: a fake receipt) or the submitter's signature | - |
| `derived.burn_record_account` | 32 | 20 distinct | fresh randomness | the burn record account, a PDA of H(note) | the burn note |
| `block.id` | 8 | 20 distinct | time / order | time | time |
| `block.timestamp` | 8 | 20 distinct | time / order | time | time |
| `commitment_root.before` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |
| `commitment_root.after` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |

## release, NOut of the release statement

| field | bytes | across records | class | what | §B6 |
|---|---:|---|---|---|---|
| `tx.from` | 20 | constant | constant | the depositor (or the submitting relayer) | the relayer |
| `tx.value` | 32 | constant | constant |  | denomination |
| `calldata.selector` | 4 | constant | constant |  | - |
| `calldata.request.token` | 32 | constant | constant |  | token |
| `calldata.request.denomIndex` | 32 | constant | constant |  | denomination |
| `calldata.request.recipient` | 32 | 20 distinct | the user's own public address | the release recipient | recipient |
| `calldata.request.relayer` | 32 | constant | constant |  | the relayer |
| `calldata.request.relayFee` | 32 | constant | constant |  | - |
| `calldata.request.burnRoot` | 32 | constant | constant |  | the root |
| `calldata.request.nullifier` | 32 | 20 distinct | nullifier | NOut | N_out |
| `calldata.seal.offset` | 32 | constant | constant |  | - |
| `calldata.seal.length` | 32 | constant | constant |  | - |
| `calldata.seal[]` | 32 | constant | constant |  | - |
| `event.Release.sig` | 32 | constant | constant |  | - |
| `event.Release.nullifier` | 32 | 20 distinct | nullifier | NOut | N_out |
| `event.Release.token` | 32 | constant | constant |  | token |
| `event.Release.recipient` | 32 | 20 distinct | the user's own public address | the release recipient | recipient |
| `event.Release.amount` | 32 | constant | constant |  | denomination |
| `event.Release.relayer` | 32 | constant | constant |  | the relayer |
| `event.Release.relayFee` | 32 | constant | constant |  | - |
| `event.Release.burnRoot` | 32 | constant | constant |  | the root |
| `block.number` | 32 | 20 distinct | time / order | time | time |
| `block.timestamp` | 32 | 20 distinct | time / order | time | time |

## root import

| field | bytes | across records | class | what | §B6 |
|---|---:|---|---|---|---|
| `public_context.actors[] (members in every record)` | 64 | constant (3) | constant |  | - |
| `root.to` | 32 | constant | constant |  | - |
| `root.message.ImportRoot.config_id` | 32 | constant | constant |  | token |
| `root.message.ImportRoot.record_slot` | 8 | constant | constant |  | the vault's root at a block |
| `root.message.ImportRoot.root` | 32 | constant | constant |  | the vault's root at a block |
| `root.message.ImportRoot.leaf_count` | 8 | constant | constant |  | the vault's root at a block |
| `fee` | 65 | constant | constant |  | - |
| `nonces` | 52 | constant | constant | time | - |
| `witness_set` | 100 | constant | constant | the proof (dev mode: a fake receipt) or the submitter's signature | - |
| `block.id` | 8 | constant | constant | time | - |
| `block.timestamp` | 8 | constant | constant | time | - |
| `commitment_root.before` | 32 | constant | constant | LEZ: the commitment-set root (state the proof read) | - |
| `commitment_root.after` | 32 | constant | constant | LEZ: the commitment-set root (state the proof read) | - |

