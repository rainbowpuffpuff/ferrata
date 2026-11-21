#!/usr/bin/env python3
"""The did:jwk of a release-signing key (modules/scripts/release-publish.sh).

    python3 -I modules/scripts/jwk_did.py KEY.jwk

KEY.jwk is an Ed25519 secret key as `lgx keygen` writes it and logos-modules-release-action's inline
signing reads it: {"crv":"Ed25519","d":<seed>,"kty":"OKP","x":<public key>}, base64url without padding.
Checks the shape, checks that x is the public key of d (RFC 8032, computed here with the standard
library, after a known-answer check), and prints lgx's DID of the public half: did:jwk: + base64url of
{"crv","kty","x"} (keys sorted, no spaces, no padding). Exits 2 on any problem; no message shows d.
"""
import base64, hashlib, json, sys
path = sys.argv[1]
def die(msg):
    sys.exit(f"release-publish.sh: {path}: {msg}")
try:
    with open(path, encoding="utf-8") as f:
        jwk = json.load(f)
except Exception as e:
    die(f"not a JSON file ({type(e).__name__})")
if not isinstance(jwk, dict):
    die("not a JSON object")
for k in ("crv", "d", "kty", "x"):
    if not isinstance(jwk.get(k), str):
        die(f"the JWK has no string field {k!r} (expected an Ed25519 secret key as lgx keygen writes: crv, d, kty, x)")
if jwk["kty"] != "OKP" or jwk["crv"] != "Ed25519":
    die(f"not an Ed25519 OKP JWK (kty={jwk['kty']!r}, crv={jwk['crv']!r})")
def b64u(field):
    s = jwk[field]
    if not s or any(c not in "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_" for c in s):
        die(f"{field} is not unpadded base64url")
    raw = base64.urlsafe_b64decode(s + "=" * (-len(s) % 4))
    if len(raw) != 32:
        die(f"{field} is not 32 bytes")
    return raw

# Ed25519 public key from the 32-byte seed (RFC 8032 section 5.1.5), with a known-answer check first.
P = 2**255 - 19
D = -121665 * pow(121666, P - 2, P) % P
def add(a, b):
    x1, y1, z1, t1 = a
    x2, y2, z2, t2 = b
    A, B = (y1 - x1) * (y2 - x2) % P, (y1 + x1) * (y2 + x2) % P
    C, E2 = 2 * t1 * t2 * D % P, 2 * z1 * z2 % P
    E, F, G, H = B - A, E2 - C, E2 + C, B + A
    return (E * F % P, G * H % P, F * G % P, E * H % P)
def public_key(seed):
    h = hashlib.sha512(seed).digest()
    s = int.from_bytes(h[:32], "little") & ((1 << 254) - 8) | (1 << 254)
    by = 4 * pow(5, P - 2, P) % P
    xx = (by * by - 1) * pow(D * by * by + 1, P - 2, P) % P
    bx = pow(xx, (P + 3) // 8, P)
    if (bx * bx - xx) % P:
        bx = bx * pow(2, (P - 1) // 4, P) % P
    if bx & 1:
        bx = P - bx
    q, b = (0, 1, 1, 0), (bx, by, 1, bx * by % P)
    while s:
        if s & 1:
            q = add(q, b)
        b, s = add(b, b), s >> 1
    zi = pow(q[2], P - 2, P)
    x, y = q[0] * zi % P, q[1] * zi % P
    return (y | (x & 1) << 255).to_bytes(32, "little")
# RFC 8032 section 7.1, TEST 1.
if public_key(bytes.fromhex("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")).hex() != \
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a":
    sys.exit("release-publish.sh: the Ed25519 known-answer check failed")
if public_key(b64u("d")) != b64u("x"):
    die("x is not the public key of d (the declared signer would not be the key that signs)")
pub = {"crv": "Ed25519", "kty": "OKP", "x": jwk["x"]}
print("did:jwk:" + base64.urlsafe_b64encode(json.dumps(pub, separators=(",", ":"), sort_keys=True).encode()).rstrip(b"=").decode())
