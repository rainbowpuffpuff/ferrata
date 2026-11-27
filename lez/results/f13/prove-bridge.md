# The bridge program of ADR 0046: a private claim and a private burn proven for real (27 November 2026)

`lez/harness`'s `prove_bridge` on the Modal build box (CPU prover, 32 vCPU), this tree's bridge program
`ferrata_bridge_f13` (`docs/image-ids.md`), with `FERRATA_EXPECT_BRIDGE_IMAGE_ID` set to its id and RISC Zero's dev
mode off. The command (from `lez/`, with LEZ's RISC Zero home):

    cargo build -p harness --locked --bin prove_bridge
    FERRATA_EXPECT_BRIDGE_IMAGE_ID=ffb47f6eaec6ef13f49505e5e80209f73d38ea53792e77656994f8069dde7675 \
      target/debug/prove_bridge --claim-out claim.borsh --burn-out burn.borsh

Its output (the paths of the two written transactions shortened; the transactions themselves are not committed):

```
bridge image ffb47f6eaec6ef13f49505e5e80209f73d38ea53792e77656994f8069dde7675 (602908 ELF bytes); RISC0_DEV_MODE off; shape wallet
private claim: vault A leaf 5 (1 ETH, private kind) under root n = 8, configuration 3dbef11e3f28; N = 1dbba1cacb2c3b88480a9f252a16cac0d96c2685524509f4b2165d1a6df068c2
private claim: REAL PROOF: execute_and_prove 301.9 s (prove + settle 302.4 s); receipt (succinct InnerReceipt, borsh) 239604 bytes; transaction 256082 bytes
private claim: wrote the proven transaction to claim.borsh (256082 bytes)
private claim: of its 256082 transaction bytes, none holds the destination, s, C, the leaf, the depositor, the witness or any of 32 path siblings; N, configId and the root read are present
private claim: settled: execution gas 556,718 (PRIVATE_VERIFY_GAS 409,764 + public part 146,954 cycles), storage gas 224,063, base fee 6,246,248; receipt at bytes 16,478..256,082 of 256,082
private burn: REAL PROOF: execute_and_prove 260.4 s (prove + settle 260.7 s); receipt (succinct InnerReceipt, borsh) 239046 bytes; transaction 254966 bytes
private burn: wrote the proven transaction to burn.borsh (254966 bytes)
private burn: one private-to-public delivery of the 185-byte Burn to the token's account, from an alias
private burn: settled: execution gas 515,680 (PRIVATE_VERIFY_GAS 409,764 + public part 105,916 cycles), storage gas 224,063, base fee 5,917,944; receipt at bytes 15,920..254,966 of 254,966
```

`/usr/bin/time -v`: 9 min 31 s wall clock for both proofs, peak resident memory 9.7 GB.

What it shows for ADR 0046: the private claim of the program whose claim names no fee schedule is proven for real by
LEZ's prover and settled (its 256,082 transaction bytes hold none of the private inputs), as is a private burn. What it
is not: the in-process harness's settlement (LEZ v0.4.0-rc1's state machine), not a sequencer; the proofs are not
verified by a vault (the claim is a LEZ-side transaction).
