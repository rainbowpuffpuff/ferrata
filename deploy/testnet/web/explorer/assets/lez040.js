// LEZ v0.4.0 (commit a255b42c): the explorer-compatible decoder for the layouts that changed after rc3.
// Objects keep assets/lez.js's public shape (including snake_case names and inner-transaction hashes). In particular,
// a receipt-bearing block still has `reverted`, while witness entries have auth_data/next instead of signatures.
// Layouts: lez/common/src/block.rs, lee/state_machine/src/transaction_message.rs, public_transaction/,
// privacy_preserving_transaction/, and lee/state_machine/core/src/{circuit_io,execution_state,signing_key}.rs.

import { Reader, DecodeError } from "./borsh.js";
import { sha256 } from "./sha256.js";
import { hex, concat, utf8 } from "./util.js";
import {
  BEDROCK_STATUS, BLOCK_HASH_PREFIX, BUILTINS as RC3_BUILTINS, forPublicPda,
  CLOCK_IDL, FEE_IDL, LEZ_BRIDGE_IDL, STAKE_IDL, signerAccounts as rc3SignerAccounts,
} from "./lez.js";

export * from "./lez.js";
export { DecodeError };

// core/src/program/mod.rs: system_upgrader itself is 0xfd; every other v0.4.0 system program is one of its PDAs,
// not rc3's direct name hashes. Keep both generations in the map because the page modules are shared by both decoders.
const pad32 = (s) => { const b = new Uint8Array(32); b.set(utf8(s)); return b; };
const SYSTEM_UPGRADER_ID = new Uint8Array(32).fill(0xfd);
const SYSTEM_PROGRAM_SEED_PREFIX = pad32("/LEE/SystemUpgrader/v1/Name");

export function systemProgramId(name) {
  if (name === "system_upgrader") return SYSTEM_UPGRADER_ID;
  const seed = sha256(SYSTEM_PROGRAM_SEED_PREFIX, pad32(name));
  return forPublicPda(SYSTEM_UPGRADER_ID, seed);
}

// v0.4.0's system programs (lee/state_machine/core/src/program/mod.rs, lez/programs/*/core, signing_key.rs);
// cross-zone messaging and its programs were removed (LEZ #1147).
export const BUILTINS = new Map(RC3_BUILTINS);
for (const [name, idl] of [["system_upgrader", null], ["clock", CLOCK_IDL], ["fee", FEE_IDL], ["bridge", LEZ_BRIDGE_IDL],
  ["sequencer_stake", STAKE_IDL], ["signature", null]]) {
  const id = systemProgramId(name);
  BUILTINS.set(hex(id), { id, name: name.replaceAll("_", " "), idl });
}

const actor = (r) => ({ account_id: r.bytes(32), program_account_id: r.bytes(32) });
const rootCall = (r) => ({ to: actor(r), message: r.bytesVec() });

function authority(r) {
  const t = r.u8();
  if (t === 0) return "none";
  if (t === 1) return "signer";
  throw new DecodeError(`Authority tag ${t}`);
}

const publicContext = (r) => ({
  actors: r.vec((r) => ({ ...actor(r), authority: authority(r) })),
  authorized_accounts: r.vec((r) => r.bytes(32)),
  promotions: r.vec((r) => r.u64()),
  private_signers: r.vec(actor),
});
const nonces = (r) => r.vec((r) => ({ account: r.bytes(32), nonce: r.u128() }));
const feeDeclaration = (r) => ({ payer: r.bytes(32), gas_limit: r.u64(), tip: r.u64(), max_fee: r.u128() });

/** A 33-byte SigningKeyState (currently only the Schnorr discriminant, zero). */
function keyState(r) {
  const state = r.bytes(33);
  if (state[0] !== 0) throw new DecodeError(`SigningKeyState key type ${state[0]}`);
  return state;
}

const evidence = (r) => {
  const t = r.u8();
  if (t === 0) return { kind: "key", public_key: keyState(r) };
  if (t === 1) return { kind: "pda", program: r.bytes(32), seed: r.bytes(32) };
  throw new DecodeError(`PublicAccountEvidence tag ${t}`);
};
const witnesses = (r) => r.vec((r) => ({ auth_data: r.bytesVec(), next: keyState(r) }));

const note = (r) => ({ ciphertext: r.bytesVec(), epk: r.bytesVec() });
const envelope = (r) => ({ from: actor(r), to: actor(r), message: r.bytesVec() });
const delivery = (r) => ({ envelope: envelope(r), grants: r.vec(actor), pda_seeds: r.vec((r) => r.bytes(32)) });
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

const publication = (r) => {
  const t = r.u8();
  if (t === 0) return { kind: "Clear", envelope: envelope(r) };
  if (t === 1) return { kind: "Sealed", commitment: r.bytes(32), note: note(r) };
  throw new DecodeError(`Publication tag ${t}`);
};
const feeFallback = (r) => ({
  nullifier: r.bytes(32), root: r.bytes(32), commitment: r.bytes(32),
  ciphertext: r.bytesVec(), boundary: r.vec(boundaryStep),
});
const provenFee = (r) => ({
  refund_to: r.option((r) => r.bytes(32)),
  tip: r.u64(),
  fallback: r.option(feeFallback),
});
const provenExecution = (r) => ({
  boundary: r.vec(boundaryStep),
  async_messages: r.vec(publication),
  recovery_bindings: r.vec((r) => ({ address: r.bytes(32), note: note(r) })),
  public_root: r.option(rootCall),
  private_actions: r.vec(privateAction),
  validity: { blocks: window(r), timestamps: window(r) },
  program_image_claims: r.vec(imageClaim),
  fee: r.option(provenFee),
});

function readPublic(r) {
  const public_context = publicContext(r);
  const execution = { root: rootCall(r) };
  const nonceList = nonces(r);
  execution.fee = r.option(feeDeclaration);
  return {
    kind: "Public", public_context, execution, nonces: nonceList,
    admission_evidence: r.vec(evidence), witnesses: witnesses(r),
  };
}

function readPrivate(r) {
  const public_context = publicContext(r);
  const execution = provenExecution(r);
  const nonceList = nonces(r);
  // Unlike the public transaction's execution.fee, execution.fee here remains the circuit's ProvenFee.
  const fee = r.option(feeDeclaration);
  const msg = {
    kind: "PrivacyPreserving", public_context, execution, nonces: nonceList, fee,
    admission_evidence: r.vec(evidence), witnesses: witnesses(r), proof: r.bytesVec(),
  };
  return msg;
}

/** One `LeeTransaction`; its hash and raw bytes have exactly the same meaning as in assets/lez.js. */
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

/** `getTransaction`'s base64-decoded borsh, with no trailing bytes accepted. */
export function decodeTransaction(bytes) {
  const r = new Reader(bytes);
  const tx = readTransaction(r);
  r.finish("LeeTransaction");
  return tx;
}

const receipt = (r) => ({
  success: r.bool(),
  fee: { gas_exec: r.u64(), gas_stor: r.u64(), fee_base: r.u128(), tip: r.u128(), refund: r.u128() },
});

/** A v0.4.0 block. Receipts parallel the user transactions; trailing system transactions have no receipt. */
export function decodeBlock(bytes) {
  const r = new Reader(bytes);
  const header = { block_id: r.u64(), prev_block_hash: r.bytes(32), hash: r.bytes(32), timestamp: r.u64() };
  const bodyStart = r.pos;
  const transactions = r.vec(readTransaction);
  const receipts = r.vec(receipt);
  const bodyEnd = r.pos;
  const st = r.u8();
  if (st > 2) throw new DecodeError(`bedrock_status ${st}`);
  r.finish("Block");
  const computed = sha256(BLOCK_HASH_PREFIX, bytes.subarray(0, 40), bytes.subarray(72, 80), bytes.subarray(bodyStart, bodyEnd));
  return {
    header, transactions, receipts,
    reverted: receipts.flatMap((r, i) => r.success ? [] : [i]),
    bedrock_status: BEDROCK_STATUS[st],
    size: bytes.length,
    computed_hash: computed,
    hash_ok: hex(computed) === hex(header.hash),
  };
}

/** `Vec<Block>`, as the live driver's concatenated block list uses it. */
export function decodeBlockList(bytes) {
  const r = new Reader(bytes);
  const n = r.u32();
  const out = [];
  for (let i = 0; i < n; i++) {
    const start = r.pos;
    r.u64(); r.bytes(32); r.bytes(32); r.u64();
    r.vec(readTransaction);
    r.vec(receipt);
    r.u8();
    const block = decodeBlock(bytes.subarray(start, r.pos));
    block.raw = bytes.subarray(start, r.pos);
    out.push(block);
  }
  r.finish("Vec<Block>");
  return out;
}

/**
 * v0.4.0 no longer puts a raw account public key in a witness. Its entries follow the sorted nonce map, which names
 * exactly the signer accounts (public_transaction/witness_set.rs and the stateless transaction check).
 */
export function signerAccounts(tx) {
  // A transaction read by rc3's decoder (the pages share this function) keeps rc3's rule: the witness's public key.
  if (tx.witnesses.length && tx.witnesses[0].public_key) return rc3SignerAccounts(tx);
  return tx.witnesses.map((_, i) => tx.nonces[i].account);
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
