// The browser's own check of attest requests (assets/eth.js) against real data:
//  * Keccak-256 on known vectors and on Sepolia block 12,105,936's header (header.rlp of
//    crates/ferrata-spectests/fixtures/sepolia-attest-2026-11-10 must hash to the block's hash);
//  * the six recorded attest messages of docs/guides/runs/2026-11-21-lez/attest-messages (LEZ's state machine ran
//    them on 21 Nov 2026; exits.txt and the run's README: the `slot --expect-eq 0x01` request was refused with 130
//    PredicateFailed, every other one applied) decoded with the IDL and checked: the header binds to the block hash,
//    every proof verifies, and the verdict equals the program's (the `== 1` request fails its condition, the others
//    hold); the storage value equals fixture.json's (totalSupply of 0x1c7d…7238).
//
//   node deploy/testnet/web/explorer/test/eth.test.mjs

import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { keccak256, checkRequest, requestFromIdl, executionHeader } from "../assets/eth.js";
import { Idl } from "../assets/idl.js";
import { hex, fromHex } from "../assets/util.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, "../../../../..");
const FIX = join(REPO, "crates/ferrata-spectests/fixtures/sepolia-attest-2026-11-10");
let failures = 0;
const check = (cond, what) => {
  console.log(`${cond ? "ok  " : "FAIL"} ${what}`);
  if (!cond) failures++;
};

const enc = new TextEncoder();
check(hex(keccak256(new Uint8Array(0))) === "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470", "keccak256('')");
check(hex(keccak256(enc.encode("abc"))) === "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45", "keccak256('abc')");

const fixture = JSON.parse(readFileSync(join(FIX, "fixture.json"), "utf8"));
const h = executionHeader(new Uint8Array(readFileSync(join(FIX, "header.rlp")))); // 679 bytes: five Keccak blocks
check(hex(h.hash) === fixture.block_hash.slice(2), `block ${fixture.block}'s header (multi-block Keccak input) hashes to its block hash`);
check(hex(h.state_root) === fixture.state_root.slice(2) && h.number === BigInt(fixture.block), "the header's state root and number");

const record = { execution: { layout: "payload", block_number: BigInt(fixture.block), block_hash: fromHex(fixture.block_hash), state_root: fromHex(fixture.state_root), receipts_root: fromHex(fixture.receipts_root) } };
const idl = new Idl(JSON.parse(readFileSync(join(REPO, "idl/ferrata_attestation.idl.json"), "utf8")));
const want = { 5: true, 6: true, 7: false, 8: false, 9: true, 10: true };
const total = fixture.storage[0].slots[0].value.slice(2);
for (const n of [5, 6, 7, 8, 9, 10]) {
  const msg = fromHex(readFileSync(join(REPO, `docs/guides/runs/2026-11-21-lez/attest-messages/message-${n}.hex`), "utf8").trim());
  const { ins, args } = idl.decodeInstruction(msg);
  const req = requestFromIdl(args[0][1]);
  const r = checkRequest(req, record);
  const proofs = r.facts.every((f) => f.proof_ok && (f.slots || []).every((s) => s.proof_ok));
  check(ins.name === "attest" && r.header && r.header.binds, `message-${n}: attest; its header binds to the record's block hash`);
  check(proofs, `message-${n}: every proof verifies (${r.facts.map((f) => f.kind).join(", ")})`);
  check(r.ok === want[n], `message-${n}: verdict ${r.ok ? "holds" : "fails"}, as the program decided (${want[n] ? "applied" : "130 PredicateFailed"})`);
  for (const f of r.facts) for (const s of f.slots || []) check(hex(s.value) === total, `message-${n}: slot 0xb of 0x${hex(f.address)} = 0x${total.replace(/^0+/, "")} (fixture.json's totalSupply)`);
  for (const f of r.facts.filter((x) => x.kind === "log")) check(hex(f.address) === fixture.transfer.token.slice(2), `message-${n}: the log's emitter is the fixture's token 0x${hex(f.address)}`);
}

if (failures) {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}
console.log("all checks passed");
