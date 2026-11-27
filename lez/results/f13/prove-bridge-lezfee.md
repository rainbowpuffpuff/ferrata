# The bridge program of ADR 0048: a private claim and a private burn proven for real (27 November 2026)

`lez/harness`'s `prove_bridge` on the Modal build box (CPU prover), this tree's bridge program `ferrata_bridge_lezfee`
(`docs/image-ids.md`: claims name no fee schedule, `CollectFees` mints the protocol fees on LEZ), with
`FERRATA_EXPECT_BRIDGE_IMAGE_ID` set to its id and RISC Zero's dev mode off. The command (from `lez/`, with LEZ's RISC
Zero home), as for ADR 0046's program (`prove-bridge.md` beside this file):

    cargo build -p harness --locked --bin prove_bridge
    FERRATA_EXPECT_BRIDGE_IMAGE_ID=9d02d54d56b026079e94c8d9b12db69f3a96b7acccd11283f68e5bc343bfa321 \
      target/debug/prove_bridge --claim-out claim.borsh --burn-out burn.borsh

Its output (the paths of the two written transactions shortened; the transactions themselves are not committed):

```
bridge image 9d02d54d56b026079e94c8d9b12db69f3a96b7acccd11283f68e5bc343bfa321 (624204 ELF bytes); RISC0_DEV_MODE off; shape wallet
private claim: vault A leaf 5 (1 ETH, private kind) under root n = 8, configuration b7e61c0d101a; N = 1dbba1cacb2c3b88480a9f252a16cac0d96c2685524509f4b2165d1a6df068c2
private claim: REAL PROOF: execute_and_prove 298.2 s (prove + settle 298.7 s); receipt (succinct InnerReceipt, borsh) 239604 bytes; transaction 256082 bytes
private claim: wrote the proven transaction to claim.borsh (256082 bytes)
private claim: of its 256082 transaction bytes, none holds the destination, s, C, the leaf, the depositor, the witness or any of 32 path siblings; N, configId and the root read are present
private claim: settled: execution gas 556,655 (PRIVATE_VERIFY_GAS 409,764 + public part 146,891 cycles), storage gas 224,063, base fee 6,245,744; receipt at bytes 16,478..256,082 of 256,082
private burn: REAL PROOF: execute_and_prove 252.1 s (prove + settle 252.4 s); receipt (succinct InnerReceipt, borsh) 239046 bytes; transaction 254966 bytes
private burn: wrote the proven transaction to burn.borsh (254966 bytes)
private burn: one private-to-public delivery of the 185-byte Burn to the token's account, from an alias
private burn: settled: execution gas 515,654 (PRIVATE_VERIFY_GAS 409,764 + public part 105,890 cycles), storage gas 224,063, base fee 5,917,736; receipt at bytes 15,920..254,966 of 254,966
```

`/usr/bin/time -v`: 9 min 19 s wall clock for both proofs, peak resident memory 9.7 GB.

What it shows for ADR 0048: the private claim of the program whose protocol fees are minted on LEZ by `CollectFees` is
proven for real by LEZ's prover and settled (its 256,082 transaction bytes hold none of the private inputs, and it
carries no fee value: its public part, the `Issue` to the token's account, names the nullifier, the token, the
denomination and the root read, and the relay fee it pays is the registry's fixed one), as is a private burn (one
185-byte `Burn` delivered, one denomination, no fee). The transactions have the same sizes as under ADR 0046's program
(`prove-bridge.md`): a claim's and a burn's encodings did not change. What it is not: the in-process harness's
settlement (LEZ v0.4.0-rc1's state machine), not a sequencer; `CollectFees` itself is a public transaction, which no
proof is made for (it runs in the harness, `lez/harness/tests/bridge_f13.rs`, and on a LEZ v0.4.0-rc3 sequencer,
`e2e/runs/f13-lezfee-1/`); the proofs are not verified by a vault (the claim is a LEZ-side transaction).
