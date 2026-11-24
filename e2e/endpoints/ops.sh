#!/usr/bin/env bash
# e2e/endpoints: the test's hands outside the view (tst_endpoints_logos.qml calls it through the runner's seqnet.ops):
# each endpoint's owner read directly, not through ferrata_bridge_module. One JSON line on stdout.
#
#   ops.sh ENV_FILE chain PORT        the chain service's module.endpoints (its Ethereum RPC and LEZ sequencer)
#   ops.sh ENV_FILE eth-rpc-module    Logos Core's eth_rpc_module: get_chain_config(CHAIN_ID)
set -uo pipefail
. "${1:?env file}"; shift
case "${1:-}" in
  chain)
    curl -fsS -m 15 -d '{"method":"module.endpoints","params":{}}' "http://127.0.0.1:${2:?port}/" | jq -c .result ;;
  eth-rpc-module)
    TMPDIR="$LOGOS_SHORT" "$LOGOSCTL" --config-dir "$S" --timeout 60000 call eth_rpc_module get_chain_config "$CHAIN_ID" --json < /dev/null \
      | jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else (.result // .) end' ;;
  *) echo "ops.sh: chain PORT | eth-rpc-module" >&2; exit 2 ;;
esac
