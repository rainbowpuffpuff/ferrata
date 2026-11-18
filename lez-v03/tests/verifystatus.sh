#!/usr/bin/env bash
# testnet.sh verify (public target) against stubs only (client, curl): no network, no keys.
# From review-lez-v03's tests/verifystatus (18 Nov 2026), turned into assertions: a failing `ferrata-v03 status`,
# or fewer checks than signed transactions, must fail verify; Pending blocks give 3; all shown gives 0.
# Usage: tests/verifystatus.sh [path to testnet.sh] (default: ../testnet.sh). Exit 0 if all pass.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
SRC="$(cd "$HERE/.." && pwd)/testnet.sh"; SRC="${1:-$SRC}"
W="$HERE/work-verifystatus"; rm -rf "$W"
L="$W/spike/wt/lez-v03"
mkdir -p "$L/target/release" "$L/results/public" "$W/bin"
cp "$SRC" "$L/testnet.sh"
echo '{}' > "$L/results/public/record.json"
printf 'seq\tlabel\n0\ta\n1\tb\n' > "$L/results/public/signed.tsv"
cat > "$L/target/release/ferrata-v03" <<'S'
#!/usr/bin/env bash
[ $# -eq 0 ] && { echo usage; exit 2; }
case "$1" in
  verify) echo "MATCH    (stub: state verdicts)"; exit 0 ;;
  status)
    case "$VS_MODE" in
      fail) echo "error: getTransaction: request timed out (stub)" >&2; exit 1 ;;
      short) printf 'h0\t5\tFinalized\ta\n' ;;
      pending) printf 'h0\t5\tPending\ta\nh1\t6\tPending\tb\n' ;;
      shown) printf 'h0\t5\tFinalized\ta\nh1\t6\tFinalized\tb\n' ;;
    esac ;;
esac
exit 0
S
printf '#!/bin/sh\necho "curl $*" >> "$SG_LOG"\necho "<html>h0 h1</html>"\n' > "$W/bin/curl"
chmod +x "$L/target/release/ferrata-v03" "$W/bin/curl"
fail=0
for c in "fail 1" "short 1" "pending 3" "shown 0"; do
  set -- $c
  (cd "$L" && env -u FERRATA_SEQUENCER PATH="$W/bin:$PATH" SG_LOG="$W/log.txt" VS_MODE="$1" bash ./testnet.sh verify > "$W/out.txt" 2>&1); rc=$?
  if [ $rc -eq "$2" ]; then echo "PASS status $1: verify exit $rc"; else echo "FAIL status $1: verify exit $rc, expected $2"; cat "$W/out.txt"; fail=1; fi
done
rm -rf "$W"
exit $fail
