# The return leg on Sepolia, second run

**Prepared; no transaction has been sent.** It replaces [`../sepolia-live/`](../sepolia-live/README.md), whose
first transaction ran out of gas on 10 November 2026 (Foundry does not price Sepolia's Glamsterdam state gas;
`vault/GAS.md`, "On Sepolia since Glamsterdam"). Every gas limit of this run is Sepolia's own estimate times 1.25
(`vault/README.md`, "Sepolia round trip").

**The deployment** (chain id 11155111; the sender's plain CREATE addresses, from its nonce 1):

| Nonce | Transaction | Address |
|---|---|---|
| 1 | create RISC Zero's `RiscZeroGroth16Verifier` (risc0-ethereum v3.0.0) | `0x8f0Af3Bf302C64d57f840127F94d9c123a8Fa2A0` |
| 2 | create its `RiscZeroVerifierEmergencyStop` | `0xe4830261A1D61Df5AF1be5579a3e3DB738331139` |
| 3 | create the test token | `0xdfedf9A73Cfb337277b95a9a1C941c3496A4385F` |
| 4 | create the `FerrataVault` | `0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7` |
| 5, 6 | register ETH (0.001, 0.01, 0.1) and the test token | |
| 7, 8 | mint and approve 100 test tokens | |
| 9 | deposit 0: 0.001 ETH, public kind, to LEZ's public test account 0; then, once its block is finalized, `lez-capture-1/` | |
| 10 | deposit 1: 0.001 ETH, private kind, to LEZ's private test account 0; then `lez-capture-2/` | |
| 11 | deposit 2: 100 test tokens (leaf 2; it funds the ERC-20 release, and no root LEZ attests holds it) | |
| 12 | `submitCandidate` with the candidate's real seal | |
| 13 | `recordBeacon`, after the wait W (40 min) | |
| 14 | `finalize` with the seal proven for that beacon during the run | |
| 15, 16 | the two releases (ETH to a fresh recipient; 100 test tokens with a relayer's fee) | |

The sender is `0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07`; a transaction of it from anywhere else before the run
moves these addresses and voids this directory.

**Paired with a real LEZ deployment.** The vault's `lezConfigId`,
`0xa2e880cb8da77b08dad9833be7a4c9aa08dd856f4648da5486937c798611d3b8`, is the configuration id the LEZ bridge
program computes for this vault (its header account, its image id, the attestation root id and the salt of its
terms: ETH in 0.001, 0.01 and 0.1). Those values come from the inbound agent's plan (`sepolia-plan.json`, written
10 November 2026 20:54 UTC; its sha256 and the fields used are in `lez-inputs.json` and in `params.json`
`lez_inputs`); the id was computed by the plan's own command and by the fixture builder, equal. The two ETH
deposits go to LEZ's published test accounts (`testnet_initial_state`), so LEZ can claim them: deposit 0
publicly, deposit 1 privately; their commitments equal those of the plan's command. The stand-in zone of the
return leg (`deployment.json`, `lez.json`) uses the same configuration identity.

**The proofs** (10 November 2026): the candidate on one NVIDIA L40S (7.7 s, the cloud app's `gpu_prove`), both
releases on the build box, each wrapped (`identity_p254` on the build box, RISC Zero's prover image) and its
Groth16 receipt verified against its image id with the native journal (`receipts/<proof>/`). The finalisation is
proven during the run, for the beacon it records.

| File | What |
|---|---|
| `lez-inputs.json` | what the inbound plan gave: the six identity fields, the deposits' kinds and destinations, the source file and its sha256 |
| `params.json` | addresses, deposits and burns of the run (`e2e/run.sh params sepolia-live-2 11155111 <sender> 1 lez-inputs.json`) |
| `lez-identity.json`, `deployment.json`, `wallet.json`, `lez.json` | the configuration identity; the stand-in zone's deployment; the wallet (deposit secrets, commitments, burn notes); the zone |
| `cp-e2e-candidate.*`, `release-*.*`, `receipts/` | the guests' inputs, their native journals, the receipts and seals |
| `fixture.json` | what `vault/script/RoundTrip.s.sol` reads |
| then, as the run goes | `manifest.json` (every transaction with its gas limit and receipt), `gas/<stage>.json` (each stage priced by the node before it was sent), `lez-capture-1/`, `lez-capture-2/` (for the LEZ session), `beacon.json`, `stages.done`, `sepolia-live.log` |
