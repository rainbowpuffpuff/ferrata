# Real Sepolia state: the live vault `0x04eE8FF1…e44c` (crown-live-3) on LEZ v0.3.1's state machine

Written by `harness/tests/sepolia.rs` (`cargo test -p harness03 --test sepolia`). The vault's storage proofs as `eth_getProof` returned them at Sepolia's finalized blocks 12,119,428 and 12,119,519, with those blocks' own RLP headers (`e2e/runs/crown-live-3/captures/capture-{1,2}`); the light client's records (Gloas layout, the block hash) placed for finalized slots 11,563,328 and 11,563,424 (the light client's verification of those finality updates is the v0.4 run's). The deposits' openings are the crown wallet's (test seed): the vault's slot 0 equals the bridge core's tree of the two leaves at both blocks.

Natively, the program's check (`vault_facts`) of both captures gives (block, root, count) = (12,119,428, `67681306f0edccb80cbae3e74fa91a5d97d9df5130c82d0253a4c117a8444967`, 1) and (12,119,519, `a67ef5a18a4399b82912087c326e1d4ca21f076d349707b8a100e6c09ac97469`, 2), and the claim statement opens both deposits under the second root.

The vault pins the v0.4 configuration `10837afeaf6402e9d4bc2c4a13039e70f88762f13336d9c222d2f6d3b6a95553`; this program's configuration is `11d07e07f3c2a9b717fd9489ed9033077fb06f002824bdee43fa3f0468f29091`. So `Init` is refused (246, the pairing working); each import's plan verifies both storage proofs and is then refused by the configuration's guard (245); the claim is refused (245, no root imported).

| # | transaction | kind | result | execution gas | storage gas (bytes) |
|---:|---|---|---|---:|---:|
| 1 | Init: the live vault's slot 2 at block 12,119,428 | public | 246 `PAIRING_MISMATCH` (public) | 217175 | 5965 |
| 2 | ImportRoot n = 1 without the block's header | public | 215 `HEADER_REQUIRED` (public) | 47935 | 6028 |
| 3 | ImportRoot n = 1 naming another root | public | 247 `IMPORT_MISMATCH` (public) | 246170 | 6745 |
| 4 | ImportRoot n = 2 at block 12,119,428 (it holds 1) | public | 247 `IMPORT_MISMATCH` (public) | 246649 | 6745 |
| 5 | ImportRoot n = 1 at block 12,119,428 (proofs verified; no configuration) | public | 245 `NOT_FOUND` (public) | 285125 | 6745 |
| 6 | ImportRoot n = 2 at block 12,119,519 (proofs verified; no configuration) | public | 245 `NOT_FOUND` (public) | 285125 | 6745 |
| 7 | Claim of deposit 0 (public kind) under the root at two leaves | public | 245 `NOT_FOUND` (public) | 131337 | 1936 |
