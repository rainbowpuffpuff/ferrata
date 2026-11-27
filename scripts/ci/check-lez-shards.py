#!/usr/bin/env python3
"""Guard and command planner for the sharded `lez-harness` CI job (standard library only).

Checks that `scripts/ci/lez-harness-shards.json` covers every Cargo test target that the unsharded
`cargo test -p` commands run on `lez/harness` and `lez/updater` exactly once — library unit tests
(`--lib`), binary unit tests (`--bins`), doc-tests (`--doc`), and integration-test binaries
(`--test`) — with no missing target, no duplicate target, no empty shard, no nonexistent entry,
honest feature-gated exclusions, and exact/skip split partitions whose test names exist in the
source files.

Targets are derived from each package's `Cargo.toml` (`[lib]`, `[[bin]]`, `[[test]]`, `autobins`,
`autotests`) and its `src/` and `tests/` trees (`tests/*.rs` and `tests/*/main.rs`; helper
subdirectories without `main.rs`, such as `tests/redteam_common/` and `tests/security_common/`,
are modules, not binaries).

Usage:
    python3 -I scripts/ci/check-lez-shards.py
    python3 -I scripts/ci/check-lez-shards.py --dry-run [SHARD] [--build-only] [--threads N] [--privacy-out DIR]
    python3 -I scripts/ci/check-lez-shards.py --emit-tsv SHARD [--build-only] [--threads N] [--privacy-out DIR]
    python3 -I scripts/ci/check-lez-shards.py --list-shards
"""

from __future__ import annotations

import json
import pathlib
import re
import shlex
import sys
import tomllib

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import ci_model  # noqa: E402

ROOT = ci_model.ROOT
DEFAULT_MAP = HERE / "lez-harness-shards.json"
DEFAULT_HARNESS_DIR = ROOT / "lez" / "harness"
DEFAULT_UPDATER_DIR = ROOT / "lez" / "updater"
DEFAULT_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
SHARD_NAME_RE = re.compile(r"^[a-z0-9][a-z0-9_-]*$")
PRIVACY_BINARY = "privacy_suite"


def _strip_rust_comments_and_strings(text: str) -> str:
    """Blank out comments and string literals while keeping newlines, so brace counting stays accurate."""
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        # Line comment
        if c == "/" and i + 1 < n and text[i + 1] == "/":
            while i < n and text[i] != "\n":
                out.append(" ")
                i += 1
            continue
        # Block comment (Rust allows nesting)
        if c == "/" and i + 1 < n and text[i + 1] == "*":
            depth = 1
            out.extend("  ")
            i += 2
            while i < n and depth:
                if text[i : i + 2] == "/*":
                    depth += 1
                    out.extend("  ")
                    i += 2
                elif text[i : i + 2] == "*/":
                    depth -= 1
                    out.extend("  ")
                    i += 2
                else:
                    out.append("\n" if text[i] == "\n" else " ")
                    i += 1
            continue
        # Raw string r#"..."# or br#"..."#
        m_raw = re.match(r'b?r(#*)"', text[i:])
        if m_raw and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            hashes = m_raw.group(1)
            end_tok = '"' + hashes
            out.extend(" " * len(m_raw.group(0)))
            i += len(m_raw.group(0))
            end = text.find(end_tok, i)
            if end == -1:
                end = n
            for ch in text[i:end]:
                out.append("\n" if ch == "\n" else " ")
            out.extend(" " * min(len(end_tok), n - end))
            i = min(n, end + len(end_tok))
            continue
        # Normal string "..."
        if c == '"':
            out.append(" ")
            i += 1
            while i < n and text[i] != '"':
                if text[i] == "\\" and i + 1 < n:
                    out.append("\n" if text[i] == "\n" else " ")
                    out.append("\n" if text[i + 1] == "\n" else " ")
                    i += 2
                    continue
                out.append("\n" if text[i] == "\n" else " ")
                i += 1
            if i < n:
                out.append(" ")
                i += 1
            continue
        # Char literal 'a' or '\x7f' (distinguish from lifetimes like 'static)
        if c == "'" and i + 2 < n:
            if text[i + 1] == "\\":
                k = i + 2
                while k < n and text[k] != "'" and text[k] != "\n":
                    k += 1
                if k < n and text[k] == "'":
                    out.extend(" " * (k - i + 1))
                    i = k + 1
                    continue
            elif text[i + 2] == "'" and text[i + 1] != "\n":
                out.extend("   ")
                i += 3
                continue
        out.append(c)
        i += 1
    return "".join(out)


def extract_ungated_test_names(text: str, enabled_features: set[str]) -> set[str]:
    """Return full libtest names of `#[test]` functions active when `enabled_features` are enabled."""
    file_gate = re.search(r'^\s*#!\[cfg\(feature\s*=\s*"([^"]+)"\)\]', text, re.M)
    if file_gate and file_gate.group(1) not in enabled_features:
        return set()

    clean = _strip_rust_comments_and_strings(text)
    raw_lines = text.splitlines()

    # Map line index -> (module prefix stack, gated_features set) using inline `mod name { ... }`.
    # First find every inline `mod <name> {` and its matching closing brace line.
    mod_spans: list[tuple[int, int, str, set[str]]] = []
    for m in re.finditer(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{", clean, re.M):
        start_line = clean.count("\n", 0, m.start())
        # Collect contiguous attributes immediately above `mod`
        attrs = []
        j = start_line - 1
        while j >= 0 and re.match(r"^\s*(?:#\[|///|//|$)", raw_lines[j]):
            if raw_lines[j].strip().startswith("#["):
                attrs.append(raw_lines[j])
            elif raw_lines[j].strip() == "":
                break
            j -= 1
        feats = {f for a in attrs for f in re.findall(r'#\[cfg\(feature\s*=\s*"([^"]+)"\)\]', a)}
        # Walk braces from opening '{'
        depth = 0
        k = m.end() - 1
        while k < len(clean):
            if clean[k] == "{":
                depth += 1
            elif clean[k] == "}":
                depth -= 1
                if depth == 0:
                    break
            k += 1
        end_line = clean.count("\n", 0, k)
        mod_spans.append((start_line, end_line, m.group(1), feats))

    names: set[str] = set()
    for i, line in enumerate(raw_lines):
        if not re.match(r"^\s*#\[(?:[a-z_]+::)?test\b", line):
            continue
        # Enclosing modules (outer to inner)
        enclosing = sorted(
            [s for s in mod_spans if s[0] < i <= s[1]],
            key=lambda s: (s[0], -s[1]),
        )
        off_feats: set[str] = set()
        for _, _, _, feats in enclosing:
            off_feats |= feats - enabled_features
        # Attributes adjacent to #[test] (above and below, up to fn)
        attrs = [line]
        j = i - 1
        while j >= 0 and re.match(r"^\s*(?:#\[|///|//)", raw_lines[j]):
            attrs.append(raw_lines[j])
            j -= 1
        j = i + 1
        fn_name = None
        while j < len(raw_lines):
            if re.match(r"^\s*(?:#\[|///|//|\s*$)", raw_lines[j]):
                attrs.append(raw_lines[j])
                j += 1
                continue
            m_fn = re.match(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\b", raw_lines[j])
            if m_fn:
                fn_name = m_fn.group(1)
            break
        for a in attrs:
            off_feats |= set(re.findall(r'#\[cfg\(feature\s*=\s*"([^"]+)"\)\]', a)) - enabled_features
        if off_feats or not fn_name:
            continue
        prefix = "::".join(s[2] for s in enclosing)
        names.add(f"{prefix}::{fn_name}" if prefix else fn_name)
    return names


def discover_test_binaries(pkg_dir: pathlib.Path, enabled_features: set[str]) -> dict[str, dict]:
    """Derive Cargo integration test binaries from Cargo.toml (`[[test]]`, `autotests`) and `tests/`."""
    manifest_path = pkg_dir / "Cargo.toml"
    with open(manifest_path, "rb") as handle:
        manifest = tomllib.load(handle)
    pkg = manifest.get("package") or {}
    defaults = set((manifest.get("features") or {}).get("default") or [])
    active_features = set(enabled_features) | defaults

    binaries: dict[str, dict] = {}
    claimed_paths: set[str] = set()

    for entry in manifest.get("test") or []:
        name = entry.get("name")
        if not name:
            continue
        rel = entry.get("path")
        if not rel:
            cand_file = f"tests/{name}.rs"
            cand_dir = f"tests/{name}/main.rs"
            rel = cand_file if (pkg_dir / cand_file).is_file() else cand_dir
        rel = pathlib.PurePosixPath(rel).as_posix()
        req = list(entry.get("required-features") or [])
        binaries[name] = {"name": name, "rel_path": rel, "required_features": req, "explicit": True}
        claimed_paths.add(rel)

    if pkg.get("autotests", True) is not False:
        tests_dir = pkg_dir / "tests"
        if tests_dir.is_dir():
            for path in sorted(tests_dir.iterdir()):
                if path.is_file() and path.suffix == ".rs":
                    rel = f"tests/{path.name}"
                    name = path.stem
                    if rel in claimed_paths or name in binaries:
                        continue
                    binaries[name] = {"name": name, "rel_path": rel, "required_features": [], "explicit": False}
                    claimed_paths.add(rel)
                elif path.is_dir():
                    main_rs = path / "main.rs"
                    if main_rs.is_file():
                        rel = f"tests/{path.name}/main.rs"
                        name = path.name
                        if rel in claimed_paths or name in binaries:
                            continue
                        binaries[name] = {"name": name, "rel_path": rel, "required_features": [], "explicit": False}
                        claimed_paths.add(rel)

    for info in binaries.values():
        src_path = pkg_dir / info["rel_path"]
        text = src_path.read_text(encoding="utf-8") if src_path.is_file() else ""
        missing_req = [f for f in info["required_features"] if f not in active_features]
        file_gate, gated_feats, ungated_count = ci_model.feature_gates(text, active_features)
        gate_features = set(missing_req)
        if file_gate:
            gate_features.add(file_gate)
        elif gated_feats and ungated_count == 0 and "proptest!" not in text:
            gate_features |= set(gated_feats)
        info["exists"] = src_path.is_file()
        info["gate_features"] = sorted(gate_features)
        info["ungated_tests"] = extract_ungated_test_names(text, active_features) if not missing_req else set()

    return binaries


def discover_package_targets(pkg_dir: pathlib.Path, enabled_features: set[str]) -> dict:
    """Inspect Cargo.toml and src/ to discover lib, bin, and doc-test targets and whether each holds tests."""
    with open(pkg_dir / "Cargo.toml", "rb") as handle:
        manifest = tomllib.load(handle)
    pkg = manifest.get("package") or {}
    pkg_name = pkg.get("name") or ""
    defaults = set((manifest.get("features") or {}).get("default") or [])
    active_features = set(enabled_features) | defaults

    lib_cfg = manifest.get("lib") or {}
    lib_rel = pathlib.PurePosixPath(lib_cfg.get("path") or "src/lib.rs").as_posix()
    has_lib_file = (pkg_dir / lib_rel).is_file()
    has_lib = has_lib_file and lib_cfg.get("test", True) is not False
    has_doc = has_lib_file and lib_cfg.get("doctest", True) is not False

    # Discover bin targets from [[bin]] and package.autobins (default true).
    bin_paths: set[str] = set()
    active_bin_targets: list[str] = []
    for entry in manifest.get("bin") or []:
        bname = entry.get("name") or ""
        rel = entry.get("path")
        if rel:
            rel = pathlib.PurePosixPath(rel).as_posix()
        elif bname == pkg_name and (pkg_dir / "src" / "main.rs").is_file():
            rel = "src/main.rs"
        else:
            cand_file = f"src/bin/{bname}.rs"
            cand_dir = f"src/bin/{bname}/main.rs"
            rel = cand_file if (pkg_dir / cand_file).is_file() else cand_dir
        bin_paths.add(rel)
        req = set(entry.get("required-features") or [])
        if req <= active_features and entry.get("test", True) is not False and (pkg_dir / rel).is_file():
            active_bin_targets.append(rel)

    if pkg.get("autobins", True) is not False:
        if (pkg_dir / "src" / "main.rs").is_file() and "src/main.rs" not in bin_paths:
            bin_paths.add("src/main.rs")
            active_bin_targets.append("src/main.rs")
        bin_dir = pkg_dir / "src" / "bin"
        if bin_dir.is_dir():
            for path in sorted(bin_dir.iterdir()):
                if path.is_file() and path.suffix == ".rs":
                    rel = f"src/bin/{path.name}"
                    if rel not in bin_paths:
                        bin_paths.add(rel)
                        active_bin_targets.append(rel)
                elif path.is_dir() and (path / "main.rs").is_file():
                    rel = f"src/bin/{path.name}/main.rs"
                    if rel not in bin_paths:
                        bin_paths.add(rel)
                        active_bin_targets.append(rel)

    lib_has_unit_tests = False
    bins_have_unit_tests = False
    lib_has_doctests = False
    src_dir = pkg_dir / "src"
    if src_dir.is_dir():
        for path in sorted(src_dir.rglob("*.rs")):
            rel = path.relative_to(pkg_dir).as_posix()
            text = path.read_text(encoding="utf-8", errors="replace")
            is_bin_file = (
                rel in bin_paths
                or rel == "src/main.rs"
                or rel.startswith("src/bin/")
                or not has_lib_file
            )
            if is_bin_file:
                if extract_ungated_test_names(text, active_features):
                    bins_have_unit_tests = True
            else:
                if extract_ungated_test_names(text, active_features):
                    lib_has_unit_tests = True
                file_gate = re.search(r'^\s*#!\[cfg\(feature\s*=\s*"([^"]+)"\)\]', text, re.M)
                if (not file_gate or file_gate.group(1) in active_features) and ci_model.has_doctest(text):
                    lib_has_doctests = True

    return {
        "has_lib": has_lib,
        "has_doc": has_doc,
        "has_bins": bool(active_bin_targets),
        "bin_count": len(active_bin_targets),
        "lib_has_unit_tests": lib_has_unit_tests,
        "bins_have_unit_tests": bins_have_unit_tests,
        "lib_has_doctests": lib_has_doctests,
    }


def _check_package_coverage(
    pkg_label: str,
    binaries: dict[str, dict],
    shards: dict[str, dict],
    feature_gated_map: dict,
    tests_key: str,
    splits_key: str,
    lib_key: str,
    bins_key: str,
    doc_key: str,
    targets: dict,
    implicit_owned: dict[str, str],
    problems: list[str],
) -> None:
    if not isinstance(feature_gated_map, dict):
        problems.append(f"{pkg_label}: feature-gated table must be an object")
        feature_gated_map = {}

    seen_whole: dict[str, list[str]] = {}
    splits_by_bin: dict[str, list[tuple[str, dict]]] = {}
    lib_shards: list[str] = []
    bins_shards: list[str] = []
    doc_shards: list[str] = []

    for bname, shard_owner in implicit_owned.items():
        if bname not in binaries:
            problems.append(f"{pkg_label}: implicit test binary {bname!r} (owned by {shard_owner}) does not exist")
        else:
            seen_whole.setdefault(bname, []).append(f"{shard_owner} (privacy)")

    for sname, spec in shards.items():
        if not isinstance(spec, dict):
            continue
        if spec.get(lib_key):
            lib_shards.append(sname)
        if spec.get(bins_key):
            bins_shards.append(sname)
        if spec.get(doc_key):
            doc_shards.append(sname)
        tlist = spec.get(tests_key, [])
        if not isinstance(tlist, list):
            problems.append(f"shard {sname}: {tests_key} must be a list")
            tlist = []
        for bname in tlist:
            if not isinstance(bname, str) or bname not in binaries:
                problems.append(f"shard {sname}: {pkg_label} binary {bname!r} does not exist")
                continue
            seen_whole.setdefault(bname, []).append(sname)

        slist = spec.get(splits_key, [])
        if not isinstance(slist, list):
            problems.append(f"shard {sname}: {splits_key} must be a list")
            slist = []
        for entry in slist:
            if not isinstance(entry, dict):
                problems.append(f"shard {sname}: invalid {splits_key} entry {entry!r}")
                continue
            bname = entry.get("test")
            if not isinstance(bname, str) or bname not in binaries:
                problems.append(f"shard {sname}: split {pkg_label} binary {bname!r} does not exist")
                continue
            splits_by_bin.setdefault(bname, []).append((sname, entry))

    # Validate feature_gated entries
    for bname, feat in feature_gated_map.items():
        if bname not in binaries:
            problems.append(f"{pkg_label}: feature-gated entry {bname!r} does not exist")
            continue
        if bname in seen_whole or bname in splits_by_bin:
            problems.append(f"{pkg_label}: binary {bname!r} is listed as both feature-gated and run by a shard")
        info = binaries[bname]
        if not isinstance(feat, str) or feat not in info["gate_features"]:
            problems.append(
                f"{pkg_label}: binary {bname!r} is listed as feature-gated on {feat!r}, "
                f"but its source/Cargo.toml gate features are {info['gate_features']}"
            )

    # Check duplicates and whole+split overlap
    for bname, owners in sorted(seen_whole.items()):
        if len(owners) > 1:
            problems.append(f"{pkg_label}: binary {bname!r} is run more than once (in {', '.join(owners)})")
        if bname in splits_by_bin:
            split_owners = [s for s, _ in splits_by_bin[bname]]
            problems.append(
                f"{pkg_label}: binary {bname!r} is run both whole (in {', '.join(owners)}) "
                f"and split (in {', '.join(split_owners)})"
            )

    # Validate split partitions
    for bname, entries in sorted(splits_by_bin.items()):
        info = binaries[bname]
        available = info["ungated_tests"]
        exact_sets: list[tuple[str, set[str]]] = []
        skip_sets: list[tuple[str, set[str]]] = []
        for sname, entry in entries:
            mode = entry.get("mode")
            names = entry.get("names")
            if mode not in ("exact", "skip"):
                problems.append(f"shard {sname}: split of {pkg_label} {bname!r} has invalid mode {mode!r}")
                continue
            if not isinstance(names, list) or not names or not all(isinstance(n, str) and n for n in names):
                problems.append(f"shard {sname}: split of {pkg_label} {bname!r} must have a non-empty list of test names")
                continue
            if len(names) != len(set(names)):
                dups = sorted({n for n in names if names.count(n) > 1})
                problems.append(f"shard {sname}: split of {pkg_label} {bname!r} lists duplicate test names: {dups}")
            missing_names = sorted(set(names) - available)
            if missing_names:
                problems.append(
                    f"shard {sname}: split of {pkg_label} {bname!r} names nonexistent test(s): {missing_names}"
                )
            if mode == "exact":
                exact_sets.append((sname, set(names)))
            else:
                skip_sets.append((sname, set(names)))

        # Pairwise disjointness among exact shards
        exact_union: set[str] = set()
        for sname, nset in exact_sets:
            overlap = sorted(exact_union & nset)
            if overlap:
                problems.append(f"{pkg_label}: split of {bname!r} runs test(s) twice across exact shards: {overlap}")
            exact_union |= nset

        if skip_sets:
            if len(skip_sets) > 1:
                problems.append(
                    f"{pkg_label}: split of {bname!r} has multiple skip shards ({[s for s, _ in skip_sets]}); "
                    f"tests outside the skip sets would run more than once"
                )
            if not exact_sets:
                problems.append(
                    f"{pkg_label}: split of {bname!r} has a skip shard without matching exact shard(s); "
                    f"skipped tests would never run"
                )
            else:
                skip_shard, skip_names = skip_sets[0]
                if skip_names != exact_union:
                    only_skip = sorted(skip_names - exact_union)
                    only_exact = sorted(exact_union - skip_names)
                    problems.append(
                        f"{pkg_label}: split of {bname!r} exact/skip mismatch (shard {skip_shard}): "
                        f"skipped-not-exact={only_skip}, exact-not-skipped={only_exact}"
                    )
                if available and available <= skip_names:
                    problems.append(
                        f"{pkg_label}: split of {bname!r} skips every test in shard {skip_shard}"
                    )
        else:
            missing_tests = sorted(available - exact_union)
            if missing_tests:
                problems.append(
                    f"{pkg_label}: exact-only split of {bname!r} leaves test(s) unrun: {missing_tests}"
                )

    # Check completeness: every discovered binary must exist on disk and be covered
    covered = set(seen_whole) | set(splits_by_bin) | set(feature_gated_map)
    for bname, info in sorted(binaries.items()):
        if not info["exists"]:
            problems.append(f"{pkg_label}: [[test]] {bname!r} points to missing file {info['rel_path']}")
        if bname not in covered:
            problems.append(
                f"{pkg_label}: test binary {bname!r} ({info['rel_path']}) is not run by any shard"
            )

    # Check lib, bin, and doc-test targets (every target `cargo test -p` runs must be covered by exactly one shard).
    for key, owners, exists, has_tests, label in (
        (lib_key, lib_shards, targets["has_lib"], targets["lib_has_unit_tests"], "lib target"),
        (bins_key, bins_shards, targets["has_bins"], targets["bins_have_unit_tests"], "bin target(s)"),
        (doc_key, doc_shards, targets["has_doc"], targets["lib_has_doctests"], "doc-tests"),
    ):
        if len(owners) > 1:
            problems.append(f"{pkg_label}: {key} is enabled in more than one shard ({', '.join(owners)})")
        if exists and len(owners) == 0:
            detail = " (and has tests)" if has_tests else ""
            problems.append(f"{pkg_label}: has {label}{detail}, but no shard enables {key}")
        elif not exists and len(owners) > 0:
            problems.append(f"{pkg_label}: has no {label}, but {key} is enabled in {', '.join(owners)}")


def check_shards(
    map_path: pathlib.Path = DEFAULT_MAP,
    harness_dir: pathlib.Path = DEFAULT_HARNESS_DIR,
    updater_dir: pathlib.Path = DEFAULT_UPDATER_DIR,
    workflow_path: pathlib.Path | None = DEFAULT_WORKFLOW,
    require_workflow_matrix: bool = True,
) -> tuple[list[str], dict]:
    problems: list[str] = []
    data = json.loads(map_path.read_text(encoding="utf-8"))
    shards = data.get("shards")
    if not isinstance(shards, dict) or not shards:
        return ["shard map has no `shards` object"], {}

    bool_keys = {"harness_lib", "harness_bins", "harness_doc", "updater_lib", "updater_bins", "updater_doc", "updater_demo", "privacy"}
    list_keys = {"harness_tests", "harness_splits", "updater_tests", "updater_splits"}
    allowed_shard_keys = bool_keys | list_keys
    privacy_shards = []
    updater_demo_shards = []
    for sname, spec in shards.items():
        if not SHARD_NAME_RE.match(sname):
            problems.append(f"invalid shard name {sname!r}")
        if not isinstance(spec, dict):
            problems.append(f"shard {sname!r} must be an object")
            continue
        unknown = sorted(set(spec) - allowed_shard_keys)
        if unknown:
            problems.append(f"shard {sname!r} has unknown key(s): {unknown}")
        for bk in bool_keys:
            if bk in spec and not isinstance(spec[bk], bool):
                problems.append(f"shard {sname!r}: {bk} must be a boolean")
        has_work = any(spec.get(bk) is True for bk in bool_keys) or any(
            isinstance(spec.get(lk), list) and len(spec.get(lk)) > 0 for lk in list_keys
        )
        if not has_work:
            problems.append(f"shard {sname!r} runs nothing (empty shard)")
        if spec.get("privacy") is True:
            privacy_shards.append(sname)
        if spec.get("updater_demo") is True:
            updater_demo_shards.append(sname)

    if len(privacy_shards) != 1:
        problems.append(f"expected exactly 1 shard with `privacy: true`, found {len(privacy_shards)} ({privacy_shards})")
    if len(updater_demo_shards) != 1:
        problems.append(
            f"expected exactly 1 shard with `updater_demo: true`, found {len(updater_demo_shards)} ({updater_demo_shards})"
        )

    harness_bins = discover_test_binaries(harness_dir, set())
    updater_bins = discover_test_binaries(updater_dir, {"harness"})
    harness_targets = discover_package_targets(harness_dir, set())
    updater_targets = discover_package_targets(updater_dir, {"harness"})

    implicit_harness = {PRIVACY_BINARY: privacy_shards[0]} if len(privacy_shards) == 1 else {}
    _check_package_coverage(
        "lez/harness",
        harness_bins,
        shards,
        data.get("harness_feature_gated", {}),
        "harness_tests",
        "harness_splits",
        "harness_lib",
        "harness_bins",
        "harness_doc",
        harness_targets,
        implicit_harness,
        problems,
    )
    _check_package_coverage(
        "lez/updater",
        updater_bins,
        shards,
        data.get("updater_feature_gated", {}),
        "updater_tests",
        "updater_splits",
        "updater_lib",
        "updater_bins",
        "updater_doc",
        updater_targets,
        {},
        problems,
    )

    if workflow_path is not None and workflow_path.is_file():
        doc = ci_model.load_yaml(workflow_path.read_text(encoding="utf-8")) or {}
        job = (doc.get("jobs") or {}).get("lez-harness") or {}
        matrix = ((job.get("strategy") or {}).get("matrix") or {}).get("shard")
        if matrix is not None:
            if not isinstance(matrix, list) or not all(isinstance(x, str) for x in matrix):
                problems.append(f"{workflow_path.name}: lez-harness strategy.matrix.shard must be a list of strings")
            else:
                if len(matrix) != len(set(matrix)):
                    dups = sorted({x for x in matrix if matrix.count(x) > 1})
                    problems.append(f"{workflow_path.name}: duplicate shard(s) in lez-harness matrix: {dups}")
                map_names = set(shards.keys())
                wf_names = set(matrix)
                if wf_names != map_names:
                    problems.append(
                        f"{workflow_path.name}: lez-harness matrix shards {sorted(wf_names)} "
                        f"differ from shard map {sorted(map_names)}"
                    )
        elif require_workflow_matrix:
            problems.append(f"{workflow_path.name}: job lez-harness has no strategy.matrix.shard")

    summary = {
        "shards": len(shards),
        "harness_total": len(harness_bins),
        "harness_feature_gated": len(data.get("harness_feature_gated", {})),
        "harness_bins": harness_targets["bin_count"],
        "updater_total": len(updater_bins),
        "updater_bins": updater_targets["bin_count"],
    }
    return problems, summary


def build_shard_plan(
    data: dict,
    shard_name: str,
    threads: str = "4",
    build_only: bool = False,
    privacy_out: str = "$PRIVACY_OUT",
    repo_prefix: str = "",
) -> list[dict]:
    """Return ordered commands `[{"cwd", "dev_mode", "argv"}]` for one shard."""
    shards = data.get("shards") or {}
    if shard_name not in shards:
        raise KeyError(f"unknown shard {shard_name!r} (valid: {', '.join(shards)})")
    spec = shards[shard_name]
    if not isinstance(spec, dict):
        raise ValueError(f"shard {shard_name!r} must be an object")
    cmds: list[dict] = []

    h_lib = bool(spec.get("harness_lib"))
    h_bins = bool(spec.get("harness_bins"))
    h_doc = bool(spec.get("harness_doc"))
    h_tests = list(spec.get("harness_tests") or [])
    h_splits = list(spec.get("harness_splits") or [])
    u_lib = bool(spec.get("updater_lib"))
    u_bins = bool(spec.get("updater_bins"))
    u_doc = bool(spec.get("updater_doc"))
    u_tests = list(spec.get("updater_tests") or [])
    u_splits = list(spec.get("updater_splits") or [])
    updater_demo = bool(spec.get("updater_demo"))
    privacy = bool(spec.get("privacy"))

    if build_only:
        # Cargo rejects combining `--doc` with `--no-run`; compiling `--lib` builds the library that `--doc` tests.
        h_build_targets = sorted(set(h_tests) | {s["test"] for s in h_splits} | ({PRIVACY_BINARY} if privacy else set()))
        if h_lib or h_doc or h_bins or h_build_targets:
            argv = ["cargo", "test", "-p", "harness", "--locked", "--no-run"]
            if h_lib or h_doc:
                argv.append("--lib")
            if h_bins:
                argv.append("--bins")
            for t in h_build_targets:
                argv += ["--test", t]
            cmds.append({"cwd": "lez", "dev_mode": False, "argv": argv})

        u_build_targets = sorted(set(u_tests) | {s["test"] for s in u_splits})
        if u_lib or u_doc or u_bins or u_build_targets:
            argv = ["cargo", "test", "-p", "ferrata-updater", "--features", "harness", "--locked", "--no-run"]
            if u_lib or u_doc:
                argv.append("--lib")
            if u_bins:
                argv.append("--bins")
            for t in u_build_targets:
                argv += ["--test", t]
            cmds.append({"cwd": "lez", "dev_mode": False, "argv": argv})
        if not cmds:
            raise ValueError(f"shard {shard_name!r} produces no build commands (empty shard)")
        return cmds

    # Run mode: harness whole (lib/bins/tests), harness doc-tests, harness splits,
    # updater whole (lib/bins/tests), updater doc-tests, updater splits, then privacy.
    if h_lib or h_bins or h_tests:
        argv = ["cargo", "test", "-p", "harness", "--locked"]
        if h_lib:
            argv.append("--lib")
        if h_bins:
            argv.append("--bins")
        for t in h_tests:
            argv += ["--test", t]
        argv += ["--", "--test-threads", str(threads)]
        cmds.append({"cwd": "lez", "dev_mode": True, "argv": argv})

    if h_doc:
        argv = ["cargo", "test", "-p", "harness", "--locked", "--doc", "--", "--test-threads", str(threads)]
        cmds.append({"cwd": "lez", "dev_mode": True, "argv": argv})

    for split in h_splits:
        argv = ["cargo", "test", "-p", "harness", "--locked", "--test", split["test"], "--", "--test-threads", str(threads), "--exact"]
        if split["mode"] == "exact":
            argv += list(split["names"])
        else:
            for name in split["names"]:
                argv += ["--skip", name]
        cmds.append({"cwd": "lez", "dev_mode": True, "argv": argv})

    if u_lib or u_bins or u_tests:
        argv = ["cargo", "test", "-p", "ferrata-updater", "--features", "harness", "--locked"]
        if u_lib:
            argv.append("--lib")
        if u_bins:
            argv.append("--bins")
        for t in u_tests:
            argv += ["--test", t]
        argv += ["--", "--test-threads", str(threads)]
        cmds.append({"cwd": "lez", "dev_mode": True, "argv": argv})

    if u_doc:
        argv = [
            "cargo", "test", "-p", "ferrata-updater", "--features", "harness", "--locked",
            "--doc", "--", "--test-threads", str(threads),
        ]
        cmds.append({"cwd": "lez", "dev_mode": True, "argv": argv})

    for split in u_splits:
        argv = [
            "cargo", "test", "-p", "ferrata-updater", "--features", "harness", "--locked",
            "--test", split["test"], "--", "--test-threads", str(threads), "--exact",
        ]
        if split["mode"] == "exact":
            argv += list(split["names"])
        else:
            for name in split["names"]:
                argv += ["--skip", name]
        cmds.append({"cwd": "lez", "dev_mode": True, "argv": argv})

    if updater_demo:
        cmds.append({"cwd": "lez", "dev_mode": True, "argv": ["bash", "updater/dev/demo.sh", "target/updater-demo"]})

    if privacy:
        run_sh = f"{repo_prefix}scripts/privacy/run.sh"
        gate_py = f"{repo_prefix}scripts/privacy/check-results.py"
        cmds.append({"cwd": ".", "dev_mode": False, "argv": ["bash", run_sh, privacy_out]})
        cmds.append({"cwd": ".", "dev_mode": False, "argv": ["python3", "-I", gate_py, privacy_out]})

    if not cmds:
        raise ValueError(f"shard {shard_name!r} produces no commands (empty shard)")
    return cmds


def _format_cmd(cmd: dict) -> str:
    prefix = "RISC0_DEV_MODE=1 " if cmd["dev_mode"] else ""
    line = prefix + shlex.join(cmd["argv"])
    if cmd["cwd"] != ".":
        return f"(cd {shlex.quote(cmd['cwd'])} && {line})"
    return line


def main(argv: list[str]) -> int:
    map_path = DEFAULT_MAP
    harness_dir = DEFAULT_HARNESS_DIR
    updater_dir = DEFAULT_UPDATER_DIR
    workflow_path: pathlib.Path | None = DEFAULT_WORKFLOW
    mode = "check"
    target_shard: str | None = None
    build_only = False
    threads = "4"
    privacy_out = "$PRIVACY_OUT"
    repo_prefix = ""

    def next_val(flag: str, idx: int) -> str:
        if idx + 1 >= len(argv):
            print(f"check-lez-shards.py: {flag} requires a value", file=sys.stderr)
            sys.exit(2)
        return argv[idx + 1]

    i = 0
    while i < len(argv):
        a = argv[i]
        if a == "--map":
            map_path = pathlib.Path(next_val(a, i))
            i += 2
        elif a == "--harness-dir":
            harness_dir = pathlib.Path(next_val(a, i))
            i += 2
        elif a == "--updater-dir":
            updater_dir = pathlib.Path(next_val(a, i))
            i += 2
        elif a == "--workflow":
            workflow_path = pathlib.Path(next_val(a, i))
            i += 2
        elif a == "--no-workflow":
            workflow_path = None
            i += 1
        elif a == "--list-shards":
            mode = "list"
            i += 1
        elif a == "--emit-tsv":
            mode = "tsv"
            target_shard = next_val(a, i)
            i += 2
        elif a == "--dry-run":
            mode = "dry-run"
            if i + 1 < len(argv) and not argv[i + 1].startswith("-"):
                target_shard = argv[i + 1]
                i += 2
            else:
                i += 1
        elif a == "--build-only":
            build_only = True
            i += 1
        elif a == "--threads":
            threads = next_val(a, i)
            i += 2
        elif a == "--privacy-out":
            privacy_out = next_val(a, i)
            i += 2
        elif a == "--repo-prefix":
            repo_prefix = next_val(a, i)
            i += 2
        else:
            print(f"check-lez-shards.py: unknown argument {a!r}", file=sys.stderr)
            return 2

    if mode == "list":
        data = json.loads(map_path.read_text(encoding="utf-8"))
        for name in data.get("shards") or {}:
            print(name)
        return 0

    if mode == "tsv":
        data = json.loads(map_path.read_text(encoding="utf-8"))
        try:
            cmds = build_shard_plan(data, target_shard or "", threads=threads, build_only=build_only,
                                    privacy_out=privacy_out, repo_prefix=repo_prefix)
        except (KeyError, ValueError) as exc:
            print(f"check-lez-shards.py: {exc}", file=sys.stderr)
            return 1
        for cmd in cmds:
            fields = [cmd["cwd"], "1" if cmd["dev_mode"] else "0", *cmd["argv"]]
            print("\t".join(fields))
        return 0

    if mode == "dry-run":
        data = json.loads(map_path.read_text(encoding="utf-8"))
        names = [target_shard] if target_shard else list((data.get("shards") or {}).keys())
        for name in names:
            try:
                cmds = build_shard_plan(data, name, threads=threads, build_only=build_only,
                                        privacy_out=privacy_out, repo_prefix=repo_prefix)
            except (KeyError, ValueError) as exc:
                print(f"check-lez-shards.py: {exc}", file=sys.stderr)
                return 1
            label = f"{name} (build)" if build_only else name
            print(f"# shard: {label}")
            for cmd in cmds:
                print(_format_cmd(cmd))
        return 0

    problems, summary = check_shards(map_path, harness_dir, updater_dir, workflow_path)
    if problems:
        for p in problems:
            print(f"ERROR: {p}", file=sys.stderr)
        print(f"check-lez-shards.py: {len(problems)} problem(s)", file=sys.stderr)
        return 1
    print(
        f"check-lez-shards.py: ok — {summary['shards']} shards cover "
        f"lez/harness (lib, {summary['harness_bins']} bins, doc, "
        f"{summary['harness_total'] - summary['harness_feature_gated']} active + "
        f"{summary['harness_feature_gated']} feature-gated tests) and "
        f"lez/updater (lib, {summary['updater_bins']} bin, doc, {summary['updater_total']} tests)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
