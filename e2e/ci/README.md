# The round trip in CI

One deposit there and back, every step asserted, on one machine and with no secret: the CI job
`roundtrip` ([`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)) runs [`run.sh`](run.sh), which
builds what it needs and runs [`cli/tests/roundtrip_b.rs`](../../cli/tests/roundtrip_b.rs). How CI works as a
whole: [`docs/guides/ci.md`](../../docs/guides/ci.md).

Prototype evidence for a grant proposal. **Not audited. Not for production.**

## What runs

| Side | What | Real or stand-in |
|---|---|---|
| Ethereum | anvil (chain id 1, local); the v5 `FerrataVault` under rule (B), deployed by [`vault/script/CiRoundTrip.s.sol`](../../vault/script/CiRoundTrip.s.sol) | real contract; the verifier is the vault tests' `MockRiscZeroVerifier` behind RISC Zero's `RiscZeroVerifierEmergencyStop`: it accepts the (image id, journal digest) pairs the test registers, as the vault's own tests do |
| LEZ | LEZ v0.4.0-rc1's state transition with fee settlement, in the harness ([`lez/harness`](../../lez/harness)); the attestation and bridge programs executed as RISC Zero guests (`RISC0_DEV_MODE=1`) | LEZ's own code, not a sequencer process: transactions are settled one per block by the harness; private transactions are not used here |
| Attestation | the bridge program's `Init`, `Register`, `ImportRoot` and `ImportFees` read the vault's storage through the attestation program's records of an Ethereum block holding exactly the vault's storage as anvil reports it (`eth_getStorageAt`) | the storage is anvil's; the block and its header record are the harness's (no beacon chain on anvil; the light-client path from a sync committee is tested on Sepolia's data in `lez/harness/tests/sepolia.rs`) |
| Checkpoint | `ci_zone` ([`checkpoint/fidelity/src/bin/ci_zone.rs`](../../checkpoint/fidelity/src/bin/ci_zone.rs)) puts every transaction the harness settled into LEZ v0.4.0-rc1 blocks (one per transaction) after a genesis block, adds an empty `T`, inscribes each block with logos-blockchain-core d480efc6 by four test sequencer keys, and has three attestation keys, each certified by one sequencer key, attest `T` (rule (B), n = 4, t = 3). The rule (B) guest is executed on that input in dev mode | the formats, the transactions and the guest are real; the keys are test keys, and no Bedrock node or sequencer ran |
| Release | the release guest executed in dev mode on the wallet's input (`ferrata_cli::release::prove`) | real guest; dev-mode receipt |

## The steps and what each asserts

1. **Configuration.** `ci_zone genesis` gives the zone's `configId` and genesis checkpoint state for the
   vault's address; the `configId` must equal the bridge program's own (`BridgeConfig::id`). The vault is
   deployed with both, rule (B), and the image ids of the two guests just built; the test reads them back.
2. **`Init`, `Register`.** From the vault's slot 2, registry entry and fee schedule 0, read from anvil.
3. **Deposit.** 1 ETH from anvil's second account, to the test seed's public account `/0`, with the commitment
   the wallet derives; the vault's tree holds one leaf.
4. **`ImportRoot`, `ImportFees`, claim.** The root and count from the vault's slots 0 and 1; the wallet finds
   its deposit; the plaintext claim mints 1 ETH on LEZ; a second claim exits 250.
5. **Burn.** 0.1 ETH to a fresh Ethereum address; the holder is debited.
6. **Rule (B) checkpoint.** The guest's journal equals the native rule's, equals the vault's own journal layout
   (`VaultTestBase._checkpointJournalBytes`), starts at the vault's genesis state, carries the wallet's burn root
   (one burn) and the imported leaf count. The mock accepts that journal; `submitCandidate`; `finalizeAttested`
   before the wait W (600 s) is refused; after it, accepted: `finalState` moves and the vault's accepted burn
   root is the wallet's.
7. **Release.** The release guest's journal equals the native one; the mock accepts it; the recipient receives
   0.1 ETH; the nullifier is used; a replay is refused.

## Running it

```sh
scripts/fetch-upstream.sh          # LEZ v0.4.0-rc1 and Logos Blockchain d480efc6 beside the repository
scripts/ci/install-risc0.sh        # rzup: r0vm 3.0.5, guest Rust 1.88.0 (~/.risc0) and 1.91.1 (~/risc0-home-lez)
e2e/ci/run.sh                      # needs Rust 1.93 and 1.98.1, Foundry 1.3.6, libpcsclite-dev
```

`RISC0_HOME_LEZ` names the RISC Zero home with guest Rust 1.91.1 (default `../risc0-home-lez` beside the
repository, else `~/risc0-home-lez`); `FERRATA_E2E_DIR` the scratch directory (default `$RUNNER_TEMP` on GitHub,
else `../tmp/ci-roundtrip` beside the repository): `identity.json`, `txs.json`, `cp-ci-b.input`, `cp-ci-b.json`.

The image ids depend on where the guests are built (risc0-build embeds paths), so the test deploys the vault
with the ids of the guests it just built and checks that the executed guests are those.
