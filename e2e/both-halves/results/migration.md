# Migration rehearsal over both real halves (`cli/tests/both_halves.rs`, RFP-022 Sec.8, R.4, Sec.9)

- old vault A 0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9 (configId 0xf64a4aa5f91d5bce93c1dd761cafe797bfdd0730451217af68a9a7c8f254bf01), new vault B 0x8438Ad1C834623CfF278AB6829a248E37C2D7E3f (configId 0xb1de1d572953dccf47a3cd64e5619c9dfe22cd94dbc0b6c61ae93e1f9d1dec92)
- A: deposits d0 (claimed), d1 (imported, unclaimed), d2, d3, d4 (not imported); burn 0 of 0.1 ETH
- retire() by A's freeze authority; a deposit on A refused; AttestRetired and the keyless Sunset applied; ImportRoot after it refused (242); A's imported count stays 2
- after Sunset: d1 claimed on LEZ; burn 1 of 0.1 ETH
- sunset checkpoint finalised (burns 2, imported 2); burn 0 released on the retired vault
- after the window: d2 and d3 refunded to their depositor (the whole escrow); refunds of d0 and d1 refused; second refunds refused
- B: deposit d0 imported and claimed under B's configuration; burn 0 under B
- Sec.9: A's emergency stop tripped: the proven release of burn 1 and the refund of d4 refused; A's escrow 1101200000000000000 wei stays (burn 1 unreleased, d4 unrefunded; the books still balance); B goes on (d1 claimed)

Invariant checks: A 6 times, B 2 times (vault balance == escrowed + owed; supply == minted - burnt; supply + unminted + unreleased == escrowed). Forgeries refused: 9. 23 s in all.
