#!/usr/bin/env bash
# Fetch the two upstream source trees that `lez/` and `checkpoint/fidelity/` build against,
# into `upstream/` beside this repository (the path their Cargo manifests use):
#   upstream/lez-v0.4.0-rc1               logos-execution-zone at tag v0.4.0-rc1 (commit 1806e87)
#   upstream/logos-blockchain-d480efc6    logos-blockchain at d480efc6, the revision LEZ pins
# Read-only clones; nothing is sent anywhere.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
up="$(dirname "$here")/upstream"
mkdir -p "$up"

lez="$up/lez-v0.4.0-rc1"
if [ ! -d "$lez/.git" ]; then
  git clone --depth 1 --branch v0.4.0-rc1 https://github.com/logos-blockchain/logos-execution-zone.git "$lez"
fi
test "$(git -C "$lez" rev-parse --short=7 HEAD)" = "1806e87" || { echo "unexpected LEZ commit" >&2; exit 1; }

lb="$up/logos-blockchain-d480efc6"
if [ ! -d "$lb/.git" ]; then
  git init -q "$lb"
  git -C "$lb" remote add origin https://github.com/logos-blockchain/logos-blockchain.git
  git -C "$lb" fetch -q --depth 1 origin d480efc6 2>/dev/null \
    || git -C "$lb" fetch -q origin   # servers that refuse a short id: fetch all, then check out
  git -C "$lb" checkout -q d480efc6 2>/dev/null || git -C "$lb" checkout -q FETCH_HEAD
fi
case "$(git -C "$lb" rev-parse HEAD)" in d480efc6*) ;; *) echo "unexpected Logos Blockchain commit" >&2; exit 1;; esac
echo "upstream sources ready in $up"
