# Release integrity manifest — module catalog (template)

The lambda-prize solution's release-integrity manifest: what was published, built from where, signed by
whom, and where every number comes from. Fill one copy per catalog release (say
`releases/catalog-0.1.0/RELEASE-INTEGRITY.md`) after `modules/scripts/release-publish.sh ORG CATALOG_REPO
KEY --yes` has published; **every value is produced by its command — do not paste from prose.** Read the
published data back from the releases (the source of truth), not from this repository's scratch.

<!-- placeholders in <angle brackets>; commands assume the repo is checked out and gh is logged into ORG -->

## The catalog

| Value | Place in it | The command that produces it |
|---|---|---|
| Catalog repository | `<ORG>/<CATALOG_REPO>` | from the publish step's arguments |
| `logos-repo.json` URL (the catalog's identity clients add) | `https://raw.githubusercontent.com/<ORG>/<CATALOG_REPO>/refs/heads/main/logos-repo.json` | — |
| Signer DID (`trustedSigners[0].did`) | `<did:jwk:…>` | `curl -fsSL https://raw.githubusercontent.com/<ORG>/<CATALOG_REPO>/refs/heads/main/logos-repo.json \| jq -r '.trustedSigners[0].did'`, or in the clone below: `git show HEAD:logos-repo.json \| jq -r '.trustedSigners[0].did'` |
| Catalog commit the submodules were pushed from | `<full-sha>` | `git ls-remote https://github.com/<ORG>/<CATALOG_REPO>.git refs/heads/main \| awk '{print $1}'` |
| The local assembly it matches | the one-step publish assembled it | `modules/scripts/release-publish.sh <ORG> <CATALOG_REPO> <KEY> --dry-run <dir>` at the same source commit (offline): it lists the same module commits; the catalog commit differs, its `.gitmodules` naming `file://` URLs |

## The modules

One row per module (`<module>-v<version>`), all six: `ferrata_attest_module`, `ferrata_bridge_module`,
`ferrata_attestor_module`, `ferrata_monitor_module`, `ferrata_updater_module`, `ferrata_relayer_module`.

| Module | Version | Release URL | Built from (source commit SHA) | rootHash | sha256 of the `.lgx` | Variants built |
|---|---|---|---|---|---|---|
| `<module>` | `<version>` | `https://github.com/<ORG>/<CATALOG_REPO>/releases/tag/<module>-v<version>` | `<full-sha>` | `<rootHash>` | `<sha256>` | `<builtVariants>` |

The producing commands, per module `<module>-v<version>`:

```sh
# the sidecar of the published release: rootHash, sha256, size, builtVariants, missingVariants,
# the manifest, and (signed packages) the signature with the signer DID
gh release download '<module>-v<version>' --repo '<ORG>/<CATALOG_REPO>' --pattern 'sidecar.json' \
  --output - | jq '{rootHash, sha256, builtVariants, missingVariants, signerDid: .signature.did}'

# the commit the built package came from: the catalog's submodule pointer (the gitlink in its tree)
git clone -q https://github.com/<ORG>/<CATALOG_REPO> && cd <CATALOG_REPO>
git ls-tree HEAD submodules/<module>            # 160000 commit <sha>  submodules/<module> — the pinned commit

# that commit is what modules/scripts/export.sh wrote from this repository, and export.sh is reproducible
# (fixed identity, the source commit's date): read the source commit its STAGED.json records (`commit`;
# rust-lib/staged/ for attest and bridge, attestor/, monitor/, vendor/ for the relayer and the updater),
# check it out in a clean clone of this repository, with LEZ v0.4.0-rc1 (1806e87) beside it
# (scripts/fetch-upstream.sh; the updater also needs lez/methods built), and export again, keeping any
# SOURCE_DATE_EPOCH or FERRATA_COMMIT of the shell (a Nix shell sets the first) out of it:
git submodule update --init submodules/<module>
git -C submodules/<module> show HEAD:rust-lib/<staged|attestor|monitor|vendor>/STAGED.json | jq -r .commit
git -C <ferrata checkout> checkout <that commit>
env -u SOURCE_DATE_EPOCH -u FERRATA_COMMIT <ferrata checkout>/modules/scripts/export.sh <module> <scratch>/<module>
git -C <scratch>/<module> rev-parse HEAD        # must equal the submodule pointer above

# the signature: lgx (logos-package, pinned) checks the package's content hashes and its signature and
# reports the signer DID, and whether that DID is in the keyring it is given; give it a keyring holding
# only the catalog's declared signer:
nix build github:logos-co/logos-package/4bdd9041ebfbd06c2b22134afdcd27fce8a43f3c#lgx -o lgx-tool
./lgx-tool/bin/lgx keyring add ferrata <the trustedSigners[0].did above> --dir <scratch>/keyring
./lgx-tool/bin/lgx verify <module>-<version>.lgx --keyring-dir <scratch>/keyring
```

Checks the manifest itself should record, once filled:

- [ ] every row's DID equals the catalog's `trustedSigners[0].did` (grep both)
- [ ] every `rootHash` equals the one in that release's sidecar (the command above prints it)
- [ ] every source commit SHA is the catalog repository's submodule pointer, not a paste from the build log
- [ ] the URLs resolve (the release exists at `releases/tag/<module>-v<version>`, assets `.lgx` + `sidecar.json`)
- [ ] `indexUrl` (`https://raw.githubusercontent.com/<ORG>/<CATALOG_REPO>/refs/heads/main/index/index.json`) lists all six:

```sh
curl -fsSL 'https://raw.githubusercontent.com/<ORG>/<CATALOG_REPO>/refs/heads/main/index/index.json' \
  | jq '{repositoryName, packages: [.packages[].name]}'     # the index the clients read
```

## Integrity notes

- The `.lgx` packages are signed (`lgx sign`, inline, key `LOGOS_SIGNING_KEY`); `lgx verify` reports the
  signer DID, and the catalog's `logos-repo.json` declares it under `trustedSigners`. That declaration does
  not make Logos clients trust it: they trust only the keys in the installer's own keyring, and by default
  (`signature_policy: warn`) install an unsigned or unknown-signer package with a warning. This manifest is
  where a user checks the DID before adding it (`logosctl key add ferrata --did <DID>`, then
  `signature_policy: require` in the daemon configuration; with `lgpm`, `lgx keyring add` and
  `--require-signatures`). Each rule's mapping: `modules/catalog/RELEASE.md`.
- The releases are immutable (repository setting) and the tags protected (ruleset) — say when, and by
  whom, both were enabled: `<owner, date>`.
- The previous, unsigned linux-amd64 dry-run sidecars (`modules/catalog/dry-run/`,
  `dry-run-relayer/`) are records of the same builder; their rootHashes will not match the signed,
  multi-variant packages published here — this manifest supersedes them.
