# endpoints-1: the mini-app switching its endpoints inside Logos Core (RFP-022 U.9), 23 November 2026

`e2e/endpoints/run.sh live 1` with `U9_PUBLIC=1`, on the build box (a Modal CPU container), with the code of commit
`b4a72546` of branch `u9-endpoints` (whose `run.sh` then held our public network's URL itself, now given as
`U9_LIVE_LEZ_URL`). The records do not name that commit (`run.log`'s first line reads "commit ;": the box's copy of
the worktree is not a git checkout, which `run.sh` now takes as `U9_COMMIT`), nor `logosctl`'s version
(pre-release-66d098d, from the build's own log on the box; later runs keep it in `versions.txt`). Its code predates
the review's fixes (`open` dropping another deployment's paths, the final-block and channel checks): see
[`endpoints-2`](../endpoints-2/README.md) for the local scenario on the fixed code; the records rewritten afterwards by the rules `run.sh`'s `records_final` now applies (this
machine's paths; our public LEZ network's host written `public-lez.example`, since that host names a person) and
checked by `scripts/scrub_records.py`. What the run is and how it is set up: [`e2e/endpoints/README.md`](../../endpoints/README.md).
Read [`summary.md`](summary.md) first.

**Result: the view's test passed, 4 of 4** (`qmltest.txt`: initTestCase, test_1_local, test_2_live, cleanupTestCase;
12.4 s), in one headless Logos Core (`logosctl` pre-release-66d098d) holding `ferrata_bridge_module` (dev `.lgx`,
sha256 `9e88b425…`) and `eth_rpc_module` (`6965f32` with its patch, sha256 `6f9ab34d…`), `artefacts-sha256.txt`.
**14 `set_endpoints`** from the view's "Apply and test": **5 switched, 9 refused**, each refusal keeping both endpoints,
read back from the owner itself after every step (`ops.jsonl`).

| Scenario | Session | Switched | Refused (endpoints kept) |
|---|---|---|---|
| L, local, the plain path | vault `0x0012962b…c0a5` on anvil (chain 11155111), configuration `0xae4fc07b…887d` on a zone of four LEZ v0.4.0-rc1 sequencers (bridge program B1 at `0x8face982…e68d`; Init, Register ETH and ImportRoot n=1 applied in blocks 40, 41, 42) | sequencer 0 → 1 with anvil → its fork (225 ms); → sequencer 2 with anvil (200 ms); → sequencer 0 (171 ms) | a closed sequencer port; a closed Ethereum port; a good sequencer with that closed port (neither switched); our public LEZ network ("holds no configuration 0xae4f… of the bridge program 0x8fac…", head 2331); Sepolia's public RPC ("no contract at the session's vault"); Hoodi's ("serves chain 560048") |
| E, live, read only; Ethereum through `eth_rpc_module` | the vault `0x19499061…2787` on Sepolia, configuration `0x3773b588…67cd` on our public LEZ network | `eth_rpc_module`'s endpoint for chain 11155111: publicnode → ethpandaops (1,805 ms; Sepolia block 12,196,898, LEZ head 2332) → publicnode (355 ms) | Hoodi's RPC ("serves chain 560048"); a closed port; a local sequencer ("its zone (head 44) holds no configuration 0x3773…") |

Nothing was sent to any network: the live session only read (no deposit, claim, burn or release was asked), and the
local vault's transactions went to the local anvil alone (`net/`, `manifest.json`). The whole run took 734 s on the box.

Files: `summary.md` (every call and the owners' records), `calls.jsonl` (the view's module calls through `logosctl`),
`ops.jsonl` (the owners read directly), `chain-local-calls.jsonl` and `chain-live-calls.jsonl` (the two chain services'
calls, their dry runs and switches), `eth-rpc-chain-start.json` and `-end.json` (`eth_rpc_module`'s record of chain
11155111), `qmltest.txt` and `.xml`, `screenshots/` (the local scenario; the five showing our public network's address in
a field are not kept), the zone's set-up (`session-a.json`, `identity.json`, `params.json`, `crown.json`, `deploy.json`,
`manifest.json`, `operator-steps.json`) and `logs/`.
