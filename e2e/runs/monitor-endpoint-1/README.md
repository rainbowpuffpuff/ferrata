# monitor-endpoint-1: the watchers' rehearsal with S12, the monitor's own endpoint failing (RFP-022 R.6)

`monitor/tests/anvil-e2e.sh` as CI runs it, on the Modal build box (`MONITOR_E2E_OUT` = this directory), branch
recovery-022 at commit `727e08a2` (S12 added in `db5adcb1`): 50 checks, 50 PASS ([summary.md](summary.md)). S12: the
monitor's RPC behind `e2e/netfault/holdproxy.py`; an outage while a wrong candidate is submitted (30 requests refused,
`poll-failed` alerted, the monitor still running); the endpoint back with the cancellation's answer lost (forwarded
and mined, the connection closed): one `CandidateCancelled`, one `eth_sendRawTransaction` forwarded, the candidate
`gone` in the monitor's state (it booked it from the chain, not as its own cancellation), a clean stop on SIGTERM.
Local anvil (chain id 31337); nothing sent to any public network. Paths of the build box are written `<repo>`.
