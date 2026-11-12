#!/usr/bin/env python3
"""Stage the crates each module links into the module's crate directory (ignored by git), where cargo
and the module builder find them: the builder copies only rust-lib/.

The crates keep their places relative to each other, so their own path dependencies hold unchanged:
the repository's under rust-lib/staged/<path in the repository>, LEZ's under
rust-lib/upstream/lez-v0.4.0-rc1/<path in LEZ> (the wire crates name LEZ as ../../../upstream/...).
Each staged manifest is written with what it inherits from its own workspace resolved (package fields,
workspace dependencies), and without its dev-dependencies and lints, which a module build does not use.
What was copied is recorded in rust-lib/staged/STAGED.json, which each module's version() reports.

    python3 -I modules/scripts/stage.py            both modules
    FERRATA_COMMIT=<sha> FERRATA_LEZ=<dir> ...     the commit to record; LEZ's clone (default
                                                   spike/upstream/lez-v0.4.0-rc1 beside the repository)
"""
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
LEZ = Path(os.environ.get("FERRATA_LEZ") or REPO.parent / "upstream" / "lez-v0.4.0-rc1").resolve()
LEZ_DIR = "upstream/lez-v0.4.0-rc1"

# Each module's crates: (root, path). The calls crate of each module depends on them by path.
MODULES = {
    "ferrata_attest_module": [
        ("repo", "crates/ferrata-core"),
        ("repo", "lez/wire"),
        ("repo", "lez/advance"),
        ("lez", "lee/state_machine/core"),
    ],
    "ferrata_bridge_module": [
        ("repo", "crates/ferrata-core"),
        ("repo", "bridge/crates/ferrata-bridge-core"),
        ("repo", "lez/wire"),
        ("repo", "lez/bridge-wire"),
        ("repo", "cli/wallet"),
        ("lez", "lee/state_machine/core"),
    ],
}
KEEP = ("package", "lib", "features", "dependencies", "build-dependencies", "target")
DEP_TABLES = ("dependencies", "build-dependencies")
BARE = re.compile(r"^[A-Za-z0-9_-]+$")


def workspace_root(crate: Path, top: Path) -> Path | None:
    """The nearest manifest with a [workspace] above `crate`, up to `top`."""
    d = crate.parent
    while True:
        m = d / "Cargo.toml"
        if m.is_file() and "workspace" in tomllib.loads(m.read_text()):
            return m
        if d == top or d == d.parent:
            return None
        d = d.parent


def resolve_dep(name, spec, ws):
    if not (isinstance(spec, dict) and spec.get("workspace") is True):
        return spec
    base = ws["workspace"]["dependencies"][name]
    out = dict(base) if isinstance(base, dict) else {"version": base}
    if "path" in out:
        sys.exit(f"stage.py: workspace dependency {name} has a path; not supported")
    for k, v in spec.items():
        if k == "workspace":
            continue
        if k == "features":
            out["features"] = list(dict.fromkeys(list(out.get("features", [])) + list(v)))
        else:
            out[k] = v
    return out


def resolve(manifest: dict, ws: dict | None) -> dict:
    out = {}
    for table in KEEP:
        if table not in manifest:
            continue
        t = manifest[table]
        if table == "package":
            pkg = {}
            for k, v in t.items():
                if isinstance(v, dict) and v.get("workspace") is True:
                    pkg[k] = ws["workspace"]["package"][k]
                elif k in ("workspace", "lints"):
                    continue
                else:
                    pkg[k] = v
            out["package"] = pkg
        elif table in DEP_TABLES:
            out[table] = {n: resolve_dep(n, s, ws) for n, s in t.items()}
        elif table == "target":
            out["target"] = {
                cfg: {dt: {n: resolve_dep(n, s, ws) for n, s in deps.items()} for dt, deps in body.items() if dt in DEP_TABLES}
                for cfg, body in t.items()
            }
        else:
            out[table] = t
    return out


def key(k: str) -> str:
    return k if BARE.match(k) else json.dumps(k)


def val(v) -> str:
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, (int, float)):
        return str(v)
    if isinstance(v, str):
        return json.dumps(v)
    if isinstance(v, list):
        return "[" + ", ".join(val(x) for x in v) + "]"
    if isinstance(v, dict):
        return "{ " + ", ".join(f"{key(k)} = {val(x)}" for k, x in v.items()) + " }"
    raise TypeError(type(v))


def write_toml(d: dict, source: str) -> str:
    lines = [f"# Staged by modules/scripts/stage.py from {source}: inherited fields resolved,", "# dev-dependencies and lints left out.", ""]
    for table, body in d.items():
        if table == "target":
            for cfg, deps in body.items():
                for dt, entries in deps.items():
                    lines.append(f"[target.{key(cfg)}.{dt}]")
                    lines += [f"{key(n)} = {val(s)}" for n, s in entries.items()]
                    lines.append("")
            continue
        lines.append(f"[{table}]")
        lines += [f"{key(k)} = {val(v)}" for k, v in body.items()]
        lines.append("")
    return "\n".join(lines)


def tree_sum(src: Path) -> str:
    files = sorted(p for p in [src / "Cargo.toml", *(src / "src").rglob("*")] if p.is_file())
    h = hashlib.sha256()
    for p in files:
        h.update(p.relative_to(src).as_posix().encode() + b"\0" + hashlib.sha256(p.read_bytes()).hexdigest().encode() + b"\n")
    return h.hexdigest()


def git_head(d: Path) -> str:
    try:
        return subprocess.run(["git", "-C", str(d), "rev-parse", "HEAD"], capture_output=True, text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def stage(module: str, crates, commit: str, lez_commit: str):
    lib = REPO / "modules" / module / "rust-lib"
    for d in ("staged", "upstream"):
        shutil.rmtree(lib / d, ignore_errors=True)
    record = {"commit": commit, "lez": {"dir": LEZ_DIR, "commit": lez_commit}, "crates": []}
    for root, path in crates:
        src = (REPO if root == "repo" else LEZ) / path
        dst = lib / ("staged" if root == "repo" else LEZ_DIR) / path
        manifest = tomllib.loads((src / "Cargo.toml").read_text())
        ws_path = workspace_root(src / "Cargo.toml", REPO if root == "repo" else LEZ)
        ws = tomllib.loads(ws_path.read_text()) if ws_path else None
        resolved = resolve(manifest, ws)
        dst.mkdir(parents=True)
        shutil.copytree(src / "src", dst / "src")
        if (src / "build.rs").is_file():
            shutil.copy2(src / "build.rs", dst / "build.rs")
        source = f"{path} of the repository" if root == "repo" else f"{path} of LEZ ({LEZ_DIR})"
        (dst / "Cargo.toml").write_text(write_toml(resolved, source))
        record["crates"].append({
            "name": resolved["package"]["name"],
            "version": resolved["package"].get("version", "0.0.0"),
            "from": f"{'' if root == 'repo' else LEZ_DIR + '/'}{path}",
            "source_sha256": tree_sum(src),
        })
    (lib / "staged").mkdir(exist_ok=True)
    (lib / "staged" / "STAGED.json").write_text(json.dumps(record, indent=1) + "\n")
    names = ", ".join(f"{c['name']} {c['version']}" for c in record["crates"])
    print(f"stage.py: {module}: {names}; commit {commit}")


def main():
    if not (LEZ / "lee" / "state_machine" / "core" / "Cargo.toml").is_file():
        sys.exit(f"stage.py: no LEZ clone at {LEZ} (FERRATA_LEZ)")
    commit = os.environ.get("FERRATA_COMMIT") or git_head(REPO)
    lez_commit = git_head(LEZ)
    for module, crates in MODULES.items():
        stage(module, crates, commit, lez_commit)


if __name__ == "__main__":
    main()
