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
| `calldata.index` | 32 | constant | constant | j = depositCount[depositor] (0 for a fresh depositor) | - |
| `receipt.calldataGas` | 32 | 2 distinct | a function of public data | the calldata's gas (EIP-2028), a function of the calldata | - |
| `event.Deposit.sig` | 32 | constant | constant |  | - |
| `event.Deposit.depositor` | 32 | 20 distinct | the user's own public address | the depositor (or the submitting relayer) | depositor |
| `event.Deposit.token` | 32 | constant | constant |  | token |
| `event.Deposit.index` | 32 | constant | constant | j = depositCount[depositor] (0 for a fresh depositor) | - |
| `event.Deposit.leafIndex` | 32 | 20 distinct | time / order | the deposit's position (its order) | - |
| `event.Deposit.amount` | 32 | constant | constant |  | denomination |
| `event.Deposit.commitment` | 32 | 20 distinct | fresh randomness | LEZ: the private account's new commitment | C |
| `block.number` | 32 | 20 distinct | time / order | time | time |
| `block.timestamp` | 32 | 20 distinct | time / order | time | time |
| `receipt.gasUsed.deposit` | 32 | 8 distinct | time / order | gas used: depends on the leaf position (order), see the summary | - |
| `storage: mapping@60[per-sender]` | 32 | written in 20 of 20 records | the user's own public address | per-depositor counter | - |
| `storage: mapping@57[per-token]` | 32 | written in 20 of 20 records, values vary | time / order | tree state, counters, escrow (order) | - |
| `storage: _branch[level]` | 32 | written in 20 of 20 records, values vary | time / order | tree state, counters, escrow (order) | - |
| `storage: depositRoot` | 32 | written in 20 of 20 records, values vary | time / order | tree state, counters, escrow (order) | - |
| `storage: nextLeafIndex` | 32 | written in 20 of 20 records, values vary | time / order | tree state, counters, escrow (order) | - |
| `balance: vault` | 32 | changed in 20 of 20 records | time / order | cumulative (order) | - |

## claim, private (default)

| field | bytes | across records | class | what | §B6 |
|---|---:|---|---|---|---|
| `public_context.actors[] (members in every record)` | 64 | constant (6) | constant | the declared public accounts every record shares (root, token, configuration, fee recipient, relayer, fee inbox; per step) | - |
| `public_context.actors[] (other members)` | 64 | 20 distinct | nullifier | = `derived.nullifier_account`: the nullifier account, a PDA of N | N |
| `boundary[].out.from` | 32 | 20 distinct | fresh randomness | LEZ: the sender's one-time alias (where it varies) | a one-time fee alias with a fixed reserve |
| `boundary[].out.to (members in every record)` | 32 | constant (3) | constant | a public end of a boundary message (where constant: the root, the token's account, the fee inbox) | - |
| `boundary[].out.message.Read` | 0 | constant | constant | the root read (its state, returned to the private part) | the root read |
| `boundary[].in.from` | 32 | constant | constant | a public end of a boundary message (where constant: the root, the token's account, the fee inbox) | - |
| `boundary[].in.to` | 32 | 20 distinct | fresh randomness | LEZ: the sender's one-time alias (where it varies) | a one-time fee alias with a fixed reserve |
| `boundary[].in.message.StateReply.state` | 117 | constant | constant | the root read (its state, returned to the private part) | the root read |
| `boundary[].out.message.Issue.config_id` | 32 | constant | constant | configId (the bridge program, vault, chain) | the bridge program |
| `boundary[].out.message.Issue.token` | 20 | constant | constant |  | token |
| `boundary[].out.message.Issue.nullifier` | 32 | 20 distinct | nullifier | N | N |
| `boundary[].out.message.Issue.amount` | 16 | constant | constant |  | denomination |
| `boundary[].out.message.Issue.range` | 24 | constant | constant | the fee schedule's leaf range the claim names | - |
| `boundary[].out.message.Issue.leaf_count` | 8 | constant | constant | the root read (its leaf count) | the root read |
| `boundary[].out.message.Issue.relayer` | 32 | constant | constant | the relayer the claim names (relay fee minted to it publicly) | - |
| `boundary[].out.message.native.Credit` | 16 | constant | constant | the fee reserve, moved from the alias to the fee inbox | a one-time fee alias with a fixed reserve |
| `private_actions[].nullifier` | 32 | 20 distinct | nullifier | LEZ: the private account's nullifier (main action; fee fallback) | - |
| `private_actions[].commitment_root` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |
| `private_actions[].commitment` | 32 | 20 distinct | fresh randomness | LEZ: the private account's new commitment | - |
| `private_actions[].note.ciphertext` | 247 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `private_actions[].note.epk` | 1088 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `fee.refund_to` | 32 | constant | constant | where the unspent reserve goes (here the harness's one payer) | - |
| `fee.fallback.action.nullifier` | 32 | 20 distinct | nullifier | LEZ: the private account's nullifier (main action; fee fallback) | - |
| `fee.fallback.action.commitment_root` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |
| `fee.fallback.action.commitment` | 32 | 20 distinct | fresh randomness | LEZ: the private account's new commitment | - |
| `fee.fallback.action.note.ciphertext` | 137 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `fee.fallback.action.note.epk` | 1088 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `fee.fallback.boundary[].out.from` | 32 | 20 distinct | fresh randomness | LEZ: the sender's one-time alias (where it varies) | a one-time fee alias with a fixed reserve |
| `fee.fallback.boundary[].out.to` | 32 | constant | constant | a public end of a boundary message (where constant: the root, the token's account, the fee inbox) | - |
| `fee.fallback.boundary[].out.message.native.Credit` | 16 | constant | constant | the fee reserve, moved from the alias to the fee inbox | a one-time fee alias with a fixed reserve |
| `validity` | 4 | constant | constant | validity windows (none set here) | time |
| `witness_set.signatures` | 4 | constant | constant | the witness set's signatures: none (the sender is an alias) | - |
| `proof.length` | 4 | constant | constant | the proof's length: the receipt's size in bytes | - |
| `proof.kind` | 1 | constant | constant | the receipt's kind (borsh tag of InnerReceipt: 1 succinct, 3 fake) | - |
| `proof.seal.length` | 4 | constant | constant | the seal's length in 32-bit words | - |
| `proof.seal.control_root` | 64 | constant | constant | seal outputs: the recursion control root (which recursion programs are allowed) | - |
| `proof.seal.claim_digest` | 64 | 20 distinct | a function of public data | seal outputs: the claim's digest in 16-bit halves (Montgomery form), that of `proof.claim` (checked from the public bytes) | - |
| `proof.seal.stark` | 222540 | 20 distinct | fresh randomness | LEZ's receipt: the STARK proper (trace commitments, FRI openings) of a zero-knowledge proof, drawn by the prover | - |
| `proof.control_id` | 32 | constant | constant | the recursion program that made the final receipt | - |
| `proof.claim` | 4524 | 20 distinct | a function of public data | the receipt's claim: ReceiptClaim::ok(circuit id, journal), the journal the frame of the transaction's message (checked from the public bytes) | - |
| `proof.hashfn` | 9 | constant | constant | the STARK's hash function | - |
| `proof.verifier_parameters` | 32 | constant | constant | the digest of the verifier parameters (RISC Zero's version) | - |
| `proof.control_inclusion_proof` | 264 | constant | constant | the control id's Merkle path under the control root | - |
| `receipt.gas_exec` | 8 | 2 distinct | time / order | LEZ execution gas: follows the position (the shared state the public part updates) | - |
| `receipt.cycles` | 8 | 2 distinct | time / order | LEZ execution gas: follows the position (the shared state the public part updates) | - |
| `receipt.gas_stor` | 8 | constant | constant |  | - |
| `receipt.fee_base` | 16 | 2 distinct | time / order | LEZ: the base fee at the block's opening (moves with the chain's history) | - |
| `derived.nullifier_account` | 32 | 20 distinct | nullifier | the nullifier account, a PDA of N | N |
| `block.id` | 8 | 20 distinct | time / order | time | time |
| `block.timestamp` | 8 | 20 distinct | time / order | time | time |
| `commitment_root.before` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |
| `commitment_root.after` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |
| `state: token(cid, ETH) (token gate, time)` | 0 | changed in every record, contents vary | time / order | cumulative state (order) | - |
| `state: the account in `derived.nullifier_account`` | 0 | one per record (20 distinct) | nullifier | created or changed by this record only | N |
| `state: the harness's fee payer (refund_to)` | 0 | changed in every record, contents vary | time / order | cumulative state (order) | - |
| `state: config(cid) (global gate, time)` | 0 | changed in every record, contents vary | time / order | cumulative state (order) | - |
| `state: holder(the relayer) (its wrapped balance: relay fees)` | 0 | changed in every record, contents vary | time / order | cumulative state (order) | - |
| `state: LEZ fee inbox` | 0 | changed in every record, contents vary | time / order | cumulative state (order) | - |

## claim, plaintext

| field | bytes | across records | class | what | §B6 |
|---|---:|---|---|---|---|
| `public_context.actors[] (members in every record)` | 64 | constant (5) | constant | the declared public accounts every record shares (root, token, configuration, fee recipient, relayer, fee inbox; per step) | - |
| `public_context.actors[] (other members)` | 64 | 20 distinct | UNCLASSIFIED | the per-record declared account | - |
| `root.to` | 32 | constant | constant |  | - |
| `root.message.Claim.config_id` | 32 | constant | constant | configId (the bridge program, vault, chain) | - |
| `root.message.Claim.leaf_count` | 8 | constant | constant | the root read (its leaf count) | - |
| `root.message.Claim.witness.depositor` | 20 | 20 distinct | UNCLASSIFIED |  | - |
| `root.message.Claim.witness.token` | 20 | constant | constant |  | - |
| `root.message.Claim.witness.amount` | 32 | constant | constant |  | - |
| `root.message.Claim.witness.leaf_index` | 8 | 20 distinct | UNCLASSIFIED |  | - |
| `root.message.Claim.witness.path[] (members in every record)` | 32 | constant (27) | constant |  | - |
| `root.message.Claim.witness.path[] (other members)` | 32 | 20 distinct | UNCLASSIFIED |  | - |
| `root.message.Claim.witness.s` | 32 | 20 distinct | UNCLASSIFIED |  | - |
| `root.message.Claim.witness.kind` | 1 | constant | constant |  | - |
| `root.message.Claim.witness.dest` | 32 | 20 distinct | UNCLASSIFIED |  | - |
| `root.message.Claim.range` | 24 | constant | constant | the fee schedule's leaf range the claim names | - |
| `root.message.Claim.relayer` | 32 | constant | constant | the relayer the claim names (relay fee minted to it publicly) | - |
| `fee` | 65 | constant | constant | fee declaration / nonces of the public submitter (the harness's payer) | - |
| `nonces` | 52 | 20 distinct | time / order | time | - |
| `witness_set` | 100 | 20 distinct | fresh randomness | the proof (dev mode: a fake receipt) or the submitter's signature | - |
| `receipt.gas_exec` | 8 | 2 distinct | time / order | LEZ execution gas: follows the position (the shared state the public part updates) | - |
| `receipt.cycles` | 8 | 2 distinct | time / order | LEZ execution gas: follows the position (the shared state the public part updates) | - |
| `receipt.gas_stor` | 8 | constant | constant |  | - |
| `receipt.fee_base` | 16 | 2 distinct | time / order | LEZ: the base fee at the block's opening (moves with the chain's history) | - |
| `derived.nullifier_account` | 32 | 20 distinct | nullifier | the nullifier account, a PDA of N | - |
| `block.id` | 8 | 20 distinct | time / order | time | - |
| `block.timestamp` | 8 | 20 distinct | time / order | time | - |
| `commitment_root.before` | 32 | constant | constant | LEZ: the commitment-set root (state the proof read) | - |
| `commitment_root.after` | 32 | constant | constant | LEZ: the commitment-set root (state the proof read) | - |
| `state: token(cid, ETH) (token gate, time)` | 0 | changed in every record, contents vary | time / order | cumulative state (order) | - |
| `state: the harness's fee payer (refund_to)` | 0 | changed in every record, contents vary | time / order | cumulative state (order) | - |
| `state: config(cid) (global gate, time)` | 0 | changed in every record, contents vary | time / order | cumulative state (order) | - |
| `state: holder(the relayer) (its wrapped balance: relay fees)` | 0 | changed in every record, contents vary | time / order | cumulative state (order) | - |
| `state: LEZ fee inbox` | 0 | changed in every record, contents vary | time / order | cumulative state (order) | - |
| `state: holder(a public destination) (plaintext universe)` | 0 | one per record (20 distinct) | UNCLASSIFIED | created or changed by this record only | - |
| `state: the account in `derived.nullifier_account`` | 0 | one per record (20 distinct) | nullifier | created or changed by this record only | - |

## burn, private holding

| field | bytes | across records | class | what | §B6 |
|---|---:|---|---|---|---|
| `public_context.actors[] (members in every record)` | 64 | constant (3) | constant | the declared public accounts every record shares (root, token, configuration, fee recipient, relayer, fee inbox; per step) | - |
| `public_context.actors[] (other members)` | 64 | 20 distinct | fresh randomness | = `derived.burn_record_account`: the burn record account, a PDA of H(note) | the burn note |
| `boundary[].out.from` | 32 | 20 distinct | fresh randomness | LEZ: the sender's one-time alias (where it varies) | a one-time fee alias with a fixed reserve |
| `boundary[].out.to (members in every record)` | 32 | constant (2) | constant | a public end of a boundary message (where constant: the root, the token's account, the fee inbox) | - |
| `boundary[].out.message.Burn.config_id` | 32 | constant | constant | configId (the bridge program, vault, chain) | the bridge program |
| `boundary[].out.message.Burn.r` | 32 | 20 distinct | fresh randomness | the burn note (tag, encrypted recipient, mac, COut) | the burn note |
| `boundary[].out.message.Burn.memo` | 20 | 20 distinct | fresh randomness | the burn note (tag, encrypted recipient, mac, COut) | the burn note |
| `boundary[].out.message.Burn.mac` | 16 | 20 distinct | fresh randomness | the burn note (tag, encrypted recipient, mac, COut) | the burn note |
| `boundary[].out.message.Burn.token` | 20 | constant | constant |  | token |
| `boundary[].out.message.Burn.amount` | 32 | constant | constant |  | denomination |
| `boundary[].out.message.Burn.c_out` | 32 | 20 distinct | fresh randomness | the burn note (tag, encrypted recipient, mac, COut) | the burn note |
| `boundary[].out.message.native.Credit` | 16 | constant | constant | the fee reserve, moved from the alias to the fee inbox | a one-time fee alias with a fixed reserve |
| `private_actions[].nullifier` | 32 | 20 distinct | nullifier | LEZ: the private account's nullifier (main action; fee fallback) | - |
| `private_actions[].commitment_root` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |
| `private_actions[].commitment` | 32 | 20 distinct | fresh randomness | LEZ: the private account's new commitment | - |
| `private_actions[].note.ciphertext` | 179 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `private_actions[].note.epk` | 1088 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `fee.refund_to` | 32 | constant | constant | where the unspent reserve goes (here the harness's one payer) | - |
| `fee.fallback.action.nullifier` | 32 | 20 distinct | nullifier | LEZ: the private account's nullifier (main action; fee fallback) | - |
| `fee.fallback.action.commitment_root` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |
| `fee.fallback.action.commitment` | 32 | 20 distinct | fresh randomness | LEZ: the private account's new commitment | - |
| `fee.fallback.action.note.ciphertext` | 247 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `fee.fallback.action.note.epk` | 1088 | 20 distinct | fresh randomness | LEZ: the private account's encrypted note and ephemeral key | - |
| `fee.fallback.boundary[].out.from` | 32 | 20 distinct | fresh randomness | LEZ: the sender's one-time alias (where it varies) | a one-time fee alias with a fixed reserve |
| `fee.fallback.boundary[].out.to` | 32 | constant | constant | a public end of a boundary message (where constant: the root, the token's account, the fee inbox) | - |
| `fee.fallback.boundary[].out.message.native.Credit` | 16 | constant | constant | the fee reserve, moved from the alias to the fee inbox | a one-time fee alias with a fixed reserve |
| `validity` | 4 | constant | constant | validity windows (none set here) | time |
| `witness_set.signatures` | 4 | constant | constant | the witness set's signatures: none (the sender is an alias) | - |
| `proof.length` | 4 | constant | constant | the proof's length: the receipt's size in bytes | - |
| `proof.kind` | 1 | constant | constant | the receipt's kind (borsh tag of InnerReceipt: 1 succinct, 3 fake) | - |
| `proof.seal.length` | 4 | constant | constant | the seal's length in 32-bit words | - |
| `proof.seal.control_root` | 64 | constant | constant | seal outputs: the recursion control root (which recursion programs are allowed) | - |
| `proof.seal.claim_digest` | 64 | 20 distinct | a function of public data | seal outputs: the claim's digest in 16-bit halves (Montgomery form), that of `proof.claim` (checked from the public bytes) | - |
| `proof.seal.stark` | 222540 | 20 distinct | fresh randomness | LEZ's receipt: the STARK proper (trace commitments, FRI openings) of a zero-knowledge proof, drawn by the prover | - |
| `proof.control_id` | 32 | constant | constant | the recursion program that made the final receipt | - |
| `proof.claim` | 3984 | 20 distinct | a function of public data | the receipt's claim: ReceiptClaim::ok(circuit id, journal), the journal the frame of the transaction's message (checked from the public bytes) | - |
| `proof.hashfn` | 9 | constant | constant | the STARK's hash function | - |
| `proof.verifier_parameters` | 32 | constant | constant | the digest of the verifier parameters (RISC Zero's version) | - |
| `proof.control_inclusion_proof` | 264 | constant | constant | the control id's Merkle path under the control root | - |
| `receipt.gas_exec` | 8 | constant | constant | LEZ execution gas: follows the position (the shared state the public part updates) | - |
| `receipt.cycles` | 8 | constant | constant | LEZ execution gas: follows the position (the shared state the public part updates) | - |
| `receipt.gas_stor` | 8 | constant | constant |  | - |
| `receipt.fee_base` | 16 | constant | constant | LEZ: the base fee at the block's opening (moves with the chain's history) | - |
| `derived.burn_record_account` | 32 | 20 distinct | fresh randomness | the burn record account, a PDA of H(note) | the burn note |
| `block.id` | 8 | 20 distinct | time / order | time | time |
| `block.timestamp` | 8 | 20 distinct | time / order | time | time |
| `commitment_root.before` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |
| `commitment_root.after` | 32 | 20 distinct | time / order | LEZ: the commitment-set root (state the proof read) | - |
| `state: token(cid, ETH) (token gate, time)` | 0 | changed in every record, contents vary | time / order | cumulative state (order) | - |
| `state: the harness's fee payer (refund_to)` | 0 | changed in every record, contents vary | time / order | cumulative state (order) | - |
| `state: config(cid) (global gate, time)` | 0 | changed in every record, contents vary | time / order | cumulative state (order) | - |
| `state: LEZ fee inbox` | 0 | changed in every record, contents vary | time / order | cumulative state (order) | - |
| `state: the account in `derived.burn_record_account`` | 0 | one per record (20 distinct) | fresh randomness | created or changed by this record only | the burn note |

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
| `calldata.request.relayFee` | 32 | constant | constant | the registered relay fee | - |
| `calldata.request.burnRoot` | 32 | constant | constant |  | the root |
| `calldata.request.nullifier` | 32 | 20 distinct | nullifier | NOut | N_out |
| `calldata.seal.offset` | 32 | constant | constant | the seal's ABI offset and length | - |
| `calldata.seal.length` | 32 | constant | constant | the seal's ABI offset and length | - |
| `calldata.seal.selector` | 4 | constant | constant | the Groth16 verifier's selector (the first 4 bytes of its parameters' digest) | - |
| `calldata.seal.groth16.a` | 64 | 20 distinct | fresh randomness | the release's Groth16 proof (pi_A, pi_B, pi_C), randomized by the prover | - |
| `calldata.seal.groth16.b` | 128 | 20 distinct | fresh randomness | the release's Groth16 proof (pi_A, pi_B, pi_C), randomized by the prover | - |
| `calldata.seal.groth16.c` | 64 | 20 distinct | fresh randomness | the release's Groth16 proof (pi_A, pi_B, pi_C), randomized by the prover | - |
| `calldata.seal.padding` | 28 | constant | constant | the seal's ABI padding | - |
| `receipt.calldataGas` | 32 | 4 distinct | a function of public data | the calldata's gas (EIP-2028), a function of the calldata | - |
| `event.Release.sig` | 32 | constant | constant |  | - |
| `event.Release.nullifier` | 32 | 20 distinct | nullifier | NOut | N_out |
| `event.Release.token` | 32 | constant | constant |  | token |
| `event.Release.recipient` | 32 | 20 distinct | the user's own public address | the release recipient | recipient |
| `event.Release.amount` | 32 | constant | constant |  | denomination |
| `event.Release.relayer` | 32 | constant | constant |  | the relayer |
| `event.Release.relayFee` | 32 | constant | constant | the registered relay fee | - |
| `event.Release.burnRoot` | 32 | constant | constant |  | the root |
| `block.number` | 32 | 20 distinct | time / order | time | time |
| `block.timestamp` | 32 | 20 distinct | time / order | time | time |
| `receipt.gasUsed` | 32 | 2 distinct | time / order | gas used: the first release's relay fee creates the relayer's account | - |
| `storage: mapping@61[per-nullifier]` | 32 | written in 20 of 20 records | nullifier | nullifierUsed[NOut] | - |
| `storage: mapping@57[per-token]` | 32 | written in 20 of 20 records, values vary | time / order | tree state, counters, escrow (order) | - |
| `balance: vault` | 32 | changed in 20 of 20 records | time / order | cumulative (order) | - |
| `balance: per record` | 32 | changed in 20 of 20 records | the user's own public address | the recipient's own address | - |
| `balance: relayer` | 32 | changed in 20 of 20 records | time / order | cumulative (order) | - |

## root import

| field | bytes | across records | class | what | §B6 |
|---|---:|---|---|---|---|
| `public_context.actors[] (members in every record)` | 64 | constant (3) | constant | the declared public accounts every record shares (root, token, configuration, fee recipient, relayer, fee inbox; per step) | - |
| `root.to` | 32 | constant | constant |  | - |
| `root.message.ImportRoot.config_id` | 32 | constant | constant | configId (the bridge program, vault, chain) | the bridge program |
| `root.message.ImportRoot.record_slot` | 8 | constant | constant |  | the vault's root at a block |
| `root.message.ImportRoot.root` | 32 | constant | constant |  | the vault's root at a block |
| `root.message.ImportRoot.leaf_count` | 8 | constant | constant |  | the vault's root at a block |
| `fee` | 65 | constant | constant | fee declaration / nonces of the public submitter (the harness's payer) | - |
| `nonces` | 52 | constant | constant | fee declaration / nonces of the public submitter (the harness's payer) | - |
| `witness_set` | 100 | constant | constant | proof (dev mode) or signature | - |
| `receipt.gas_exec` | 8 | constant | constant | LEZ execution gas: follows the position (the shared state the public part updates) | - |
| `receipt.cycles` | 8 | constant | constant | LEZ execution gas: follows the position (the shared state the public part updates) | - |
| `receipt.gas_stor` | 8 | constant | constant |  | - |
| `receipt.fee_base` | 16 | constant | constant | LEZ: the base fee at the block's opening (moves with the chain's history) | - |
| `block.id` | 8 | constant | constant | time | - |
| `block.timestamp` | 8 | constant | constant | time | - |
| `commitment_root.before` | 32 | constant | constant | LEZ: the commitment-set root (state the proof read) | - |
| `commitment_root.after` | 32 | constant | constant | LEZ: the commitment-set root (state the proof read) | - |
| `state: the harness's fee payer (refund_to)` | 0 | changed in every record | constant |  | - |
| `state: LEZ fee inbox` | 0 | changed in every record | constant |  | - |
| `state: root(cid, 20)` | 0 | changed in every record | constant |  | - |

