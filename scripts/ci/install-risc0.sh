#!/usr/bin/env bash
# RISC Zero's toolchains for CI, with rzup (https://risczero.com/install): in ~/.risc0, cargo-risczero and r0vm
# 3.0.5, guest Rust 1.88.0 and C++ 2024.1.5 (the guests of bridge/ and checkpoint/); in $RISC0_HOME_LEZ (default
# ~/risc0-home-lez), the same with guest Rust 1.91.1, the version of LEZ's own guest builds (lez/RESULTS.md §8),
# kept apart so that it does not change ~/.risc0's default. Installs only what is missing: the CI job restores both
# homes from its cache first.
set -euo pipefail
export PATH="$HOME/.risc0/bin:$PATH"
lez_home="${RISC0_HOME_LEZ:-$HOME/risc0-home-lez}"
command -v rzup > /dev/null || curl -sSfL https://risczero.com/install | bash
need() { # home component version
  ls -d "$1"/toolchains/v"$3"-"$2"-* "$1"/extensions/v"$3"-"$2"-* > /dev/null 2>&1 || RISC0_HOME="$1" rzup install "$2" "$3"
}
for home in "$HOME/.risc0" "$lez_home"; do
  mkdir -p "$home"
  need "$home" cargo-risczero 3.0.5
  need "$home" r0vm 3.0.5
  need "$home" cpp 2024.1.5
done
need "$HOME/.risc0" rust 1.88.0
need "$lez_home" rust 1.91.1
ls "$HOME/.risc0"/toolchains "$HOME/.risc0"/extensions "$lez_home"/toolchains "$lez_home"/extensions
