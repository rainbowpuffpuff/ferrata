#!/usr/bin/env bash
# Stage both modules' crates, then build and test each module's methods natively (the calls crates,
# without Logos Core). Writes a missing rust-lib/Cargo.lock (the module builder needs it committed).
# Needs cargo; the logos-rust-sdk source of a module (its path dependency) is taken from the other
# module when only one has it (both builds pin the same builder, hence the same SDK).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
"$here/stage.sh"
for m in ferrata_attest_module ferrata_bridge_module; do
  d="$repo/modules/$m"
  if [ ! -e "$d/logos-rust-sdk-src" ]; then
    for o in ferrata_attest_module ferrata_bridge_module; do
      if [ -e "$repo/modules/$o/logos-rust-sdk-src" ]; then ln -s "$(readlink -f "$repo/modules/$o/logos-rust-sdk-src")" "$d/logos-rust-sdk-src"; break; fi
    done
  fi
  if [ ! -f "$d/rust-lib/Cargo.lock" ]; then (cd "$d/rust-lib" && cargo generate-lockfile); fi
done
(cd "$repo/modules/ferrata_attest_module/rust-lib" && cargo test --release --locked -p ferrata-attest-calls && cargo build --release --locked -p ferrata-attest-calls --example call)
(cd "$repo/modules/ferrata_bridge_module/rust-lib" && cargo test --release --locked -p ferrata-bridge-calls && cargo build --release --locked -p ferrata-bridge-calls --example call)
