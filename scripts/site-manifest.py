#!/usr/bin/env python3
"""The test network site's manifest: every file the host serves, its SHA-256, and the commit and path of the
repository it comes from; and the check that catches a host serving something else.

    python3 -I scripts/site-manifest.py verify [--site URL] [--manifest FILE | --github REF] [--repo DIR]
        Anyone, from a clone of the repository. Compares three things that a compromised host cannot all forge:
          1. the manifest the host serves (/bridge/verify/manifest.json, which the verify page shows);
          2. the manifest committed in the repository (this checkout's copy, --manifest FILE, or --github REF:
             the copy at REF of the public repository, fetched from GitHub, not from the host);
          3. every listed file as the host serves it, hashed here, and, where this clone has the listed commit, the
             same file read from git.
        Also fetched a second time under the host's other name (--second URL: the domain in your browser's address
        bar when --site is the sslip.io name, or the reverse) with a browser's User-Agent, and the security headers
        checked. Exit 1 on any difference. Standard library only; GET requests only.
    python3 -I scripts/site-manifest.py build --served FILE --out FILE [--ref REF ...]
        The maintainer, after a deploy: FILE lists the served tree as `sha256<TAB>path` (sha256sum's output on the
        host). Each file is matched, by its SHA-256 and its path, to a blob under deploy/testnet/ or names/js/ of the
        commits of the given refs; files that match none are listed as `unmatched` (generated on the host, or not from
        the repository: the page says which).
"""
import argparse
import hashlib
import json
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST_REPO_PATH = "deploy/testnet/web/trust/bridge/verify/manifest.json"
SERVED_PATH = "/bridge/verify/manifest.json"
GITHUB = "https://raw.githubusercontent.com/rainbowpuffpuff/ferrata/{ref}/" + MANIFEST_REPO_PATH
# Written by the host itself, not deployed from the repository: listed, never hashed.
GENERATED = {
    "status/status.json": "the host's status daemon rewrites it every few seconds",
    "status/history.json": "the host's status daemon",
    "status/programs.json": "the host's deploy step (deploy-programs.sh publish), from the programs it deployed",
    "programs.json": "the host's deploy step, the same file as /status/programs.json",
    "config.json": "the explorer's config with the host's deployed accounts filled in (install-web.sh)",
    "wallet/config.json": "the wallet's config merged on the host with the deployed programs",
    "msg/config.json": "the messaging library's config merged on the host",
    "AGENTS.md": "assembled on the host by install-web.sh: the explorer's AGENTS.md, the program streams' sections and "
                 "the host's limits",
}
PREFIXES = ("deploy/testnet/", "names/js/")
# Files deploy/testnet/install-web.sh installs under another name than the repository's.
RENAMED = {"network.html": "deploy/testnet/web/explorer/index.html", "health.html": "deploy/testnet/host/www/status.html"}
RENAMED_DIRS = {"names/": "names/js/"}


def sha256(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def git(*args, inp=None) -> bytes:
    return subprocess.run(["git", "-C", str(ROOT), *args], input=inp, capture_output=True, check=True).stdout


# ---------------------------------------------------------------- build
def blob_index(refs):
    """sha256 -> [(commit, path)] over every blob under PREFIXES in the commits of refs (newest first)."""
    by_oid, seen = {}, set()
    for ref in refs:
        commits = git("rev-list", "--max-count=400", ref, "--", *PREFIXES).decode().split()
        for c in commits:
            for line in git("ls-tree", "-r", c, "--", *PREFIXES).decode().splitlines():
                meta, path = line.split("\t", 1)
                _, kind, oid = meta.split()
                if kind == "blob" and (oid, path) not in seen:
                    seen.add((oid, path))
                    by_oid.setdefault(oid, []).append((c, path, ref))
    oids = list(by_oid)
    out = git("cat-file", "--batch", inp="".join(o + "\n" for o in oids).encode())
    idx, i = {}, 0
    for oid in oids:
        nl = out.index(b"\n", i)
        size = int(out[i:nl].split()[2])
        data = out[nl + 1: nl + 1 + size]
        i = nl + 1 + size + 1
        for c, path, ref in by_oid[oid]:
            idx.setdefault(sha256(data), []).append((c, path, ref, size))
    return idx


def remote_refs() -> dict:
    """{remote-tracking ref: commit} as this clone last fetched them (`git fetch` first for today's state)."""
    out = git("for-each-ref", "--format=%(refname:short) %(objectname)", "refs/remotes").decode().split("\n")
    return dict(line.split() for line in out if line.strip() and not line.endswith("/HEAD"))


def is_public(commit: str, cache: dict) -> bool:
    """Whether a remote-tracking ref contains the commit: only then can anyone outside check the file against it."""
    if commit not in cache:
        r = subprocess.run(["git", "-C", str(ROOT), "for-each-ref", "--contains", commit, "--format=%(refname)",
                            "refs/remotes"], capture_output=True, text=True)
        cache[commit] = r.returncode == 0 and bool(r.stdout.strip())
    return cache[commit]


def build(a) -> int:
    idx = blob_index(a.ref)
    files, unmatched, generated, pub = [], [], [], {}
    for line in Path(a.served).read_text().splitlines():
        h, path = line.split("\t") if "\t" in line else line.split(None, 1)
        path = path.strip().lstrip("./")
        if path in GENERATED:
            generated.append({"path": "/" + path, "why": GENERATED[path]})
            continue
        want = RENAMED.get(path) or next((v + path[len(k):] for k, v in RENAMED_DIRS.items() if path.startswith(k)), None)
        cands = [x for x in idx.get(h, []) if x[1] == want or (not want and (x[1] == path or x[1].endswith("/" + path)))]
        if not cands:
            unmatched.append({"path": "/" + path, "sha256": h})
            continue
        # The same bytes often sit in several commits: name a published one when there is one.
        c, rpath, ref, size = next((x for x in cands if is_public(x[0], pub)), cands[0])
        files.append({"path": "/" + path, "sha256": h, "bytes": size, "commit": c, "repo_path": rpath, "branch": ref,
                      "public": is_public(c, pub)})
    files.sort(key=lambda f: f["path"])
    # The manifest cannot list its own hash; it is the file the other two sources are compared on.
    m = {"about": "Every file the Ferrata LEZ test network's host serves from the repository, with its SHA-256 and "
                  "the commit it comes from. Check it with `python3 -I scripts/site-manifest.py verify` from a clone. "
                  "A file whose commit is not public (\"public\": false) can be checked against this manifest but not "
                  "yet against its source.",
         "site": a.site, "made": a.made, "made_how": a.how,
         "public_checked": {"utc": time.strftime("%Y-%m-%dT%H:%MZ", time.gmtime()), "refs": remote_refs()},
         "files": files, "generated": generated, "unmatched": unmatched}
    Path(a.out).write_text(json.dumps(m, indent=1) + "\n")
    npub = sum(f["public"] for f in files)
    print(f"{len(files)} files matched to the repository ({npub} at public commits), {len(generated)} generated on "
          f"the host, {len(unmatched)} unmatched")
    for u in unmatched:
        print(f"  unmatched {u['path']}")
    return 0


# ---------------------------------------------------------------- verify
def fetch(url: str, ua: str = "ferrata-site-verify/1"):
    """(body, headers, None) or (None, {}, the error): a refused or failed request is a finding, not a crash."""
    req = urllib.request.Request(url, headers={"User-Agent": ua, "Cache-Control": "no-cache"})
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.read(), {k.lower(): v for k, v in r.headers.items()}, None
    except (urllib.error.URLError, OSError, ValueError) as e:
        return None, {}, f"{type(e).__name__}: {getattr(e, 'code', '') or getattr(e, 'reason', '') or e}"


def second_name(site: str):
    """The host's sslip.io name, from its A record (another TLS name and Host header), unless the site is that name."""
    import socket
    host = urllib.parse.urlparse(site).hostname
    if host.endswith(".sslip.io"):
        return None
    try:
        return "https://" + socket.gethostbyname(host).replace(".", "-") + ".sslip.io"
    except OSError:
        return None


def shown(p: Path) -> str:
    """A path as the output may show it: relative to the repository, never this machine's layout."""
    try:
        return str(p.resolve().relative_to(ROOT))
    except ValueError:
        return p.name


def verify(a) -> int:
    site = a.site.rstrip("/")
    fails = []

    def bad(msg):
        fails.append(msg)
        print("FAIL  " + msg)

    alt = None if a.no_second else (a.second.rstrip("/") if a.second else second_name(site))
    print(f"verified {time.strftime('%Y-%m-%d %H:%M:%S UTC', time.gmtime())}; site {site}; second name "
          f"{(a.second_label or alt) if alt else '(none)'}")
    served_bytes, _, err = fetch(site + SERVED_PATH)
    if served_bytes is None:
        bad(f"cannot read the served manifest {SERVED_PATH}: {err}")
        print("VERDICT: FAIL (1)")
        return 1
    if a.github:
        repo_bytes, _, err = fetch(GITHUB.format(ref=a.github))
        src = f"github {a.github}"
        if repo_bytes is None:
            bad(f"cannot read the manifest at GitHub {a.github}: {err}")
            print("VERDICT: FAIL (1)")
            return 1
    else:
        p = Path(a.manifest) if a.manifest else ROOT / MANIFEST_REPO_PATH
        repo_bytes, src = p.read_bytes(), shown(p)
    print(f"manifest served by the host  {sha256(served_bytes)}")
    print(f"manifest in the repository   {sha256(repo_bytes)}  ({src})")
    if served_bytes != repo_bytes:
        bad("the served manifest differs from the repository's copy")
    m = json.loads(repo_bytes)  # the repository's copy is the reference, never the host's
    have_git = (ROOT / ".git").exists()
    ok = unpublished = 0
    for f in m["files"]:
        body, hdr, err = fetch(site + f["path"])
        if body is None:
            bad(f"{f['path']}: cannot fetch ({err})")
            continue
        h = sha256(body)
        line = f"{f['path']:44} {h[:16]}"
        if h != f["sha256"]:
            bad(f"{f['path']}: served {h[:16]}…, manifest {f['sha256'][:16]}…")
            continue
        if have_git:
            try:
                g = sha256(git("show", f"{f['commit']}:{f['repo_path']}"))
                line += "  git " + ("=" if g == f["sha256"] else "DIFFERS")
                if g != f["sha256"]:
                    bad(f"{f['path']}: git {f['commit'][:8]}:{f['repo_path']} has {g[:16]}…")
                    continue
            except subprocess.CalledProcessError:
                line += f"  git: {f['commit'][:8]} not in this clone"
        if not f.get("public", True):
            unpublished += 1
            line += "  (commit not public yet)"
        if alt:
            b2, _, err2 = fetch(alt + f["path"], ua="Mozilla/5.0 (X11; Linux x86_64) Chrome/140 Safari/537.36")
            if b2 is None:
                line += f"  2nd name unreachable ({err2})"
            elif sha256(b2) != h:
                bad(f"{f['path']}: the second name serves {sha256(b2)[:16]}…")
                continue
            else:
                line += "  2nd name ="
        if f["path"].endswith(".html"):
            csp = hdr.get("content-security-policy", "")
            if "script-src 'self'" not in csp:
                bad(f"{f['path']}: no Content-Security-Policy with script-src 'self' (got {csp!r})")
                continue
        ok += 1
        if a.verbose:
            print("ok    " + line)
    _, hdr, err = fetch(site + "/")
    for name, want in (("strict-transport-security", "max-age="), ("x-content-type-options", "nosniff"),
                       ("content-security-policy", "frame-ancestors 'none'")):
        if want not in hdr.get(name, ""):
            bad(f"header {name}: {hdr.get(name)!r} lacks {want!r}" + (f" ({err})" if err else ""))
    print(f"{ok} of {len(m['files'])} files equal the manifest; {len(m.get('generated', []))} generated on the host "
          f"(not hashed); {len(m.get('unmatched', []))} served files the repository does not hold")
    if unpublished:
        print(f"note  {unpublished} of the equal files are at commits no public branch holds yet: they match the "
              f"manifest, but nobody outside the project can check them against their source until those commits "
              f"are published")
    for u in m.get("unmatched", []):
        print(f"note  {u['path']}: served when the manifest was made, matched to no committed file")
    print("VERDICT: " + ("PASS" if not fails else f"FAIL ({len(fails)})"))
    return 1 if fails else 0


def main() -> int:
    site = json.loads((ROOT / "scripts/reproduce/deployed-programs.json").read_text())["network"]  # one place
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="mode", required=True)
    v = sub.add_parser("verify")
    v.add_argument("--site", default=site, help=f"default: the network of scripts/reproduce/deployed-programs.json ({site})")
    v.add_argument("--manifest")
    v.add_argument("--github", metavar="REF")
    v.add_argument("--second", metavar="URL", help="the host's other name, fetched again with a browser's User-Agent "
                   "(default: the sslip.io name of the site's address, when --site is a domain)")
    v.add_argument("--second-label", metavar="TEXT", help="how the output names --second (keeps a record free of it)")
    v.add_argument("--no-second", action="store_true", help="skip the second fetch")
    v.add_argument("-v", "--verbose", action="store_true")
    b = sub.add_parser("build")
    b.add_argument("--served", required=True)
    b.add_argument("--out", required=True)
    b.add_argument("--ref", action="append", required=True)
    b.add_argument("--site", default=site)
    b.add_argument("--made", required=True, help="UTC time the served list was read")
    b.add_argument("--how", default="the host's own file list (sha256sum of its web root)",
                   help="how the served list was obtained")
    a = ap.parse_args()
    return verify(a) if a.mode == "verify" else build(a)


if __name__ == "__main__":
    sys.exit(main())
