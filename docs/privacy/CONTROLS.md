# Privacy gate: negative controls (18 November 2026)

The privacy suite ([`RESULTS.md`](RESULTS.md)) says that no public byte of a round trip reveals what the deposit,
the claim, the burn and the release must hide. A gate that passes is only worth something if it fails when it
should. This page shows that it does: copies of the suite's public data with a secret planted in them, and copies of
the gate's own detectors with a weakness planted in them.

**Where it ran.** The Modal build box of this worktree (2 vCPU). First `scripts/privacy/run.sh` at this commit's code
(both halves in 342 s from a cold build, RISC Zero dev mode, Foundry with the mock verifier; the bridge program built
at this worktree's path, image `ccde6171…71dd`, [`controls/baseline/summary.md`](controls/baseline/summary.md)); then
`scripts/privacy/controls.py` on that run's data, 68 s. Every number below is copied from
[`controls/controls.json`](controls/controls.json), which the script wrote; the full tables are
[`controls/controls.md`](controls/controls.md).

## The gate

The suite's verdict on one operation's 20 records is three detectors, all `scripts/privacy/adversary.py`'s own
functions, unchanged except for the hidden-column search (below):

1. **the differential gate** (`gate`): the records must be byte for byte identical once the values of fields
   classified as nullifiers, fresh randomness, a user's own address or time are cut out where they vary;
2. **the hidden-column search** (`hidden_check`): every value §B6 says the step must hide is searched for in each
   record's whole public data;
3. **the adversary** (`attack`): it pairs each claim with a deposit and each release with a burn from shared 16-byte
   windows and the identifiers it can derive; it must learn nothing (every candidate set is all 20 records).

The gate catches a copy if any of the three fires. `controls.py` first runs the suite's own `main()` on the unmodified
data ([`controls/baseline/`](controls/baseline/)), keeping the exact groups, contexts and secrets it used, then calls
the same three functions on the planted copies.

**Changed with this work: the hidden-column search.** It used to look for 320 values (16 per user) verbatim. It now
looks for 680 (34 per user): the seed and the two keys it derives (`kAuth`, `kView`, design note §4) in every step;
in the burn also `sOut` (the input of `NOut`), `NOut` itself and the memo pad; in the release also the holder, `sOut`
and the memo pad; each value as it is, byte-reversed (a little-endian reading), and as hex text in lower and upper case,
whole and as every 16-byte window with at least 12 non-zero bytes (the adversary's rule for hash-like content). On the
unmodified data it finds **0 of 680**; the gate passes the four default operations and the adversary is at chance
(5.0%) inbound and outbound, as before. The committed [`generated/`](generated/) and [`real/`](real/) outputs predate
this change (they say 320) and were not re-run.

## (a) Planted leaks: 272 of 272 caught

For every value an operation must hide, a test-only copy of that operation's 20 records with the value written into
a public field of every record, each user's own value in its own record:

| operation | values planted |
|---|---|
| deposit (hides the LEZ destination) | destination, `s`, seed, `kAuth`, `kView`; and the claim's `N` |
| claim (hides which deposit and a private destination) | destination, `s`, `C`, leaf, `fid`, depositor, seed, `kAuth`, `kView` (`N`'s inputs are `s` and `fid`); `H(C)`, `H(leaf)`, `H(fid)`; the leaf index |
| burn (hides the holder and the recipient) | holder, recipient, `sOut`, `NOut`, memo pad, seed, `kAuth`, `kView` |
| release (hides which burn) | `r`, `COut`, memo, mac, burn leaf, `burnId`, holder, `sOut`, memo pad, seed, `kAuth`, `kView`; `H(burn leaf)`, `H(burnId)`, the note's hash (the burn record's key); the burn index |

Fields written into ("carriers"): a **constant** field (the fee reserve's refund account `fee.refund_to` on LEZ, which
a wallet naming its own account would vary, RESULTS.md finding 3; the release's `burnRoot`; an extra vault event on the
deposit), a **message** field (the claim's `Issue.relayer`, the burn's `Burn.config_id`), a **fresh-randomness** field
(the LEZ encrypted note, `C`, `NOut`), and the **state** (the largest per-record LEZ account state; the deposit root's
or the escrow's storage write). Each value raw in each carrier, and in the fresh field also byte-reversed, as hex text,
and as its first or last 16 bytes. The hashes (`H(C)` ...) are values only a linker derives; the indices are 8-byte
numbers.

| operation | copies | caught | by the differential gate | by the hidden-column search | by the adversary | by the search alone |
|---|---:|---:|---:|---:|---:|---:|
| deposit | 37 | **37** | 6 | 35 | 2 | 30 |
| claim | 80 | **80** | 32 | 72 | 30 | 33 |
| burn | 64 | **64** | 24 | 64 | 12 | 34 |
| release | 91 | **91** | 16 | 84 | 30 | 54 |
| all | 272 | **272** | 78 | 255 | 74 | 151 |

What each detector is for shows in the columns. In a fresh-randomness field (177 copies) the differential gate sees
nothing, by design (it cuts such fields); the search finds 170 and the adversary 43, and the 7 the search does not
find are the values only a linker derives (`N` on a deposit, the six hashes), which the adversary finds. The 3
low-entropy indices, in a constant or a message field, are seen by the differential gate alone.

## (b) Weakened detectors: 16 of 16 caught

Each detector mutated in one way. Every mutant passes the unmodified data (no false alarm), and every one misses
planted leaks the real detector finds; "the whole gate" counts the copies the mutant lets through when the other two
detectors run unchanged.

| detector | weakened by | planted leaks the real one finds | missed by the mutant | missed by the whole gate |
|---|---|---:|---:|---:|
| hidden-column search | no byte-reversed form | 255 | 34 | 34 |
| hidden-column search | no hex form | 255 | 34 | 34 |
| hidden-column search | whole values only, no 16-byte windows | 255 | 82 | 60 |
| hidden-column search | only a prefix of each value (its first 16 bytes) | 255 | 33 | 22 |
| hidden-column search | only a prefix of each record (its first 2,048 bytes) | 255 | 34 | 0 |
| hidden-column search | the encrypted notes skipped | 255 | 85 | 67 |
| hidden-column search | the state skipped (LEZ account states, Ethereum storage writes) | 255 | 22 | 5 |
| hidden-column search | the events skipped | 255 | 5 | 0 |
| differential gate | every varying field cut, classified or not | 78 | 40 | 2 |
| differential gate | classes too broad (`Issue.*`, `Burn.*`, `fee.*`, `request.*` as fresh randomness) | 78 | 18 | 1 |
| differential gate | only the first 256 bytes of each part compared | 78 | 10 | 1 |
| differential gate | only each part's length compared | 78 | 56 | 2 |
| differential gate | account states and storage values not compared | 78 | 17 | 0 |
| adversary | no derived identifiers | 74 | 32 | 6 |
| adversary | 32-byte windows instead of 16 | 74 | 36 | 0 |
| adversary | state and events not read | 74 | 6 | 0 |

11 of the 16 change the whole gate's verdict on some copy; the other 5 are covered by another detector on every
planted copy, which is why the suite has three. The search is the one that cannot be weakened without letting leaks
through: dropping the reversed or hex forms lets 34 copies pass the whole gate, skipping the encrypted notes 67.

## (c) What the gate cannot see

Three planted copies the gate is **not** expected to catch, run to show it: all three pass all three detectors.

- **A low-entropy secret in a fresh-randomness field** (the claim's leaf index, 8 bytes, at the start of the encrypted
  note). The search cannot tell an 8-byte number from unrelated bytes, the differential gate cuts the field, the
  adversary reads only hash-like windows. Small values (indices, `j`, amounts) are covered only where the gate compares
  them, in fields that are otherwise constant.
- **A secret in an encoding other than the four** (`s` XOR a public constant). Any reversible transform the search
  does not try (XOR, base64, a shifted hex nibble, a hash the adversary does not compute) hides it from every detector.
- **A secret split into pieces shorter than 16 bytes** (`s` in two 8-byte halves, 40 bytes apart).

And what no byte comparison can see:

- **Timing.** Block ids, timestamps, the order of operations and the delay between a deposit and its claim are cut
  as time and order. The suite holds them uninformative (random orders, one minute apart, every deposit before every
  claim); real traffic does not, and a deposit claimed alone in a quiet hour is linked by time. §B6's timing
  residual is the client's delay, not the gate's.
- **Amounts and tokens.** One token and one denomination in the suite, so the anonymity set is all 20. Denominations,
  tokens, fee schedules, relayers and the first use of a relayer split the set, and the gate does not measure that.
- **Elimination by small anonymity sets.** The adversary scores pairs within one batch of 20. Who has already claimed,
  how many deposits are outstanding, a set of size one at a quiet time: none of it is in a byte, and none of it is
  tested here.
- **Real proofs' randomness.** In dev mode the proof is a fake receipt (classified, compared by structure). The
  real-proof run ([`real/RESULTS.md`](real/RESULTS.md)) tests the receipts' parts; the zero-knowledge property itself
  is assumed.
- **Values not on the list.** The search finds what it is told to look for (34 values a user, 680 in all). A field
  that leaks something not on the list (a client-chosen account, a reused alias, RESULTS.md findings 3 and 9) is
  caught only if it varies where the gate expects a constant, or if it links.
- **Network metadata**, the relayer's view and the wallet's local state are outside the data.

## Reproduce

```sh
scripts/privacy/run.sh                                                   # the suite; raw data to target/privacy/
python3 -I scripts/privacy/controls.py target/privacy docs/privacy/controls   # the controls (68 s on 2 vCPU)
```

The controls are deterministic for a given run of the collectors; the bridge program's image (and so a few ids in the
data) depends on the path it was built at.
