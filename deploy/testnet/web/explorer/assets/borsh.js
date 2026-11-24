// A strict borsh reader: little-endian integers (u64 and wider as BigInt), u32-prefixed vectors, one-byte option
// and enum tags. Every read checks bounds; `finish()` refuses trailing bytes.

export class DecodeError extends Error {}

export class Reader {
  constructor(bytes, pos = 0) {
    this.buf = bytes;
    this.pos = pos;
    this.dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  }
  need(n) {
    if (this.pos + n > this.buf.length) {
      throw new DecodeError(`needs ${n} byte(s) at ${this.pos}, ${this.buf.length - this.pos} left`);
    }
  }
  u8() {
    this.need(1);
    return this.buf[this.pos++];
  }
  u16() {
    this.need(2);
    const v = this.dv.getUint16(this.pos, true);
    this.pos += 2;
    return v;
  }
  u32() {
    this.need(4);
    const v = this.dv.getUint32(this.pos, true);
    this.pos += 4;
    return v;
  }
  u64() {
    this.need(8);
    const v = this.dv.getBigUint64(this.pos, true);
    this.pos += 8;
    return v;
  }
  u128() {
    const lo = this.u64();
    const hi = this.u64();
    return (hi << 64n) | lo;
  }
  uint(n) {
    if (n === 1) return this.u8();
    if (n === 2) return this.u16();
    if (n === 4) return this.u32();
    if (n === 8) return this.u64();
    if (n === 16) return this.u128();
    throw new DecodeError(`integer width ${n}`);
  }
  bool() {
    const b = this.u8();
    if (b > 1) throw new DecodeError(`bool byte ${b}`);
    return b === 1;
  }
  bytes(n) {
    this.need(n);
    const out = this.buf.subarray(this.pos, this.pos + n);
    this.pos += n;
    return out;
  }
  /** Vec<u8>. */
  bytesVec() {
    return this.bytes(this.u32());
  }
  string() {
    return new TextDecoder("utf-8", { fatal: true }).decode(this.bytesVec());
  }
  vec(item) {
    const n = this.u32();
    // Each element is at least one byte, except zero-sized ones we never decode: refuse lengths that cannot fit.
    if (n > this.buf.length - this.pos) throw new DecodeError(`vector of ${n} at ${this.pos} cannot fit`);
    const out = new Array(n);
    for (let i = 0; i < n; i++) out[i] = item(this);
    return out;
  }
  option(item) {
    const tag = this.u8();
    if (tag > 1) throw new DecodeError(`option tag ${tag}`);
    return tag ? item(this) : null;
  }
  get left() {
    return this.buf.length - this.pos;
  }
  finish(what = "value") {
    if (this.pos !== this.buf.length) throw new DecodeError(`${this.buf.length - this.pos} byte(s) left after ${what}`);
  }
}
