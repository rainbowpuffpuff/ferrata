#!/usr/bin/env bash
# The whole reproduction in one command, with Docker: build the image of scripts/reproduce/Dockerfile (every toolchain
# pinned), copy this checkout's HEAD into a container (git archive: committed files only; a directory that is not a
# git checkout is copied without its build outputs), run scripts/reproduce/check.sh there and print its progress, one
# line per check and a verdict. Logs: target/reproduce/<UTC time>/, or --out DIR.
#
#   scripts/reproduce/run.sh [--only NAME[,NAME...]] [--out DIR]
#
# The checks and what they leave out: scripts/reproduce/README.md. Needs Docker with x86-64 images (Apple silicon
# emulates them, slowly), network for crates.io, GitHub and a public Sepolia endpoint (read-only; SEPOLIA_RPC_URL, if
# set, is passed in), and about 60 GB of disk. FERRATA_HARNESS_THREADS, if set, is passed in (check.sh).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$(cd "$HERE/../.." && pwd)"
OUT="" ARGS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --only) ARGS+=(--only "$2"); shift 2 ;;
    -h|--help) sed -n '2,12p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done
OUT="${OUT:-$SRC/target/reproduce/$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$OUT" && OUT="$(cd "$OUT" && pwd)"

docker build --platform linux/amd64 -t ferrata-reproduce "$HERE"

if COMMIT="$(git -C "$SRC" rev-parse --verify -q HEAD 2> /dev/null)"; then
  [ -z "$(git -C "$SRC" status --porcelain --untracked-files=no)" ] \
    || echo "note: uncommitted changes are not checked; the container gets HEAD, $COMMIT" >&2
  tree() { git -C "$SRC" archive --format=tar HEAD; }
else
  COMMIT="not a git checkout"
  tree() { tar -C "$SRC" --exclude=target --exclude=./vault/lib --exclude=./vault/out --exclude=./vault/cache -cf - .; }
fi
# The logs come back owned by the caller; the exit status is check.sh's.
tree | docker run --rm -i --platform linux/amd64 -v "$OUT:/out:z" -e FERRATA_COMMIT="$COMMIT" -e SEPOLIA_RPC_URL \
  -e FERRATA_HARNESS_THREADS -e HOST_IDS="$(id -u):$(id -g)" ferrata-reproduce bash -c \
  'mkdir -p /work/ferrata && tar -x -C /work/ferrata && /work/ferrata/scripts/reproduce/check.sh --out /out "$@"
   rc=$?; chown -R "$HOST_IDS" /out; exit $rc' check ${ARGS[@]+"${ARGS[@]}"}
