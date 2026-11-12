#!/usr/bin/env bash
# Stage the modules' crates, then build and test each module's methods natively (the calls crates,
# without Logos Core). Writes a missing rust-lib/Cargo.lock (the module builder needs it committed).
# Needs cargo; the logos-rust-sdk source of a module (its path dependency) is taken from another
# module when one has it (every build pins the same builder, hence the same SDK).
#
#   MODULES="..." modules/scripts/native.sh
#
# Default: the four modules that need only cargo. ferrata_updater_module also needs the guests of
# lez/methods built (the RISC Zero toolchain of lez/, stage-updater.sh) and LEZ's host toolchain (1.98.1).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
MODULES=${MODULES:-"ferrata_attest_module ferrata_bridge_module ferrata_attestor_module ferrata_monitor_module"}
ALL="ferrata_attest_module ferrata_bridge_module ferrata_attestor_module ferrata_monitor_module ferrata_updater_module"
"$here/stage.sh" $MODULES
calls_crate() { case "$1" in ferrata_attest_module) echo ferrata-attest-calls ;; ferrata_bridge_module) echo ferrata-bridge-calls ;;
  ferrata_attestor_module) echo ferrata-attestor-calls ;; ferrata_monitor_module) echo ferrata-monitor-calls ;;
  ferrata_updater_module) echo ferrata-updater-calls ;; esac; }
cargo_for() { case "$1" in ferrata_updater_module) echo "cargo +1.98.1" ;; *) echo cargo ;; esac; }
for m in $MODULES; do
  d="$repo/modules/$m"
  if [ ! -e "$d/logos-rust-sdk-src" ]; then
    for o in $ALL; do
      if [ -e "$repo/modules/$o/logos-rust-sdk-src" ]; then ln -s "$(readlink -f "$repo/modules/$o/logos-rust-sdk-src")" "$d/logos-rust-sdk-src"; break; fi
    done
  fi
  if [ ! -f "$d/rust-lib/Cargo.lock" ]; then (cd "$d/rust-lib" && $(cargo_for "$m") generate-lockfile); fi
  (cd "$d/rust-lib" && RISC0_DEV_MODE=1 $(cargo_for "$m") test --release --locked -p "$(calls_crate "$m")" \
     && $(cargo_for "$m") build --release --locked -p "$(calls_crate "$m")" --example call)
done
