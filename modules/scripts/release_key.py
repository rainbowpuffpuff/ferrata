#!/usr/bin/env python3
"""One Ed25519 release key in the three forms the Basecamp release is signed with (basecamp-release.sh keygen).

    python3 -I modules/scripts/release_key.py KEYFILE

KEYFILE is an unencrypted OpenSSH Ed25519 private key (`ssh-keygen -t ed25519 -N ''`), the form `ssh-keygen -Y sign`
uses. Beside it this writes, mode 600:
  KEYFILE.jwk          the same key as the JWK `lgx sign` takes ({"crv","d","kty","x"}, base64url, no padding);
  KEYFILE.minisign     the same key as an unencrypted minisign secret key (`minisign -S -s`);
and, public: KEYFILE.minisign.pub (`minisign -V -p`). It prints one JSON line: the public key's 32 bytes in hex,
the minisign key id. Nothing it prints or writes publicly contains the secret.

Formats: PROTOCOL.key of OpenSSH (openssh-key-v1: cipher and KDF "none", one key, the private section holding the
64-byte seed || public key); minisign's key files as minisign.c writes them with -W (signature algorithm "Ed",
KDF algorithm 0, checksum algorithm "B2": BLAKE2b-256 of the algorithm, the key id and the 64-byte secret key).
The key id is the first 8 bytes of SHA-256 of the public key (minisign draws it at random; any 8 bytes do).
"""
import base64, hashlib, json, os, struct, sys


def die(msg):
    sys.exit(f"release_key.py: {msg}")


def b64u(b):
    return base64.urlsafe_b64encode(b).rstrip(b"=").decode()


def read_string(buf, off):
    (n,) = struct.unpack(">I", buf[off:off + 4])
    return buf[off + 4:off + 4 + n], off + 4 + n


def parse_openssh(path):
    text = open(path, encoding="ascii").read().strip().splitlines()
    if text[0] != "-----BEGIN OPENSSH PRIVATE KEY-----" or text[-1] != "-----END OPENSSH PRIVATE KEY-----":
        die("not an OpenSSH private key")
    buf = base64.b64decode("".join(text[1:-1]))
    magic = b"openssh-key-v1\0"
    if not buf.startswith(magic):
        die("not openssh-key-v1")
    off = len(magic)
    cipher, off = read_string(buf, off)
    kdf, off = read_string(buf, off)
    _kdfopts, off = read_string(buf, off)
    if cipher != b"none" or kdf != b"none":
        die("the key is encrypted (make it with -N '')")
    (nkeys,) = struct.unpack(">I", buf[off:off + 4])
    off += 4
    if nkeys != 1:
        die("expected one key")
    pubblob, off = read_string(buf, off)
    priv, off = read_string(buf, off)
    ktype, p = read_string(pubblob, 0)
    pub, _ = read_string(pubblob, p)
    if ktype != b"ssh-ed25519" or len(pub) != 32:
        die("not an Ed25519 key")
    c1, c2 = struct.unpack(">II", priv[:8])
    if c1 != c2:
        die("check words differ")
    p = 8
    ktype2, p = read_string(priv, p)
    pub2, p = read_string(priv, p)
    sk, p = read_string(priv, p)
    if ktype2 != b"ssh-ed25519" or pub2 != pub or len(sk) != 64 or sk[32:] != pub:
        die("inconsistent private section")
    return sk[:32], pub


def write_private(path, data):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w", encoding="ascii") as f:
        f.write(data)


def main():
    if len(sys.argv) != 2:
        die("usage: release_key.py KEYFILE")
    path = sys.argv[1]
    seed, pub = parse_openssh(path)
    jwk = {"crv": "Ed25519", "d": b64u(seed), "kty": "OKP", "x": b64u(pub)}
    write_private(path + ".jwk", json.dumps(jwk, separators=(",", ":"), sort_keys=True) + "\n")
    key_id = hashlib.sha256(pub).digest()[:8]
    sk64 = seed + pub
    chk = hashlib.blake2b(b"Ed" + key_id + sk64, digest_size=32).digest()
    # sig_alg, kdf_alg (none), chk_alg, kdf salt (32), opslimit (8), memlimit (8), key id, secret key, checksum
    blob = b"Ed" + b"\0\0" + b"B2" + bytes(32) + bytes(8) + bytes(8) + key_id + sk64 + chk
    write_private(path + ".minisign",
                  "untrusted comment: minisign secret key (unencrypted)\n" + base64.b64encode(blob).decode() + "\n")
    kid = key_id[::-1].hex().upper()  # minisign prints the id as a little-endian 64-bit number
    with open(path + ".minisign.pub", "w", encoding="ascii") as f:
        f.write(f"untrusted comment: minisign public key {kid}\n" + base64.b64encode(b"Ed" + key_id + pub).decode() + "\n")
    print(json.dumps({"public_key_hex": pub.hex(), "minisign_key_id": kid}))


main()
