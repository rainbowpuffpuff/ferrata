.pragma library
// The view's side of the network session's sealed channel with ferrata_bridge_module (the module's side and the full
// description: modules/ferrata_bridge_module/rust-lib/calls/src/seal.rs). Logos Core's IPC carries a call's arguments
// in clear, and Qt's remote-objects debug logging writes them to Basecamp's log; so the recovery phrase never crosses
// it in clear: the view seals it to a one-time X25519 key of the module, a new phrase's words come back sealed to the
// view, and every later call of the session carries a MAC under a key only the view and the module hold.
//
// Here: SHA-256 (FIPS 180-4), HMAC-SHA256 (RFC 2104) and X25519 (RFC 7748; the field arithmetic and Montgomery ladder of
// TweetNaCl, public domain), on Uint8Arrays, which the caller wipes (wipe()) once used; QML strings cannot be wiped.
// Checked against the RFCs' vectors and the module's own implementation by tests/seal-test.mjs.

var LABEL = "ferrata/basecamp-seal/v1";
var CALL_LABEL = "ferrata/call/v1";

// ---- bytes ----------------------------------------------------------------------------------------------------------

function wipe(b) { if (b && b.fill) b.fill(0); }

function concat(parts) {
    var n = 0, i;
    for (i = 0; i < parts.length; i++) n += parts[i].length;
    var out = new Uint8Array(n), at = 0;
    for (i = 0; i < parts.length; i++) { out.set(parts[i], at); at += parts[i].length; }
    return out;
}

// UTF-8 of a JS string (the call bodies, which may hold any text the person typed).
function utf8(s) {
    var out = [], i, c;
    for (i = 0; i < s.length; i++) {
        c = s.charCodeAt(i);
        if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length) {
            var d = s.charCodeAt(i + 1);
            if (d >= 0xdc00 && d <= 0xdfff) { c = 0x10000 + ((c - 0xd800) << 10) + (d - 0xdc00); i++; }
        }
        if (c < 0x80) out.push(c);
        else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 63));
        else if (c < 0x10000) out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
        else out.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
    }
    return new Uint8Array(out);
}

function hex(b) {
    var s = "", i;
    for (i = 0; i < b.length; i++) s += (b[i] < 16 ? "0" : "") + b[i].toString(16);
    return s;
}

function unhex(s) {
    s = String(s).replace(/^0x/, "");
    if (s.length % 2 !== 0 || /[^0-9a-fA-F]/.test(s)) return null;
    var out = new Uint8Array(s.length / 2);
    for (var i = 0; i < out.length; i++) out[i] = parseInt(s.substr(2 * i, 2), 16);
    return out;
}

function be32(n) { return new Uint8Array([(n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255]); }

function be64(n) {
    var hi = Math.floor(n / 4294967296), lo = n >>> 0;
    return concat([be32(hi), be32(lo)]);
}

// Equal in time independent of where they differ.
function same(a, b) {
    if (!a || !b || a.length !== b.length) return false;
    var d = 0;
    for (var i = 0; i < a.length; i++) d |= a[i] ^ b[i];
    return d === 0;
}

// ---- SHA-256, HMAC-SHA256 -------------------------------------------------------------------------------------------

var K256 = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
];

function sha256(msg) {
    var H = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    var len = msg.length;
    var total = Math.ceil((len + 9) / 64) * 64;
    var p = new Uint8Array(total);
    p.set(msg);
    p[len] = 0x80;
    var bits = len * 8;
    p[total - 8] = 0; p[total - 7] = 0; p[total - 6] = 0;
    p[total - 5] = Math.floor(bits / 4294967296) & 255;
    p[total - 4] = (bits >>> 24) & 255; p[total - 3] = (bits >>> 16) & 255; p[total - 2] = (bits >>> 8) & 255; p[total - 1] = bits & 255;
    var w = new Int32Array(64), i, off;
    for (off = 0; off < total; off += 64) {
        for (i = 0; i < 16; i++)
            w[i] = (p[off + 4 * i] << 24) | (p[off + 4 * i + 1] << 16) | (p[off + 4 * i + 2] << 8) | p[off + 4 * i + 3];
        for (i = 16; i < 64; i++) {
            var x = w[i - 15], y = w[i - 2];
            var s0 = ((x >>> 7) | (x << 25)) ^ ((x >>> 18) | (x << 14)) ^ (x >>> 3);
            var s1 = ((y >>> 17) | (y << 15)) ^ ((y >>> 19) | (y << 13)) ^ (y >>> 10);
            w[i] = (w[i - 16] + s0 + w[i - 7] + s1) | 0;
        }
        var a = H[0], b = H[1], c = H[2], d = H[3], e = H[4], f = H[5], g = H[6], h = H[7];
        for (i = 0; i < 64; i++) {
            var S1 = ((e >>> 6) | (e << 26)) ^ ((e >>> 11) | (e << 21)) ^ ((e >>> 25) | (e << 7));
            var ch = (e & f) ^ (~e & g);
            var t1 = (h + S1 + ch + K256[i] + w[i]) | 0;
            var S0 = ((a >>> 2) | (a << 30)) ^ ((a >>> 13) | (a << 19)) ^ ((a >>> 22) | (a << 10));
            var maj = (a & b) ^ (a & c) ^ (b & c);
            var t2 = (S0 + maj) | 0;
            h = g; g = f; f = e; e = (d + t1) | 0; d = c; c = b; b = a; a = (t1 + t2) | 0;
        }
        H[0] = (H[0] + a) | 0; H[1] = (H[1] + b) | 0; H[2] = (H[2] + c) | 0; H[3] = (H[3] + d) | 0;
        H[4] = (H[4] + e) | 0; H[5] = (H[5] + f) | 0; H[6] = (H[6] + g) | 0; H[7] = (H[7] + h) | 0;
    }
    var out = new Uint8Array(32);
    for (i = 0; i < 8; i++) {
        out[4 * i] = (H[i] >>> 24) & 255; out[4 * i + 1] = (H[i] >>> 16) & 255;
        out[4 * i + 2] = (H[i] >>> 8) & 255; out[4 * i + 3] = H[i] & 255;
    }
    wipe(p); wipe(w);
    return out;
}

function hmac(key, msg) {
    var k = new Uint8Array(64);
    if (key.length > 64) { var kh = sha256(key); k.set(kh); wipe(kh); } else k.set(key);
    var inner = new Uint8Array(64 + msg.length), outer = new Uint8Array(96), i;
    for (i = 0; i < 64; i++) { inner[i] = k[i] ^ 0x36; outer[i] = k[i] ^ 0x5c; }
    inner.set(msg, 64);
    var ih = sha256(inner);
    outer.set(ih, 64);
    var out = sha256(outer);
    wipe(k); wipe(inner); wipe(outer); wipe(ih);
    return out;
}

// ---- X25519 (RFC 7748), TweetNaCl's arithmetic --------------------------------------------------------------------

function gf(init) {
    var r = new Float64Array(16);
    if (init) for (var i = 0; i < init.length; i++) r[i] = init[i];
    return r;
}

var _121665 = gf([0xdb41, 1]);

function car25519(o) {
    var i, v, c = 1;
    for (i = 0; i < 16; i++) {
        v = o[i] + c + 65535;
        c = Math.floor(v / 65536);
        o[i] = v - c * 65536;
    }
    o[0] += c - 1 + 37 * (c - 1);
}

function sel25519(p, q, b) {
    var t, c = ~(b - 1);
    for (var i = 0; i < 16; i++) {
        t = c & (p[i] ^ q[i]);
        p[i] ^= t;
        q[i] ^= t;
    }
}

function pack25519(o, n) {
    var i, j, b, m = gf(), t = gf();
    for (i = 0; i < 16; i++) t[i] = n[i];
    car25519(t); car25519(t); car25519(t);
    for (j = 0; j < 2; j++) {
        m[0] = t[0] - 0xffed;
        for (i = 1; i < 15; i++) {
            m[i] = t[i] - 0xffff - ((m[i - 1] >> 16) & 1);
            m[i - 1] &= 0xffff;
        }
        m[15] = t[15] - 0x7fff - ((m[14] >> 16) & 1);
        b = (m[15] >> 16) & 1;
        m[14] &= 0xffff;
        sel25519(t, m, 1 - b);
    }
    for (i = 0; i < 16; i++) {
        o[2 * i] = t[i] & 0xff;
        o[2 * i + 1] = t[i] >> 8;
    }
    wipe(m); wipe(t);
}

function unpack25519(o, n) {
    for (var i = 0; i < 16; i++) o[i] = n[2 * i] + (n[2 * i + 1] << 8);
    o[15] &= 0x7fff;
}

function A(o, a, b) { for (var i = 0; i < 16; i++) o[i] = a[i] + b[i]; }
function Z(o, a, b) { for (var i = 0; i < 16; i++) o[i] = a[i] - b[i]; }

function M(o, a, b) {
    var i, j, t = new Float64Array(31);
    for (i = 0; i < 16; i++)
        for (j = 0; j < 16; j++)
            t[i + j] += a[i] * b[j];
    for (i = 0; i < 15; i++) t[i] += 38 * t[i + 16];
    for (i = 0; i < 16; i++) o[i] = t[i];
    car25519(o);
    car25519(o);
}

function S(o, a) { M(o, a, a); }

function inv25519(o, i) {
    var c = gf(), a;
    for (a = 0; a < 16; a++) c[a] = i[a];
    for (a = 253; a >= 0; a--) {
        S(c, c);
        if (a !== 2 && a !== 4) M(c, c, i);
    }
    for (a = 0; a < 16; a++) o[a] = c[a];
    wipe(c);
}

// X25519(scalar, u): both 32 bytes; the scalar is clamped as RFC 7748 says.
function x25519(n, p) {
    var z = new Uint8Array(32), x = new Float64Array(80), r, i, q = new Uint8Array(32);
    var a = gf(), b = gf(), c = gf(), d = gf(), e = gf(), f = gf();
    for (i = 0; i < 31; i++) z[i] = n[i];
    z[31] = (n[31] & 127) | 64;
    z[0] &= 248;
    unpack25519(x, p);
    for (i = 0; i < 16; i++) {
        b[i] = x[i];
        d[i] = a[i] = c[i] = 0;
    }
    a[0] = d[0] = 1;
    for (i = 254; i >= 0; --i) {
        r = (z[i >>> 3] >>> (i & 7)) & 1;
        sel25519(a, b, r);
        sel25519(c, d, r);
        A(e, a, c);
        Z(a, a, c);
        A(c, b, d);
        Z(b, b, d);
        S(d, e);
        S(f, a);
        M(a, c, a);
        M(c, b, e);
        A(e, a, c);
        Z(a, a, c);
        S(b, a);
        Z(c, d, f);
        M(a, c, _121665);
        A(a, a, d);
        M(c, c, a);
        M(a, d, f);
        M(d, b, x);
        S(b, e);
        sel25519(a, b, r);
        sel25519(c, d, r);
    }
    for (i = 0; i < 16; i++) {
        x[i + 16] = a[i];
        x[i + 32] = c[i];
        x[i + 48] = b[i];
        x[i + 64] = d[i];
    }
    var x32 = x.subarray(32), x16 = x.subarray(16);
    inv25519(x32, x32);
    M(x16, x16, x32);
    pack25519(q, x16);
    wipe(z); wipe(x); wipe(a); wipe(b); wipe(c); wipe(d); wipe(e); wipe(f);
    return q;
}

var BASE = new Uint8Array(32);
BASE[0] = 9;
function x25519Base(n) { return x25519(n, BASE); }

// ---- the session's channel -----------------------------------------------------------------------------------------

// The keys of one agreement: X25519(view secret, module key), bound to both public keys.
function derive(shared, viewPk, modulePk) {
    var ikm = concat([shared, viewPk, modulePk]);
    var prk = hmac(utf8(LABEL), ikm);
    var k = {
        phraseEnc: hmac(prk, utf8("phrase-enc")), phraseMac: hmac(prk, utf8("phrase-mac")),
        wordsEnc: hmac(prk, utf8("words-enc")), wordsMac: hmac(prk, utf8("words-mac")),
        call: hmac(prk, utf8("call-mac"))
    };
    wipe(ikm); wipe(prk);
    return k;
}

function wipeKeys(k) {
    if (!k) return;
    wipe(k.phraseEnc); wipe(k.phraseMac); wipe(k.wordsEnc); wipe(k.wordsMac); wipe(k.call);
}

// data XOR HMAC(key, be32(0)) ‖ HMAC(key, be32(1)) ‖ …, in place.
function streamXor(key, data) {
    for (var block = 0; block * 32 < data.length; block++) {
        var ks = hmac(key, be32(block));
        for (var i = 0; i < 32 && block * 32 + i < data.length; i++) data[block * 32 + i] ^= ks[i];
        wipe(ks);
    }
}

// The view's one-time secret: hashed from 32 bytes of the operating system's randomness the module sent with its key
// (seal_key's `entropy`: QML has no cryptographic generator; Math.random is Qt's one process-wide MT19937, which any
// mini-app in Basecamp's window can sample), the phrase when there is one, and Math.random and the time besides.
function viewSecret(modulePk, phrase, entropy) {
    var noise = new Uint8Array(64), i;
    for (i = 0; i < 64; i++) noise[i] = Math.floor(Math.random() * 256);
    var parts = [entropy, utf8(LABEL + "/view-secret"), modulePk, noise, utf8(String(Date.now()))];
    if (phrase) parts.push(phrase);
    var all = concat(parts);
    var sk = sha256(all);
    wipe(all); wipe(noise);
    return sk;
}

// No agreement without the module's 32 bytes of entropy.
function agree(modulePkHex, phrase, entropyHex) {
    var modulePk = unhex(modulePkHex), entropy = unhex(entropyHex);
    if (!modulePk || modulePk.length !== 32 || !entropy || entropy.length !== 32) return null;
    var sk = viewSecret(modulePk, phrase, entropy);
    wipe(entropy);
    var viewPk = x25519Base(sk), shared = x25519(sk, modulePk);
    wipe(sk);
    var zero = true;
    for (var i = 0; i < 32; i++) if (shared[i] !== 0) zero = false;
    if (zero) { wipe(shared); return null; }
    var keys = derive(shared, viewPk, modulePk);
    wipe(shared);
    return { viewKey: hex(viewPk), keys: keys };
}

// Seals the phrase (its UTF-8 bytes, which the caller then wipes) to the module's one-time key: the open_network
// parameters view_key and phrase_sealed, and the session's keys (kept by the view for the MACs).
function sealPhrase(modulePkHex, phrase, entropyHex) {
    var a = agree(modulePkHex, phrase, entropyHex);
    if (!a) return null;
    var ct = new Uint8Array(phrase.length);
    ct.set(phrase);
    streamXor(a.keys.phraseEnc, ct);
    var tag = hmac(a.keys.phraseMac, ct);
    var out = { viewKey: a.viewKey, ct: hex(ct), mac: hex(tag), keys: a.keys };
    wipe(ct); wipe(tag);
    return out;
}

// For a new phrase: the view's key alone (the module seals the words to it).
function viewKeyFor(modulePkHex, entropyHex) { return agree(modulePkHex, null, entropyHex); }

// The words the module sealed to the view: their bytes (wiped by the caller), or null when the MAC does not hold.
function openWords(keys, ctHex, macHex) {
    var ct = unhex(ctHex), tag = unhex(macHex);
    if (!ct || !tag || !same(hmac(keys.wordsMac, ct), tag)) return null;
    streamXor(keys.wordsEnc, ct);
    return ct;
}

// The MAC of one call of the session: HMAC(call key, "ferrata/call/v1" ‖ be64(n) ‖ the call's JSON text).
function macCall(keys, n, body) {
    var msg = concat([utf8(CALL_LABEL), be64(n), utf8(body)]);
    var tag = hmac(keys.call, msg);
    wipe(msg);
    return hex(tag);
}
