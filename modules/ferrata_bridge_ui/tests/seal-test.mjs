// The view's sealed channel (qml/js/seal.js) under Node, as QML's JavaScript engine runs it (a `.pragma library` file):
// SHA-256 (FIPS 180-2's examples), HMAC-SHA256 (RFC 4231), X25519 (RFC 7748, §5.2 and §6.1), and the vector the
// module's own implementation is tested against (calls/src/seal.rs, `the_views_sealed_phrase_opens_in_the_module`):
// the same keys, ciphertexts and MACs on both sides.   node modules/ferrata_bridge_ui/tests/seal-test.mjs [--print]
import { createHash, createPrivateKey, createPublicKey, diffieHellman, randomBytes } from "node:crypto";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const src = readFileSync(new URL("../qml/js/seal.js", import.meta.url), "utf8").replace(/^\.pragma library\s*$/m, "");
// A context whose Math.random and Date.now are fixed, for the deterministic vector (QML's are the engine's own).
function load(fixed) {
  const math = Object.create(Math);
  if (fixed) math.random = () => 0.5;
  const date = fixed ? { now: () => 1764000000000 } : Date;
  const ctx = vm.createContext({ Math: math, Date: date, Uint8Array, Int32Array, Float64Array, String, parseInt, Array });
  vm.runInContext(src, ctx);
  return ctx;
}
const S = load(true);
const h = (s) => Uint8Array.from(Buffer.from(s, "hex"));
const hex = (b) => Buffer.from(b).toString("hex");
const t = (s) => new TextEncoder().encode(s);
let failed = 0;
function eq(name, got, want) {
  if (got === want) console.log(`ok   ${name}`);
  else { failed++; console.log(`FAIL ${name}\n     got  ${got}\n     want ${want}`); }
}

eq("sha256 empty", hex(S.sha256(new Uint8Array(0))), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
eq("sha256 abc", hex(S.sha256(t("abc"))), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
eq("sha256 two blocks", hex(S.sha256(t("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"))), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
// The padding's edges (55, 56, 63, 64, 65, 119, 120 bytes) against Node's own SHA-256.
const lens = [55, 56, 63, 64, 65, 119, 120, 1000];
eq("sha256 at the padding's edges", lens.map((n) => hex(S.sha256(new Uint8Array(n).fill(0x61)))).join(","),
   lens.map((n) => createHash("sha256").update(Buffer.alloc(n, 0x61)).digest("hex")).join(","));
eq("hmac rfc4231 #1", hex(S.hmac(new Uint8Array(20).fill(0x0b), t("Hi There"))), "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7");
eq("hmac rfc4231 #6", hex(S.hmac(new Uint8Array(131).fill(0xaa), t("Test Using Larger Than Block-Size Key - Hash Key First"))),
   "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54");
eq("x25519 rfc7748 5.2 #1", hex(S.x25519(h("a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4"), h("e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c"))),
   "c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552");
eq("x25519 rfc7748 5.2 #2", hex(S.x25519(h("4b66e9d4d1b4673c5ad22691957d6af5c11b6421e0ea01d42ca4169e7918ba0d"), h("e5210f12786811d3f4b7959d0538ae2c31dbe7106fc03c3efc4cd549c715a493"))),
   "95cbde9476e8907d7aade45cb4b873f88b595a68799fa152e6f8f7647aac7957");
const alice = h("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a"), bob = h("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb");
eq("x25519 rfc7748 6.1 alice", hex(S.x25519Base(alice)), "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a");
eq("x25519 rfc7748 6.1 bob", hex(S.x25519Base(bob)), "de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f");
eq("x25519 rfc7748 6.1 shared", hex(S.x25519(alice, S.x25519Base(bob))), "4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");
for (let i = 0; i < 8; i++) {
  const sk = randomBytes(32), u = randomBytes(32);
  u[31] &= 127;
  const priv = createPrivateKey({ key: Buffer.concat([Buffer.from("302e020100300506032b656e04220420", "hex"), sk]), format: "der", type: "pkcs8" });
  const pub = createPublicKey({ key: Buffer.concat([Buffer.from("302a300506032b656e032100", "hex"), u]), format: "der", type: "spki" });
  eq(`x25519 random #${i} (Node's own)`, hex(S.x25519(sk, u)), diffieHellman({ privateKey: priv, publicKey: pub }).toString("hex"));
}
eq("utf8", hex(S.utf8("a€😀")), "61e282acf09f9880");

// The vector shared with seal.rs: the module's one-time secret, the test phrase sealed to it by this view, a new
// phrase's words sealed back by the module, and call 7's MAC.
const moduleSecret = new Uint8Array(32).fill(0x42);
const modulePk = S.x25519Base(moduleSecret);
const phrase = t("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about");
const ENTROPY = "24".repeat(32);  // seal_key's entropy, fixed here
const sealed = S.sealPhrase(hex(modulePk), phrase, ENTROPY);
eq("no seal without the module's entropy", S.sealPhrase(hex(modulePk), phrase) === null && S.viewKeyFor(hex(modulePk), "24") === null, true);
const words = t("legal winner thank year wave sausage worth useful legal winner thank yellow");
const back = new Uint8Array(words); S.streamXor(sealed.keys.wordsEnc, back);
const vector = {
  module_secret: hex(moduleSecret), module_pk: hex(modulePk),
  view_key: sealed.viewKey, phrase_ct: sealed.ct, phrase_mac: sealed.mac,
  words_ct: hex(back), words_mac: hex(S.hmac(sealed.keys.wordsMac, back)),
  call7_mac: S.macCall(sealed.keys, 7, '{"method":"info","params":{}}'),
};
if (process.argv.includes("--print")) console.log(JSON.stringify(vector, null, 1));
const want = {
  module_pk: "132c442be010fbd57e72603328aa76e71fccc1503aae219327d14d9c9993f472",
  view_key: "ff854c15857d81eed2aff488fc21159467d9237bd12eec0640a5b28b397a3637",
  phrase_ct: "e8b88ff3a680ffa6b61317cb4856be235024974e70904d84b8da6ac7f4fa427ce5de0bb4e3c5dca4b37bebf7e757617cc7e1558622eb04acaabd5dc23d7a9df291ac33bf00007ccc8661b7b5ae76723c8cbb63076ddfb26cefdfc837ba",
  phrase_mac: "34c69a20aad6445d026f3e83730e800ec1e8dfc753052779d24edc078fbfe3d3",
  words_ct: "faf6875fa16f670447b59cfd944404dffa6445a1ede7f95073775582fbca4ab4bc06367b7ff973e2665b44f9f4d887a9281fa5bf9dcfcfd7ec482f688ee7290fffdbce9a3fed0f81d0f301",
  words_mac: "3fa1d584f8993ea258cdbc250428f52481d25a9ec3fb818a7edcc798c5855035",
  call7_mac: "9e2db54380de70a44661f2dc1d8bb9f0e3ad1580c4fc57e8165dbcc383e75fe7",
};
for (const k of Object.keys(want)) if (want[k]) eq(`vector ${k}`, vector[k], want[k]);
// The module's shared secret from its side, and the words opened by the view.
const shared = S.x25519(moduleSecret, h(vector.view_key));
const keys = S.derive(shared, h(vector.view_key), modulePk);
eq("module derives the view's keys", hex(keys.call), hex(sealed.keys.call));
const opened = S.openWords(sealed.keys, vector.words_ct, vector.words_mac);
eq("words open", opened ? Buffer.from(opened).toString() : "null", Buffer.from(words).toString());
eq("a changed MAC is refused", S.openWords(sealed.keys, vector.words_ct, "00" + vector.words_mac.slice(2)), null);
const ct = h(vector.phrase_ct); S.streamXor(keys.phraseEnc, ct);
eq("the module opens the phrase", Buffer.from(ct).toString(), Buffer.from(phrase).toString());
eq("phrase MAC", hex(S.hmac(keys.phraseMac, h(vector.phrase_ct))), vector.phrase_mac);
// Real randomness: two seals of one phrase share nothing.
const R = load(false);
const a = R.sealPhrase(hex(modulePk), phrase, ENTROPY), b = R.sealPhrase(hex(modulePk), phrase, ENTROPY);
eq("two seals differ", a.viewKey !== b.viewKey && a.ct !== b.ct, true);
console.log(failed ? `${failed} FAILED` : "all passed");
process.exit(failed ? 1 : 0);
