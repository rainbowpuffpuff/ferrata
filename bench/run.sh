#!/usr/bin/env bash
# Record what is measured, build every guest config, run the executor (no proving).
# Usage: ./run.sh [CONFIG ...] [--modes MODE,...]      Results go to results/.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
export TMPDIR="$here/../../tmp"   # spike/tmp: never /tmp on this machine
mkdir -p "$TMPDIR" "$here/results"
cd "$here"
{
  echo "# Provenance of the ferrata-core measured by bench/, captured before the run"
  date -Is
  echo "HEAD $(git -C .. rev-parse HEAD)"
  echo "git status --short -- crates/ferrata-core:"
  git -C .. status --short -- crates/ferrata-core
  echo "sha256 of git diff HEAD -- crates/ferrata-core: $(git -C .. diff HEAD -- crates/ferrata-core | sha256sum | cut -d' ' -f1)"
  (cd ../crates/ferrata-core && find Cargo.toml src -type f | sort | xargs sha256sum)
} > results/PROVENANCE.txt
cargo build --release -p host
./target/release/host "$@"
