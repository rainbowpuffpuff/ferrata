# Ferrata's modules reading Ethereum through Logos Core's `eth_rpc_module` (13 November 2026)

Where it ran: the Modal build box (gVisor, x86-64, 64 GiB), workspace ws-5; Nix 2.28.3, cargo 1.93.0,
anvil 1.3.6, `logosctl` pre-release-66d098d built from logos-logoscore-cli `66d098d` ([`env.txt`](env.txt)).
Tree: branch `logos-core-io` at `6d31183` (run 3) and `b2946c4` (run 4, the bridge's end-to-end run after its
fix). RISC Zero dev mode (no proof in these runs). What the modules are and why they are built this way:
[`../../LOGOS-CORE-MODULES.md`](../../LOGOS-CORE-MODULES.md).

Re-check (Linux x86-64 with Nix, cargo, Foundry, r0vm; GitHub for the Logos modules' sources):

```sh
RELEASE_ELF=bridge/target/riscv-guest/.../release.bin LOGOS_TMPDIR=/var/tmp/lio modules/scripts/logos-io.sh modules/out/logos-io
```

## What ran (run 3: `scripts/logos-io.sh`, 892 s; [`steps.md`](steps.md), [`summary.md`](summary.md))

1. **Logos Core's modules built from their sources** with `logos-module-builder` overridden to our pin
   `5f74cdb` ([`logos-modules.tsv`](logos-modules.tsv); package hashes in [`lgx-sha256.txt`](lgx-sha256.txt)):
   `eth_rpc_module` `6965f32` with its two optional dependencies called by name
   ([`eth_rpc_module-by-name.diff`](eth_rpc_module-by-name.diff), `scripts/eth_rpc_by_name.py`): built (82 s);
   `beacon_api_module` `aca138e`, `fee_module` `f2065e7`, `signer_manager_module` `bd5c84d`, `tx_sender_module`
   `f1754c3`: built; **`keystore_module` `e83a899`: not built** (compiles and links, then its install stops at
   `logos-cpp-generator --normalize-lidl` on its interface contract: "Plugin file does not exist:
   --normalize-lidl"; unchanged `eth_rpc_module` stops at the same step on its optional dependencies, runs
   1 and 2a).
2. **`check.sh` for `ferrata_attest_module` and `ferrata_bridge_module`** (dev packages, native tests and
   calls, the calls in Logos Core): step failures 0, check failures 0 ([`check/summary.md`](check/summary.md)).
3. **One headless Logos Core** holding `eth_rpc_module`, `beacon_api_module`, `fee_module`,
   `signer_manager_module`, `tx_sender_module` (installed) and our two modules; loaded: `eth_rpc_module`,
   `beacon_api_module`, `ferrata_attest_module`, `ferrata_bridge_module` and Logos Core's own
   (`capability_module`, `modules_state`, `package_manager`, `package_downloader`, `storage_module`)
   ([`modules-loaded.json`](modules-loaded.json)). `tx_sender_module` does not load: `signer_manager_module`
   requires `keystore_module`, which did not build.

## Reads through `eth_rpc_module`, equal to the plain path (measured)

`e2e.sh` with `CALL=logosctl ETH_VIA=eth_rpc_module` ([`e2e/`](e2e/)): `eth_rpc_module`'s chain 1 set to the
recorded server of mainnet block 26,384,851 (`recorded_rpc.py`, the attestation fixture); the attestation
module reads Ethereum only through `eth_rpc_module.raw_rpc`, called from inside the module:

| Reply | Ethereum calls through `eth_rpc_module` (its route) | Equal to the same call reading over plain HTTP |
|---|---|---|
| `fetch_log_proof`, the USDC transfer (tx 0, log 0) | `eth_getBlockByNumber`, `eth_getBlockReceipts` (`direct`, `direct`) | yes |
| `fetch_log_proof`, the largest receipt's log (`block_log_index` 7) | the same two | yes |
| `dry_run_attest`, WETH's balance slot (proof read at the record's block) | `eth_getProof` (`direct`) | yes |
| `build_attest_tx`, the same slot (`storage.block`) | `eth_getProof` (`direct`) | yes |

The recorded server's log shows each of the three methods asked twice by `eth_rpc_module`'s client
(`Accept: */*`) and twice by the module's own plain client (`Accept: application/json`)
([`e2e/rpc.log`](e2e/rpc.log)). The whole end-to-end run then used those replies: the `Attest` messages
built from them executed by LEZ v0.4.0-rc1's state machine in the harness, the program's events read
back, the two refusals, `advance`: **43 checks, 0 failed** ([`e2e/e2e.md`](e2e/e2e.md)).

`bridge_e2e.sh` with `CALL=logosctl ETH_VIA=eth_rpc_module` (run 4, 21 s; a fresh Logos Core holding run 3's
`eth_rpc_module` and `ferrata_bridge_module`, [`bridge-e2e/`](bridge-e2e/)): after the module's whole flow
(root import, registry, private claims, the release rule, the release guest, releases on the vault),
a third deposit and the devnet's record of anvil's newest block (`lc.finalize`); then `import_root` under
that record three ways:

| Reply | How the vault's `eth_getProof` (slots 0 and 1) came | Verdict | Equal to the reply to the proofs given |
|---|---|---|---|
| `import_given` | given in the request (the devnet read it from anvil) | `importable`, n = 3 | (reference) |
| `import_read_plain` | read by the module over plain HTTP from anvil (`http`) | `importable`, n = 3 | yes |
| `import_read_eth` | read by the module through `eth_rpc_module` (`direct`), chain 1 = anvil | `importable`, n = 3 | yes |

The module took the vault's address from the bridge's configuration and the block from the request; the
dry run is the bridge and attestation programs' own checks. **24 checks, 0 failed** through Logos Core
([`bridge-e2e/bridge_e2e.md`](bridge-e2e/bridge_e2e.md)); natively the same run: 24/24, `import_read_plain`
equal ([`bridge-e2e/native-bridge_e2e.json`](bridge-e2e/native-bridge_e2e.json)). Run 3's bridge step had
stopped on a bug of the new section (it ran after the devnet's shutdown, curl exit 7), fixed in `b2946c4`.

## Not shown here (findings)

- **Verified reads**: `verified_proxy_module` serves mainnet, Sepolia and Hoodi only and has no
  `eth_getProof`; not built, and no public network read. Setting a chain's mode to `required` in
  `eth_rpc_module` sends these same reads through it unchanged.
- **Sends**: the send stack (`keystore_module` → `signer_manager_module` → `tx_sender_module`, with
  `fee_module` and `eth_rpc_module`) does not assemble with our builder pin (`keystore_module` above); the
  attempt is recorded in [`send-stack.log`](send-stack.log) (`tx_sender_module` not loaded). No Ferrata
  module signs or sends through Logos Core.
- **Beacon data and LEZ**: `beacon_api_module` built and loaded, not called by the updater module;
  `lez_core` not built (it pins LEZ v0.3.1; v0.4.0-rc1 is in its open pull requests #72 and #74).
