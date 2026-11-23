# Rows: bridge-aux (the RISC Zero 3.1 vault on Sepolia: record, checks, Sourcify; the web wallet's deposit)

For the integrator. No status moves to **shown** from this stream alone. It adds evidence to rows already handled by
`docs/rows/bridge-31.md`.

## The deployment (owner, 23 Nov 2026, block 12,196,188)

| Contract | Address | Sourcify (14:21Z) |
|---|---|---|
| FerrataVault | `0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787` | exact_match `src/FerrataVault.sol` |
| FerrataRegistry (linked library) | `0x03953f8888f2666f5DF572551a3141b4B94e0C13` | exact_match |
| TestToken (TUSD, 6 decimals) | `0x5d4DdB1863d2EE85765499dd850a9AeedecA03bC` | exact_match |
| RiscZeroVerifierEmergencyStop | `0xC15916BDC7b32951B0310e0E24F98764EbA4b314` | exact_match (already held by Sourcify) |
| RiscZeroGroth16Verifier 3.1.0 | `0x43C858f02f209c6C10c06B64B4ab2Cefd8B8dc96` | exact_match |

- Record: `vault/deployments/sepolia-31-B.json` (commit `146a281a`), fetched from the deploy tool's volume.
- Read-only check: `RPC=<Sepolia endpoint> vault/deployments/sepolia-31-B.check.sh .`. It passed **47 of 47** on each
  of two public endpoints. Transcript: `vault/deployments/sepolia-31-B.verification.txt`. What it checks:
  - the five addresses are the deployer's creations at nonces 57 to 61;
  - the vault's code appears in block 12,196,188, not before;
  - the verifier: `VERSION` 3.1.0, `SELECTOR` 0xe71e6620;
  - the stop: owner renounced, not paused, wrapping that verifier; the vault calls the stop (no router);
  - programs, `lezConfigId`, `genesisState`, rule (B), nMin/tMin 1/1 as in `params/sepolia-31.json`;
  - admin, freeze authority, Δ = floor = 3600, the release floor, and the global, ETH and token caps as recorded;
  - ETH and TUSD registered.

  Also read, not asserted: one watcher, `0x78E5…E0B6`, which the record does not list.
- The local build from this tree (`vault/lib` as `lib.lock`) has the same CBOR metadata hash as the deployed code for
  all five contracts. So the sources Sourcify holds are this tree's.

## Proposed additions

- **RFP-022 R.8** (status unchanged, *in part*). Add to the Ethereum half: "deployed on Sepolia as
  `0x43C8…8dc96` and source-verified on Sourcify as an exact match of risc0-ethereum v3.1.0's
  `RiscZeroGroth16Verifier` (`vault/deployments/sepolia-31-B.verification.txt`)". The verifier is the unmodified
  upstream one, checked by bytecode and metadata, not only by `VERSION()`.
- **EVIDENCE row** (proposed text): "RISC Zero 3.1 vault on Sepolia (rule (B), our test network's zone): record
  `vault/deployments/sepolia-31-B.json`; 47 of 47 read-only checks on two public endpoints
  (`vault/deployments/sepolia-31-B.check.sh`); all five contracts exact_match on Sourcify (14:21Z, 23 Nov 2026)."
- **Web wallet deposit** (branch `testnet-wallet`, commit `35a4a619`; for whoever owns its rows). The bridge tab
  deposits ETH or TUSD to this vault. TUSD is an exact approval, then `deposit`. The page checks `configId` against the
  vault's `lezConfigId()`, and `lezReady: false` closes deposits.
  - `deploy/testnet/web/wallet/test/fork_bridge.py` runs it on an anvil fork of Sepolia against these addresses:
    **22 of 22** (`test/results/fork-31B-2026-11-23/record.json`).
  - Live at `/wallet/` since 14:29Z: 17 of 17 files equal to the commit, with deposits closed until the LEZ side is
    set up.
  - Not shown: a real MetaMask run, and a deposit on Sepolia. Both are the owner's (`OWNER-DEPOSIT.md` of this stream).
