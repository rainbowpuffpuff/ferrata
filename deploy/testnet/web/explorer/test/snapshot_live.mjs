// Records, read-only, a few real transactions of the test network and the account states they touched, for the offline
// tests (test/live-snapshot.json): the names program's commits, reveals and a primary name; the messages program's
// key publications, policy, contacts, block list and sends; a faucet transfer. Nothing is sent.
//
//   node deploy/testnet/web/explorer/test/snapshot_live.mjs https://HOST/

import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Rpc } from "../assets/rpc.js";
import { Idl } from "../assets/idl.js";
import { readFileSync } from "node:fs";
import { hex, b58, b64encode, fromB58 } from "../assets/util.js";
import { rootOf, signerAccounts } from "../assets/lez.js";
import { nameAccount } from "../assets/names.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const site = (process.argv[2] || "").replace(/\/?$/, "/");
const dep = await (await fetch(new URL("programs.json", site))).json();
const rpc = new Rpc(new URL("rpc", site).href, { rangeLimit: 50 });
const P = { names: dep.programs.ferrata_names.program_account, msg: dep.programs.ferrata_msg.program_account };
const namesIdl = new Idl(JSON.parse(readFileSync(join(HERE, "../idl/ferrata_names.idl.json"), "utf8")));
const head = await rpc.lastBlockId();
const txs = [];
const accounts = new Map(); // b58 -> getAccount JSON
const want = new Set();
let faucetTx = 0;
for (let a = 2; a <= head; a += 50) {
  const blocks = await rpc.blockRange(a, Math.min(head, a + 49));
  for (const b of blocks) {
    b.transactions.forEach((tx, i) => {
      const r = rootOf(tx);
      if (!r || !tx.witnesses.length) return;
      const p = b58(r.to.program_account_id);
      const isNative = p === "11111111111111111111111111111111";
      if (p !== P.names && p !== P.msg && !(isNative && faucetTx < 2)) return;
      if (isNative) faucetTx++;
      txs.push({ block: Number(b.header.block_id), timestamp: String(b.header.timestamp), index: i, reverted: b.reverted.includes(i), tx: b64encode(tx.raw), hash: hex(tx.hash) });
      for (const s of signerAccounts(tx)) want.add(b58(s));
      if (p === P.msg && r.message[0] === 4) want.add(b58(r.message.subarray(1, 33)));
      if (p === P.names && r.message[0] === 0) {
        const { args } = namesIdl.decodeInstruction(r.message);
        want.add(b58(nameAccount(fromB58(P.names), args[0][1].v)));
      }
    });
  }
}
for (const id of want) accounts.set(id, await rpc.call("getAccount", [id]));
const out = {
  what: "Read-only snapshot of the Ferrata LEZ test network: names, messages and faucet transactions with the accounts they touched; for the explorer's offline tests.",
  recorded_utc: new Date().toISOString().slice(0, 16) + "Z",
  head,
  programs: { ferrata_names: P.names, ferrata_msg: P.msg },
  txs,
  accounts: Object.fromEntries(accounts),
};
writeFileSync(join(HERE, "live-snapshot.json"), JSON.stringify(out) + "\n");
console.log(`recorded ${txs.length} transactions and ${accounts.size} accounts at head ${head}`);
