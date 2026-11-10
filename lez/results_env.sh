#!/usr/bin/env bash
# Records the environment the tables in results/ were measured in (results/environment.md).
#
# On a machine that holds the repository's history, run it as it is. Where the sources are only a
# copy without `.git` (a build container), pass what git would say, from a machine that has it:
#   FERRATA_HEAD      `git rev-parse HEAD` of this repository
#   FERRATA_DIRTY     the number of uncommitted paths besides results/ and RESULTS.md
#   FERRATA_LEZ       LEZ's tag and commit, and whether its tree is clean
#   FERRATA_WHERE     a line saying where the measurements ran
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
up="$here/../../upstream/lez-v0.4.0-rc1"
risc0_home="${RISC0_HOME:-$HOME/.risc0}"
# Uncommitted paths other than the results themselves (results/, which the measurements rewrite,
# and RESULTS.md, which reports them).
dirty="${FERRATA_DIRTY:-$(git -C "$here" status --short -- ':(top)' ':(top,exclude)lez/results' ':(top,exclude)lez/RESULTS.md' | wc -l)}"
head="${FERRATA_HEAD:-$(git -C "$here" rev-parse HEAD)}"
lez="${FERRATA_LEZ:-$(git -C "$up" describe --tags 2>/dev/null) = $(git -C "$up" rev-parse HEAD), upstream tree clean: $([ -z "$(git -C "$up" status --short)" ] && echo yes || echo NO)}"
guest_rust="$(ls "$risc0_home/toolchains" 2>/dev/null | sed -n 's/^v\([0-9.]*\)-rust-.*/\1/p' | sort -V | tail -1)"
{
  echo "# Measurement environment"
  echo
  echo "- date: $(date -Is)"
  if [ -n "${FERRATA_WHERE:-}" ]; then echo "- where: $FERRATA_WHERE"; fi
  echo "- ferrata worktree HEAD: $head (uncommitted: $dirty paths besides results/ and RESULTS.md)"
  echo "- LEZ: $lez"
  echo "- risc0-zkvm (host and guests): $(grep -A1 'name = "risc0-zkvm"' "$here/Cargo.lock" | sed -n 2p | tr -d ' ')"
  echo "- r0vm: $(r0vm --version)"
  echo "- host toolchain: $(rustc +1.98.1 --version)"
  echo "- guest toolchain: RISC Zero Rust ${guest_rust:-unknown} (in RISC0_HOME), C++ 2024.1.5"
  echo "- CPU: $(lscpu | sed -n 's/^Model name: *//p' | head -1), $(nproc) threads"
  echo "- RAM: $(free -g | awk '/Mem:/{print $2}') GB; GPU: none"
  echo "- OS: $(uname -sr)"
} > "$here/results/environment.md"
cat "$here/results/environment.md"
