# One command for every number and every release (docs/release-engineering.md).
#
# Heavy stages run where FERRATA_BOX says: empty (the default) runs them on this machine; set it to a command prefix
# that runs a shell command on a build machine, for example the project's Modal build box:
#   export FERRATA_BOX="../modal-build/mrun --profile <workspace> --"
# Each recipe is a thin wrapper of a script under scripts/, which can also be run without `just`.

set shell := ["bash", "-euo", "pipefail", "-c"]

box := env_var_or_default("FERRATA_BOX", "")
# With a remote box: a command prefix that copies a path from the box to the same path here, followed by `-- true`
# (for the Modal build box: "../modal-build/mrun --profile <workspace> --no-sync --get").
fetch := env_var_or_default("FERRATA_BOX_FETCH", "")
stamp := `date -u +%Y%m%dT%H%MZ`
commit := `git rev-parse --short=12 HEAD 2>/dev/null || echo unknown`
# The full commit, passed to a build machine whose copy of the tree has no .git (FERRATA_COMMIT); "+dirty" when
# tracked files differ from it.
commit_full := `if [ -n "${FERRATA_COMMIT:-}" ]; then echo "$FERRATA_COMMIT"; elif c=$(git rev-parse --verify -q HEAD); then git diff --quiet HEAD -- . && echo $c || echo $c+dirty; else echo unknown; fi`

# List the recipes.
default:
    @just --list

# The proposal's performance numbers again (§B8): hardware, gas, light-client cycles, a real release proof; REPORT.md.
bench out=("bench/report/" + stamp + "-" + commit):
    mkdir -p {{out}}
    {{box}} scripts/bench.sh box {{out}}
    python3 -I scripts/bench_report.py {{out}}

# The user-side proofs (attestation, claim, burn, padded like LEZ's wallet) proven on Modal at BENCH_CPUS CPUs, 16 GiB.
bench-user out=("bench/report/" + stamp + "-" + commit) stage=(justfile_directory() + "/../tmp/bench-stage/" + commit):
    {{box}} scripts/bench.sh user-stage {{stage}}
    if [ -n "{{fetch}}" ]; then {{fetch}} {{stage}}/stage/bundles -- true; fi
    BENCH_BUNDLES={{stage}}/stage/bundles scripts/bench.sh user-prove {{out}}
    python3 -I scripts/bench_report.py {{out}}

# Every M0 check of the proposal with its measured value and GO / NO-GO (--online re-reads upstream tags).
m0 *args:
    python3 -I scripts/m0.py {{args}}

# A versioned release (programs and image ids, vault bytecode, CLI, MANIFEST.json, SHA256SUMS), recorded in releases/.
release version out=(justfile_directory() + "/../tmp/releases"):
    {{box}} env FERRATA_COMMIT={{commit_full}} scripts/release.sh build {{out}}
    {{box}} env FERRATA_COMMIT={{commit_full}} scripts/release.sh package {{out}} {{version}}
    {{box}} scripts/release.sh verify {{out}}/ferrata-{{version}}
    {{box}} scripts/release.sh record {{out}}/ferrata-{{version}}
    if [ -n "{{fetch}}" ]; then {{fetch}} {{out}}/ferrata-{{version}}-x86_64-linux.tar.gz -- true; fi

# Check a release directory (checksums, manifest, image ids where r0vm is installed).
release-verify dir:
    scripts/release.sh verify {{dir}}

# Deploy a release on a LEZ testnet and an EVM chain; plan only unless --send (scripts/deploy-testnet.sh --help).
deploy-testnet *args:
    scripts/deploy-testnet.sh {{args}}

# The four Halmos properties that did not finish in 900 s, with a longer budget (HALMOS_LONG_TIMEOUT, default 4800 s).
halmos-long:
    {{box}} scripts/halmos-long.sh install run
