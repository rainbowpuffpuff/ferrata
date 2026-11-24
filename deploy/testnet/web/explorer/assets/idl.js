// Decoding with a SPEL IDL (spel-framework-core 0.7.0's `SpelIdl`), as idl/check.py does it: the types are SPEL's
// grammar only (primitives, vec, option, defined, array), every value is borsh, and a decode must use every byte.
// Used for Ferrata's two programs (idl/*.idl.json) and for the small IDLs written here for LEZ's built-in programs.

import { Reader, DecodeError } from "./borsh.js";
import { hex, b58 } from "./util.js";

const INTS = { u8: 1, u16: 2, u32: 4, u64: 8, u128: 16, i8: 1, i16: 2, i32: 4, i64: 8, i128: 16 };
const PRIMS32 = new Set(["account_id", "AccountId", "[u8; 32]", "[u8;32]", "program_id", "ProgramId", "[u32; 8]", "[u32;8]"]);

const definedName = (d) => (typeof d === "string" ? d : d.name);

export class Idl {
  constructor(json) {
    this.json = json;
    this.name = json.name;
    this.defs = new Map();
    for (const t of json.types || []) this.defs.set(t.name, t.type || t);
    // an account may name a type of `types` ({"defined": "NameRecord"}) instead of spelling it out
    for (const a of json.accounts || []) this.defs.set(a.name, a.type && a.type.defined ? this.defs.get(definedName(a.type.defined)) : a.type);
    this.byDisc = new Map();
    for (const ins of json.instructions || []) {
      const d = ins.discriminator || [];
      if (d.length === 1) this.byDisc.set(d[0], ins);
    }
    this.errors = new Map((json.errors || []).map((e) => [e.code, e]));
    this.accountTypes = (json.accounts || []).map((a) => a.name);
  }

  /** Decode `bytes` as the named type, refusing leftovers. */
  decodeType(bytes, typeName) {
    const r = new Reader(bytes);
    const v = this.defined(r, typeName);
    r.finish(typeName);
    return v;
  }

  /** The instruction table's path: the one-byte discriminator, then the instruction's args in order. */
  decodeInstruction(bytes) {
    if (bytes.length === 0) throw new DecodeError("empty message");
    const ins = this.byDisc.get(bytes[0]);
    if (!ins) throw new DecodeError(`no instruction has discriminator ${bytes[0]}`);
    const r = new Reader(bytes, 1);
    const args = ins.args.map((a) => [a.name, this.value(r, a.type)]);
    r.finish(ins.name);
    return { ins, args };
  }

  /** Every account type of the IDL that decodes these actor-state bytes exactly (SPEL's try-all). */
  decodeAccount(bytes) {
    const out = [];
    for (const name of this.accountTypes) {
      try {
        out.push({ type: name, value: this.decodeType(bytes, name) });
      } catch (e) {
        if (!(e instanceof DecodeError) && !(e instanceof RangeError)) throw e;
      }
    }
    return out;
  }

  defined(r, name) {
    const td = this.defs.get(name);
    if (!td) throw new DecodeError(`undefined type ${name}`);
    if (td.kind === "struct") {
      return { k: "struct", name, tuple: td["x-rust"] === "tuple", fields: (td.fields || []).map((f) => [f.name, this.value(r, f.type)]) };
    }
    const idx = r.u8();
    if (idx >= td.variants.length) throw new DecodeError(`${name}: variant index ${idx} of ${td.variants.length}`);
    const v = td.variants[idx];
    return { k: "enum", name, variant: v.name, fields: (v.fields || []).map((f) => [f.name, this.value(r, f.type)]) };
  }

  value(r, t) {
    if (typeof t === "string") {
      if (t in INTS) {
        const w = INTS[t];
        let n = r.uint(w);
        if (t[0] === "i") {
          const big = BigInt(n);
          const half = 1n << BigInt(8 * w - 1);
          n = big >= half ? big - (half << 1n) : big;
          if (w <= 4) n = Number(n);
        }
        return { k: "int", ty: t, v: n };
      }
      if (t === "bool") return { k: "bool", v: r.bool() };
      if (t === "string") return { k: "str", v: r.string() };
      if (PRIMS32.has(t)) return { k: t.includes("ccount") ? "acct" : "bytes", ty: t, v: r.bytes(32) };
      throw new DecodeError(`unknown primitive ${t}`);
    }
    const [kind, inner] = Object.entries(t)[0];
    if (kind === "vec") {
      if (inner === "u8") return { k: "bytes", ty: "Vec<u8>", v: r.bytesVec() };
      return { k: "list", items: r.vec(() => this.value(r, inner)) };
    }
    if (kind === "array") {
      const [elem, n] = inner;
      if (elem === "u8") return { k: "bytes", ty: `[u8; ${n}]`, v: r.bytes(n) };
      const items = [];
      for (let i = 0; i < n; i++) items.push(this.value(r, elem));
      return { k: "list", items };
    }
    if (kind === "option") return { k: "opt", v: r.option(() => this.value(r, inner)) };
    if (kind === "defined") return this.defined(r, definedName(inner));
    throw new DecodeError(`type form ${kind}`);
  }
}

/** A decoded value as plain JSON (integers as decimal strings past 2^53, bytes as hex, account ids as base58). */
export function plain(v) {
  if (v === null || v === undefined) return null;
  switch (v.k) {
    case "int":
      return typeof v.v === "bigint" ? (v.v <= BigInt(Number.MAX_SAFE_INTEGER) && v.v >= -BigInt(Number.MAX_SAFE_INTEGER) ? Number(v.v) : v.v.toString()) : v.v;
    case "bool":
    case "str":
      return v.v;
    case "bytes":
      return hex(v.v);
    case "acct":
      return b58(v.v);
    case "list":
      return v.items.map(plain);
    case "opt":
      return v.v === null ? null : plain(v.v);
    case "struct":
      return Object.fromEntries(v.fields.map(([n, f]) => [n, plain(f)]));
    case "enum":
      return v.fields.length ? { [v.variant]: Object.fromEntries(v.fields.map(([n, f]) => [n, plain(f)])) } : v.variant;
    default:
      return String(v);
  }
}

/** The field `name` of a decoded struct or enum variant. */
export function field(v, name) {
  const f = v && v.fields && v.fields.find(([n]) => n === name);
  return f ? f[1] : undefined;
}
