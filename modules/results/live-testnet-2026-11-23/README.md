# `ferrata_updater_module` on our public LEZ test network, its Ethereum data through Logos Core (23 November 2026)

One headless Logos Core (`logosctl` 66d098d) ran `ferrata_updater_module` with `daemon.ledger = "sequencer"` against
our public test network (LEZ v0.4.0-rc3 anchored to its own Logos Blockchain node; `https://<domain>/rpc`), keeping a
Sepolia instance of our own following Sepolia, **both** of its light-client sources going through Logos Core's
`beacon_api_module`. Beside it in the same Core: `beacon_api_module`, `eth_rpc_module` (Sepolia's execution endpoint)
and `ferrata_attest_module`, whose calls read Ethereum through `eth_rpc_module`; `attest-monitor` watched the same
instance. Every process was sampled every 5 s and each daemon's traffic was counted by its own relay, for RFP-021 P.6.
The attestation CLI's journey and the day-one tool were measured in the same run, and RFP-022's two Ethereum-side
daemons ran as Logos modules on `bench/perf/daemons-anvil.sh`'s anvil fixture (RFP-022 P.6).

Transactions went to our test network only: the instance's `Init`, the updater's jobs and one attestation. Ethereum was
only read. The network's own instance (`c741a1f1…`, which the host's updater follows) was not touched.

⟨FIGURES⟩
