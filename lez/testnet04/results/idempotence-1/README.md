# Idempotence 1: `day1.sh run` repeated with its state lost, its stored `Init` lost, another URL spelling

18 November 2026, 20:00-20:07 UTC, on the Modal build box, after an independent review (review-testnet04, finding 3:
`Init` and `Attest` were kept only in the state directory, so a run with that directory lost could sign a second
`Init` for a new instance). The signed stages, the instance's checkpoint and its id now live beside the keys
(`KEYS/tx/`, `KEYS/init/`, `KEYS/instance.json`), and `Init` is not signed when its instance exists on chain. This test
shows it on a fresh zone of our own (LEZ v0.4.0-rc3's sequencer and its Logos Blockchain node 0.3.5-rc.1, one validator;
**stand-in** for the public network, reached by URL through the box's own address as in rehearsal 1), with the tool
built from this tree (`ferrata-day1` sha256 `5b908a46…`, `box/tools.log`). The hook: [`../../tests/idempotence-hook.sh`](../../tests/idempotence-hook.sh).

| | what was done | the payer's nonce after | sent |
|---|---|---:|---|
| run 1 | `plan`, the stand-in funding, `FERRATA_DAY1_UNTIL=b day1.sh run`: the program in 8 transactions (blocks to 19, 158.4 s), `Init` in block 21 | 9 | 8 + 1 |
| run 2 | the state directory removed, the same `run` | 9 | nothing: "(a) the program is deployed already, byte for byte", "(b) the instance f32f9e03… exists on chain: no Init" |
| run 3 | the stored `Init` (`KEYS/tx/init`) and `KEYS/instance.json` set aside, the state removed: only the kept checkpoint (`KEYS/init/`) left | 9 | nothing: the tool signed no `Init` ("the instance f32f9e03… exists on chain already: no Init signed"), `run` counted (b) done |
| run 4 | another spelling of the URL (`…:3041/`): another state directory, the same keys | 9 | nothing (as run 2) |

Result (`out/events.tsv`): **pass**: the payer's nonce stayed 9 through the three repeated runs and no second `Init`
stage was written. The sequencer's log (`zone/logs/sequencer.log`) validated 11 transactions in all, the last at
20:05:09, before run 2 began (20:05:35): the two stand-in funding transfers, the 8 deployment transactions and the one
`Init`; none during runs 2-4.

What protects what (as `../../README.md` now says): the deployments, also the chain (their fresh accounts sign at
nonce 0); `Init`, the checkpoint kept beside the keys and a read of the chain; the attestation, the key directory
alone (an `Attest` changes no state that could be read back; it was not part of this test, which stopped after (b)).
Losing the key directory loses the keys.

## Also run on the box in the same job (`box/`)

- The reviewer's guard test (finding 1) against this build, with `--keys` added where `status` and `send` now need the
  key store (`box/guard-keys.log`): a URL with a userinfo (`http://127.0.0.1:1@HOST:PORT`, `http://localhost:1@…`) is
  refused before any connection, for reading and for `fund` (T6-T8: 0 calls to the dummy endpoint; before the fix T6
  and T8 read it and T7 reached the transfer); the other cases as before (an unnamed remote refused, a named one read
  but not sent to without `FERRATA_TESTNET_SEND=yes`, another spelling refused, the gate open with both).
- `cargo test --locked` as committed (LEZ v0.4.0-rc1): `lez/updater` 17 passed (9 in the library, with the strict URL
  parser's 3, and 8 in `tests/config.rs`); the lab driver 3 passed and the live driver 3 passed (the URL parser's
  tests, compiled into both through `live.rs`) (`box/test-*.log`).

No record here holds a key, a wallet or a signed transaction's bytes (`store/` keeps the stages' metadata and results
and the instance's checkpoint and id; the stored `Init` that was set aside is not kept).
