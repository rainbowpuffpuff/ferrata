// The bridge tab's review against ethskills' frontend-ux, qa and wallets checklists (27 November 2026): a browser
// wallet's errors and the vault's refusals in plain words, next to the action; the cost priced live before the
// signature, with the balance it needs; a name typed as the burn's recipient refused with its reason.
import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DEPOSIT_REFUSALS, revertDataOf, walletError, costOf, burnRecipient } from '../bridge.js';
import { keccak } from '../eth.js';
import { hex } from '../lez.js';

const REPO = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..', '..', '..');
const sel = (sig) => '0x' + hex(keccak(new TextEncoder().encode(sig)).subarray(0, 4));

test('wallet errors in plain words: declined, already waiting, not allowed, disconnected, no Sepolia ETH', () => {
  assert.match(walletError({ code: 4001, message: 'MetaMask Tx Signature: User denied transaction signature.' }), /^You declined it in your browser wallet, so it was not sent\.$/);
  assert.match(walletError({ message: 'User rejected the request.' }), /declined/);
  assert.match(walletError({ code: 4001, message: 'x' }), /declined/);                // by its code alone
  assert.match(walletError({ code: -32002, message: 'Request of type eth_requestAccounts already pending' }), /already has a request waiting/);
  assert.match(walletError({ code: 4100, message: 'Unauthorized' }), /not allowed this page/);
  assert.match(walletError({ code: 4901, message: 'the transaction names chain 0x1' }), /not connected to the network/);
  assert.match(walletError({ code: -32000, message: 'insufficient funds for gas * price + value' }), /Sepolia ETH does not cover .* testers' page/);
  assert.equal(walletError(new Error('No browser wallet (EIP-1193) in this page')), 'No browser wallet (EIP-1193) in this page');
});

test('the vault\'s refusals by name, wherever the wallet put the revert data', () => {
  const cap = sel('TokenDepositCapExceeded()');
  for (const e of [
    { code: 3, message: 'execution reverted', data: cap },                                     // the JSON-RPC error itself
    { code: -32603, message: 'Internal JSON-RPC error.', data: { code: 3, data: cap } },        // MetaMask, nested once
    { code: -32603, message: 'x', data: { originalError: { code: 3, data: cap + '00' } } },     // MetaMask, nested twice
    { code: 3, message: `execution reverted: custom error ${cap}` },                            // anvil names it in the message
  ]) {
    assert.equal(revertDataOf(e).slice(0, 10), cap);
    assert.equal(walletError(e), 'The vault refuses it (TokenDepositCapExceeded): this asset\'s daily deposit cap is reached: try again later, or a smaller amount. It was not sent.');
  }
  const allowance = sel('ERC20InsufficientAllowance(address,uint256,uint256)') + '0'.repeat(192);
  assert.match(walletError({ code: 3, message: 'execution reverted', data: allowance }), /ERC20InsufficientAllowance.*approval first/);
  // An address inside a message is not a selector; an unknown refusal keeps the node's words.
  assert.equal(revertDataOf({ message: 'execution reverted: to 0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787' }), null);
  assert.equal(walletError({ code: 3, message: 'execution reverted', data: '0xdeadbeef' }), 'The vault refuses it (execution reverted). It was not sent.');
});

test('every refusal the page names is declared where it says (the vault; OpenZeppelin\'s ERC-20 errors when vault/lib is there)', () => {
  const vault = readFileSync(join(REPO, 'vault/src/FerrataVault.sol'), 'utf8');
  const oz = join(REPO, 'vault/lib/openzeppelin-contracts/contracts/interfaces/draft-IERC6093.sol');
  for (const sig of Object.keys(DEPOSIT_REFUSALS)) {
    const name = sig.slice(0, sig.indexOf('('));
    if (name.startsWith('ERC20')) {
      if (existsSync(oz)) assert.match(readFileSync(oz, 'utf8'), new RegExp(`error ${name}\\(address \\w+, uint256 \\w+, uint256 \\w+\\);`), sig);
      continue;
    }
    assert.match(vault, new RegExp(`^\\s*error ${name}\\(\\);`, 'm'), `${sig} is not declared in FerrataVault.sol`);
    assert.match(vault, new RegExp(`revert ${name}\\(\\)`), `${sig} is never raised by FerrataVault.sol`);
  }
});

test('the cost: priced from the answers, rounded up to the gwei, and the balance it needs', () => {
  const c = costOf({ gas: '0x1d4c0', gasPrice: '0x3b9aca07', balance: 10n ** 15n, value: 10n ** 15n });  // 120,000 gas at 1.000000007 gwei
  assert.equal(c.fee, 120_001_000_000_000n);              // 120,000,000,840,000 wei, up to the gwei
  assert.equal(c.need, 10n ** 15n + c.fee);
  assert.equal(c.enough, false);
  assert.match(c.text, /^about 0\.000120001 ETH \(120,000 gas at 1\.00 gwei, read now; your wallet shows the final figure\)$/);
  assert.equal(costOf({ gas: 21000n, gasPrice: 1_000_000n, balance: 21_000_000_000_000n }).enough, true);  // exactly enough
  assert.match(costOf({ gas: 21000n, gasPrice: 1_000_000n, balance: 0n }).text, /at 0\.0010 gwei/);
});

test('a name as the burn\'s recipient is refused with its reason; addresses pass as before', () => {
  assert.throws(() => burnRecipient('vitalik.eth', null), /looks like a name: this page resolves no names/);
  assert.throws(() => burnRecipient('alice.box', null), /resolves no names/);
  assert.equal(burnRecipient('0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed', null).how, 'typed; EIP-55 checksum checked');
  assert.throws(() => burnRecipient('nonsense', null), /0x followed by 40 hex digits/);
});
