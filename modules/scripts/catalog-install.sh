#!/usr/bin/env bash
# Install the modules into a headless Logos Core *from the catalog*, as a client would: the dry-run
# catalog (release-local.sh's packages and index.json, and modules/catalog/logos-repo.json with its
# indexUrl pointed at the local server) served over HTTPS on 127.0.0.1 (logosctl's catalogs require
# https: "unsupported URL scheme (https required in v1)") with a throwaway self-signed certificate that
# the daemon is given as its CA bundle (SSL_CERT_FILE), `logosctl catalog add` its logos-repo.json,
# `catalog source http`, `catalog refresh`, `search`, `install NAME`, `module load`, `call version`.
#
#   modules/scripts/catalog-install.sh RELEASE_DIR LOGOSCTL WORK_DIR    (logs also to modules/catalog/install-test/)
# RELEASE_DIR: release-local.sh's OUT_DIR (its index.json names packages on 127.0.0.1:18546).
# LOGOSCTL: a portable logosctl (check.sh's out/logos-portable/bin/logosctl): the packages are portable.
# INSTALL_TEST_DIR names another directory than modules/catalog/install-test/ for the kept logs.
set -uo pipefail
# Logos Core keeps each module's socket in TMPDIR (logos_<module>_<id>). A Unix socket path holds at most
# 107 bytes; with a longer one every module's host process dies at load (SIGSEGV after "failed to listen",
# met on the build box). LOGOS_TMPDIR (default TMPDIR) must leave room for the longest module name.
LOGOS_TMPDIR=${LOGOS_TMPDIR:-${TMPDIR:-/tmp}}
if [ $(( ${#LOGOS_TMPDIR} + 45 )) -gt 107 ]; then
  echo "$(basename "$0"): LOGOS_TMPDIR=$LOGOS_TMPDIR is too long for Logos Core's sockets (62 bytes at most); set a shorter one" >&2
  exit 2
fi
mkdir -p "$LOGOS_TMPDIR"
export TMPDIR="$LOGOS_TMPDIR"
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
rel=$(cd "$1" && pwd)
ctl_bin=$2
mkdir -p "$3"
out=$(cd "$3" && pwd)
PORT=18546
srv="$out/www"
rm -rf "$srv" "$out/session" "$out/home"
mkdir -p "$srv" "$out/home"
cp -r "$rel/dist/." "$srv/"
jq --arg from "http://127.0.0.1:$PORT/" --arg to "https://127.0.0.1:$PORT/" \
  '(.. | strings) |= (if startswith($from) then $to + .[($from | length):] else . end)' "$rel/index.json" > "$srv/index.json"
jq --arg u "https://127.0.0.1:$PORT/index.json" '.indexUrl = $u' "$repo/modules/catalog/logos-repo.json" > "$srv/logos-repo.json"
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$out/key.pem" -out "$out/cert.pem" -days 1 \
  -subj "/CN=127.0.0.1" -addext "subjectAltName=IP:127.0.0.1" > "$out/openssl.log" 2>&1
python3 -I -c '
import functools, http.server, ssl, sys
port, d, cert, key = int(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
h = functools.partial(http.server.SimpleHTTPRequestHandler, directory=d)
s = http.server.ThreadingHTTPServer(("127.0.0.1", port), h)
c = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER); c.load_cert_chain(cert, key)
s.socket = c.wrap_socket(s.socket, server_side=True)
s.serve_forever()' "$PORT" "$srv" "$out/cert.pem" "$out/key.pem" > "$out/http.log" 2>&1 &
pid=$!
export SSL_CERT_FILE="$out/cert.pem" NIX_SSL_CERT_FILE="$out/cert.pem"
ctl() { "$ctl_bin" --config-dir "$out/session" "$@"; }
trap 'ctl daemon stop > /dev/null 2>&1; kill $pid 2>/dev/null || true' EXIT
for _ in $(seq 1 100); do (echo > "/dev/tcp/127.0.0.1/$PORT") 2>/dev/null && break; sleep 0.1; done
fails=0
run() { echo "\$ logosctl $*" >> "$out/log.txt"; if ctl "$@" >> "$out/log.txt" 2>&1; then echo "ok: $*"; else echo "FAILED: $*"; fails=$((fails + 1)); fi; }
: > "$out/log.txt"
echo "\$ RISC0_DEV_MODE=1 logosctl daemon start --detach   (the updater module's in-process chain runs as its tests do)" >> "$out/log.txt"
HOME="$out/home" RISC0_DEV_MODE=1 "$ctl_bin" --config-dir "$out/session" daemon start --detach >> "$out/log.txt" 2>&1 || fails=$((fails + 1))
# The daemon's default catalog (Logos's official one, on GitHub) cannot be verified with the throwaway
# CA bundle: disable it, so that the refresh reads this catalog only.
default=$(ctl catalog ls 2>/dev/null | jq -r '.[] | select(.isDefault == true) | .url // .repositoryUrl // empty' 2>/dev/null | head -n 1)
run catalog disable "${default:-https://raw.githubusercontent.com/logos-co/logos-modules-release/refs/heads/main/logos-repo.json}"
run catalog add "https://127.0.0.1:$PORT/logos-repo.json"
run catalog source http
run catalog refresh
run catalog ls
run search ferrata
for m in ${MODULES:-ferrata_attest_module ferrata_bridge_module ferrata_attestor_module ferrata_monitor_module ferrata_updater_module ferrata_relayer_module}; do
  run install "$m" -y
  run module load "$m"
  run call "$m" version --json
done
run package ls
# The installed modules at work: with CATALOG_E2E=1, attestor-e2e.sh through this daemon (the zone's
# records, the attestor's loop against a node served over HTTP), and any further script named in
# CATALOG_E2E_MORE (run with CALL=logosctl LOGOSCTL SESSION set, an output directory as argument), for
# example "updater-e2e.sh monitor-e2e.sh e2e.sh bridge_e2e.sh" (bridge_e2e.sh needs RELEASE_ELF).
if [ "${CATALOG_E2E:-0}" = 1 ]; then
  for s in attestor-e2e.sh ${CATALOG_E2E_MORE:-}; do
    echo "\$ CALL=logosctl $s" >> "$out/log.txt"
    if env -u SSL_CERT_FILE -u NIX_SSL_CERT_FILE CALL=logosctl LOGOSCTL="$ctl_bin" SESSION="$out/session" "$here/$s" "$out/${s%.sh}" \
        > "$out/${s%.sh}.log" 2>&1; then echo "ok: $s"; else echo "FAILED: $s"; fails=$((fails + 1)); fi
    grep -E '^- (PASS|FAIL)' "$out/${s%.sh}/summary.md" >> "$out/log.txt" 2>/dev/null || true
    # e2e.sh and bridge_e2e.sh write their checks as JSON (e2e.json, bridge_e2e.json).
    for j in "$out/${s%.sh}"/*e2e.json; do
      if [ -f "$j" ]; then jq -r '"- \(.checks) checks, \(.failed) failed (\(input_filename | split("/") | last))"' "$j" >> "$out/log.txt"; fi
    done
  done
fi
run daemon stop
echo "failures: $fails" | tee -a "$out/log.txt"
# The small logs into the repository, without machine paths.
keep="${INSTALL_TEST_DIR:-$repo/modules/catalog/install-test}"
rm -rf "$keep"
mkdir -p "$keep"
cp "$out/log.txt" "$out/http.log" "$keep/"
spike=$(cd "$repo/.." && pwd)
sed -i "s#$spike#spike#g" "$keep/log.txt" "$keep/http.log"
exit $(( fails > 0 ? 1 : 0 ))
