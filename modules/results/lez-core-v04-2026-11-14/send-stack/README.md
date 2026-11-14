# Logos Core's Ethereum send stack with logos-module-builder 5f74cdb's own locks (14 November 2026, Modal build box)

`modules/scripts/send-stack.sh build OUT` (each module at the revision `tx_sender_module`'s lock pins, its `flake.lock`
given builder 5f74cdb's own subtree by `modules/scripts/graft_builder_lock.py`, every other input as it locks it).
[`steps.tsv`](steps.tsv) keeps every attempt, in order:

| module | result | why |
|---|---|---|
| `eth_rpc_module` 6965f32 | **does not compile** | [`build-eth_rpc_module-errors.txt`](build-eth_rpc_module-errors.txt): `src/glue.rs` 212 and 247, `E0308`: the client builder 5f74cdb generates for its optional dependency `verified_proxy_module` returns `BTreeMap<String, Value>` from `status_with_timeout` and takes `&[Value]` in `rpc_with_timeout`; the glue was written for the older generator (with `--override-input` the old generator compiled it, then its install stopped at `--normalize-lidl`, logos-core-io run 2a) |
| `keystore_module` e83a899 | **built, installed, loaded** (second attempt; 129 s) | [`keystore-load.txt`](keystore-load.txt): `caller_identity` answers the host (`kind: host`) with the default roles (approvers `evm_signer_ui`, custodians `evm_keystore_ui`, managers `signer_manager_module`); `list_accounts` empty; `import_private_key` from logosctl `not authorized` (tier D is refused from the CLI by design) |
| `signer_manager_module` bd5c84d | **built, installed, loaded with `keystore_module`** (second attempt; 135 s) | [`signer-manager-load.txt`](signer-manager-load.txt): `dependencies_loaded: ["keystore_module"]`, the optional keycard signers skipped (not installed); `signers` lists the keystore (`logos.signer/1`); `accounts` empty |
| `fee_module`, `tx_sender_module` | not built | both require `eth_rpc_module`; their first attempts stopped on the builders' leftover `/homeless-shelter`, not retried |

The first attempt of the four (and the second of three) stopped at "home directory '/homeless-shelter' exists": a
derivation of the module's own build writes `$HOME`; `send-stack.sh` now removes it and retries, as logos-core-io's runs
did. [`run.log`](run.log), [`calls.jsonl`](calls.jsonl): the first `send-stack.sh run`, when nothing had built.
