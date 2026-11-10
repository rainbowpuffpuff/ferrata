#!/usr/bin/env bash
# The end-to-end return leg, one step per subcommand (e2e/README.md says which step runs where and
# in which order). A run is a directory e2e/runs/<name>/ that starts with params.json.
#
#   run.sh params <run> <chain id> <deployer> <first nonce> [<lez-inputs.json>]
#                                                             addresses and amounts of a run
#   run.sh beacon <run> <block number> <prevrandao>           the Ethereum block whose beacon the
#                                                             finalisation echoes -> beacon.json
#   run.sh inputs <run>          deployment, wallet, zone, checkpoint inputs, release inputs; every
#                                guest is executed on its input (no proving); fixture.json so far
#
# lez-inputs.json names a real LEZ deployment and the accounts the deposits go to:
#   {"identity": {"bedrock_genesis_id", "channel_id", "bridge_header", "bridge_image_id",
#                 "attestation_root_id", "salt"},                  each 0x + 32 bytes
#    "deposits": [{"token": "eth" | "test-token", "amount": "<base units>",
#                  "kind": "private" | "public", "dest": "0x<32 bytes>"}, ...],     in leaf order
#    "source": {...}}                                               where the values came from
# params.json then holds lez_identity, these deposits and lez_inputs (source and the file's sha256);
# inputs writes lez_identity to <run>/lez-identity.json for the deployment's configId. Without it,
# the identity is a stand-in and the deposits go to made-up accounts.
#   run.sh prove-release <run>   succinct receipts of the release guest (CPU, r0vm)
#   run.sh stage-gpu <run>       work/gpu-inputs/: what gpu-prover/modal_app.py proves on a GPU
#   run.sh p254 <run> <proof>    first step of the Groth16 wrap -> work/<proof>/input.json
#   run.sh groth16 <run> <proof> second step, from work/<proof>/proof.json -> receipts/<proof>/
#   run.sh fixture <run>         fixture.json from whatever exists
#
# <proof> is cp-e2e-candidate, cp-e2e-finalize or release-<name>. The guest programs are read from
# $FERRATA_PROGRAMS (default e2e/programs/): release.bin, checkpoint.bin, segment.bin.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"
export TMPDIR="${TMPDIR:-$repo/../tmp}"          # tmp/ beside the repository, never /tmp
mkdir -p "$TMPDIR"
unset RISC0_DEV_MODE
step="${1:?step}"
run="$here/runs/${2:?run name}"
programs="${FERRATA_PROGRAMS:-$here/programs}"
mkdir -p "$run"

e2e() { (cd "$repo/bridge" && cargo run --quiet --release -p ferrata-e2e -- "$@"); }
lez() {
  (cd "$repo/checkpoint" && XDG_CACHE_HOME="${XDG_CACHE_HOME:-$repo/../tmp/xdg-cache}" \
    cargo +1.98.1 run --quiet -p fidelity --bin e2e -- "$@")
}
image_id() { e2e image-id "$programs/$1.bin" | jq -r .image_id; }
program_of() { case "$1" in release-*) echo release ;; *) echo checkpoint ;; esac; }
# An address nobody holds a key for: the last 20 bytes of keccak256(label).
fresh() { cast keccak "$1" | sed 's/^0x.\{24\}/0x/'; }

case "$step" in
params)
  chain="${3:?chain id}"; deployer="${4:?deployer}"; nonce="${5:?first nonce}"; lez_inputs="${6:-}"
  at() { cast compute-address "$deployer" --nonce $((nonce + $1)) | awk '{print $NF}'; }
  token="$(at 2)"
  params="$(jq -n --argjson chain "$chain" --arg deployer "$deployer" --argjson nonce "$nonce" \
    --arg groth16 "$(at 0)" --arg estop "$(at 1)" --arg token "$token" --arg vault "$(at 3)" \
    --arg eth_recipient "$(fresh 'ferrata e2e: ETH recipient')" \
    --arg erc20_recipient "$(fresh 'ferrata e2e: ERC-20 recipient')" \
    --arg relayer "$(fresh 'ferrata e2e: relayer')" \
    --arg eth 0x0000000000000000000000000000000000000000 '{
      what: "Inputs of one end-to-end run. The contracts are the deployer'"'"'s CREATE addresses at first_nonce + 0..3 (Groth16 verifier, emergency stop, test token, vault). Amounts are base units.",
      chain_id: $chain, deployer: $deployer, first_nonce: $nonce,
      groth16_verifier: $groth16, emergency_stop: $estop, token: $token, vault: $vault,
      depositor: $deployer,
      deposits: [{token: $eth, amount: "1000000000000000"}, {token: $token, amount: "100000000"}],
      burns: [
        {name: "eth", token: $eth, amount: "1000000000000000", recipient: $eth_recipient, relayer: $eth, relay_fee: "0"},
        {name: "erc20", token: $token, amount: "100000000", recipient: $erc20_recipient, relayer: $relayer, relay_fee: "250000"}
      ]}')"
  if [ -n "$lez_inputs" ]; then
    params="$(jq --slurpfile f "$lez_inputs" --arg sha256 "$(sha256sum "$lez_inputs" | cut -d' ' -f1)" --arg token "$token" \
      --arg eth 0x0000000000000000000000000000000000000000 '
      def hex32: type == "string" and test("^0x[0-9a-fA-F]{64}$");
      def check(ok; what): if ok then . else error("lez inputs: \(what)") end;
      ["bedrock_genesis_id", "channel_id", "bridge_header", "bridge_image_id", "attestation_root_id", "salt"] as $keys
      | $f[0] as $f
      | check(($f.identity | type) == "object" and ($f.identity | keys) == ($keys | sort)
              and all($keys[]; $f.identity[.] | hex32); "identity: exactly \($keys), each 0x + 32 bytes")
      | check(($f.deposits | type) == "array" and ($f.deposits | length) > 0; "deposits: a non-empty array")
      | .deposits = [$f.deposits[]
          | check((.token == "eth" or .token == "test-token") and (.amount | type == "string" and test("^[0-9]+$"))
                  and (.kind == "private" or .kind == "public") and (.dest | hex32);
                  "deposit \(.): token eth | test-token, amount in base units (a decimal string), kind private | public, dest 0x + 32 bytes")
          | {token: (if .token == "eth" then $eth else $token end), amount, kind, dest}]
      | .lez_identity = ($keys | map({(.): $f.identity[.]}) | add)
      | .lez_inputs = {source: $f.source, sha256: $sha256}' <<< "$params")"
  fi
  printf '%s\n' "$params" > "$run/params.json"
  cat "$run/params.json"
  ;;
beacon)
  number="${3:?block number}"; randao="${4:?prevrandao}"
  vault="$(jq -r .vault "$run/params.json")"
  nonce="$(cast keccak "$(cast abi-encode 'f(uint256,uint256,address)' "$randao" "$number" "$vault")")"
  jq -n --argjson number "$number" --arg randao "$randao" --arg nonce "$nonce" --arg vault "$vault" \
    '{blockNumber: $number, prevrandao: $randao, nonce: $nonce, vault: $vault}' > "$run/beacon.json"
  cat "$run/beacon.json"
  ;;
inputs)
  p="$run/params.json"
  identity_args=()
  if jq -e 'has("lez_identity")' "$p" > /dev/null; then
    jq .lez_identity "$p" > "$run/lez-identity.json"
    identity_args=(--identity "$run/lez-identity.json")
  fi
  lez deployment --chain-id "$(jq -r .chain_id "$p")" --vault "$(jq -r .vault "$p")" "${identity_args[@]}" \
    --out "$run/deployment.json"
  e2e notes --deployment "$run/deployment.json" --params "$p" --out "$run/wallet.json"
  echo_args=()
  [ -f "$run/beacon.json" ] && echo_args=(--echo-nonce "$(jq -r .nonce "$run/beacon.json")")
  lez chain --deployment "$run/deployment.json" --wallet "$run/wallet.json" "${echo_args[@]}" --out-dir "$run"
  e2e release --deployment "$run/deployment.json" --wallet "$run/wallet.json" --lez "$run/lez.json" --out-dir "$run"
  e2e image-id "$programs/release.bin" "$programs/checkpoint.bin" "$programs/segment.bin" \
    | jq -s 'map({(.program | split("/") | last | sub("\\.bin$"; "")): {image_id, bytes, sha256}}) | add' \
    > "$run/programs.json"
  : > "$run/executor.jsonl"
  for input in "$run"/cp-e2e-*.input "$run"/release-*.input; do
    name="$(basename "$input" .input)"
    e2e exec --program "$programs/$(program_of "$name").bin" --input "$input" --meta "$run/$name.json" \
      | jq -c --arg name "$name" '{name: $name} + .' | tee -a "$run/executor.jsonl"
  done
  "$0" fixture "$2"
  ;;
prove-release)
  for input in "$run"/release-*.input; do
    name="$(basename "$input" .input)"
    e2e prove --program "$programs/release.bin" --input "$input" --meta "$run/$name.json" \
      --out-dir "$run/receipts/$name"
  done
  ;;
stage-gpu)
  out="$run/work/gpu-inputs"
  rm -rf "$out"; mkdir -p "$out/programs" "$out/fixtures"
  cp -p "$programs/checkpoint.bin" "$programs/segment.bin" "$out/programs/"
  cp -p "$run"/cp-e2e-*.input "$run"/cp-e2e-*.json "$out/fixtures/"
  (cd "$out" && for f in programs/* fixtures/*; do
     jq -n --arg f "$f" --argjson bytes "$(stat -c %s "$f")" --arg sha "$(sha256sum "$f" | cut -d' ' -f1)" \
       '{($f): {bytes: $bytes, sha256: $sha}}'
   done | jq -s --arg commit "$(git -C "$repo" rev-parse HEAD)" --slurpfile ids "$run/programs.json" \
     '{source: "e2e/run.sh stage-gpu", commit: $commit,
       local_image_ids: {checkpoint: $ids[0].checkpoint.image_id, segment: $ids[0].segment.image_id},
       files: add}' > MANIFEST.json)
  ls -la "$out"/*
  ;;
p254)
  name="${3:?proof}"
  e2e p254 --receipt "$run/receipts/$name/receipt-succinct.bin" --image-id "$(image_id "$(program_of "$name")")" \
    --out-dir "$run/work/$name"
  ;;
groth16)
  name="${3:?proof}"
  e2e groth16 --receipt "$run/receipts/$name/receipt-succinct.bin" --image-id "$(image_id "$(program_of "$name")")" \
    --proof "$run/work/$name/proof.json" --out-dir "$run/receipts/$name"
  e2e verify --receipt "$run/receipts/$name/receipt-groth16.bin" --image-id "$(image_id "$(program_of "$name")")" \
    --meta "$run/$name.json"
  ;;
fixture)
  e2e fixture --run-dir "$run" --checkpoint-program "$programs/checkpoint.bin" \
    --release-program "$programs/release.bin" --out "$run/fixture.json"
  ;;
*)
  echo "unknown step $step" >&2; exit 2
  ;;
esac
