# crown-rc3-rehearsal-2: halted after the captures for a long pause, then stopped before session B (18 Nov 2026)

`e2e/crown.sh start --fork --inputs rc3 --halt-after capture2 crown-rc3-rehearsal-2` on our Modal workspace, from
branch `crown-rc3` at commit 799b4297, the same inputs as [`crown-rc3-rehearsal-1`](../crown-rc3-rehearsal-1/README.md).
The plan: stop after the captures, leave the zone's saved generation idle for longer than the live run's finality
waits (crown-live-3's deposits waited 14.7 and 25.2 min for their captures), then let `start` resume it, so that session B
restores an rc3 zone (rc3's sequencer, the 0.3.5-rc.1 node, 4-s slots) after such a pause, and proves its private
claim and burn with the prover threads `session.sh` now sets (commit 0ef6a47b).

What ran (`stages.done`, `crown.log`, `lez/session-a/`): a second rc3 zone's session A in 304 s (T0 = 23, the node's
fork digest `0xc025c6d2…58ac` as `bedrockGenesisId`, the sequencer's blocks 3 s after an epoch second, one start); the
identity on an anvil fork of Sepolia block 12,162,293 (vault `0x8055…3470` at the fork's nonce 36); the deployment,
both deposits and their fork captures; halted after `capture2` at 15:27:18Z as asked. Resumed by `start` at 16:12:57Z
(46 min after session A had saved its generation) and **stopped by me at 16:13:52Z, before session B began** (its
call cancelled alone, not with `crown.sh stop`, which cancels every runner): the live run `crown-live-4` had been
started from this tree in the meantime, and the app's LEZ session and GPU functions take one container each, so this
rehearsal's session B (35 min in rehearsal-1) could have held the live run's session B back. Its question is now
answered by the live run itself, whose session B restores its zone after the real finality waits. Not resumed; the
zone's generation 1 stays on the Volume (`lez/crown-rc3-rehearsal-2/`).
