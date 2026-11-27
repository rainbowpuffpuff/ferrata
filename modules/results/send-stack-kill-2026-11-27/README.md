# The relayer and the monitor on Logos's real send stack, killed and cut off (RFP-022 R.6, 27 Nov 2026)

`modules/scripts/send-stack-kill.sh` on the Modal build box: `ferrata_relayer_module` and `ferrata_monitor_module` in
one headless Logos Core with Logos's own `tx_sender_module`, `signer_manager_module`, `keystore_module`, `fee_module`,
`eth_rpc_module` and its headless approver `evm_signer_cli`, built from their sources (`modules/scripts/send-stack.sh
build`, the revisions it pins; logosctl `66d098d`); Ferrata's two modules from this branch (`modules/scripts/check.sh`,
with the custody fix below). A local anvil (chain id 31337, nothing public), reached only through
`e2e/netfault/holdproxy.py`. `.lgx` sha256: `lgx-sha256.txt`.

    STACK=<send-stack.sh build OUT> FERRATA_LGX=<check.sh OUT> bash modules/scripts/send-stack-kill.sh OUT

This record is run 2, the script as of `ca4d071a` and the two Ferrata modules with `custody`'s fix of `2182e843`:
**39 of 40 checks** (`checks.tsv`, `summary.txt`); the one failure is the harness's own expectation, not the relayer:
K2 expected the recipient to receive the whole denomination, and the chain shows one payment of 0.0989 ETH, the
denomination less the relay and checkpoint fees (a second payment would show 0.1978). Every relayer and monitor line
per step is in `run/`, every SIGKILL's command line in `run/kills.log`, every module call in `calls.jsonl`; no key or
password. That each fault fired is in the run's own notes (`checks.tsv`, NOTE lines): K1 and M1 trapped at the proxy
with 7 module hosts killed, K2 with the relayer's host alone, K3 with its transaction in anvil's pool (`txpool 0x1`) and
the daemon and 13 hosts killed, K4's round ended on the endpoint's error, K5 with one late forward.

Checks added after this run, from the two review rounds (not in this record): every fault must fire and the hosts meant
must die as hard checks, K3's transaction broadcast and pending, K4's outage seen by the proxy, K5's late fault fired, a
round still running or unread at the end of its wait a failure, at most one reverted extra monitor call, nothing in
flight or queued for the relayer at the end, K2's expected payment corrected. Run 3, with most of them, ran on the box,
but its record could not be fetched: the box's spend ledger reached its cap first.

| Step | Fault | What happened (the relayer's or monitor's own lines) | Checked on the chain |
|---|---|---|---|
| K0 | none | a rule (B) checkpoint sent through `tx_sender_module` | one `submitCandidate` |
| K1 | `finalizeAttested`'s broadcast trapped before the node; every module host SIGKILLed (7) | after the restart the approver unlocked the two locked wallets and `custody` was ok; the relayer found its request forgotten by the restarted `tx_sender_module` (`inflight-request-forgotten`), judged it lost (`inflight-lost`) and sent it again at its nonce, 1 (`inflight-resent`), mined | one `finalizeAttested`, nothing in flight |
| K2 | a release's broadcast forwarded and its answer held 25 s; the relayer module's host alone SIGKILLed | after the restart, `inflight-found` (mined while held), the job booked, then the fees | one `release` (the recipient paid once: 0.0989 ETH, the denomination less the relay and checkpoint fees), one `claimOwed` |
| K3 | a checkpoint broadcast and pending (anvil not mining); the whole Logos Core SIGKILLed (the daemon and all 13 hosts) | a block mined; after the restart the round found it mined and booked it | two `submitCandidate` in all |
| K4 | the endpoint down for a whole round (every call 503 or cut) | the round ended on the endpoint's error, nothing sent; the next round finalised | two `finalizeAttested` in all |
| K5 | a checkpoint's broadcast answered 503 at once and delivered to the node 8 s later | the first round deferred it with its record kept; the next round found it mined (`inflight-found`), not sent again | three `submitCandidate` in all |
| M1 | the monitor's `cancelCandidate` trapped before the node; every module host SIGKILLed | after the restart the restarted `tx_sender_module` burnt the lost request's number (0), so its new cancel (1) waited behind a gap; the monitor filled the gap the plain way at 0 (`gapFill`), and the module's cancel then reverted (`UnknownCandidate`), changing nothing | one successful cancel, the candidate gone, at most one reverted extra call, the watcher's nonces 0–1 without a gap, nothing queued |

At the end: the relayer's 7 transactions for 7 effects, nonces 0–6 without a gap, every receipt successful, every job
in `done/`, the inbox empty, no in-flight record after K5, the vault's final state the second checkpoint's.

**What run 1 found (fixed before this run).** With every module host killed (K1), the relayer stayed blocked after the
restart, its record kept (nothing sent twice, nothing sent at all): `custody`'s re-import was answered "this key is
here already, in a locked wallet (…): unlock it in the Signer app first" (logos-evm-keystore-module `e83a899`), which
it did not take for a key held already; and after a restart of Logos Core nothing stays unlocked (signer manager
`bd5c84d`, `on_context_ready`), while only an approver may unlock a wallet (`unlock_wallet`: the Signer app, a person;
`evm_signer_cli` headless), never a custodian. `custody` (`cli/src/logos_send.rs`, `monitor/src/logos_send.rs`) now
answers `"verdict": "locked"` with that step and goes on once the wallet is unlocked; the harness plays the person at
the Signer app with `evm_signer_cli` after each restart. **On this route a daemon resumes after a restart of Logos
Core's signer stack only once its operator unlocks its wallet** (Logos's design). Run 2 passed every recovery; its one
failing check was the harness's own expectation (K2's payment, written as the whole denomination), corrected with the
harness's other hardening (every fault must fire, the hosts meant must die, a round must end) for run 3.
