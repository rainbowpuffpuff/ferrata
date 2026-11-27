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
# Where CI's runner keeps its scratch files ($RUNNER_TEMP in the workflows): a local stand-in when it is
# unset, used by the recipes that run a whole CI job (the links job's sequencer-record check writes there).
export RUNNER_TEMP := env_var_or_default("RUNNER_TEMP", justfile_directory() + "/../tmp/runner-temp")
# The environment CI's workflows set for every job (ci.yml's top-level env), exported here so a recipe that
# runs a whole job runs it with the variables the job's steps see (scripts/ci/test_ci_model.py compares them).
export FOUNDRY_OFFLINE := "false"

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

# Read our public LEZ test network (or --lez URL) from a terminal: head, block N, tx HASH, programs, instance ACCOUNT.
lez-read *args:
    python3 -I scripts/lez-read.py "$@"

# The internal security review of 27 November 2026 (docs/security/internal-review-2026-11-27.md): the vault suites of
# its fixes, of RFP-022 Sec.9's LEZ case (docs/security/lez-receipt-unsound.md), of the real-seal journal words and of
# R.4's entitlement invariant; the verifier check against fake wirings on anvil; the R.8 checks and their tamper
# tests. Needs forge, anvil and vault/lib (scripts/vault_libs.py install).
security-review:
    cd vault && forge test --match-path 'test/{InternalReview,LezReceiptUnsound,DeployDefaults,DeployVaultChecks,VerifierCode}.t.sol'
    cd vault && forge test --match-path 'test/{real/Rc3JournalWords,invariant/EntitlementInvariants}.t.sol'
    bash vault/script/test-check-verifier-local.sh
    python3 -I scripts/verify.py r8-risc0-selectors r8-phase-split
    python3 -I scripts/test_verify_sec.py

# Print the commands CI's jobs run, as scripts/ci/ci_model.py reads the workflows (one job, several, or all).
ci-commands *jobs:
    python3 -I scripts/ci/ci_model.py --trace "$@"

# ── The CI jobs a terminal can run as they are ─────────────────────────────────────────────────────
# Each recipe below runs exactly the `run:` commands of one job of .github/workflows/ci.yml, in order, one
# recipe line per command line: scripts/ci/test_ci_model.py compares them with the workflow as
# scripts/ci/ci_model.py reads it (run `just ci-commands JOB` or
# `python3 -I scripts/ci/ci_model.py --steps JOB` to see them) — the command text, the directory it runs in,
# the environment the workflow sets, and the shell options. What the recipes leave out is the runner's own
# setup, the `uses:` steps: checkout, toolchain installs and caches. Each recipe line is its own shell, as
# each CI step is; `just` runs them with bash -euo pipefail, so those jobs ask GitHub for `shell: bash`
# (which adds pipefail to its `-e`; `just`'s `-u` only makes its own run stricter), and the recipes export
# the variables the workflows set for the whole file (FOUNDRY_OFFLINE).

# CI's idl job: the IDL alone decodes every committed borsh vector of both programs (idl/README.md).
idl:
    python3 -I idl/check.py

# CI's admin job: the admin view's self-test, offline (admin/README.md).
admin:
    node admin/dump.mjs --selftest

# CI's links job: every documentation, record and tooling check of the repository that needs no toolchain
# beyond Python, Node, bash and git (30 steps; see the workflow's comments for what each one checks).
links:
    python3 -I scripts/check-links.py README.md docs cli/README.md monitor/README.md lez/ATTEST-CLI.md lez/updater/README.md e2e/seqnet/README.md
    python3 -I scripts/check-guides.py
    python3 -I scripts/test_check_guides.py
    python3 -I scripts/check-decisions.py
    python3 -I scripts/test_check_decisions.py
    python3 -I scripts/reproduce/test_image_id_tools.py
    python3 -I scripts/test_fetch_native.py
    python3 -I scripts/test_release_manifest.py
    python3 -I scripts/test_release_scripts.py
    python3 -I scripts/test_vault_libs.py
    python3 -I scripts/test_deny_all.py
    python3 -I deploy/testnet/tests/test_prover_client.py
    python3 -I scripts/test_verify_live_roundtrip.py
    python3 -I lez/scripts/test_fetch_attest_fixture.py
    python3 -I scripts/test_perf_records.py
    python3 -I bench/perf/test_summarize_proofs.py
    python3 -I modules/scripts/test_live_testnet.py
    python3 -I checkpoint/attestor/test_resources.py
    python3 -I scripts/test_audit_scope.py
    python3 -I deploy/testnet/tests/test_gateway_gasdrop.py
    python3 -I deploy/testnet/tests/test_zone_keys.py && python3 -I deploy/testnet/tests/test_checkpoint_attestors.py
    python3 -I site/test_site.py --skip-freshness
    python3 -I scripts/ci/check-row-tests.py
    python3 -I scripts/ci/test_ci_model.py
    python3 -I scripts/ci/check-lez-shards.py
    python3 -I scripts/ci/test_lez_shards.py
    if command -v yq > /dev/null; then python3 -I scripts/ci/ci_model.py --compare-yq; else echo "yq not on this runner; skipped"; fi
    python3 -I scripts/ci/check-portable.py
    bash scripts/ci/test-sequencer-records.sh "$RUNNER_TEMP/test-sequencer-records"
    bash scripts/ci/check-unaudited.sh

# CI's verify job, the reviewer's re-check (docs/VERIFY.md). Named ci-verify because `just verify ARGS…` is
# the pass-through to scripts/verify.py itself. Needs Foundry (forge) and the vault's pinned libraries, which
# the first command installs when they are missing, and leaves the tree as it found it, as the job does.
ci-verify:
    (cd vault && { [ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.1.0; })
    python3 -I scripts/vault_libs.py check
    (cd vault && forge build)
    python3 -I scripts/verify.py --all --offline --strict
    python3 -I scripts/test_verify.py && python3 -I scripts/test_verify_lc.py && python3 -I scripts/test_verify_zone.py && python3 -I scripts/test_verify_f13.py && python3 -I scripts/test_verify_f13_lez.py && python3 -I scripts/test_verify_f7.py && python3 -I scripts/test_verify_bridge.py && python3 -I scripts/test_verify_bridge_rows.py && python3 -I scripts/test_verify_sec.py
    git status --porcelain --untracked-files=all
    test -z "$(git status --porcelain --untracked-files=all)"

# CI's lez-harness job's test step, one shard: harness-1 to harness-4, updater-gate, updater-privacy
# (scripts/ci/lez-harness.sh --dry-run prints them). The job's own build step runs the same command with a
# trailing `build`; the steps before it are the runner's setup (toolchains, upstream sources, libraries).
lez-harness shard:
    scripts/ci/lez-harness.sh --shard "$1"
