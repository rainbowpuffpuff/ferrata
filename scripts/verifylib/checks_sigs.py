"""The signature family: the Basecamp release the test network serves, signed with the release key (EVIDENCE rows 241,
256): its SHA256SUMS verified offline with both signatures (ssh-keygen's SSHSIG and minisign, verifylib/sigs.py) and
the key's fingerprint the documents state; online, every package the sums list downloaded from the network and held
to its line and to its own two signatures."""
from __future__ import annotations

import hashlib
import urllib.error
import urllib.request

from . import sigs
from .core import CheckFailed, Skipped, check, fail_if

DL = "deploy/testnet/web/basecamp/dl"
FINGERPRINT = "SHA256:m5xC7zkg0LzI3YhCtol4qSfgMJGijs84mFWWGk0eX9Y"  # EVIDENCE rows 241 and 256


def _keys(ctx):
    allowed = (ctx.root / DL / "keys/allowed_signers").read_text()
    line = next(ln for ln in allowed.splitlines() if "ssh-ed25519" in ln)
    principal = line.split()[0]
    namespace = "file"
    for field in line.split():
        if field.startswith('namespaces="'):
            namespace = field.split('"')[1]
    key = sigs.ssh_ed25519_key(line)
    pub = sigs.ssh_ed25519_key((ctx.root / DL / "keys/ferrata-release.pub").read_text())
    mini = (ctx.root / DL / "keys/minisign.pub").read_text()
    return principal, namespace, key, pub, mini


@check(id="basecamp-sums-signed", family="signature", mode="offline", evidence=(241, 256),
       rows=("RFP-021 S.9", "RFP-021 S.10", "RFP-022 S.2"),
       title="The Basecamp release's sums, signed with the release key (SSHSIG and minisign), verified locally",
       what=f"{DL}/SHA256SUMS (every package of the release and its SHA-256) with SHA256SUMS.sig and SHA256SUMS.minisig",
       against=f"the committed release key ({DL}/keys/: allowed_signers, ferrata-release.pub, minisign.pub: one "
               f"Ed25519 key) and its fingerprint the documents state, {FINGERPRINT}",
       inputs=(DL,))
def basecamp_sums_signed(ctx):
    principal, namespace, key, pub, mini = _keys(ctx)
    problems = []
    if pub != key:
        problems.append("ferrata-release.pub is not the allowed_signers key")
    if sigs.minisign_key(mini)[1] != key:
        problems.append("minisign.pub is not the same Ed25519 key")
    fp = sigs.ssh_fingerprint(key)
    if fp != FINGERPRINT:
        problems.append(f"the key's fingerprint is {fp}, the documents state {FINGERPRINT}")
    sums = (ctx.root / DL / "SHA256SUMS").read_bytes()
    ok1, why1 = sigs.sshsig_verify((ctx.root / DL / "SHA256SUMS.sig").read_text(), sums, key, namespace)
    ok2, why2 = sigs.minisign_verify((ctx.root / DL / "SHA256SUMS.minisig").read_text(), sums, mini)
    if not ok1:
        problems.append(f"SHA256SUMS.sig: {why1}")
    if not ok2:
        problems.append(f"SHA256SUMS.minisig: {why2}")
    n = sum(1 for ln in sums.decode().splitlines() if ln.strip())
    fail_if(problems, "the Basecamp release's signatures")
    return [f"SHA256SUMS ({n} packages) signed by {principal}: {why1}", f"and {why2}"]


@check(id="basecamp-packages-online", family="signature", mode="online", evidence=(241, 256),
       rows=("RFP-021 S.9", "RFP-022 S.2"),
       title="Every Basecamp package the test network serves equals its signed sum and carries its two signatures",
       what="each package SHA256SUMS lists, downloaded from the network's /basecamp/dl/ (GET only), and its .sig and "
            ".minisig",
       against=f"the committed, signed {DL}/SHA256SUMS and the committed release key",
       inputs=(DL,))
def basecamp_packages_online(ctx):
    principal, namespace, key, _pub, mini = _keys(ctx)
    lines = [ln.split(None, 1) for ln in (ctx.root / DL / "SHA256SUMS").read_text().splitlines() if ln.strip()]
    problems, total = [], 0
    for digest, name in lines:
        name = name.strip().lstrip("*")
        url = f"{ctx.lez_url}/basecamp/dl/{name}"
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "ferrata-verify/1"})
            data = urllib.request.urlopen(req, timeout=120).read()
        except urllib.error.HTTPError as e:
            problems.append(f"{name}: HTTP {e.code}")
            continue
        except (urllib.error.URLError, TimeoutError) as e:
            raise Skipped(f"the network's /basecamp/dl/ could not be read: {e}") from e
        total += len(data)
        if hashlib.sha256(data).hexdigest() != digest:
            problems.append(f"{name}: served {hashlib.sha256(data).hexdigest()[:16]}…, the signed sums say {digest[:16]}…")
            continue
        sig_path, mini_path = ctx.root / DL / (name + ".sig"), ctx.root / DL / (name + ".minisig")
        if sig_path.exists():
            ok, why = sigs.sshsig_verify(sig_path.read_text(), data, key, namespace)
            if not ok:
                problems.append(f"{name}.sig: {why}")
        else:
            problems.append(f"{name}.sig is not committed")
        if mini_path.exists():
            ok, why = sigs.minisign_verify(mini_path.read_text(), data, mini)
            if not ok:
                problems.append(f"{name}.minisig: {why}")
        else:
            problems.append(f"{name}.minisig is not committed")
    if not lines:
        raise CheckFailed("SHA256SUMS lists no package")
    fail_if(problems, "the served Basecamp packages")
    return [f"{len(lines)} of {len(lines)} packages served by {ctx.lez_url}/basecamp/dl/ ({total / 1e6:.1f} MB) equal "
            f"their signed sums, each with its SSHSIG and minisign signature by {principal}"]
