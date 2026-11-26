# Disclosure replay: crown-rc3-rehearsal-1

This is a replay against the recorded run `e2e/runs/crown-rc3-rehearsal-1/`, an Anvil fork of
Sepolia with a stand-in LEZ beacon committee. No chain value in `VIEW.json` is synthetic. The
wallet uses the deliberately public end-to-end test seed; this is not a real user's secret.
`VIEW.json` and `DISCLOSURE.json` are the files consumed by the CLI and by
`bridge/crates/ferrata-disclose/tests/cli.rs`.

## Field provenance

Paths below are relative to `e2e/runs/crown-rc3-rehearsal-1/` unless noted.

| VIEW.json field | Recorded source and transformation |
| --- | --- |
| `chainId` | `crown.json` `.chainId` (`11155111`), encoded as a 32-byte big-endian `uint256` and hex. `deploy.json` has the same id. |
| `vault` | `crown.json` `.vault`, lowercased and without `0x`. `deploy.json` and `session-b.json` have the same address. |
| `configId` | `identity.json` `.configId`, without `0x`; cross-checked by every `lez/session-b/burn-notes.json` entry's `.configId`. |
| `deposits` | The two `Deposit` logs in `captures/capture-2/deposits_12162012.json`, which are cumulative through deposit 1. Topic 1 supplies `depositor`, topic 2 supplies `token`; the four 32-byte ABI data words supply `j`, `leafIndex`, `amount`, and `commitment` in that order. Addresses lose their 12-byte topic padding; `amount` stays a 32-byte word. Both commitments match `crown.json` `.deposits[].commitment`. |
| `spentNullifiers` | `lez/session-b/session-b.json` `.claims[].nullifier`, in claim order, without `0x`. |
| `burns` | `lez/session-b/burn-notes.json` supplies each entry's `r`, `memo`, `mac`, `token`, decimal `amount`, and `cOut`. Amounts are converted to 32-byte big-endian words. `burnIndex` comes from `checkpoint/input/burns.json` `.burns[].burn_index`, matched by the same entry's `r`; the two entries have indices 0 and 1. |

`DESTINATIONS.json` (the wallet's own input to `make`, not part of the public view): `crown.json` `.deposits[]` supplies
`kind` (1 private, 2 public) and `dest`; `account` is that entry's `j`. This is the mapping implemented by the public
crown driver: public test account 0 for j=0 and private test account 0 for j=1.

The binary's doc comment and `docs/privacy/viewing-keys.md` describe the JSON encoding. Every field `verify` reads comes
from the public run records.

The bridge seed passed to `make` is
`sha256(b"e2e: user bridge seed (test value)")`. The public source for both the label and its use
is `lez/crown/driver/src/crown.rs`: `BRIDGE_SEED_LABEL`, `bridge_keys()`, and `destination(j)`.
`BridgeKeys::from_bridge_seed` is then used unchanged. No recorded secret file or external query is
needed.

## Reproduction

From the worktree root:

```sh
printf %s 'e2e: user bridge seed (test value)' | sha256sum | cut -d' ' -f1 > seed.hex   # the public test seed
cargo run --manifest-path bridge/Cargo.toml --locked --offline -p ferrata-disclose \
  --bin ferrata-disclose -- make --bridge-seed-file seed.hex \
  --destinations docs/privacy/disclosure-replay/crown-rc3-rehearsal-1/DESTINATIONS.json \
  --view docs/privacy/disclosure-replay/crown-rc3-rehearsal-1/VIEW.json \
  --out docs/privacy/disclosure-replay/crown-rc3-rehearsal-1/DISCLOSURE.json

cargo run --manifest-path bridge/Cargo.toml --locked --offline -p ferrata-disclose \
  --bin ferrata-disclose -- verify \
  --disclosure docs/privacy/disclosure-replay/crown-rc3-rehearsal-1/DISCLOSURE.json \
  --view docs/privacy/disclosure-replay/crown-rc3-rehearsal-1/VIEW.json
```

The committed report verifies two claimed deposits (leaves 0 and 1, with the two claim nullifiers
from `session-b.json`) and two burns (indices 0 and 1, both for 0.001 ETH to the recorded deployer).
The integration test also flips exactly one byte in the first claim's `s` hex text in a scratch
copy and requires exit 1 with a named verification error; it never modifies the committed pair.
