# Upstream drafts (not opened)

Changes drafted for Logos's own modules, so that what RFP-021 asks of "the Logos Ethereum core module" is an API of a
Logos module rather than only of ours. Nothing here has been sent upstream.

## `beacon_api_module.verify_checkpoint` (RFP-021 F.9)

[`beacon_api_module-aca138e-verify-checkpoint.patch`](beacon_api_module-aca138e-verify-checkpoint.patch), against
logos-co/logos-evm-beacon-api-module at `aca138e` (0.2.0, the module Logos Core loads for beacon API access).

F.9: "The Logos Ethereum core module exposes an API through which anyone can independently verify an instance's
checkpoint against public Ethereum sources before relying on it." We cannot change Logos's module, so the check ships in
`ferrata_attest_module.verify_checkpoint` (its beacon checks through `beacon_api_module.raw_get` when the request says
`"providers": {"via": "beacon_api_module", "chain_id": N}`, its execution check through `eth_rpc_module`); this patch is
the same check as a method of `beacon_api_module` itself:

- `verify_checkpoint(chain_id, root, expect_json) -> String`: every enabled provider of the chain, each pinned (no
  failover), asked whether `root` is a canonical block (`/eth/v1/beacon/headers/{root}`), the canonical block at its
  own slot (`/eth/v1/beacon/headers/{slot}`), at or below its finalized slot (`/eth/v1/beacon/headers/finalized`), on the
  expected chain (`/eth/v1/beacon/genesis`: `genesis_validators_root` and `genesis_time`; `/eth/v1/config/spec`:
  `DEPOSIT_CHAIN_ID`). A provider agrees, disagrees or is unavailable (not counted); the checkpoint passes when at least
  `min_providers` (default 3) agree on distinct hosts and none disagrees.
- `src/checkpoint.rs` (new, the check), `src/glue.rs` (the method), `src/lib.rs` (the module and `CheckpointExpect`),
  `src/beacon.rs` (`get_path`, a GET of a JSON document with 404 as `None`; `Plan::client` made crate-visible),
  `src/tests.rs` (three tests on providers served on loopback, in the crate's own test style).
- What it leaves to the caller: the instance's own parts (its configuration, the instance id, the bootstrap's committee
  branch and `KeyValidate`), which need the instance's wire format; `ferrata_attest_module.verify_checkpoint` does those.

Tested: `cargo test --no-default-features` in the module's `rust-lib/` with the patch (results below).

**Result** (Modal build box, 21 November 2026): the patch applies with `git apply` to a fresh export of `aca138e`;
`cargo test --no-default-features` in `rust-lib/`: **32 passed** (the crate's 29 and the 3 new ones:
`verify_checkpoint_three_providers_agree`; `verify_checkpoint_refuses_another_block_unfinalized_or_another_chain`, each
case one bad provider among three good ones, called `disagrees`, the checkpoint `failed`;
`verify_checkpoint_counts_hosts_and_skips_the_unavailable`: two ids for one endpoint count once, an unreachable provider
is `unavailable` and not counted). The glue method (`src/glue.rs`) is compiled only by the module builder (it includes
the generated scaffold), which this test does not run.
