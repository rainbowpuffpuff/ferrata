"""Checks of the internal security review of 27 November 2026 (docs/security/internal-review-2026-11-27.md), offline.

`r8-risc0-selectors` re-derives the selector of RISC Zero's Groth16 verifier, releases 3.0.0 and 3.1.0, from the
verifying key and control ids the pure-Python verifier (verifylib/groth16.py) uses: the selector is the first four
bytes of the digest of `Groth16ReceiptVerifierParameters` (control root, BN254 control id, verifying-key digest), as
risc0-ethereum's `RiscZeroGroth16Verifier` constructor computes it. A selector that comes out right ties every one of
those numbers to the verifier the vault deploys (its `SELECTOR()` is checked on the contract by
vault/test/Verifier31.t.sol and by the deployment script). Where vault/lib holds risc0-ethereum v3.1.0, the constants
of its ControlID.sol and Groth16Verifier.sol are compared with the copied ones too.

`r8-phase-split` reads the committed Cargo lockfiles: every workspace that makes a proof the vault takes (phase 1 of
docs/design/risc0-3.1-migration.md) locks risc0-zkvm 3.1.0, risc0-groth16 3.0.6 and risc0-circuit-recursion 4.1.0;
the LEZ-linked workspaces (phase 2, our port to LEZ v0.4.0 on risc0 3.1, not started) are listed with what they lock, and RFP-022
R.8's row must say what they lock, in its phrase "LEZ verifies private transactions with risc0-zkvm X" (a row that
still says 3.0.5 after LEZ moved, or the reverse, fails).
"""
from __future__ import annotations

import hashlib
import importlib.util
import re
from pathlib import Path

from .core import CheckFailed, check, fail_if

LOCK = "vault/lib.lock"
MIGRATION = "docs/design/risc0-3.1-migration.md"
STATUS = "docs/requirements-status.md"
RISC0_SRC = "vault/lib/risc0-ethereum/contracts/src/groth16"

# Phase 1 (docs/design/risc0-3.1-migration.md, "Phase 1: Ethereum side"): the guests and hosts that make the proofs
# the vault verifies. Each must lock exactly these versions.
PHASE1 = ("bridge/Cargo.lock", "bridge/guests/Cargo.lock", "checkpoint/Cargo.lock", "checkpoint/guests/Cargo.lock",
          "checkpoint/guests-plain/Cargo.lock", "checkpoint/guests-0-3-5/Cargo.lock", "gpu-prover/prover/Cargo.lock")
PHASE1_VERSIONS = {"risc0-zkvm": "3.1.0", "risc0-groth16": "3.0.6", "risc0-circuit-recursion": "4.1.0"}
# Phase 2 (LEZ-linked: LEZ's own state machine, our LEZ programs, the CLI and the Logos modules, which link LEZ's
# wallet): they lock LEZ's risc0 until our port to LEZ v0.4.0 (risc0 3.1). The first one decides what R.8's row must say.
PHASE2 = ("lez/Cargo.lock", "cli/Cargo.lock", "lez/guests/bridge/Cargo.lock", "lez/guests/program/Cargo.lock",
          "modules/ferrata_bridge_module/rust-lib/Cargo.lock", "modules/ferrata_relayer_module/rust-lib/Cargo.lock")


def _groth16(root: Path):
    """verifylib/groth16.py of the tree under check (a tampered copy's own, not this process's import)."""
    spec = importlib.util.spec_from_file_location("groth16_under_check", root / "scripts/verifylib/groth16.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _sha(b: bytes) -> bytes:
    return hashlib.sha256(b).digest()


def _u256(x: int) -> bytes:
    return x.to_bytes(32, "big")


def tagged_struct(tag: str, down: list) -> bytes:
    """risc0's `StructHash.taggedStruct(sha256(tag), down)`: sha256(sha256(tag) || down... || len(down) as u16 LE)."""
    return _sha(_sha(tag.encode()) + b"".join(down) + len(down).to_bytes(2, "little"))


def tagged_list(tag: str, items: list) -> bytes:
    """`StructHash.taggedList`: cons cells from the last item to the first, starting at the zero digest."""
    cur = bytes(32)
    for item in reversed(items):
        cur = tagged_struct(tag, [item, cur])
    return cur


def _g2_words(pt) -> bytes:
    """A G2 point of groth16.PARAMS, ((x real, x imaginary), (y real, y imaginary)), in the Solidity constants' order
    x1, x2, y1, y2 = (x imaginary, x real, y imaginary, y real)."""
    (xr, xi), (yr, yi) = pt
    return _u256(xi) + _u256(xr) + _u256(yi) + _u256(yr)


def verifier_key_digest(p: dict) -> bytes:
    """`RiscZeroGroth16Verifier.verifier_key_digest()` of risc0-ethereum v3.0.0 and v3.1.0."""
    ic = [_sha(_u256(x) + _u256(y)) for x, y in p["ic"]]
    return tagged_struct("risc0_groth16.VerifyingKey", [
        _sha(_u256(p["alpha"][0]) + _u256(p["alpha"][1])),
        _sha(_g2_words(p["beta"])),
        _sha(_g2_words(p["gamma"])),
        _sha(_g2_words(p["delta"])),
        tagged_list("risc0_groth16.VerifyingKey.IC", ic),
    ])


def selector(p: dict) -> str:
    """`SELECTOR = bytes4(sha256(tag || control_root || reversed(bn254_control_id) || vk digest || 3 as u16 LE))`."""
    control_root = bytes.fromhex(p["control_root"])
    bn254 = bytes.fromhex(p["bn254_control_id"])[::-1]
    return tagged_struct("risc0.Groth16ReceiptVerifierParameters",
                         [control_root, bn254, verifier_key_digest(p)])[:4].hex()


def _solidity_constants(text: str) -> dict:
    out = {}
    for m in re.finditer(r"uint256 constant (\w+) =\s*(\d+);", text):
        out[m.group(1)] = int(m.group(2))
    for m in re.finditer(r'bytes32 public constant (\w+) =\s*hex"([0-9a-fA-F]{64})";', text):
        out[m.group(1)] = m.group(2).lower()
    return out


def _expected_constants(p: dict) -> dict:
    """The names and values risc0-ethereum's Groth16Verifier.sol and ControlID.sol give the parameters `p` holds."""
    want = {"alphax": p["alpha"][0], "alphay": p["alpha"][1],
            "CONTROL_ROOT": p["control_root"], "BN254_CONTROL_ID": p["bn254_control_id"]}
    for name in ("beta", "gamma", "delta"):
        (xr, xi), (yr, yi) = p[name]
        want.update({f"{name}x1": xi, f"{name}x2": xr, f"{name}y1": yi, f"{name}y2": yr})
    for i, (x, y) in enumerate(p["ic"]):
        want[f"IC{i}x"], want[f"IC{i}y"] = x, y
    return want


@check(id="r8-risc0-selectors", family="hash", mode="offline", tier="quick", rows=("RFP-022 R.8",),
       evidence=(231, 232),
       title="RISC Zero's Groth16 selectors re-derived from the verifying keys and control ids (3.0.0 and 3.1.0)",
       what="each release's selector, computed as RiscZeroGroth16Verifier's constructor computes it (the digest of the "
            "control root, the BN254 control id and the verifying key's digest), and, where vault/lib holds "
            "risc0-ethereum v3.1.0, every constant of its ControlID.sol and Groth16Verifier.sol",
       against="the selectors 0x73c457ba (3.0.0) and 0xe71e6620 (3.1.0) that verifylib/groth16.py, "
               "vault/test/Verifier31.t.sol and the deployment script use; vault/lib.lock's risc0-ethereum@v3.1.0",
       inputs=("scripts/verifylib/groth16.py", LOCK, MIGRATION))
def r8_selectors(ctx):
    problems, lines = [], []
    groth16 = _groth16(ctx.root)
    for release, want in (("3.0.0", "73c457ba"), ("3.1.0", "e71e6620")):
        p = groth16.PARAMS[release]
        got = selector(p)
        if p["selector"] != want:
            problems.append(f"{release}: groth16.py records selector {p['selector']}, the release's is {want}")
        if got != want:
            problems.append(f"{release}: the parameters in groth16.py give selector {got}, not {want}: a verifying "
                            "key or control id differs from the release's")
        else:
            lines.append(f"{release}: selector 0x{got} re-derived from its control root, BN254 control id and "
                         f"verifying key ({len(p['ic'])} IC points)")
    lock = ctx.path(LOCK).read_text()
    if not re.search(r"^risc0-ethereum\s+risc0/risc0-ethereum@v3\.1\.0\s+[0-9a-f]{64}$", lock, re.M):
        problems.append(f"{LOCK}: no risc0-ethereum@v3.1.0 pin with a tree hash")
    src = ctx.path(RISC0_SRC)
    if (src / "Groth16Verifier.sol").exists() and (src / "ControlID.sol").exists():
        have = _solidity_constants((src / "Groth16Verifier.sol").read_text() + (src / "ControlID.sol").read_text())
        want = _expected_constants(groth16.PARAMS["3.1.0"])
        bad = [k for k, v in want.items() if have.get(k) != v]
        if bad:
            problems.append(f"{RISC0_SRC}: {len(bad)} constants differ from groth16.py's 3.1.0 ({', '.join(bad[:6])})")
        else:
            lines.append(f"{RISC0_SRC}: all {len(want)} constants (verifying key, control root, BN254 control id) "
                         "equal groth16.py's 3.1.0")
    else:
        lines.append(f"{RISC0_SRC} not installed (scripts/vault_libs.py install): compared with the selector only")
    fail_if(problems, "RISC Zero selectors")
    return lines


def _locked(text: str, name: str) -> list:
    return re.findall(r'^name = "' + re.escape(name) + r'"\nversion = "([^"]+)"', text, re.M)


def _row_text(text: str, rfp: str, row: str) -> str:
    """The reason cell of `row` in `rfp`'s section of docs/requirements-status.md (rows are repeated per RFP)."""
    start = text.find(f"## {rfp}:")
    if start < 0:
        raise CheckFailed(f"{STATUS}: no section '## {rfp}:'")
    m = re.search(r"^\| " + re.escape(row) + r" \| [^|]+ \| (.*?) \|", text[start:], re.M)
    if not m:
        raise CheckFailed(f"{STATUS}: no row {row} after '## {rfp}'")
    return m.group(1)


@check(id="r8-phase-split", family="hash", mode="offline", tier="quick", rows=("RFP-022 R.8",),
       evidence=(231,),
       title="RISC Zero 3.1 phase 1 complete in every Ethereum-side prover; what phase 2 leaves, as R.8's row says",
       what="risc0-zkvm, risc0-groth16 and risc0-circuit-recursion in the lockfiles of every workspace that makes a "
            "proof the vault takes, and risc0-zkvm in the LEZ-linked ones",
       against="3.1.0, 3.0.6 and 4.1.0 (docs/design/risc0-3.1-migration.md, phase 1); RFP-022 R.8's reason, which must "
               "name the version the LEZ-linked workspaces lock",
       inputs=PHASE1 + PHASE2 + (MIGRATION, STATUS))
def r8_phase_split(ctx):
    problems, lines = [], []
    for rel in PHASE1:
        text = ctx.path(rel).read_text()
        got = {name: _locked(text, name) for name in PHASE1_VERSIONS}
        off = {n: v for n, v in got.items() if v != [PHASE1_VERSIONS[n]]}
        if off:
            have = ", ".join(n + " " + (",".join(v) or "absent") for n, v in off.items())
            want = ", ".join(n + " " + v for n, v in PHASE1_VERSIONS.items())
            problems.append(f"{rel}: {have}; phase 1 wants {want}")
    if not problems:
        lines.append(f"phase 1: {len(PHASE1)} Ethereum-side lockfiles lock risc0-zkvm 3.1.0, risc0-groth16 3.0.6, "
                     "risc0-circuit-recursion 4.1.0")
    lez = {}
    for rel in PHASE2:
        v = _locked(ctx.path(rel).read_text(), "risc0-zkvm")
        lez[rel] = v[0] if len(v) == 1 else ",".join(v) or "absent"
    versions = sorted(set(lez.values()))
    lines.append("phase 2 (LEZ-linked, waits on our port to LEZ v0.4.0 on risc0 3.1): " +
                 "; ".join(f"{rel} risc0-zkvm {v}" for rel, v in lez.items()))
    reason = _row_text(ctx.path(STATUS).read_text(), "RFP-022", "R.8")
    # The row's LEZ sentence names the zkVM LEZ verifies private transactions with; the row also names 3.1.0 for the
    # Ethereum side, so only that phrase says which is LEZ's (review round 2: a row naming another version passed).
    said = re.search(r"LEZ verifies private transactions with risc0-zkvm (\d+\.\d+\.\d+)(.*?)(?:;|\.\s|\.$|$)", reason)
    # The rest of that sentence qualifies it: on 3.0.x it must say "until" and "phase 2" there, not just somewhere in
    # the row (final review round: "... 3.0.5, forever; phase 2 is complete" passed).
    qualifier = said.group(2) if said else ""
    if len(versions) != 1:
        problems.append(f"the LEZ-linked lockfiles disagree: {lez}")
    elif not said:
        problems.append("R.8's row does not say which risc0-zkvm LEZ verifies private transactions with")
    elif said.group(1) != versions[0]:
        problems.append(f"the LEZ-linked lockfiles lock risc0-zkvm {versions[0]}, but R.8's row says LEZ verifies "
                        f"private transactions with {said.group(1)}: the row is stale")
    elif versions[0].startswith("3.0.") and ("until" not in qualifier or "phase 2" not in qualifier):
        problems.append(f"R.8's row does not say that LEZ is on risc0-zkvm {versions[0]} until phase 2")
    else:
        lines.append(f"R.8's row says LEZ verifies private transactions with risc0-zkvm {versions[0]}, as the "
                     "LEZ-linked lockfiles lock" + (", until phase 2" if versions[0].startswith("3.0.") else ""))
    fail_if(problems, "RISC Zero 3.1 phase split")
    return lines
