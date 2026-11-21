# The publish step's dry run: everything the one owner step creates, on local bare repositories

`modules/scripts/release-publish.sh` prepares the whole publication — the generated `logos-repo.json`,
the module repositories, the assembled catalog — and prints the `gh`/`git` commands the owner then runs
([`../RELEASE.md`](../RELEASE.md)). This is what it did on 21 November 2026 with `--dry-run` and a
throwaway key: every repository a **local bare git repository** under the dry-run directory, the catalog's
submodules pointing at them over `file://` remotes, **no network command run**, exit 0:

```sh
openssl genpkey -algorithm ed25519 -out <scratch>/release-key.pem            # throwaway key
# ... converted to the JWK the action's inline signing reads (crv, d, kty, x) ...
MODULES="ferrata_attest_module ferrata_bridge_module ferrata_attestor_module ferrata_monitor_module ferrata_relayer_module" \
  modules/scripts/release-publish.sh example-org ferrata-modules <scratch>/release-key.jwk \
    --dry-run <scratch>/publish-dry
```

(Machine-specific paths — the scratch directory, the key file — are `<scratch>` here; the run printed the
real ones. `MODULES` names five of the six modules: **`ferrata_updater_module` was not exported**, because
its staging wants lez/methods' risc0-build output (`lez/target`, built on the build box by `cd lez &&
cargo build --locked -p methods` with `RISC0_HOME` as `lez/RESULTS.md` says), which this worktree does not
have — a multi-minute guest build, skipped on this shared laptop. The published catalog takes all six.)

## The repositories created (bare, `<scratch>/publish-dry/`)

| Repository | Commit pushed to `main` |
|---|---|
| `ferrata_attest_module.git` | `83629ce86d87878e9f18ec13db3bfbd74f7302a1` (its `v0.1.0` tag would follow) |
| `ferrata_bridge_module.git` | `eaeae1a400f071819b06531995a6c6edbd3a893e` |
| `ferrata_attestor_module.git` | `68637791ee4d3b83e012cfdad26b54ef6ce238d3` |
| `ferrata_monitor_module.git` | `db9ddcfdc9c2e833b52a9be6d68ca53369e4a226` |
| `ferrata_relayer_module.git` | `b6dde8283d38657393662e9fc38a02b0a4ac6139` |
| `ferrata-modules.git` | `ab0b64bada64a9f8f87a949b62191ab789acb73e` (the assembled catalog) |

Each module repository is what `export.sh` writes (the module with its staged crate committed); the commit
ids differ between runs (the staged trees record the source commit's checksums), so what matters is that
the catalog's submodule pointers pin exactly the pushed commits — they did.

## The generated `logos-repo.json` (the assembled catalog's)

Identity and description are this catalog's; `indexUrl` was formed from `example-org/ferrata-modules` and
`trustedSigners` from **the throwaway key's** public half (the owner's real key replaces it; the private key
was read at run time and never written under any repository):

```json
{
  "schemaVersion": 1,
  "name": "ferrata-modules",
  "displayName": "Ferrata modules",
  "description": "Ferrata's Logos core modules (prototype, unaudited): ferrata_attest_module (Ethereum state attestation on LEZ), ferrata_bridge_module (the user side of the ERC-20/ETH bridge), and the daemons as modules: ferrata_attestor_module (rule (B)'s attestor beside a sequencer), ferrata_monitor_module (the watchers' monitor), ferrata_updater_module (the light-client updater), ferrata_relayer_module (the relayer: root imports, checkpoints, releases).",
  "homepage": "https://github.com/rainbowpuffpuff/ferrata",
  "indexUrl": "https://github.com/example-org/ferrata-modules/releases/download/index/index.json",
  "trustedSigners": [
    {
      "did": "did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6ImVHNEF3R080M2JCaVlNNWhVYXkzUDREMGlGdGNyQnBOTUtMZ2JTdHJ0QmMifQ",
      "name": "Ferrata module releases"
    }
  ]
}
```

The `did:jwk:` above is the **dry run's throwaway key** (what `lgx` and Logos's clients construct from a
JWK's public part: base64url of `{"crv":"Ed25519","kty":"OKP","x":…}`, alphabetical, unpadded) — no public
catalog must ever list it; a real owner step replaces the key and therefore the DID. The assembled catalog
also carries the release workflows at its `.github/workflows/` (signing on, the action pinned) and a
`README.md` with the canonical install instructions.

## The commands the owner runs (printed by the dry run; none of them ran)

With the exported repositories under the run's workspace `$WS` (the script prints its path):

```sh
# one module (repeated for each: attestation, bridge, attestor, monitor, relayer — and the updater,
# whose export needs lez/methods built):
gh repo create 'example-org/ferrata_attest_module' --public --description 'Ferrata ferrata_attest_module — Logos Core module (prototype, unaudited)'
git -C "$WS/src/ferrata_attest_module" remote add origin 'https://github.com/example-org/ferrata_attest_module.git'
git -C "$WS/src/ferrata_attest_module" push -u origin main
git -C "$WS/src/ferrata_attest_module" tag -a 'v0.1.0' -m 'ferrata_attest_module 0.1.0, exported from the Ferrata repository'
git -C "$WS/src/ferrata_attest_module" push origin 'v0.1.0'
# the catalog:
gh repo create 'example-org/ferrata-modules' --public --description "Ferrata's Logos module catalog"
git -C "$WS/catalog" remote add origin 'https://github.com/example-org/ferrata-modules.git'
git -C "$WS/catalog" push -u origin main
# the signing key, read once (the command names the key file given to the script):
gh secret set LOGOS_SIGNING_KEY --repo 'example-org/ferrata-modules' < <the signing key's JWK file>
# the settings with no gh command this offline setup could verify, done in the browser (see ../RELEASE.md):
#   immutable releases on; a ruleset protecting refs/tags/*; organization 2FA. Tokens stay least-privilege.
# then the release itself:
gh workflow run release-all.yml --repo 'example-org/ferrata-modules'
```

With `--yes` the script runs the `gh`/`git` commands itself and stops short only of the browser steps.

## What this dry run does not do

It publishes nothing and builds no `.lgx` packages: the packages are built by the release workflows
(`logos-modules-release-action`) from the pushed module repositories, on GitHub's runners. The Nix builds
were skipped here as they were before — the same builder's per-module `linux-amd64` runs (verify, sidecar,
`index.json`) are recorded in [`../dry-run/`](../dry-run/) and [`../dry-run-relayer/`](../dry-run-relayer/)
([`../README.md`](../README.md)); the signed, multi-variant packages the real step publishes supersede
those, which is why the release-integrity manifest is filled from the published sidecars
([`releases/RELEASE-INTEGRITY.template.md`](../../../releases/RELEASE-INTEGRITY.template.md)).
