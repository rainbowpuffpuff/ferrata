# Publishing the module catalog: one owner step (RFP-021 Supportability #9, #10; RFP-022 Usability #10, Supportability #3)

RFP-021 Supportability #9: "Publish the resulting modules in a module catalog of the team's own, built
from the Logos module catalog template (logos-modules-release-base), so the modules are installable by
Logos clients." #10: "The modules must be built and released to that catalog using
logos-modules-release-action." RFP-022 Supportability #3 is the same for the bridge's modules (its
Usability #10 the catalog itself). This directory is the catalog's content, laid out as the template
(`logos-modules-release-base@33fb564`) prescribes; nothing has been published from this repository, and
nothing can be by anyone but the owner: the one step below creates, signs and publishes everything.

## The one step

```sh
modules/scripts/release-publish.sh ORG CATALOG_REPO SIGNING_KEY_FILE --yes
```

`ORG` is the GitHub organization (say `ferrata-co`), `CATALOG_REPO` the catalog repository (say
`ferrata-modules`), `SIGNING_KEY_FILE` the Ed25519 private key for module signatures — a JWK,
`{"crv":"Ed25519","d":<seed>,"kty":"OKP","x":<pubkey>}`, the format `logos-modules-release-action`'s inline
signing mode reads. Generate it out-of-band and keep it offline, with `lgx keygen` (logos-package's README,
"Generate a Signing Key"):

```sh
lgx keygen --name ferrata-release        # writes ~/.config/logos/keys/ferrata-release.jwk and prints the DID
```

or with OpenSSL 1.1.1 or later (not the LibreSSL of macOS before 3.7, which has no Ed25519), whose DER
encodings end with the 32-byte seed and the 32-byte public key:

```sh
openssl genpkey -algorithm ed25519 -out ferrata-release.pem
d=$(openssl pkey -in ferrata-release.pem -outform DER | tail -c 32 | base64 | tr '+/' '-_' | tr -d '=\n')
x=$(openssl pkey -in ferrata-release.pem -pubout -outform DER | tail -c 32 | base64 | tr '+/' '-_' | tr -d '=\n')
printf '{"crv":"Ed25519","d":"%s","kty":"OKP","x":"%s"}\n' "$d" "$x" > ferrata-release.jwk
```

The script needs bash (3.2 or later, so macOS's own will do), git, jq and python3, and `gh` logged in to an
account that can create repositories in `ORG`. It writes only to a workspace it makes with `mktemp` (under
`TMPDIR` when that is set) and prints its path. Without `--yes` it prepares everything and prints the plan;
with `--dry-run DIR` it also pushes everything into local bare repositories under `DIR` (`file://`
remotes, no network command; the recorded run is [`dry-run-publish/README.md`](dry-run-publish/README.md)).
[`test-release-publish.sh`](../scripts/test-release-publish.sh) runs every path of it, `--yes` included,
against a stub `gh` with no network (CI job `release-publish`).

What it does, in order:

1. **Checks its inputs.** The key file must be the JWK the action's signing expects (an Ed25519 OKP with
   `d` and `x`), and `x` must be the public key of `d` (computed by [`jwk_did.py`](../scripts/jwk_did.py),
   standard-library Python checked against RFC 8032's test vector), or the signer the catalog declares
   would not be the key that signs; it refuses anything else before doing anything, and no message ever
   shows `d`.
2. **Derives the signer's `did:jwk:`** from the public half (`did:jwk:` + base64url of
   `{"crv":"Ed25519","kty":"OKP","x":…}`, the way `lgx` constructs it) and writes it, with the `indexUrl`
   formed from `ORG/CATALOG_REPO`, into the `logos-repo.json` it generates for the published catalog (from
   this directory's copy: identity and description kept, `trustedSigners` and `indexUrl` replaced). The
   private key is read at run time only and never written anywhere.
3. **Exports each module repository** with [`export.sh`](../scripts/export.sh) (the module's directory with
   its staged crate committed; reproducible: the same source commit gives the same commit SHA, its date
   being the source commit's) and **assembles the catalog repository**: the catalog files at its root
   (`logos-repo.json`, the release workflows, a `README.md` with the install instructions of rule 5), each
   module a git submodule pointing at `https://github.com/ORG/<module>`. (`ferrata_updater_module`'s
   export additionally needs lez/methods' risc0-build output, `lez/target` — `cd lez && cargo build
   --locked -p methods`, `RISC0_HOME` as `lez/RESULTS.md` says; on the machine the publish runs from.)
4. **With `--yes`, checks the remote side before creating anything**, read-only: `gh` is there and logged
   in, `ORG` is an organization, and no repository of the seven already holds a different `main` or a
   different version tag. Any failure stops it with nothing created.
5. **Then creates, pushes and sets the secret**, each step idempotent (a rerun after a failure resumes and
   moves nothing): create the seven repositories; push each module's `main` and its `v<version>` tag; push
   the catalog; set the `LOGOS_SIGNING_KEY` Actions secret from the key file on standard input (`gh secret
   set LOGOS_SIGNING_KEY --repo ORG/CATALOG_REPO < KEY`, the `signing_key` the workflows pass in inline
   mode).
6. **Stops, and prints the owner's remaining steps in order.** First the three settings this offline
   setup has no verified `gh` command for, done in the browser (rules 2 and 6 below): immutable releases on
   the catalog and module repositories, a ruleset protecting `refs/tags/*`, organization 2FA. Only then
   the release, which the script prints and does not run:
   `gh workflow run release-all.yml --repo ORG/CATALOG_REPO`. The workflows — pinned, signing set up,
   least-privilege `permissions` per job — do what `logos-modules-release-action` does: build each `.lgx`
   per variant (`darwin-arm64`, `linux-amd64`, `linux-arm64`; at the pinned v1.8 every listed variant
   gets a build leg, and a leg that fails fails alone: the release then publishes the variants that built
   as a partial release, its sidecar listing them in `builtVariants` and the rest in `missingVariants`),
   merge, `lgx verify`, `lgx sign` with the secret, publish the `<module>-v<version>` release with its
   sidecar, then rebuild the rolling `index` release clients read. `ferrata_updater_module` vendors the
   linux-x86_64 builds of the circuits and of rapidsnark (`modules/scripts/stage-updater.sh`), so its
   release is expected to carry `linux-amd64` alone.
7. Afterwards the owner fills [`releases/RELEASE-INTEGRITY.template.md`](../../releases/RELEASE-INTEGRITY.template.md)
   — every value's producing command is in it — and publishes that manifest with the solution.

## Each rule of the lambda-prize release-integrity rules, and how this catalog meets it

The rules are those of the lambda-prize README, "Release integrity and security", added in its commit `80cf11e`
(14 November 2026; the clone's `master` at `a6f2234` does not show it, the commit does). The bridge module handles
funds, so Tier 2 applies to it on top of the baseline. Each row says what the step above does, or what the owner must
do, or what is not met.

| Rule | How this catalog meets it | What the owner must do |
|---|---|---|
| **1. Signed `.lgx`, a declared signer DID (`did:jwk:…`) and `trustedSigners` in `logos-repo.json`** | `_release-module.yml` sets `signing_mode: inline`; the pinned action's release job runs `lgx sign` with the `LOGOS_SIGNING_KEY` secret (a JWK Ed25519 key), `lgx verify`s the signed package, and the sidecar carries `signature` with the signer DID. The published `logos-repo.json` lists that DID under `trustedSigners` (as `{did, name}` — the client reads `did` alone), so clients verify every package against it; `release-publish.sh` derives the DID from the key it is given, so they cannot disagree. `validate-repo`'s shape check (`catalog-format` §2.3) is what the dry run exercised ([`dry-run/log/validate-repo.txt`](dry-run/log/validate-repo.txt)). | Generate the key out-of-band, keep it offline, pass the file to the one step. If the tier's rules ever demand the key never touch CI, switch `_release-module.yml` to `signing_mode: external` with a `signing_command` on a signing rig (HSM, hardware token): the catalog contracts are unchanged. |
| **2. Immutable GitHub releases** (a repository setting) | The `index` release and every `<module>-v<version>` release the workflows create are then read-only to everyone but the owner: the package, its sidecar and the tag cannot be edited or deleted by a token compromise. There is no `gh`/API command for the setting this offline setup could verify, so the script does not pretend to run one. | Enable "immutable releases" in the catalog repository's settings (and in each module repository), once, before the first release. |
| **3. The frozen evaluated `rootHash`** | Every package's Merkle root (`lgx manifest`'s `hashes.root`, recomputed by `lgx verify` against the `.lgx` at install time) is recorded in that release's sidecar, aggregated into `index.json`, and frozen once more in the release-integrity manifest (§5 below), which pins the evaluated build a third party can compare against. A pinned `rootHash` is also the exact form Logos's catalog `includes` use to mend a dependency to one build (`{name, rootHash}` — the same field, so other catalogs can pin ours). | Fill the manifest's `rootHash` rows after the release (the template's command prints them from the sidecars); publish it with the solution so the evaluated roots are frozen in the public record. |
| **4. The release integrity manifest** | [`releases/RELEASE-INTEGRITY.template.md`](../../releases/RELEASE-INTEGRITY.template.md) holds, for each module: the full source commit SHA (the exported commit the submodule points at), the signer DID, the `rootHash`, the catalog's `logos-repo.json` URL, and the release URL — each row with the command that produces the value, so nothing in it is trusted from prose. | Run those commands after the release, fill the placeholders, check the manifest in with the solution. |
| **5. Canonical install instructions** | One URL is the catalog's whole contract: clients add `https://raw.githubusercontent.com/ORG/CATALOG_REPO/refs/heads/main/logos-repo.json` (`logosctl catalog add …`, the package-manager UI, `lgpd`), then `refresh`, `search`, `install <name>`. The step writes them into the published catalog's `README.md`, and how they have been exercised end to end — headless Logos Core installing, loading and calling every module from a served catalog — is recorded in [`install-test/`](install-test/) and [`install-test-relayer/`](install-test-relayer/) ([`README.md`](README.md), "Installed from the catalog"). | Publish the solution with that URL as the install instruction; nothing else (release lists, mirrors) is offered. |
| **6. Account and Actions hygiene** | **2FA** — an organization setting (the step prints it as a browser step); the owner account creating the repositories must have 2FA so no password alone can publish. **Actions pinned by SHA** — the workflows call `logos-co/logos-modules-release-action/.github/workflows/release.yml@bbdc52871dc4305c8007923e1b08738ad2dd6b50 # v1 (= v1.8)`, the commit `v1.8` names (a moved `v1` tag cannot change what a release runs), and `actions/checkout` at the same SHA the pinned action itself uses; `scripts/ci/check-actions-pinned.py` holds the line. **Least-privilege tokens** — no personal token is used anywhere: the workflows run on the built-in `GITHUB_TOKEN`, each job's `permissions:` grants `contents: read` (the module scan) or `contents: write` (the releases) and nothing else, and the only secret is `LOGOS_SIGNING_KEY`. **Protected tags** — the release tags (`<module>-v<version>`) are what the action cuts in the catalog repository; a ruleset protecting `refs/tags/*` keeps them from being moved afterwards. | Enforce 2FA on the organization; add the tag ruleset; keep the signing key offline between rotations. |
| **Baseline 7. An address nobody else can claim** | `ORG` is the step's first argument: the catalog and the module repositories are created under it. | Use a GitHub organisation, not a personal account; do not rename or delete it while the submission is evaluated or after an award. |
| **Baseline 9. Secure development basics** | No key, seed or token is in the repository: the script reads the key file at run time and writes it nowhere under the repository; lockfiles are committed (`Cargo.lock` of every workspace, each module's `flake.lock`). | Keep the throwaway-key rule: the dry run's DID is a throwaway's and is labelled so. |
| **Baseline 10. Safe for evaluators to run** | The module flakes declare no `nixConfig`, so they build without `--accept-flake-config` and without extra substituters (`modules/RESULTS.md`: every Logos dependency from source, the rest from cache.nixos.org). | None beyond keeping it so. |
| **Tier 2.1 Threat model in the repository** | [`docs/trust.md`](../../docs/trust.md) (assets, actors, trust boundaries, every off-chain party) and [`docs/design/bridge.md`](../../docs/design/bridge.md) §17, §20. | **Not met in full:** what a co-installed module or untrusted input inside Logos Core can do to the bridge module is not written yet. |
| **Tier 2.2 Approval enforced in the core module** | **Not met by this module:** `ferrata_bridge_module` takes the wallet seed from its caller with each request (`modules/ferrata_bridge_module/rust-lib/src/lib.rs`) and holds no key; the user's approval and the check of the calling module sit in the caller (the mini-app, or LEZ's wallet module `lez_core`). | Before Tier 2 adoption: move key custody and per-transfer approval into the module (or document that `lez_core` provides them, with a test). |
| **Tier 2.3 No plaintext keys** | The `ferrata` tool reads an encrypted keystore with a password file; the module keeps nothing at rest. | As 2.2: the module's caller must keep the seed encrypted and unlock it per session. |
| **Tier 2.4 Spend caps outside the agent** | Not an agent; caps are enforced on chain (the vault's per-token and global buckets, the LEZ program's caps). | None. |
| **Tier 2.5 Independent review before adoption counts** | Nothing is audited: every module is labelled prototype, unaudited, not for production (`scripts/ci/check-unaudited.sh` checks it in CI). | Adoption counts only after the review or bug-bounty window the prize states. |

The acceptance of this file is not a claim that any of this ran against GitHub: the rows above say
what the published catalog will be. What has run offline is the dry run ([`dry-run-publish/README.md`](dry-run-publish/README.md)):
six of the seven repositories assembled as local bare repositories, the generated `logos-repo.json` with a
throwaway key's DID, the commands that would run (`ferrata_updater_module`'s export was the exception: its
staging wants lez/methods' built guests, absent in this worktree — see §3). The Nix builds of the `.lgx`
packages were skipped there (heavy on a shared laptop); they have run before, per module for
`linux-amd64`, recorded in [`dry-run/`](dry-run/) and [`dry-run-relayer/`](dry-run-relayer/) — the packages'
`rootHash`es that record holds will be superseded by the signed, multi-variant packages the action
publishes, which is why the manifest (§4) is filled from the published sidecars, not from those.
