#!/usr/bin/env node
// Ferrata admin view, from the command line: runs the inline script of admin/index.html in a node:vm context
// (so the page and this dump share one implementation) and prints its text report.
//
//   node admin/dump.mjs <rpcUrl> <vault> [fromBlock]     read a vault and print the report (read-only RPC calls)
//   node admin/dump.mjs --selftest                       check keccak, SHA-256, the ABI codec and the vault
//                                                        encodings against fixed vectors; no network
//
// Exit codes: 0 read completed with no RPC error; 3 read completed but some read failed (listed in the report);
// 1 fatal (no chain, no code, bad input) or a self-test failure. Node 22, no packages.

import { readFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash, randomBytes } from 'node:crypto';
import vm from 'node:vm';

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, '..');

function loadAdmin() {
  const html = readFileSync(join(here, 'index.html'), 'utf8');
  const m = /<script id="ferrata-admin">([\s\S]*?)<\/script>/.exec(html);
  if (!m) throw new Error('admin/index.html has no <script id="ferrata-admin"> block');
  // Only what the page's script uses from its host: fetch (the RPC), timers, AbortController (request timeouts).
  const ctx = vm.createContext({ fetch, AbortController, setTimeout, clearTimeout, console });
  vm.runInContext(m[1], ctx, { filename: 'admin/index.html#ferrata-admin' });
  if (!ctx.FerrataAdmin) throw new Error('the inline script did not define globalThis.FerrataAdmin');
  return ctx.FerrataAdmin;
}

// ---------------------------------------------------------------------------------------------------------------
// Self-test. Expected values come from outside the page's code: Foundry's `cast` (keccak, sig, abi-encode,
// calldata; computed offline), node:crypto (SHA-256), the vault's own constants and the repository's vectors
// (vault/test/vectors/*.json, written from the Solidity vault), and EIP-55's examples.
// ---------------------------------------------------------------------------------------------------------------
const PATTERN = (n) => Uint8Array.from({ length: n }, (_, i) => (i * 7 + 3) & 255);
const KECCAK_VECTORS = [
  ['""', '', '0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470'],
  ['"abc"', 'abc', '0x4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45'],
  ['135 bytes (one block, 0x81 pad byte)', PATTERN(135), '0x00ef96af9cf4b24c7f269d922294444a197d0a33638c2e56634c57e892103a8f'],
  ['136 bytes (full block + padding block)', PATTERN(136), '0x742061bcad767ed4c4f5883b1dcb1aad11afdcc140dc469d953759b127b9f9ed'],
  ['137 bytes (two blocks)', PATTERN(137), '0xe3371f61e770abf254c34239c3b0099ad90594507415bc81dd0a10b9692bbf2a'],
  ['300 bytes (three blocks)', PATTERN(300), '0xfa75f2293be9f9a14dcdeeff53f7b91ff6a2b1331b13886e69077ab1cf8252a9']];
const SELECTOR_VECTORS = [
  ['transfer(address,uint256)', '0xa9059cbb'], ['depositRoot()', '0x8955fe8a'], ['tokenConfig(address)', '0xfe136c4e'],
  ['fees(address,uint256,uint8)', '0xcbcdf12e'], ['candidates(bytes32)', '0x1a0478d5'], ['checkpoints(uint256)', '0xb8a24252'],
  ['roleProposal()', '0xcc9b92a2'], ['retireOpensAt()', '0x88e69e22'], ['paused()', '0x5c975abb'], ['decimals()', '0x313ce567']];
const TOPIC_VECTORS = [
  ['Deposit(address,uint64,uint64,address,uint256,bytes32)', '0x83d2d1c34c8825843c9af84bec80378e997084e8c1d6d5a3c2a6e758fe05538e'],
  ['CandidateSubmitted(bytes32,bytes32,bytes32,uint64,uint64,bool,uint256,uint64,bytes32,uint32,uint32)', '0x58b1868fbc2fcdacde98160ba462d0bf29441c0a88a2f3c717df5a2dd12b1cee'],
  ['CheckpointFinalized(uint256,bytes32,bytes32,bytes32,uint64,uint64,bool,uint64,bytes32,uint32,uint32)', '0x1bb4a3986a611ac71897de8175d28f9ce1b37954cc39170bbb10c249d39c5443'],
  ['Release(bytes32,address,address,uint256,address,uint256,bytes32)', '0xa4dac6aee972c8703de5d2c8d8f28c9c3b2305bb6ff4a040a9c6c6523ba4a7eb'],
  ['TokenRegistered(address,uint256[],uint256[],uint256,uint256)', '0x157c3cd2a9ff899abde019f7aa55058b780d5bc2799126d80fa4619b13851612'],
  ['TokenCapSet(address,uint8,uint256,uint256)', '0x907fc09b0e7913051a1182de8185a2e544e49f01b4b2f0ef544825e6bf08d060'],
  ['RolesProposed(address[3],address,uint256)', '0x3186349aae794ad9e06816e261f8e3c5653e353d5360f40befcba5b2ba6fcacf'],
  ['FeesAnnounced(address,uint256,uint64,uint64,uint256[],uint256[],uint256[])', '0x1886df6d9fa60dea9f7ccb53ceb088d0a5915000ddb7767e39e2449351e4ce92'],
  ['FeesInForce(address,uint256,uint64,uint64,bytes32)', '0xb116813bda4a5628d8affb84d03f7215fa9e5d652f322d3c90dc7d48ab66558a'],
  ['OwedCredited(address,address,uint256)', '0x5e6777bbce5269e6bf9b6dbe08117b87391de5da8bd1cf9430e4fe3911ee436a'],
  ['Transfer(address,address,uint256)', '0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef']];
// The 40 vault errors of cli/src/codes.rs (VAULT_ERRORS), each `cast sig "<Name>()"`.
const ERROR_SELECTORS = {
  VaultRetired: '0x95961660', DepositsAreFrozen: '0x36d5cac2', TokenDepositsAreFrozen: '0xb6c684dd', DepositsDisabled: '0x717a1648',
  UseDepositETH: '0x491fe534', InvalidDenominationIndex: '0x84daa5ff', ZeroCommitment: '0x51bda03d', WrongDepositIndex: '0x93526816',
  WrongEthAmount: '0x3acace01', TransferAmountMismatch: '0xb0a6ea29', TreeFull: '0xb48f2cf8', TokenDepositCapExceeded: '0x0140df75',
  GlobalDepositCapExceeded: '0x89fd9b50', TokenNotRegistered: '0x259ba1ad', ReleasesAreFrozen: '0x6a62cb6b',
  TokenReleasesAreFrozen: '0xdad32ed3', BurnRootNotAccepted: '0x02d2e0be', ReleaseDelayNotElapsed: '0xedd7d80f',
  NullifierAlreadyUsed: '0xcad2ae02', InvalidRelayFee: '0x2cd454b1', ZeroRecipient: '0xd27b4443', InsufficientEscrow: '0x90b8ab88',
  TokenReleaseCapExceeded: '0xf1fb62e6', GlobalReleaseCapExceeded: '0x5f21a1ef', EthTransferFailed: '0x6d963f88',
  NothingOwed: '0x797dba54', VerifierStopped: '0xc8e9f884', CheckpointsAreFrozen: '0x79392e50', WrongMode: '0x2a580d6d',
  WaitNotOver: '0x2628a860', ParentNotFinal: '0x78e897c4', UnknownCandidate: '0x3c6ed8ee', UnknownBeacon: '0xecc81a8a',
  UnknownLeaf: '0x732ee0ce', AlreadyRefunded: '0xa85e6f1a', NotRefundable: '0x3742d1f6', RefundNotYetOpen: '0x96b70a71',
  InvalidMerklePath: '0x66b08299', DirectEthNotAccepted: '0x867783b7', FunctionNotFound: '0x403e7fa6'};
const EIP55 = ['0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed', '0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359', '0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB', '0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb'];

function selftest(A) {
  let pass = 0, failures = 0;
  const check = (name, got, want) => {
    const g = typeof got === 'bigint' ? got.toString() : JSON.stringify(got, (k, v) => (typeof v === 'bigint' ? v.toString() : v));
    const w = typeof want === 'bigint' ? want.toString() : JSON.stringify(want, (k, v) => (typeof v === 'bigint' ? v.toString() : v));
    if (g === w) { pass++; return; }
    failures++; console.log('FAIL ' + name + '\n     got  ' + g + '\n     want ' + w);
  };
  const throws = (name, fn) => { try { fn(); failures++; console.log('FAIL ' + name + ': no error thrown'); } catch (e) { pass++; } };
  const section = (s) => console.log('-- ' + s);

  section('keccak-256');
  for (const [name, input, want] of KECCAK_VECTORS) check('keccak256(' + name + ')', A.keccak256Hex(input), want);
  for (const [sig, want] of SELECTOR_VECTORS) check('selector ' + sig, A.selector(sig), want);
  for (const [sig, want] of TOPIC_VECTORS) check('topic ' + sig, A.eventTopic(sig), want);
  section('vault error selectors (cli/src/codes.rs)');
  const codesPath = join(repo, 'cli', 'src', 'codes.rs');
  let names = Object.keys(ERROR_SELECTORS);
  if (existsSync(codesPath)) {
    const src = readFileSync(codesPath, 'utf8');
    const block = /VAULT_ERRORS[^=]*=\s*\[([\s\S]*?)\];/.exec(src);
    const fromCodes = block ? Array.from(block[1].matchAll(/\(\s*"([A-Za-z]+)"\s*,/g), (m) => m[1]) : [];
    check('codes.rs lists the same errors as these vectors', fromCodes.slice().sort(), names.slice().sort());
    names = fromCodes.length ? fromCodes : names;
  } else console.log('   (cli/src/codes.rs not found next to admin/: using the built-in list)');
  for (const n of names) {
    check('selector ' + n + '()', A.selector(n + '()'), ERROR_SELECTORS[n]);
    check('decodeRevert names ' + n, A.decodeRevert(ERROR_SELECTORS[n]), 'reverted with ' + n + '()');
    check(n + ' is in the page\'s error table', A.VAULT_ERRORS.includes(n + '()'), true);
  }
  check('decodeRevert Error(string)', A.decodeRevert('0x08c379a0' + '0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000d5772617070656420457468657200000000000000000000000000000000000000'), 'Error("Wrapped Ether")');
  section('EIP-55 checksums');
  for (const a of EIP55) check('checksum ' + a, A.toChecksumAddress(a.toLowerCase()), a);

  section('SHA-256');
  check('sha256("")', A.sha256Hex(''), '0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855');
  check('sha256("abc")', A.sha256Hex('abc'), '0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
  check('LEAF_TAG = sha256("ferrata/vault/leaf/v2")', A.sha256Hex('ferrata/vault/leaf/v2'), '0x17b9508a0d0cbf61207c0fcb66f06ed92a6aeec110a54860479f0bf3cd6aff0b');
  check('CHECKPOINT_DOMAIN', A.sha256Hex('ferrata/vault/checkpoint/v3'), '0xbdb3b52df781536a6bcce7088d156b3951a00a45ec8bcf2805ba8406fbeb5ec7');
  check('RELEASE_DOMAIN', A.sha256Hex('ferrata/vault/release/v2'), '0x6e04f4b1fa66535e182a1ef2e4f5bfdfe44f7257f1663ef81ffa4f38d7696e51');
  for (const len of [1, 55, 56, 63, 64, 65, 119, 120, 128, 200, 1000]) {
    const b = randomBytes(len);
    check('sha256 of ' + len + ' random bytes = node:crypto', A.sha256Hex(new Uint8Array(b)), '0x' + createHash('sha256').update(b).digest('hex'));
  }
  check('empty deposit tree root (vault/README.md)', A.depositTreeRoot([]), '0x782d35b1fdad7d54e7a1b36a2ab1021e872c7692bb80fdd12bfc321e9e420409');

  const treePath = join(repo, 'vault', 'test', 'vectors', 'tree-v2.json');
  if (existsSync(treePath)) {
    section('deposit tree (vault/test/vectors/tree-v2.json)');
    const tv = JSON.parse(readFileSync(treePath, 'utf8'));
    const leaves = [];
    for (const d of tv.deposits) {
      const leaf = A.leafHash(BigInt(d.leafIndex), d.depositor, d.token, BigInt(d.amount), d.commitment);
      check('leaf ' + d.leafIndex, leaf, d.leaf.toLowerCase());
      leaves.push(A.fromHex(leaf));
      check('root after leaf ' + d.leafIndex, A.depositTreeRoot(leaves.slice()), d.rootAfterInsert.toLowerCase());
    }
    check('final root', A.depositTreeRoot(leaves), tv.finalRoot.toLowerCase());
  } else console.log('   (vault/test/vectors/tree-v2.json not found: tree vectors skipped)');

  const regPath = join(repo, 'vault', 'test', 'vectors', 'registry-storage.json');
  if (existsSync(regPath)) {
    section('published registry entries and fee schedules (vault/test/vectors/registry-storage.json)');
    const rv = JSON.parse(readFileSync(regPath, 'utf8'));
    const stored = new Map();
    for (const snap of rv.snapshots || []) for (const [k, v] of snap.storage || []) stored.set(k.toLowerCase(), v.toLowerCase());
    for (const t of rv.tokens) {
      const h = A.registryEntryHash(t.token.toLowerCase(), t.lez_definition, BigInt(t.decimals), BigInt(t.lez_unit), BigInt(t.unit_weight), t.denominations.map(BigInt), t.relay_fees.map(BigInt));
      check('entry hash of ' + t.token, h, t.entry_hash.toLowerCase());
      const slot = A.registrySlot(t.token.toLowerCase());
      if (stored.has(slot)) check('stored at its registry slot ' + t.token, stored.get(slot), t.entry_hash.toLowerCase());
    }
    for (const s of rv.schedules) {
      const h = A.feeScheduleHash(s.token.toLowerCase(), BigInt(s.index), BigInt(s.from_leaf), BigInt(s.announced_at), BigInt(s.effective_at), s.mint_fees.map(BigInt), s.burn_fees.map(BigInt), s.checkpoint_fees.map(BigInt));
      check('schedule hash ' + s.token + ' #' + s.index, h, s.schedule_hash.toLowerCase());
      const slot = A.feeSlot(s.token.toLowerCase(), BigInt(s.index));
      if (stored.has(slot)) check('stored at its fee slot ' + s.token + ' #' + s.index, stored.get(slot), s.schedule_hash.toLowerCase());
    }
    const slotHits = rv.tokens.filter((t) => stored.has(A.registrySlot(t.token.toLowerCase()))).length + rv.schedules.filter((s) => stored.has(A.feeSlot(s.token.toLowerCase(), BigInt(s.index)))).length;
    check('some registry/fee slots found in the storage snapshots', slotHits > 0, true);
  } else console.log('   (vault/test/vectors/registry-storage.json not found: registry vectors skipped)');

  section('ABI codec (inputs from cast abi-encode / cast calldata)');
  const A1 = '0x00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e8d4a51000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000038d7ea4c68000000000000000000000000000000000000000000000000000002386f26fc10000';
  check('decode (bool,bool,uint256,uint256,uint256[])', A.decodeAbi(['bool', 'bool', 'uint256', 'uint256', 'uint256[]'], A1), [true, false, 1000000000000n, 1n, [1000000000000000n, 10000000000000000n]]);
  const A2 = '0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000d5772617070656420457468657200000000000000000000000000000000000000';
  check('decode (string)', A.decodeAbi(['string'], A2), ['Wrapped Ether']);
  const A3 = '0x0000000000000000000000001111111111111111111111111111111111111111000000000000000000000000000000000000000000000000000000000000000000000000000000000000000079a95ae9a3f9000eb41f7cbab6a6f97826895c07000000000000000000000000000000000000000000000000000000006af46d6c';
  check('decode (address[3],uint256)', A.decodeAbi(['address[3]', 'uint256'], A3), [['0x1111111111111111111111111111111111111111', '0x0000000000000000000000000000000000000000', '0x79a95ae9a3f9000eb41f7cbab6a6f97826895c07'], 1794403692n]);
  const A4 = '0x000000000000000000000000000000000000000000000000000000006aee4480000000000000000000000000000000000000000000000000000000006af77f0000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000001e0000000000000000000000000000000000000000000000000000000000000028';
  check('decode (uint64,uint64,uint256[],uint256[],uint256[]) with an empty array', A.decodeAbi(['uint64', 'uint64', 'uint256[]', 'uint256[]', 'uint256[]'], A4), [1794000000n, 1794604800n, [10n, 20n], [], [30n, 40n]]);
  const A5 = '0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb73c457ba0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000004deadbeef00000000000000000000000000000000000000000000000000000000';
  check('decode (int256,bytes4,bytes)', A.decodeAbi(['int256', 'bytes4', 'bytes'], A5), [-5n, '0x73c457ba', '0xdeadbeef']);
  throws('decode refuses short data', () => A.decodeAbi(['uint256', 'uint256'], '0x' + '00'.repeat(40)));
  throws('decode refuses a bool word of 2', () => A.decodeAbi(['bool'], '0x' + '00'.repeat(31) + '02'));
  throws('decode refuses a uint8 word of 256', () => A.decodeAbi(['uint8'], '0x' + '00'.repeat(30) + '0100'));
  throws('decode refuses an address word with high bits', () => A.decodeAbi(['address'], '0x01' + '00'.repeat(31)));
  throws('decode refuses an out-of-range offset', () => A.decodeAbi(['string'], '0x' + '00'.repeat(31) + 'ff'));
  check('encode fees(address,uint256,uint8)', A.encodeCall('fees(address,uint256,uint8)', ['0x8E1140D771892abdbe6b0Ef03aDd081F2153B73b', 3, 1]), '0xcbcdf12e0000000000000000000000008e1140d771892abdbe6b0ef03add081f2153b73b00000000000000000000000000000000000000000000000000000000000000030000000000000000000000000000000000000000000000000000000000000001');
  check('encode owed(address,address)', A.encodeCall('owed(address,address)', ['0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07', '0x0000000000000000000000000000000000000000']), '0x28079e4a00000000000000000000000079a95ae9a3f9000eb41f7cbab6a6f97826895c070000000000000000000000000000000000000000000000000000000000000000');
  check('encode candidates(bytes32)', A.encodeCall('candidates(bytes32)', ['0xe7a78c0f00000000000000000000000000000000000000000000000000000001']), '0x1a0478d5e7a78c0f00000000000000000000000000000000000000000000000000000001');
  throws('encode refuses a uint8 of 256', () => A.encodeCall('tokenCap(address,uint8)', ['0x0000000000000000000000000000000000000000', 256]));

  section('event decoding');
  const dep = A.decodeLog({
    topics: ['0x83d2d1c34c8825843c9af84bec80378e997084e8c1d6d5a3c2a6e758fe05538e', '0x00000000000000000000000079a95ae9a3f9000eb41f7cbab6a6f97826895c07', '0x' + '00'.repeat(32)],
    data: '0x0000000000000000000000000000000000000000000000000000000000000007000000000000000000000000000000000000000000000000000000000000002900000000000000000000000000000000000000000000000000038d7ea4c680003d0c7f616a3f9f6f79bebf876122f90d3ee68933506804a217bf8816bf5c8aab',
    blockNumber: '0xb8cdd5', logIndex: '0x3', transactionHash: '0x' + 'ab'.repeat(32)});
  check('Deposit log', dep, { event: 'Deposit', args: { depositor: '0x79a95ae9a3f9000eb41f7cbab6a6f97826895c07', index: 7n, leafIndex: 41n, token: '0x0000000000000000000000000000000000000000', amount: 1000000000000000n, commitment: '0x3d0c7f616a3f9f6f79bebf876122f90d3ee68933506804a217bf8816bf5c8aab' }, block: 12111317, logIndex: 3, tx: '0x' + 'ab'.repeat(32) });
  const reg = A.decodeLog({
    topics: ['0x157c3cd2a9ff899abde019f7aa55058b780d5bc2799126d80fa4619b13851612', '0x0000000000000000000000008e1140d771892abdbe6b0ef03add081f2153b73b'],
    data: '0x000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000e0000000000000000000000000000000000000000000000000000000e8d4a510000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000038d7ea4c68000000000000000000000000000000000000000000000000000002386f26fc1000000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000009184e72a000',
    blockNumber: '0x1', logIndex: '0x0', transactionHash: '0x' + 'cd'.repeat(32)});
  check('TokenRegistered log', reg ? reg.args : null, { token: '0x8e1140d771892abdbe6b0ef03add081f2153b73b', denominations: [1000000000000000n, 10000000000000000n], relayFees: [0n, 10000000000000n], lezUnit: 1000000000000n, unitWeight: 1n });
  check('an unknown topic decodes to null', A.decodeLog({ topics: ['0x' + '11'.repeat(32)], data: '0x', blockNumber: '0x1' }), null);
  let allKnown = true;
  for (const d of A.EVENT_DEFS) { const e = A.parseEventDef(d); if (A.eventTopic(e.signature) !== e.topic) allKnown = false; }
  check('every event definition parses', allKnown, true);

  section('code scanning and report');
  check('PUSH20 scan skips PUSH data', A.push20Constants('0x6073' + '73' + 'aa'.repeat(20) + '7f73' + 'bb'.repeat(31) + '73' + 'ff'.repeat(20)), ['0x' + 'aa'.repeat(20)]);
  check('URL redaction', A.redactUrl('https://user:pw@rpc.example/v3/SECRETKEY?x=1'), 'https://<credentials>@rpc.example/<path hidden>?<query hidden>');
  check('URL redaction keeps a bare origin', A.redactUrl('https://ethereum-sepolia-rpc.publicnode.com'), 'https://ethereum-sepolia-rpc.publicnode.com');
  return { pass, failures };
}

async function main() {
  const args = process.argv.slice(2);
  const A = loadAdmin();
  if (args[0] === '--selftest') {
    const { pass, failures } = selftest(A);
    // The reader refuses bad input before any request.
    const st = await A.readVault('ftp://nowhere', '0x1234', '');
    const text = A.renderText(st);
    const okFatal = /FATAL/.test(text) && st.fatal;
    console.log(okFatal ? '' : 'FAIL readVault did not refuse a non-http URL');
    const total = failures + (okFatal ? 0 : 1);
    console.log((total ? 'SELFTEST FAILED: ' + total + ' failure(s), ' : 'SELFTEST PASSED: ') + (pass + (okFatal ? 1 : 0)) + ' checks passed');
    process.exit(total ? 1 : 0);
  }
  if (args.length < 2 || args.length > 3) {
    console.error('usage: node admin/dump.mjs <rpcUrl> <vault> [fromBlock]\n       node admin/dump.mjs --selftest');
    process.exit(1);
  }
  const [rpcUrl, vault, fromBlock] = args;
  const started = new Date();
  const st = await A.readVault(rpcUrl, vault, fromBlock, { onProgress: (m) => process.stderr.write('  ' + m + '\n') });
  const header = [
    '# Ferrata vault admin view: text dump (admin/dump.mjs running admin/index.html\'s script; read-only)',
    '# endpoint:   ' + A.redactUrl(rpcUrl),
    '# vault:      ' + vault,
    '# chain:      ' + (st.chain && st.chain.chainId !== undefined ? st.chain.chainId + ' (' + st.chain.name + ')' : 'not read'),
    '# read at:    block ' + (st.chain && st.chain.block !== undefined ? st.chain.block : 'n/a') +
      (st.chain && typeof st.chain.timestamp === 'bigint' ? ', block time ' + new Date(Number(st.chain.timestamp) * 1000).toISOString().replace('.000Z', 'Z') : ''),
    '# UTC time:   ' + started.toISOString() + ' (start of the read, local clock)',
    ''].join('\n');
  process.stdout.write(header + A.renderText(st));
  const errors = st.errors.filter((e) => e.severity !== 'info').length;
  process.exit(st.fatal ? 1 : errors ? 3 : 0);
}

main().catch((e) => { console.error('dump failed: ' + (e && e.stack ? e.stack : e)); process.exit(1); });
