"""Offline and read-only online re-checks of the LEZ v0.4.0 port's own records.

The selector is parsed as shell text, never run. The SPEL check re-derives its counts from the
committed JSON and vectors. The catalogue check also holds its evidential paths to the inputs of
another lez040 check, so a recorded run cannot be cited without a re-check that reads it.
"""
from __future__ import annotations

import json
import re
import shutil
import subprocess
import tomllib
import urllib.error
import urllib.request
from pathlib import Path

from .core import REGISTRY, CheckFailed, Skipped, check, command, fail_if

TAGS = ("v0.4.0-rc1", "v0.4.0-rc2", "v0.4.0-rc3", "v0.4.0")
SELECTOR = "lez/scripts/lez-select.sh"
TAG_RECORD = "docs/port-v0.4.0/lez-tags.json"
PORT_DOC = "docs/port-v0.4.0.md"
LEZ_REMOTE = "https://github.com/logos-blockchain/logos-execution-zone"
RAW_LEZ = "https://raw.githubusercontent.com/logos-blockchain/logos-execution-zone"
RISC0_NAMES = ("risc0-zkvm", "risc0-build", "risc0-zkos-v1compat")
PROGRAMS = ("ferrata_attestation", "ferrata_bridge")
SPEL_RUNS = (
    ("v0.4.0", "lez-a255b42c"),
    ("v0.4.0-rc3", "lez-bafd07be"),
)


def _array(text: str, name: str) -> list[str]:
    m = re.search(rf"^{re.escape(name)}=\((.*?)\)", text, re.S | re.M)
    if not m:
        raise CheckFailed(f"{SELECTOR}: no shell array {name}")
    return m.group(1).split()


def _commit_map(text: str) -> dict[str, str]:
    m = re.search(r"^commit_of\(\) \{(.*?)^\}", text, re.S | re.M)
    body = m.group(1) if m else ""
    pairs = re.findall(r"^\s+(v0\.4\.0(?:-rc\d+)?)\) echo ([0-9a-f]{40}) ;;$", body, re.M)
    if dict(pairs) != {tag: commit for tag, commit in pairs} or set(dict(pairs)) != set(TAGS) or len(pairs) != 4:
        raise CheckFailed(f"{SELECTOR}: commit_of must name exactly {', '.join(TAGS)}, each with one 40-hex commit")
    return dict(pairs)


def _risc0_map(text: str) -> dict[str, tuple[str, str, str]]:
    m = re.search(
        r'^risc0_of\(\) \{ if \[ "\$1" = v0\.4\.0 \]; then echo "([0-9.]+ [0-9.]+ [0-9.]+)"; '
        r'else echo "([0-9.]+ [0-9.]+ [0-9.]+)"; fi; \}$', text, re.M)
    if not m:
        raise CheckFailed(f"{SELECTOR}: risc0_of is not the expected v0.4.0/other-tag three-pin switch")
    final, earlier = tuple(m.group(1).split()), tuple(m.group(2).split())
    return {tag: final if tag == "v0.4.0" else earlier for tag in TAGS}


def _selector_inputs() -> tuple[str, ...]:
    """The manifests and every lockfile any parsed patch can name (for the minimal tamper copies). Read when the module
    is imported: a missing or unparsable file leaves the inputs at the selector and the record, and the check itself
    then fails on it (the registry of every other check must still load)."""
    root = Path(__file__).resolve().parents[2]
    out = [SELECTOR, TAG_RECORD]
    try:
        text = (root / SELECTOR).read_text(encoding="utf-8")
        for name in ("manifests", "rc3_pinned", "featured", "sourced_040"):
            out.extend(f"lez/{d}/Cargo.toml" for d in _array(text, name))
        for d in _array(text, "sourced_040"):
            for twin in ("src/main_lez040.rs", "src/lib_lez040.rs"):
                if (root / "lez" / d / twin).is_file():
                    out.append(f"lez/{d}/{twin}")
        for tag in TAGS[1:]:
            rel = f"lez/lockfiles/{tag}.patch"
            out.append(rel)
            patch = (root / rel).read_text(encoding="utf-8")
            out.extend(f"lez/{p}" for p in re.findall(r"^(?:--- a/|\+\+\+ b/)([^\n]+)$", patch, re.M))
    except (OSError, CheckFailed):
        pass
    return tuple(dict.fromkeys(out))


def _remote_map(text: str) -> dict[str, str]:
    return {tag: commit for commit, tag in re.findall(
        r"^([0-9a-f]{40})\trefs/tags/(v0\.4\.0(?:-rc\d+)?)(?:\^\{\})?$", text, re.M)}


@check(id="lez040-selector", family="hash", mode="offline", rows=(), evidence=(),
       title="The LEZ tag selector against LEZ's published tags and RISC Zero pins",
       what="lez-select.sh's four tag commits, RISC Zero triples, feature selection, manifests and lockfile patches",
       against="docs/port-v0.4.0/lez-tags.json's GitHub record, the committed manifests and patched Cargo.lock files",
       inputs=_selector_inputs())
def lez040_selector(ctx):
    text = (ctx.root / SELECTOR).read_text(encoding="utf-8")
    record = json.loads((ctx.root / TAG_RECORD).read_text(encoding="utf-8"))
    commits, pins = _commit_map(text), _risc0_map(text)
    remote = _remote_map("\n".join(record.get("ls_remote", [])))
    problems = []

    for tag in TAGS:
        entry = record.get("tags", {}).get(tag, {})
        recorded = entry.get("commit")
        raw = remote.get(tag)
        if commits[tag] != recorded:
            problems.append(f"{tag}: selector commit {commits[tag]} differs from the record's {recorded}")
        if raw != recorded:
            problems.append(f"{tag}: record commit {recorded} differs from its raw ls_remote line's {raw}")
        triple = tuple(entry.get("risc0", {}).get(name) for name in RISC0_NAMES)
        if pins[tag] != triple:
            problems.append(f"{tag}: selector RISC Zero triple {pins[tag]} differs from the record's {triple}")

    feature_body = re.search(r"^features_of\(\) \{(.*?)^\}", text, re.S | re.M)
    found = re.findall(r"(v0\.4\.0(?:-rc\d+)?|\*)\) echo (.*?) ;;", feature_body.group(1) if feature_body else "")
    expected_features = {
        "v0.4.0-rc1": '""', "v0.4.0-rc2": '\'"lez-rc2"\'',
        "v0.4.0-rc3": '\'"lez-rc3"\'', "*": '\'"lez-040"\''}
    if len(found) != 4 or dict(found) != expected_features:
        problems.append(f"features_of: expected rc1 no feature, rc2 lez-rc2, rc3 lez-rc3, v0.4.0 lez-040; found {found}")

    for group in ("featured", "manifests", "rc3_pinned"):
        for directory in _array(text, group):
            rel = f"lez/{directory}/Cargo.toml"
            manifest = (ctx.root / rel).read_text(encoding="utf-8")
            if group == "featured":
                feature = re.search(r"^lez-040\s*=\s*\[([^]]*)\]", manifest, re.M)
                if not feature or '"lez-rc3"' not in feature.group(1):
                    problems.append(f"{rel}: lez-040 does not include \"lez-rc3\"")
                if len(re.findall(r"^default = \[\] # lez-select$", manifest, re.M)) != 1:
                    problems.append(f"{rel}: the committed default is not exactly 'default = [] # lez-select'")
            else:
                trees = set(re.findall(r"upstream/lez-([A-Za-z0-9.+-]+)/", manifest))
                wanted = "v0.4.0-rc3" if group == "rc3_pinned" else "v0.4.0-rc1"
                if trees != {wanted}:
                    problems.append(f"{rel}: LEZ trees {sorted(trees)}; expected only upstream/lez-{wanted}/")

    sourced = _array(text, "sourced_040") if re.search(r"^sourced_040=\(", text, re.M) else []
    for directory in sourced:
        rel = f"lez/{directory}/Cargo.toml"
        manifest = (ctx.root / rel).read_text(encoding="utf-8")
        line = re.search(r'^path = "([^"]*/)(main|lib)(_lez040)?\.rs" # lez-select$', manifest, re.M)
        if not line:
            problems.append(f"{rel}: no '# lez-select' source path line")
        elif line.group(3):
            problems.append(f"{rel}: the committed source line names the v0.4.0 twin; the tree commits the release "
                            "candidates' source")
        elif not (ctx.root / "lez" / directory / f"{line.group(1)}{line.group(2)}_lez040.rs").is_file():
            problems.append(f"{rel}: its v0.4.0 twin {line.group(1)}{line.group(2)}_lez040.rs is missing")

    patched = set()
    for tag in TAGS[1:]:
        rel = f"lez/lockfiles/{tag}.patch"
        path = ctx.root / rel
        if not path.is_file():
            problems.append(f"{rel}: missing")
            continue
        paths = re.findall(r"^(?:--- a/|\+\+\+ b/)([^\n]+)$", path.read_text(encoding="utf-8"), re.M)
        if not paths:
            problems.append(f"{rel}: names no files in its --- a/ and +++ b/ lines")
        for name in paths:
            patched.add(name)
            if Path(name).name != "Cargo.lock":
                problems.append(f"{rel}: patches {name}, not a Cargo.lock")
            if not (ctx.root / "lez" / name).is_file():
                problems.append(f"{rel}: patched file lez/{name} does not exist")
    fail_if(problems, "the LEZ v0.4.0 selector")
    return [
        f"4 of 4 tags: the selector's commit is LEZ's tag as GitHub listed it at {record['read_at']}",
        "4 of 4 tags: risc0-zkvm, risc0-build and risc0-zkos-v1compat triples equal the record's pins; "
        "features rc1 none, rc2 lez-rc2, rc3 lez-rc3, v0.4.0 lez-040",
        f"{len(_array(text, 'featured'))} featured manifests: lez-040 includes lez-rc3 and the committed default is rc1; "
        f"{len(_array(text, 'manifests'))} moving manifests on rc1, {len(_array(text, 'rc3_pinned'))} pinned on rc3",
        f"3 lockfile patches: every ---/+++ path is an existing Cargo.lock ({len(patched)} distinct paths)",
        f"{len(sourced)} crates with a v0.4.0 twin of their source: each commits the release candidates' file, the twin "
        "beside it",
    ]


def _ls_remote(url: str) -> str:
    """The one live tag read; replaceable by tests."""
    p = subprocess.run(["git", "ls-remote", "--tags", url, "refs/tags/v0.4.0*"],
                       capture_output=True, text=True, timeout=30)
    if p.returncode:
        raise OSError(f"git ls-remote exited {p.returncode}: {p.stderr.strip()}")
    return p.stdout


def _fetch(url: str) -> str:
    """The one live Cargo.toml read; replaceable by tests."""
    with urllib.request.urlopen(url, timeout=30) as response:
        return response.read().decode("utf-8")


@check(id="lez040-selector-online", family="github", mode="online", rows=(), evidence=(),
       title="LEZ's four selector tags and v0.4.0 RISC Zero pins, read live from GitHub",
       what="the four live v0.4.0 tag commits and the root Cargo.toml's three workspace dependency pins at v0.4.0",
       against="lez-select.sh's commit_of and risc0_of (additional live tags are information only)",
       needs=("python3", "git", "network"), inputs=(SELECTOR,))
def lez040_selector_online(ctx):
    try:
        listing = _ls_remote(LEZ_REMOTE)
    except (OSError, urllib.error.URLError) as e:
        raise Skipped(f"cannot read LEZ's live tags: {e}") from e
    live = _remote_map(listing)
    text = (ctx.root / SELECTOR).read_text(encoding="utf-8")
    commits, pins = _commit_map(text), _risc0_map(text)
    problems = []
    for tag in TAGS:
        if tag not in live:
            problems.append(f"{tag}: absent from git ls-remote's live answer")
        elif live[tag] != commits[tag]:
            problems.append(f"{tag}: live commit {live[tag]} differs from the selector's {commits[tag]}")
    commit = live.get("v0.4.0", commits["v0.4.0"])
    url = f"{RAW_LEZ}/{commit}/Cargo.toml"
    try:
        manifest = _fetch(url)
    except (OSError, urllib.error.URLError) as e:
        raise Skipped(f"cannot read LEZ's root Cargo.toml at v0.4.0: {e}") from e
    try:
        dependencies = tomllib.loads(manifest).get("workspace", {}).get("dependencies", {})
    except tomllib.TOMLDecodeError as e:
        raise CheckFailed(f"{url}: invalid TOML: {e}") from e
    for name, wanted in zip(RISC0_NAMES, pins["v0.4.0"]):
        value = dependencies.get(name)
        version = value.get("version") if isinstance(value, dict) else value
        if not isinstance(version, str) or version.removeprefix("=") != wanted:
            problems.append(f"{url}: {name} pin {version!r} differs from risc0_of v0.4.0's {wanted}")
    fail_if(problems, "LEZ's live v0.4.0 tags")
    lines = [f"4 of 4 selector tags equal git ls-remote's live commits; v0.4.0 is {commits['v0.4.0']}",
             f"{url}: all three RISC Zero pins equal risc0_of v0.4.0"]
    lines.extend(f"information: additional live tag {tag} at {live[tag]} (not one of the selector's four)"
                 for tag in sorted(set(live) - set(TAGS)))
    return lines


def _spel_paths(tree: str):
    prefix = f"idl/generated/spel-v2-f739d8a-{tree}.txt"
    directory = f"idl/generated/at-f739d8a-{tree}"
    return prefix, directory


def _check_spel_log(ctx, tag: str, tree: str, record_commits: dict, problems: list) -> None:
    log_rel, directory = _spel_paths(tree)
    text = (ctx.root / log_rel).read_text(encoding="utf-8")
    lines = text.splitlines()
    header = re.match(r"# lee_core patched to upstream/" + re.escape(tree) + r" \(LEZ commit ([0-9a-f]{40})\)$",
                      lines[1] if len(lines) > 1 else "")
    wanted_commit = record_commits[tag]
    if not header or header.group(1) != wanted_commit:
        problems.append(f"{log_rel}: second-line LEZ commit differs from the record's {tag} commit {wanted_commit}")
    pattern = re.compile(
        r'  generated: IDL version "(\d+)", (\d+) messages, (\d+) types\n'
        r'  wrote ' + re.escape(directory) + r'/(ferrata_(?:attestation|bridge))\.idl-v2\.json')
    matches = list(pattern.finditer(text))
    if len(matches) != 2:
        problems.append(f"{log_rel}: expected two generated-IDL records, found {len(matches)}")
        return
    for i, (m, program) in enumerate(zip(matches, PROGRAMS)):
        end = matches[i + 1].start() if i + 1 < len(matches) else len(text)
        block = text[m.end():end]
        if m.group(4) != program:
            problems.append(f"{log_rel}: generated record {m.group(4)} is out of order; expected {program}")
            continue
        generated = json.loads((ctx.root / directory / f"{program}.idl-v2.json").read_text(encoding="utf-8"))
        counts = (str(generated.get("version")), len(generated.get("messages", [])), len(generated.get("types", [])))
        stated = (m.group(1), int(m.group(2)), int(m.group(3)))
        if stated != counts:
            problems.append(f"{log_rel}: {program} generated tuple {stated} differs from JSON's {counts}")
        if not re.search(r"against the hand-written IDL: \d+ messages and types compared, 0 difference\(s\)", block):
            problems.append(f"{log_rel}: {program} has no 0 difference(s) line")
        vectors = json.loads((ctx.root / f"idl/vectors/{program}.vectors.json").read_text(encoding="utf-8"))["vectors"]
        decoded = re.search(r"committed vectors: SPEL v2 decoded (\d+) of (\d+) exactly; mutants refused", block)
        if not decoded or (int(decoded.group(1)), int(decoded.group(2))) != (len(vectors), len(vectors)):
            problems.append(f"{log_rel}: {program} decoded counts {decoded.groups() if decoded else None} differ from "
                            f"the vector file's {len(vectors)} of {len(vectors)}")


TUPLE_NAMES = {"Tuple_Root_u128": "FeeShare"}  # SPEL's generated name for `(Root, u128)`, the hand-written IDL's name


def _semantics(generated: dict, hand: dict, program: str) -> list:
    """The generated IDL (SPEL v2) against the hand-written one: each message's name, tag and fields (names and types)
    against the instruction its discriminator names, and the type names both ways (the message enum and the tuple's
    name aside). Returns the differences."""
    def norm(value):
        text = json.dumps(value, sort_keys=True)
        for generated_name, hand_name in TUPLE_NAMES.items():
            text = text.replace(f'"{generated_name}"', f'"{hand_name}"')
        return text
    out = []
    messages, instructions = generated.get("messages", []), hand.get("instructions", [])
    if len(messages) != len(instructions):
        out.append(f"{program}: {len(messages)} generated messages, {len(instructions)} hand-written instructions")
    for message, instruction in zip(messages, instructions):
        where = f"{program}: message {message.get('tag')} ({message.get('name')})"
        if message.get("name") != instruction.get("variant") or [message.get("tag")] != instruction.get("discriminator"):
            out.append(f"{where}: the hand-written instruction there is {instruction.get('variant')} "
                       f"{instruction.get('discriminator')}")
            continue
        fields = [(f.get("name"), norm(f.get("type"))) for f in message.get("fields", [])]
        args = [(a.get("name"), norm(a.get("type"))) for a in instruction.get("args", [])]
        if fields != args:
            out.append(f"{where}: fields {[f[0] for f in fields]} differ from the hand-written arguments "
                       f"{[a[0] for a in args]} (names or types)")
    generated_types = {TUPLE_NAMES.get(t.get("name"), t.get("name")) for t in generated.get("types", [])}
    hand_types = {t.get("name") for t in hand.get("types", [])} | {a.get("name") for a in hand.get("accounts", [])}
    hand_types.discard(hand.get("instruction_type", "").rsplit("::", 1)[-1])
    for name in sorted(generated_types - hand_types):
        out.append(f"{program}: type {name} is generated, not hand-written")
    for name in sorted(hand_types - generated_types):
        out.append(f"{program}: hand-written type {name} is not generated")
    return out


@check(id="lez040-spel", family="hash", mode="offline", rows=("RFP-021 U.4", "RFP-022 U.4"), evidence=(301,),
       title="SPEL's runs on LEZ v0.4.0 and rc3 at the same tree: the generated IDLs against the hand-written ones, the records' figures",
       what="the generated IDLs of the v0.4.0 run against the hand-written IDLs (every message's name, tag and fields, "
            "every type name), byte for byte the rc3 run's; the logs' header commits, counts and '0 difference(s)'; "
            "the two logs equal apart from their headers and the LEZ tree names. The generator itself is rerun by "
            "lez040-spel-rerun (toolchain tier)",
       against="idl/ferrata_attestation.idl.json and idl/ferrata_bridge.idl.json, idl/vectors, "
               "docs/port-v0.4.0/lez-tags.json",
       inputs=(TAG_RECORD,) + tuple(
           rel
           for _, tree in SPEL_RUNS
           for rel in (_spel_paths(tree)[0],) + tuple(f"{_spel_paths(tree)[1]}/{p}.idl-v2.json" for p in PROGRAMS)
       ) + tuple(f"idl/vectors/{p}.vectors.json" for p in PROGRAMS) + tuple(f"idl/{p}.idl.json" for p in PROGRAMS))
def lez040_spel(ctx):
    record = json.loads((ctx.root / TAG_RECORD).read_text(encoding="utf-8"))
    commits = {tag: record["tags"][tag]["commit"] for tag, _ in SPEL_RUNS}
    problems = []
    for program in PROGRAMS:
        final = ctx.root / _spel_paths("lez-a255b42c")[1] / f"{program}.idl-v2.json"
        rc3 = ctx.root / _spel_paths("lez-bafd07be")[1] / f"{program}.idl-v2.json"
        if final.read_bytes() != rc3.read_bytes():
            problems.append(f"{program}.idl-v2.json: v0.4.0 and rc3 generated files are not byte for byte equal")
        problems.extend(_semantics(json.loads(final.read_text(encoding="utf-8")),
                                   json.loads((ctx.root / f"idl/{program}.idl.json").read_text(encoding="utf-8")),
                                   program))
    texts = {}
    for tag, tree in SPEL_RUNS:
        rel, _ = _spel_paths(tree)
        texts[tree] = (ctx.root / rel).read_text(encoding="utf-8")
        _check_spel_log(ctx, tag, tree, commits, problems)
    comparable = []
    for tree, text in texts.items():
        body = text.splitlines()[2:]
        comparable.append("\n".join(line.replace("lez-a255b42c", "<tree>").replace("lez-bafd07be", "<tree>")
                                    for line in body))
    if comparable[0] != comparable[1]:
        first = next((i for i, (a, b) in enumerate(zip(comparable[0].splitlines(), comparable[1].splitlines()), 3)
                      if a != b), "length")
        problems.append(f"the SPEL logs differ beyond the two header lines and tree names (first body line {first})")
    fail_if(problems, "the SPEL records on LEZ v0.4.0")
    return [
        "2 generated program IDLs: v0.4.0 equals rc3 byte for byte, and each equals its hand-written IDL message by "
        "message (name, tag, fields and their types) and type by type; log headers name the recorded v0.4.0 and rc3 "
        "commits",
        "both runs: 15 messages/18 types and 21/21 vectors (attestation), 34 messages/32 types and 60/60 vectors "
        "(bridge), each with 0 differences; bodies equal after setting aside headers and tree names",
    ]


def _cells(line: str) -> list[str]:
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def _catalogue_rows(text: str):
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if not line.startswith("|"):
            continue
        header = _cells(line)
        if "Status" not in header or "Evidence" not in header:
            continue
        status_col, evidence_col = header.index("Status"), header.index("Evidence")
        for row_line in lines[i + 2:]:
            if not row_line.startswith("|"):
                break
            row = _cells(row_line)
            if row and re.fullmatch(r"[CS]\d+", row[0]):
                yield row[0], row[status_col], row[evidence_col]


def _under_root(ctx, token: str):
    token = token.strip("'\"(),;:[]{}<>")
    path = Path(token)
    if path.is_absolute() or ".." in path.parts:
        return None
    candidate = ctx.root / path
    return token if candidate.exists() else None


def _read_by(path: str, inputs: set) -> bool:
    """A cited path is read by a check whose inputs name it, a directory holding it, or a file inside it (a run's
    directory cited, its records read)."""
    p = path.rstrip("/")
    return any(p == i.rstrip("/") or p.startswith(i.rstrip("/") + "/") or i.startswith(p + "/") for i in inputs)


# The rows the catalogue must hold (more may follow), so that a renamed header cannot leave nothing to check.
EXPECTED_ROWS = tuple(f"C{i}" for i in range(1, 21)) + ("S1", "S2", "S3")
# Where a run's record lives: a "recorded run" path must be one, not any file a check happens to read.
RUN_RECORD_DIRS = ("e2e/runs/", "cli/results/", "docs/port-v0.4.0/image-ids/", "docs/port-v0.4.0/harness-",
                   "idl/generated/", "lez/results/")
# A "tested on v0.4.0" row needs at least one command that is about v0.4.0: a lez040 check, or a path or flag that
# names it (any CI test would otherwise do).
V040_MARKERS = ("lez040", "lez_040", "v040", "v0.4.0", "lez-0-4-0", "lez_0_4_0", "lez-a255b42c")


def _words(command: str) -> list:
    """The command's shell words, a `#` comment dropped (a marker in a comment is no part of what runs)."""
    import shlex
    try:
        return shlex.split(command, comments=True)
    except ValueError:
        return command.split("#", 1)[0].split()


def _is_run_record(path: str) -> bool:
    p = path.rstrip("/")
    web = p.startswith("deploy/testnet/web/") and "/test/results/" in p + "/"
    return web or any(p.startswith(d) and len(p) > len(d.rstrip("/")) for d in RUN_RECORD_DIRS)


def _about_v040(command: str) -> bool:
    tokens = _words(command)
    if "scripts/verify.py" in tokens:
        return any(t.startswith("lez040-") for t in tokens)
    return any(marker in token for token in tokens for marker in V040_MARKERS)


CI_FILES = (".github/workflows/ci.yml", ".github/workflows/sequencer.yml", ".github/workflows/verify-online.yml", "justfile")


def _command_runs_somewhere(ctx, command: str) -> bool:
    """A "tested on v0.4.0" command must be run by something that fails when it fails: `scripts/verify.py` naming
    registered checks only, the command of a registered check itself, or a test file a CI workflow or the justfile
    runs. This check does not run the command; CI's jobs and verify.py do."""
    from . import catalog  # every module's checks (verify.py loads them all; a test may import this module alone)
    from .core import ROOT
    catalog.load(ROOT)
    tokens = _words(command)
    if "scripts/verify.py" in tokens:
        ids = [t for t in tokens[tokens.index("scripts/verify.py") + 1:] if not t.startswith("-")]
        return bool(ids) and all(i in REGISTRY for i in ids)
    for registered in REGISTRY.values():
        own = " ".join(registered.argv) if registered.argv else registered.shell
        if own and own == command:
            return True
    runners = "\n".join((ctx.root / f).read_text(encoding="utf-8") for f in CI_FILES if (ctx.root / f).is_file())
    paths = [t for t in tokens if "/" in t and _under_root(ctx, t)]
    return bool(paths) and all(path in runners for path in paths)


def _commit_exists(git_dir: Path, commit: str) -> bool:
    p = subprocess.run(["git", "cat-file", "-e", f"{commit}^{{commit}}"], cwd=git_dir,
                       capture_output=True, text=True)
    return p.returncode == 0


# The port's unpublished development history: ids the catalogue and records name, published as one commit.
UNPUBLISHED = "docs/port-v0.4.0/unpublished-commits.txt"
PUBLISHED_PORT = "6a539b7f"


def _unpublished_ids(root: Path) -> set:
    path = root / UNPUBLISHED
    if not path.is_file():
        return set()
    return {line.split()[0] for line in path.read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.startswith("#")}


@check(id="lez040-catalogue", family="docs", mode="offline", rows=(), evidence=(),
       title="The v0.4.0 port catalogue's statuses held to the evidence they name",
       what="rows C1-C20 and S1-S3 present, each with an allowed status; building commits in git (or the port's listed unpublished "
            "history, published as 6a539b7f), tested commands and "
            "their paths (one of them about v0.4.0), recorded-run paths that are a run's record another lez040 check "
            "actually reads, and the page's status line",
       against="docs/port-v0.4.0.md, this clone's git objects, the cited files and the lez040 checks' declared inputs",
       needs=("python3", "git"),
       inputs=(PORT_DOC, UNPUBLISHED) + tuple(_spel_paths(tree)[0] for _, tree in SPEL_RUNS) + CI_FILES)
def lez040_catalogue(ctx):
    text = (ctx.root / PORT_DOC).read_text(encoding="utf-8")
    problems = []
    if "**Status of " not in text:
        problems.append(f"{PORT_DOC}: no '**Status of ' line")
    rows = list(_catalogue_rows(text))
    missing = [r for r in EXPECTED_ROWS if r not in {row_id for row_id, _, _ in rows}]
    if missing:
        problems.append(f"{PORT_DOC}: no row {', '.join(missing)} in a table with Status and Evidence columns")
    counts = {status: 0 for status in ("not started", "building", "tested on v0.4.0", "recorded run")}
    git_dir = getattr(ctx, "git_dir", None) or ctx.root
    needs_git = any(status == "building" for _, status, _ in rows)
    if needs_git and (shutil.which("git") is None or not (git_dir / ".git").exists()):
        raise Skipped(f"building rows need git and the repository objects at {git_dir}")
    checked_commits = {}
    unpublished = _unpublished_ids(ctx.root)
    other_inputs = {rel for cid, registered in REGISTRY.items()
                    if cid.startswith("lez040-") and cid != "lez040-catalogue" for rel in registered.inputs}
    for row_id, status, evidence in rows:
        if status not in counts:
            problems.append(f"{row_id}: unknown status {status!r}")
            continue
        counts[status] += 1
        if status == "building":
            commits = re.findall(r"`([0-9A-Fa-f]{8,40})`", evidence)
            if not commits:
                problems.append(f"{row_id}: building evidence names no backticked commit id")
            for commit in commits:
                if commit not in checked_commits:
                    # A commit of the unpublished history counts when its published commit is in the clone.
                    checked_commits[commit] = _commit_exists(git_dir, commit) or (
                        commit in unpublished and _commit_exists(git_dir, PUBLISHED_PORT))
                if not checked_commits[commit]:
                    problems.append(f"{row_id}: building commit {commit} is not a commit in this repository nor one of "
                                    f"{UNPUBLISHED} (published as {PUBLISHED_PORT})")
        elif status == "tested on v0.4.0":
            commands = re.findall(r"`((?:python3|node|cargo|bash|just|forge) [^`]*)`", evidence)
            if not commands:
                problems.append(f"{row_id}: tested on v0.4.0 evidence has no backticked command with an allowed runner")
            for command in commands:
                for token in command.split():
                    if "/" in token and _under_root(ctx, token) is None:
                        problems.append(f"{row_id}: command path {token!r} does not exist under the repository root")
                if not _command_runs_somewhere(ctx, command):
                    problems.append(f"{row_id}: the command {command!r} is no registered check, names none, and is no "
                                    "test CI runs")
            if commands and not any(_about_v040(command) for command in commands):
                problems.append(f"{row_id}: tested on v0.4.0, but no command is about v0.4.0 (a lez040 check, or a "
                                "path or flag naming v0.4.0)")
        elif status == "recorded run":
            paths = re.findall(r"`([^`\s]+/[^`\s]*)`", evidence)
            existing = [_under_root(ctx, path) for path in paths]
            if not any(existing):
                problems.append(f"{row_id}: recorded run names no existing backticked repository path")
            for path, present in zip(paths, existing):
                if present is None:
                    problems.append(f"{row_id}: recorded-run path {path} does not exist under the repository root")
                elif not _is_run_record(path):
                    problems.append(f"{row_id}: recorded-run path {path} is not a run's record (under "
                                    f"{', '.join(RUN_RECORD_DIRS)} or deploy/testnet/web/*/test/results/)")
                elif not _read_by(path, other_inputs):
                    problems.append(f"{row_id}: recorded-run path {path} is in no other lez040 check's inputs")
    fail_if(problems, "the LEZ v0.4.0 catalogue")
    summary = ", ".join(f"{counts[status]} {status}" for status in counts)
    return [
        f"{len(rows)} catalogue rows: {summary}; the page has its **Status of line",
        f"building: {len(checked_commits)} distinct commit ids found in this clone's git objects; tested on v0.4.0: "
        "every command has an allowed runner and every path it names exists",
        "recorded run: every cited path exists, is a run's record and is an input of another lez040 check (which "
        "CI's verify job runs); tested on v0.4.0: every command is a registered check, names registered checks, or is "
        "a test CI runs, and one is about v0.4.0 (this check does not run them)",
    ]


command(id="lez040-spel-rerun", family="hash", mode="offline", tier="tests", timeout=3600,
        needs=("bash", "cargo", "git"),
        shell="FERRATA_REV=b612b652 bash idl/spel-v2/run-at.sh f739d8a lez-a255b42c && git diff --exit-code -- "
              "idl/generated/at-f739d8a-lez-a255b42c/ferrata_attestation.idl-v2.json "
              "idl/generated/at-f739d8a-lez-a255b42c/ferrata_bridge.idl-v2.json "
              "idl/generated/spel-v2-f739d8a-lez-a255b42c.txt",
        title="SPEL's generator and decoder rerun on LEZ v0.4.0: the committed IDLs and log reproduced",
        what="idl/spel-v2/run-at.sh f739d8a lez-a255b42c (SPEL's head f739d8a, lee_core patched to LEZ v0.4.0), then git "
             "diff of what it wrote against the committed files",
        against="idl/generated/at-f739d8a-lez-a255b42c/*.idl-v2.json and idl/generated/spel-v2-f739d8a-lez-a255b42c.txt "
                "(needs upstream/spel-f739d8a and upstream/lez-a255b42c beside the repository: idl/README.md)",
        inputs=("idl/spel-v2", "idl/generated/at-f739d8a-lez-a255b42c/ferrata_attestation.idl-v2.json",
                "idl/generated/at-f739d8a-lez-a255b42c/ferrata_bridge.idl-v2.json",
                "idl/generated/spel-v2-f739d8a-lez-a255b42c.txt"))
