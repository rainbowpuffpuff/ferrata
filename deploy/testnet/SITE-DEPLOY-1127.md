# Deploying the site of 27 November 2026 to the test network's host

The maintainer's procedure for putting the build page (with a reviewer's four links), the testers' page (step 6e, a
private claim) and the web wallet's bridge tab (refusals in plain words, the cost before the signature) on the host,
after they are merged and pushed. It installs only the files the host serves differently from public main, all or
none, keeps the files it replaces, touches no configuration, restarts nothing, and ends with the host's site manifest
made again so that `scripts/site-manifest.py verify` passes. **Not run** by this project: the commands below were
checked read-only against the host's published manifest (`plan`) and on a scratch web root (the tests); the install
itself needs the owner's AWS session.

What changes on the host, at this branch's tip, against the manifest the host publishes (made 25 November 2026,
15:01Z, after the deploy of `dc4ca708`; `scripts/site-manifest.py verify` on 27 November, 15:28Z: 232 of 232 files equal,
PASS): the build page's six files
(`/index.html`, `/evidence.html`, `/evidence.json`, `/testers/index.html`, `/assets/site.css`, `/build.json`) and the
wallet's two (`/wallet/bridge.js`, `/wallet/sw.js`). `plan` also lists `/README.md` (the explorer's README, changed on
main since that deploy) as outside the default parts: add `--parts build,wallet,other` to install it too. The wallet's
`config.json`, the explorer's config, `AGENTS.md` and the status files are the host's own and never in a package.

## 1. A clean checkout of the tip, and its checks (any machine; no key)

```sh
git clone https://github.com/rainbowpuffpuff/ferrata site-deploy && cd site-deploy   # or fetch an existing clone
git checkout --detach origin/main && git log --oneline -1                           # the tip to deploy
python3 -I site/build.py --check                 # exit 0: the committed bundle equals a fresh build (else: rebuild, commit, push first)
python3 -I site/test_site.py                     # counts, links, freshness, the reviewer links and their numbers
python3 -I scripts/check-guides.py               # check 7: README's reviewer links and counts
node deploy/testnet/web/wallet/test/pwa-files.mjs --check
node --test deploy/testnet/web/wallet/test/*.test.mjs
python3 -I deploy/testnet/tests/test_site_delta.py
```

## 2. What the deploy changes, and the package (any machine; one GET to the host)

```sh
python3 -I deploy/testnet/site-delta.py plan                       # reads <network>/bridge/verify/manifest.json
python3 -I deploy/testnet/site-delta.py pack --out ../site-deploy-pkg
```

`<network>` is the address `scripts/reproduce/deployed-programs.json` names. `pack` refuses a stale bundle (the tip must
be checked out, the tree clean, `site/build.py --check` passing). Read the plan: every line is a file the
host serves today with the SHA-256 the manifest records, and the one it gets. If the plan lists a file you did not
expect, stop and find out why. `pack` prints the package's path (`site-TIP12.tar`) and its SHA-256: keep both.

## 3. Install on the host (the owner's AWS session; the host's artifact bucket and instance as in `deploy/testnet/README.md`)

```sh
TIP12=<the 12 hex digits of the tip>; SHA=<the package's sha256>; BUCKET=<artifact bucket>; INSTANCE=<instance id>
aws s3 cp ../site-deploy-pkg/site-$TIP12.tar s3://$BUCKET/hotfix/site-$TIP12/site-$TIP12.tar
cat > ../site-deploy-pkg/ssm.json <<EOF
{"commands": ["set -eu",
  "d=\$(mktemp -d /root/site-$TIP12.XXXXXX)",
  "aws s3 cp --only-show-errors s3://$BUCKET/hotfix/site-$TIP12/site-$TIP12.tar \$d/",
  "echo '$SHA  '\$d/site-$TIP12.tar | sha256sum -c --quiet",
  "tar -xf \$d/site-$TIP12.tar -C \$d",
  "bash \$d/install.sh"]}
EOF
aws ssm send-command --instance-ids $INSTANCE --document-name AWS-RunShellScript \
  --parameters file://../site-deploy-pkg/ssm.json --query Command.CommandId --output text
aws ssm get-command-invocation --instance-id $INSTANCE --command-id <the id printed> --query '[Status,StandardOutputContent,StandardErrorContent]'
```

The commands run under the host's `/bin/sh` (Ubuntu 24.04: dash), the package's script under bash. `install.sh` first checks the package and every target against what the manifest says the host serves; if one
differs (the host moved since the manifest was made), it says which and installs nothing; it stages every file before
it replaces any, and puts back the files already renamed if a rename fails. Otherwise it prints
`installed N file(s) from TIP12; the replaced ones are in /var/lib/ferrata/site-backup/TIP12-<time>`. Caddy, the
configs, the sequencer and every other file are untouched; no service restarts.

**Roll back** (only if a page breaks): the backup directory holds every replaced file at its path under the web root;
files the package added are listed `absent` in its `EXPECT`.

```sh
# on the host, as root (an SSM command as above): B=/var/lib/ferrata/site-backup/<TIP12-time>; D=<the package's directory>
cd "$B" && find . -type f -print0 | while IFS= read -r -d '' f; do install -o root -g root -m 644 "$f" "/srv/ferrata/www/$f"; done
awk '$1 == "absent" {print $2}' "$D/EXPECT" | while read -r p; do rm -f "/srv/ferrata/www/$p"; done
```

## 4. Check the live site (any machine; read-only)

```sh
curl -s <network>/build.json | python3 -I -c 'import json,sys; b=json.load(sys.stdin); print(b["commit"], b["hard"])'
python3 -I deploy/testnet/site-delta.py plan          # still lists the files: the host's manifest is the old one until step 5
```

In a browser, at 1440 and 390 px and in both themes: the build page opens with the four links (status, ten-minute
verify, try it, run records), each one click to its target; the testers' page has step 6e and its link to
`docs/VERIFY.md` section 5; the wallet's Bridge tab shows the note under "Destination on LEZ" when "this wallet's private
account /0" is chosen. The wallet updates itself on its next load (its service worker keeps the build its `sw.js` lists).

## 5. The site manifest again, from the host's own file list

```sh
# on the host (SSM): the served tree, as site-manifest.py build reads it
cd /srv/ferrata/www && find . -type f -print0 | sort -z | xargs -0 sha256sum
```

Save that output as `served.txt`, then in a worktree of public main on a new branch `site-manifest-TIP12`:

```sh
python3 -I scripts/site-manifest.py build --served served.txt --out deploy/testnet/web/trust/bridge/verify/manifest.json --ref origin/main
```

It must match every served file to a public commit (as on 25 November: 232 matched, 8 generated on the host, 16
unmatched, each in a committed SHA256SUMS). Commit it (`The site manifest made from the host's file list after the
deploy of TIP12`), push, install that one file on the host as `install-manifest.sh` did on 25 November (its SHA-256
checked, written whole, root:root 644), then:

```sh
python3 -I scripts/site-manifest.py verify --github main          # VERDICT: PASS
python3 -I scripts/verify.py site-manifest-online                  # PASS
```
