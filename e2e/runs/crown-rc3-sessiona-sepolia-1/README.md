# crown-rc3-sessiona-sepolia-1: the live run's session A on LEZ v0.4.0-rc3, from the real Sepolia checkpoint (18 Nov 2026)

`e2e/crown.sh start --fork --inputs rc3 --lez-chain sepolia --halt-after identity crown-rc3-sessiona-sepolia-1` on our
Modal workspace (app `ferrata-crown`, `runner_rehearsal`), from branch `crown-rc3` at commit 12d0486e, input set `rc3`
(the bundle and programs of [`crown-rc3-rehearsal-1`](../crown-rc3-rehearsal-1/README.md)). Started 15:29:10Z, halted
after identity at 15:36:52Z as asked. The point: the one stage of the live run that the fork rehearsals run on a
stand-in. Here session A starts LEZ's Sepolia light client exactly as `crown-live-4` will, from the bundle's
checkpoint [`sepolia-2026-11-18`](../../../crates/ferrata-spectests/fixtures/sepolia-2026-11-18/SOURCE.md), on a new
LEZ v0.4.0-rc3 zone (rc3's sequencer, its Logos Blockchain node 0.3.5-rc.1, one validator, dev-mode leader proofs).
As `crown-sessiona-sepolia-1` did for crown-live-3 on rc1.

Session A, 398 s, 23 transactions applied (`lez/session-a/logs/driver.log`): the attestation program deployed (seven
segments and its header), the bridge program (six and its header), `Init` of the Sepolia instance
`0x0740a232…22b7` from the Gloas bootstrap at slot 11,599,872 (114,811 gas) and its two adopt parts (3,397,076 and
3,480,132), then the fixture's period-1416 update (Open 2,177,341, Miller 3,310,470, two adopt parts 3,408,111 and
3,415,303, Finish 3,696,140: adopted, not advanced, as that update finalizes the bootstrap's own header); T0 = block 33.
The program's weak-subjectivity bound accepted the checkpoint (the `Init` reverts outside it); the bundle's
`crown-driver check --chain sepolia` gives its end: 3 Dec 2026 09:05:36 UTC. `bedrockGenesisId` = the node's fork
digest `0xc025c6d2…58ac` (`lez/session-a/chain-fork.json`); node slot 0 at unix second 1,785,493,027, the sequencer's
blocks 4 s after an epoch second, one start (`lez/session-a/phase.txt`).

Identity on the fork (anvil at Sepolia block 12,162,339; nothing sent anywhere): the deployer at nonce 36, the vault
`0x80555Fe53CCcEf46951B23A3c75643a4C9283470` = CREATE(deployer, 38), configId `0xaafbcbf6…893c`, genesisState
`0x4c7f24b1…3bd0` (this zone's channel key; the live run's session A makes its own zone, so its genesisState will
differ, while its configId is this one as long as the deployer is at nonce 36: the configuration does not name the key).
The stages after identity (Sepolia's real captures and finality updates in session B) run only live.
