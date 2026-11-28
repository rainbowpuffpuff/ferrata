// The page's one choice of LEZ network codec. Account and transaction construction is version-specific; byte encoders,
// base58 ids and the fee calculation shared by both versions stay in their codecs (or rc3's shared helpers).
import * as rc3 from './lez.js';
import * as v040 from './lez040.js';

const accountNode = (account) => account?.node ?? account;
const paths = (codec, path) => typeof path === 'string' ? codec.parsePath(path) : path;

function network(version, codec, seq) {
  const is040 = version === 'v0.4.0';
  return {
    version, codec, seq, readOnly: false, error: null, is040, accountsAs: version,
    admissionKind: is040 ? '33-byte initial key state' : '32-byte public key',
    admissionForm: is040 ? 'initial key state (66 hex digits, starting 00)' : 'public key (64 hex digits)',
    deriveAccount(seed, path) { return codec.deriveNode(seed, paths(codec, path)); },
    importAccount(text) {
      if (!is040) return rc3.rawKeyNode(text);
      const secret = codec.unhex(String(text).trim());
      if (secret.length !== 32) throw new Error('an account secret is 32 bytes (64 hex digits)');
      return { secret, path: null, accountId: codec.publicAccountId(secret) };
    },
    admissionKeyOf(account) {
      const node = accountNode(account);
      return is040 ? codec.initialKey(node.secret) : node.pk;
    },
    /** Parse a first-recipient key and derive its account id. The other LEZ version's key gets a specific refusal. */
    parseAdmissionKey(text) {
      let t = String(text ?? '').trim();
      if (t.startsWith('0x')) t = t.slice(2);
      if (is040 && /^(0x)?[0-9a-fA-F]{64}$/.test(String(text ?? '').trim())) {
        throw new Error('That is a 32-byte public key from LEZ v0.4.0-rc3. This v0.4.0 network needs the recipient\'s 33-byte initial key state (66 hex digits, starting 00).');
      }
      if (!is040 && /^(0x)?00[0-9a-fA-F]{64}$/.test(String(text ?? '').trim())) {
        throw new Error('That is a 33-byte initial key state from LEZ v0.4.0. This v0.4.0-rc3 network needs the recipient\'s 32-byte public key (64 hex digits).');
      }
      const key = codec.unhex(t);
      if (is040) {
        if (key.length !== 33 || key[0] !== 0) throw new Error('A v0.4.0 admission key is a 33-byte initial key state: 66 hex digits, starting 00.');
        return { id: codec.accountIdForInitialKey(key), key };
      }
      if (key.length !== 32) throw new Error('An rc3 admission key is a 32-byte public key: 64 hex digits.');
      return { id: codec.accountIdOf(key), key };
    },
    buildTransfer(args) { return codec.buildTransfer(args); },
    buildCall(args) { return codec.buildCall(args); },
    feeFor(args) { return rc3.feeFor(args); },
    /** Prefer v0.4.0's receipt RPC; a supplied raw or decoded block works on either version. */
    async outcome(hash, block) {
      if (is040 && (block === undefined || block === null)) {
        const receipt = await seq.receipt(hash);
        if (receipt === null || receipt === undefined) return 'missing';
        if (typeof receipt.success !== 'boolean') throw new Error('getTransactionReceipt answered without a success boolean');
        return receipt.success ? 'applied' : 'reverted';
      }
      if (block === undefined || block === null) throw new Error('a block is needed to read an rc3 transaction outcome');
      const decoded = typeof block === 'string' ? codec.decodeBlock(block) : block;
      return codec.outcomeIn(decoded, hash);
    },
    // Messaging keys are stable account material, not a v0.4.0 per-transaction signing key.
    messagingSecretOf(account) { const n = accountNode(account); return is040 ? n.secret : n.ssk; },
    accountSecretHex(account) { const n = accountNode(account); return codec.hex(is040 ? n.secret : n.ssk); },
  };
}

function readOnlyNetwork(seq, cause) {
  const reason = cause?.message ?? String(cause);
  const error = `the network's LEZ version could not be read, so nothing is signed: ${reason}`;
  const refuse = () => { throw new Error(error); };
  return {
    version: 'unknown', codec: null, seq, readOnly: true, error, is040: false, accountsAs: null,
    async recheck() { return false; },
    admissionKind: 'unavailable admission key', admissionForm: 'admission key (network version unknown)',
    deriveAccount: refuse, importAccount: refuse, admissionKeyOf: refuse, parseAdmissionKey: refuse,
    buildTransfer: refuse, buildCall: refuse,
    feeFor(args) { return rc3.feeFor(args); },
    outcome: refuse, messagingSecretOf: refuse, accountSecretHex: refuse,
  };
}

const versionOf = (found) => (found === 'v0.4.0' ? 'v0.4.0' : 'v0.4.0-rc3');
const storageKey = (rpcUrl) => `ferrata.lezVersion ${rpcUrl}`;
function remembered(storage, rpcUrl) {
  try { const v = storage?.getItem(storageKey(rpcUrl)); return v === 'v0.4.0' || v === 'v0.4.0-rc3' ? v : null; } catch { return null; }
}
function remember(storage, rpcUrl, version) { try { storage?.setItem(storageKey(rpcUrl), version); } catch { /* not kept */ } }
function confirmed(rpcUrl, version, probe) {
  if (version === 'v0.4.0') return network('v0.4.0', v040, probe);
  // rc3's calls keep lez.js's own timeout (15 s, as before the port): the page's short timeout is the version probe's.
  return network('v0.4.0-rc3', rc3, new rc3.Sequencer(rpcUrl));
}

/**
 * The network as the page and the CLI use it, its version re-read before each signature (`recheck()`): `assumed` is
 * the version its accounts follow. Unread (offline, a timeout, a rate limit, a sequencer restart: `cause`), it still
 * derives and shows accounts but builds, signs and sends nothing; read and equal, it signs; another version, once read,
 * is final for the page (signing refused until a reload, whatever later reads say), so a page left open while its
 * network moves to another LEZ never signs the old format. A failed re-read refuses that one signature only.
 */
function watched(rpcUrl, assumed, probe, storage, timeoutMs, cause = null) {
  const net = confirmed(rpcUrl, assumed, probe);
  const signing = { buildTransfer: net.buildTransfer, buildCall: net.buildCall, outcome: net.outcome };
  const refusing = {
    buildTransfer() { throw new Error(net.error); },
    buildCall() { throw new Error(net.error); },
    async outcome() { throw new Error(net.error); },
  };
  const unreadable = (e) => `the network's LEZ version could not be read, so nothing is signed: ${e?.message ?? String(e)}`;
  let probing = null; // one read at a time: the timer, the online event and a send share it
  Object.assign(net, {
    mismatch: false,
    /** Read the version again; true when it is the one the accounts follow (signing allowed). */
    recheck() {
      if (net.mismatch) return Promise.resolve(false);
      probing ??= (async () => {
        let found;
        try { found = versionOf(await v040.detectLezVersion(new v040.Sequencer(rpcUrl, { timeoutMs }))); }
        catch (e) { net.error = unreadable(e); return false; }
        if (net.mismatch) return false;
        remember(storage, rpcUrl, found);
        if (found !== assumed) {
          net.mismatch = true;
          Object.assign(net, { version: 'unknown', readOnly: true, error: `refused before anything was signed or sent: the network runs LEZ ${found}, but this page's accounts follow LEZ ${assumed}; reload the page` }, refusing);
          return false;
        }
        Object.assign(net, { version: found, readOnly: false, error: null }, signing);
        return true;
      })().finally(() => { probing = null; });
      return probing;
    },
  });
  if (cause) Object.assign(net, { version: 'unknown', readOnly: true, error: unreadable(cause) }, refusing);
  return net;
}

/**
 * Detect once, then use only that network's account, transaction and outcome rules. Unknown networks never sign.
 * Without options a failed probe gives a network that refuses everything version-specific (the CLI). The page passes
 * `storage` (the last version read from each RPC is kept there) and `fallback` (the version its network runs): a failed
 * probe then gives the unread state of `watched` above, which shows accounts but signs nothing until `recheck()` reads
 * the same version. The page calls `recheck()` before every signature.
 */
export async function connect(rpcUrl, { storage = null, fallback = null, timeoutMs } = {}) {
  const probe = new v040.Sequencer(rpcUrl, { timeoutMs });
  let found;
  try { found = versionOf(await v040.detectLezVersion(probe)); }
  catch (e) {
    const assumed = remembered(storage, rpcUrl) ?? fallback;
    return assumed ? watched(rpcUrl, assumed, probe, storage, timeoutMs, e) : readOnlyNetwork(probe, e);
  }
  remember(storage, rpcUrl, found);
  return watched(rpcUrl, found, probe, storage, timeoutMs);
}
