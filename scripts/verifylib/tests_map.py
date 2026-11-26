"""The `test` checks: every test the row-to-test map (scripts/ci/row-tests.toml) names for a hard requirement row,
run as one command with its toolchain (tier `tests`). The map is the source; this module only derives, for each test
file, the narrowest command that runs it (the CI job named in the map runs it in CI)."""
from __future__ import annotations

import json
import re
import shlex
import tomllib
from pathlib import Path

from .core import Check, Skipped, register
from .rows import row_tests

SETUP = {
    "spectests": "the vectors first: scripts/fetch-vectors.sh",
    "lez": "LEZ's sources beside the repository and RISC Zero 3.0.5: scripts/fetch-upstream.sh, "
           "scripts/ci/install-risc0.sh (lez/RESULTS.md §8)",
    "forge": "the Foundry libraries at the pins of vault/README.md (forge install --no-git ...)",
    "cli": "LEZ's sources beside the repository (scripts/fetch-upstream.sh) and the native archives (scripts/fetch_native.py)",
}


def _package_of(root: Path, path: str):
    """(workspace dir, package name) of a Rust test file: the nearest Cargo.toml with [package] and the nearest one at
    or above it with [workspace] (or the package's own dir)."""
    p = (root / path).parent
    pkg_dir = None
    name = None
    while p != root.parent and p != p.parent:
        ct = p / "Cargo.toml"
        if ct.exists():
            data = tomllib.loads(ct.read_text())
            if pkg_dir is None and "package" in data:
                pkg_dir = p
                name = data["package"]["name"]
            if "workspace" in data:
                return p.relative_to(root).as_posix() or ".", name, pkg_dir
        p = p.parent
    return (pkg_dir.relative_to(root).as_posix() if pkg_dir else "."), name, pkg_dir


def _feature_gated(root: Path) -> dict:
    try:
        return json.loads((root / "scripts/ci/lez-harness-shards.json").read_text()).get("harness_feature_gated", {})
    except (OSError, ValueError):
        return {}


def derive(root: Path, t: dict):
    """(shell command, needs, setup note) for one test entry of the map."""
    path = t["path"]
    run = t.get("run")
    if run:
        needs = ["bash"]
        if "cargo" in run:
            needs.append("cargo")
        if "forge" in run:
            needs.append("forge")
        if re.search(r"\bnode\b", run):
            needs.append("node")
        lez_cargo = ("lez" in run.split("&&")[0] or "RISC0" in run) and "cargo" in run
        if lez_cargo:
            needs.append("r0vm")
        return run, tuple(needs), SETUP["lez"] if lez_cargo else ""
    ext = path.rsplit(".", 1)[-1] if "." in path.rsplit("/", 1)[-1] else ""
    if path.startswith(".github/workflows/"):
        return None, (), ""
    if ext == "py":
        return f"python3 -I {shlex.quote(path)}", ("python3",), ""
    if ext == "mjs":
        if path == "admin/dump.mjs":
            return "node admin/dump.mjs --selftest", ("node",), ""
        return f"node --test {shlex.quote(path)}", ("node",), ""
    if ext == "sh":
        return f"bash {shlex.quote(path)}", ("bash",), ""
    if ext == "sol":
        proj = "vault" if path.startswith("vault/") else path.split("/test/")[0]
        rel = path[len(proj) + 1:]
        return (f"cd {shlex.quote(proj)} && forge test --match-path {shlex.quote(rel)}", ("forge",), SETUP["forge"])
    if ext == "rs":
        ws, pkg, _ = _package_of(root, path)
        stem = Path(path).stem
        is_test_file = "/tests/" in path
        target = f"--test {stem}" if is_test_file else "--lib"
        feats = ""
        if pkg == "harness" and stem in _feature_gated(root):
            feats = f" --features {_feature_gated(root)[stem]}"
        env = ""
        needs = ["cargo"]
        note = ""
        if ws == "lez" or ws.startswith("lez/"):
            env = "RISC0_DEV_MODE=1 "
            needs.append("r0vm")
            note = SETUP["lez"]
        elif ws == "cli":
            note = SETUP["cli"]
        elif path.startswith("crates/ferrata-spectests"):
            note = SETUP["spectests"]
        toolchain = "+1.88 " if ws == "checkpoint" else ""
        cmd = f"{env}cargo {toolchain}test --locked -p {pkg}{feats} {target}"
        return (f"cd {shlex.quote(ws)} && {cmd}" if ws != "." else cmd), tuple(needs), note
    return None, (), ""


MANUAL_CI = {"box", "local", "-"}  # the map's places that are not a CI job
_PLACEHOLDER = re.compile(r"(?<![\w$/.-])(?:[A-Z][A-Z0-9]*_)*(?:URL|DIR|LOGOSCTL|FILE|PATH|NAME|OUT|HOST)(?![\w=])|\.\.\.|…")
_SENDS = re.compile(r"--send\b|SEND=yes")


def manual_reason(shell: str, ci: set):
    """Why a named test is a recipe to run by hand rather than a command verify.py runs, or None: its command still
    holds a placeholder, or it can send transactions. (A test no CI job runs, only a build machine or a person, stays
    runnable: its title says so.)"""
    why = []
    if _SENDS.search(shell):
        why.append("it can SEND transactions (to the endpoint you give it)")
    if _PLACEHOLDER.search(shell):
        why.append("its command has placeholders to fill in")
    return "; ".join(why) or None


def register_test_checks(root: Path) -> int:
    grouped: dict = {}
    for entry in row_tests(root):
        for t in entry.get("tests", []):
            if t.get("kind") != "test" or t.get("branch"):
                continue
            key = (t["path"], t.get("run", ""))
            g = grouped.setdefault(key, {"t": t, "rows": [], "ci": set()})
            g["rows"].append(entry["id"])
            g["ci"].add(t.get("ci", "-"))
    by_path: dict = {}
    for (path, run) in grouped:
        by_path.setdefault(path, []).append(run)
    n = 0
    for (path, run), g in sorted(grouped.items()):
        shell, needs, note = derive(root, g["t"])
        cid = "test:" + path
        if len(by_path[path]) > 1:
            cid += "#" + str(sorted(by_path[path]).index(run) + 1)
        ci = ", ".join(sorted(g["ci"]))
        where = f"not in CI: {ci}" if g["ci"] <= MANUAL_CI else f"CI job: {ci}"
        common = dict(id=cid, title=f"{path} ({where})", family="test", mode="offline", tier="tests",
                      rows=tuple(dict.fromkeys(g["rows"])), what=f"the test {path}",
                      against="the behaviour its requirement rows name (scripts/ci/row-tests.toml, its note)",
                      source="scripts/ci/row-tests.toml" + (f"; setup: {note}" if note else ""), inputs=(path,))
        manual = manual_reason(shell, g["ci"]) if shell else None
        if shell is None:
            def fn(ctx, _p=path):
                raise Skipped(f"{_p} is a CI workflow: its evidence is its runs on GitHub, not a local command")
            register(Check(fn=fn, **common))
        elif manual:
            def fn(ctx, _s=shell, _why=manual, _note=note):
                raise Skipped(f"a recipe to run by hand, not run here: {_why}. The recipe: {_s}"
                              + (f" (setup: {_note})" if _note else ""))
            register(Check(fn=fn, **{**common, "title": common["title"] + " [by hand]"}))
        else:
            register(Check(shell=shell, needs=needs, timeout=4 * 3600, **common))
        n += 1
    return n
