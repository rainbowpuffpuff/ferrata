# Shared by the zone scripts of this directory (zone.sh, zone-attest.sh, zone-bridge.sh, zone-bundle.sh,
# zone-v040.sh): what differs between the LEZ tags a zone runs, kept in one place and tested on its own
# (checkpoint/attestor/test_zone_lib.py). Sourced; defines functions only (prefix zl_), no side effects.
#
# The tags: v0.4.0-rc1 (the default of the scripts that predate this file), v0.4.0-rc3 (the test network's) and
# v0.4.0 (released 27 Nov 2026; the same git tree as v0.4.0-rc4, 3049ef30…). What a zone script needs to know of
# each:
# - its commit (as lez/scripts/lez-select.sh pins it) and the r0vm of its risc0-zkvm (3.0.5 before v0.4.0, 3.1.0 at it:
#   LEZ PR 1124);
# - the Logos Blockchain release its Bedrock configuration is written for: 0.3.5-rc.1 at rc3, 0.4.0-rc.2 at v0.4.0 (zone-sdk
#   v0.2.0, LEZ PR 1154), whose deployment settings use the `V2` era schema (`0: !V2`);
# - the channel parameter that bounds a full unstake: `exit_delay` (L2 blocks) before v0.4.0, `finality_depth` (L1
#   blocks under the removal's config; LEZ PR 1026) at v0.4.0;
#   (0.4.0-rc.2's node API also checks the Host header: IP addresses and `localhost` pass, nothing to configure);
# - the sequencer's log format: env_logger's `[2026-11-28T01:02:03Z INFO target] ...` before v0.4.0, tracing's
#   `2026-11-28T01:02:03.456789Z  INFO target: ...` at v0.4.0 (LEZ PR 1083).

# zl_lez_commit TAG: the commit of a LEZ tag (exit 1 for an unknown tag)
zl_lez_commit() {
  case "${1:-}" in
    v0.4.0-rc1) echo 1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21 ;;
    v0.4.0-rc2) echo e3edf47402a82557103ca719b30d0bd2684338d1 ;;
    v0.4.0-rc3) echo bafd07be0701398f9bc435f73ab05e9a7936c301 ;;
    v0.4.0) echo a255b42c0fc80d1cb219454a3a1f5ec3b5d92115 ;;
    *) return 1 ;;
  esac
}

# zl_known_tag TAG: exit 0 for a tag the zone scripts know
zl_known_tag() { zl_lez_commit "${1:-}" > /dev/null; }

# zl_is_v040 TAG: exit 0 for v0.4.0 (the release tag; no release candidate of it is a tag here)
zl_is_v040() { [ "${1:-}" = v0.4.0 ]; }

# zl_r0vm_version TAG: the r0vm of the tag's risc0-zkvm (Cargo.lock)
zl_r0vm_version() { if zl_is_v040 "${1:-}"; then echo 3.1.0; else echo 3.0.5; fi; }

# zl_bedrock_release TAG: the Logos Blockchain release the tag's bedrock/ configuration is written for
zl_bedrock_release() {
  case "${1:-}" in
    v0.4.0) echo 0.4.0-rc.2 ;;
    v0.4.0-rc3) echo 0.3.5-rc.1 ;;
    v0.4.0-rc1|v0.4.0-rc2) echo d480efc6 ;;
    *) return 1 ;;
  esac
}

# zl_era_schema FILE: the era schema of a deployment-settings.yaml (V1, V2; empty if none is found)
zl_era_schema() { sed -n -E 's/^[[:space:]]*0:[[:space:]]*!(V[0-9]+)([[:space:]]*(#.*)?)?$/\1/p' "$1" | sed -n 1p; }

# zl_tag_of_tree DIR: a LEZ tree's tag, from its directory name (upstream/lez-<tag>), or its lez-tag.txt (a bundle's
# lez-config/)
zl_tag_of_tree() {
  local d="${1%/}" n t
  if [ -f "$d/lez-tag.txt" ]; then cut -d' ' -f1 "$d/lez-tag.txt"; return; fi
  case "$(basename "$d")" in lez-*) n="$(basename "$d" | sed 's/^lez-//')" ;; *) return 1 ;; esac
  # a tree in a directory named for its commit (lez-<8 to 40 hex digits of it>, as spike/upstream/lez-a255b42c) is its tag's
  if [[ "$n" =~ ^[0-9a-f]{8,40}$ ]]; then
    for t in v0.4.0-rc1 v0.4.0-rc2 v0.4.0-rc3 v0.4.0; do
      case "$(zl_lez_commit "$t")" in "$n"*) echo "$t"; return 0 ;; esac
    done
  fi
  echo "$n"
}

# zl_find_r0vm VERSION HOME...: the r0vm binary of VERSION in the first rzup home that has it (rzup keeps it inside
# cargo-risczero's extension: extensions/v<version>-cargo-risczero-<target>/r0vm)
zl_find_r0vm() {
  local v="$1" h f; shift
  for h in "$@"; do
    for f in "$h"/extensions/v"$v"-cargo-risczero-*/r0vm; do [ -x "$f" ] && { echo "$f"; return 0; }; done
  done
  return 1
}

# zl_log_ts LINE: the UTC time (YYYY-MM-DDTHH:MM:SS) a sequencer log line starts with, in either format; empty if
# the line has none
zl_log_ts() {
  printf '%s\n' "$1" | sed -e 's/\x1b\[[0-9;]*m//g' | sed -n -E \
    -e 's/^\[([0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2})(\.[0-9]+)?Z .*/\1/p' \
    -e 's/^([0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2})(\.[0-9]+)?Z .*/\1/p' | head -n 1
}

# zl_first_marker_ts LOG MARKER PATTERN: the time of the first line matching PATTERN (grep -E) after the last line
# equal to MARKER (a launch's marker line), in either log format; empty if there is none yet
zl_first_marker_ts() {
  local line
  line="$(awk -v m="$2" '$0 == m { buf = ""; seen = 1; next } seen { buf = buf $0 "\n" } END { printf "%s", buf }' "$1" \
          | sed -e 's/\x1b\[[0-9;]*m//g' | grep -m1 -E "$3" || true)"
  [ -n "$line" ] && zl_log_ts "$line"
}

# zl_threshold N: the committee's threshold of LEZ's slashing and approvals: ceil(2n/3), at least 2 (LEZ PRs 818, 946)
zl_threshold() {
  local n="$1" t
  t=$(( (2 * n + 2) / 3 )); [ "$t" -lt 2 ] && t=2; echo "$t"
}

# zl_node_loopback FILE PORT: a Bedrock node-config.yaml (in place) on loopback: the API on 127.0.0.1:PORT and the swarm
# bound to 127.0.0.1. `allowed_hosts` (0.4.0-rc.2's allowlist of Host names) is left as upstream has it: IP addresses and
# `localhost` always pass, and an IP address written there is refused when the node starts (lb_http_api_common's
# HostName: "IP addresses are always allowed").
zl_node_loopback() {
  sed -i -e "s#listen_address: 0.0.0.0:18080#listen_address: 127.0.0.1:$2#" \
         -e 's#^    swarm:$#    swarm:\n      host: 127.0.0.1#' "$1"
}

# zl_seq_cfg SRC LOCAL DST API I NPEERS TAG: a sequencer's configuration from upstream's devnet one: the node's URL,
# gossip on loopback UDP 19100+I with the other NPEERS-1 as bootstrap peers, metrics on 19200+I; LOCAL (the tag's
# networks/local.json; `-` for none) adds its public test accounts to the genesis. TAG is checked against the genesis's
# channel parameters: `finality_depth` and no `exit_delay` at v0.4.0, the reverse before it.
zl_seq_cfg() {
  python3 -I - "$@" <<'EOF'
import json, sys
src, local, dst, api, i, npeers, tag = sys.argv[1:8]; i = int(i); npeers = int(npeers)
c = json.load(open(src))
params = (c.get("genesis") or {}).get("channel_params") or {}
if tag == "v0.4.0":
    assert "finality_depth" in params and "exit_delay" not in params, f"{src}: not a v0.4.0 genesis (finality_depth, no exit_delay): {sorted(params)}"
else:
    assert "exit_delay" in params and "finality_depth" not in params, f"{src}: not a genesis of LEZ {tag} (exit_delay, no finality_depth): {sorted(params)}"
if local != "-":
    l = json.load(open(local))
    have = {json.dumps(a, sort_keys=True) for a in c["genesis"]["actions"]}
    extra = [a for a in l["genesis"]["actions"] if json.dumps(a, sort_keys=True) not in have]
    assert all("supply_account" in a for a in extra), "only local.json's public test accounts are added"
    c["genesis"]["actions"] += extra
c["bedrock_config"]["node_url"] = api
c["gossip"] = {"listen_addr": f"/ip4/127.0.0.1/udp/{19100 + i}/quic-v1",
               "bootstrap_peers": [f"/ip4/127.0.0.1/udp/{19100 + j}/quic-v1" for j in range(npeers) if j != i]}
c["metrics_listen_address"] = f"127.0.0.1:{19200 + i}"
json.dump(c, open(dst, "w"), indent=2)
EOF
}

# zl_channel_summary: the channel JSON on stdin -> "keys tip_sequencer tip_slot" (- for a missing field; tip_sequencer is
# the turn's index in accredited_keys at 0.4.0-rc.2, so 0 is a value)
zl_channel_summary() {
  python3 -I -c '
import json, sys
try:
    c = json.load(sys.stdin)
except Exception:
    print("- - -"); sys.exit(0)
keys = c.get("accredited_keys")
dash = lambda v: "-" if v is None or v == "" else v
print(len(keys) if isinstance(keys, list) else "-", dash(c.get("tip_sequencer")), dash(c.get("tip_slot")))'
}

# ---- the wallet and the operator commands of LEZ v0.4.0 (attestor/zone-v040.sh) ---------------------------------------

# zl_published_secrets JUSTFILE: the public-account secrets LEZ publishes for its local test accounts (its Justfile's
# `account import public --secret HEX` lines; v0.4.0's secrets fund local.json's two public accounts), one per line
zl_published_secrets() { sed -n 's/.*account import public --secret \([0-9a-f]\{64\}\)\b.*/\1/p' "$1"; }

# zl_wallet_config SRC DST URL: LEZ's wallet configuration (lez/wallet/configs/debug/wallet_config.json) pointed at URL
zl_wallet_config() { sed -E "s#\"sequencer_addr\": *\"[^\"]*\"#\"sequencer_addr\": \"$3\"#" "$1" > "$2"; grep -q "\"sequencer_addr\": \"$3\"" "$2"; }

# zl_public_id: the first `Public/<base58 id>` on stdin (the wallet's and the operator commands' outputs name accounts so)
# (sed reads all: no SIGPIPE under pipefail; no match prints nothing and exits 0, so a caller's `x="$(… | zl_public_id)"`
# under `set -e -o pipefail` reaches its own check of an empty x instead of stopping the script without a word)
zl_public_id() { { grep -o -E 'Public/[1-9A-HJ-NP-Za-km-z]{32,44}' || true; } | sed -n 1p; }

# zl_tx_hash: a command's transaction hash from its output on stdin: the hash LEZ's wallet names (`Transaction hash is
# <hex>`, after which it prints the transaction's own data, full of other hashes), else the last 64-hex string (the
# operator commands end with `...: <hash>`); an optional 0x is dropped
zl_tx_hash() {
  python3 -I -c '
import re, sys
t = sys.stdin.read()
m = re.search(r"[Tt]ransaction hash is (?:0x)?([0-9a-f]{64})", t)
if m:
    print(m.group(1))
else:
    a = re.findall(r"(?:0x)?([0-9a-f]{64})", t)
    if a:
        print(a[-1])'
}

# zl_storage_sealed FILE: exit 0 if a wallet storage file is LEZ's sealed form (JSON with version, kdf, nonce and
# ciphertext; LEZ PR 987, already in rc3), not the plaintext storage
zl_storage_sealed() {
  python3 -I -c '
import json, sys
try:
    d = json.load(open(sys.argv[1]))
except Exception:
    sys.exit(1)
ok = isinstance(d, dict) and {"version", "kdf", "nonce", "ciphertext"} <= set(d) and isinstance(d["ciphertext"], str) and len(d["ciphertext"]) > 0
sys.exit(0 if ok else 1)' "$1"
}

# zl_no_founding_stakes SRC DST: a sequencer configuration without its genesis's founding stakes (`stake_sequencer`
# actions): the channel's creator then stakes its own key at genesis (LEZ v0.4.0 `setup`), and every other key joins
# by `sequencer_service stake`, whose ownership account is the staking wallet's (so its owner can vote)
zl_no_founding_stakes() {
  python3 -I - "$1" "$2" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
acts = c["genesis"]["actions"]
c["genesis"]["actions"] = [a for a in acts if "stake_sequencer" not in a]
assert len(c["genesis"]["actions"]) < len(acts), "the configuration has no founding stakes"
json.dump(c, open(sys.argv[2], "w"), indent=2)
PY
}

# zl_channel_param NAME: a channel parameter from the channel JSON on stdin (the node's /channel/<id>), wherever the
# node's release puts it (top level or under a params object); empty if absent
zl_channel_param() {
  python3 -I -c '
import json, sys
name = sys.argv[1]
try:
    c = json.load(sys.stdin)
except Exception:
    sys.exit(0)
def find(o):
    if isinstance(o, dict):
        if name in o and not isinstance(o[name], (dict, list)):
            return o[name]
        for v in o.values():
            r = find(v)
            if r is not None:
                return r
    return None
r = find(c)
print("" if r is None else r)' "$1"
}
