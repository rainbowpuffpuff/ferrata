# One command for every number and every release (docs/release-engineering.md).
#
# Heavy stages run where FERRATA_BOX says: empty (the default) runs them on this machine; set it to a command prefix
# that runs a shell command on a build machine, for example the project's Modal build box:
#   export FERRATA_BOX="../modal-build/mrun --profile <workspace> --"
# Each recipe is a thin wrapper of a script under scripts/, which can also be run without `just`.

set shell := ["bash", "-euo", "pipefail", "-c"]
# Recipe arguments reach the scripts as positional parameters ("$@"), each one word, never re-split or run by the shell.
set positional-arguments

box := env_var_or_default("FERRATA_BOX", "")
# With a remote box: a command prefix that copies a path from the box to the same path here, followed by `-- true`
# (for the Modal build box: "../modal-build/mrun --profile <workspace> --no-sync --get").
fetch := env_var_or_default("FERRATA_BOX_FETCH", "")
stamp := `date -u +%Y%m%dT%H%MZ`
commit := `git rev-parse --short=12 HEAD 2>/dev/null || echo unknown`
# The full commit, passed to a build machine whose copy of the tree has no .git (FERRATA_COMMIT); "+dirty" when a
# tracked file differs from it or an untracked file exists (scripts/release.sh commit).
commit_full := `scripts/release.sh commit`

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
    python3 -I scripts/m0.py "$@"

# A versioned release (programs and image ids, vault bytecode, CLI, MANIFEST.json, SHA256SUMS), recorded in releases/.
release version out=(justfile_directory() + "/../tmp/releases"):
    {{box}} env FERRATA_COMMIT={{commit_full}} scripts/release.sh build {{out}}
    {{box}} env FERRATA_COMMIT={{commit_full}} scripts/release.sh package {{out}} {{version}}
    {{box}} scripts/release.sh verify {{out}}/ferrata-{{version}} --strict
    {{box}} scripts/release.sh record {{out}}/ferrata-{{version}}
    if [ -n "{{fetch}}" ]; then {{fetch}} {{out}}/ferrata-{{version}}-x86_64-linux.tar.gz -- true; fi
    if [ -n "{{fetch}}" ]; then {{fetch}} {{out}}/ferrata-{{version}}-third-party-sources.tar -- true; fi

# Check a release directory (checksums, manifest, image ids where r0vm is installed).
release-verify dir:
    scripts/release.sh verify "$1"

# Deploy a release on a LEZ testnet and an EVM chain; plan only unless --send (scripts/deploy-testnet.sh --help).
deploy-testnet *args:
    scripts/deploy-testnet.sh "$@"

# The four Halmos properties that did not finish in 900 s, with a longer budget (HALMOS_LONG_TIMEOUT, default 4800 s).
halmos-long:
    {{box}} scripts/halmos-long.sh install run

# Re-check the evidence: every check, its requirement rows and EVIDENCE rows (docs/VERIFY.md; --list, --all, --online).
verify *args:
    python3 -I scripts/verify.py "$@"

# CI's verify job, the reviewer's offline re-check and every check family's tamper tests (docs/VERIFY.md).
# Needs Foundry and the vault's pinned libraries. As in CI, the final commands refuse any tree change left by a check.
ci-verify:
    (cd vault && { [ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.1.0; })
    python3 -I scripts/vault_libs.py check
    (cd vault && forge build)
    python3 -I scripts/verify.py --all --offline --strict
    python3 -I scripts/test_verify.py && python3 -I scripts/test_verify_lc.py && python3 -I scripts/test_verify_f13.py && python3 -I scripts/test_verify_f7.py
    git status --porcelain --untracked-files=all
    test -z "$(git status --porcelain --untracked-files=all)"

# Read our public LEZ test network (or --lez URL) from a terminal: head, block N, tx HASH, programs, instance ACCOUNT.
lez-read *args:
    python3 -I scripts/lez-read.py "$@"

# Print the commands CI's jobs run, as scripts/ci/ci_model.py reads the workflows (one job, several, or all).
ci-commands *jobs:
    python3 -I scripts/ci/ci_model.py --trace "$@"
