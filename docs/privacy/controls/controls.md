# Privacy gate: negative controls (generated)

Written by `scripts/privacy/controls.py`; see [CONTROLS.md](../CONTROLS.md) for what this is.

Planted leaks: 275 copies of the public data; 272 must be caught, 272 are (gate 78, hidden-column search 255, adversary 74); 3 are leaks the gate is not expected to see, of which 0 were seen.

Weakened detectors: 16, caught 16.

## Planted leaks

| step | value | carrier | encoding | gate | hidden search | adversary | caught | expected |
|---|---|---|---|---|---|---|---|---|
| deposit | destination | const | raw | yes | yes | no | **yes** | caught |
| deposit | destination | fresh | raw | no | yes | no | **yes** | caught |
| deposit | destination | state | raw | no | yes | no | **yes** | caught |
| deposit | destination | fresh | reversed | no | yes | no | **yes** | caught |
| deposit | destination | fresh | hex | no | yes | no | **yes** | caught |
| deposit | destination | fresh | prefix16 | no | yes | no | **yes** | caught |
| deposit | destination | fresh | suffix16 | no | yes | no | **yes** | caught |
| deposit | s | const | raw | yes | yes | no | **yes** | caught |
| deposit | s | fresh | raw | no | yes | no | **yes** | caught |
| deposit | s | state | raw | no | yes | no | **yes** | caught |
| deposit | s | fresh | reversed | no | yes | no | **yes** | caught |
| deposit | s | fresh | hex | no | yes | no | **yes** | caught |
| deposit | s | fresh | prefix16 | no | yes | no | **yes** | caught |
| deposit | s | fresh | suffix16 | no | yes | no | **yes** | caught |
| deposit | bridge seed | const | raw | yes | yes | no | **yes** | caught |
| deposit | bridge seed | fresh | raw | no | yes | no | **yes** | caught |
| deposit | bridge seed | state | raw | no | yes | no | **yes** | caught |
| deposit | bridge seed | fresh | reversed | no | yes | no | **yes** | caught |
| deposit | bridge seed | fresh | hex | no | yes | no | **yes** | caught |
| deposit | bridge seed | fresh | prefix16 | no | yes | no | **yes** | caught |
| deposit | bridge seed | fresh | suffix16 | no | yes | no | **yes** | caught |
| deposit | kAuth | const | raw | yes | yes | no | **yes** | caught |
| deposit | kAuth | fresh | raw | no | yes | no | **yes** | caught |
| deposit | kAuth | state | raw | no | yes | no | **yes** | caught |
| deposit | kAuth | fresh | reversed | no | yes | no | **yes** | caught |
| deposit | kAuth | fresh | hex | no | yes | no | **yes** | caught |
| deposit | kAuth | fresh | prefix16 | no | yes | no | **yes** | caught |
| deposit | kAuth | fresh | suffix16 | no | yes | no | **yes** | caught |
| deposit | kView | const | raw | yes | yes | no | **yes** | caught |
| deposit | kView | fresh | raw | no | yes | no | **yes** | caught |
| deposit | kView | state | raw | no | yes | no | **yes** | caught |
| deposit | kView | fresh | reversed | no | yes | no | **yes** | caught |
| deposit | kView | fresh | hex | no | yes | no | **yes** | caught |
| deposit | kView | fresh | prefix16 | no | yes | no | **yes** | caught |
| deposit | kView | fresh | suffix16 | no | yes | no | **yes** | caught |
| claim | destination | const | raw | yes | yes | no | **yes** | caught |
| claim | destination | message | raw | yes | yes | no | **yes** | caught |
| claim | destination | fresh | raw | no | yes | no | **yes** | caught |
| claim | destination | state | raw | yes | yes | no | **yes** | caught |
| claim | destination | fresh | reversed | no | yes | no | **yes** | caught |
| claim | destination | fresh | hex | no | yes | no | **yes** | caught |
| claim | destination | fresh | prefix16 | no | yes | no | **yes** | caught |
| claim | destination | fresh | suffix16 | no | yes | no | **yes** | caught |
| claim | s | const | raw | yes | yes | no | **yes** | caught |
| claim | s | message | raw | yes | yes | no | **yes** | caught |
| claim | s | fresh | raw | no | yes | no | **yes** | caught |
| claim | s | state | raw | yes | yes | no | **yes** | caught |
| claim | s | fresh | reversed | no | yes | no | **yes** | caught |
| claim | s | fresh | hex | no | yes | no | **yes** | caught |
| claim | s | fresh | prefix16 | no | yes | no | **yes** | caught |
| claim | s | fresh | suffix16 | no | yes | no | **yes** | caught |
| claim | C | const | raw | yes | yes | yes | **yes** | caught |
| claim | C | message | raw | yes | yes | yes | **yes** | caught |
| claim | C | fresh | raw | no | yes | yes | **yes** | caught |
| claim | C | state | raw | yes | yes | yes | **yes** | caught |
| claim | C | fresh | reversed | no | yes | no | **yes** | caught |
| claim | C | fresh | hex | no | yes | no | **yes** | caught |
| claim | C | fresh | prefix16 | no | yes | yes | **yes** | caught |
| claim | C | fresh | suffix16 | no | yes | yes | **yes** | caught |
| claim | leaf | const | raw | yes | yes | yes | **yes** | caught |
| claim | leaf | message | raw | yes | yes | yes | **yes** | caught |
| claim | leaf | fresh | raw | no | yes | yes | **yes** | caught |
| claim | leaf | state | raw | yes | yes | yes | **yes** | caught |
| claim | leaf | fresh | reversed | no | yes | no | **yes** | caught |
| claim | leaf | fresh | hex | no | yes | no | **yes** | caught |
| claim | leaf | fresh | prefix16 | no | yes | yes | **yes** | caught |
| claim | leaf | fresh | suffix16 | no | yes | yes | **yes** | caught |
| claim | fid | const | raw | yes | yes | yes | **yes** | caught |
| claim | fid | message | raw | yes | yes | yes | **yes** | caught |
| claim | fid | fresh | raw | no | yes | yes | **yes** | caught |
| claim | fid | state | raw | yes | yes | yes | **yes** | caught |
| claim | fid | fresh | reversed | no | yes | no | **yes** | caught |
| claim | fid | fresh | hex | no | yes | no | **yes** | caught |
| claim | fid | fresh | prefix16 | no | yes | yes | **yes** | caught |
| claim | fid | fresh | suffix16 | no | yes | yes | **yes** | caught |
| claim | depositor | const | raw | yes | yes | yes | **yes** | caught |
| claim | depositor | message | raw | yes | yes | yes | **yes** | caught |
| claim | depositor | fresh | raw | no | yes | yes | **yes** | caught |
| claim | depositor | state | raw | yes | yes | yes | **yes** | caught |
| claim | depositor | fresh | reversed | no | yes | no | **yes** | caught |
| claim | depositor | fresh | hex | no | yes | no | **yes** | caught |
| claim | depositor | fresh | prefix16 | no | yes | yes | **yes** | caught |
| claim | depositor | fresh | suffix16 | no | yes | yes | **yes** | caught |
| claim | bridge seed | const | raw | yes | yes | no | **yes** | caught |
| claim | bridge seed | message | raw | yes | yes | no | **yes** | caught |
| claim | bridge seed | fresh | raw | no | yes | no | **yes** | caught |
| claim | bridge seed | state | raw | yes | yes | no | **yes** | caught |
| claim | bridge seed | fresh | reversed | no | yes | no | **yes** | caught |
| claim | bridge seed | fresh | hex | no | yes | no | **yes** | caught |
| claim | bridge seed | fresh | prefix16 | no | yes | no | **yes** | caught |
| claim | bridge seed | fresh | suffix16 | no | yes | no | **yes** | caught |
| claim | kAuth | const | raw | yes | yes | no | **yes** | caught |
| claim | kAuth | message | raw | yes | yes | no | **yes** | caught |
| claim | kAuth | fresh | raw | no | yes | no | **yes** | caught |
| claim | kAuth | state | raw | yes | yes | no | **yes** | caught |
| claim | kAuth | fresh | reversed | no | yes | no | **yes** | caught |
| claim | kAuth | fresh | hex | no | yes | no | **yes** | caught |
| claim | kAuth | fresh | prefix16 | no | yes | no | **yes** | caught |
| claim | kAuth | fresh | suffix16 | no | yes | no | **yes** | caught |
| claim | kView | const | raw | yes | yes | no | **yes** | caught |
| claim | kView | message | raw | yes | yes | no | **yes** | caught |
| claim | kView | fresh | raw | no | yes | no | **yes** | caught |
| claim | kView | state | raw | yes | yes | no | **yes** | caught |
| claim | kView | fresh | reversed | no | yes | no | **yes** | caught |
| claim | kView | fresh | hex | no | yes | no | **yes** | caught |
| claim | kView | fresh | prefix16 | no | yes | no | **yes** | caught |
| claim | kView | fresh | suffix16 | no | yes | no | **yes** | caught |
| burn | holder (private account) | const | raw | yes | yes | no | **yes** | caught |
| burn | holder (private account) | message | raw | yes | yes | no | **yes** | caught |
| burn | holder (private account) | fresh | raw | no | yes | no | **yes** | caught |
| burn | holder (private account) | state | raw | yes | yes | no | **yes** | caught |
| burn | holder (private account) | fresh | reversed | no | yes | no | **yes** | caught |
| burn | holder (private account) | fresh | hex | no | yes | no | **yes** | caught |
| burn | holder (private account) | fresh | prefix16 | no | yes | no | **yes** | caught |
| burn | holder (private account) | fresh | suffix16 | no | yes | no | **yes** | caught |
| burn | recipient | const | raw | yes | yes | yes | **yes** | caught |
| burn | recipient | message | raw | yes | yes | yes | **yes** | caught |
| burn | recipient | fresh | raw | no | yes | yes | **yes** | caught |
| burn | recipient | state | raw | yes | yes | yes | **yes** | caught |
| burn | recipient | fresh | reversed | no | yes | no | **yes** | caught |
| burn | recipient | fresh | hex | no | yes | no | **yes** | caught |
| burn | recipient | fresh | prefix16 | no | yes | yes | **yes** | caught |
| burn | recipient | fresh | suffix16 | no | yes | yes | **yes** | caught |
| burn | sOut | const | raw | yes | yes | no | **yes** | caught |
| burn | sOut | message | raw | yes | yes | no | **yes** | caught |
| burn | sOut | fresh | raw | no | yes | no | **yes** | caught |
| burn | sOut | state | raw | yes | yes | no | **yes** | caught |
| burn | sOut | fresh | reversed | no | yes | no | **yes** | caught |
| burn | sOut | fresh | hex | no | yes | no | **yes** | caught |
| burn | sOut | fresh | prefix16 | no | yes | no | **yes** | caught |
| burn | sOut | fresh | suffix16 | no | yes | no | **yes** | caught |
| burn | NOut | const | raw | yes | yes | yes | **yes** | caught |
| burn | NOut | message | raw | yes | yes | yes | **yes** | caught |
| burn | NOut | fresh | raw | no | yes | yes | **yes** | caught |
| burn | NOut | state | raw | yes | yes | yes | **yes** | caught |
| burn | NOut | fresh | reversed | no | yes | no | **yes** | caught |
| burn | NOut | fresh | hex | no | yes | no | **yes** | caught |
| burn | NOut | fresh | prefix16 | no | yes | yes | **yes** | caught |
| burn | NOut | fresh | suffix16 | no | yes | yes | **yes** | caught |
| burn | memo pad | const | raw | yes | yes | no | **yes** | caught |
| burn | memo pad | message | raw | yes | yes | no | **yes** | caught |
| burn | memo pad | fresh | raw | no | yes | no | **yes** | caught |
| burn | memo pad | state | raw | yes | yes | no | **yes** | caught |
| burn | memo pad | fresh | reversed | no | yes | no | **yes** | caught |
| burn | memo pad | fresh | hex | no | yes | no | **yes** | caught |
| burn | memo pad | fresh | prefix16 | no | yes | no | **yes** | caught |
| burn | memo pad | fresh | suffix16 | no | yes | no | **yes** | caught |
| burn | bridge seed | const | raw | yes | yes | no | **yes** | caught |
| burn | bridge seed | message | raw | yes | yes | no | **yes** | caught |
| burn | bridge seed | fresh | raw | no | yes | no | **yes** | caught |
| burn | bridge seed | state | raw | yes | yes | no | **yes** | caught |
| burn | bridge seed | fresh | reversed | no | yes | no | **yes** | caught |
| burn | bridge seed | fresh | hex | no | yes | no | **yes** | caught |
| burn | bridge seed | fresh | prefix16 | no | yes | no | **yes** | caught |
| burn | bridge seed | fresh | suffix16 | no | yes | no | **yes** | caught |
| burn | kAuth | const | raw | yes | yes | no | **yes** | caught |
| burn | kAuth | message | raw | yes | yes | no | **yes** | caught |
| burn | kAuth | fresh | raw | no | yes | no | **yes** | caught |
| burn | kAuth | state | raw | yes | yes | no | **yes** | caught |
| burn | kAuth | fresh | reversed | no | yes | no | **yes** | caught |
| burn | kAuth | fresh | hex | no | yes | no | **yes** | caught |
| burn | kAuth | fresh | prefix16 | no | yes | no | **yes** | caught |
| burn | kAuth | fresh | suffix16 | no | yes | no | **yes** | caught |
| burn | kView | const | raw | yes | yes | no | **yes** | caught |
| burn | kView | message | raw | yes | yes | no | **yes** | caught |
| burn | kView | fresh | raw | no | yes | no | **yes** | caught |
| burn | kView | state | raw | yes | yes | no | **yes** | caught |
| burn | kView | fresh | reversed | no | yes | no | **yes** | caught |
| burn | kView | fresh | hex | no | yes | no | **yes** | caught |
| burn | kView | fresh | prefix16 | no | yes | no | **yes** | caught |
| burn | kView | fresh | suffix16 | no | yes | no | **yes** | caught |
| release | r | const | raw | yes | yes | yes | **yes** | caught |
| release | r | fresh | raw | no | yes | yes | **yes** | caught |
| release | r | state | raw | no | yes | no | **yes** | caught |
| release | r | fresh | reversed | no | yes | no | **yes** | caught |
| release | r | fresh | hex | no | yes | no | **yes** | caught |
| release | r | fresh | prefix16 | no | yes | yes | **yes** | caught |
| release | r | fresh | suffix16 | no | yes | yes | **yes** | caught |
| release | COut | const | raw | yes | yes | yes | **yes** | caught |
| release | COut | fresh | raw | no | yes | yes | **yes** | caught |
| release | COut | state | raw | no | yes | no | **yes** | caught |
| release | COut | fresh | reversed | no | yes | no | **yes** | caught |
| release | COut | fresh | hex | no | yes | no | **yes** | caught |
| release | COut | fresh | prefix16 | no | yes | yes | **yes** | caught |
| release | COut | fresh | suffix16 | no | yes | yes | **yes** | caught |
| release | memo | const | raw | yes | yes | yes | **yes** | caught |
| release | memo | fresh | raw | no | yes | yes | **yes** | caught |
| release | memo | state | raw | no | yes | no | **yes** | caught |
| release | memo | fresh | reversed | no | yes | no | **yes** | caught |
| release | memo | fresh | hex | no | yes | no | **yes** | caught |
| release | memo | fresh | prefix16 | no | yes | yes | **yes** | caught |
| release | memo | fresh | suffix16 | no | yes | yes | **yes** | caught |
| release | mac | const | raw | yes | yes | yes | **yes** | caught |
| release | mac | fresh | raw | no | yes | yes | **yes** | caught |
| release | mac | state | raw | no | yes | no | **yes** | caught |
| release | mac | fresh | reversed | no | yes | no | **yes** | caught |
| release | mac | fresh | hex | no | yes | no | **yes** | caught |
| release | mac | fresh | prefix16 | no | yes | yes | **yes** | caught |
| release | mac | fresh | suffix16 | no | yes | yes | **yes** | caught |
| release | burn leaf | const | raw | yes | yes | yes | **yes** | caught |
| release | burn leaf | fresh | raw | no | yes | yes | **yes** | caught |
| release | burn leaf | state | raw | no | yes | no | **yes** | caught |
| release | burn leaf | fresh | reversed | no | yes | no | **yes** | caught |
| release | burn leaf | fresh | hex | no | yes | no | **yes** | caught |
| release | burn leaf | fresh | prefix16 | no | yes | yes | **yes** | caught |
| release | burn leaf | fresh | suffix16 | no | yes | yes | **yes** | caught |
| release | burnId | const | raw | yes | yes | yes | **yes** | caught |
| release | burnId | fresh | raw | no | yes | yes | **yes** | caught |
| release | burnId | state | raw | no | yes | no | **yes** | caught |
| release | burnId | fresh | reversed | no | yes | no | **yes** | caught |
| release | burnId | fresh | hex | no | yes | no | **yes** | caught |
| release | burnId | fresh | prefix16 | no | yes | yes | **yes** | caught |
| release | burnId | fresh | suffix16 | no | yes | yes | **yes** | caught |
| release | holder (private account) | const | raw | yes | yes | no | **yes** | caught |
| release | holder (private account) | fresh | raw | no | yes | no | **yes** | caught |
| release | holder (private account) | state | raw | no | yes | no | **yes** | caught |
| release | holder (private account) | fresh | reversed | no | yes | no | **yes** | caught |
| release | holder (private account) | fresh | hex | no | yes | no | **yes** | caught |
| release | holder (private account) | fresh | prefix16 | no | yes | no | **yes** | caught |
| release | holder (private account) | fresh | suffix16 | no | yes | no | **yes** | caught |
| release | sOut | const | raw | yes | yes | no | **yes** | caught |
| release | sOut | fresh | raw | no | yes | no | **yes** | caught |
| release | sOut | state | raw | no | yes | no | **yes** | caught |
| release | sOut | fresh | reversed | no | yes | no | **yes** | caught |
| release | sOut | fresh | hex | no | yes | no | **yes** | caught |
| release | sOut | fresh | prefix16 | no | yes | no | **yes** | caught |
| release | sOut | fresh | suffix16 | no | yes | no | **yes** | caught |
| release | memo pad | const | raw | yes | yes | no | **yes** | caught |
| release | memo pad | fresh | raw | no | yes | no | **yes** | caught |
| release | memo pad | state | raw | no | yes | no | **yes** | caught |
| release | memo pad | fresh | reversed | no | yes | no | **yes** | caught |
| release | memo pad | fresh | hex | no | yes | no | **yes** | caught |
| release | memo pad | fresh | prefix16 | no | yes | no | **yes** | caught |
| release | memo pad | fresh | suffix16 | no | yes | no | **yes** | caught |
| release | bridge seed | const | raw | yes | yes | no | **yes** | caught |
| release | bridge seed | fresh | raw | no | yes | no | **yes** | caught |
| release | bridge seed | state | raw | no | yes | no | **yes** | caught |
| release | bridge seed | fresh | reversed | no | yes | no | **yes** | caught |
| release | bridge seed | fresh | hex | no | yes | no | **yes** | caught |
| release | bridge seed | fresh | prefix16 | no | yes | no | **yes** | caught |
| release | bridge seed | fresh | suffix16 | no | yes | no | **yes** | caught |
| release | kAuth | const | raw | yes | yes | no | **yes** | caught |
| release | kAuth | fresh | raw | no | yes | no | **yes** | caught |
| release | kAuth | state | raw | no | yes | no | **yes** | caught |
| release | kAuth | fresh | reversed | no | yes | no | **yes** | caught |
| release | kAuth | fresh | hex | no | yes | no | **yes** | caught |
| release | kAuth | fresh | prefix16 | no | yes | no | **yes** | caught |
| release | kAuth | fresh | suffix16 | no | yes | no | **yes** | caught |
| release | kView | const | raw | yes | yes | no | **yes** | caught |
| release | kView | fresh | raw | no | yes | no | **yes** | caught |
| release | kView | state | raw | no | yes | no | **yes** | caught |
| release | kView | fresh | reversed | no | yes | no | **yes** | caught |
| release | kView | fresh | hex | no | yes | no | **yes** | caught |
| release | kView | fresh | prefix16 | no | yes | no | **yes** | caught |
| release | kView | fresh | suffix16 | no | yes | no | **yes** | caught |
| deposit | N (the claim's nullifier) | const | raw | yes | no | yes | **yes** | caught |
| deposit | N (the claim's nullifier) | fresh | raw | no | no | yes | **yes** | caught |
| claim | H(C) | const | raw | yes | no | yes | **yes** | caught |
| claim | H(C) | fresh | raw | no | no | yes | **yes** | caught |
| claim | H(leaf) | const | raw | yes | no | yes | **yes** | caught |
| claim | H(leaf) | fresh | raw | no | no | yes | **yes** | caught |
| claim | H(fid) | const | raw | yes | no | yes | **yes** | caught |
| claim | H(fid) | fresh | raw | no | no | yes | **yes** | caught |
| release | H(burn leaf) | const | raw | yes | no | yes | **yes** | caught |
| release | H(burn leaf) | fresh | raw | no | no | yes | **yes** | caught |
| release | H(burnId) | const | raw | yes | no | yes | **yes** | caught |
| release | H(burnId) | fresh | raw | no | no | yes | **yes** | caught |
| release | H(note), the burn record's key | const | raw | yes | no | yes | **yes** | caught |
| release | H(note), the burn record's key | fresh | raw | no | no | yes | **yes** | caught |
| claim | leaf index (u64) | const | raw | yes | no | no | **yes** | caught |
| claim | leaf index (u64) | message | raw | yes | no | no | **yes** | caught |
| release | burn index (u64) | const | raw | yes | no | no | **yes** | caught |
| claim | leaf index (u64) | fresh | raw | no | no | no | **no** | limit: low entropy in a fresh field |
| claim | s XOR a public constant | fresh | raw | no | no | no | **no** | limit: an encoding outside the four |
| claim | s in two 8-byte halves, 40 bytes apart | fresh | raw | no | no | no | **no** | limit: no 16-byte window |

## Weakened detectors

| detector | mutant | false alarm on the unmodified data | planted leaks the real detector finds | missed by the mutant | missed by the whole gate | caught | example |
|---|---|---|---:|---:|---:|---|---|
| hidden | no byte-reversed form | no | 255 | 34 | 34 | **yes** | deposit: destination (fresh, reversed) |
| hidden | no hex form | no | 255 | 34 | 34 | **yes** | deposit: destination (fresh, hex) |
| hidden | whole values only, no 16-byte windows | no | 255 | 82 | 60 | **yes** | deposit: destination (fresh, hex) |
| hidden | only a prefix of each value (its first 16 bytes) | no | 255 | 33 | 22 | **yes** | deposit: destination (fresh, suffix16) |
| hidden | only a prefix of each record (its first 2,048 bytes) | no | 255 | 34 | 0 | **yes** | claim: destination (const, raw) |
| hidden | encrypted notes skipped | no | 255 | 85 | 67 | **yes** | claim: destination (fresh, raw) |
| hidden | state skipped (LEZ account states, Ethereum storage writes) | no | 255 | 22 | 5 | **yes** | deposit: destination (state, raw) |
| hidden | events skipped | no | 255 | 5 | 0 | **yes** | deposit: destination (const, raw) |
| gate | every varying field cut, classified or not | no | 78 | 40 | 2 | **yes** | deposit: destination (const, raw) |
| gate | classes too broad (Issue.*, Burn.*, fee.*, request.* as fresh randomness) | no | 78 | 18 | 1 | **yes** | claim: destination (message, raw) |
| gate | only the first 256 bytes of each part compared | no | 78 | 10 | 1 | **yes** | claim: destination (message, raw) |
| gate | only each part's length compared | no | 78 | 56 | 2 | **yes** | claim: destination (const, raw) |
| gate | account states and storage values not compared | no | 78 | 17 | 0 | **yes** | claim: destination (state, raw) |
| adversary | no derived identifiers | no | 74 | 32 | 6 | **yes** | claim: leaf (const, raw) |
| adversary | 32-byte windows instead of 16 | no | 74 | 36 | 0 | **yes** | claim: C (fresh, prefix16) |
| adversary | state and events not read | no | 74 | 6 | 0 | **yes** | claim: C (state, raw) |
