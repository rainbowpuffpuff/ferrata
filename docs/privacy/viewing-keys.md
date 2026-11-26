# Viewing keys and voluntary disclosure

RFP-022, soft requirement 2: "Optional viewing keys allowing a user to *voluntarily* disclose
their own bridge activity to a chosen third party, without weakening privacy for anyone else and
without any protocol-level disclosure capability."

## What already existed

`kView` (derived from the bridge seed, `bridge/crates/ferrata-bridge-core/src/hash.rs`) opens a
user's burns: whoever holds it can scan the burn notes published on LEZ, keep the ones whose MAC
verifies, and read token, amount and recipient (`outbound::view_burns`). It grants nothing else:
claims need `s`, releases need `sOut`, and both derive from `kAuth`.

Deposits it could not show. `s` and `C` derive from `kAuth`, so handing over a key that shows
deposits also handed over the claim secret (`bridge/RESULTS.md`, limits, point 4). This crate,
`bridge/crates/ferrata-disclose/`, closes that gap on the wallet side only.

## What a user can disclose, and how

The wallet writes a JSON document from its own keys and public chain data (`make_disclosure`).
It holds:

* `chainId` and `vault` — which deposit side the document is about;
* `kView` — 32 bytes, hex; with it the third party runs the existing burn scan and lists the
  user's burns itself;
* one entry per deposit the wallet recognises as its own: the public leaf fields (`leafIndex`,
  `depositor`, `token`, `amount`, `commitment`), the destination it committed to (`kind`, `dest`),
  and — only when that deposit's nullifier is spent — the deposit secret `s` and the claim's
  `nullifier` `N`.

Keep the document to the third party you chose. It contains `kView`, so anyone who obtains it
reads your burns, forever after for every burn you publish under that key.

## What the third party learns and cannot do

`verify(disclosure, public_view)` checks the document against what the verifier reads from the
chains itself — the vault's published leaves, the LEZ bridge's spent nullifiers, the burn notes on
LEZ — and returns a report: each of the user's deposits with its destination and, if claimed, its
claim's `N`; each of the user's burns with token, amount and recipient. Any mismatch is refused
with an error naming the entry and the field.

What the third party cannot do with a verified document:

* **Claim.** Before the claim, `s` is absent, so it cannot even attempt one. After the claim, the
  nullifier is spent and `s` grants nothing.
* **Release.** Releases need `sOut`, which derives from `kAuth`. It is never in the document.
* **Refund.** A refund pays the depositor named in the leaf. It needs no `s`.
* **Spend, sign, or impersonate.** Neither `kAuth` nor the bridge seed is in the document; the
  document authorises nothing.

## Why `s` only after the claim

`s` is exactly what a claim proves knowledge of. Whoever holds it before the claim can submit the
claim itself — minted to the user's own destination, but with itself named as the relayer (taking
the relay fee) and at a timing of its choice. Once the claim is done, `N` is spent and `s` grants
nothing, so disclosing it is safe. Until then an entry carries only the public fields and the
declared `kind` and `dest`, which the third party can neither check nor act on; the report marks
them unverified.

## What the document proves, and what it cannot

* **It is a bearer document.** It proves that whoever wrote it knew `kView` and, for each claimed deposit, `s`. It
  does not prove who wrote it or who presents it: a recipient can pass it on, or present it as its own, and the next
  reader can tell no difference. A user who needs the disclosure bound to a person binds it outside this mechanism
  (for example by signing it with the depositor address's key, which is a separate, deliberate act).
* **Its burns are complete; its deposits are not.** With `kView` the third party scans every burn note itself and finds
  every burn made under that key, so it can see that none is missing. Deposits it cannot scan for: a deposit is found
  only from `kAuth`, which the document does not carry, so the user can leave a deposit out and the third party cannot
  tell. Read the report's deposit list as the user's own selection, never as all of the user's deposits.

## Effect on others

**No other user's secret is revealed.** The document holds only its author's keys and entries; verifying it reads
public chain data only; nothing here touches a guest, a LEZ program, the vault, or any existing source file of
`ferrata-bridge-core`, and no protocol component can produce or demand a disclosure. The tests check that none of the
other user's deposits, claims or burns appears in the report.

**But every disclosed link shrinks everyone else's anonymity set, as the recipient sees it.** The recipient learns
each disclosed deposit-to-claim pair (and each disclosed burn with its recipient). By elimination, that pair is no
longer a candidate for anyone else's claim: for another claim of the same token and denomination, the deposits it may
spend shrink, in the recipient's eyes, from all `n` deposits of that denomination to `n − k` once `k` of them are
disclosed and matched. In a small pool this is exact: with two deposits and two claims of one denomination, disclosing
one pair tells the recipient the other user's pair. The same holds for burns: a burn disclosed as the user's, with its
recipient, leaves the candidates for the other releases of that denomination. The tests do not measure this inference;
it follows from what the report contains.

If the recipient **publishes** what it learned, the same shrinkage applies against everyone.

So the clause's "without weakening privacy for anyone else" cannot be met absolutely by any disclosure that reveals a
link, this one included. What this design meets: the disclosure is voluntary, reveals no secret of anyone else's,
needs no protocol-level capability, and weakens others' privacy only by removing the disclosed links from their
candidate sets, against the recipient and against whoever the recipient tells. A user deciding to disclose should know
that it also tells the recipient something about the other users of the same denominations.

## The `ferrata-disclose` command and public-view files

The wallet and third party use the same library through two commands:

```sh
ferrata-disclose make --bridge-seed-file SEED --destinations DESTINATIONS.json --view VIEW.json --out DISCLOSURE.json
ferrata-disclose verify --disclosure DISCLOSURE.json --view VIEW.json
```

`make` writes the disclosure from the wallet's own 32-byte bridge seed, read from a file (64 hex digits; never an
argument, which process listings and shell history would show), and the wallet's own destination accounts. `verify` prints its report as JSON
and exits 0 on success. A library refusal is a JSON object with `error.kind` (the
`DisclosureError` variant), the optional `leafIndex` and `field`, and `message`; it exits 1.
Invalid command arguments, files, hex, or input JSON print usage on stderr and exit 2.

`VIEW.json`, public chain data only, is an object with `chainId`, `vault`, `configId`, `deposits`,
`spentNullifiers` and `burns` (any other field is refused). Hashes and 256-bit amounts are 64 lowercase hex
characters without `0x`; addresses are 40 and burn MACs are 32. Integer indices and counters are
JSON numbers. The fields are:

* Deposits are one object per vault `Deposit` event: `leafIndex`, the depositor's counter `j`,
  `depositor`, `token`, `amount` (32-byte big-endian), and `commitment`. The array is in leaf order.
* `spentNullifiers` is the array of claimed deposit nullifiers recorded on LEZ.
* Burns are `burnIndex` plus the note's public fields `r`, `memo`, `mac`, `token`, `amount`, and
  `cOut`. Their amount is also a 32-byte big-endian word.
`DESTINATIONS.json`, the wallet's own and read by `make` only, is `{"destinations": [...]}`: each
candidate has `kind` (1 private, 2 public), `account` (its candidate counter), and `dest`, the LEZ
accounts the wallet's seed derives. The scan tries every declared kind/account pair up to one past
the largest account number; omitted pairs cannot recover a deposit. An empty array discloses no
deposits but can still disclose burns. These candidates are wallet-supplied account identifiers,
not additional proof, and `verify` never reads them. See the binary's top doc comment for a
complete JSON example.

`DISCLOSURE.json` is the library document described above, not a second format. The verifier must
obtain its public-chain fields independently; accepting a view supplied by the discloser gives no
independent chain verification.

A complete non-synthetic public-test replay is committed at
[`disclosure-replay/crown-rc3-rehearsal-1/`](disclosure-replay/crown-rc3-rehearsal-1/README.md).
Its README maps every view field to the recorded run file, including the event-log decoding and
burn-tree indices. The CLI integration tests re-verify that exact pair and reject a one-byte
change to a disclosed claim.

## Running the tests

From `bridge/`:

```sh
nice -n 19 cargo test -p ferrata-disclose -j 4 -- --test-threads 4
nice -n 19 cargo clippy -p ferrata-disclose -j 4 --all-targets -- -D warnings
cargo fmt -p ferrata-disclose -- --check
```

The tests (`bridge/crates/ferrata-disclose/tests/disclose.rs`) build a world of two users with
seed-derived keys and assert: an honest disclosure verifies and lists exactly its author's
deposits, claims and burns; nothing of the other user's appears beyond the public view; every
tampering is refused with a named error; the JSON carries no `kAuth`, no bridge seed, no `sOut`
and no `s` of an unclaimed deposit; and a JSON round trip gives the same report. They use only
public data and the user's own disclosure — no protocol component is involved in disclosing
anything.
