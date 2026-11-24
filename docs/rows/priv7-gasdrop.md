# Rows: RFP-022 Priv.7 and U.5 (stream priv7-gasdrop)

Branch `priv7-gasdrop` from `861c1c0d`. For the integrator: proposed status, the evidence per clause, a proposed
EVIDENCE row. Strict reading as in `docs/requirements-status.md`'s header.

## What was built

- **The tool uses the network's relayer by default** (`cli/src/gasdrop_net.rs`, `cli/src/main.rs`, `cli/src/flow.rs`):
  with no `--gas-drop-inbox`, `ferrata claim` reads `<origin of --sequencer>/relayer/gas-drop` (the relayer's LEZ
  account, the guest's image id, the native LEZ per drop, the inbox URL, what a drop pays for). The job is posted to the
  inbox URL. The guest the network serves is used only if the tool pins its image id (`gasdrop`, `gasdrop_31`), and the
  receipt must verify with exactly the natively computed journal before it leaves the machine. The dry run's `fee
  payer:` line names the relayer, where it was published and the opt-out (`--no-gas-drop` / `FERRATA_NO_GAS_DROP`).
  A network that publishes nothing (4xx) leaves the fee to the user's own account with the existing warning; one that
  cannot be read (5xx, transport) or is paused stops the claim (the weaker choice is never taken silently).
- **Burns**: a drop is sized for the claim and one burn; `ferrata burn` (private) pays its LEZ fee from a recorded drop
  account that still holds native LEZ, asking nothing new of any relayer (LEZ's private nonce and nullifier are derived
  with the account's `nsk`, so the relayer cannot recognise that spend); `--no-gas-drop` opts out.
- **Relayer side**: `ferrata-relayer --gas-drops-only` (no Ethereum key, other jobs refused); the test network's
  gateway serves `GET /relayer/gas-drop`, `GET /relayer/gas-drop/guest`, `POST /relayer/gas-drop` (shape and journal
  checks, one job per N held, inbox cap, per-IP / per-/24 / hourly caps; the binding bound is the relayer's one drop
  per deposit); `ferrata-gasdrop.service`, `ferrata-gasdrop-setup`, Caddy route, gateway inbox bind
  (`deploy/testnet/`, README "The gas-drop relayer: deploying it"). Not deployed: the lead deploys after review.
- **Web wallet**: the bridge tab reads the same terms and names the relayer, what it learns and the opt-out
  (`deploy/testnet/web/wallet/bridge.js`, `showGasDrop`).

## Priv.7: proposed **in part** (narrower)

| Clause | Evidence |
|---|---|
| private by default: the fee payer's mitigation with no user action, a relayer reachable by default over the network | `e2e/runs/seqnet-gasdrop-3/gasdrop.md` (8 of 8 PASS): with no gas-drop setting and no native LEZ of the user's own, the claim's dry run names the network's relayer read over HTTP from the test network's gateway; the private claim applied (LEZ tx `42121a36…2f83`), its fee paid by one-time account `0xa94c…06f3` from the relayer's sealed credit of 200,000,000 (relayer tx `48c1cd51…2de4`), the job posted to the gateway's inbox; `--no-gas-drop` names the user's own account |
| weakening needs explicit confirmation | `--no-gas-drop` is the only way to the user's own fee payer when a relayer is published; an unreadable or paused relayer stops the claim (`gasdrop_net::tests::discover_none_error_and_found`; gateway 503 checks) |

Still short: the mini-app's claim and burn (its drop is a stand-in, rows 102, 103); the live test network run (the
relayer is not deployed there yet, and no unclaimed deposit or wrapped balance exists on it: a run needs the lead's
deploy and one Sepolia deposit by the owner); real proofs on this path (the e2e is dev mode).

## U.5: proposed **in part** (narrower)

| Clause | Evidence |
|---|---|
| fee-payer mitigation on by default (CLI) | as Priv.7 above |
| fee-payer mitigation for burns | same run: the private burn applied (LEZ tx `dfcf79b4…aed`), its fee paid by the same one-time account from the 189,900,781 the credit left (the claim's fee 10,099,219, derived), no setting |
| indicator of what leaks | the claim's and the burn's `fee payer:` lines name the relayer, what it learns (N, the one-time account, a time, the posting address; never which deposit) and the opt-out; the web wallet's line (`test/gasdrop.test.mjs`, 4 tests) |
| relayer serves it with bounds | `deploy/testnet/tests/test_gateway_gasdrop.py` (26 checks) |

Still short: the mini-app (U.5 names the mini-app and the CLI); the live run; real proofs.

## Tests and commands (branch tip in REPORT)

- `cargo test --locked --lib gasdrop_net` (cli/, `RISC0_HOME` = LEZ's guest toolchain): 7 passed (box).
- `cargo check --locked --bins --tests --features seqnet` (cli/): clean (box).
- `python3 -I deploy/testnet/tests/test_gateway_gasdrop.py`: 26 checks; `test_units.py`: all passed.
- `node --test deploy/testnet/web/wallet/test/*.test.mjs`: 48 passed.
- `e2e/seqnet/run.sh gasdrop NAME` (box; `CROWN_BRIDGE_BIN` = B1's bridge program, sha256 `537cbe7a…`): `seqnet-gasdrop-3`, 8 of 8 PASS (dev-mode proofs; earlier attempts: gasdrop-1 stopped at the harness's vault deploy, DeployVault refusing the mock verifier, fixed in drive.sh; gasdrop-2 at the gateway's viewing-key bound, fixed)

## Proposed EVIDENCE row

| n | The gas drop by default: `ferrata claim` and `ferrata burn` with no gas-drop setting pay their LEZ fees from the network's relayer, read from the network's gateway | 8 of 8: relayer published, claim dry run names it, claim applied (fee from the one-time account, credit 200,000,000), drop recorded, user never funded, burn dry run names the drop account, opt-out names the own account, burn applied (fee from the 189,900,781 left) | local LEZ v0.4.0-rc1 sequencer and node, anvil, the test network's gateway on loopback (build box) | dev-mode proofs | `e2e/seqnet/run.sh gasdrop NAME` | `e2e/runs/seqnet-gasdrop-3/gasdrop.md` |
