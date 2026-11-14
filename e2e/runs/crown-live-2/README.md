# crown-live-2: stopped at its first stage, nothing sent (12 Nov 2026)

The owner's `e2e/crown.sh start crown-live-2` on our Modal workspace, from the tree `spike/ferrata-wt-integ7`,
branch `integration7`, commit fff83c9 (the final programs of [`docs/image-ids.md`](../../../docs/image-ids.md);
`programs/ids.jsonl` is byte for byte `crown-live-3`'s). One runner, `fc-01M77G3GHWYAE1M7T2KCSH7ARY`, started
09:06:03Z; it stopped itself at 09:19:05Z in stage `lez-a`, LEZ session A, before any other stage (`state.json`: done
nothing).

**Why it stopped.** Session A ran three times (the driver retries a failed session); each time the programs deployed
and `crown sepolia Init` **reverted on the LEZ devnet's sequencer with "Execution outside of the validity window"**
(`crown.log`: blocks 17 of three fresh devnets, transactions `bf4cbd31…`, `9afb3d01…`, `93fb10db…`). The final
programs bound the creation of a light-client instance by the weak-subjectivity period of the instance's finalized
header, by the sequencer's clock (3,532 epochs, about 15.7 days; [`crates/ferrata-core/src/ws.rs`](../../../crates/ferrata-core/src/ws.rs)).
The bundle's driver started from the period-1378 bootstrap (slot 11,288,576, 5 Oct 2026 10:35 UTC), 38 days old that
morning and out of its period since about 21 Oct: the bound refused it, as designed, on a real sequencer. The
rehearsals could not show it (they use a stand-in beacon chain on LEZ) and crown-live-1's programs had no such bound.
The instance this run would have created, `0x88df8428…c358`, never existed.

**Nothing was sent to Sepolia**: no stage after `lez-a` ran; the deployer's nonce stayed 23 (where crown-live-1 left it;
crown-live-3's identity read 23 at 14:08:38Z and its first transaction used it).

**What followed**: not a longer period but a fresh checkpoint,
[`sepolia-2026-11-12`](../../../crates/ferrata-spectests/fixtures/sepolia-2026-11-12/SOURCE.md) (period 1411; an `Init`
from it is valid until 28 Nov 2026 03:13 UTC), read by the driver from the bundle's fixture, session A rehearsed on it
([`../crown-sessiona-sepolia-1/`](../crown-sessiona-sepolia-1/README.md)), and a new run name because a resumed
`crown-live-2` would have kept its copy of the old bundle: [`../crown-live-3/`](../crown-live-3/README.md), DONE on the
same programs.

Files: `run.json`, `state.json`, `bundle.json` (the old bundle, sha256 `4dd813e2…3530`), `programs/ids.jsonl`, and the
runner's `crown.log` (no local path in it). The session's own logs and the runner's are on the Modal Volume
(`e2e/crown.sh fetch --lez crown-live-2`).
