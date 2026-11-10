#!/usr/bin/env python3
"""Independent check of crates/attestor-core/vectors/attestor-v1.json.

Rebuilds every message from the field values as the specification text writes them (SPEC-v3 D3
key-set hash v2, D13 certificate and final-tier message v2, with the certificate binding the
deployment), derives the public keys from the seeds, signs again and verifies, with the RFC 8032
reference Ed25519 (section 6, pure Python, no third-party packages) and hashlib. Shares no code with
the Rust crate.

    python3 -I scripts/check-attestor-vectors.py [path]
"""

import hashlib
import json
import sys
from pathlib import Path

# ---- RFC 8032 section 6 reference implementation ------------------------------------------------

p = 2**255 - 19
q = 2**252 + 27742317777372353535851937790883648493


def modp_inv(x):
    return pow(x, p - 2, p)


d = -121665 * modp_inv(121666) % p
modp_sqrt_m1 = pow(2, (p - 1) // 4, p)


def sha512(s):
    return hashlib.sha512(s).digest()


def sha512_modq(s):
    return int.from_bytes(sha512(s), "little") % q


def point_add(P, Q):
    A, B = (P[1] - P[0]) * (Q[1] - Q[0]) % p, (P[1] + P[0]) * (Q[1] + Q[0]) % p
    C, D = 2 * P[3] * Q[3] * d % p, 2 * P[2] * Q[2] % p
    E, F, G, H = B - A, D - C, D + C, B + A
    return (E * F, G * H, F * G, E * H)


def point_mul(s, P):
    Q = (0, 1, 1, 0)
    while s > 0:
        if s & 1:
            Q = point_add(Q, P)
        P = point_add(P, P)
        s >>= 1
    return Q


def point_equal(P, Q):
    return (P[0] * Q[2] - Q[0] * P[2]) % p == 0 and (P[1] * Q[2] - Q[1] * P[2]) % p == 0


def recover_x(y, sign):
    if y >= p:
        return None
    x2 = (y * y - 1) * modp_inv(d * y * y + 1)
    if x2 == 0:
        return None if sign else 0
    x = pow(x2, (p + 3) // 8, p)
    if (x * x - x2) % p != 0:
        x = x * modp_sqrt_m1 % p
    if (x * x - x2) % p != 0:
        return None
    if (x & 1) != sign:
        x = p - x
    return x


g_y = 4 * modp_inv(5) % p
g_x = recover_x(g_y, 0)
G = (g_x, g_y, 1, g_x * g_y % p)


def point_compress(P):
    zinv = modp_inv(P[2])
    x, y = P[0] * zinv % p, P[1] * zinv % p
    return int.to_bytes(y | ((x & 1) << 255), 32, "little")


def point_decompress(s):
    y = int.from_bytes(s, "little")
    sign = y >> 255
    y &= (1 << 255) - 1
    x = recover_x(y, sign)
    return None if x is None else (x, y, 1, x * y % p)


def secret_expand(secret):
    h = sha512(secret)
    a = int.from_bytes(h[:32], "little")
    a &= (1 << 254) - 8
    a |= 1 << 254
    return a, h[32:]


def secret_to_public(secret):
    a, _ = secret_expand(secret)
    return point_compress(point_mul(a, G))


def sign(secret, msg):
    a, prefix = secret_expand(secret)
    A = point_compress(point_mul(a, G))
    r = sha512_modq(prefix + msg)
    Rs = point_compress(point_mul(r, G))
    h = sha512_modq(Rs + A + msg)
    return Rs + int.to_bytes((r + h * a) % q, 32, "little")


def verify(public, msg, signature):
    A = point_decompress(public)
    R = point_decompress(signature[:32])
    if A is None or R is None:
        return False
    s = int.from_bytes(signature[32:], "little")
    if s >= q:
        return False
    h = sha512_modq(signature[:32] + public + msg)
    return point_equal(point_mul(s, G), point_add(R, point_mul(h, A)))


# ---- self-test: RFC 8032 section 7.1, tests 1 and 2 -----------------------------------------------

for sk, pk, msg, sig in [
    ("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
     "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a", "",
     "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155"
     "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"),
    ("4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
     "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c", "72",
     "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da"
     "085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00"),
]:
    sk, pk, msg, sig = (bytes.fromhex(x) for x in (sk, pk, msg, sig))
    assert secret_to_public(sk) == pk and sign(sk, msg) == sig and verify(pk, msg, sig), "RFC 8032 self-test"

# ---- the vectors ------------------------------------------------------------------------------------


def b(x):
    return bytes.fromhex(x)


path = Path(sys.argv[1]) if len(sys.argv) > 1 else \
    Path(__file__).resolve().parent.parent / "crates/attestor-core/vectors/attestor-v1.json"
vectors = json.loads(path.read_text())["vectors"]
for i, v in enumerate(vectors):
    dep = v["deployment"]
    chain, vault, config, channel = (b(dep[k]) for k in ("evm_chain_id", "vault", "lez_config_id", "channel_id"))
    assert (len(chain), len(vault), len(config), len(channel)) == (32, 20, 32, 32)
    ck_seed, ak_seed = b(v["channel_key_seed"]), b(v["att_key_seed"])
    ck, ak = secret_to_public(ck_seed), secret_to_public(ak_seed)
    assert ck == b(v["channel_key"]) and ak == b(v["att_key"]), f"vector {i}: keys from seeds"
    keys = [b(k) for k in v["key_set"]]
    assert keys[0] == ck, f"vector {i}: the certifying key is in the key set"

    ksh = hashlib.sha256(b"ferrata/keyset/v2" + len(keys).to_bytes(4, "big")
                         + v["posting_timeframe"].to_bytes(4, "big") + b"".join(keys)).digest()
    assert ksh == b(v["key_set_hash"]), f"vector {i}: keySetHash"

    cert_msg = b"ferrata/attest-key/v1" + chain + vault + config + channel + ak
    assert cert_msg == b(v["certificate_message"]) and len(cert_msg) == 169, f"vector {i}: certificate message"
    cert = b(v["certificate"])
    assert sign(ck_seed, cert_msg) == cert and verify(ck, cert_msg, cert), f"vector {i}: certificate"

    tip = v["tip"]
    tip_id, tip_hash, final_msg_id = tip["tip_id"], b(tip["tip_hash"]), b(tip["final_msg_id"])
    att_msg = (b"ferrata/final-tier/v2" + chain + vault + config + channel + ksh
               + tip_id.to_bytes(8, "big") + tip_hash + final_msg_id)
    assert att_msg == b(v["attestation_message"]) and len(att_msg) == 241, f"vector {i}: attestation message"
    sig = b(v["signature"])
    assert sign(ak_seed, att_msg) == sig and verify(ak, att_msg, sig), f"vector {i}: attestation"

    entry = ck + ak + cert + tip_id.to_bytes(8, "little") + tip_hash + final_msg_id + sig
    assert entry == b(v["signed_attestation"]) and len(entry) == 264, f"vector {i}: input entry"

    # Bound fields: a changed byte anywhere in either message breaks its signature.
    for msg, key, s in ((cert_msg, ck, cert), (att_msg, ak, sig)):
        for pos in (0, 25, 60, 100, 136, len(msg) - 1):
            bad = bytearray(msg)
            bad[pos] ^= 1
            assert not verify(key, bytes(bad), s), f"vector {i}: byte {pos} not bound"

print(f"{len(vectors)} attestor vectors verified independently (RFC 8032 reference Ed25519, hashlib SHA-256)")
