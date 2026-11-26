"""RISC Zero's Groth16 seals verified in pure Python (standard library only), for scripts/verify.py.

`verify_seal(seal, image_id, journal_digest)` has the semantics of `RiscZeroGroth16Verifier.verify(seal, imageId,
journalDigest)` of risc0-ethereum (contracts/src/groth16/RiscZeroGroth16Verifier.sol, Groth16Verifier.sol,
IRiscZeroVerifier.sol), releases 3.0.0 and 3.1.0, the two this repository's vaults pin: the seal's 4-byte selector
names the release; the claim digest is `ReceiptClaim.ok(imageId, journalDigest).digest()`; the five public inputs are
the release's control root split in two, the claim digest split in two (`splitDigest`: byte order reversed, low 128
bits first) and its BN254 control id; then the Groth16 pairing check e(-A, B) e(alpha, beta) e(vk_x, gamma)
e(C, delta) = 1 over BN254 (alt_bn128), with the checks EIP-197's precompile makes (coordinates below the field
modulus, points on their curves, B in the order-r subgroup) and the verifier's own (each public input below r).

The verifying keys and control ids are copied from risc0-ethereum v3.0.0 and v3.1.0 (Apache-2.0: Copyright RISC Zero,
Inc.; contracts/src/groth16/Groth16Verifier.sol and ControlID.sol of each release), G2 coordinates reordered from the
precompile's (imaginary, real) to (real, imaginary). The pairing is the optimal ate pairing as py_ecc's bn128 module
computes it (the Miller loop over 6x+2 with the two Frobenius steps, then the final exponentiation (p^12 - 1) / r),
with the loop's points kept on the twist E'(Fp2) and each line evaluated in Fp12 = Fp[w]/(w^12 - 18 w^6 + 82), where
Fp2 = Fp[u]/(u^2 + 1) embeds as u = w^6 - 9 and the twist maps (x, y) to (x w^2, y w^3).

A 3.0.0 seal that verifies shows only that it is well-formed for those circuits: verifier 3.0.0 accepts forged claims
(RISC Zero's advisory GHSA-mm6c-35cm-g4x2, 11 November 2026).
"""
from __future__ import annotations

import hashlib

P = 21888242871839275222246405745257275088696311157297823662689037894645226208583
R = 21888242871839275222246405745257275088548364400416034343698204186575808495617
ATE_LOOP = 29793968203157093288  # 6x + 2; its top bit is the loop's starting point R = Q
SYSTEM_STATE_ZERO = bytes.fromhex("a3acc27117418996340b84e5a90f3ef4c49d22c79e44aad822ec9c313e1eb8e2")

PARAMS = {
    "3.0.0": {
        "selector": "73c457ba",
        "alpha": (20491192805390485299153009773594534940189261866228447918068658471970481763042,
                  9383485363053290200918347156157836566562967994039712273449902621266178545958),
        "beta": ((6375614351688725206403948262868962793625744043794305715222011528459656738731,
                  4252822878758300859123897981450591353533073413197771768651442665752259397132),
                 (10505242626370262277552901082094356697409835680220590971873171140371331206856,
                  21847035105528745403288232691147584728191162732299865338377159692350059136679)),
        "gamma": ((10857046999023057135944570762232829481370756359578518086990519993285655852781,
                  11559732032986387107991004021392285783925812861821192530917403151452391805634),
                 (8495653923123431417604973247489272438418190587263600148770280649306958101930,
                  4082367875863433681332203403145435568316851327593401208105741076214120093531)),
        "delta": ((12043754404802191763554326994664886008979042643626290185762540825416902247219,
                  1668323501672964604911431804142266013250380587483576094566949227275849579036),
                 (13740680757317479711909903993315946540841369848973133181051452051592786724563,
                  7710631539206257456743780535472368339139328733484942210876916214502466455394)),
        "ic": [(8446592859352799428420270221449902464741693648963397251242447530457567083492,
                1064796367193003797175961162477173481551615790032213185848276823815288302804),
               (3179835575189816632597428042194253779818690147323192973511715175294048485951,
                20895841676865356752879376687052266198216014795822152491318012491767775979074),
               (5332723250224941161709478398807683311971555792614491788690328996478511465287,
                21199491073419440416471372042641226693637837098357067793586556692319371762571),
               (12457994489566736295787256452575216703923664299075106359829199968023158780583,
                19706766271952591897761291684837117091856807401404423804318744964752784280790),
               (19617808913178163826953378459323299110911217259216006187355745713323154132237,
                21663537384585072695701846972542344484111393047775983928357046779215877070466),
               (6834578911681792552110317589222010969491336870276623105249474534788043166867,
                15060583660288623605191393599883223885678013570733629274538391874953353488393)],
        "control_root": "a54dc85ac99f851c92d7c96d7318af41dbe7c0194edfcc37eb4d422a998c1f56",
        "bn254_control_id": "04446e66d300eb7fb45c9726bb53c793dda407a62e9601618bb43c5c14657ac0",
    },
    "3.1.0": {
        "selector": "e71e6620",
        "alpha": (16428432848801857252194528405604668803277877773566238944394625302971855135431,
                  16846502678714586896801519656441059708016666274385668027902869494772365009666),
        "beta": ((16348171800823588416173124589066524623406261996681292662100840445103873053252,
                  3182164110458002340215786955198810119980427837186618912744689678939861918171),
                 (19687132236965066906216944365591810874384658708175106803089633851114028275753,
                  4920802715848186258981584729175884379674325733638798907835771393452862684714)),
        "gamma": ((10857046999023057135944570762232829481370756359578518086990519993285655852781,
                  11559732032986387107991004021392285783925812861821192530917403151452391805634),
                 (8495653923123431417604973247489272438418190587263600148770280649306958101930,
                  4082367875863433681332203403145435568316851327593401208105741076214120093531)),
        "delta": ((15776672394616990683767400715354533957990327248900631669473166016258279232588,
                  15616655295497688016920922285786248029410410634174255119755032442279507261612),
                 (6666730711543397578682474175129345128534747409410156889826997147291122518312,
                  15603769682755083227316504195454390494226357053638046843688553601570183264395)),
        "ic": [(18663918579753010382002747204100316475180663666125247285366810668134508578264,
                17189072794432247265173990289507014998311702955803346313941168113828133843434),
               (10995657406716404592042449088731908804822551193011116024160605390904112699922,
                16840054500543522560166231854936072122401577171365452702445604851845250658027),
               (17223746139751910493708334502747005647365937123243899090309734281916083451762,
                4092210736358501801028887881307062309243791997444282548134854129284894411044),
               (3765213192101990618643617040373059738050185238026301923348610794767426428033,
                12210223264334159465299028585581460715942724522871187397965100355295734194591),
               (10211980968769599651826920300372241897389214017084266564307091587192484525770,
                6627619408770540710319105693121261544533216204749218194792386277379533604252),
               (13454640350766589059799184485082639692652215534373479852236969801352010275184,
                8731676129773990833613417922371950977268622282768299686490610454524284784606)],
        "control_root": "e6feef13db7aed4a731c7b0852de1f5c89b137726073f625d05333184d3a551d",
        "bn254_control_id": "0ac2eaf9d5ae64ffb75c4747fde32456e9ee6db3b32350cb2a0614ba91d081de",
    },
}
SELECTORS = {bytes.fromhex(v["selector"]): k for k, v in PARAMS.items()}


class Invalid(ValueError):
    pass


# ---- Fp2 = Fp[u] / (u^2 + 1): (a, b) is a + b u ------------------------------------------------------------------

def f2_add(x, y):
    return ((x[0] + y[0]) % P, (x[1] + y[1]) % P)


def f2_sub(x, y):
    return ((x[0] - y[0]) % P, (x[1] - y[1]) % P)


def f2_mul(x, y):
    a, b = x
    c, d = y
    return ((a * c - b * d) % P, (a * d + b * c) % P)


def f2_inv(x):
    a, b = x
    n = (a * a + b * b) % P
    if n == 0:
        raise Invalid("division by zero in Fp2")
    n = pow(n, P - 2, P)
    return (a * n % P, -b * n % P)


def f2_pow(x, e):
    out = (1, 0)
    while e:
        if e & 1:
            out = f2_mul(out, x)
        x = f2_mul(x, x)
        e >>= 1
    return out


XI = (9, 1)  # 9 + u: w^6 = xi
B_TWIST = f2_mul((3, 0), f2_inv(XI))  # E': y^2 = x^3 + 3 / (9 + u)
FROB_X = f2_pow(XI, (P - 1) // 3)  # the twist's Frobenius: (x, y) -> (conj(x) xi^((p-1)/3), conj(y) xi^((p-1)/2))
FROB_Y = f2_pow(XI, (P - 1) // 2)


# ---- Fp12 = Fp[w] / (w^12 - 18 w^6 + 82): a list of 12 coefficients ----------------------------------------------

ONE12 = [1] + [0] * 11


def f12_mul(x, y):
    t = [0] * 23
    for i, a in enumerate(x):
        if a:
            for j, b in enumerate(y):
                if b:
                    t[i + j] += a * b
    for k in range(22, 11, -1):  # w^k = 18 w^(k-6) - 82 w^(k-12)
        c = t[k]
        if c:
            t[k - 6] += 18 * c
            t[k - 12] -= 82 * c
    return [v % P for v in t[:12]]


def f12_pow(x, e):
    out = ONE12
    for bit in bin(e)[2:]:
        out = f12_mul(out, out)
        if bit == "1":
            out = f12_mul(out, x)
    return out


def _put(c, k, z):
    """Add the Fp2 element z times w^k to the Fp12 coefficients c (z = a + b u = (a - 9 b) + b w^6)."""
    c[k] = (c[k] + z[0] - 9 * z[1]) % P
    c[k + 6] = (c[k + 6] + z[1]) % P


# ---- the curves ---------------------------------------------------------------------------------------------------

def on_g1(pt) -> bool:
    x, y = pt
    return (y * y - x * x * x - 3) % P == 0


def on_twist(pt) -> bool:
    x, y = pt
    return f2_sub(f2_mul(y, y), f2_add(f2_mul(f2_mul(x, x), x), B_TWIST)) == (0, 0)


def g1_add(p1, p2):
    if p1 is None:
        return p2
    if p2 is None:
        return p1
    (x1, y1), (x2, y2) = p1, p2
    if x1 == x2:
        if (y1 + y2) % P == 0:
            return None
        lam = 3 * x1 * x1 * pow(2 * y1, P - 2, P) % P
    else:
        lam = (y2 - y1) * pow(x2 - x1, P - 2, P) % P
    x3 = (lam * lam - x1 - x2) % P
    return (x3, (lam * (x1 - x3) - y1) % P)


def g1_mul(pt, k):
    out = None
    while k:
        if k & 1:
            out = g1_add(out, pt)
        pt = g1_add(pt, pt)
        k >>= 1
    return out


def g2_add(p1, p2):
    if p1 is None:
        return p2
    if p2 is None:
        return p1
    (x1, y1), (x2, y2) = p1, p2
    if x1 == x2:
        if f2_add(y1, y2) == (0, 0):
            return None
        lam = f2_mul(f2_mul((3, 0), f2_mul(x1, x1)), f2_inv(f2_add(y1, y1)))
    else:
        lam = f2_mul(f2_sub(y2, y1), f2_inv(f2_sub(x2, x1)))
    x3 = f2_sub(f2_sub(f2_mul(lam, lam), x1), x2)
    return (x3, f2_sub(f2_mul(lam, f2_sub(x1, x3)), y1))


def g2_mul(pt, k):
    out = None
    while k:
        if k & 1:
            out = g2_add(out, pt)
        pt = g2_add(pt, pt)
        k >>= 1
    return out


def g2_frobenius(pt):
    (x0, x1), (y0, y1) = pt
    return (f2_mul((x0, -x1 % P), FROB_X), f2_mul((y0, -y1 % P), FROB_Y))


# ---- the pairing --------------------------------------------------------------------------------------------------

def _line(r, lam, p):
    """The line through the twisted R with slope lam w (lam in Fp2), at the G1 point P, in Fp12:
    lam w (xP - xR w^2) - (yP - yR w^3) = lam xP w + (yR - lam xR) w^3 - yP."""
    (xr, yr), (xp, yp) = r, p
    c = [0] * 12
    c[0] = -yp % P
    _put(c, 1, (lam[0] * xp % P, lam[1] * xp % P))
    _put(c, 3, f2_sub(yr, f2_mul(lam, xr)))
    return c


def _step(f, r, q, p):
    """f times the line through R and Q (the tangent when R = Q) at P; returns (f, R + Q)."""
    (x1, y1), (x2, y2) = r, q
    if x1 == x2 and y1 == y2:
        lam = f2_mul(f2_mul((3, 0), f2_mul(x1, x1)), f2_inv(f2_add(y1, y1)))
    elif x1 == x2:
        raise Invalid("a vertical line in the Miller loop (a point outside the subgroup)")
    else:
        lam = f2_mul(f2_sub(y2, y1), f2_inv(f2_sub(x2, x1)))
    f = f12_mul(f, _line(r, lam, p))
    x3 = f2_sub(f2_sub(f2_mul(lam, lam), x1), x2)
    return f, (x3, f2_sub(f2_mul(lam, f2_sub(x1, x3)), y1))


def miller_loop(q, p):
    """The Miller loop of the optimal ate pairing e(P, Q) for P in G1, Q in G2 (before the final exponentiation)."""
    if q is None or p is None:
        return ONE12
    r, f = q, ONE12
    for i in range(ATE_LOOP.bit_length() - 2, -1, -1):
        f = f12_mul(f, f)
        f, r = _step(f, r, r, p)
        if (ATE_LOOP >> i) & 1:
            f, r = _step(f, r, q, p)
    q1 = g2_frobenius(q)
    q2 = g2_frobenius(q1)
    nq2 = (q2[0], ((-q2[1][0]) % P, (-q2[1][1]) % P))
    f, r = _step(f, r, q1, p)
    f, _ = _step(f, r, nq2, p)
    return f


FINAL_EXP = (P ** 12 - 1) // R


def pairing_product_is_one(pairs) -> bool:
    f = ONE12
    for p, q in pairs:
        f = f12_mul(f, miller_loop(q, p))
    return f12_pow(f, FINAL_EXP) == ONE12


# ---- RISC Zero's Groth16 receipts ---------------------------------------------------------------------------------

def _sha(*parts: bytes) -> bytes:
    h = hashlib.sha256()
    for x in parts:
        h.update(x)
    return h.digest()


def claim_digest(image_id: bytes, journal_digest: bytes) -> bytes:
    """`ReceiptClaimLib.ok(imageId, journalDigest).digest()` (IRiscZeroVerifier.sol)."""
    output = _sha(_sha(b"risc0.Output"), journal_digest, bytes(32), (2 << 8).to_bytes(2, "big"))
    return _sha(_sha(b"risc0.ReceiptClaim"), bytes(32), image_id, SYSTEM_STATE_ZERO, output,
                bytes(4), bytes(4), (4 << 8).to_bytes(2, "big"))


def split_digest(d: bytes):
    """`splitDigest`: the byte order reversed, then (low 128 bits, high 128 bits)."""
    rev = int.from_bytes(d, "little")
    return rev & ((1 << 128) - 1), rev >> 128


def _g1(x: int, y: int):
    if x >= P or y >= P:
        raise Invalid("a G1 coordinate is not below the field modulus")
    if x == 0 and y == 0:
        return None
    if not on_g1((x, y)):
        raise Invalid("a G1 point is not on the curve")
    return (x, y)


def _g2(xr: int, xi: int, yr: int, yi: int, check_subgroup: bool = True):
    if max(xr, xi, yr, yi) >= P:
        raise Invalid("a G2 coordinate is not below the field modulus")
    if xr == xi == yr == yi == 0:
        return None
    pt = ((xr, xi), (yr, yi))
    if not on_twist(pt):
        raise Invalid("a G2 point is not on the twist")
    if check_subgroup and g2_mul(pt, R) is not None:
        raise Invalid("a G2 point is not in the order-r subgroup")
    return pt


def verify_seal(seal: bytes, image_id: bytes, journal_digest: bytes):
    """(ok, release, reason): `RiscZeroGroth16Verifier.verify(seal, imageId, journalDigest)` of the release the seal's
    selector names; never raises on malformed input."""
    try:
        if len(image_id) != 32 or len(journal_digest) != 32:
            return False, "", "the image id and the journal digest are 32 bytes each"
        release = SELECTORS.get(bytes(seal[:4]))
        if release is None:
            return False, "", f"selector {bytes(seal[:4]).hex()} is neither 3.0.0's nor 3.1.0's"
        if len(seal) != 4 + 8 * 32:
            return False, release, f"a seal is 260 bytes, this one {len(seal)}"
        w = [int.from_bytes(seal[4 + 32 * i: 36 + 32 * i], "big") for i in range(8)]
        prm = PARAMS[release]
        a = _g1(w[0], w[1])
        b = _g2(w[3], w[2], w[5], w[4])  # the precompile's order: x imaginary, x real, y imaginary, y real
        c = _g1(w[6], w[7])
        c0, c1 = split_digest(claim_digest(image_id, journal_digest))
        r0, r1 = split_digest(bytes.fromhex(prm["control_root"]))
        pub = [r0, r1, c0, c1, int(prm["bn254_control_id"], 16)]
        if any(x >= R for x in pub):
            return False, release, "a public input is not below the group order"
        vk_x = prm["ic"][0]
        for i, s in enumerate(pub, 1):
            vk_x = g1_add(vk_x, g1_mul(prm["ic"][i], s))
        neg_a = None if a is None else (a[0], (-a[1]) % P)
        pairs = [(neg_a, b), (prm["alpha"], _vk_g2(release, "beta")), (vk_x, _vk_g2(release, "gamma")),
                 (c, _vk_g2(release, "delta"))]
        if pairing_product_is_one(pairs):
            return True, release, ""
        return False, release, "the pairing check fails"
    except Invalid as e:
        return False, "", str(e)


_VK_CACHE = {}


def _vk_g2(release: str, name: str):
    key = (release, name)
    if key not in _VK_CACHE:
        (xr, xi), (yr, yi) = PARAMS[release][name]
        _VK_CACHE[key] = _g2(xr, xi, yr, yi)
    return _VK_CACHE[key]
