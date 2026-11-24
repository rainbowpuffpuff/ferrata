// LEZ v0.4.0-rc3's block and transaction encodings, read with borsh exactly as the sequencer's RPC serves them
// (`getBlock` and `getTransaction` return base64(borsh)). Layouts from logos-execution-zone v0.4.0-rc3:
// lez/common/src/{block,transaction}.rs, lee/state_machine/src/{transaction_message.rs, public_transaction/,
// privacy_preserving_transaction/}, lee/state_machine/core/src/{account.rs, circuit_io.rs, execution_state.rs,
// encryption/mod.rs, program/mod.rs, sealing.rs, recovery.rs}.

import { Reader, DecodeError } from "./borsh.js";
import { sha256 } from "./sha256.js";
import { Idl } from "./idl.js";
import { hex, b58, utf8, concat } from "./util.js";

export { DecodeError };

const pad32 = (s) => {
  const b = new Uint8Array(32);
  b.set(utf8(s));
  return b;
};

/** `HashableBlockData::compute_hash`'s 32-byte prefix. */
export const BLOCK_HASH_PREFIX = pad32("/LEE/v0.3/Message/Block/");
/** LEZ v0.4's `AccountId::for_public_pda` prefix: SHA-256(prefix ‖ program ‖ seed). */
export const PDA_PREFIX = pad32("/LEE/v0.2/AccountId/PDA/");
const BUILTIN_PREFIX = utf8("/LEE-BuiltinProgram/v1/AccountId");

export const BEDROCK_STATUS = ["Pending", "Safe", "Finalized"];

// ---- account ids -------------------------------------------------------------------------------------------

export function forPublicPda(program, seed) {
  return sha256(PDA_PREFIX, program, seed);
}

export function builtinProgramId(name) {
  return sha256(BUILTIN_PREFIX, utf8(name));
}

/** `AccountId::from(&PublicKey)`: the public account a witness key signs for. */
export function accountFromPublicKey(pk) {
  return sha256(pad32("/LEE/v0.3/AccountId/Public/"), pk);
}

export const NATIVE_TOKEN_PROGRAM_ID = new Uint8Array(32);
export const PROGRAM_LOADER_ID = new Uint8Array(32).fill(0xfe);

// ---- the small IDLs of LEZ's built-in programs (rc3 sources), decoded by the same IDL decoder -----------------

const acct = "account_id";
export const NATIVE_TOKEN_IDL = new Idl({
  name: "native_token",
  instructions: [
    { name: "transfer", discriminator: [0], args: [{ name: "to", type: acct }, { name: "amount", type: "u128" }] },
    { name: "credit", discriminator: [1], args: [{ name: "0", type: "u128" }] },
    { name: "read_state", discriminator: [2], args: [] },
  ],
  types: [],
});

export const LOADER_IDL = new Idl({
  name: "program_loader",
  instructions: [
    { name: "write_segment", discriminator: [0], args: [{ name: "bytecode", type: { vec: "u8" } }, { name: "next_segment", type: { option: acct } }] },
    { name: "create_header", discriminator: [1], args: [{ name: "first_segment", type: acct }, { name: "immutable", type: "bool" }] },
    { name: "update_header", discriminator: [2], args: [{ name: "first_segment", type: acct }, { name: "immutable", type: "bool" }] },
  ],
  accounts: [
    {
      name: "LoaderEntry",
      type: {
        kind: "enum",
        variants: [
          { name: "Header", fields: [{ name: "image_id", type: "program_id" }, { name: "program_first_segment", type: acct }, { name: "immutable", type: "bool" }] },
          { name: "Segment", fields: [{ name: "bytecode", type: { vec: "u8" } }, { name: "next_segment", type: { option: acct } }] },
        ],
      },
    },
  ],
  types: [],
});

export const CLOCK_IDL = new Idl({
  name: "clock",
  instructions: [
    { name: "tick", discriminator: [0], args: [{ name: "timestamp", type: "u64" }, { name: "block_id", type: "u64" }] },
    { name: "read_block_id", discriminator: [1], args: [{ name: "granularity", type: "u64" }] },
    { name: "read_timestamp", discriminator: [2], args: [{ name: "granularity", type: "u64" }] },
  ],
  types: [],
});

export const FEE_IDL = new Idl({
  name: "fee",
  instructions: [
    {
      name: "distribute",
      discriminator: [0],
      args: [{ name: "summary", type: { defined: "BlockFeeSummary" } }, { name: "payout", type: "u128" }, { name: "producer", type: acct }],
    },
    { name: "refund", discriminator: [1], args: [{ name: "amount", type: "u128" }, { name: "payer", type: acct }] },
  ],
  types: [
    {
      name: "BlockFeeSummary",
      type: {
        kind: "struct",
        fields: [
          { name: "gas_used_exec", type: "u64" },
          { name: "gas_used_stor", type: "u64" },
          { name: "revenue_base", type: "u128" },
          { name: "revenue_tip", type: "u128" },
        ],
      },
    },
  ],
});

export const LEZ_BRIDGE_IDL = new Idl({
  name: "bridge",
  instructions: [
    { name: "deposit", discriminator: [0], args: [{ name: "l1_deposit_op_id", type: { array: ["u8", 32] } }, { name: "recipient_id", type: acct }, { name: "amount", type: "u64" }] },
    { name: "withdraw", discriminator: [1], args: [{ name: "amount", type: "u64" }, { name: "bedrock_account_pk", type: { array: ["u8", 32] } }] },
  ],
  types: [],
});

const key = { array: ["u8", 32] };
export const STAKE_IDL = new Idl({
  name: "sequencer_stake",
  instructions: [
    { name: "stake", discriminator: [0], args: [{ name: "sequencer_key", type: key }, { name: "amount", type: "u128" }, { name: "has_record", type: "bool" }, { name: "funding", type: acct }] },
    { name: "unstake_request", discriminator: [1], args: [{ name: "sequencer_key", type: key }, { name: "amount", type: "u128" }, { name: "destination", type: acct }, { name: "requested_at", type: "u64" }] },
    { name: "finalize_unstake", discriminator: [2], args: [{ name: "sequencer_key", type: key }] },
    { name: "slash", discriminator: [3], args: [{ name: "sequencer_key", type: key }, { name: "inscription", type: key }, { name: "approvals", type: { vec: { defined: "SlashApproval" } } }] },
    { name: "init_channel_params", discriminator: [4], args: [{ name: "params", type: { defined: "ChannelParams" } }, { name: "channel_id", type: key }] },
    { name: "record_stake", discriminator: [5], args: [{ name: "sequencer_key", type: key }, { name: "ownership", type: acct }, { name: "amount", type: "u128" }, { name: "has_record", type: "bool" }] },
    { name: "track_unstake_request", discriminator: [6], args: [{ name: "sequencer_key", type: key }, { name: "ownership", type: acct }, { name: "amount", type: "u128" }, { name: "destination", type: acct }, { name: "requested_at", type: "u64" }] },
  ],
  types: [
    { name: "SlashApproval", type: { kind: "struct", fields: [{ name: "signer", type: key }, { name: "signature", type: { vec: "u8" } }] } },
    {
      name: "ChannelParams",
      type: {
        kind: "struct",
        fields: [
          { name: "minimum_sequencer_stake", type: "u128" },
          { name: "posting_timeframe", type: "u32" },
          { name: "posting_timeout", type: "u32" },
          { name: "exit_delay", type: "u64" },
          { name: "max_block_size", type: "u64" },
        ],
      },
    },
  ],
});

/** LEZ's built-in programs by account id (hex), with the IDL used to read their messages where we have one. */
export const BUILTINS = new Map();
function addBuiltin(id, name, idl = null) {
  BUILTINS.set(hex(id), { id, name, idl });
}
addBuiltin(NATIVE_TOKEN_PROGRAM_ID, "native token", NATIVE_TOKEN_IDL);
addBuiltin(PROGRAM_LOADER_ID, "program loader", LOADER_IDL);
addBuiltin(builtinProgramId("clock"), "clock", CLOCK_IDL);
addBuiltin(builtinProgramId("fee"), "fee", FEE_IDL);
addBuiltin(builtinProgramId("bridge"), "bridge", LEZ_BRIDGE_IDL);
addBuiltin(builtinProgramId("sequencer_stake"), "sequencer stake", STAKE_IDL);
for (const n of ["bridge_lock", "cross_zone_inbox", "cross_zone_outbox", "wrapped_token", "ping_sender", "ping_receiver"]) {
  addBuiltin(builtinProgramId(n), n.replaceAll("_", " "));
}

// ---- transactions ------------------------------------------------------------------------------------------

const actor = (r) => ({ account_id: r.bytes(32), program_account_id: r.bytes(32) });
const rootCall = (r) => ({ to: actor(r), message: r.bytesVec() });
const publicContext = (r) => ({
  actors: r.vec(actor),
  authorized_accounts: r.vec((r) => r.bytes(32)),
  promotions: r.vec((r) => r.u64()),
});
const nonces = (r) => r.vec((r) => ({ account: r.bytes(32), nonce: r.u128() }));
const evidence = (r) => {
  const t = r.u8();
  if (t === 0) return { kind: "key", public_key: r.bytes(32) };
  if (t === 1) return { kind: "pda", program: r.bytes(32), seed: r.bytes(32) };
  throw new DecodeError(`PublicAccountEvidence tag ${t}`);
};
const witnesses = (r) => r.vec((r) => ({ signature: r.bytes(64), public_key: r.bytes(32) }));
const feeDeclaration = (r) => ({ payer: r.bytes(32), gas_limit: r.u64(), tip: r.u64(), max_fee: r.u128() });

const note = (r) => ({ ciphertext: r.bytesVec(), epk: r.bytesVec() });
const delivery = (r) => ({
  envelope: { from: actor(r), to: actor(r), message: r.bytesVec() },
  inherited_authorizations: r.vec((r) => r.bytes(32)),
  inherits_entry_authorizations: r.bool(),
  pda_seeds: r.vec((r) => r.bytes(32)),
});
const BOUNDARY = ["PrivateToPublic", "PublicToPrivate", "EndPrivateSubtree", "EndPublicSubtree"];
const boundaryStep = (r) => {
  const t = r.u8();
  if (t > 3) throw new DecodeError(`BoundaryStep tag ${t}`);
  return t < 2 ? { kind: BOUNDARY[t], delivery: delivery(r) } : { kind: BOUNDARY[t] };
};
const privateAction = (r) => ({ nullifier: r.bytes(32), root: r.bytes(32), commitment: r.bytes(32), encrypted_post_state: note(r) });
const window = (r) => ({ from: r.option((r) => r.u64()), to: r.option((r) => r.u64()) });
const imageClaim = (r) => {
  const t = r.u8();
  if (t === 0) return { kind: "Disclosed", account_id: r.bytes(32), image_id: r.bytes(32) };
  if (t === 1) return { kind: "Undisclosed", root: r.bytes(32) };
  throw new DecodeError(`ProgramImageClaim tag ${t}`);
};
const provenFee = (r) => ({
  refund_to: r.option((r) => r.bytes(32)),
  fallback: r.option((r) => ({ action: privateAction(r), boundary: r.vec(boundaryStep) })),
});
const provenExecution = (r) => ({
  boundary: r.vec(boundaryStep),
  async_messages: r.vec((r) => ({ commitment: r.bytes(32), note: note(r) })),
  recovery_bindings: r.vec((r) => ({ address: r.bytes(32), note: note(r) })),
  public_root: r.option(rootCall),
  private_actions: r.vec(privateAction),
  validity: { blocks: window(r), timestamps: window(r) },
  program_image_claims: r.vec(imageClaim),
  fee: r.option(provenFee),
});

function readPublic(r) {
  const public_context = publicContext(r);
  const execution = { root: rootCall(r), fee: r.option(feeDeclaration) };
  return { kind: "Public", public_context, execution, nonces: nonces(r), admission_evidence: r.vec(evidence), witnesses: witnesses(r) };
}

function readPrivate(r) {
  const public_context = publicContext(r);
  const execution = provenExecution(r);
  const msg = { kind: "PrivacyPreserving", public_context, execution, nonces: nonces(r), admission_evidence: r.vec(evidence) };
  msg.witnesses = witnesses(r);
  msg.proof = r.bytesVec();
  return msg;
}

/** One `LeeTransaction` at the reader's position; its hash is SHA-256 of the inner transaction's borsh. */
export function readTransaction(r) {
  const tagAt = r.pos;
  const tag = r.u8();
  const start = r.pos;
  let tx;
  if (tag === 0) tx = readPublic(r);
  else if (tag === 1) tx = readPrivate(r);
  else throw new DecodeError(`LeeTransaction tag ${tag}`);
  tx.size = r.pos - start;
  tx.hash = sha256(r.buf.subarray(start, r.pos));
  tx.raw = r.buf.subarray(tagAt, r.pos);
  return tx;
}

/** `getTransaction`'s value: base64(borsh(LeeTransaction)). */
export function decodeTransaction(bytes) {
  const r = new Reader(bytes);
  const tx = readTransaction(r);
  r.finish("LeeTransaction");
  return tx;
}

// ---- blocks ------------------------------------------------------------------------------------------------

/**
 * A block as `getBlock` serves it: header, body, then the Bedrock status byte, which sits outside the hash.
 * `hash_ok` says whether SHA-256(prefix ‖ id ‖ prev ‖ timestamp ‖ body) equals the header's hash.
 */
export function decodeBlock(bytes) {
  const r = new Reader(bytes);
  const header = { block_id: r.u64(), prev_block_hash: r.bytes(32), hash: r.bytes(32), timestamp: r.u64() };
  const bodyStart = r.pos;
  const transactions = r.vec(readTransaction);
  const reverted = r.vec((r) => r.u64());
  const bodyEnd = r.pos;
  const st = r.u8();
  if (st > 2) throw new DecodeError(`bedrock_status ${st}`);
  r.finish("Block");
  const computed = sha256(BLOCK_HASH_PREFIX, bytes.subarray(0, 40), bytes.subarray(72, 80), bytes.subarray(bodyStart, bodyEnd));
  return {
    header,
    transactions,
    reverted: reverted.map(Number),
    bedrock_status: BEDROCK_STATUS[st],
    size: bytes.length,
    computed_hash: computed,
    hash_ok: hex(computed) === hex(header.hash),
  };
}

/** `Vec<Block>` (the live driver's `blocks.borsh`). */
export function decodeBlockList(bytes) {
  const r = new Reader(bytes);
  const n = r.u32();
  const out = [];
  for (let i = 0; i < n; i++) {
    const start = r.pos;
    // Walk one block to find its end, then decode it on its own slice.
    r.u64(); r.bytes(32); r.bytes(32); r.u64();
    r.vec(readTransaction);
    r.vec((r) => r.u64());
    r.u8();
    const block = decodeBlock(bytes.subarray(start, r.pos));
    block.raw = bytes.subarray(start, r.pos);
    out.push(block);
  }
  r.finish("Vec<Block>");
  return out;
}

// ---- what a transaction does, in words ----------------------------------------------------------------------

/** The root call of a transaction: the public one, or the public root a private one reveals. */
export function rootOf(tx) {
  if (tx.kind === "Public") return tx.execution.root;
  return tx.execution.public_root;
}

/** The accounts the witness set signs for, in its order. */
export function signerAccounts(tx) {
  return tx.witnesses.map((w) => accountFromPublicKey(w.public_key));
}

export const idText = (bytes) => b58(bytes);
export const isZero = (bytes) => bytes.every((b) => b === 0);

export function sum(...xs) {
  return xs.reduce((a, b) => a + b, 0);
}

export function txSummaryCounts(tx) {
  if (tx.kind === "Public") return {};
  const e = tx.execution;
  return {
    nullifiers: e.private_actions.length,
    commitments: e.private_actions.length + e.async_messages.length,
    boundary: e.boundary.length,
    proof_bytes: tx.proof.length,
  };
}

export { concat };
