"""Ed25519 signatures (RFC 8032), OpenSSH's SSHSIG files (`ssh-keygen -Y sign`, PROTOCOL.sshsig) and minisign
signatures, verified with the Python standard library only, for scripts/verify.py.

Ed25519 verification follows RFC 8032 §5.1.7 with the cofactorless equation [S]B = R + [k]A: the public key A and the
point R must be canonical encodings of curve points, S below the group order L; the check compares the encoding of
[S]B - [k]A with R's bytes. Like ed25519-dalek's `verify_strict` (which the attestors use), it refuses a public key or
an R of small order ([8]P the identity): under such a key a signature can verify for any message.
"""
from __future__ import annotations

import base64
import hashlib

# ---- Ed25519 ------------------------------------------------------------------------------------------------------

Q = 2 ** 255 - 19
L = 2 ** 252 + 27742317777372353535851937790883648493
D = -121665 * pow(121666, Q - 2, Q) % Q
SQRT_M1 = pow(2, (Q - 1) // 4, Q)


def _inv(x):
    return pow(x, Q - 2, Q)


def _add(p1, p2):
    """Extended coordinates (X, Y, Z, T), x = X/Z, y = Y/Z, x y = T/Z (the add-2008-hwcd-3 formulas, a = -1)."""
    x1, y1, z1, t1 = p1
    x2, y2, z2, t2 = p2
    a = (y1 - x1) * (y2 - x2) % Q
    b = (y1 + x1) * (y2 + x2) % Q
    c = 2 * t1 * t2 * D % Q
    d = 2 * z1 * z2 % Q
    e, f, g, h = b - a, d - c, d + c, b + a
    return (e * f % Q, g * h % Q, f * g % Q, e * h % Q)


def _mul(s, pt):
    out = (0, 1, 1, 0)
    while s:
        if s & 1:
            out = _add(out, pt)
        pt = _add(pt, pt)
        s >>= 1
    return out


def _encode(pt) -> bytes:
    x, y, z, _ = pt
    zi = _inv(z)
    x, y = x * zi % Q, y * zi % Q
    return (y | ((x & 1) << 255)).to_bytes(32, "little")


def _decode(b: bytes):
    """A canonical point encoding, or None."""
    if len(b) != 32:
        return None
    n = int.from_bytes(b, "little")
    sign, y = n >> 255, n & ((1 << 255) - 1)
    if y >= Q:
        return None
    u, v = (y * y - 1) % Q, (D * y * y + 1) % Q
    x = u * pow(v, 3, Q) * pow(u * pow(v, 7, Q), (Q - 5) // 8, Q) % Q
    if (v * x * x - u) % Q != 0:
        x = x * SQRT_M1 % Q
        if (v * x * x - u) % Q != 0:
            return None
    if x == 0 and sign:
        return None
    if (x & 1) != sign:
        x = Q - x
    return (x, y, 1, x * y % Q)


_BY = 4 * _inv(5) % Q
_BASE = _decode(_BY.to_bytes(32, "little"))


_IDENTITY = bytes([1]) + bytes(31)


def _small_order(pt) -> bool:
    return _encode(_mul(8, pt)) == _IDENTITY


def ed25519_verify(public: bytes, message: bytes, signature: bytes) -> bool:
    if len(signature) != 64 or len(public) != 32:
        return False
    a, r = _decode(public), _decode(signature[:32])
    if a is None or r is None or _small_order(a) or _small_order(r):
        return False
    s = int.from_bytes(signature[32:], "little")
    if s >= L:
        return False
    k = int.from_bytes(hashlib.sha512(signature[:32] + public + message).digest(), "little") % L
    neg_a = ((Q - a[0]) % Q, a[1], a[2], (Q - a[3]) % Q)
    return _encode(_add(_mul(s, _BASE), _mul(k, neg_a))) == signature[:32]


# ---- OpenSSH wire strings, SSHSIG ---------------------------------------------------------------------------------

def _string(b: bytes, pos: int):
    if pos + 4 > len(b):
        raise ValueError("truncated")
    n = int.from_bytes(b[pos:pos + 4], "big")
    if pos + 4 + n > len(b):
        raise ValueError("truncated")
    return b[pos + 4:pos + 4 + n], pos + 4 + n


def _ssh(*parts: bytes) -> bytes:
    return b"".join(len(p).to_bytes(4, "big") + p for p in parts)


def ssh_ed25519_key(line: str) -> bytes:
    """The 32-byte key of an `ssh-ed25519 AAAA…` line (an authorized-keys or allowed-signers entry)."""
    fields = line.split()
    i = fields.index("ssh-ed25519")
    blob = base64.b64decode(fields[i + 1])
    kind, pos = _string(blob, 0)
    key, pos = _string(blob, pos)
    if kind != b"ssh-ed25519" or len(key) != 32 or pos != len(blob):
        raise ValueError("not an ssh-ed25519 key")
    return key


def ssh_fingerprint(key: bytes) -> str:
    """OpenSSH's SHA256 fingerprint: base64 (unpadded) of SHA-256 over the key's wire blob."""
    blob = _ssh(b"ssh-ed25519", key)
    return "SHA256:" + base64.b64encode(hashlib.sha256(blob).digest()).decode().rstrip("=")


def sshsig_verify(armored: str, message: bytes, key: bytes, namespace: str) -> tuple:
    """(ok, reason) for an armored SSHSIG over `message` by `key` in `namespace` (PROTOCOL.sshsig)."""
    lines = [ln.strip() for ln in armored.strip().splitlines()]
    if not lines or lines[0] != "-----BEGIN SSH SIGNATURE-----" or lines[-1] != "-----END SSH SIGNATURE-----":
        return False, "not an armored SSH signature"
    try:
        blob = base64.b64decode("".join(lines[1:-1]), validate=True)
        if blob[:6] != b"SSHSIG" or int.from_bytes(blob[6:10], "big") != 1:
            return False, "not an SSHSIG version 1 blob"
        pub, pos = _string(blob, 10)
        ns, pos = _string(blob, pos)
        reserved, pos = _string(blob, pos)
        hash_alg, pos = _string(blob, pos)
        sig, pos = _string(blob, pos)
        if pos != len(blob):
            return False, "bytes after the signature"
        kind, p2 = _string(pub, 0)
        signer, p2 = _string(pub, p2)
        if kind != b"ssh-ed25519" or p2 != len(pub):
            return False, "not an ssh-ed25519 signer"
        if signer != key:
            return False, "signed by another key"
        if ns.decode() != namespace:
            return False, f"namespace {ns.decode()!r}, not {namespace!r}"
        hashes = {b"sha512": hashlib.sha512, b"sha256": hashlib.sha256}
        if hash_alg not in hashes:
            return False, f"hash algorithm {hash_alg!r}"
        skind, p3 = _string(sig, 0)
        raw, p3 = _string(sig, p3)
        if skind != b"ssh-ed25519" or p3 != len(sig):
            return False, "not an ssh-ed25519 signature"
        signed = b"SSHSIG" + _ssh(ns, reserved, hash_alg, hashes[hash_alg](message).digest())
        if not ed25519_verify(key, signed, raw):
            return False, "the Ed25519 signature does not verify"
        return True, f"SSHSIG by {ssh_fingerprint(key)}, namespace {namespace!r}, {hash_alg.decode()}"
    except (ValueError, IndexError) as e:
        return False, f"malformed: {e}"


# ---- minisign -----------------------------------------------------------------------------------------------------

def minisign_key(text: str) -> tuple:
    """(key id, 32-byte key) of a minisign public key file."""
    line = [ln for ln in text.splitlines() if ln.strip() and not ln.startswith("untrusted comment:")][0]
    b = base64.b64decode(line.strip(), validate=True)
    if len(b) != 42 or b[:2] != b"Ed":
        raise ValueError("not a minisign Ed25519 public key")
    return b[2:10], b[10:]


def minisign_verify(sigtext: str, message: bytes, pubtext: str) -> tuple:
    """(ok, reason) for a .minisig over `message` with a minisign public key (algorithms Ed and prehashed ED; the
    global signature over the signature and the trusted comment)."""
    try:
        key_id, key = minisign_key(pubtext)
        lines = sigtext.strip().splitlines()
        if len(lines) != 4 or not lines[0].startswith("untrusted comment:") or not lines[2].startswith("trusted comment: "):
            return False, "not a minisign signature file"
        sig = base64.b64decode(lines[1].strip(), validate=True)
        if len(sig) != 74 or sig[:2] not in (b"Ed", b"ED"):
            return False, "not an Ed25519 minisign signature"
        if sig[2:10] != key_id:
            return False, f"key id {sig[2:10][::-1].hex().upper()}, not the public key's"
        signed = hashlib.blake2b(message, digest_size=64).digest() if sig[:2] == b"ED" else message
        if not ed25519_verify(key, signed, sig[10:]):
            return False, "the signature does not verify"
        trusted = lines[2][len("trusted comment: "):].encode()
        glob = base64.b64decode(lines[3].strip(), validate=True)
        if not ed25519_verify(key, sig[10:] + trusted, glob):
            return False, "the global signature (over the trusted comment) does not verify"
        return True, f"minisign {sig[:2].decode()} by key {key_id[::-1].hex().upper()}; trusted comment {trusted.decode()!r}"
    except (ValueError, IndexError) as e:
        return False, f"malformed: {e}"
