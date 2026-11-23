# endpoints-2: the local scenario on the reviewed code (RFP-022 U.9), 23 November 2026

`U9_COMMIT=3fcd6220 e2e/endpoints/run.sh live 2` (no `U9_PUBLIC`: no public network), on the build box (a Modal CPU
container), after `e2e/endpoints/run.sh build` of the same tree; commit `3fcd6220`: the code with
the review's fixes (`open` drops another deployment's Ethereum and LEZ paths; a candidate Ethereum RPC must hold the block
the endpoint in use holds final; a candidate sequencer's channel must be the configuration's). The build's artefacts and
`logosctl`'s version are in [`versions.txt`](versions.txt); what the run is: [`e2e/endpoints/README.md`](../../endpoints/README.md).
Read [`summary.md`](summary.md) first.

**Result: the view's test passed, 3 of 3 run** (`qmltest.txt`: initTestCase, test_1_local, cleanupTestCase; test_2_live
skipped by design without `U9_PUBLIC`; 3.2 s), in one headless Logos Core (`logosctl` pre-release-66d098d) holding
`ferrata_bridge_module` (sha256 `6fe09b68…`) and `eth_rpc_module` (`6f9ab34d…`). Before it, on the same box: the
module's native tests and every method called through Logos Core against the native crate (`modules/scripts/check.sh`,
0 step and 0 check failures), and `seqnet-chain`'s tests (3 passed, debug).

The zone: four LEZ v0.4.0-rc1 sequencers on one Logos Blockchain node (heights 11/12/11/11 at the start), the bridge
program B1 at `0x8face982…e68d` (T0 = 30), its configuration `0xae4fc07b…887d` paired with the vault `0x0012962b…c0a5`
on anvil (chain 11155111) and Init, Register ETH and ImportRoot n=1 applied in blocks 35, 36, 37; a fork of that anvil on
port 8546. The chain service (`seqnet-chain`, the stand-in for Logos Core's LEZ module) holds both connections (the
plain path).

| # | Asked (Ethereum RPC; sequencer) | Answer | ms |
|---:|---|---|---:|
| 1 | the fork; sequencer 1 | switched both: the fork has anvil's final block (block 0, `0xd0330…fe5d5`); sequencer 1's zone holds the configuration, `ready`, channel `0x0101…01` | 160 |
| 2 | anvil; sequencer 2 | switched both (the same checks) | 160 |
| 3 | anvil; a closed sequencer port | refused (`getLastBlockId`: connect), both kept | 37 |
| 4 | a closed Ethereum port; sequencer 2 | refused (`eth_chainId`: connection refused), both kept | 49 |
| 5 | that closed port; sequencer 0 | refused: neither switched (sequencer 0 alone would have been accepted) | 49 |
| 6 | anvil; sequencer 0 | switched the sequencer | 130 |

After every step the chain service's own record was read back (`ops.jsonl`, 6 reads) and matched what the view showed.
The fork is accepted because it shares the history up to anvil's final block: the readings check an endpoint's
configuration, not its honesty. Nothing was sent to any network but the local anvil's (`net/`, `manifest.json`). The run
took 471 s.

Files as in `endpoints-1` (`summary.md`, `calls.jsonl`, `ops.jsonl`, `chain-local-calls.jsonl` with each switch's
checks, `qmltest.txt` and `.xml`, `screenshots/`, the zone's set-up, `logs/`), and `versions.txt`.
