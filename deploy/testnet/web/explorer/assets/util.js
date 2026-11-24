// Small encoding helpers shared by every page. No dependencies; runs in browsers and in Node (tests).

const HEX = Array.from({ length: 256 }, (_, i) => i.toString(16).padStart(2, "0"));

export function hex(bytes) {
  let s = "";
  for (let i = 0; i < bytes.length; i++) s += HEX[bytes[i]];
  return s;
}

export function fromHex(text) {
  const t = text.startsWith("0x") || text.startsWith("0X") ? text.slice(2) : text;
  if (t.length % 2 !== 0 || /[^0-9a-fA-F]/.test(t)) throw new Error("not hex");
  const out = new Uint8Array(t.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(t.substr(i * 2, 2), 16);
  return out;
}

const B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const B58_INDEX = new Map([...B58].map((c, i) => [c, i]));

export function b58(bytes) {
  let zeros = 0;
  while (zeros < bytes.length && bytes[zeros] === 0) zeros++;
  const digits = [];
  for (let i = zeros; i < bytes.length; i++) {
    let carry = bytes[i];
    for (let j = 0; j < digits.length; j++) {
      carry += digits[j] << 8;
      digits[j] = carry % 58;
      carry = (carry / 58) | 0;
    }
    while (carry > 0) {
      digits.push(carry % 58);
      carry = (carry / 58) | 0;
    }
  }
  let s = "1".repeat(zeros);
  for (let i = digits.length - 1; i >= 0; i--) s += B58[digits[i]];
  return s;
}

export function fromB58(text) {
  let zeros = 0;
  while (zeros < text.length && text[zeros] === "1") zeros++;
  const bytes = [];
  for (let i = zeros; i < text.length; i++) {
    const v = B58_INDEX.get(text[i]);
    if (v === undefined) throw new Error("not base58");
    let carry = v;
    for (let j = 0; j < bytes.length; j++) {
      carry += bytes[j] * 58;
      bytes[j] = carry & 0xff;
      carry >>= 8;
    }
    while (carry > 0) {
      bytes.push(carry & 0xff);
      carry >>= 8;
    }
  }
  const out = new Uint8Array(zeros + bytes.length);
  for (let i = 0; i < bytes.length; i++) out[out.length - 1 - i] = bytes[i];
  return out;
}

/** A LEZ account id (32 bytes) from what a person may paste: base58, `Public/<base58>`, or 64 hex digits. */
export function parseAccountId(text) {
  let t = String(text).trim();
  if (t.startsWith("Public/")) t = t.slice(7);
  if (/^(0x)?[0-9a-fA-F]{64}$/.test(t)) return fromHex(t);
  const b = fromB58(t);
  if (b.length !== 32) throw new Error("an account id is 32 bytes");
  return b;
}

export function b64decode(text) {
  if (typeof atob === "function") {
    const bin = atob(text);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  }
  return new Uint8Array(Buffer.from(text, "base64"));
}

export function b64encode(bytes) {
  if (typeof btoa === "function") {
    let bin = "";
    for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    return btoa(bin);
  }
  return Buffer.from(bytes).toString("base64");
}

export function concat(...parts) {
  const n = parts.reduce((a, p) => a + p.length, 0);
  const out = new Uint8Array(n);
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

export const utf8 = (s) => new TextEncoder().encode(s);

export function equalBytes(a, b) {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

/** u64 little-endian of a BigInt or number. */
export function u64le(v) {
  const out = new Uint8Array(8);
  let x = BigInt(v);
  for (let i = 0; i < 8; i++) {
    out[i] = Number(x & 0xffn);
    x >>= 8n;
  }
  return out;
}

/**
 * JSON.parse that keeps integers of 16 digits or more exact, as strings (LEZ's RPC writes u128 balances and
 * nonces as bare JSON numbers, which JavaScript's own parser rounds past 2^53).
 */
export function parseJsonLossless(text) {
  const safe = text.replace(/"(?:[^"\\]|\\.)*"|-?\d{16,}(?![.\deE])/g, (m) => (m[0] === '"' ? m : `"${m}"`));
  return JSON.parse(safe);
}

/** A BigInt from a JSON number, a decimal string or a BigInt. */
export const big = (v) => (typeof v === "bigint" ? v : BigInt(v));

export function fmtInt(v) {
  const s = big(v).toString();
  return s.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

export function shortHex(h, n = 8) {
  return h.length > 2 * n + 1 ? `${h.slice(0, n)}…${h.slice(-n)}` : h;
}

export function fmtTime(ms) {
  const n = Number(ms);
  if (n === 0) return "genesis (no timestamp)";
  if (!Number.isFinite(n) || n < 0) return "—";
  return new Date(n).toISOString().replace(".000Z", "Z").replace("T", " ");
}

export function fmtAge(ms, now = Date.now()) {
  if (Number(ms) === 0) return "genesis";
  const s = Math.round((now - Number(ms)) / 1000);
  if (!Number.isFinite(s)) return "—";
  const a = Math.abs(s);
  const sign = s < 0 ? "in " : "";
  const tail = s < 0 ? "" : " ago";
  if (a < 60) return `${sign}${a} s${tail}`;
  if (a < 3600) return `${sign}${Math.floor(a / 60)} min${tail}`;
  if (a < 86400) return `${sign}${(a / 3600).toFixed(1)} h${tail}`;
  return `${sign}${(a / 86400).toFixed(1)} d${tail}`;
}

export function fmtBytes(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / 1024 / 1024).toFixed(2)} MiB`;
}

/** An integer amount of the native token's smallest units, in whole tokens (config.native: {symbol, decimals}). */
export function fmtNative(units, native) {
  const v = big(units);
  if (!native || !native.symbol) return fmtInt(v);
  const d = BigInt(native.decimals || 0);
  const scale = 10n ** d;
  const whole = v / scale;
  const frac = (v % scale).toString().padStart(Number(d), "0").replace(/0+$/, "");
  return `${fmtInt(whole)}${frac ? "." + frac : ""} ${native.symbol}`;
}
