# Installed from the published catalog after its update: `landclimber/ferrata-modules`, 25 November 2026

The catalog's update of 25 November added the mini-app `ferrata_bridge_ui` 0.1.0 and `ferrata_bridge_module` 0.1.1
(released by `logos-modules-release-action` v1.8 on GitHub's runners, runs 45543214624 and 45543219299; the index on
`main` lists seven packages, eight versions; [`releases/RELEASE-INTEGRITY.md`](../../../releases/RELEASE-INTEGRITY.md),
"The update of 25 November 2026"). This record installs **every package of the bridge from the published catalog**,
the URL users add (`https://raw.githubusercontent.com/landclimber/ferrata-modules/refs/heads/main/logos-repo.json`,
catalog `main` `3292b160`), into three Logos clients, signatures checked by each client:

| Client | What it shows | Result |
|---|---|---|
| A headless Logos Core (`logosctl` of logos-logoscore-cli `66d098d`, portable), `signature_policy: require`, the catalog's DID the only key | the seven packages (the six core modules, `ferrata_bridge_module` at 0.1.1, and the mini-app) installed by name from the catalog; first the refusals: the catalog's bridge package before the key is trusted, then an unsigned, an other-signer and a tampered copy of it | **74 of 74 steps as expected** ([`steps.tsv`](steps.tsv), table below) |
| Logos Basecamp `b642a9a`, its portable standalone mode (`#dev-portable`), as [`../install-test-plus/basecamp-open.sh`](../install-test-plus/basecamp-open.sh) runs it | the two published packages (downloaded from their releases, sha256 equal to their sidecars') installed by Basecamp's own `lgpm`, the mini-app opened, the Module Inspector loading the bridge module, the view's call reaching it | **the probe 3 of 3** ([`basecamp/`](basecamp/)) |
| **Logos Basecamp 0.4.0-rc.2**, the release AppImage testers run (`pre-release-4cea4bb-1433`, sha256 `e59e14cd…`), on a virtual screen | its own Package Manager: the catalog added by its URL (Settings, Package Repositories), its seven packages listed, `ferrata_bridge_module` offered at 0.1.1 and 0.1.0; asked before installing a package signed by a key it did not yet trust (its prompt: Cancel, a one-time **Install** without trusting, or **Trust & install**; Cancel installed nothing), then **Trust & install**; the mini-app installed under the same key; the app opened, the Module Inspector showing the bridge module loaded, the view's call answered by the module | **every step as expected, 21 screens** ([`basecamp-0.4.0-rc.2/`](basecamp-0.4.0-rc.2/)) |

All three ran on 25 November 2026 on the Modal build box (Linux x86-64 under gVisor, Nix 2.28.3, a fresh container;
`versions.txt`). Not shown here: an install on macOS or on `linux-arm64` (this box is Linux x86-64; the releases carry
`darwin-arm64` and `linux-arm64` for every package but the monitor's macOS and the updater's two, as on 24 November),
and anything on a network: the mini-app was opened, not used for a transfer (its flows in Basecamp are recorded with
the mini-app, [`modules/ferrata_bridge_ui/`](../../ferrata_bridge_ui/README.md)).

## A. Headless Logos Core: the seven packages, signatures required

The committed [`../install-test-plus/box-install.sh`](../install-test-plus/box-install.sh), unmodified (sha256
`12d9d2e6058a5bd5…`), with the seven releases read back from the catalog (each `TAG/ASSET/ROOTHASH` from its
sidecar, [`read-back-checks.txt`](read-back-checks.txt)) and `REC` this directory:

```sh
REC=modules/catalog/install-test-landclimber-2026-11-25 modules/catalog/install-test-plus/box-install.sh \
  landclimber ferrata-modules did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6ImNLVmpDSnFiWEFqYTBtenc1VFBOODhqWUs2QXkwTmtpRWNldUduekN6bGsifQ \
  ferrata_attest_module-v0.1.0/ferrata_attest_module-0.1.0.lgx/2a879a4d… ferrata_attestor_module-v0.1.0/…/85dcae24… \
  ferrata_bridge_module-v0.1.1/ferrata_bridge_module-0.1.1.lgx/92024bf1… ferrata_monitor_module-v0.1.0/…/161a90a4… \
  ferrata_relayer_module-v0.1.0/…/aac37f8b… ferrata_updater_module-v0.1.0/…/0f23ee4c… ferrata_bridge_ui-v0.1.0/ferrata_bridge_ui-0.1.0.lgx/f9d203fb…
```

| What | Result |
|---|---|
| Downloaded, `lgx verify` | the seven `.lgx` from their releases, each verified by lgx (logos-package `4bdd9041`) against a keyring holding only the catalog's DID: structure valid, signature valid, signer trusted |
| Catalog | default catalog disabled; `catalog add` the raw `logos-repo.json` URL; `catalog source http`; `refresh`; `search ferrata` finds the seven |
| Refused before the key is trusted | `install ferrata_bridge_module` from the catalog: `ferrata_bridge_module: Package signed by untrusted key: did:jwk:…` (the catalog's `trustedSigners` are not trusted by themselves) |
| Refused after the key is trusted | the published bridge package without its `manifest.sig`: `Package is unsigned and signature policy requires signatures`; the same package signed by a fresh key under the signer name `Ferrata module releases`: `Package signed by untrusted key` (another DID); one bit flipped in its `linux-amd64` plugin: `Package validation failed: Content hash mismatch`. Nothing installed after the refusals |
| Installed | each of the seven by name (`install <name> -y`); `package show ferrata_bridge_module` names the two versions the catalog offers, `["0.1.1","0.1.0"]`, and `install` installs 0.1.1; each installed `hashes.root` equal to its release sidecar's `rootHash` |
| Loaded and called | the six core modules listed as `core`, loaded, each `version` reply naming its release's version (`ferrata_bridge_module`: `0.1.1`, `staged.commit` `f9623cfd`, the source commit); the mini-app listed as a `ui_qml` package in the session's `plugins/` (a headless Logos Core has no view to show it in: Basecamp is the host of ui modules, B and C) |

`box-install.sh: 74 of 74 steps as expected` in 213 s (the two `nix build`s restored, nothing rebuilt: 93 s). The
daemon ran with `RISC0_DEV_MODE=1`, as the earlier install tests did; no step proves anything.

## B. Basecamp `b642a9a`, portable standalone mode: the mini-app from its published package

[`../install-test-plus/basecamp-open.sh`](../install-test-plus/basecamp-open.sh), unmodified, on the two `.lgx`
downloaded from `ferrata_bridge_ui-v0.1.0` and `ferrata_bridge_module-v0.1.1` (sha256 `fd65af73…` and `1ab1b2ff…`, the
sidecars'): both installed by the `lgpm` in Basecamp's closure, then the mini-app's probe
([`../../ferrata_bridge_ui/basecamp/probe.mjs`](../../ferrata_bridge_ui/basecamp/probe.mjs), offscreen): **3 passed,
0 failed** in 5 s ([`basecamp/probe.log`](basecamp/probe.log)): "the view opens in Basecamp"; "Basecamp's Module
Inspector lists and loads the core modules" (`ferrata_bridge_module`: `Unload (loaded)`); "the view calls
ferrata_bridge_module through Basecamp's logos object" (its `wallet info` answered by the module: `no wallet session:
open it first`, [`basecamp/probe/call-info.json`](basecamp/probe/call-info.json)). The signatures are A's and C's.

## C. Basecamp 0.4.0-rc.2 (the release AppImage): installed from the catalog by Basecamp's own Package Manager

Basecamp 0.4.0-rc.2's AppImage (293,951,992 bytes, sha256
`e59e14cddd211bed763c49e6a9bdfea462b91123b368386bdb8f700527b747a0`, checked before use), extracted (no FUSE in the
box), on Xvfb 1920x1080 with openbox and a D-Bus session, a fresh home; driven with `xdotool` by
[`basecamp-0.4.0-rc.2/drive.sh`](basecamp-0.4.0-rc.2/drive.sh) and the clicks of
[`basecamp-0.4.0-rc.2/actions.txt`](basecamp-0.4.0-rc.2/actions.txt) (the times: `actions.tsv`), each screen
photographed ([`basecamp-0.4.0-rc.2/shots/`](basecamp-0.4.0-rc.2/shots/), the window cropped, 128 colours):

| Screen | What it shows |
|---|---|
| `03-package-manager` | the Package Manager before: the built-in "Logos Official" repository only |
| `04-repositories`, `05-url-typed`, `06-repository-added` | Settings, **Package Repositories**: "Add a repository: paste the URL of a logos-repo.json index"; the catalog's URL typed, **Add**: "Ferrata modules" listed beside "Logos Official", enabled, with the catalog's URL and description |
| `07-trusted-publishers-before` | Trusted Publishers: Logos's built-in key alone |
| `08-catalog-packages` | the Package Manager, "Ferrata modules": the seven packages (six `core`, "Ferrata Bridge" `ui_qml`), the bridge at 0.1.1, released 25 Nov 2026 |
| `09-bridge-versions-offered` | the bridge's version list: 0.1.1 and 0.1.0 |
| `10-trust-prompt-bridge` | **Install**: "Install 'ferrata_bridge_module' v0.1.1, signed by a publisher you don't trust yet … Trusting key **9287 CD83 84E2 E015** lets what it signs install without asking from now on"; Cancel, Install, Trust & install |
| `11-cancelled-nothing-installed` | Cancel: every package still "Install", nothing installed |
| `12-trust-prompt-again`, `13-bridge-installed` | the same prompt; **Trust & install**: "Ferrata bridge" installed, 0.1.1 |
| `14-ui-install-prompt`, `15-ui-installed` | the mini-app: "Install 'ferrata_bridge_ui' v0.1.0, signed by key 9287 CD83 84E2 E015" (now trusted: no trust question), its optional package "Ferrata bridge v.0.1.1, installed"; **Install**: installed, its icon in the sidebar |
| `16-installed-tab` | Installed: the two |
| `17-trusted-publishers-after` | Trusted Publishers: `publisher-6339d50abb4f7fa8`, key 9287 CD83 84E2 E015, "trusted since 25 Nov 2026" |
| `18-app-open`, `20-app` | the mini-app open: "Ferrata Bridge … Prototype, not audited", "Logos core: ferrata_bridge_module" |
| `19-module-inspector` | the Module Inspector: "Ferrata bridge, `ferrata_bridge_module`, **LOADED**" (Basecamp loaded it as the mini-app's optional dependency: `basecamp.log`, "Loading optional dependency for "ferrata_bridge_ui" : "ferrata_bridge_module"", "Module loaded: ferrata_bridge_module") |
| `21-connect-wallet-module-answer` | **Connect wallet**: the view asks the module for its `info`; the module's answer, `no wallet session` (a text only `ferrata_bridge_module` writes; the view's `js/format.js` words it), shown as "Not connected to a network: open a session above … Nothing was read or sent." |

The key Basecamp asks to trust is the catalog's: `9287 CD83 84E2 E015` is the first 8 bytes of SHA-256 of the public
key in the catalog's DID, and `publisher-6339d50abb4f7fa8` the first 8 bytes of SHA-256 of the DID itself
([`basecamp-0.4.0-rc.2/key-ids.py`](basecamp-0.4.0-rc.2/key-ids.py), its output
[`key-ids.txt`](basecamp-0.4.0-rc.2/key-ids.txt)). Basecamp names the publisher "Unnamed publisher" in the prompt
because the release workflow signs without a signer name; the catalog's `trustedSigners` entry ("Ferrata module
releases") is a declaration Basecamp does not use, as Logos Core does not. What Basecamp installed, read from its data
directory after the run ([`installed.tsv`](basecamp-0.4.0-rc.2/installed.tsv)): `ferrata_bridge_module` 0.1.1 `core`,
`hashes.root` `92024bf1…3463`, and `ferrata_bridge_ui` 0.1.0 `ui_qml`, `f9d203fb…c8f4`, each its release sidecar's
`rootHash`. Basecamp downloaded both from the catalog's release assets (its log: `Downloaded "ferrata_bridge_module"
from "https://github.com/landclimber/ferrata-modules/releases/download/ferrata_bridge_module-v0.1.1/…"`).

The run recorded here is the sixth (c6, 04:46:42Z-04:49:20Z); c1 to c5 found the screens' coordinates (the same clicks, up to
a step each time). In `installed.tsv` the driver wrote its header's tabs as `\t`; the copy here has real tabs.

## The read-back and the manifest's commands

Before the installs, a read-only read-back of the published catalog
([`read-back.sh`](read-back.sh), catalog-kit's `after-release.sh` adapted to a catalog of two publications;
[`read-back-checks.txt`](read-back-checks.txt)): **212 checks passed, 0 failed**, one "unknown" (the monitor
release's missing `darwin-arm64`, as on 24 November). Among them: the signer DID, from the raw URL and the clone,
equal to the 24 November manifest's; each of the seven submodule pointers equal to its module repository's `main` and
its version tag, and to the commit of its publication's offline dry run (`release-publish.sh --dry-run` at `98375f82`
for five, `release-publish.sh --update --dry-run` at `f9623cfd` for the two new); each source commit on public `main`;
every release immutable, two assets, signed by the catalog's DID, its `.lgx`'s sha256 and size equal to the sidecar's;
`ferrata_bridge_module-v0.1.0` still offered, unchanged; the index's seven packages and eight versions equal to the
sidecars; immutable releases on all eight repositories, the tag ruleset, the organisation's 2FA; the two runs: success,
every build leg (`darwin-arm64`, `linux-amd64`, `linux-arm64`) and the release job, the action pinned at `bbdc5287`
in `_release-module.yml` at the run's commit. The values of the manifest's new rows and the template's commands that
produced them: [`integrity-commands.log`](integrity-commands.log) (`WORK` the scratch directory, `DRY` the update's dry run).

Files: [`steps.tsv`](steps.tsv), [`steps/`](steps/), [`log.txt`](log.txt), [`versions.txt`](versions.txt),
[`node.yaml`](node.yaml), [`daemon.log`](daemon.log), [`nix-lgx.log`](nix-lgx.log), [`nix-logosctl.log`](nix-logosctl.log)
(A); [`basecamp/`](basecamp/) (B); [`basecamp-0.4.0-rc.2/`](basecamp-0.4.0-rc.2/) (C: `drive.sh`, `actions.txt`,
`actions.tsv`, `shots/`, `session.log`, `basecamp.log`, `datadir.txt`, `installed.tsv`, `key-ids.py`, `key-ids.txt`);
[`read-back.sh`](read-back.sh), [`read-back-checks.txt`](read-back-checks.txt),
[`integrity-commands.log`](integrity-commands.log). Paths: `<repo>` is this repository, `<top>` the directory above it, `<work>` the run's scratch directory.

## Steps (A)

| # | Step | Expected | Exit | ms | As expected | Output (first 160 characters; whole output in `steps/<step>.txt`) |
|---:|---|---|---:|---:|---|---|
| 1 | `download-ferrata_attest_module` | ok | 0 | 402 | yes | 28e2b00194ab9aba7b32d9dd82a846fe87499c98d61ecf242eea9f3f7febe92b  ferrata_attest_module-0.1.0.lgx  |
| 2 | `lgx-verify-ferrata_attest_module` | ok | 0 | 915 | yes | Package structure is valid: <repo>/modules/catalog/install-test-landclimber-2026-11-25/pkgs/ferrata_attest_module-0.1.0.lgx Signature is valid Signer DID: did:j |
| 3 | `download-ferrata_attestor_module` | ok | 0 | 333 | yes | 8bef9d31d44a806c3b6a0a53c0a37203dac50bcb5c4e0b03d2fc9ccda9a84dd8  ferrata_attestor_module-0.1.0.lgx  |
| 4 | `lgx-verify-ferrata_attestor_module` | ok | 0 | 746 | yes | Package structure is valid: <repo>/modules/catalog/install-test-landclimber-2026-11-25/pkgs/ferrata_attestor_module-0.1.0.lgx Signature is valid Signer DID: did |
| 5 | `download-ferrata_bridge_module` | ok | 0 | 525 | yes | 1ab1b2ff43d88f6f7171e83c5c31ab28f5cc9c270547298ef68d0cf7233582d1  ferrata_bridge_module-0.1.1.lgx  |
| 6 | `lgx-verify-ferrata_bridge_module` | ok | 0 | 1629 | yes | Package structure is valid: <repo>/modules/catalog/install-test-landclimber-2026-11-25/pkgs/ferrata_bridge_module-0.1.1.lgx Signature is valid Signer DID: did:j |
| 7 | `download-ferrata_monitor_module` | ok | 0 | 414 | yes | 667107ffdfe9322e249a4b386d8b05a5538c7d10cf72e6ab9ca5e416ae8bb062  ferrata_monitor_module-0.1.0.lgx  |
| 8 | `lgx-verify-ferrata_monitor_module` | ok | 0 | 1139 | yes | Package structure is valid: <repo>/modules/catalog/install-test-landclimber-2026-11-25/pkgs/ferrata_monitor_module-0.1.0.lgx Signature is valid Signer DID: did: |
| 9 | `download-ferrata_relayer_module` | ok | 0 | 643 | yes | 53750b24f07b311343bcbde1bfe727d92258c8aa3a075120fdf203806c3baf64  ferrata_relayer_module-0.1.0.lgx  |
| 10 | `lgx-verify-ferrata_relayer_module` | ok | 0 | 2397 | yes | Package structure is valid: <repo>/modules/catalog/install-test-landclimber-2026-11-25/pkgs/ferrata_relayer_module-0.1.0.lgx Signature is valid Signer DID: did: |
| 11 | `download-ferrata_updater_module` | ok | 0 | 755 | yes | e693c9b2c966bc80dd22d5d994a8533c579bc8cbeac6d4797300c0ec81eda736  ferrata_updater_module-0.1.0.lgx  |
| 12 | `lgx-verify-ferrata_updater_module` | ok | 0 | 2171 | yes | Package structure is valid: <repo>/modules/catalog/install-test-landclimber-2026-11-25/pkgs/ferrata_updater_module-0.1.0.lgx Signature is valid Signer DID: did: |
| 13 | `download-ferrata_bridge_ui` | ok | 0 | 196 | yes | fd65af7325ca331cb21580d85fa689d9aa86ab600bbc5501c9fb079dc6a48a8a  ferrata_bridge_ui-0.1.0.lgx  |
| 14 | `lgx-verify-ferrata_bridge_ui` | ok | 0 | 17 | yes | Package structure is valid: <repo>/modules/catalog/install-test-landclimber-2026-11-25/pkgs/ferrata_bridge_ui-0.1.0.lgx Signature is valid Signer DID: did:jwk:e |
| 15 | `lgx-signature-unsigned` | ok | 0 | 440 | yes | unsigned: lgx signature prints nothing  |
| 16 | `lgx-signature-other-signer` | ok | 0 | 458 | yes | {   "did": "did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6IkNVSnhuX3FLcmVWSWxaUXd0cG00TFdsd25fY0tuLUF6cXMwbENOT2pfWG8ifQ",   "name": null }  |
| 17 | `config-set` | ok | 0 | 30 | yes | {"path":"<repo>/modules/catalog/install-test-landclimber-2026-11-25/session/daemon/config.yaml","status":"ok"}  |
| 18 | `daemon-start` | ok | 0 | 228 | yes | Daemon started (pid 2295) Logs: <repo>/modules/catalog/install-test-landclimber-2026-11-25/session/logs/daemon.log  |
| 19 | `config-show` | ok | 0 | 24 | yes | {"config":{"signature_policy":"require","version":2},"exists":true,"path":"<repo>/modules/catalog/install-test-landclimber-2026-11-25/session/daemon/config.yaml |
| 20 | `catalog-disable-default` | ok | 0 | 27 | yes | {"success":true}  |
| 21 | `catalog-add` | ok | 0 | 139 | yes | {"success":true}  |
| 22 | `catalog-source` | ok | 0 | 30 | yes | {"downloadSource":"http","success":true}  |
| 23 | `catalog-refresh` | ok | 0 | 151 | yes | {"success":true}  |
| 24 | `catalog-ls` | ok | 0 | 31 | yes | [{"description":"Curated official Logos modules.","displayName":"Logos Official","enabled":false,"homepage":"https://github.com/logos-co/logos-modules-release", |
| 25 | `search` | ok | 0 | 36 | yes | [{"author":"Ferrata","category":"wallet","description":"Ferrata: private ERC-20 and ETH bridge between Ethereum and LEZ. Deposit, claim, burn, release, recovery |
| 26 | `refuse-catalog-before-key` | refused: Package signed by untrusted key | 1 | 4131 | yes | {"affected_loaded":[],"changes":[{"action":"install","fromVersion":"","name":"ferrata_bridge_module","optional":false,"repository":"https://raw.githubuserconten |
| 27 | `key-add` | ok | 0 | 25 | yes | {"success":true}  |
| 28 | `key-ls` | ok | 0 | 26 | yes | [{"addedAt":"2026-11-25T04:31:22Z","did":"did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6ImNLVmpDSnFiWEFqYTBtenc1VFBOODhqWUs2QXkwTmtpRWNldUduekN6bGsifQ","d |
| 29 | `refuse-unsigned` | refused: Package is unsigned | 1 | 4109 | yes | {"affected_loaded":[],"changes":[{"action":"install","file":"<repo>/modules/catalog/install-test-landclimber-2026-11-25/unsigned.lgx","fromVersion":"","name":"f |
| 30 | `refuse-other-signer` | refused: Package signed by untrusted key | 1 | 4158 | yes | {"affected_loaded":[],"changes":[{"action":"install","file":"<repo>/modules/catalog/install-test-landclimber-2026-11-25/other-signed.lgx","fromVersion":"","name |
| 31 | `refuse-tampered` | refused: Content hash mismatch | 1 | 4143 | yes | {"affected_loaded":[],"changes":[{"action":"install","file":"<repo>/modules/catalog/install-test-landclimber-2026-11-25/tampered.lgx","fromVersion":"","name":"f |
| 32 | `nothing-installed-after-refusals` | ok | 0 | 34 | yes | [{"author":"Logos Core Team","category":"security","dependencies":[],"description":"Coordinates permissions between modules","displayName":"Capabilities","hashe |
| 33 | `show-ferrata_attest_module` | ok | 0 | 38 | yes | {"author":"","category":"ethereum","description":"Ferrata (prototype, unaudited, not for production): dry-run an Ethereum attestation against a header record an |
| 34 | `install-ferrata_attest_module` | ok | 0 | 2637 | yes | {"affected_loaded":[],"changes":[{"action":"install","fromVersion":"","name":"ferrata_attest_module","optional":false,"repository":"https://raw.githubuserconten |
| 35 | `roothash-ferrata_attest_module` | ok | 0 | 34 | yes | installed ferrata_attest_module rootHash: 2a879a4df2278c3d09a366d1a77a0d89ffe8855b8df2d4f292a80571f25a8736; release sidecar: 2a879a4df2278c3d09a366d1a77a0d89ffe |
| 36 | `show-ferrata_attestor_module` | ok | 0 | 38 | yes | {"author":"","category":"bridge","description":"Ferrata (prototype, unaudited, not for production): ferrata_attestor as a module, the rule (B) signer a LEZ sequ |
| 37 | `install-ferrata_attestor_module` | ok | 0 | 2071 | yes | {"affected_loaded":[],"changes":[{"action":"install","fromVersion":"","name":"ferrata_attestor_module","optional":false,"repository":"https://raw.githubusercont |
| 38 | `roothash-ferrata_attestor_module` | ok | 0 | 38 | yes | installed ferrata_attestor_module rootHash: 85dcae24795b94e5e21484f43bf367d79fe7e6cc11071f1379d48203ad6f2359; release sidecar: 85dcae24795b94e5e21484f43bf367d79 |
| 39 | `show-ferrata_bridge_module` | ok | 0 | 37 | yes | {"author":"","category":"bridge","description":"Ferrata (prototype, unaudited, not for production): the user side of the Ferrata bridge between Ethereum and LEZ |
| 40 | `install-ferrata_bridge_module` | ok | 0 | 4448 | yes | {"affected_loaded":[],"changes":[{"action":"install","fromVersion":"","name":"ferrata_bridge_module","optional":false,"repository":"https://raw.githubuserconten |
| 41 | `roothash-ferrata_bridge_module` | ok | 0 | 35 | yes | installed ferrata_bridge_module rootHash: 92024bf12434e1dc35051d7f8ee9a467ad3eb16df9ea9e36e9d929cc92253463; release sidecar: 92024bf12434e1dc35051d7f8ee9a467ad3 |
| 42 | `show-ferrata_monitor_module` | ok | 0 | 39 | yes | {"author":"","category":"bridge","description":"Ferrata (prototype, unaudited, not for production): the watchers' monitor as a module: follows a Ferrata vault,  |
| 43 | `install-ferrata_monitor_module` | ok | 0 | 3242 | yes | {"affected_loaded":[],"changes":[{"action":"install","fromVersion":"","name":"ferrata_monitor_module","optional":false,"repository":"https://raw.githubuserconte |
| 44 | `roothash-ferrata_monitor_module` | ok | 0 | 35 | yes | installed ferrata_monitor_module rootHash: 161a90a4985bd0b1351635ab5031aef70a1a0390b3c9657bb2d4d9e02a18a301; release sidecar: 161a90a4985bd0b1351635ab5031aef70a |
| 45 | `show-ferrata_relayer_module` | ok | 0 | 39 | yes | {"author":"","category":"bridge","description":"Ferrata (prototype, unaudited, not for production): ferrata-relayer as a module: imports the vault's deposit roo |
| 46 | `install-ferrata_relayer_module` | ok | 0 | 6547 | yes | {"affected_loaded":[],"changes":[{"action":"install","fromVersion":"","name":"ferrata_relayer_module","optional":false,"repository":"https://raw.githubuserconte |
| 47 | `roothash-ferrata_relayer_module` | ok | 0 | 36 | yes | installed ferrata_relayer_module rootHash: aac37f8b4a5ba583940c8f2b2ff5a258740a4dc618eb4a20212262f5967b846f; release sidecar: aac37f8b4a5ba583940c8f2b2ff5a25874 |
| 48 | `show-ferrata_updater_module` | ok | 0 | 38 | yes | {"author":"","category":"ethereum","description":"Ferrata (prototype, unaudited, not for production): ferrata-updater as a module: keeps Ferrata light-client in |
| 49 | `install-ferrata_updater_module` | ok | 0 | 6020 | yes | {"affected_loaded":[],"changes":[{"action":"install","fromVersion":"","name":"ferrata_updater_module","optional":false,"repository":"https://raw.githubuserconte |
| 50 | `roothash-ferrata_updater_module` | ok | 0 | 39 | yes | installed ferrata_updater_module rootHash: 0f23ee4c1eeabbfaa950aec7ab83ecb487a8cb44ae54f8101aac6257c651f867; release sidecar: 0f23ee4c1eeabbfaa950aec7ab83ecb487 |
| 51 | `show-ferrata_bridge_ui` | ok | 0 | 39 | yes | {"author":"Ferrata","category":"wallet","description":"Ferrata: private ERC-20 and ETH bridge between Ethereum and LEZ. Deposit, claim, burn, release, recovery, |
| 52 | `install-ferrata_bridge_ui` | ok | 0 | 181 | yes | {"affected_loaded":[],"changes":[{"action":"install","fromVersion":"","name":"ferrata_bridge_ui","optional":false,"repository":"https://raw.githubusercontent.co |
| 53 | `roothash-ferrata_bridge_ui` | ok | 0 | 37 | yes | installed ferrata_bridge_ui rootHash: f9d203fbaf0b5de328478d5a948f2326e84ff357b7090db3aba37187f838c8f4; release sidecar: f9d203fbaf0b5de328478d5a948f2326e84ff35 |
| 54 | `installed-as-core-ferrata_attest_module` | ok | 0 | 34 | yes | [{"author":"Logos Core Team","category":"security","dependencies":[],"description":"Coordinates permissions between modules","displayName":"Capabilities","hashe |
| 55 | `load-ferrata_attest_module` | ok | 0 | 46 | yes | {"dependencies_loaded":[],"module":"ferrata_attest_module","status":"ok","version":"0.1.0"}  |
| 56 | `version-ferrata_attest_module` | ok | 0 | 33 | yes | {"method":"version","module":"ferrata_attest_module","result":"{\"bls_backend\":\"blst (advance runs the program's steps natively)\",\"logos_core_modules_called |
| 57 | `installed-as-core-ferrata_attestor_module` | ok | 0 | 35 | yes | [{"author":"Logos Core Team","category":"security","dependencies":[],"description":"Coordinates permissions between modules","displayName":"Capabilities","hashe |
| 58 | `load-ferrata_attestor_module` | ok | 0 | 46 | yes | {"dependencies_loaded":[],"module":"ferrata_attestor_module","status":"ok","version":"0.1.0"}  |
| 59 | `version-ferrata_attestor_module` | ok | 0 | 33 | yes | {"method":"version","module":"ferrata_attestor_module","result":"{\"ferrata_attestor\":{\"commit\":\"98375f82c754f2fa4a56e20418687a89b321228f\",\"source_sha256\ |
| 60 | `installed-as-core-ferrata_bridge_module` | ok | 0 | 36 | yes | [{"author":"Logos Core Team","category":"security","dependencies":[],"description":"Coordinates permissions between modules","displayName":"Capabilities","hashe |
| 61 | `load-ferrata_bridge_module` | ok | 0 | 52 | yes | {"dependencies_loaded":[],"module":"ferrata_bridge_module","status":"ok","version":"0.1.1"}  |
| 62 | `version-ferrata_bridge_module` | ok | 0 | 37 | yes | {"method":"version","module":"ferrata_bridge_module","result":"{\"ethereum\":\"reads through Logos Core's eth_rpc_module (or a plain http:// endpoint); sends th |
| 63 | `installed-as-core-ferrata_monitor_module` | ok | 0 | 41 | yes | [{"author":"Logos Core Team","category":"security","dependencies":[],"description":"Coordinates permissions between modules","displayName":"Capabilities","hashe |
| 64 | `load-ferrata_monitor_module` | ok | 0 | 55 | yes | {"dependencies_loaded":[],"module":"ferrata_monitor_module","status":"ok","version":"0.1.0"}  |
| 65 | `version-ferrata_monitor_module` | ok | 0 | 37 | yes | {"method":"version","module":"ferrata_monitor_module","result":"{\"methods\":[\"version\",\"check\",\"node_view\",\"run\",\"start\",\"status\",\"stop\",\"custod |
| 66 | `installed-as-core-ferrata_relayer_module` | ok | 0 | 37 | yes | [{"author":"Logos Core Team","category":"security","dependencies":[],"description":"Coordinates permissions between modules","displayName":"Capabilities","hashe |
| 67 | `load-ferrata_relayer_module` | ok | 0 | 55 | yes | {"dependencies_loaded":[],"module":"ferrata_relayer_module","status":"ok","version":"0.1.0"}  |
| 68 | `version-ferrata_relayer_module` | ok | 0 | 35 | yes | {"method":"version","module":"ferrata_relayer_module","result":"{\"lez_wallet\":false,\"methods\":[\"version\",\"run\",\"start\",\"status\",\"stop\",\"custody\" |
| 69 | `installed-as-core-ferrata_updater_module` | ok | 0 | 37 | yes | [{"author":"Logos Core Team","category":"security","dependencies":[],"description":"Coordinates permissions between modules","displayName":"Capabilities","hashe |
| 70 | `load-ferrata_updater_module` | ok | 0 | 62 | yes | {"dependencies_loaded":[],"module":"ferrata_updater_module","status":"ok","version":"0.1.0"}  |
| 71 | `version-ferrata_updater_module` | ok | 0 | 156 | yes | {"method":"version","module":"ferrata_updater_module","result":"{\"dev_mode\":\"1\",\"ledgers\":[\"harness\",\"sequencer\"],\"methods\":[\"version\",\"check_con |
| 72 | `installed-as-ui-ferrata_bridge_ui` | ok | 0 | 33 | yes | [{"author":"Ferrata","category":"wallet","dependencies":[],"description":"Ferrata: private ERC-20 and ETH bridge between Ethereum and LEZ. Deposit, claim, burn, |
| 73 | `package-ls` | ok | 0 | 30 | yes | [{"author":"Logos Core Team","category":"security","dependencies":[],"description":"Coordinates permissions between modules","displayName":"Capabilities","hashe |
| 74 | `daemon-stop` | ok | 0 | 834 | yes | {"message":"Daemon shutting down.","status":"ok"}  |
