#!/usr/bin/env python3
"""Reproduce the source inventory in docs/security/audit-scope.md.

Standard library only.  The allowlists below enumerate production source files;
`git ls-files`, not filesystem discovery, decides which of them exist and are
tracked.  Counts are physical lines.  A line containing code and a comment is
code; a line containing only comments is comment; lines inside block comments
are comments.  Rust nested block comments and quoted strings are handled.
"""

from __future__ import annotations

import collections
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# path prefix -> (short purpose, deployed artifact, externally reachable input)
GROUPS = {
    "crates/ferrata-core/src/": (
        "consensus, SSZ/RLP/MPT/execution proofs and stores",
        "linked into ferrata_lez / ferrata_bridge_b1",
        "library calls from the guests; no independent entry point",
    ),
    "lez/wire/src/": (
        "attestation actors, messages, state and attestation requests",
        "linked into ferrata_lez and ferrata_bridge_b1",
        "defines Message for ferrata_lez",
    ),
    "lez/bridge-wire/src/": (
        "bridge actors, messages, state and vault-proof bindings",
        "linked into ferrata_bridge_b1",
        "defines BridgeMessage for the bridge guest",
    ),
    "lez/guests/program/src/": (
        "LEZ light-client actor shell and inclusion attestations",
        "LEZ R0BF ferrata_lez (docs/image-ids.md:44)",
        "main dispatches Message",
    ),
    "lez/guests/bridge/src/": (
        "LEZ deposit, burn, balances, clocks, roles and fees",
        "LEZ R0BF ferrata_bridge_b1 (docs/image-ids.md:45)",
        "main dispatches BridgeMessage",
    ),
    "checkpoint/crates/checkpoint-core/src/": (
        "LEZ block parsing, checkpoint rules, segments and journals",
        "linked into checkpoint_31, checkpoint_b_31 and segment_31; "
        "the bedrock-0-3-5 feature links it into both *_0_3_5_31 guests",
        "library calls from checkpoint guests",
    ),
    "checkpoint/guests/checkpoint/src/": (
        "checkpoint rule A and composed segment-receipt verification",
        "RISC Zero checkpoint_31 (docs/image-ids.md:51)",
        "zkVM main reads CheckpointInput",
    ),
    "checkpoint/guests/checkpoint-b/src/": (
        "checkpoint rule B and composed segment-receipt verification",
        "RISC Zero checkpoint_b_31 (docs/image-ids.md:52)",
        "zkVM main reads CheckpointInputB",
    ),
    "checkpoint/guests/segment/src/": (
        "bounded LEZ block-chain segment used by either checkpoint rule",
        "RISC Zero segment_31 (docs/image-ids.md:54), verified by checkpoint guests",
        "zkVM main reads SegmentInput",
    ),
    "checkpoint/guests-0-3-5/checkpoint/src/": (
        "rule A for the Bedrock 0.3.5-rc.1 transaction layout",
        "RISC Zero checkpoint_0_3_5_31 (docs/image-ids.md:85)",
        "zkVM main reads CheckpointInput",
    ),
    "checkpoint/guests-0-3-5/checkpoint-b/src/": (
        "rule B for the Bedrock 0.3.5-rc.1 transaction layout",
        "RISC Zero checkpoint_b_0_3_5_31 (docs/image-ids.md:86)",
        "zkVM main reads CheckpointInputB",
    ),
    "bridge/crates/ferrata-bridge-core/src/": (
        "deposit/burn trees, commitments, nullifiers and release encoding",
        "linked into release_31 and ferrata_bridge_b1",
        "library calls from the guests; no independent entry point",
    ),
    "bridge/guests/src/bin/release.rs": (
        "burn membership verification and bound release journal",
        "RISC Zero release_31 (docs/image-ids.md:55), verified by FerrataVault",
        "zkVM main reads ReleaseInput",
    ),
    "vault/src/FerrataVault.sol": (
        "escrow, deposits, checkpoints, releases, roles and failsafes",
        "FerrataVault EVM bytecode",
        "constructor, receive, fallback and public/external functions",
    ),
    "vault/src/FerrataRegistry.sol": (
        "vault token, fee, cap, role and storage-proof library",
        "FerrataRegistry library; linked into FerrataVault",
        "publishListing external; other functions called by the vault",
    ),
    "vault/src/interfaces/": (
        "interfaces to the externally deployed verifier and emergency stop",
        "interface only; no independent deployed bytecode",
        "verify and paused interface declarations",
    ),
}

ATTESTATION_PREFIXES = (
    "crates/ferrata-core/src/",
    "lez/wire/src/",
    "lez/guests/program/src/",
)
BRIDGE_PREFIXES = tuple(GROUPS)


def tracked_files() -> set[str]:
    result = subprocess.run(
        ["git", "ls-files", "-z"], cwd=ROOT, check=True, stdout=subprocess.PIPE
    )
    return {p.decode() for p in result.stdout.split(b"\0") if p}


def selected(tracked: set[str], prefixes: tuple[str, ...]) -> list[str]:
    return sorted(
        p
        for p in tracked
        if p.endswith((".rs", ".sol"))
        and any(p == prefix or p.startswith(prefix) for prefix in prefixes)
    )


def group_for(path: str) -> str:
    matches = [prefix for prefix in GROUPS if path == prefix or path.startswith(prefix)]
    if len(matches) != 1:
        raise AssertionError(f"source has no unique scope group: {path}")
    return matches[0]


def language(path: str) -> str:
    return {"rs": "Rust", "sol": "Solidity"}[path.rsplit(".", 1)[1]]


def count_lines(path: str) -> tuple[int, int, int]:
    """Return (code, comment, blank), lexing // and nested /* */ comments."""
    text = (ROOT / path).read_text(encoding="utf-8")
    code = comment = blank = 0
    depth = 0
    for line in text.splitlines():
        i = 0
        has_code = has_comment = False
        quote = None
        while i < len(line):
            if depth:
                if line.startswith("/*", i):
                    depth += 1
                    i += 2
                elif line.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
                has_comment = True
                continue
            if quote:
                if line[i] == "\\":
                    i += 2
                elif line[i] == quote:
                    quote = None
                    i += 1
                else:
                    i += 1
                has_code = True
                continue
            if line.startswith("//", i):
                has_comment = True
                break
            if line.startswith("/*", i):
                depth = 1
                has_comment = True
                i += 2
                continue
            if line[i] in "\"'":
                quote = line[i]
                has_code = True
            elif not line[i].isspace():
                has_code = True
            i += 1
        if has_code:
            code += 1
        elif has_comment:
            comment += 1
        else:
            blank += 1
    if depth or quote:
        raise AssertionError(f"unterminated comment/string in {path}")
    return code, comment, blank


def markdown_table(paths: list[str]) -> str:
    rows = [
        "| Path | Lang | Code | Comments | Blank | Total | Purpose / artifact / entry |",
        "|---|---|---:|---:|---:|---:|---|",
    ]
    sums = [0, 0, 0]
    for path in paths:
        counts = count_lines(path)
        sums = [a + b for a, b in zip(sums, counts)]
        purpose, artifact, entry = GROUPS[group_for(path)]
        rows.append(
            f"| {path} | {language(path)} | {counts[0]} | {counts[1]} | "
            f"{counts[2]} | {sum(counts)} | {purpose}; {artifact}; {entry} |"
        )
    rows.append(
        f"| **TOTAL ({len(paths)} files)** | | **{sums[0]}** | **{sums[1]}** | "
        f"**{sums[2]}** | **{sum(sums)}** | |"
    )
    return "\n".join(rows)


def lock_packages(path: str, names: set[str]) -> dict[str, list[dict[str, str]]]:
    with (ROOT / path).open("rb") as handle:
        data = tomllib.load(handle)
    found: dict[str, list[dict[str, str]]] = collections.defaultdict(list)
    for package in data.get("package", []):
        if package.get("name") in names:
            found[package["name"]].append(package)
    missing = names - found.keys()
    if missing:
        raise AssertionError(f"{path} has no locked package for {sorted(missing)}")
    return found


def only_version(packages: dict[str, list[dict[str, str]]], name: str) -> str:
    versions = sorted({package["version"] for package in packages[name]})
    if len(versions) != 1:
        raise AssertionError(f"expected one version of {name}, got {versions}")
    return versions[0]


def git_tag(package: dict[str, str]) -> str:
    match = re.search(r"[?&]tag=([^#]+)", package.get("source", ""))
    if not match:
        raise AssertionError(f"locked git package has no tag: {package}")
    return match.group(1)


def dependency_lines() -> list[str]:
    root = lock_packages("Cargo.lock", {"blst"})
    lez = lock_packages("lez/guests/program/Cargo.lock", {"risc0-zkvm", "lee_core", "blst"})
    checkpoint = lock_packages("checkpoint/guests/Cargo.lock", {"risc0-zkvm", "risc0-groth16"})
    bridge = lock_packages("bridge/guests/Cargo.lock", {"risc0-zkvm", "risc0-groth16"})
    guest_blst = lez["blst"]
    if len(guest_blst) != 1:
        raise AssertionError(f"expected one guest blst package, got {guest_blst}")
    lib = (ROOT / "vault/lib.lock").read_text(encoding="utf-8").splitlines()
    pins = [line.split() for line in lib if line and not line.startswith("#")]
    pinmap = {parts[0]: parts[1] for parts in pins}
    rows = [
        ("blst (native)", only_version(root, "blst"), "Cargo.lock"),
        (
            "blst (attestation guest)",
            f"{only_version(lez, 'blst')}; RISC Zero fork tag {git_tag(guest_blst[0])}",
            "lez/guests/program/Cargo.lock",
        ),
        ("risc0-zkvm (LEZ guests)", only_version(lez, "risc0-zkvm"), "lez/guests/program/Cargo.lock"),
        ("risc0-zkvm (checkpoint guests)", only_version(checkpoint, "risc0-zkvm"), "checkpoint/guests/Cargo.lock"),
        ("risc0-zkvm (release guest)", only_version(bridge, "risc0-zkvm"), "bridge/guests/Cargo.lock"),
        ("risc0-groth16 (checkpoint guests)", only_version(checkpoint, "risc0-groth16"), "checkpoint/guests/Cargo.lock"),
        ("risc0-groth16 (release guest)", only_version(bridge, "risc0-groth16"), "bridge/guests/Cargo.lock"),
        ("RISC Zero verifier and emergency stop", pinmap["risc0-ethereum"], "vault/lib.lock:7"),
        ("OpenZeppelin contracts", pinmap["openzeppelin-contracts"], "vault/lib.lock:6"),
        (
            "LEZ",
            f"v0.4.0-rc1; lee_core {only_version(lez, 'lee_core')} by path",
            "lez/guests/program/Cargo.toml:14; lez/guests/program/Cargo.lock",
        ),
    ]
    return ["| Reused component | Pin | Record |", "|---|---|---|"] + [
        f"| {name} | {version} | {source} |" for name, version, source in rows
    ]


def render() -> str:
    tracked = tracked_files()
    attestation = selected(tracked, ATTESTATION_PREFIXES)
    bridge = selected(tracked, BRIDGE_PREFIXES)
    unknown = [p for p in bridge if group_for(p) not in GROUPS]
    if unknown:
        raise AssertionError(unknown)
    sections = [
        "RFP-021 AUDIT: attestation program",
        markdown_table(attestation),
        "",
        "RFP-022 AUDIT: contracts and bridge/checkpoint programs",
        markdown_table(bridge),
        "",
        "REUSED COMPONENTS (parsed from the named lockfiles)",
        *dependency_lines(),
    ]
    return "\n".join(sections) + "\n"


if __name__ == "__main__":
    sys.stdout.write(render())
