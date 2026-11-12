#!/usr/bin/env bash
# Copy the crates each module links into the module's crate directory (ignored by git), where cargo
# and the module builder find them: the builder copies only rust-lib/. The work is stage.py's (the
# repository's crates under rust-lib/staged/, LEZ's lee_core under rust-lib/upstream/, manifests with
# their inherited fields resolved; STAGED.json records what was copied, which version() reports).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
exec python3 -I "$here/stage.py" "$@"
