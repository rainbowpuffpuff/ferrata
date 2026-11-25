# Release integrity manifest — module catalog `landclimber/ferrata-modules`: modules v0.1.0, then `ferrata_bridge_module` 0.1.1 and the mini-app

The lambda-prize release-integrity manifest (rule 4) for the first publication of Ferrata's module catalog, filled
from [`RELEASE-INTEGRITY.template.md`](RELEASE-INTEGRITY.template.md) on 24 November 2026. **Every value below is the
output of the template's command for it**, run read-only against the published data (raw files, `git ls-remote`, a
clone, `gh release view` / `gh release download`); the commands and their outputs, in order, are
[`integrity-commands.log`](../modules/catalog/install-test-landclimber-2026-11-24/integrity-commands.log) (`WORK` is
the scratch directory). The install from this catalog into a headless Logos Core, signatures required, is
[`modules/catalog/install-test-landclimber-2026-11-24/`](../modules/catalog/install-test-landclimber-2026-11-24/README.md).

The catalog's update of 25 November 2026 (`ferrata_bridge_module` 0.1.1 and the mini-app `ferrata_bridge_ui` 0.1.0,
the rest unchanged) has its own section, [The update of 25 November 2026](#the-update-of-25-november-2026), filled
the same way on 25 November; its commands and outputs:
[`integrity-commands.log`](../modules/catalog/install-test-landclimber-2026-11-25/integrity-commands.log), and the
install of all seven packages from the updated catalog (a headless Logos Core with signatures required, and Basecamp
0.4.0-rc.2's own Package Manager):
[`modules/catalog/install-test-landclimber-2026-11-25/`](../modules/catalog/install-test-landclimber-2026-11-25/README.md).

## The catalog

| Value | Place in it | The command that produces it |
|---|---|---|
| Catalog repository | `landclimber/ferrata-modules` (organization `landclimber`) | the publish step's arguments: `modules/scripts/release-publish.sh landclimber ferrata-modules KEY --yes` |
| `logos-repo.json` URL (the catalog's identity clients add) | `https://raw.githubusercontent.com/landclimber/ferrata-modules/refs/heads/main/logos-repo.json` | — |
| Signer DID (`trustedSigners[0].did`) | `did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6ImNLVmpDSnFiWEFqYTBtenc1VFBOODhqWUs2QXkwTmtpRWNldUduekN6bGsifQ` | `curl -fsSL <the URL above> \| jq -r '.trustedSigners[0].did'`; the same from the clone, `git show HEAD:logos-repo.json \| jq -r '.trustedSigners[0].did'` |
| `indexUrl` (the index clients read) | `https://raw.githubusercontent.com/landclimber/ferrata-modules/refs/heads/main/index/index.json` | `git show HEAD:logos-repo.json \| jq -r .indexUrl` |
| Catalog commit the submodules were pushed from | `c9bdb6644b38bc333dd39110df6eeadd435e396d` | `git rev-list --max-parents=0 HEAD` in the clone (the commit `release-publish.sh` pushed) |
| The catalog's `main` today | `9738dd099435979df64eb746905ab73344d2cfc7` | `git ls-remote https://github.com/landclimber/ferrata-modules.git refs/heads/main`; its history (`git log --reverse --format='%h %an: %s'`): `c9bdb66` ferrata-release-publish: Ferrata module catalog: 6 modules, signing on, action pinned (assembled by modules/scripts/release-publish.sh); `6c85ede` rainbowpuffpuff: Index on main: rebuild-index.yml commits index/index.json; indexUrl points at it; `4408d28` rainbowpuffpuff: rebuild-index.yml: lgx built from logos-package at a pinned commit (4bdd9041, the revision the last run used); `9738dd0` github-actions[bot]: index: 6 package(s), rebuilt by rebuild-index.yml |
| Submodules and `.gitmodules` since the publication | unchanged | `git diff --quiet c9bdb664 HEAD -- submodules .gitmodules` |
| Source commit (every export's `STAGED.json`) | `98375f82c754f2fa4a56e20418687a89b321228f`, on public `main` of https://github.com/rainbowpuffpuff/ferrata | per module, below |
| The local assembly it matches | the six module commits below | `release-publish.sh landclimber ferrata-modules KEY --dry-run DIR` at `98375f82` (offline), then `git -C DIR/<module>.git rev-parse refs/heads/main`: equal to each submodule pointer (the transcript); the catalog commit differs (`file://` URLs in its `.gitmodules`) |
| What the index lists | `{"repositoryName":"ferrata-modules","packages":["ferrata_updater_module","ferrata_relayer_module","ferrata_monitor_module","ferrata_bridge_module","ferrata_attestor_module","ferrata_attest_module"]}` | `curl -fsSL <indexUrl> \| jq -c '{repositoryName, packages: [.packages[].name]}'` |

## The modules

| Module | Version | Release URL | Built from: module commit (the catalog's submodule pointer) | Source commit (its `STAGED.json`) | rootHash | sha256 of the `.lgx` | Variants built | Immutable | `lgx verify`, keyring = the DID alone | Installed package's rootHash |
|---|---|---|---|---|---|---|---|---|---|---|
| `ferrata_attest_module` | `0.1.0` | https://github.com/landclimber/ferrata-modules/releases/tag/ferrata_attest_module-v0.1.0 | `06580ba155284ee3bf1759c07bcc74e1a062ad75` | `98375f82c754f2fa4a56e20418687a89b321228f` | `2a879a4df2278c3d09a366d1a77a0d89ffe8855b8df2d4f292a80571f25a8736` | `28e2b00194ab9aba7b32d9dd82a846fe87499c98d61ecf242eea9f3f7febe92b` | darwin-arm64, linux-amd64, linux-arm64 | true | valid, signer trusted (box step `lgx-verify-ferrata_attest_module`) | = sidecar (box step `roothash-ferrata_attest_module`) |
| `ferrata_bridge_module` | `0.1.0` | https://github.com/landclimber/ferrata-modules/releases/tag/ferrata_bridge_module-v0.1.0 | `1728fa13777d92de8e2fcb9dfff1eea20f558a54` | `98375f82c754f2fa4a56e20418687a89b321228f` | `0e46fbecf4c39caa9880bac11bc53c9abab4a4c703b9af9fa34db6a4f166fdd8` | `b160668aa824c8f08dc42d20a071c4f92173625a521671b7e4ec3028acdb7f7d` | darwin-arm64, linux-amd64, linux-arm64 | true | valid, signer trusted (box step `lgx-verify-ferrata_bridge_module`) | = sidecar (box step `roothash-ferrata_bridge_module`) |
| `ferrata_attestor_module` | `0.1.0` | https://github.com/landclimber/ferrata-modules/releases/tag/ferrata_attestor_module-v0.1.0 | `9937299ceca69c365995c4761e4e7481d213d298` | `98375f82c754f2fa4a56e20418687a89b321228f` | `85dcae24795b94e5e21484f43bf367d79fe7e6cc11071f1379d48203ad6f2359` | `8bef9d31d44a806c3b6a0a53c0a37203dac50bcb5c4e0b03d2fc9ccda9a84dd8` | darwin-arm64, linux-amd64, linux-arm64 | true | valid, signer trusted (box step `lgx-verify-ferrata_attestor_module`) | = sidecar (box step `roothash-ferrata_attestor_module`) |
| `ferrata_monitor_module` | `0.1.0` | https://github.com/landclimber/ferrata-modules/releases/tag/ferrata_monitor_module-v0.1.0 | `834f0d893db29f289e555f463ce84d471490cf74` | `98375f82c754f2fa4a56e20418687a89b321228f` | `161a90a4985bd0b1351635ab5031aef70a1a0390b3c9657bb2d4d9e02a18a301` | `667107ffdfe9322e249a4b386d8b05a5538c7d10cf72e6ab9ca5e416ae8bb062` | linux-amd64, linux-arm64 (missing: darwin-arm64) | true | valid, signer trusted (box step `lgx-verify-ferrata_monitor_module`) | = sidecar (box step `roothash-ferrata_monitor_module`) |
| `ferrata_updater_module` | `0.1.0` | https://github.com/landclimber/ferrata-modules/releases/tag/ferrata_updater_module-v0.1.0 | `c3266a324df29984fb992f4981be1dcc05395ad6` | `98375f82c754f2fa4a56e20418687a89b321228f` | `0f23ee4c1eeabbfaa950aec7ab83ecb487a8cb44ae54f8101aac6257c651f867` | `e693c9b2c966bc80dd22d5d994a8533c579bc8cbeac6d4797300c0ec81eda736` | linux-amd64 (missing: darwin-arm64, linux-arm64) | true | valid, signer trusted (box step `lgx-verify-ferrata_updater_module`) | = sidecar (box step `roothash-ferrata_updater_module`) |
| `ferrata_relayer_module` | `0.1.0` | https://github.com/landclimber/ferrata-modules/releases/tag/ferrata_relayer_module-v0.1.0 | `4560ecdb11b1e92f77aef766e0b635cbcc059aaf` | `98375f82c754f2fa4a56e20418687a89b321228f` | `aac37f8b4a5ba583940c8f2b2ff5a258740a4dc618eb4a20212262f5967b846f` | `53750b24f07b311343bcbde1bfe727d92258c8aa3a075120fdf203806c3baf64` | darwin-arm64, linux-amd64, linux-arm64 | true | valid, signer trusted (box step `lgx-verify-ferrata_relayer_module`) | = sidecar (box step `roothash-ferrata_relayer_module`) |

The producing commands, per module `<module>-v0.1.0` (as run; the transcript has each output):

```sh
gh release view '<module>-v0.1.0' --repo landclimber/ferrata-modules --json url,isImmutable           # URL, Immutable
git -C ferrata-modules ls-tree HEAD submodules/<module>                                                # module commit
curl -fsSL https://raw.githubusercontent.com/landclimber/<module>/<module commit>/rust-lib/<staged|attestor|monitor|vendor>/STAGED.json | jq -r .commit
gh release download '<module>-v0.1.0' --repo landclimber/ferrata-modules --pattern sidecar.json --output - \
  | jq -c '{rootHash, sha256, builtVariants, missingVariants, signerDid: .signature.did}'               # rootHash, variants, signer
gh release download '<module>-v0.1.0' --repo landclimber/ferrata-modules --pattern '*.lgx' --dir WORK/pkgs
sha256sum WORK/pkgs/<module>-0.1.0.lgx                                                                  # = the sidecar's sha256
curl -fsSL '<indexUrl>' | jq -c --arg t '<module>-v0.1.0' '.packages[].versions[] | select(.publisherRef == $t) | {rootHash, sha256}'
# on the build box (the install test): lgx of logos-package 4bdd9041, a keyring holding only the DID above
lgx keyring add ferrata <DID> --dir keyring && lgx verify <module>-0.1.0.lgx --keyring-dir keyring
```

## The template's checks, filled

- [x] Every row's DID equals the catalog's `trustedSigners[0].did`: the raw URL's, the clone's and the six sidecars'
  `signature.did` are one value, the DID above, which is also the one the owner's key gave when it was made
  ([`read-back-checks.txt`](../modules/catalog/install-test-landclimber-2026-11-24/read-back-checks.txt): "signer DID
  equals EXPECTED_DID").
- [x] Every `rootHash` above is the release sidecar's (`.rootHash`), equal to the sidecar manifest's `hashes.root`,
  to `index.json`'s entry for that release, and to the installed package's `hashes.root` (`logosctl package ls`).
- [x] Every source commit is read from the module repository at the catalog's submodule pointer (`STAGED.json`),
  not from a build log; the offline dry run at that commit gave the same six module commits.
- [x] The URLs resolve: `gh release view` returns each release; each has two assets, the `.lgx` and `sidecar.json`,
  and the downloaded `.lgx`'s sha256 equals the sidecar's.
- [x] `indexUrl` lists all six, `repositoryName` `ferrata-modules` (the catalog's `name`).

## The update of 25 November 2026

`modules/scripts/release-publish.sh landclimber ferrata-modules - --update --modules 'ferrata_bridge_module
ferrata_bridge_ui' --yes` from public `main` `f9623cfd` moved the bridge module's repository to 0.1.1 (a
fast-forward), created the mini-app's, and pushed the catalog's new `main` (a fast-forward); the owner then ran the two
release workflows. Every value below is a command's output, read back on 25 November 2026
([`integrity-commands.log`](../modules/catalog/install-test-landclimber-2026-11-25/integrity-commands.log); `WORK` the
scratch directory, `DRY` the update's offline dry run).

| Value | Place in it | The command that produces it |
|---|---|---|
| Signer DID | unchanged: the DID above, from the raw URL and from the clone | as above |
| Catalog commit the update pushed | `32fbcbc7568753118962e0eda5e0b372644e6cfa`, on `9738dd09` | `git log --reverse --format='%h %an %ad: %s' 9738dd09..HEAD` in the clone: `32fbcbc` ferrata-release-publish 2026-11-25 02:53: Ferrata module catalog: ferrata_bridge_module 0.1.1; ferrata_bridge_ui 0.1.0 added (modules/scripts/release-publish.sh --update); `ad5a6df` github-actions[bot] 2026-11-25 03:59: index: 7 package(s), rebuilt by rebuild-index.yml; `3292b16` github-actions[bot] 2026-11-25 04:08: index: 7 package(s), rebuilt by rebuild-index.yml |
| The catalog's `main` today | `3292b160c6bb496a87b32b7c15547d18480f67ca` | `git ls-remote https://github.com/landclimber/ferrata-modules.git refs/heads/main` |
| What the update changed | `.github/workflows/release-ferrata_bridge_ui.yml`, `.gitmodules`, `submodules/ferrata_bridge_module`, `submodules/ferrata_bridge_ui` (the other five pointers unchanged) | `git diff --name-only 9738dd09 HEAD -- submodules .gitmodules .github` |
| Source commit | `f9623cfd90ae0af852d2dc7000e90821e8381f06`, public `main` of https://github.com/rainbowpuffpuff/ferrata | per module, below |
| The local assembly it matches | the two module commits and the catalog commit `32fbcbc7`, each equal to the published one (and so their trees) | `release-publish.sh landclimber ferrata-modules - --update --modules 'ferrata_bridge_module ferrata_bridge_ui' --dry-run DRY` at `f9623cfd` (offline, started read-only from the live repositories), then `git -C DRY/<repository>.git rev-parse refs/heads/main` and `rev-parse 'refs/heads/main^{tree}'` |
| What the index lists | `{"repositoryName":"ferrata-modules","packages":["ferrata_bridge_ui 0.1.0","ferrata_bridge_module 0.1.1","ferrata_bridge_module 0.1.0","ferrata_updater_module 0.1.0","ferrata_relayer_module 0.1.0","ferrata_monitor_module 0.1.0","ferrata_attestor_module 0.1.0","ferrata_attest_module 0.1.0"]}`: seven packages, eight versions | `curl -fsSL <indexUrl> \| jq -c '{repositoryName, packages: [.packages[] \| .name as $n \| .versions[] \| "\($n) \(.manifest.version)"]}'` |
| The release runs | 45543214624 (`Release ferrata_bridge_module`, 03:55:02Z to 04:07:55Z) and 45543219299 (`Release ferrata_bridge_ui`, 03:55:05Z to 03:58:34Z): `workflow_dispatch` at `32fbcbc7`, conclusion success; jobs setup, build (darwin-arm64), build (linux-amd64), build (linux-arm64) and release: success (already-published: skipped) | `gh run view <id> --repo landclimber/ferrata-modules --json workflowName,headSha,event,conclusion,createdAt,updatedAt,jobs` |
| The action | `uses: logos-co/logos-modules-release-action/.github/workflows/release.yml@bbdc52871dc4305c8007923e1b08738ad2dd6b50 # v1 (= v1.8)` | `git show 32fbcbc7:.github/workflows/_release-module.yml \| grep logos-modules-release-action` |
| The release tags | `ferrata_bridge_ui-v0.1.0` at `32fbcbc7`; `ferrata_bridge_module-v0.1.1` at `ad5a6df4` (the action tags `main` as it is when it publishes; between the two, an index commit changed `index/` alone; the action's step that re-points a tag at the built commit was then refused by the tag ruleset: GH013 in run 45543214624's log) | `git rev-list -n 1 refs/tags/<tag>`; `git diff --quiet ad5a6df4 HEAD -- submodules .gitmodules .github logos-repo.json` |

| Module | Version | Release URL | Built from: module commit (the catalog's submodule pointer) | Source commit | rootHash | sha256 of the `.lgx` | Variants built | Immutable | `lgx verify`, keyring = the DID alone | Installed package's rootHash |
|---|---|---|---|---|---|---|---|---|---|---|
| `ferrata_bridge_module` | `0.1.1` | https://github.com/landclimber/ferrata-modules/releases/tag/ferrata_bridge_module-v0.1.1 | `34ee2fcf5f6fc1214b4586eaca6c9d6c25672f8c` (on `1728fa13`, the 0.1.0 commit) | `f9623cfd90ae0af852d2dc7000e90821e8381f06` (its `STAGED.json` and its commit message agree) | `92024bf12434e1dc35051d7f8ee9a467ad3eb16df9ea9e36e9d929cc92253463` | `1ab1b2ff43d88f6f7171e83c5c31ab28f5cc9c270547298ef68d0cf7233582d1` | darwin-arm64, linux-amd64, linux-arm64 | true | valid, signer trusted (box step `lgx-verify-ferrata_bridge_module`) | = sidecar (box step `roothash-ferrata_bridge_module`; Basecamp 0.4.0-rc.2's install, `installed.tsv`) |
| `ferrata_bridge_ui` | `0.1.0` | https://github.com/landclimber/ferrata-modules/releases/tag/ferrata_bridge_ui-v0.1.0 | `972892161b21069ead711048443d687410eca1ce` (a new repository) | `f9623cfd90ae0af852d2dc7000e90821e8381f06` (its commit message: a QML-only module has no `STAGED.json`) | `f9d203fbaf0b5de328478d5a948f2326e84ff357b7090db3aba37187f838c8f4` | `fd65af7325ca331cb21580d85fa689d9aa86ab600bbc5501c9fb079dc6a48a8a` | darwin-arm64, linux-amd64, linux-arm64 | true | valid, signer trusted (box step `lgx-verify-ferrata_bridge_ui`) | = sidecar (box step `roothash-ferrata_bridge_ui`; Basecamp 0.4.0-rc.2's install, `installed.tsv`) |

The producing commands, per release `<module>-v<version>` (as run; the transcript has each output): those of the first
publication above, with the source commit from the module commit's message where there is no `STAGED.json`, as the
template says (`git log -1 --format=%s <pointer>` in a blobless clone of the module repository:
"ferrata_bridge_module 0.1.1, exported from the Ferrata repository at f9623cfd…", "ferrata_bridge_ui 0.1.0, exported
from the Ferrata repository at f9623cfd…"). `ferrata_bridge_module-v0.1.0` is still published and offered beside
0.1.1, unchanged: its sidecar's `rootHash` and `sha256` are the row above.

- [x] Both sidecars' `signature.did` equal the catalog's `trustedSigners[0].did`, the DID above (the read-back,
  [`read-back-checks.txt`](../modules/catalog/install-test-landclimber-2026-11-25/read-back-checks.txt): 212 checks
  passed, 0 failed).
- [x] Each `rootHash` is its release sidecar's, equal to the sidecar manifest's `hashes.root`, to `index.json`'s entry,
  and to the installed package's (`logosctl package ls`; Basecamp's data directory).
- [x] Each source commit is read from the module repository at the catalog's pointer, and the update's offline dry run
  at that commit gave the same module commits.
- [x] The URLs resolve: each release has two assets, the `.lgx` (sha256 and size equal to the sidecar's) and
  `sidecar.json`; both releases Immutable.
- [x] `indexUrl` lists seven packages, eight versions, `repositoryName` `ferrata-modules`.

## Integrity notes

- **Signatures.** The `.lgx` packages are signed in the release workflow (`lgx sign`, inline, the `LOGOS_SIGNING_KEY`
  Actions secret); the catalog's `logos-repo.json` declares the DID under `trustedSigners`. Logos clients do not trust
  that declaration by itself: only keys in the installer's own keyring count, and by default (`signature_policy:
  warn`) an unsigned or unknown-signer package installs with a warning. Check the DID here, then
  `logosctl key add ferrata --did did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6ImNLVmpDSnFiWEFqYTBtenc1VFBOODhqWUs2QXkwTmtpRWNldUduekN6bGsifQ --display-name "Ferrata module releases"` and
  `signature_policy: require` in the daemon configuration (with `lgpm`: `lgx keyring add` and `--require-signatures`;
  Basecamp keeps its own keyring). The install test shows both halves: refused before the key is added, installed after.
- **Immutable releases and protected tags,** enabled by the owner on 24 November 2026, before the first module release
  (16:58:29Z): immutable releases on all seven repositories (`GET repos/landclimber/<repo>/immutable-releases`:
  `enabled: true`; the API records no time for the setting); every module release shows Immutable. Rulesets on the
  catalog, created 16:29:04Z (`GET repos/landclimber/ferrata-modules/rulesets/<id>`: `created_at`): `release-tags`
  (all tags: updates, deletions and force-pushes blocked, no bypass) and `main` (the default branch: deletion and
  force-pushes blocked; pushes allowed, `rebuild-index.yml` commits the index to it). Organization 2FA: required
  (`GET orgs/landclimber`: `two_factor_requirement_enabled: true`).
- **The update's settings.** The new repository `landclimber/ferrata_bridge_ui` has immutable releases on (`GET
  repos/landclimber/ferrata_bridge_ui/immutable-releases`: `enabled: true`), as the seven others still do; both new
  releases show Immutable, and the tag ruleset and the organisation's 2FA are unchanged (the read-back). A client
  that trusted the key on 24 November needs nothing new: the update is signed by the same DID.
- **The index is not a release.** The base catalog's rolling `index` release (created before immutability was on, as
  the prize rule's PR #189 advises) turned immutable at the first rebuild, which republished it, and never held an
  asset; an immutable release's tag cannot be reused. The index is `index/index.json` on the catalog's `main`,
  committed by the catalog's own `rebuild-index.yml` (run 45454593821, commit `9738dd09`); clients follow
  `indexUrl` there. The `index` release is left as it is (the tag ruleset forbids deleting its tag). Why, and what
  else was weighed: [`modules/catalog/RELEASE.md`](../modules/catalog/RELEASE.md), rule 2.
- **Partial releases.** `ferrata_monitor_module` carries `linux-amd64` and `linux-arm64`: its `darwin-arm64` leg was
  never picked up by a GitHub-hosted macOS runner ("The job was not acquired by Runner of type hosted even after
  multiple attempts") and was cancelled after 37 minutes; the action then published the variants that built, as it
  does by design. `ferrata_updater_module` carries `linux-amd64` alone, by design: it vendors linux-x86_64 builds of
  the circuits and rapidsnark, and its `linux-arm64` and `darwin-arm64` legs failed linking those objects (`error
  adding symbols: file in wrong format`; `archive member 'poc_local.o' … is not mach-o or llvm bitcode file`). The
  other four carry all three variants. So run 45438763159 (`release-all.yml`,
  logos-modules-release-action `bbdc5287` = v1.8) ended "failure" although all six releases were published. A
  missing variant is shipped as a new version, never by replacing a release.
- The earlier unsigned `linux-amd64` dry-run sidecars (`modules/catalog/dry-run/`, `dry-run-relayer/`) are records of
  the same builder; their rootHashes differ from these signed, multi-variant packages. This manifest supersedes them.
