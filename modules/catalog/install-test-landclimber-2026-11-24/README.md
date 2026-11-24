# Installed from the published catalog: `landclimber/ferrata-modules`, 24 November 2026

A headless Logos Core installed all six Ferrata modules **from the published catalog**, the URL users add
(`https://raw.githubusercontent.com/landclimber/ferrata-modules/refs/heads/main/logos-repo.json`), with
`signature_policy: require` and the catalog's signer DID as the only key in its keyring; loaded each and called its
`version`. Before that it refused what it must refuse, each for its own reason. **56 of 56 steps as expected**
(18:33Z-18:34Z, the Modal build box, Linux x86-64 under gVisor; `versions.txt`).

| What | Result |
|---|---|
| Client | `logosctl` of logos-logoscore-cli `66d098d` (`ctl-bundle-dir`, the portable bundle, built with Nix 2.28.3 from source on the box), its own daemon, a fresh configuration directory |
| Catalog | default catalog disabled; `catalog add` the raw `logos-repo.json` URL; `catalog refresh`; `catalog ls` shows it enabled, its `indexUrl` `…/refs/heads/main/index/index.json` and one trusted-signer DID; `search ferrata` finds the six |
| Refused before the key is trusted | `install ferrata_attest_module` from the catalog: `Package signed by untrusted key: did:jwk:…` (the catalog's `trustedSigners` are not trusted by themselves) |
| Refused after the key is trusted | the published attest package with its `manifest.sig` removed: `Package is unsigned and signature policy requires signatures`; the same package signed by a fresh key under the signer name `Ferrata module releases`: `Package signed by untrusted key` (another DID); the published package with one bit flipped in its `linux-amd64` plugin: `Content hash mismatch`. Nothing installed after the refusals (`package ls`) |
| Installed | each of the six by name from the catalog (`install <module> -y`, the `linux-amd64` variant); each installed package's `hashes.root` equals its release sidecar's `rootHash` (and `RELEASE-INTEGRITY.md`'s), and `package ls` records the DID as its `signerDid` |
| Loaded and called | `module load` and `call <module> version --json` for all six |
| `lgx verify` | each published `.lgx`, downloaded from its release, against a keyring holding only the DID (lgx of logos-package `4bdd9041`): structure valid, signature valid, signer trusted |

Not shown here: an install on macOS or on `linux-arm64` (this box is Linux x86-64), or in Basecamp's own package
manager. Of the published packages, four carry `darwin-arm64`; `ferrata_monitor_module` does not (its macOS leg never
got a GitHub-hosted runner) and `ferrata_updater_module` carries `linux-amd64` alone, by design
([`releases/RELEASE-INTEGRITY.md`](../../../releases/RELEASE-INTEGRITY.md), "Partial releases"). The daemon ran with
`RISC0_DEV_MODE=1`, as the earlier install tests did (the updater module's in-process chain); no step here proves
anything. `SSL_CERT_FILE` named the box's CA bundle.

## How it ran

`after-release.sh landclimber ferrata-modules --box` (catalog-kit, not in the repository: the read-only read-back,
[`read-back-checks.txt`](read-back-checks.txt), 123 checks passed, 0 failed, one "unknown": the monitor release's
missing `darwin-arm64`) wrote [`box-install.sh`](box-install.sh) and ran it on the build box with
`box-install.sh landclimber ferrata-modules <DID> <tag>/<asset>/<rootHash>…` (the six releases read back). The values
of the release-integrity manifest and the commands that produced them: [`integrity-commands.log`](integrity-commands.log).
Paths: `WORK` is the box's work directory, `SOCK` Logos Core's socket directory, `DRY` the offline dry run's
repositories (`release-publish.sh … --dry-run` at `98375f82`).

Files: [`steps.tsv`](steps.tsv) (name, expected, exit code, milliseconds, as expected, output), `out/<step>.txt`
(each step's whole output), [`log.txt`](log.txt), [`versions.txt`](versions.txt), [`node.yaml`](node.yaml) (the
daemon configuration set), [`daemon.log`](daemon.log).

## Steps

| # | Step | Expected | Exit | ms | As expected | Output (first 160 characters; whole output in `out/<step>.txt`) |
|---:|---|---|---:|---:|---|---|
| 1 | `download-ferrata_attest_module` | ok | 0 | 216 | yes | 28e2b00194ab9aba7b32d9dd82a846fe87499c98d61ecf242eea9f3f7febe92b  ferrata_attest_module-0.1.0.lgx  |
| 2 | `lgx-verify-ferrata_attest_module` | ok | 0 | 713 | yes | Package structure is valid: WORK/pkgs/ferrata_attest_module-0.1.0.lgx Signature is valid Signer DID: did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6ImNLVmp |
| 3 | `download-ferrata_attestor_module` | ok | 0 | 177 | yes | 8bef9d31d44a806c3b6a0a53c0a37203dac50bcb5c4e0b03d2fc9ccda9a84dd8  ferrata_attestor_module-0.1.0.lgx  |
| 4 | `lgx-verify-ferrata_attestor_module` | ok | 0 | 577 | yes | Package structure is valid: WORK/pkgs/ferrata_attestor_module-0.1.0.lgx Signature is valid Signer DID: did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6ImNLV |
| 5 | `download-ferrata_bridge_module` | ok | 0 | 252 | yes | b160668aa824c8f08dc42d20a071c4f92173625a521671b7e4ec3028acdb7f7d  ferrata_bridge_module-0.1.0.lgx  |
| 6 | `lgx-verify-ferrata_bridge_module` | ok | 0 | 1035 | yes | Package structure is valid: WORK/pkgs/ferrata_bridge_module-0.1.0.lgx Signature is valid Signer DID: did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6ImNLVmp |
| 7 | `download-ferrata_monitor_module` | ok | 0 | 234 | yes | 667107ffdfe9322e249a4b386d8b05a5538c7d10cf72e6ab9ca5e416ae8bb062  ferrata_monitor_module-0.1.0.lgx  |
| 8 | `lgx-verify-ferrata_monitor_module` | ok | 0 | 881 | yes | Package structure is valid: WORK/pkgs/ferrata_monitor_module-0.1.0.lgx Signature is valid Signer DID: did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6ImNLVm |
| 9 | `download-ferrata_relayer_module` | ok | 0 | 460 | yes | 53750b24f07b311343bcbde1bfe727d92258c8aa3a075120fdf203806c3baf64  ferrata_relayer_module-0.1.0.lgx  |
| 10 | `lgx-verify-ferrata_relayer_module` | ok | 0 | 1833 | yes | Package structure is valid: WORK/pkgs/ferrata_relayer_module-0.1.0.lgx Signature is valid Signer DID: did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6ImNLVm |
| 11 | `download-ferrata_updater_module` | ok | 0 | 462 | yes | e693c9b2c966bc80dd22d5d994a8533c579bc8cbeac6d4797300c0ec81eda736  ferrata_updater_module-0.1.0.lgx  |
| 12 | `lgx-verify-ferrata_updater_module` | ok | 0 | 1638 | yes | Package structure is valid: WORK/pkgs/ferrata_updater_module-0.1.0.lgx Signature is valid Signer DID: did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6ImNLVm |
| 13 | `lgx-signature-unsigned` | ok | 0 | 232 | yes | unsigned: lgx signature prints nothing  |
| 14 | `lgx-signature-other-signer` | ok | 0 | 235 | yes | {   "did": "did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6ImF3alFLaEZCSnJaSjlTeHhVWlEtQXNlMkNyb2V6ZVplZjBjY2h4MHU2N00ifQ",   "name": null }  |
| 15 | `config-set` | ok | 0 | 18 | yes | {"path":"WORK/session/daemon/config.yaml","status":"ok"}  |
| 16 | `daemon-start` | ok | 0 | 119 | yes | Daemon started (pid 1321) Logs: WORK/session/logs/daemon.log  |
| 17 | `config-show` | ok | 0 | 17 | yes | {"config":{"signature_policy":"require","version":2},"exists":true,"path":"WORK/session/daemon/config.yaml"}  |
| 18 | `catalog-disable-default` | ok | 0 | 21 | yes | {"success":true}  |
| 19 | `catalog-add` | ok | 0 | 75 | yes | {"success":true}  |
| 20 | `catalog-source` | ok | 0 | 21 | yes | {"downloadSource":"http","success":true}  |
| 21 | `catalog-refresh` | ok | 0 | 66 | yes | {"success":true}  |
| 22 | `catalog-ls` | ok | 0 | 20 | yes | [{"description":"Curated official Logos modules.","displayName":"Logos Official","enabled":false,"homepage":"https://github.com/logos-co/logos-modules-release", |
| 23 | `search` | ok | 0 | 22 | yes | [{"author":"","category":"ethereum","description":"Ferrata (prototype, unaudited, not for production): ferrata-updater as a module: keeps Ferrata light-client i |
| 24 | `refuse-catalog-before-key` | refused: Package signed by untrusted key | 1 | 1830 | yes | {"affected_loaded":[],"changes":[{"action":"install","fromVersion":"","name":"ferrata_attest_module","optional":false,"repository":"https://raw.githubuserconten |
| 25 | `key-add` | ok | 0 | 20 | yes | {"success":true}  |
| 26 | `key-ls` | ok | 0 | 20 | yes | [{"addedAt":"2026-11-24T18:34:29Z","did":"did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6ImNLVmpDSnFiWEFqYTBtenc1VFBOODhqWUs2QXkwTmtpRWNldUduekN6bGsifQ","d |
| 27 | `refuse-unsigned` | refused: Package is unsigned | 1 | 1849 | yes | {"affected_loaded":[],"changes":[{"action":"install","file":"WORK/unsigned.lgx","fromVersion":"","name":"ferrata_attest_module","signature":"unsigned","toVersio |
| 28 | `refuse-other-signer` | refused: Package signed by untrusted key | 1 | 1864 | yes | {"affected_loaded":[],"changes":[{"action":"install","file":"WORK/other-signed.lgx","fromVersion":"","name":"ferrata_attest_module","signature":"signed","toVers |
| 29 | `refuse-tampered` | refused: Content hash mismatch | 1 | 1845 | yes | {"affected_loaded":[],"changes":[{"action":"install","file":"WORK/tampered.lgx","fromVersion":"","name":"ferrata_attest_module","signature":"error","toVersion": |
| 30 | `nothing-installed-after-refusals` | ok | 0 | 26 | yes | [{"author":"Logos Core Team","category":"security","dependencies":[],"description":"Coordinates permissions between modules","displayName":"Capabilities","hashe |
| 31 | `install-ferrata_attest_module` | ok | 0 | 2022 | yes | {"affected_loaded":[],"changes":[{"action":"install","fromVersion":"","name":"ferrata_attest_module","optional":false,"repository":"https://raw.githubuserconten |
| 32 | `roothash-ferrata_attest_module` | ok | 0 | 26 | yes | installed ferrata_attest_module rootHash: 2a879a4df2278c3d09a366d1a77a0d89ffe8855b8df2d4f292a80571f25a8736; release sidecar: 2a879a4df2278c3d09a366d1a77a0d89ffe |
| 33 | `install-ferrata_attestor_module` | ok | 0 | 1659 | yes | {"affected_loaded":[],"changes":[{"action":"install","fromVersion":"","name":"ferrata_attestor_module","optional":false,"repository":"https://raw.githubusercont |
| 34 | `roothash-ferrata_attestor_module` | ok | 0 | 28 | yes | installed ferrata_attestor_module rootHash: 85dcae24795b94e5e21484f43bf367d79fe7e6cc11071f1379d48203ad6f2359; release sidecar: 85dcae24795b94e5e21484f43bf367d79 |
| 35 | `install-ferrata_bridge_module` | ok | 0 | 3009 | yes | {"affected_loaded":[],"changes":[{"action":"install","fromVersion":"","name":"ferrata_bridge_module","optional":false,"repository":"https://raw.githubuserconten |
| 36 | `roothash-ferrata_bridge_module` | ok | 0 | 26 | yes | installed ferrata_bridge_module rootHash: 0e46fbecf4c39caa9880bac11bc53c9abab4a4c703b9af9fa34db6a4f166fdd8; release sidecar: 0e46fbecf4c39caa9880bac11bc53c9abab |
| 37 | `install-ferrata_monitor_module` | ok | 0 | 2578 | yes | {"affected_loaded":[],"changes":[{"action":"install","fromVersion":"","name":"ferrata_monitor_module","optional":false,"repository":"https://raw.githubuserconte |
| 38 | `roothash-ferrata_monitor_module` | ok | 0 | 33 | yes | installed ferrata_monitor_module rootHash: 161a90a4985bd0b1351635ab5031aef70a1a0390b3c9657bb2d4d9e02a18a301; release sidecar: 161a90a4985bd0b1351635ab5031aef70a |
| 39 | `install-ferrata_relayer_module` | ok | 0 | 5247 | yes | {"affected_loaded":[],"changes":[{"action":"install","fromVersion":"","name":"ferrata_relayer_module","optional":false,"repository":"https://raw.githubuserconte |
| 40 | `roothash-ferrata_relayer_module` | ok | 0 | 28 | yes | installed ferrata_relayer_module rootHash: aac37f8b4a5ba583940c8f2b2ff5a258740a4dc618eb4a20212262f5967b846f; release sidecar: aac37f8b4a5ba583940c8f2b2ff5a25874 |
| 41 | `install-ferrata_updater_module` | ok | 0 | 4886 | yes | {"affected_loaded":[],"changes":[{"action":"install","fromVersion":"","name":"ferrata_updater_module","optional":false,"repository":"https://raw.githubuserconte |
| 42 | `roothash-ferrata_updater_module` | ok | 0 | 29 | yes | installed ferrata_updater_module rootHash: 0f23ee4c1eeabbfaa950aec7ab83ecb487a8cb44ae54f8101aac6257c651f867; release sidecar: 0f23ee4c1eeabbfaa950aec7ab83ecb487 |
| 43 | `load-ferrata_attest_module` | ok | 0 | 33 | yes | {"dependencies_loaded":[],"module":"ferrata_attest_module","status":"ok","version":"0.1.0"}  |
| 44 | `version-ferrata_attest_module` | ok | 0 | 20 | yes | {"method":"version","module":"ferrata_attest_module","result":"{\"bls_backend\":\"blst (advance runs the program's steps natively)\",\"logos_core_modules_called |
| 45 | `load-ferrata_attestor_module` | ok | 0 | 32 | yes | {"dependencies_loaded":[],"module":"ferrata_attestor_module","status":"ok","version":"0.1.0"}  |
| 46 | `version-ferrata_attestor_module` | ok | 0 | 19 | yes | {"method":"version","module":"ferrata_attestor_module","result":"{\"ferrata_attestor\":{\"commit\":\"98375f82c754f2fa4a56e20418687a89b321228f\",\"source_sha256\ |
| 47 | `load-ferrata_bridge_module` | ok | 0 | 36 | yes | {"dependencies_loaded":[],"module":"ferrata_bridge_module","status":"ok","version":"0.1.0"}  |
| 48 | `version-ferrata_bridge_module` | ok | 0 | 20 | yes | {"method":"version","module":"ferrata_bridge_module","result":"{\"ethereum\":\"reads through Logos Core's eth_rpc_module (or a plain http:// endpoint); sends th |
| 49 | `load-ferrata_monitor_module` | ok | 0 | 36 | yes | {"dependencies_loaded":[],"module":"ferrata_monitor_module","status":"ok","version":"0.1.0"}  |
| 50 | `version-ferrata_monitor_module` | ok | 0 | 21 | yes | {"method":"version","module":"ferrata_monitor_module","result":"{\"methods\":[\"version\",\"check\",\"node_view\",\"run\",\"start\",\"status\",\"stop\",\"custod |
| 51 | `load-ferrata_relayer_module` | ok | 0 | 40 | yes | {"dependencies_loaded":[],"module":"ferrata_relayer_module","status":"ok","version":"0.1.0"}  |
| 52 | `version-ferrata_relayer_module` | ok | 0 | 21 | yes | {"method":"version","module":"ferrata_relayer_module","result":"{\"lez_wallet\":false,\"methods\":[\"version\",\"run\",\"start\",\"status\",\"stop\",\"custody\" |
| 53 | `load-ferrata_updater_module` | ok | 0 | 46 | yes | {"dependencies_loaded":[],"module":"ferrata_updater_module","status":"ok","version":"0.1.0"}  |
| 54 | `version-ferrata_updater_module` | ok | 0 | 103 | yes | {"method":"version","module":"ferrata_updater_module","result":"{\"dev_mode\":\"1\",\"ledgers\":[\"harness\",\"sequencer\"],\"methods\":[\"version\",\"check_con |
| 55 | `package-ls` | ok | 0 | 22 | yes | [{"author":"Logos Core Team","category":"security","dependencies":[],"description":"Coordinates permissions between modules","displayName":"Capabilities","hashe |
| 56 | `daemon-stop` | ok | 0 | 834 | yes | {"message":"Daemon shutting down.","status":"ok"}  |
