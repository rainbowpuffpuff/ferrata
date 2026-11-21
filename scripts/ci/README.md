# CI's own checks

The checks that live under `scripts/ci/` and what runs them. `install-risc0.sh` and
`witness-independence.sh` predate this directory's other files; they are documented in
[`docs/guides/ci.md`](../../docs/guides/ci.md) and in ci.yml's own comments.

## check-row-tests.py

That **every hard requirement of RFP-021 (59 rows) and RFP-022 (68 rows) has at least one named test**
(RFP-022 Reliability #11; the A4/B4 gates' "one named test per hard requirement" of
[`docs/traceability.md`](../../docs/traceability.md)). It parses the hard row IDs out of
traceability.md's matrices and checks the counts, then reads the map
[`row-tests.toml`](row-tests.toml): every hard row has at least one entry; every entry names a path
tracked by git (entries on another branch are printed as pending and not required to exist here);
every `ci` names a workflow job under `.github/workflows/` or is `box`/`local`, a box entry naming
its exact `run` command. The summary lists the rows of each RFP, how many have a test that CI runs,
the rows with box or local tests only, and the pending entries. Standard library only.

```sh
python3 -I scripts/ci/check-row-tests.py
```

Run by CI's `links` job on every push and pull request.
[`row-tests.toml`](row-tests.toml) is the map itself, read by the check, with one `[[row]]` per hard
row (IDs as in traceability.md; the rows of the two RFPs that are addressed on branch `rfp-docs-final`
carry `branch = "rfp-docs-final"`, printed as pending until that branch merges).

## check-unaudited.sh

What the bid promises until the audit reports are published (RFP-021 S.8, RFP-022 Sec.1): **the code
is marked unaudited / not for production** in `README.md`, `SECURITY.md`, the catalog's description
(`modules/catalog/logos-repo.json`) and every `modules/*/metadata.json` description. The two audit rows
can have no functional test until the reports exist, so this is their entry in
[`row-tests.toml`](row-tests.toml). A marking must say it: "unaudited", "not audited", "has not been
audited", "not for production", "unfit for production" - "prototype" alone is a maturity label, not
this promise, and does not pass. Fails naming each file whose marking is missing. bash, grep, python3.

```sh
bash scripts/ci/check-unaudited.sh
```

Run by CI's `links` job on every push and pull request.

## check-actions-pinned.py

That **every third-party action a workflow uses is pinned to a full 40-character commit SHA**, in
`.github/workflows/*.yml` and `modules/catalog/.github/workflows/*.yml`. A moving tag in CI is a
supply-chain hole. Local workflow paths (no `@`) and TODO-marked lines (a tag today, carrying
`# TODO: pin to a commit SHA`) are reported separately and do not fail; anything else unpinned fails.
No SHA can be checked offline here, so the workflows still use tags (the new ones TODO-marked), and this check
fails today (22 unpinned uses on 21 November 2026). It joins CI's `links` job once the owner has run
`pin-actions.py --write`. Standard library only.

```sh
python3 -I scripts/ci/check-actions-pinned.py
```

## pin-actions.py

The owner's one step that makes the check above pass: for every `uses: owner/repo@ref` that is not a commit SHA,
`git ls-remote https://github.com/owner/repo` resolves the tag or branch (an annotated tag to its commit) and the
line becomes `uses: owner/repo@<sha> # <ref>`. It reads the network, so it was tested here only with a stand-in
resolver (every rewrite as expected on `macos.yml`).

```sh
python3 scripts/ci/pin-actions.py            # what it would pin
python3 scripts/ci/pin-actions.py --write    # pin; then add check-actions-pinned.py to the links job
```

## install-risc0.sh

RISC Zero's toolchains for CI, installed with rzup only when missing: in `~/.risc0` cargo-risczero,
r0vm 3.0.5, guest Rust 1.88.0 and C++ 2024.1.5 (bridge/ and checkpoint/); the same with guest Rust
1.91.1 in `~/risc0-home-lez` (LEZ's own guest builds, [`lez/RESULTS.md`](../../lez/RESULTS.md) §8),
kept apart so it does not change `~/.risc0`'s default. Run by CI's `witness-independence` and
`roundtrip` jobs before their builds, with the caches above.

```sh
scripts/ci/install-risc0.sh
```

## witness-independence.sh

The zkVM step of [`bridge/scripts/check.sh`](../../bridge/README.md) in CI: N random valid witnesses
per guest (release, claim), executed in r0vm; fails if a guest's cycle count depends on its witness
(SPEC-v2 §5.2 - a witness-dependent count is a privacy leak, as the one this check found once was).

```sh
scripts/ci/witness-independence.sh 64
```

Run by CI's `witness-independence` job, which passes the witness count.
