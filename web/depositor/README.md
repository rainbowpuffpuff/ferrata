# Web depositor: a deposit from a common Ethereum wallet

RFP-022 soft requirement 5, verbatim: "**Common Ethereum wallet compatibility.** The hard requirements assume the use
of the Logos Ethereum core module, meaning users hold their Ethereum assets in software loaded in Basecamp. This can
add friction: a user whose funds are in another Ethereum wallet first has to transfer them to the Logos Ethereum core
module before wrapping. Proposals may offer a supplementary approach compatible with common Ethereum wallet software,
such as a web application or another wallet integration, in addition to the Logos modules required above.
Browser-based software is exposed to security risks that locally loaded Logos modules are intended to avoid, such as
malicious or compromised browser extensions, injected scripts, and phishing front ends. Any such approach must be
carefully considered, and the proposal must document its security trade-offs."

**Prototype, unaudited, not for production.** Tested against a local anvil chain in headless Chromium; not used on any
public network.

## What it is, and is not

[`index.html`](index.html) is one static page. It makes a bridge **deposit** (ETH, or a registered ERC-20 with its
`approve` first) from the account in the user's own wallet: MetaMask or any wallet that injects `window.ethereum`
(EIP-1193). It has no server, no build step and no dependency; all its JavaScript is inline.

It does **not** claim, burn, release or recover. Those stay in the Logos tool (`ferrata`) or the bridge module in
Basecamp. The browser never holds a LEZ key and never sees the bridge seed.

## Use

1. **Make a ticket with your Logos tool, in a dry run (it sends nothing).** Name the wallet address you will deposit
   from:

   ```sh
   ferrata deposit --denomination 1 --from 0xYourWalletAddress
   ```

   It prints, among other lines, `commitment C  0x…` and `depositor  0x…   j = N`. The commitment `C` binds your LEZ
   destination; `j` is your next deposit index at the vault ([`cli/README.md`](../../cli/README.md)). Put them in a
   ticket (or type the fields into the page one by one):

   ```json
   {"version":1,"chainId":11155111,"vault":"0x…","token":"0x…","denomIndex":1,"commitment":"0x…","depositor":"0x…","index":0}
   ```

   `token` is the vault's `ETH` constant for ether.
2. **Open a copy of the page you checked**, from disk (`file://…/web/depositor/index.html`). Check its hash first:

   ```sh
   sha256sum web/depositor/index.html
   ```

   At the commit that added this README it is `d0b3ed7f607b072739d3d9ca0914fbeb5c050558a0a3660bd02afbf1b469d58a`; a
   release states the hash of its own copy.
3. **Connect, paste the ticket, press Check.** The page refuses, and sends nothing, if the wallet is on another chain,
   the connected account is not the ticket's depositor, the vault has no code, the token is not registered or the
   denomination index is out of range, `j` is not the vault's `depositCount` for the depositor, `C` is zero, or an
   ERC-20 balance does not cover the total.
4. **Read the summary, then sign in the wallet.** It shows the chain, vault, token, denomination, fees and total (priced
   exactly as the `ferrata` tool prices it), `j`, `C`, the exact calldata of each transaction the wallet will ask you
   to sign, and what becomes public on Ethereum (your address, the token, the amount, `C`, the time) and what stays
   private (your LEZ destination, inside `C`). An ERC-20 is approved for exactly the total, never more.
5. After the receipt, the page decodes the vault's `Deposit` event, shows the leaf index, and checks that the logged
   commitment and depositor are the ticket's. Check the deposit again with your Logos tool (`ferrata status`,
   `ferrata scan`), then claim with it as usual.

## What the page sends, and to whom

Only to the wallet, through `window.ethereum`. Its wallet client allows `eth_requestAccounts`, `eth_accounts`,
`eth_chainId`, `eth_call`, `eth_getCode`, `eth_getBlockByNumber`, `eth_blockNumber`, `eth_sendTransaction` and
`eth_getTransactionReceipt`, and refuses anything else. Its Content-Security-Policy (`default-src 'none'`) forbids the
page any network request of its own: no fetch, no WebSocket, no external script, font or image.

## Security trade-offs

A page in a browser runs beside code the user does not control. What each threat the RFP names could do, and what
limits it:

| Threat | What it could do | What limits it |
|---|---|---|
| A phishing copy of the page | Show a ticket form that sends your deposit with **its own `C`** (the funds then mint to the attacker's LEZ destination), or to **its own "vault"** (a contract that keeps them) | Open only a local copy whose SHA-256 you checked; compare the vault address and `C` in the wallet's confirmation with what your Logos tool printed (the page asks you to); the vault address is the one of your deployment record |
| A malicious or compromised extension, or an injected script | Rewrite the page's DOM or the transaction before the wallet shows it: the same swaps of `C` or the vault, or an unlimited `approve` | The wallet's own confirmation shows the real calldata: check `C` and the vault there, not in the page; the page only ever asks for an `approve` of exactly the total, so an unlimited one is a sign of tampering; deposit from an account that holds only what you mean to bridge |
| A compromised wallet extension | Sign anything | Out of the page's reach; the Logos modules avoid this by holding keys in Basecamp, which is why they remain the primary path |
| Loss of privacy in the browser | A page or extension sees which account deposits and when | All of it is public on Ethereum anyway; the LEZ destination is never in the browser: the seed and the destination stay in the Logos tool, which only hands over `C` |

What the design gives up against the Logos module: the user's eye on `C` and the vault is the last check before the
funds leave. What it keeps: the seed and every LEZ key stay out of the browser, so a compromised page cannot claim,
burn or move anything already on LEZ, and cannot learn the user's LEZ destination.

## Test

```sh
bash web/depositor/test/run.sh
```

Starts a local anvil (chain id 11155111, port 18645, loopback only), deploys the vault with
[`vault/script/WebDepositorLocal.s.sol`](../../vault/script/WebDepositorLocal.s.sol) (the CLI test's vault and a
registered ERC-20 test token), opens the page from `file://` in headless Chromium (Python Playwright) with a stand-in
EIP-1193 wallet that forwards to anvil, and runs nine groups of cases: an ETH deposit and an ERC-20 deposit (approve,
then deposit) checked on chain; refusals for a wrong chain, another account, a stale index, a zero commitment, an
unregistered token; a wallet rejection (EIP-1193 4001: shown, nothing retried); and the page's own traffic (nothing but
its `file://` URL) and wallet methods (only the allowed ones). The test first checks its Python derivations of `s` and
`C` against all three cases of [`bridge/vectors/inbound-v2.json`](../../bridge/vectors/inbound-v2.json). Record of 21
November 2026: [`results/anvil-2026-11-21.txt`](results/anvil-2026-11-21.txt), 53 checks, every one passed, and a
screenshot of the summary, [`results/anvil-2026-11-21.png`](results/anvil-2026-11-21.png).

Needs Foundry, Python 3 with Playwright and its Chromium. Foundry runs `--offline` (a networked `forge script` waited
on a lookup here) and deploys one transaction at a time (`--slow`).
