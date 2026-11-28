// client.js: ferrata_msg over LEZ's JSON-RPC: read an actor's mailbox, and sign and send the program's messages as
// public transactions. Existing callers default to rc3; the page passes its selected net object for v0.4.0.
import * as lez from '../lez.js'; // the messages library's client.js as the wallet took it, importing this directory's lez.js
import * as M from './ferrata-msg.js';

/** Gas limits per action, from the harness's measured cycles (lez/harness/results/messages.md) with headroom. */
// Measured maxima (harness, full 64 KiB inbox): send 831,966 cycles, owner edits 666,326, publish 512,019. A transaction
// outside its validity window is charged the whole limit, so the limits stay close to what is used.
export const GAS = { publish: 1_200_000n, send: 1_500_000n, edit: 1_200_000n };

export const actorOf = (account, program) => ({ account, program });

/** The raw state of the actor (account, program), or an empty array. */
export async function actorState(seq, account, program) {
  const acc = await seq.account(account);
  const states = acc?.data?.actor_states ?? acc?.data?.actorStates ?? {};
  const v = states[lez.idToString(program)];
  if (v == null) return new Uint8Array(0);
  if (Array.isArray(v)) return Uint8Array.from(v);
  if (typeof v === 'string') return /^[0-9a-f]*$/i.test(v) && v.length % 2 === 0 ? lez.unhex(v) : lez.base64.decode(v);
  throw new Error('unexpected actor state encoding');
}
export async function mailbox(seq, account, program) { return M.decodeMailbox(await actorState(seq, account, program)); }

/** Sign `data` to the signer's own actor of `program` (with `others`' actors), pay with the signer, send it. */
export async function call(seq, { node, program, data, others = [], gasLimit, nonce = null, net = null, gate = null }) {
  const feeState = await seq.feeState();
  if (nonce == null) nonce = lez.big((await seq.nonces([node.accountId]))[0]);
  // `gate` (the page's version re-read) runs after the reads, right before signing: nothing is awaited in between.
  if (gate && !(await gate())) throw new Error(net?.error ?? 'the network\'s LEZ version could not be read, so nothing is signed');
  const actors = [actorOf(node.accountId, program), ...others.map((o) => actorOf(o, program))];
  const args = { payer: node, to: actorOf(node.accountId, program), actors, data, nonce, gasLimit, evidence: [] };
  const build = net ? (a) => net.buildCall(a) : lez.buildCall;
  const probe = build({ ...args, maxFee: 0n });
  const { maxFee, reserve } = lez.feeFor({ gasLimit, size: probe.size, feeState });
  const tx = build({ ...args, maxFee });
  const hash = await seq.send(tx.wire);
  return { hash: typeof hash === 'string' ? hash : tx.hash, size: tx.size, reserve, maxFee };
}

/** Wait until the transaction is in a block (or `timeoutMs` passes). Returns its block id, or null. */
export async function included(seq, hash, { timeoutMs = 90_000, everyMs = 1000 } = {}) {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    const r = await seq.transaction(hash).catch(() => null);
    if (r) return Array.isArray(r) ? r[1] : r.block_id ?? r;
    await new Promise((ok) => setTimeout(ok, everyMs));
  }
  return null;
}
