"""The seal family, offline: every Groth16 seal the repository records, verified locally against the image id and
journal its records give (verifylib/groth16.py, RISC Zero's verifier in pure Python, releases 3.0.0 and 3.1.0).

The seals are found the way scripts/verify-mainnet-risc0.py finds them (its `Catalog`: crown runs, receipt
directories, the vault's real fixtures, the bridge's release guest runs), once for each release's selector.
"""
from __future__ import annotations

import importlib.util
import time
from pathlib import Path

from . import groth16
from .core import CheckFailed, check, fail_if

GHSA = ("RISC Zero's verifier 3.0.0 accepts forged claims (GHSA-mm6c-35cm-g4x2, 11 November 2026; "
        "docs/security/risc0-groth16-advisory-2026-11-21/): a 3.0.0 seal that verifies is well-formed for those "
        "circuits, not evidence of a sound proof")


def _catalog_module(root: Path):
    path = root / "scripts" / "verify-mainnet-risc0.py"
    spec = importlib.util.spec_from_file_location("verify_mainnet_risc0", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def recorded_seals(root: Path, release: str):
    """(entries, nojournal, unmapped) of the recorded seals of `release`: each entry a seal with the image id and the
    journal its records give (one entry per distinct pair)."""
    mod = _catalog_module(root)
    selector = bytes.fromhex(groth16.PARAMS[release]["selector"])
    entries, nojournal, unmapped, _found, _skipped, _notes = mod.Catalog(str(root), selector).build()
    return entries, nojournal, unmapped


LIVE_PROOFS = "e2e/runs/live-roundtrip-2026-11-24/proofs.json"  # EVIDENCE row 250: the proofs the vault verified


def _live_proofs(root: Path) -> list:
    """The test network's live round trip: the prover client's records of the checkpoint and release proofs (seal,
    image id, journal), which scripts/verify-live-roundtrip.py holds to the vault's transactions."""
    import hashlib
    import json
    f = root / LIVE_PROOFS
    if not f.exists():
        return []
    d = json.loads(f.read_text())
    out = []
    for name in ("checkpoint", "release"):
        x = d.get(name) or {}
        if x.get("seal") and x.get("image_id") and x.get("journal"):
            journal = bytes.fromhex(x["journal"].removeprefix("0x"))
            out.append({"seal": bytes.fromhex(x["seal"].removeprefix("0x")),
                        "image": bytes.fromhex(x["image_id"].removeprefix("0x")),
                        "digest": hashlib.sha256(journal).digest(), "label": f"{LIVE_PROOFS} {name}"})
    return out


def _seal_check(ctx, release: str):
    t0 = time.monotonic()
    entries, nojournal, unmapped = recorded_seals(ctx.root, release)
    if release == "3.1.0":
        live = _live_proofs(ctx.root)
        seen = {e["seal"] for e in entries}
        entries = entries + [e for e in live if e["seal"] not in seen]
        unmapped = {k: v for k, v in unmapped.items() if bytes.fromhex(k) not in {e["seal"] for e in live}}
    if not entries:
        raise CheckFailed(f"no recorded {release} seal with its image id and journal was found")
    problems, runs = [], {}
    for e in entries:
        ok, rel, why = groth16.verify_seal(e["seal"], e["image"], e["digest"])
        if not ok or rel != release:
            problems.append(f"{e['label']}: seal {e['seal'][:10].hex()}… does not verify for image "
                            f"{e['image'].hex()[:16]}… and its journal: {why or rel}")
        top = "/".join(e["label"].split("/")[:3])
        runs[top] = runs.get(top, 0) + 1
    fail_if(problems, f"the recorded {release} seals")
    lines = [f"{len(entries)} of {len(entries)} recorded (seal, image id, journal) verify locally with release "
             f"{release}'s verifying key and control root, in {time.monotonic() - t0:.0f} s",
             "from: " + ", ".join(f"{k} ({n})" for k, n in sorted(runs.items()))]
    if nojournal:
        lines.append(f"{len(nojournal)} more recorded without their journal (not verifiable here): "
                     + ", ".join(sorted({x['label'] for x in nojournal}))[:300])
    if unmapped:
        lines.append(f"{len(unmapped)} more found in files no record rule maps to an image id: "
                     + ", ".join(sorted({f for fs in unmapped.values() for f in fs}))[:300])
    return lines


@check(id="seals-v3.0", family="seal", mode="offline", evidence=(149, 41, 91, 141),
       rows=("RFP-022 F.7",),
       title="Every recorded RISC Zero 3.0.0 Groth16 seal verified locally (pure Python)",
       what="each Groth16 seal of release 3.0.0 the tree records (the crown runs' checkpoints and releases, the "
            "zone-bridge runs, the vault's real fixtures, the bridge's release runs) with its image id and journal",
       against="RiscZeroGroth16Verifier 3.0.0's own check, in verifylib/groth16.py: the claim digest, the control "
               "root and BN254 control id of risc0-ethereum v3.0.0, its verifying key, the pairing check. " + GHSA,
       inputs=("scripts/verify-mainnet-risc0.py", "e2e/runs", "vault/test/real", "bridge/artifacts"))
def seals_v30(ctx):
    return _seal_check(ctx, "3.0.0")


@check(id="seals-v3.1", family="seal", mode="offline", evidence=(231, 232, 250),
       rows=("RFP-022 R.8", "RFP-022 R.10"),
       title="Every recorded RISC Zero 3.1.0 Groth16 seal verified locally (pure Python)",
       what="each Groth16 seal of release 3.1.0 the tree records with its image id and journal: the vault's 3.1 "
            "fixtures and the test network's live round trip (its checkpoint and release proofs, e2e/runs/"
            "live-roundtrip-2026-11-24/proofs.json, the ones the vault verified on Sepolia)",
       against="RiscZeroGroth16Verifier 3.1.0's own check (risc0-ethereum v3.1.0, selector 0xe71e6620, the verifier "
               "our test network's vault pins), in verifylib/groth16.py",
       inputs=("scripts/verify-mainnet-risc0.py", LIVE_PROOFS, "vault/test/real"))
def seals_v31(ctx):
    return _seal_check(ctx, "3.1.0")
