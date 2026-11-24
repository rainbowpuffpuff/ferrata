# `/basecamp/` on the test network: Ferrata's signed Logos packages

The page (`index.html`, made by `build.py`), the release record it is made from (`release.json`), the public half of
the release key and every signature (`dl/`), and the install test's record (`tests/`). The `.lgx` packages are not
committed: `release.json` and `dl/SHA256SUMS` hold their sizes and SHA-256, and the deploy bundle carries them.

## The release of `a0f38827` (23 November 2026)

| | |
|---|---|
| Packages | 13: seven `linux-amd64` (the six modules and the mini-app) built from a clean clone of public main on a Modal build box; five `darwin-arm64` modules from the `macos` workflow's run 45159833675 on `macos-15`; the mini-app for `darwin-arm64` made with `lgx add`/`lgx remove` from the Linux build's files (QML only; its variant hash equals the Linux package's) |
| Reproducible | the seven Linux packages built twice, in two Modal workspaces (one with a Nix store restored from an earlier stream, one empty): 7 of 7 byte-identical (`tests/reproduce.json`) |
| Release key | Ed25519, made on the build box on 23 Nov 2026 15:17Z; its private half, without a passphrase, only in that box's storage (which `mrun` snapshots to a Modal volume of the project), never on a laptop or in git; `SHA256:m5xC7zkg0LzI3YhCtol4qSfgMJGijs84mFWWGk0eX9Y` (ssh-keygen), Basecamp's form `761B F9D4 D821 9186`, `did:jwk:…` in `dl/keys/ferrata-release.did`, minisign key id `869121D8D4F91B76` |
| Signatures | in each package (`lgx sign`: `manifest.sig`, checked by Logos Core and Basecamp at install), beside it (`.sig`: `ssh-keygen -Y sign -n file`; `.minisig`), and over `SHA256SUMS` |
| Install test | `modules/scripts/basecamp-install-test.sh` (rerun 23 Nov 2026 22:33Z on the build box): a headless Logos Core (logosctl `66d098d`, portable) with `signature_policy: require` and the release key as its only trusted signer refused an unsigned, a tampered and an other-signer copy, each for its own reason; installed all seven Linux packages and loaded and called the six core modules; decoded the network's Sepolia instance read from its RPC by the script; 33 of 33 steps (`tests/install-test.json`, `tests/install-test/`) |
| Licences | `dl/notices/NOTICES.md` (the shared libraries, the updater's GPL-3.0 native code with its source offer, the GMP sources beside it), `dl/notices/crates.md` (every crate linked into each plugin, `modules/scripts/module_notices.py`), `dl/notices/licenses/` |

## Remaking it

Needs Nix with flakes, git, curl, jq and python3; on Linux the updater also needs LEZ's guests built first, with Rust,
RISC Zero 3.0.5 and a RISC Zero home holding guest Rust 1.91.1 (`RISC0_HOME`, `lez/RESULTS.md`); on a Mac the updater
is left out. Signing needs `lgx` (logos-package `4bdd9041`), ssh-keygen and minisign.

```sh
# 1. Build (Linux x86-64 or an Apple-silicon Mac), from a clean checkout of the commit:
git clone https://github.com/rainbowpuffpuff/ferrata && cd ferrata && git checkout <commit> && scripts/fetch-upstream.sh
RISC0_HOME=<RISC Zero home with guest Rust 1.91.1> modules/scripts/basecamp-release.sh build . ../out
# 2. On the machine that holds the key (made once with `basecamp-release.sh keygen KEYDIR`): put the packages in
#    REL/<platform>/ with REL/build-<platform>.json, then
modules/scripts/basecamp-release.sh sign REL KEYDIR
python3 -I modules/scripts/basecamp_release.py assemble REL
modules/scripts/basecamp-release.sh verify REL            # stops at the first failed check
# 3. The install test and its record (FERRATA_TESTNET_RPC=<the network's /rpc> for the live reads):
modules/scripts/basecamp-install-test.sh REL ../out/linux-amd64 WORK
python3 -I modules/scripts/basecamp_install_record.py WORK/record deploy/testnet/web/basecamp/tests <the network's base URL>
# 4. The crate notices (cargo and the network, on the build machine), then the page:
python3 -I modules/scripts/module_notices.py collect ../out/src NOTICES_DATA
python3 -I modules/scripts/module_notices.py render NOTICES_DATA deploy/testnet/web/basecamp/dl/notices
#    copy REL/release.json here, REL/keys/* to dl/keys/, REL/SHA256SUMS* and every .sig/.minisig to dl/, then
python3 -I deploy/testnet/web/basecamp/build.py
python3 -I deploy/testnet/web/basecamp/test_page.py --packages REL
modules/scripts/test-basecamp-release.sh <an empty directory>   # the verify script against faulty releases
```

The `.lgx` packages and the GMP source archives (`dl/notices/sources/`) are not committed; the deploy bundle carries
them. A second build is compared with `modules/scripts/basecamp_compare.py`. Deploying: the test network's S3/SSM route,
only `/srv/ferrata/www/basecamp/`, every file checked against the bundle's `files.sha256` on the host before the swap.
