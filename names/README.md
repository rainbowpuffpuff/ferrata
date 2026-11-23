# Names on the LEZ v0.4 test network (`ferrata_names` v2)

**Experimental, unaudited, for the test network.** `alice` → an account: a LEZ program that registers human-readable
names, resolves them, reverse-resolves accounts, and lets the owner update, renew, transfer or release a name. A name
can also carry a **private receiver** (LEZ's nullifier public key and viewing public key), so that anyone can pay
`alice` privately without learning any of the holder's accounts.

**v2 and v1.** v2 (this source; image ids in [`image-ids.md`](image-ids.md)) answers the independent review of
23 November 2026: a refused reveal no longer hands the name to a copier (M1), deadlines hold for the block (L13),
look-alikes and variants of reserved names are refused (L12), and a name leaves an account without that account's
actor in the transaction (L14). v1 (`0xee9578ca…f790`) is live on our public test network, immutable; v2 is a new
program at a new address, to be deployed by the operator ([Deploying it](#deploying-it-for-the-test-network-operator)).
v1's names stay on v1.

| Piece | Where |
|---|---|
| The program (all its logic, the rules, the exit codes) | [`lez/names-wire/src/lib.rs`](../lez/names-wire/src/lib.rs) (`handle`); guest entry [`lez/guests/names`](../lez/guests/names/src/main.rs), built by [`lez/methods-names`](../lez/methods-names) |
| Harness tests: every action and refusal; the review's cases | [`lez/harness/tests/names.rs`](../lez/harness/tests/names.rs), [`names_review.rs`](../lez/harness/tests/names_review.rs) (feature `names`); their first run on v1: [`e2e/results/review-before/`](e2e/results/review-before/) |
| Measured costs | [`lez/results/names-*.md`](../lez/results/) (rc1; rc3's identical: [`e2e/results/harness-rc3/`](e2e/results/harness-rc3/)) |
| IDL (SPEL format) and its check | [`idl/ferrata_names.idl.json`](../idl/ferrata_names.idl.json), `python3 -I idl/tools/check_names_idl.py` |
| JS resolver, reveal check and call builder (no dependencies; the wallet and explorer import it) | [`names/js/names.js`](js/names.js); tests `node --test names/js/test/names.test.mjs names/js/test/review.test.mjs` |
| CLI | [`names/js/cli.mjs`](js/cli.mjs) (`ferrata-names`; signs with the web wallet's `deploy/testnet/web/wallet/lez.js`; Node 22.7+ or 20.19+) |
| rc3 rehearsal: build, rc3 harness, image ids, deploy on a local rc3 sequencer, the CLI, the review's race replayed | [`names/e2e/box.sh`](e2e/box.sh), results in [`names/e2e/results/`](e2e/results/) |
| CI | job `names` of `.github/workflows/ci.yml` ([`scripts/ci/names.sh`](../scripts/ci/names.sh)) |
| Every count this page cites, recomputed | `python3 -I names/e2e/claims.py [--run]` |
| Section for the top-level AGENTS.md | [`names/AGENTS.md`](AGENTS.md) |

## How it works

* `user(a)` = `Actor(a, P)`: every action is a transaction **signed by account `a`**, its root message at `a`'s own
  actor of the program, which LEZ marks authorized only when `a` signed. `user(a)` forwards the action to the name's
  actor, which sees the caller as `from = user(a)`: nobody can act for an account they cannot sign for. `user(a)`'s
  state lists the names `a` holds, its primary name (the reverse record) and its pending commitment.
* `name(n)`: the public PDA of the program with seed `sha256("ferrata-names/v2/name/" || skeleton(n))`. The
  **skeleton** reads `0` as `o`, `1` as `l`, `rn` as `m` and `vv` as `w`: look-alikes share one address, so only one of
  them can be registered at a time. Its state is the record (the name, owner, target account, private receiver,
  registration and expiry times, a pending offer, the number of the commitment that registered it), or empty.
* `sequence`: the public PDA with seed `sha256("ferrata-names/v2/sequence")`, a counter that numbers every commitment
  in the order the chain applies them.
* Time: an action carries the caller's `now_ms`; the program sets the transaction's timestamp validity window to
  `[now_ms, now_ms + 10 min)` (`[now_ms, now_ms + 2 min)` for a commit and a reveal), so LEZ applies it only in a
  block whose time is in that window. A deadline must hold for the whole window (`now_ms + window <= deadline`): no
  action is ever accepted after its deadline, so send a deadline-bound action at least 10 minutes before it.
* State growth: LEZ removes no account. Every name address ever registered keeps its account in the state, empty
  after a release (the lifecycle test checks it), as does the counter's; an account's own state under the program
  lives on its own account. Each was created by a transaction whose fee paid for it; nothing else grows.
* Every change emits a `ProgramEvent` (`Registered`, `Displaced`, `Renewed`, `Offered`, `Transferred`, `Updated`,
  `Released`; selector `sha256("ferrata_names::<kind>")[..8]`, data `NameEvent`) for explorers.

### Registration: commit, reveal, and the oldest commitment wins

1. **Commit**: the account signs `Commit { commitment, now_ms }` with
   `commitment = sha256("ferrata-names/v2/commit/" || account || salt || name)` and a fresh 32-byte salt. It is stored
   in the account's own state, numbered by `sequence` (one pending commitment per account; a new one replaces it). It
   reveals nothing about the name and reserves nothing.
2. **Reveal**: the same account signs `Register { name, ..., salt }`, its `now_ms` from 4 minutes after the commit's
   (`COMMIT_SLACK_MS + MIN_REVEAL_DELAY_MS`) with its whole window inside the hour after it (`MAX_REVEAL_WINDOW_MS`).
   The program recomputes the hash with the signer's own account: a reveal copied by another account matches nothing
   (`NO_COMMIT` 230, or `COMMIT_MISMATCH` 231 if it has a commitment of its own), as does a wrong salt or name (231);
   too early is `TOO_EARLY` (232); past the hour the commitment is void (`COMMIT_EXPIRED` 233). A reveal spends it.
3. **The oldest commitment wins.** A reveal whose commitment has a smaller number than the one that registered the
   name takes the name from that registrant, as long as that registration is less than an hour old (`CONTEST_MS`;
   event `Displaced`). A registration is final an hour after it.

**Why (review M1).** A reveal that is refused or reverts is still in a block: it publishes the name. In v1 a copier
could then commit with `now_ms` 2 minutes back, reveal about 2 minutes later, and keep the name; the review showed it
in the harness and on rc3's sequencer, and our wallet once sent a reveal 30 s early on the live v1. In v2 the copier's
commitment is necessarily younger (it is made after the name leaked, and `sequence` orders commitments exactly, with
no clock involved), so the account whose commitment came first takes the name back with a valid reveal anywhere in its
own hour: `names_review.rs` replays both of the review's cases (too early; refused for a full list) and the name ends
with its first committer; the rc3 run below replays the race on rc3's own sequencer. The 4-minute delay and the
2-minute window still keep a copier from revealing while a first reveal can land at all: someone who reads a name from
a reveal whose `now_ms` is `n` can commit only in a block at or after `n`, so their commitment's `now_ms` is above
`n - 2 min` and their reveal opens after `n + 2 min`, when that reveal's window has closed (`MIN_REVEAL_DELAY_MS >=
COMMIT_SLACK_MS`, a compile-time check). One hour (`CONTEST_MS = MAX_REVEAL_WINDOW_MS`, also checked at compile time)
covers every older commitment that can still be revealed. What remains: a sequencer that withholds an account's valid
reveals for the whole hour of its commitment can give the name to another (censorship); an attacker who commits to a
name before anyone else wants it wins it, as first-come registration intends. The clients never send a reveal that can
fail: the CLI's `reveal` checks the commitment, its window, the name, the list of names and the balance first
(`preflightReveal` in `names.js`), never sends one early, and sends it again at once if it reverts inside its window.

## Rules (enforced by the program; each refusal reached in a test)

| Rule | Refusal (exit) |
|---|---|
| 3–32 bytes of `a-z 0-9 -`; no leading or trailing `-`; no `--`. No upper case, no Unicode | `INVALID_NAME` (245) |
| One name per skeleton (`0`=`o`, `1`=`l`, `rn`=`m`, `vv`=`w`): `a1ice` cannot be registered while `alice` holds the address, and does not resolve | `CONFUSABLE` (234) |
| Reserved: `admin administrator bridge explorer faucet ferrata help lez logos null official root security sequencer support system treasury validator wallet`, and any name with a `-`-separated part whose skeleton, as it is or without its leading and trailing digits, is one of them (`faucet1`, `wa11et`, `ferrata-team`, `0fficial`). Unhyphenated compounds (`ferratateam`) are not caught | `RESERVED_NAME` (246) |
| Commit first; reveal with the same account, name and salt, in the commitment's window | `NO_COMMIT` (230), `COMMIT_MISMATCH` (231), `TOO_EARLY` (232), `COMMIT_EXPIRED` (233) |
| First committed, first served; no fee on the test network | `TAKEN` (247) while held (or in grace) by an older commitment's registration, or by any registration more than an hour old |
| At most 5 names per account (by registration or transfer); a name that left the account without it signing counts until it is forgotten | `TOO_MANY` (251) |
| `Forget` only names in the signer's list and no longer the signer's | `NOT_HELD` (236), `STILL_HELD` (235) |
| Term 180 days from the reveal's `now_ms`; the owner renews (to 180 days from then) until 30 days after expiry, the renewal's whole window before that end | `LAPSED` (250) after |
| Expired (or may be by the time the request lands): does not resolve; cannot be offered, accepted, re-pointed or set primary; only renewed or released | `EXPIRED` (249) |
| Only the owner offers (transfers), updates, renews, releases | `NOT_OWNER` (248) |
| Only the account a name is offered to accepts it; a primary name must point at the account that sets it | `NOT_TARGET` (254) |
| An offer to oneself with no offer pending | `SAME_OWNER` (253) |
| Unsigned root call; the program's own messages sent by an account; malformed message, keys or `Forget` list | `NOT_SIGNED` (242), `WRONG_SENDER` (243), `BAD_MESSAGE` (240) |
| Defensive: a stored state that does not decode (reached in a test with a corrupt record put in genesis); a program message at the wrong actor (the program never sends one: not reachable). No message makes the program read one kind of its states as another: a name's actions go to the address derived from the name, an account's own state is written only under that account, the counter's only by `Tick`. (In the clients, a name's or the counter's account read as an account has no name: `reverse()` returns null, `user()` throws `NotAnAccount`.) | `BAD_STATE` (241), `WRONG_RECEIVER` (244) |

A transfer takes two steps: the owner offers the name (`transfer NAME TO`), the account offered to takes it (`accept
NAME`); nobody can push a name into another account and a mistyped account never gets it. An offer to oneself
withdraws it; an offer does not survive the name's expiry. On acceptance the name points at the new owner and the
private receiver is dropped. **The list of names (review L14).** When a name leaves an account without that account
signing (accepted by another, registered afresh after it lapsed, or displaced by an older commitment), that account's
actor is not touched: a private account's actor could not be admitted in a public transaction, so in v1 such a name
could never leave a private owner. The name stays in the old owner's list until it sends `Forget` (`forget` in the
CLI, automatic before a reveal when the list is full).

## Costs (measured)

Harness: LEZ's state machine and fee settlement, `RISC0_DEV_MODE=1`; the tables are identical on LEZ v0.4.0-rc1 and
v0.4.0-rc3. Execution gas equals the guest's cycles; base fee in native units at the harness's opening fee state.
Sequencer: the fee each action cost its signer on LEZ v0.4.0-rc3's own standalone sequencer (balance before minus
after, `e2e/results/local-rc3-v2/fees.md`).

| Action | Execution gas | Storage gas (tx bytes) | Base fee (harness) | Fee charged (rc3 sequencer) |
|---|---:|---:|---:|---:|
| commit (step 1 of registering; three handlers: the account, the counter, the account) | 33,111 | 744 | 270,840 | 269,432 |
| reveal (step 2; a new name) | 70,731 | 787 | 572,144 | 570,736 |
| reveal of an older commitment, taking the name from a younger one | 73,152 | 721 | 590,984 | 593,176 |
| reveal of a lapsed name | 72,955 | 720 | 589,400 | |
| reveal of a 32-byte name | 80,187 | 814 | 648,008 | |
| set primary name | 41,364 | 657 | 336,168 | 334,760 |
| clear primary name | 8,467 | 584 | 72,408 | |
| update target | 39,920 | 690 | 324,880 | |
| update: add a private receiver (1,184-byte vpk) | 54,467 | 1,878 | 450,760 | 462,960 |
| renew | 30,169 | 656 | 246,600 | 360,328 |
| transfer (offer) | 31,711 | 688 | 259,192 | 372,928 |
| accept (the transfer's second step) | 43,619 | 656 | 354,200 | 434,112 |
| forget a name that left the account | 41,896 | 652 | 340,384 | 332,960 |
| release | 42,013 | 648 | 341,288 | 355,088 |
| a refusal (charged as metered; 74 in the tables) | 5,828 – 73,854 | 575 – 822 | 51,224 – 597,136 | |

The sequencer's fee is higher than the harness's for renew, offer and accept: in the end-to-end run the record holds a
private receiver (1,184-byte key) from the step before, which each of those decodes and re-encodes.

**Gas limits: from each action's worst case.** The table above is ordinary use. What a client must budget is the worst
case of each action: [`lez/results/names-worst.md`](../lez/results/names-worst.md) measures every action with 32-byte
names of six shapes (the costs depend on the characters: `vvvv…` is the costliest for most actions), records carrying
a private receiver and a pending offer, full lists of five such names, a 32-byte primary name and a pending
commitment; a reveal that displaces such a record; a `Forget` of 1 to 5 such names. The costliest action is a `Forget`
of five names, **300,583** cycles (the CLI sends one itself before a reveal when the list is full); a reveal that
displaces, 148,966; every other action at most 74,078. A transaction outside its validity window (a wrong clock), or
out of gas, is charged its **whole gas limit**, and the limit also sets the fee reserve, so each action is sent with its
own limit, its worst case plus about 25 % (`GAS` and `gasFor` in [`names.js`](js/names.js); the CLI and the wallet use
them; `--gas` overrides): Commit 60,000; Register (the reveal) 190,000; Renew and Transfer 70,000; SetTarget 100,000;
Accept, SetPrimary and Release 90,000; Forget 20,000 + 72,000 per name (380,000 for five). The harness test
`worst_cases_fit_the_clients_gas` reads `GAS` from names.js and fails if any measured worst case does not fit. The CLI
sends `now_ms` 30 s before the local clock.

## Reading: what the RPC is trusted for (review L5)

LEZ's RPC serves public state without proofs: `resolve()` returns the record as the RPC returns it, and a dishonest
RPC can point a name anywhere. Show the resolved account before sending to it, and give `Names` several independent
endpoints (`rpc: [url1, url2]`): it then refuses any answer they disagree on (`review.test.mjs`).

## Deploying it (for the test network operator)

No configuration, no admin key, no init transaction (the counter is created by the first commit, with its PDA
evidence). With LEZ v0.4.0-rc3's wallet CLI, the rc3 build (`names/e2e/box.sh build` writes it to
`target/names-box/bin/ferrata_names.bin`; check its sha256 and image id against [`image-ids.md`](image-ids.md)):

```sh
wallet account new public            # the header account: its id is the program's id (P)
wallet account new public            # one segment account per 96 KiB of the program's user ELF: 3 for this build
wallet program-loader deploy --elf ferrata_names.bin --header Public/<P> --segments Public/<S1> Public/<S2> Public/<S3> \
  --payer Public/<operator> --immutable
```

The wallet refuses before uploading if the segment count is wrong and says the count it expects. Publish `P`
(base58) with the network's programs; the wallet and explorer read it from their `config.json`
(`"names": {"program": "<P>"}`). The public RPC must serve `getAccountView` (or `getAccount`) for resolution, and
`getAccountsNonces`, `getFeeState`, `getAccountBalance`, `sendTransaction`, `getTransaction`, `getBlock` for the CLI.

## Using it

```sh
export FERRATA_RPC=https://<host>/rpc FERRATA_NAMES_PROGRAM=<P>    # signing: deploy/testnet/web/wallet/lez.js
node names/js/cli.mjs keygen my.key                 # a test key (0600); fund its account from the faucet
node names/js/cli.mjs register alice --key my.key   # commit, wait ~4 min, reveal; points at the signer's account
#   or the two steps apart: commit alice --key my.key  (salt kept in alice.commit.json, 0600), later: reveal alice --key my.key
node names/js/cli.mjs set-primary alice --key my.key
node names/js/cli.mjs resolve alice                 # exit 0 with the record, 1 if free/expired/only a look-alike
node names/js/cli.mjs whois alice                   # the record, its status, provisional until when, look-alikes
node names/js/cli.mjs reverse <account>
node names/js/cli.mjs set-target alice --receiver-keys keys.txt --key my.key   # npk and vpk hex, one per line
node names/js/cli.mjs transfer alice <account> --key my.key   # an offer; the other account runs: accept alice
node names/js/cli.mjs forget --key my.key           # drop names that left this account
```

In a page: `import { Names } from './names.js'`; `new Names({ rpc: '/rpc', program })`; `resolve(name)`,
`reverse(account)`, `record(name)`, `user(account)`, `lostNames(account)`, `preflightReveal(signer, { name, salt })`
and `call(signer, {kind, ...})`, which returns the `{ to, actors, data, evidence }` a signer signs. To register:
`const c = await names.commitCall(me, 'alice')` (sign and send `c`; keep `c.salt`), then once
`Date.now() - 30_000 >= c.revealFrom` and `preflightReveal` is ok: `names.call(me, { kind: 'Register', name: 'alice',
salt: c.salt })`.

## Reproduce

```sh
cd lez && RISC0_DEV_MODE=1 cargo test -p harness --features names --test names --test names_review -- --test-threads 1
cargo test -p ferrata-lez-names-wire                # from lez/
python3 -I idl/tools/check_names_idl.py             # from the repository root
node --test names/js/test/names.test.mjs names/js/test/review.test.mjs
names/e2e/box.sh build && names/e2e/box.sh harness && names/e2e/box.sh ids && names/e2e/box.sh e2e NAME   # rc3
python3 -I names/e2e/claims.py --run                # the counts this page cites
```

## Not done / limits

* The owner is a public account. A name that points only at a private receiver hides the holder's accounts from
  senders, but the record shows the owning account: use an account that does nothing else.
* The program is not composable with other programs (a root call must come from the signer itself).
* Commit and reveal stops copiers, not a sequencer that censors an account's reveals for a whole hour.
* The skeleton covers four mappings only: `cl`/`d`, `nn`/`m`, `vvv`/`vw`, other digits and unhyphenated compounds of
  reserved names (`ferratateam`) stay distinct and register.
* A registration is provisional for its first hour (an older commitment, even a guess made before anyone wanted the
  name, takes it): a wallet should show it as provisional until `finalAt(record)`. A commitment must be revealed
  within its hour.
* Unaudited. Image ids: [`image-ids.md`](image-ids.md).
