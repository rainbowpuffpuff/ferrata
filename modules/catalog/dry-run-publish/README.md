# The publish step's dry run: everything the one owner step creates, on local bare repositories

`modules/scripts/release-publish.sh` prepares the whole publication — the generated `logos-repo.json`,
the module repositories, the assembled catalog — and, with `--yes`, creates and pushes it on GitHub and
then stops before the release ([`../RELEASE.md`](../RELEASE.md)). This is what it did on 21 November 2026
(source commit `ec4b932d`) with `--dry-run` and a throwaway key: every repository a **local bare git
repository** under the dry-run directory, the catalog's submodules pointing at them over `file://`
remotes, **no network command run**, exit 0:

```sh
openssl genpkey -algorithm ed25519 -out <scratch>/k.pem            # throwaway key, made into the JWK
# ... with RELEASE.md's recipe (d and x: the last 32 bytes of the DER encodings, base64url) ...
MODULES="ferrata_attest_module ferrata_bridge_module ferrata_attestor_module ferrata_monitor_module ferrata_relayer_module" \
  modules/scripts/release-publish.sh example-org ferrata-modules <scratch>/release-key.jwk \
    --dry-run <scratch>/publish-dry
```

(Machine-specific paths — the scratch directory, the key file — are `<scratch>` here; the run printed the
real ones. `MODULES` names five of the six modules: **`ferrata_updater_module` was not exported**, because
its staging wants lez/methods' risc0-build output (`lez/target`, built on the build box by `cd lez &&
cargo build --locked -p methods` with `RISC0_HOME` as `lez/RESULTS.md` says), which this worktree does not
have. The published catalog takes all six. Every path of the script, `--yes` included, also runs against a
stub `gh` in [`../../scripts/test-release-publish.sh`](../../scripts/test-release-publish.sh), CI job
`release-publish`.)

**All six, on the build box.** The same dry run with no `MODULES` (all six modules), on the build box where
lez/methods' guests are built, 21 November 2026: exit 0 in 21 s, seven bare repositories, the updater's export
staging the linux-x86_64 circuits (`logos-blockchain-circuits-v0.5.7`) and rapidsnark (`v0.0.8`) and the guests,
`ferrata_lez` among them at `816d1fef…` as in `docs/image-ids.md`. The box's copy of the tree carries no git
metadata, so its staged records name the source commit `unknown` and its export commits differ from the laptop's
below; the laptop's run is the record of a source commit.

## The repositories created (bare, `<scratch>/publish-dry/`, each with `HEAD` on `main`)

| Repository | `main` | Tag `v0.1.0` (annotated) |
|---|---|---|
| `ferrata_attest_module.git` | `027a6c0a846bf6efbc4c84c2394918b5e0bbd7cc` | `623cdcda3949a3d93411f5c06fa686722135f917` |
| `ferrata_bridge_module.git` | `71f10e040b0a3d5bbc52780bfa548c55abe0b8ef` | `7f542ee1d94c34fb97c42a922320469ab8108a99` |
| `ferrata_attestor_module.git` | `e4c2746dcab8f7d7da2592b0b6ef2e5334703e5c` | `d605fe2566c52211448f8ebb2cbd8c95565fa762` |
| `ferrata_monitor_module.git` | `2067745dd50cb3926f2f24bb09fe552ddc1d4330` | `9cf0bc071e8b3c31d3a39d5658bb4f9579d2ed86` |
| `ferrata_relayer_module.git` | `b9058b9c4cd3a0c3ccfd26567a55da507ace4dbe` | `1da5db5adde3da68351e053fa94f8f1ed2324d1d` |
| `ferrata-modules.git` | `c0d11ee34d2aa5de6f3e900ca7dd65e66b46ddf3` (the assembled catalog) | — |

Each module repository is what [`export.sh`](../../scripts/export.sh) writes (the module with its staged
crate committed). The export is reproducible: its commit carries a fixed identity and the source commit's
date, so the same source commit gives these same commit ids on any machine (with the same LEZ v0.4.0-rc1
tree beside the repository, which the attestation, bridge and relayer modules vendor parts of); a
different source commit gives different ones, since every staged `STAGED.json` records the source commit
it came from. A clone of the catalog (`git clone file://<scratch>/publish-dry/ferrata-modules.git`, then
`git submodule update --init` with `protocol.file.allow=always`) checks out every module at exactly the
pushed commits.

## The generated `logos-repo.json` (the assembled catalog's)

Identity and description are this catalog's; `indexUrl` was formed from `example-org/ferrata-modules` and
`trustedSigners` from **the throwaway key's** public half (the script first checked that `x` is the public
key of `d`; the owner's real key replaces it; the private key was read at run time and written nowhere):

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
      "did": "did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6ImNTcG93dTdSOTRTMFVJTTU3dFNWLTBzZXRHY1d4MFFjMTJkaHVIOFVCbGMifQ",
      "name": "Ferrata module releases"
    }
  ]
}
```

`python3 -I index.py validate-repo logos-repo.json` (logos-modules-release-tool `94462e6`) on this file:
`OK`. The `did:jwk:` above is the **dry run's throwaway key** (lgx's construction from a JWK's public part:
base64url of `{"crv":"Ed25519","kty":"OKP","x":…}`, alphabetical, unpadded) — no public catalog must ever
list it. Declaring it there is not what makes a client trust it: Logos clients trust only keys in their own
keyring, so the assembled catalog's `README.md` tells users to add the DID (`logosctl key add ferrata --did
…`) and set `signature_policy: require` before installing ([`../RELEASE.md`](../RELEASE.md), rules 1 and
5). The assembled catalog also carries the release workflows at its `.github/workflows/` (signing on, the
action pinned).

## The commands `--yes` runs (printed by the dry run; the `gh` ones not run)

With the exported repositories under the run's workspace `$WS` (the script prints its path), and in a
real run `https://github.com/example-org/<name>.git` in place of the dry run's `file://` URLs:

```sh
# one module (repeated for each: attestation, bridge, attestor, monitor, relayer, and the updater):
gh repo create example-org/ferrata_attest_module --public --description 'Ferrata ferrata_attest_module - Logos Core module (prototype, unaudited)'
git -C "$WS/src/ferrata_attest_module" push https://github.com/example-org/ferrata_attest_module.git refs/heads/main:refs/heads/main
git -C "$WS/src/ferrata_attest_module" push https://github.com/example-org/ferrata_attest_module.git refs/tags/v0.1.0
# the catalog:
gh repo create example-org/ferrata-modules --public --description "Ferrata's Logos module catalog (prototype, unaudited)"
git -C "$WS/catalog" push https://github.com/example-org/ferrata-modules.git refs/heads/main:refs/heads/main
# the signing key, read once from standard input:
gh secret set LOGOS_SIGNING_KEY --repo example-org/ferrata-modules < <the signing key's JWK file>
```

Before any of them, `--yes` checks read-only that `gh` is logged in, that `example-org` is an
organization and that no repository already holds a different `main` or version tag, and creates nothing
if a check fails; a repository that exists is not created again. After them it stops and prints the
owner's remaining steps in order: the browser settings (immutable releases, a ruleset protecting
`refs/tags/*`, organization 2FA), and only then the release, `gh workflow run release-all.yml --repo
example-org/ferrata-modules`, which it does not run.

## What this dry run does not do

It publishes nothing and builds no `.lgx` packages: the packages are built by the release workflows
(`logos-modules-release-action`) from the pushed module repositories, on GitHub's runners. The Nix builds
were skipped here as they were before — the same builder's per-module `linux-amd64` runs (verify, sidecar,
`index.json`) are recorded in [`../dry-run/`](../dry-run/) and [`../dry-run-relayer/`](../dry-run-relayer/)
([`../README.md`](../README.md)); the signed, multi-variant packages the real step publishes supersede
those, which is why the release-integrity manifest is filled from the published sidecars
([`releases/RELEASE-INTEGRITY.template.md`](../../../releases/RELEASE-INTEGRITY.template.md)).
