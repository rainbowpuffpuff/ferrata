## Names (`ferrata_names` v2) — for agents

Human-readable names for LEZ accounts on our v0.4 test network: `alice` → an account (and optionally a private
receiver). Program `lez/guests/names` (all logic in `lez/names-wire/src/lib.rs`), IDL `idl/ferrata_names.idl.json`,
JS `names/js/names.js`, CLI `names/js/cli.mjs` (Node 22.7+ or 20.19+). The program id is published with the network's
other programs; v2 is a new program at a new address (v1 stays live and immutable at its own; names do not carry over).

**The web wallet is still on v1.** `/wallet/` registers and resolves names with the first version (`ferrata_names`
in `/programs.json`) until its bundled client is updated: a name a wallet user registered is on v1, not here, and the
same name can belong to another account on v2. Ask which version the user means, and show the account before sending.

**Resolve before you send.** Never send to a name without resolving it on chain at that moment, and show the
resolved account to the user first. The RPC is trusted for what it returns (LEZ serves public state without proofs):
pass several independent RPCs as `rpc: [url1, url2]` to refuse an answer they disagree on.

```js
import { Names } from './names.js';
const names = new Names({ rpc: 'https://<host>/rpc', program: '<names program id, base58>' });
const r = await names.resolve('alice');        // null if free, released, expired, or only a look-alike is registered
// r.target: the public account (base58) or null; r.receiver: { npk, vpk } for a private send, or null
// names.js finalAt(r): before it the registration is provisional (an older commitment can still take it)
const who = await names.reverse('<account>');  // the account's name, already checked against the forward record
```

From a shell: `node names/js/cli.mjs --rpc URL --program ID resolve alice` (exit 0 found, 1 not), `reverse ACCOUNT`,
`whois NAME` (record, status `live|grace|lapsed|free`, provisional until when, a look-alike if one holds the address),
`check NAME` (the rules, offline), `rules`.

**Act on names** (signed by the account that acts, which also pays the fee; `--key FILE` holds a test key in hex):
`register NAME [--target ID|--no-target] [--receiver-keys FILE]` (commit, wait about 4 minutes, reveal),
`set-target`, `set-primary NAME|--clear`, `renew`, `transfer NAME TO` (an offer: the account `TO` takes it with
`accept NAME`; `transfer NAME <yourself>` withdraws it), `release`, `forget` (drop names that left you). Each prints
the transaction and whether it applied or reverted (exit 0 / 1). Send each action with `gasFor(msg)` from names.js
(its worst case measured, with a margin): out of gas, a transaction is charged its whole limit.

**Never send a reveal that can fail.** A reveal that is refused or reverts is still in a block: it publishes the name.
The CLI checks everything first (`preflightReveal` in names.js: the commitment, its window, the name, the list of
names, the balance) and sends nothing early; a wallet must do the same. The program protects the account whose
commitment is oldest: if a copier reveals first, a later reveal of the older commitment, within its hour, takes the
name back (event `Displaced`). Registrations are provisional for an hour.

**Rules** (the program refuses anything else, with the exit code in brackets): 3–32 characters of `a-z 0-9 -`, no
leading/trailing `-`, no `--` [245]; reserved names such as `faucet`, `wallet`, `admin` and their variants
(`faucet1`, `wa11et`, `ferrata-team`) [246]; one name per look-alike: `0`=`o`, `1`=`l`, `rn`=`m`, `vv`=`w` [234];
commit first [230–233]; oldest commitment first [247]; at most 5 names per account, names that left you counting
until you `forget` them [251]; 180-day term, renewable by the owner until 30 days after expiry [250], after which
anyone may register it; only the owner offers, updates, renews or releases [248], only the account offered to
accepts [254]; an expired name does not resolve and cannot be transferred or updated [249]. A deadline holds for the
block: send a deadline-bound action at least 10 minutes before it. No fee on the test network.

**Pitfalls.** Lower-case what a user typed (`normalize`): `Alice` is refused, not folded, by the program. Do not
trust a stored primary name on its own: `reverse()` re-checks it. `resolve('a1ice')` is null while `alice` holds the
address: compare the name you show with the record's. Names are a test-network convenience, unaudited.
