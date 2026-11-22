# A claim interrupted between its proof and its sending (`harness/tests/interrupted_operations.rs`), RISC Zero dev mode

RFP-022 Reliability #7. Bridge program image `ac1fb59b0188a355523580f160cf929965740928b18b3394344cd5229483442e`. Vault A's leaf 5 (1 ETH, private kind), claimed as the private account it names.

| stage | interrupted at | outcome |
|---|---|---|
| seed recovery | — | the fresh client finds all three of its deposits of vault A; leaf 5: `s`, kind, destination, leaf and N, all equal to the world's own (bridge core `inbound::recover`) |
| claim proven, dropped | between the proof and the sending | N unspent, no holder state, no fee minted, root/token/configuration untouched, the client's committed account exactly what it was: nothing consumed, nothing exposed |
| fresh client claims | — | accepted once: N spent with exactly (A, ETH, 8 leaves, 1 ETH), the holding private, the relay fee public |
| second claim | — | refused with the double-claim code 250 in the proving error, as `bridge_private.rs`'s replay |
| the dropped transaction, sent late | after the fresh client's claim landed | refused at LEZ's commitment/nullifier re-use check before anything runs; every state exactly as after the accepted claim: an old proof cannot double-spend or corrupt |

The dropped transaction: 8,937 bytes. The claim that landed: execution gas 556,901, storage gas 224,063, base fee 6,247,712.
