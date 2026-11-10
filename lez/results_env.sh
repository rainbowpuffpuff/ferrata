#!/usr/bin/env bash
# Records the environment the tables in results/ were measured in (results/environment.md).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
up="$here/../../upstream/lez-v0.4.0-rc1"
{
  echo "# Measurement environment"
  echo
  echo "- date: $(date -Is)"
  echo "- ferrata worktree HEAD: $(git -C "$here" rev-parse HEAD) (uncommitted: $(git -C "$here" status --short | wc -l) paths)"
  echo "- LEZ: $(git -C "$up" describe --tags 2>/dev/null) = $(git -C "$up" rev-parse HEAD), upstream tree clean: $([ -z "$(git -C "$up" status --short)" ] && echo yes || echo NO)"
  echo "- risc0-zkvm (host and guests): $(grep -A1 'name = "risc0-zkvm"' "$here/Cargo.lock" | sed -n 2p | tr -d ' ')"
  echo "- r0vm: $(r0vm --version)"
  echo "- host toolchain: $(rustc +1.98.1 --version)"
  echo "- guest toolchain: RISC Zero Rust $(RISC0_HOME=${RISC0_HOME:-$HOME/.risc0} rzup show 2>/dev/null | awk '/^rust/{getline; print $2}'), C++ 2024.1.5"
  echo "- CPU: $(lscpu | sed -n 's/^Model name: *//p'), $(nproc) threads"
  echo "- RAM: $(free -g | awk '/Mem:/{print $2}') GB; GPU: none (CPU proving)"
  echo "- OS: $(uname -sr)"
} > "$here/results/environment.md"
cat "$here/results/environment.md"
