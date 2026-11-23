#!/usr/bin/env bash
# RISC Zero's toolchains for CI, with rzup (https://risczero.com/install): in ~/.risc0, cargo-risczero and r0vm
# 3.1.0, guest Rust 1.88.0 and C++ 2024.1.5 (the guests of bridge/ and checkpoint/, on risc0-zkvm 3.1.0 since the
# migration's phase 1, docs/design/risc0-3.1-migration.md); in $RISC0_HOME_LEZ (default ~/risc0-home-lez),
# cargo-risczero and r0vm 3.0.5 (LEZ's version until LEZ is tagged on 3.1) with guest Rust 1.91.1, the version of
# LEZ's own guest builds (lez/RESULTS.md §8), kept apart so that it does not change ~/.risc0's default. Installs only what is missing: the CI job restores both
# homes from its cache first.
set -euo pipefail
export PATH="$HOME/.risc0/bin:$PATH"
lez_home="${RISC0_HOME_LEZ:-$HOME/risc0-home-lez}"
command -v rzup > /dev/null || curl -sSfL https://risczero.com/install | bash
need() { # home component version: rzup installs it unless that version is there. rzup keeps toolchains in
  # toolchains/, cargo-risczero in extensions/, and r0vm inside cargo-risczero's extension (no directory of its own).
  # Each place is tested on its own: an unmatched pattern stays literal and does not exist.
  local d
  for d in "$1"/toolchains/v"$3"-"$2"-* "$1"/extensions/v"$3"-"$2"-* "$1"/extensions/v"$3"-cargo-risczero-*/"$2"; do
    [ -e "$d" ] && return 0
  done
  RISC0_HOME="$1" rzup install "$2" "$3"
}
mkdir -p "$HOME/.risc0" "$lez_home"
need "$HOME/.risc0" cargo-risczero 3.1.0
need "$HOME/.risc0" r0vm 3.1.0
need "$lez_home" cargo-risczero 3.0.5
need "$lez_home" r0vm 3.0.5
for home in "$HOME/.risc0" "$lez_home"; do
  need "$home" cpp 2024.1.5
done
need "$HOME/.risc0" rust 1.88.0
need "$lez_home" rust 1.91.1
for home in "$HOME/.risc0" "$lez_home"; do echo "$home:"; (cd "$home" && ls -d toolchains/* extensions/*/*); done
