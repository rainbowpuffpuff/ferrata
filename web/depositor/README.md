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
(EIP-1193). It has no back end (`serve.py` only hands the file to your own browser), no build step and no
dependency; all its JavaScript is inline.

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
2. **Serve the copy of the page you checked to your own browser, and open it there.** Check its hash, then start the
   server beside it (Python 3, standard library only):

   ```sh
   sha256sum web/depositor/index.html
   python3 -I web/depositor/serve.py      # prints the hash of what it serves; then open http://127.0.0.1:8645/
   ```

   The copy in this repository, as changed on 21 November 2026, hashes to
   `41219f0d71ad7e7f6b617d789ff85ef766381e88cfba46bcb081901d44b5c5e1`; a release states the hash of its own copy.
   [`serve.py`](serve.py) reads the page once, serves exactly those bytes and nothing else, listens on 127.0.0.1 only,
   answers only requests addressed to 127.0.0.1 or localhost, and sends the page's Content-Security-Policy as a header
   with `frame-ancestors 'none'` (no other site may frame the page). Stop it with Ctrl-C when you are done.

   Why not open the file directly (`file://…/index.html`): Chromium-based browsers run an extension's scripts in
   `file://` pages only after you turn on that extension's "Allow access to file URLs", which is off by default, so a
   wallet such as MetaMask injects nothing there and the page shows "No injected wallet". Turning the switch on works,
   but it also lets that extension read every local file the browser can open; serving the page on 127.0.0.1 needs no
   such permission. (Neither way has been tried with a real wallet extension yet: the test uses a stand-in wallet.)
3. **Connect, paste the ticket, press Check.** The page refuses, and sends nothing, if the wallet is on another chain,
   the connected account is not the ticket's depositor, the vault has no code, the token is not registered or the
   denomination index is out of range, `j` is not the vault's `depositCount` for the depositor, `C` is zero, or an
   ERC-20 balance does not cover the total. The check holds only for the chain and account it was made on: if the
   wallet announces a switch of either (EIP-1193 `chainChanged`, `accountsChanged`), the page drops the summary; and
   right before each transaction it asks the wallet again for its chain and account and sends nothing if either
   changed. If the wallet changes while a transaction is with it (awaiting your signature, or sent and not yet mined),
   the page says so, with the transaction's hash once the wallet returned it, sends nothing more, and reports the
   transaction's outcome when its receipt arrives. Every transaction it asks for carries the ticket's `chainId`, which
   wallets such as MetaMask refuse to sign on another chain.
4. **Read the summary, then sign in the wallet.** It shows the chain, vault, token, denomination, fees and total (priced
   exactly as the `ferrata` tool prices it), `j`, `C`, the exact calldata of each transaction the wallet will ask you
   to sign, and what becomes public on Ethereum (your address, the token, the amount, `C`, the time) and what stays
   private (your LEZ destination, inside `C`). An ERC-20 is approved for exactly the total, never more. If an allowance
   below the total is left over (from a deposit that failed after a fee rise), the page first resets it to zero:
   tokens such as USDT refuse to change one non-zero allowance into another.
5. After the receipt, the page decodes the vault's `Deposit` event, shows the leaf index, and checks that the logged
   commitment and depositor are the ticket's. Check the deposit again with your Logos tool (`ferrata status`,
   `ferrata scan`), then claim with it as usual.

## What the page sends, and to whom

Only to the wallet, through `window.ethereum`. Its wallet client allows `eth_requestAccounts`, `eth_accounts`,
`eth_chainId`, `eth_call`, `eth_getCode`, `eth_getBlockByNumber`, `eth_blockNumber`, `eth_sendTransaction` and
`eth_getTransactionReceipt`, and refuses anything else. Its Content-Security-Policy (`default-src 'none'`, and its
one inline script allowed by its SHA-256 hash rather than any inline script) forbids the page any network request of
its own (no fetch, no WebSocket, no external script, font or image) and any script but its own. `serve.py` sends the
same policy as a header, with `frame-ancestors 'none'`. A CSP limits what the page itself loads and runs; it stops
neither a navigation the user makes to another site nor anything a browser extension does.

## Security trade-offs

A page in a browser runs beside code the user does not control. What each threat the RFP names could do, and what
limits it:

| Threat | What it could do | What limits it |
|---|---|---|
| A phishing copy of the page | Show a ticket form that sends your deposit with **its own `C`** (the funds then mint to the attacker's LEZ destination), or to **its own "vault"** (a contract that keeps them) | Open only a local copy whose SHA-256 you checked; compare the vault address and `C` in the wallet's confirmation with what your Logos tool printed (the page asks you to); the vault address is the one of your deployment record |
| A malicious or compromised extension, or an injected script | Rewrite the page's DOM or the transaction before the wallet shows it: the same swaps of `C` or the vault, or an unlimited `approve` | The wallet's own confirmation shows the real calldata: check `C` and the vault there, not in the page; the page only ever asks for an `approve` of exactly the total, so an unlimited one is a sign of tampering; deposit from an account that holds only what you mean to bridge |
| A compromised wallet extension | Sign anything | Out of the page's reach; the Logos modules avoid this by holding keys in Basecamp, which is why they remain the primary path |
| Loss of privacy in the browser | A page or extension sees which account deposits and when | All of it is public on Ethereum anyway; the LEZ destination is never in the browser: the seed and the destination stay in the Logos tool, which only hands over `C` |
| The local server, and the way the page is opened | Another site framing the page to trick clicks; a site that resolves its own name to 127.0.0.1 (DNS rebinding) serving the page under its own origin; an extension given "Allow access to file URLs" reading local files | `serve.py`: `frame-ancestors 'none'` and `X-Frame-Options: DENY`; requests that name any host but 127.0.0.1 or localhost get 421; it serves only the bytes it read and hashed at start. The page holds no secret, so a local process that reads it learns nothing. Serving on 127.0.0.1 avoids the file-access permission altogether |
| A wallet switched after the check | A deposit sent to the same address on another chain (where the vault may not exist), or from another account | Checked again right before every send; dropped on the wallet's change events; `chainId` in every transaction; a change while a transaction is pending is reported with that transaction's hash and outcome, never as "nothing was sent" |

What the design gives up against the Logos module: the user's eye on `C` and the vault is the last check before the
funds leave. What it keeps: the seed and every LEZ key stay out of the browser, so a compromised page cannot claim,
burn or move anything already on LEZ, and cannot learn the user's LEZ destination.

## Test

```sh
bash web/depositor/test/run.sh
```

Starts a local anvil (chain id 11155111, port 18645, loopback only), deploys the vault with
[`vault/script/WebDepositorLocal.s.sol`](../../vault/script/WebDepositorLocal.s.sol) (the CLI test's vault, a
registered ERC-20 test token and a registered Tether-style token,
[`vault/script/TestTetherToken.sol`](../../vault/script/TestTetherToken.sol): `approve` returns nothing and refuses to
change a non-zero allowance into another), serves the page with `serve.py` on `http://127.0.0.1:18646/` as step 2 says,
and opens it in headless Chromium (Python Playwright) with a stand-in EIP-1193 wallet that forwards to anvil, can
switch its chain or account with or without announcing it, and refuses a transaction whose `chainId` is not its chain.
Sixteen groups of cases: what the server sends (the file byte for byte, the CSP header with the script's hash and
`frame-ancestors 'none'`, a request naming another host refused, a page of another origin unable to frame it); an ETH
deposit and an ERC-20 deposit (approve, then deposit) checked on chain, each transaction carrying the chain id;
refusals for a wrong chain, another account, a stale index, a zero commitment, an unregistered token; a wallet
rejection (EIP-1193 4001: shown, nothing retried); the wallet switching chain, then account, after the check, once
announced and once silently (each time: nothing sent); a Tether-style deposit with an allowance of 1 left over (reset to
0, approve, deposit, checked on chain); the wallet switching account while a sent deposit is still pending (anvil's
automine off: the page shows the deposit's hash and that nothing more is sent, never "nothing was sent", and once the
block is mined, that the deposit was mined, checked on chain); and the page's own traffic (nothing but itself) and wallet methods (only the
allowed ones). The test first checks its Python derivations of `s` and `C` against all three cases of
[`bridge/vectors/inbound-v2.json`](../../bridge/vectors/inbound-v2.json). The record is named after the run's UTC date:
run of 21 November 2026 (on the Modal build box), [`results/anvil-2026-11-21.txt`](results/anvil-2026-11-21.txt), 97
checks, every one passed, and a screenshot of the summary section,
[`results/anvil-2026-11-21.png`](results/anvil-2026-11-21.png).

What the test does not show: the page working with a real wallet extension. The stand-in answers `eth_chainId` and
`eth_accounts` itself and forwards everything else to anvil; a wallet's own confirmation screen, its network
switching and its injection rules are not exercised.

Needs Foundry, Python 3 with Playwright and its Chromium, and `curl`. Foundry runs `--offline` (a networked `forge
script` waited on a lookup here) and deploys one transaction at a time (`--slow`).
