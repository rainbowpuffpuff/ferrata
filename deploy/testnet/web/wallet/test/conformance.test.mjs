// The wallet's LEZ encoding against LEZ v0.4.0-rc3's own pinned test vectors (node --test).
// Sources: lee/key_protocol/src/key_management/key_tree/keys_public.rs (master_keys_generation, child_keys_generation),
// lee/state_machine/src/signature/private_key.rs (tweak_deterministic), public_transaction/message.rs
// (a_public_message_has_a_pinned_layout_and_hash), signature/bip340_test_vectors.rs (BIP-340's own vectors).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import * as lez from '../lez.js';
import { sha256 } from '../vendor/noble.js';

const B = (a) => Uint8Array.from(a);
const SEED = B([
  88, 189, 37, 237, 199, 125, 151, 226, 69, 153, 165, 113, 191, 69, 188, 221, 9, 34, 173,
  134, 61, 109, 34, 103, 121, 39, 237, 14, 107, 194, 24, 194, 191, 14, 237, 185, 12, 87, 22,
  227, 38, 71, 17, 144, 251, 118, 217, 115, 33, 222, 201, 61, 203, 246, 121, 214, 6, 187,
  148, 92, 44, 253, 210, 37,
]);

test('master keys: LEZ keys_public.rs master_keys_generation', () => {
  const k = lez.masterNode(SEED);
  assert.deepEqual(k.cc, B([184, 94, 197, 114, 84, 79, 170, 62, 107, 107, 141, 196, 11, 255, 15, 165, 7, 40, 93,
    211, 244, 153, 12, 70, 10, 174, 141, 69, 117, 167, 165, 81]));
  assert.deepEqual(k.sk, B([142, 140, 44, 81, 255, 159, 131, 163, 210, 67, 198, 176, 43, 243, 163, 35, 242, 200,
    232, 99, 69, 240, 63, 16, 33, 104, 8, 152, 243, 153, 180, 169]));
  assert.deepEqual(k.ssk, B([241, 47, 167, 208, 182, 77, 106, 158, 182, 41, 17, 3, 91, 229, 165, 35, 90, 33, 145,
    202, 246, 65, 127, 65, 124, 240, 165, 152, 127, 50, 60, 198]));
  assert.deepEqual(k.pk, B([43, 138, 92, 79, 223, 49, 90, 162, 205, 76, 143, 151, 96, 77, 10, 85, 179, 208, 244,
    71, 251, 191, 237, 226, 120, 247, 194, 57, 117, 180, 96, 65]));
});

test('child keys: LEZ keys_public.rs child_keys_generation (index 2^31 + 13)', () => {
  const k = lez.childNode(lez.masterNode(SEED), 2 ** 31 + 13);
  assert.deepEqual(k.cc, B([184, 162, 65, 125, 129, 202, 96, 126, 157, 15, 189, 122, 22, 152, 31, 107, 244, 188,
    215, 30, 70, 205, 164, 142, 6, 152, 106, 147, 160, 1, 168, 168]));
  assert.deepEqual(k.sk, B([222, 78, 224, 138, 167, 32, 235, 208, 192, 129, 121, 150, 204, 149, 151, 33, 82, 109,
    238, 245, 20, 106, 70, 126, 120, 66, 165, 169, 241, 242, 224, 10]));
  assert.deepEqual(k.ssk, B([103, 101, 18, 63, 86, 198, 110, 120, 163, 160, 181, 249, 184, 163, 7, 38, 132, 223, 72,
    208, 74, 223, 16, 110, 60, 227, 167, 192, 89, 28, 14, 222]));
  assert.deepEqual(k.pk, B([107, 153, 105, 58, 6, 157, 131, 253, 141, 130, 168, 182, 82, 2, 99, 26, 211, 22, 55,
    203, 23, 34, 236, 147, 86, 156, 194, 114, 89, 77, 219, 173]));
  assert.equal(lez.pathString(k.path), '/2147483661');
  assert.deepEqual(lez.parsePath('/2147483661'), [2147483661]);
});

test('tweak: LEZ private_key.rs tweak_deterministic, and its refusals', () => {
  assert.deepEqual(lez.tweak(new Uint8Array(32).fill(1)), B([242, 210, 33, 19, 65, 108, 136, 176, 179, 128, 110, 210,
    107, 193, 168, 112, 206, 171, 86, 238, 131, 10, 39, 36, 44, 39, 246, 20, 46, 193, 204, 66]));
  assert.throws(() => lez.tweak(new Uint8Array(32)));
  assert.throws(() => lez.tweak(new Uint8Array(32).fill(0xff)));
});

test('public message: LEZ message.rs a_public_message_has_a_pinned_layout_and_hash', () => {
  const id = new Uint8Array(32).fill(42);
  const to = { account: id, program: new Uint8Array(32) };
  const bytes = lez.encodeMessage({ actors: [to], signers: [id], to, data: B([0]), fee: null, nonces: [[id, 1n]], evidence: [] });
  const expected = lez.concat(
    B([1, 0, 0, 0]), id, new Uint8Array(32), // actors
    B([1, 0, 0, 0]), id,                      // authorized accounts
    B([0, 0, 0, 0]),                          // promotions
    id, new Uint8Array(32),                   // root.to
    B([1, 0, 0, 0]), B([0]),                  // root.message
    B([0]),                                   // fee: None
    B([1, 0, 0, 0]), id, B([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]), // nonces
    B([0, 0, 0, 0]),                          // admission evidence
  );
  assert.deepEqual(bytes, expected);
  const prefix = new Uint8Array(32); prefix.set(new TextEncoder().encode('/LEE/v0.3/Message/Public/'));
  assert.deepEqual(lez.messageHash(bytes), sha256(lez.concat(prefix, expected)));
});

test('BIP-340 signing: the vectors LEZ tests Signature::new_with_aux_random with', () => {
  const vectors = [
    ['0000000000000000000000000000000000000000000000000000000000000003', 'F9308A019258C31049344F85F89D5229B531C845836F99B08601F113BCE036F9',
      '0000000000000000000000000000000000000000000000000000000000000000', '0000000000000000000000000000000000000000000000000000000000000000',
      'E907831F80848D1069A5371B402410364BDF1C5F8307B0084C55F1CE2DCA821525F66A4A85EA8B71E482A74F382D2CE5EBEEE8FDB2172F477DF4900D310536C0'],
    ['B7E151628AED2A6ABF7158809CF4F3C762E7160F38B4DA56A784D9045190CFEF', 'DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659',
      '0000000000000000000000000000000000000000000000000000000000000001', '243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89',
      '6896BD60EEAE296DB48A229FF71DFE071BDE413E6D43F917DC8DCF8C78DE33418906D11AC976ABCCB20B091292BFF4EA897EFCB639EA871CFA95F6DE339E4B0A'],
    ['C90FDAA22168C234C4C6628B80DC1CD129024E088A67CC74020BBEA63B14E5C9', 'DD308AFEC5777E13121FA72B9CC1B7CC0139715309B086C960E18FD969774EB8',
      'C87AA53824B4D7AE2EB035A2B5BBBCCC080E76CDC6D1692C4B0B62D798E6D906', '7E2D58D8B3BCDF1ABADEC7829054F90DDA9805AAB56C77333024B9D0A508B75C',
      '5831AAEED7B44BB74E5EAB94BA9D4294C49BCF2A60728D8B4C200F50DD313C1BAB745879A5AD954A72C45A91C3A51D3C7ADEA98D82F8481E0E1E03674A6F3FB7'],
  ];
  for (const [sk, pk, aux, msg, sig] of vectors) {
    const s = lez.sign(lez.unhex(sk), lez.unhex(msg), lez.unhex(aux));
    assert.equal(lez.hex(s).toUpperCase(), sig);
    assert.equal(lez.hex(lez.publicKeyOf(lez.unhex(sk))).toUpperCase(), pk);
    assert.ok(lez.verify(s, lez.unhex(msg), lez.unhex(pk)));
  }
});

test('the whole vector file of LEZ, when LEZ rc3 is beside the repository', { skip: !existsSync(lezFile('lee/state_machine/src/signature/bip340_test_vectors.rs')) }, () => {
  const src = readFileSync(lezFile('lee/state_machine/src/signature/bip340_test_vectors.rs'), 'utf8');
  const blocks = src.split('TestVector {').slice(1);
  let checked = 0;
  for (const b of blocks) {
    const get = (field) => { const m = b.match(new RegExp(field + '[^"]*"([0-9A-F]+)"')); return m && m[1]; };
    if (!/seckey: Some/.test(b) || !/aux_rand: Some/.test(b) || !/verification_result: true/.test(b)) continue;
    const s = lez.sign(lez.unhex(get('seckey')), lez.unhex(get('message')), lez.unhex(get('aux_rand')));
    assert.equal(lez.hex(s).toUpperCase(), get('signature'));
    checked++;
  }
  assert.ok(checked >= 4, `checked ${checked}`);
});

function lezFile(rel) {
  return fileURLToPath(new URL('../../../../../../upstream/lez-v0.4.0-rc3/' + rel, import.meta.url));
}

test('a transfer round-trips through the decoder, and its signature holds', () => {
  const from = lez.deriveNode(SEED, [0]);
  const toNode = lez.deriveNode(SEED, [1]);
  const tx = lez.buildTransfer({ from, to: toNode.accountId, toKey: toNode.pk, amount: 123456789n, nonce: 7n,
    gasLimit: 50000n, maxFee: 999999n });
  const d = lez.decodeWire(tx.wire);
  assert.equal(d.kind, 'public');
  assert.equal(d.hash, tx.hash);
  const t = lez.nativeTransferOf(d.tx);
  assert.deepEqual(t.from, from.accountId);
  assert.deepEqual(t.to, toNode.accountId);
  assert.equal(t.amount, 123456789n);
  assert.equal(d.tx.nonces[0][1], 7n);
  assert.deepEqual(d.tx.evidence[0].key, toNode.pk);
  assert.ok(lez.verify(d.tx.witnesses[0].sig, lez.messageHash(d.tx.messageBytes), from.pk));
  assert.throws(() => lez.buildTransfer({ from, to: toNode.accountId, toKey: from.pk, amount: 1n, nonce: 0n, gasLimit: 1n, maxFee: 1n }));
});
