## Names (`ferrata_names`) — for agents

Human-readable names for LEZ accounts on our v0.4 test network: `alice` → an account (and optionally a private
receiver). Program `lez/guests/names` (all logic in `lez/names-wire/src/lib.rs`), IDL `idl/ferrata_names.idl.json`,
JS `names/js/names.js`, CLI `names/js/cli.mjs`. The program id is published with the network's other programs.

**Resolve before you send.** Never send to a name without resolving it on chain at that moment:

```js
import { Names } from './names.js';
const names = new Names({ rpc: 'https://{{HOST}}/rpc', program: '<names program id, base58>' });
const r = await names.resolve('alice');        // null if free, released or expired
// r.target: the public account (base58) or null; r.receiver: { npk, vpk } for a private send, or null
const who = await names.reverse('<account>');  // the account's name, already checked against the forward record
```

From a shell: `node names/js/cli.mjs --rpc URL --program ID resolve alice` (exit 0 found, 1 not), `reverse ACCOUNT`,
`whois NAME` (record + status `live|grace|lapsed|free`), `check NAME` (the rules, offline), `rules`.

**Act on names** (signed by the account that acts, which also pays the fee; `--key FILE` holds a test key in hex):
`register NAME [--target ID|--no-target] [--receiver-keys FILE]` (commit, wait about 4 minutes, reveal: names are
registered by commit and reveal so that nobody can copy a pending registration; `commit` and `reveal` run the steps
apart, the salt kept in `NAME.commit.json`), `set-target`, `set-primary NAME|--clear`, `renew`,
`transfer NAME TO` (an offer: the account `TO` takes it with `accept NAME`; `transfer NAME <yourself>` withdraws it),
`release`. Each prints the transaction and whether it applied or reverted (exit 0 / 1).

**Rules** (the program refuses anything else, with the exit code in brackets): 3–32 characters of `a-z 0-9 -`, no
leading/trailing `-`, no `--` [245]; reserved names such as `faucet`, `wallet`, `admin` [246]; first come [247]; a reveal needs the signer's own commitment, same name and salt, 4 min to 24 h after it
[230–233]; at
most 5 names per account [251]; 180-day term, renewable by the owner until 30 days after expiry [250], after which
anyone may register it; only the owner offers, updates, renews or releases [248], only the account offered to accepts [254]; an expired name does not resolve
and cannot be transferred or updated [249]. No fee on the test network. An accepted transfer points the name at the new
owner and drops the private receiver.

**Pitfalls.** Lower-case what a user typed (`normalize`): `Alice` is refused, not folded, by the program. Do not
trust a stored primary name on its own: `reverse()` re-checks it. Names are a test-network convenience, unaudited:
show the resolved account to the user before any transfer.
