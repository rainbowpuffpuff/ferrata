#!/usr/bin/env python3
"""The did:jwk of a release-signing key (modules/scripts/release-publish.sh).

    python3 -I modules/scripts/jwk_did.py KEY.jwk

KEY.jwk is an Ed25519 secret key as `lgx keygen` writes it and logos-modules-release-action's inline
signing reads it: {"crv":"Ed25519","d":<seed>,"kty":"OKP","x":<public key>}, base64url without padding.
Checks the shape, checks that x is the public key of d (RFC 8032 section 5.1.5, computed here with the
standard library, after known-answer checks), and prints lgx's DID of the public half: did:jwk: + base64url
of {"crv","kty","x"} (keys sorted, no spaces, no padding). Exits 2 on any problem; no message shows d.

The curve arithmetic is this project's own: affine coordinates and the twisted Edwards addition law, from the
curve's definition (RFC 8032 section 5.1, RFC 7748 section 4.1), not RFC 8032 section 6's reference code; it
is checked against RFC 8032 section 7.1's test vectors before any key is read.
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

# Ed25519 public key from the 32-byte seed (RFC 8032 section 5.1.5). The group: -x^2 + y^2 = 1 + d*x^2*y^2
# over GF(2^255 - 19), d = -121665/121666, written in affine coordinates; its addition law is complete (d is not
# a square), so one formula adds and doubles. Slow (two inversions per step) and fine for one key.
FIELD = 2**255 - 19
CURVE_D = -121665 * pow(121666, FIELD - 2, FIELD) % FIELD
BASE = (15112221349535400772501151409588531511454012693041857206046113283949847762202,
        46316835694926478169428394003475163141307993866256225615783033603165251855960)


def inverse(v):
    return pow(v, FIELD - 2, FIELD)


def on_curve(point):
    x, y = point
    return (y * y - x * x - 1 - CURVE_D * x * x * y * y) % FIELD == 0


def edwards_add(p1, p2):
    (x1, y1), (x2, y2) = p1, p2
    t = CURVE_D * x1 * x2 * y1 * y2 % FIELD
    return ((x1 * y2 + y1 * x2) * inverse(1 + t) % FIELD, (y1 * y2 + x1 * x2) * inverse(1 - t) % FIELD)


def times(n, point):
    """n * point, left to right: double, then add when the bit is set."""
    acc = (0, 1)  # the neutral element
    for bit in format(n, "b"):
        acc = edwards_add(acc, acc)
        if bit == "1":
            acc = edwards_add(acc, point)
    return acc


def public_key(seed):
    h = hashlib.sha512(seed).digest()
    n = int.from_bytes(h[:32], "little")
    n &= ~7  # a multiple of the cofactor 8
    n &= (1 << 255) - 1
    n |= 1 << 254  # bit 254 set, bit 255 clear
    x, y = times(n, BASE)
    return (y | (x & 1) << 255).to_bytes(32, "little")


# The base point is the curve's (y = 4/5, x even), and RFC 8032 section 7.1's TEST 1 and TEST 2 hold.
if not (on_curve(BASE) and BASE[1] == 4 * inverse(5) % FIELD and BASE[0] % 2 == 0):
    sys.exit("release-publish.sh: the Ed25519 base point check failed")
for seed_hex, pub_hex in (
        ("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
         "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"),
        ("4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
         "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c")):
    if public_key(bytes.fromhex(seed_hex)).hex() != pub_hex:
        sys.exit("release-publish.sh: the Ed25519 known-answer check failed")
if public_key(b64u("d")) != b64u("x"):
    die("x is not the public key of d (the declared signer would not be the key that signs)")
pub = {"crv": "Ed25519", "kty": "OKP", "x": jwk["x"]}
print("did:jwk:" + base64.urlsafe_b64encode(json.dumps(pub, separators=(",", ":"), sort_keys=True).encode()).rstrip(b"=").decode())
