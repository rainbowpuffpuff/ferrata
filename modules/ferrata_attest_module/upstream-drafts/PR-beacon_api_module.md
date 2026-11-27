# Pull request text for logos-co/logos-evm-beacon-api-module (not opened)

For the owner to open, against `main` (`d0ac27d` on 27 November 2026; the patch was made on `aca138e` and applies to
both). The patch is [`beacon_api_module-aca138e-verify-checkpoint.patch`](beacon_api_module-aca138e-verify-checkpoint.patch).

---

**Title:** feat: verify_checkpoint, an independent check of a weak-subjectivity checkpoint root

**Body:**

A light client that starts from a weak-subjectivity checkpoint is only as trustworthy as that checkpoint. This adds one
method that lets anyone check a checkpoint block root against several beacon API providers before relying on it, for
example an Ethereum light client deployed elsewhere (RFP-021 Functionality 9 asks that "the Logos Ethereum core module
exposes an API through which anyone can independently verify an instance's checkpoint against public Ethereum sources
before relying on it").

`verify_checkpoint(chain_id: i64, root: String, expect_json: String) -> String`

Every enabled provider of the chain is asked, pinned (no failover), and judged on its own answers:

- `/eth/v1/beacon/headers/{root}`: the root is a block it knows, canonical;
- `/eth/v1/beacon/headers/{slot}`: the canonical block at that block's own slot is the root;
- `/eth/v1/beacon/headers/finalized`: its finalized slot is at or after that slot;
- `/eth/v1/beacon/genesis` and `/eth/v1/config/spec`: the chain is the one expected (`genesis_validators_root` if
  given, `genesis_time` from the known network or `expect`, `DEPOSIT_CHAIN_ID` equal to the chain id unless `expect`
  says otherwise).

A provider agrees, disagrees or is unavailable (not counted). The verdict is `ok` when at least `min_providers`
(default 3, at least 1) agree on distinct hosts (host and port), none disagrees, and the agreeing ones put the root at
the same slot. A provider that knows the root but gives no slot for it disagrees: its slot and finality cannot be
checked.
`expect_json`: `{ genesis_validators_root?, genesis_time?, deposit_chain_id?, min_providers?, allow_plain_http? }`.
Reply: `{ ok, chainId, root, verdict, agreeing, needed, slot, providers: [{ id, endpoint, verdict, counted, slot,
finalized_slot, disagree, unavailable }] }`.

Plain-http providers are asked too, and a disagreement from one fails the check, but their agreement is not counted
unless `allow_plain_http` is set: as `finalized_header` takes its root over https only (docs/specs.md, "HTTPS-only
enforcement on `finalized_header`"), a checkpoint is a trust anchor and an on-path attacker could answer for a
plain-http provider. With the built-in providers alone the default quorum is not reachable (Sepolia has one https
provider, mainnet one); a caller adds providers of distinct operators with `add_provider` first.

What it does not do: it checks a root, not a light client's own state (a sync committee, a configuration); those are
the caller's, as they depend on the light client's format.

Changes: `rust-lib/src/checkpoint.rs` (new: the check), `src/glue.rs` (the method), `src/lib.rs` (the module and
`CheckpointExpect`), `src/beacon.rs` (`get_path`: a GET of a JSON document with 404 as `None`; `Plan::client` made
crate-visible), `src/tests.rs` (five tests on providers served on loopback, in the crate's test style, and an ignored live one: three
agreeing; another block at the slot, not finalized, another genesis, each one bad provider among three good ones; two
ids of one endpoint counted once and an unreachable provider not counted; plain-http agreement not counted by default;
a provider without the root's slot disagrees and `min_providers: 0` is taken as 1).

Tested: `cargo test --no-default-features` in `rust-lib/`, 34 passed (29 existing, 5 new) and one ignored,
`verify_checkpoint_live`, which asks the providers named in the environment (`BEACON_LIVE_PROVIDERS`, `BEACON_LIVE_ROOT`,
`BEACON_LIVE_GVR`); run on 27 November 2026 against three https Sepolia providers of three operators and the
plain-http Nimbus one: `ok`, 3 counted, Nimbus asked and not counted. Not tested here: the module builder's build of
the glue and a call through `logosctl`.
