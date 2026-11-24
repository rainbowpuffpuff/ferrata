// Which program an account id is, and how to read its messages and actor states: LEZ's built-ins, Ferrata's
// programs named in config.json, and any program whose loader header carries an image id config.json knows
// (so a fresh deployment is recognised without editing the config).

import { BUILTINS, LOADER_IDL, PROGRAM_LOADER_ID, NATIVE_TOKEN_PROGRAM_ID } from "./lez.js";
import { hex, fromB58 } from "./util.js";
import { loadIdl, labels } from "./ui.js";
import { decodeAttestationState } from "./ferrata.js";
import { plain } from "./idl.js";

export class Programs {
  constructor(rpc, config) {
    this.rpc = rpc;
    this.config = config;
    this.cache = new Map(); // program hex -> Promise<info>
    this.known = new Map(); // image id hex -> entry of config.known_images
    for (const [img, e] of Object.entries(config.known_images || {})) this.known.set(img.replace(/^0x/, "").toLowerCase(), e);
    this.byAccount = new Map();
    for (const p of config.programs || []) {
      if (p.account) {
        try {
          this.byAccount.set(hex(fromB58(p.account)), p);
        } catch {}
      }
    }
  }

  /** {name, kind: builtin|ferrata|deployed|unknown, idl?, image_id?, key?} for a program account id. */
  info(programBytes) {
    const h = hex(programBytes);
    if (!this.cache.has(h)) this.cache.set(h, this.lookup(programBytes, h));
    return this.cache.get(h);
  }

  async lookup(programBytes, h) {
    const bi = BUILTINS.get(h);
    if (bi) return { name: bi.name, kind: "builtin", idl: bi.idl };
    const cfg = this.byAccount.get(h);
    let image = cfg && cfg.image_id ? cfg.image_id.replace(/^0x/, "") : null;
    if (!image) {
      try {
        const acct = await this.rpc.accountView(programBytes, PROGRAM_LOADER_ID);
        const st = acct && acct.states.find((s) => hex(s.program) === hex(PROGRAM_LOADER_ID));
        if (st && st.data.length) {
          const v = LOADER_IDL.decodeType(st.data, "LoaderEntry");
          if (v.variant === "Header") image = hex(v.fields[0][1].v);
        }
      } catch {}
    }
    const known = image ? this.known.get(image) : null;
    const entry = cfg || known;
    if (entry) {
      const out = { name: entry.name, kind: "ferrata", key: entry.key, image_id: image, idlPath: entry.idl || null };
      if (out.idlPath) {
        try {
          out.idl = await loadIdl(out.idlPath);
        } catch {}
      }
      labels.set(programBytes, entry.name, "ferrata");
      return out;
    }
    return { name: null, kind: image ? "deployed" : "unknown", image_id: image };
  }

  /** {name, args, error?} for a root message to `programBytes`, or null when we have no IDL for it. */
  async decodeMessage(programBytes, message) {
    const info = await this.info(programBytes);
    if (!info.idl) return { info, decoded: null };
    try {
      const d = info.idl.decodeInstruction(message);
      return { info, decoded: d };
    } catch (e) {
      // an instruction our table does not list is shown raw, not as a failure
      if (/^no instruction has discriminator/.test(e.message)) return { info, decoded: null };
      return { info, decoded: null, error: e.message };
    }
  }

  /** What an actor state is, decoded where we can. */
  async decodeState(programBytes, data) {
    const h = hex(programBytes);
    if (h === hex(NATIVE_TOKEN_PROGRAM_ID)) {
      let bal = 0n;
      for (let i = data.length - 1; i >= 0; i--) bal = (bal << 8n) | BigInt(data[i]);
      return { kind: "balance", balance: bal };
    }
    if (h === hex(PROGRAM_LOADER_ID)) {
      try {
        const v = LOADER_IDL.decodeType(data, "LoaderEntry");
        if (v.variant === "Header") {
          const image = hex(v.fields[0][1].v);
          return { kind: "loader-header", value: v, image_id: image, known: this.known.get(image) || null };
        }
        return { kind: "loader-segment", value: v };
      } catch (e) {
        return { kind: "raw", error: e.message };
      }
    }
    const info = await this.info(programBytes);
    if (info.idl && info.key === "attestation") {
      const d = decodeAttestationState(info.idl, data);
      if (d) return { kind: "attestation", info, ...d };
    }
    if (info.idl) {
      const hits = info.idl.decodeAccount(data);
      if (hits.length) return { kind: "idl", info, type: hits[0].type, value: hits[0].value, plain: plain(hits[0].value) };
    }
    return { kind: "raw", info };
  }
}
