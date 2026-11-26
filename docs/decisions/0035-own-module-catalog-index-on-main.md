# ADR 0035: Publish an own module catalog with immutable releases and its index committed to `main`

- **Date:** 21–24 November 2026
- **Status:** accepted; published and installed from
- **Requirements:** RFP-021 S.9, S.10; RFP-022 U.10, S.3

## Context

RFP-021 Supportability #9 requires a module catalog of the team's own, built from the Logos catalog template, and #10
requires releases through `logos-modules-release-action`; RFP-022 repeats the catalog requirement for the bridge. The
release-integrity rules also require signed packages, immutable releases and a reproducible integrity manifest. The
template kept its catalog index in a rolling release which its index workflow rewrote. With immutable releases enabled,
the first rewrite made that release immutable, the asset upload was refused and its tag could never be reused.

## Options considered

1. Publish the modules only through another party's catalog. *Ruled out:* RFP-021 S.9 requires a catalog of the
   team's own; RFP-022 U.10 asks for that catalog itself.
2. Keep the template's rolling `index` release while making releases immutable, including creating that release before
   enabling the setting. *Recorded:* modules/catalog/RELEASE.md, rule 2. The first publication demonstrated that the
   release still became immutable at its first rebuild and could accept no asset; creating it earlier did not solve this.
3. Publish each module through its own repository and immutable signed release, assemble the team's catalog from the
   template, and replace only its rolling-index workflow: build the same index and commit `index/index.json` to the
   catalog's protected `main` by fast-forward.

## Decision

The third. The owner publication step creates the repositories and sets the signing secret, and the pinned action
builds, verifies, signs and publishes the packages and sidecars. The owner enables immutable releases and tag and
`main` protections before running the releases. The catalog's own index workflow builds at pinned tool commits and
writes the index to `main`; no `index` release is created. A changed module receives a new version, and catalog updates
are fast-forwards rather than re-assembly. The initial six modules were published on 24 November; the updated catalog
of 25 November held seven packages and retained the earlier bridge-module release beside its new version.

## Rationale and trade-offs

Immutable releases freeze each package, sidecar and tag, while the index still names the packages' `rootHash` and
SHA-256 for clients to check. Putting only the mutable index on protected `main` removes the contradiction between
immutability and a rolling release without changing the package releases. The integrity manifest records source commits,
signer, roots, hashes and release locations by commands whose output can be read back, and installation was exercised
with signatures required.

The trade-off is a protected, fast-moving `main` carrying generated index commits rather than a wholly immutable
catalog snapshot. Publication also depends on owner-only repository settings that the script prints rather than
performs, and a failed platform variant leaves a partial release. Signing does not make every client safe by default:
the installer must add the declared signer to its own keyring and require signatures.

## Sources

- Commits `509251e6` and `aea28fec`; modules/catalog/RELEASE.md (the owner step, changing a published catalog, the
  template differences and rules 1–6)
- Commit `f6185d75`; releases/RELEASE-INTEGRITY.md (published values and the 25 November update)
- docs/EVIDENCE.md rows 253–254 (immutable releases, index on `main`, read-back and installation)
- modules/catalog/.github/workflows/rebuild-index.yml (the index build and commit)
