# The LEZ version used by the testnet scripts. Source this file; FERRATA_LEZ_TAG selects v0.4.0-rc3
# (the default, the running network) or v0.4.0. FERRATA_LEZ_SRC overrides the source tree for tests.
#
# On a host install.sh moved to v0.4.0, /etc/ferrata/lez-tag records the tag (FERRATA_LEZ_TAG_FILE names another file,
# for tests): with FERRATA_LEZ_TAG unset or empty the host's record is used, so a command run by hand there without the
# variable does not take rc3's settings, and a FERRATA_LEZ_TAG that disagrees with the record is refused.
LEZ_TAG_FILE="${FERRATA_LEZ_TAG_FILE:-/etc/ferrata/lez-tag}"
LEZ_HOST_TAG=""
if [ -e "$LEZ_TAG_FILE" ]; then   # a record that exists must name a tag: an empty or damaged one is refused, not taken for rc3
  LEZ_HOST_TAG="$(tr -d '[:space:]' < "$LEZ_TAG_FILE" 2>/dev/null)" || LEZ_HOST_TAG=""
  case "$LEZ_HOST_TAG" in
    v0.4.0-rc3|v0.4.0) ;;
    *) echo "lez-tag.sh: this host's tag record $LEZ_TAG_FILE holds '$LEZ_HOST_TAG', not v0.4.0-rc3 or v0.4.0" >&2; exit 2 ;;
  esac
fi
if [ -n "${FERRATA_LEZ_TAG:-}" ] && [ -n "$LEZ_HOST_TAG" ] && [ "$FERRATA_LEZ_TAG" != "$LEZ_HOST_TAG" ]; then
  echo "lez-tag.sh: FERRATA_LEZ_TAG=$FERRATA_LEZ_TAG, but this host runs LEZ $LEZ_HOST_TAG ($LEZ_TAG_FILE)" >&2
  exit 2
fi
LEZ_TAG="${FERRATA_LEZ_TAG:-${LEZ_HOST_TAG:-v0.4.0-rc3}}"
case "$LEZ_TAG" in
  v0.4.0-rc3)
    LEZ_COMMIT=bafd07be0701398f9bc435f73ab05e9a7936c301
    R0VM_VERSION=3.0.5
    BEDROCK_RELEASE=0.3.5-rc.1
    WALLET_IMPORT_FLAG=--private-key
    ;;
  v0.4.0)
    LEZ_COMMIT=a255b42c0fc80d1cb219454a3a1f5ec3b5d92115
    R0VM_VERSION=3.1.0
    BEDROCK_RELEASE=0.4.0-rc.2
    WALLET_IMPORT_FLAG=--secret
    ;;
  *)
    echo "lez-tag.sh: FERRATA_LEZ_TAG must be v0.4.0-rc3 or v0.4.0 (got '$LEZ_TAG')" >&2
    exit 2
    ;;
esac
LEZ_SRC="${FERRATA_LEZ_SRC:-/opt/ferrata/src/upstream/lez-$LEZ_TAG}"

# Bind the node API to 127.0.0.1:PORT and the swarm to loopback, in place. In particular, leave
# api.backend.allowed_hosts untouched: IP addresses and localhost are allowed without being listed, and
# the Logos Blockchain 0.4.0-rc.2 node refuses an IP address in that list at start-up.
lez_node_config() {
  local file="$1" port="$2"
  sed -i -e "s#listen_address: 0.0.0.0:$port#listen_address: 127.0.0.1:$port#" \
         -e 's#^    swarm:$#    swarm:\n      host: 127.0.0.1#' "$file"
}

# Refuse a Bedrock deployment-settings.yaml written for the other node release (or no known era schema).
lez_check_deployment() {
  local file="$1" expected=V2
  [ "$LEZ_TAG" = v0.4.0 ] || expected=V1
  if ! grep -qE "^[[:space:]]*0:[[:space:]]*!$expected([[:space:]]*(#.*)?)?\$" "$file"; then
    echo "lez-tag.sh: $file is not a $LEZ_TAG deployment-settings.yaml (expected 0: !$expected)" >&2
    return 1
  fi
}

# A v0.4.0 public genesis key is a 33-byte typed initial key: 00 (Schnorr), then a 32-byte commitment.
lez_genesis_key_ok() {
  local key="$1"
  if [ "$LEZ_TAG" = v0.4.0 ]; then
    [[ "$key" =~ ^00[0-9a-fA-F]{64}$ ]]
  else
    [[ "$key" =~ ^[0-9a-fA-F]{64}$ ]]
  fi
}

# Print the selected r0vm from the first rzup home that has it; fail if none does.
lez_find_r0vm() {
  local home candidate
  for home in "$@"; do
    for candidate in "$home"/extensions/v"$R0VM_VERSION"-cargo-risczero-*/r0vm; do
      if [ -f "$candidate" ]; then
        printf '%s\n' "$candidate"
        return 0
      fi
    done
  done
  return 1
}
