// LEZ v0.4.0-rc3 sequencer JSON-RPC (lez/sequencer/service/rpc/src/lib.rs), read-only methods only. Blocks and
// transactions arrive as base64(borsh) and are decoded here; account ids travel as base58, hashes as hex.

import { b64decode, b58, hex, parseJsonLossless, big } from "./util.js";
import * as lezRc3 from "./lez.js";
import * as lez040 from "./lez040.js";

export class RpcError extends Error {
  constructor(code, message, data) {
    super(message);
    this.code = code;
    this.data = data;
  }
}

/** Where the pages read the chain: `?rpc=` (kept for the session), the config's `rpc`, else same-origin `/rpc`. */
export function rpcUrl(config) {
  const q = new URLSearchParams(location.search).get("rpc");
  if (q) {
    sessionStorage.setItem("ferrata-rpc", q);
    return q;
  }
  return sessionStorage.getItem("ferrata-rpc") || (config && config.rpc) || "/rpc";
}

export class Rpc {
  constructor(url, { timeoutMs = 15000, rangeLimit = 100 } = {}) {
    this.url = url;
    this.timeoutMs = timeoutMs;
    this.rangeLimit = Math.max(1, Math.min(100, Number(rangeLimit) || 100)); // the gateway refuses longer spans
    this.id = 0;
    this.blocks = new Map(); // `${version}:${id}` -> decoded block (only Finalized ones are kept)
    this.calls = 0;
    this.version = "v0.4.0-rc3";
    this.versionKnown = false;
    this.versionProbe = null;
  }

  /**
   * Pin the wire version after one successful probe. rc3 remains the fallback for a call whose probe fails for any
   * reason other than method-not-found; such a failure is not cached, so the next decode probes again.
   */
  async lezVersion() {
    if (this.versionKnown) return this.version;
    if (!this.versionProbe) {
      this.versionProbe = (async () => {
        try {
          // v0.4.0's SequencerStatus is tagged by `state` (lez/sequencer/service/protocol/src/lib.rs); any other
          // answer names no version: rc3 for this call, probed again on the next.
          const status = await this.call("getSequencerStatus");
          if (status && (status.state === "online" || status.state === "bootstrapping")) {
            this.version = "v0.4.0";
            this.versionKnown = true;
          } else {
            this.version = "v0.4.0-rc3";
          }
        } catch (e) {
          this.version = "v0.4.0-rc3";
          // Only v0.4.0 reports that its sequencer is not online yet.
          if (e instanceof RpcError && e.code === -31902) {
            this.version = "v0.4.0";
            this.versionKnown = true;
          } else if (e instanceof RpcError && e.code === -32601) {
            this.versionKnown = true;
          }
        } finally {
          this.versionProbe = null;
        }
        return this.version;
      })();
    }
    return this.versionProbe;
  }

  async decoder() {
    return await this.lezVersion() === "v0.4.0" ? lez040 : lezRc3;
  }

  /**
   * Decode with the chosen codec; while the version is unknown (a probe that named none: rc3 by default), bytes that
   * rc3's decoder refuses are tried as v0.4.0, and the decoder that read them is returned with the result.
   */
  decodeWith(kind, bytes, codec, sure) {
    try {
      return { codec, value: codec[kind](bytes) };
    } catch (e) {
      if (sure || codec === lez040) throw e;
      return { codec: lez040, value: lez040[kind](bytes) };
    }
  }

  cacheKey(id, codec) {
    return `${codec === lez040 ? "v0.4.0" : "v0.4.0-rc3"}:${Number(id)}`;
  }

  /**
   * One JSON-RPC call. A gateway's "slow down" (HTTP 429, JSON-RPC -32005, with Retry-After) is waited out and the call
   * made again, up to three times; any other error is thrown.
   */
  async call(method, params = []) {
    for (let attempt = 0; ; attempt++) {
      try {
        return await this.callOnce(method, params);
      } catch (e) {
        if (!(e instanceof RpcError) || e.code !== -32005 || attempt >= 3) throw e;
        const wait = Math.min(10, Math.max(1, Number(e.retryAfter) || 2 ** attempt));
        await new Promise((r) => setTimeout(r, wait * 1000));
      }
    }
  }

  async callOnce(method, params = []) {
    const ctrl = typeof AbortController === "function" ? new AbortController() : null;
    const timer = ctrl ? setTimeout(() => ctrl.abort(), this.timeoutMs) : null;
    this.calls++;
    let res;
    try {
      res = await fetch(this.url, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", id: ++this.id, method, params }),
        signal: ctrl ? ctrl.signal : undefined,
      });
    } catch (e) {
      throw new RpcError(-1, `cannot reach ${this.url}: ${e.message || e}`);
    } finally {
      if (timer) clearTimeout(timer);
    }
    const text = await res.text();
    let body;
    try {
      body = parseJsonLossless(text);
    } catch {
      throw new RpcError(res.status, `HTTP ${res.status}: not JSON-RPC (${text.slice(0, 120)})`);
    }
    if (body.error) {
      const err = new RpcError(body.error.code, body.error.message, body.error.data);
      err.retryAfter = res.headers.get("retry-after");
      throw err;
    }
    return body.result;
  }

  async health() {
    await this.call("checkHealth");
    return true;
  }

  async lastBlockId() {
    return Number(await this.call("getLastBlockId"));
  }

  async block(id) {
    const n = Number(id);
    const codec = await this.decoder(), sure = this.versionKnown;
    const key = this.cacheKey(n, codec);
    if (this.blocks.has(key)) return this.blocks.get(key);
    const b64 = await this.call("getBlock", [n]);
    if (b64 === null || b64 === undefined) return null;
    const { codec: used, value: block } = this.decodeWith("decodeBlock", b64decode(b64), codec, sure);
    if (block.bedrock_status === "Finalized") this.blocks.set(this.cacheKey(n, used), block);
    return block;
  }

  /**
   * Blocks [from, to] inclusive, newest last, through getBlockRange in pages of at most `rangeLimit`. A gateway that
   * refuses a page ("at most N blocks per getBlockRange") lowers the limit to its N for this and later calls.
   */
  async blockRange(from, to) {
    const out = [];
    const codec = await this.decoder(), sure = this.versionKnown;
    let a = Number(from);
    while (a <= Number(to)) {
      const n = this.rangeLimit;
      const b = Math.min(Number(to), a + n - 1);
      const cached = [];
      for (let i = a; i <= b; i++) {
        const key = this.cacheKey(i, codec);
        if (this.blocks.has(key)) cached.push(this.blocks.get(key));
      }
      if (cached.length === b - a + 1) {
        out.push(...cached);
        a = b + 1;
        continue;
      }
      let list;
      try {
        list = await this.call("getBlockRange", [a, b]);
      } catch (e) {
        const m = /at most (\d+) blocks/.exec(e.message || "");
        if (m && Number(m[1]) > 0 && Number(m[1]) < n) {
          this.rangeLimit = Number(m[1]);
          continue;
        }
        throw e;
      }
      for (const b64 of list || []) {
        const { codec: used, value: block } = this.decodeWith("decodeBlock", b64decode(b64), codec, sure);
        if (block.bedrock_status === "Finalized") this.blocks.set(this.cacheKey(block.header.block_id, used), block);
        out.push(block);
      }
      a = b + 1;
    }
    return out;
  }

  /** {tx, blockId} or null. */
  async transaction(hashHex) {
    const codec = await this.decoder(), sure = this.versionKnown;
    const r = await this.call("getTransaction", [hashHex.replace(/^0x/, "").toLowerCase()]);
    if (!r) return null;
    const [b64, blockId] = r;
    return { tx: this.decodeWith("decodeTransaction", b64decode(b64), codec, sure).value, blockId: Number(blockId) };
  }

  /** A v0.4.0 transaction receipt, with every receipt integer kept exact. rc3 has no such method. */
  async receipt(hashHex) {
    if (await this.lezVersion() !== "v0.4.0") throw new RpcError(-32601, "getTransactionReceipt requires LEZ v0.4.0");
    const r = await this.call("getTransactionReceipt", [String(hashHex).replace(/^0x/, "").toLowerCase()]);
    if (!r) return null;
    return {
      success: r.success,
      fee: {
        gas_exec: big(r.fee.gas_exec), gas_stor: big(r.fee.gas_stor),
        fee_base: big(r.fee.fee_base), tip: big(r.fee.tip), refund: big(r.fee.refund),
      },
    };
  }

  /** {nonce, states: [{program, data}]} or null; `program` and `data` as bytes. */
  async account(idBytes) {
    const a = await this.call("getAccount", [b58(idBytes)]);
    return a ? normalizeAccount(a) : null;
  }

  async accountView(accountBytes, programBytes) {
    const a = await this.call("getAccountView", [{ account_id: b58(accountBytes), program_account_id: b58(programBytes) }]);
    return a ? normalizeAccount(a) : null;
  }

  async balance(idBytes) {
    return big(await this.call("getAccountBalance", [b58(idBytes)]));
  }

  /** {name: image id hex}: each ProgramId is eight u32 words, shown as their little-endian bytes. */
  async programIds() {
    const m = await this.call("getProgramIds");
    const out = {};
    for (const [name, words] of Object.entries(m || {})) out[name] = imageIdHex(words);
    return out;
  }

  async channelId() {
    return String(await this.call("getChannelId"));
  }

  async feeState() {
    return await this.call("getFeeState");
  }
}

export function imageIdHex(words) {
  if (typeof words === "string") return words.replace(/^0x/, "");
  const b = new Uint8Array(32);
  const dv = new DataView(b.buffer);
  words.forEach((w, i) => dv.setUint32(i * 4, Number(w), true));
  return hex(b);
}

import { fromB58 } from "./util.js";

export function normalizeAccount(a) {
  const states = [];
  const map = (a.data && a.data.actor_states) || {};
  for (const [program, bytes] of Object.entries(map)) {
    states.push({ program: fromB58(program), data: Uint8Array.from(bytes, Number) });
  }
  return { nonce: big(a.nonce ?? 0), states };
}
