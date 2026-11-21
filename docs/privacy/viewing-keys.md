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

## Effect on others

None from the mechanism. Nothing here touches a guest, a LEZ program, the vault, or any existing
source file of `ferrata-bridge-core`: disclosure is a file the user chooses to hand over, and no
protocol component can produce or demand one. A user who discloses nothing changes nothing for
anyone.

Said plainly anyway: if the third party **publishes** what it learned, that is a real loss for
others. Each disclosed deposit-to-claim pair is a public link from a vault leaf to a LEZ
destination and a spent nullifier, and every such link leaves the candidate set that everyone
else's deposits hide in. Each disclosed burn is a public recognition of one burn note as the
user's, with its recipient. The mechanism weakens nobody's privacy; a third party that breaks
confidence weakens everyone's, one published pair at a time.

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
