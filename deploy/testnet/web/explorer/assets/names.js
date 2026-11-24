// Names (ferrata_names) and encrypted messages (ferrata_msg), read for the explorer. Names: `name(n)` is the PDA
// SHA-256("/LEE/v0.2/AccountId/PDA/" ‖ program ‖ SHA-256("ferrata-names/v1/name/" ‖ n)) holding a NameRecord;
// `user(a)` is the actor (a, program) holding a UserState (names owned, primary name). An account's name is its
// primary name only when that name is live and its record points back at the account. Messages: the actor
// (a, ferrata_msg) holds a Mailbox; the explorer shows who, when and how large, never a body.

import { sha256 } from "./sha256.js";
import { forPublicPda } from "./lez.js";
import { utf8, hex } from "./util.js";
import { plain, field } from "./idl.js";

export const GRACE_MS = 30 * 86_400_000;

export function normalizeName(s) {
  return String(s).trim().toLowerCase().replace(/^@/, "").replace(/\.lez$/, "");
}

/** The program's naming rule: 3-32 of a-z, 0-9 and -, no leading, trailing or double hyphen. */
export function wellFormedName(n) {
  return typeof n === "string" && n.length >= 3 && n.length <= 32 && /^[a-z0-9-]+$/.test(n) && !n.startsWith("-") && !n.endsWith("-") && !n.includes("--");
}

export function nameAccount(program, name) {
  return forPublicPda(program, sha256(utf8("ferrata-names/v1/name/"), utf8(name)));
}

export function nameStatus(rec, now = Date.now()) {
  if (!rec) return "free";
  if (now < rec.expires_ms) return "live";
  if (now < rec.expires_ms + GRACE_MS) return "grace";
  return "lapsed";
}

const stateOf = (view, program) => {
  const st = view && view.states.find((s) => hex(s.program) === hex(program));
  return st && st.data.length ? st.data : null;
};

export class NameService {
  constructor(rpc, program, idl) {
    this.rpc = rpc;
    this.program = program;
    this.idl = idl;
    this.reverseCache = new Map();
  }

  /** The record of `name` (any status), or null. Fields as decoded: account fields are 32-byte arrays. */
  async record(name) {
    const n = normalizeName(name);
    if (!wellFormedName(n)) return null;
    const acct = nameAccount(this.program, n);
    const data = stateOf(await this.rpc.accountView(acct, this.program), this.program);
    if (!data) return { name: n, account: acct, missing: true };
    const v = this.idl.decodeType(data, "NameRecord");
    const opt = (k) => (field(v, k).v ? field(v, k).v.v : null);
    return {
      name: field(v, "name").v,
      account: acct,
      owner: field(v, "owner").v,
      target: opt("target"),
      receiver: field(v, "receiver").v !== null,
      registered_ms: Number(field(v, "registered_ms").v),
      expires_ms: Number(field(v, "expires_ms").v),
      offer: opt("offer"),
    };
  }

  /** The account's UserState: {owned, primary, commit} (primary unchecked). */
  async user(account) {
    const data = stateOf(await this.rpc.accountView(account, this.program), this.program);
    if (!data) return { owned: 0, primary: null, commit: null };
    const p = plain(this.idl.decodeType(data, "UserState"));
    return { owned: p.owned, primary: p.primary, commit: p.commit };
  }

  /** The account's name: its primary, only if live and pointing back at it. Cached for the page's life. */
  reverse(account) {
    const k = hex(account);
    if (!this.reverseCache.has(k)) {
      this.reverseCache.set(k, (async () => {
        try {
          const u = await this.user(account);
          if (!u.primary) return null;
          const r = await this.record(u.primary);
          return r && !r.missing && nameStatus(r) === "live" && r.target && hex(r.target) === k ? u.primary : null;
        } catch {
          return null;
        }
      })());
    }
    return this.reverseCache.get(k);
  }
}

/** A Mailbox as the explorer may show it: no bodies, only their sizes. */
export function mailboxSummary(idl, data) {
  const v = idl.decodeType(data, "Mailbox");
  const key = field(v, "key").v;
  const inbox = field(v, "inbox").items.map((s) => ({
    seq: Number(field(s, "seq").v),
    from: field(s, "from").v,
    at_ms: Number(field(s, "at_ms").v),
    size: field(s, "body").v.length,
  }));
  return {
    key: key ? { epoch: Number(field(key, "epoch").v), hybrid: field(key, "mlkem768").v !== null } : null,
    policy: field(v, "policy").variant,
    contacts: field(v, "contacts").items.length,
    blocked: field(v, "blocked").items.length,
    next_seq: Number(field(v, "next_seq").v),
    sent_total: Number(field(v, "sent_total").v),
    inbox,
  };
}
