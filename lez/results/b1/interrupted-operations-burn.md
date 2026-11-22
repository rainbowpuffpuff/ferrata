# A burn interrupted, and a burn whose note was never recorded (`harness/tests/interrupted_operations.rs`), RISC Zero dev mode

RFP-022 Reliability #7. Bridge program image `847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515`. The private claim of vault A's leaf 5 gives the account its 1 ETH holding; the burn is the wallet's 0th note for e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1 on Ethereum.

| stage | interrupted at | outcome |
|---|---|---|
| burn proven, dropped | between the proof and the sending | the holding unchanged (the committed account still the claimed one), no `burn(cid, r)` record: nothing consumed, nothing exposed |
| client restarts, burns again | — | accepted once: the note written once at `burn(cid, r)`, the holding gone exactly once |
| the note a third time | — | refused before the zone is touched: the private debit fails, the public `holder(account)` refuses 254 (INSUFFICIENT_BALANCE) |
| the dropped proof, sent late | after the retry landed | refused at LEZ's commitment/nullifier re-use check before anything runs; the zone exactly as after the accepted burn |
| burn sent, note never recorded | between the sending and the wallet's own record | a fresh client from the seed alone finds the note in the zone's blocks and at `burn(cid, r)`, and recovers the counter (0), the recipient, `sOut`, the burn leaf and `NOut` (bridge core `outbound::recover_burns`): everything a release needs except the burn root and path, which a published checkpoint gives; the entitlement is intact |

The dropped transaction: 7,857 bytes. The burn that landed: execution gas 515,479, storage gas 224,063.

NOut of the note: `df3ad7c7c13caac83d5f8b97cb9f0bfcde6b1db25339ede556b1c050b46955a9`.
