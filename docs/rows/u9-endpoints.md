# Stream u9-endpoints: RFP-022 U.9, proposed status

For the integrator of `docs/requirements-status.md`, `docs/traceability.md`, `docs/EVIDENCE.md` and the README's status
lines. Branch `u9-endpoints` from `a0f38827`. Main's reason for U.9 ends "To be shown: `set_endpoints` through the
modules that hold the connections, and a test that switches between two live endpoints inside Logos Core"; this branch
builds that for the Ethereum RPC and, for the LEZ sequencer, through the chain service, which stands in for Logos Core's
LEZ module. Revised after review-u9 (two Medium, ten Low findings: all addressed, below). Nothing was pushed, so the two
new CI steps have no GitHub run; the same commands passed on the build box (a Modal CPU container, Linux x86-64). Every
commit is Claude Code (claude-opus-5-5).

## RFP-022

| ID | Proposed | Clause by clause | Evidence (path; command; where, commit) |
|---|---|---|---|
| U.9 | **in part** (unchanged; advanced) | *UI lets users change Ethereum RPC*: **shown.** Outside Logos the tool by `--eth-rpc` (row 85) and the view with its devnet backend (row 62); inside Logos Core the view's "Apply and test" (`Main.qml`, `settings.apply`) sends `wallet set_endpoints` and `ferrata_bridge_module` switches the RPC where the connection is held: Logos Core's own `eth_rpc_module` (its endpoint for the session's chain, on the session's `eth` path), live between two public Sepolia RPCs of the live deployment (publicnode → ethpandaops → publicnode, read only), or the chain service on the plain path (anvil → its fork → anvil). *And LEZ sequencer or zone*: **in part.** The tool by `--sequencer` (row 85); inside Logos Core the view switches the sequencer only at the chain service, `seqnet-chain`, which is a stand-in for Logos Core's LEZ module, not a Logos module (between sequencers 0, 1 and 2 of a zone of four, live); with the session's LEZ through Logos Core's `lez_core` a sequencer change is refused (switching `lez_core`'s own sequencer is not built). "Or zone": no list of configurations in the view; the configuration (and so its zone) is the one the host opens the session on. *Validated, old kept* (the project's addition): a new endpoint is used only after it answered for this deployment (Ethereum: the chain id, the vault's code and `lezConfigId`, and the block the endpoint in use holds final; LEZ: the configuration on its zone and the zone's channel), both kept otherwise; a check of the endpoint's configuration, not of its honesty: a fork sharing the history up to the endpoint in use's final block (the run's anvil fork) and an endpoint that answers all of it falsely are accepted | [`e2e/runs/endpoints-2/`](../../e2e/runs/endpoints-2/README.md) (the local scenario on this branch's code, commit `3fcd6220`: 6 calls, 3 switched, 3 refused; 3 of 3 tests run, the live one skipped by design); [`e2e/runs/endpoints-1/`](../../e2e/runs/endpoints-1/README.md) (both scenarios on the code before the review's fixes, `b4a72546`: 14 calls, 5 switched, 9 refused; 4 of 4 tests): `e2e/endpoints/run.sh build`, then `U9_COMMIT=… e2e/endpoints/run.sh live N` (scenario E: `U9_PUBLIC=1 U9_LIVE_LEZ_URL=<our test network's RPC, from the owner>`), build box; the view's test [`qmltest-logos/tst_endpoints_logos.qml`](../../modules/ferrata_bridge_ui/qmltest-logos/tst_endpoints_logos.qml) in a Qt Quick Test runner over one headless Logos Core (`logosctl` pre-release-66d098d) with `ferrata_bridge_module` and `eth_rpc_module` (`6965f32` with `modules/patches/eth_rpc_module-6965f32-generated-clients.patch`); natively [`calls/tests/endpoints.rs`](../../modules/ferrata_bridge_module/rust-lib/calls/tests/endpoints.rs) (CI `modules-resolve`; on the box 1 passed, debug, and in `modules/scripts/check.sh`, 0 step and 0 check failures); `cli/src/bin/seqnet_chain.rs`'s tests (CI `roundtrip`; on the box 3 passed, debug); code: `modules/ferrata_bridge_module/rust-lib/calls/src/session.rs` (`set_endpoints`, `open`), `cli/src/bin/seqnet_chain.rs` (`module.set_endpoints`, `module.endpoints`) |

### Proposed text

**`docs/requirements-status.md`, row U.9** (status `in part`; Evidence `62, 85, <new row>; cli/README.md;
e2e/endpoints/README.md`):

> The tool takes the Ethereum RPC and the LEZ sequencer by option or environment variable (tested: an RPC that does not
> answer, an RPC of another chain and an address with no vault each reported). Inside Logos Core the view's Apply and
> test switches the Ethereum RPC at Logos Core's `eth_rpc_module` (live, two public Sepolia RPCs, read only) or at the
> chain service, and the LEZ sequencer at the chain service (`seqnet-chain`, the stand-in for Logos Core's LEZ module;
> three sequencers of a zone of four), each only after the new one answered for the deployment, both kept otherwise
> (row <new>). The readings check the endpoint's configuration, not its honesty: a fork sharing the history up to the
> final block of the endpoint in use, or an endpoint that lies, is accepted. With LEZ through `lez_core` a sequencer
> change is refused (not built). To be shown: `lez_core`'s sequencer switch, run live in a headless Logos Core with the
> session's LEZ through `lez_core`.

**`docs/traceability.md`, row U.9**, "Shown by": replace "through `ferrata_bridge_module` inside Logos Core
`set_endpoints` is refused (`modules/ferrata_bridge_module/rust-lib/calls/src/session.rs:190`), so the endpoints are set
where the chain service starts" with "inside Logos Core the view's Apply and test switches the Ethereum RPC at
`eth_rpc_module` (two public Sepolia RPCs) or the chain service, and the LEZ sequencer at the chain service (the stand-in
for Logos Core's LEZ module; three sequencers of a zone of four), each only after the new one answered for the
deployment, both kept otherwise (row <new>; `e2e/runs/endpoints-1`, `endpoints-2`); with LEZ through `lez_core` a
sequencer change is refused"; "Named test": add `modules/ferrata_bridge_module/rust-lib/calls/tests/endpoints.rs`;
`cli/src/bin/seqnet_chain.rs`; `e2e/endpoints/run.sh`; `modules/ferrata_bridge_ui/qmltest-logos/tst_endpoints_logos.qml`.
Line 306 (the in-part list): replace U.9's sentence with "Inside Logos Core the view switches the LEZ sequencer only
through the chain service, the stand-in for Logos Core's LEZ module."

**Counts:** unchanged (RFP-022 41 shown, 24 in part; Usability 2 shown, 7 in part).

**`docs/EVIDENCE.md`, new row:**

> **The mini-app switches its endpoints inside Logos Core: the Ethereum RPC through `eth_rpc_module` and the chain
> service, the LEZ sequencer through the chain service (the stand-in for Logos Core's LEZ module) (RFP-022 U.9, in
> part).** A Qt Quick Test runner drives the view (`Main.qml`) over one headless Logos Core (`logosctl`
> pre-release-66d098d) holding `ferrata_bridge_module` and Logos's `eth_rpc_module` (`6965f32`, built from source with
> our compile patch `modules/patches/eth_rpc_module-6965f32-generated-clients.patch`); its "Apply and test" sends
> `wallet set_endpoints`. Local scenario (`e2e/runs/endpoints-2`, this branch's code): a zone of four live rc1 sequencers on
> one Logos Blockchain node with the bridge program B1 and its configuration, the vault on anvil and a fork of it; the
> chain service holds both connections: switched sequencer 0 → 1 → 2 → 0 and anvil → fork → anvil (the fork holding anvil's
> final block, each sequencer's zone the configuration and its channel); refused, both endpoints kept as the chain service
> reports them: a closed sequencer, a closed RPC, a good sequencer with that RPC (neither switched). Live scenario (`e2e/runs/endpoints-1`, before the review's fixes; read only): the Sepolia vault of row
> 232 on current main and its configuration on our public LEZ network, Ethereum through `eth_rpc_module`: its endpoint
> for chain 11155111 switched publicnode → ethpandaops (Sepolia block 12,196,898) → publicnode; refused Hoodi, a closed
> port and a local sequencer; the local scenario there also refused our public LEZ network (another zone), Sepolia's RPC
> (no vault) and Hoodi's (another chain). The readings check the configuration, not honesty: a fork sharing the history
> up to the final block of the endpoint in use (the anvil fork) and a lying endpoint are accepted. Natively (CI
> `modules-resolve`): `calls/tests/endpoints.rs`, adding a fork refused, the put-back after a failed reading through
> `eth_rpc_module`'s route, the refusals while a job runs and while `lez_core` holds LEZ, the endpoints in use given
> again, and a session opened on another chain's deployment dropping the old Ethereum path (no write to the old chain's
> record); `seqnet_chain.rs`'s configuration check (CI `roundtrip`). Where: build box, live rc1 sequencers, local EVM;
> in `endpoints-1` also Sepolia and our public LEZ network (read only). Proofs: none involved. Re-check:
> `e2e/endpoints/README.md` (scenario E needs our test network's RPC URL from the owner). Details:
> `e2e/runs/endpoints-2/README.md`, `e2e/runs/endpoints-1/README.md`.

**`scripts/ci/row-tests.toml`**: done on this branch (U.9: `calls/tests/endpoints.rs`, `ci = "modules-resolve"`;
`cli/src/bin/seqnet_chain.rs`, `ci = "roundtrip"`; `e2e/endpoints/run.sh`, `ci = "box"`, the local scenario's command);
`check-row-tests.py`: consistent, RFP-022 66 of 68 rows with a test CI runs (unchanged).

## review-u9, finding by finding

| Finding | What was done |
|---|---|
| M-1, a re-open on another chain switched the old chain's record of `eth_rpc_module` | `open` drops the Ethereum and LEZ paths when the chain service or its configuration differs (its reply names them, `paths_reset`), keeps them for the same deployment; `set_endpoints` refuses a path whose chain is not the session's; the reading after the switch checks the route's own chain. `calls/tests/endpoints.rs` holds the reviewer's reproduction (it failed before: the chain-560048 RPC was written into chain 11155111's record) and the same deployment opened again (paths kept); the job guard and the endpoint in use given again (the two mutations that passed) are cases now |
| M-2, status "shown" with the sequencer half through a stand-in | proposed **in part**; the text says the chain service is a stand-in and that the readings check configuration, not honesty |
| L-1, the zone's own identifier | `check_sequencer` compares `getChannelId` with the configuration's channel; the text says two zones on one channel id are told apart only by the configuration |
| L-2, forks and liars | the candidate must hold the block the endpoint in use holds final (module and chain service; skipped when the one in use does not answer); the text says what is still accepted |
| L-3, test gaps | the two cases above; seqnet-chain's tests in CI (`roundtrip`), its configuration check a pure function with a unit test |
| L-4, the row map's command | the local scenario's command; scenario E said apart |
| L-5, scenario E's URL | README and the EVIDENCE "Re-check": from the owner |
| L-6, the run's commit and `logosctl` | `endpoints-1`'s README says both are not in its records; `run.sh` writes `versions.txt`; `endpoints-2` was given its commit |
| L-7, `info` on the `eth` path | reports `eth_rpc_module`'s error, never the chain service's endpoint in its place; the comment corrected |
| L-8, our patch to `eth_rpc_module` | named in the EVIDENCE text and the README |
| L-9, two citations | the audit dropped; the live vault cited as row 232 of current main |
| L-10, the pre-rewrite commits | this branch's reflog expired (no `git gc` on the shared repository: other streams' objects) |

Not changed, reported by the review as found on main: P-1, row 62's devnet view test fails at `tst_miniapp.qml:283`
(`test_12_admin`) on main as on this branch.

**Files another stream may own, changed minimally:** `.github/workflows/ci.yml` (a step at the end of
`modules-resolve`, one in `roundtrip`), `scripts/ci/row-tests.toml` (the U.9 entry), `modules/ferrata_bridge_ui/README.md`
(the Endpoints line of the screens table), `cli/src/bin/seqnet_chain.rs` (two methods, `info` names its endpoints, its
configuration check).
