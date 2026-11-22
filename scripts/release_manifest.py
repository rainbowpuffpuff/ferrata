"""The manifest of a Ferrata release (scripts/release.sh): what each file is, its size, sha256 and image id.

    python3 -I scripts/release_manifest.py make  DIR VERSION COMMIT SOURCES.tar BUILD-OK
                                                                      write DIR/MANIFEST.json from DIR's files
    python3 -I scripts/release_manifest.py check DIR [--strict]       every file of the manifest: size and sha256; the
                                                                      CLI is the binary check-build passed; the notices
                                                                      cover the native code and the crates
    python3 -I scripts/release_manifest.py id    DIR FILE             the image id the manifest gives programs/FILE
    python3 -I scripts/release_manifest.py params DIR [BASE.json]     vault deployment parameters with this release's
                                                                      image ids (BASE: the other fields, e.g. a chain's)

A release is made only from a build whose every step passed (review-fix-supply M-2): `make` requires the build's
BUILD-OK, whose cli_sha256 must be cli/ferrata's, and cli/check-build.json, the record check-build wrote after the
build (result pass, the same binary sha256, the pinned archives); the manifest's `native` section is that record's
archives, not the pins. The CLI links native code that cargo does not build from source (scripts/native-deps.json:
rapidsnark with GMP and ffiasm, the Logos circuits with circom's runtime, circomlib, nlohmann/json and GMP): `make`
and `check` refuse a release whose THIRD-PARTY-NOTICES.md does not name every archive with its sha256 and every
component with its licence, or whose licenses/ lacks a text a component needs, and one whose THIRD-PARTY-CRATES.md,
cli/crates.tsv and cli/Cargo.lock do not match (scripts/crate_notices.py check: review-fix-supply M-1). `make` records
the sha256 of every member of the third-party sources tarball and checks it against the pins; `check` checks the
tarball beside the release directory when it is there (--strict: it must be). A manifest without a `native` or a
`build` section (a release made before the notices or before BUILD-OK) is refused. --pins FILE (after the other
arguments) reads another scripts/native-deps.json (tests). Standard library only.
"""

import datetime
import hashlib
import importlib.util
import json
import os
import sys
import tarfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PINS = os.path.join(ROOT, "scripts", "native-deps.json")
NOTICES = "THIRD-PARTY-NOTICES.md"
LEGAL = ("LICENSE-MIT", "LICENSE-APACHE")
CHECK_BUILD = "cli/check-build.json"
_spec = importlib.util.spec_from_file_location("crate_notices", os.path.join(ROOT, "scripts", "crate_notices.py"))
cn = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cn)

LEZ_PROGRAMS = {
    "ferrata_lez.bin": "attestation program (LEZ program ELF, deployed with LEZ's program loader)",
    "ferrata_bridge.bin": "bridge program (LEZ program ELF, deployed with LEZ's program loader)",
}
R0BF_PROGRAMS = {
    "checkpoint.bin": "rule (A) checkpoint guest (the vault's checkpointImageIdA)",
    "checkpoint_b.bin": "rule (B) checkpoint guest (the vault's checkpointImageIdB)",
    "segment.bin": "rule (A) segment guest (composed into the checkpoint proof)",
    "release.bin": "release guest (the vault's releaseImageId)",
}


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def hex_id(s):
    s = s.strip().lower()
    return s if s.startswith("0x") else "0x" + s


def entry(d, rel, what, **extra):
    p = os.path.join(d, rel)
    e = {"file": rel, "bytes": os.path.getsize(p), "sha256": sha256(p), "what": what}
    e.update(extra)
    return e


def read_ids(d):
    ids = {}
    with open(os.path.join(d, "programs", "ids.txt")) as f:
        for line in f:
            parts = line.split()
            if len(parts) >= 3:
                ids[parts[0]] = (hex_id(parts[2]), parts[1])
    with open(os.path.join(d, "programs", "ids.jsonl")) as f:
        for line in f:
            if line.strip():
                j = json.loads(line)
                ids[os.path.basename(j["program"])] = (hex_id(j["image_id"]), j["sha256"])
    return ids


def vault_entry(d, name):
    rel = f"vault/{name}.json"
    a = json.load(open(os.path.join(d, rel)))
    code = a["bytecode"]["object"]
    runtime = a["deployedBytecode"]["object"]
    meta = a.get("metadata") or {}
    if isinstance(meta, str):
        meta = json.loads(meta)
    settings = meta.get("settings", {})
    if (len(runtime) - 2) // 2 > 24576:
        sys.exit(f"make: {name}'s runtime is {(len(runtime) - 2) // 2} bytes, over EIP-170's 24,576")
    return entry(
        d, rel, f"{name}: ABI, creation and runtime bytecode (forge artifact)",
        compiler=(meta.get("compiler") or {}).get("version"),
        evm_version=settings.get("evmVersion"),
        optimizer=settings.get("optimizer"),
        creation_bytes=(len(code) - 2) // 2,
        runtime_bytes=(len(runtime) - 2) // 2,
        creation_sha256=hashlib.sha256(code.encode()).hexdigest(),
        runtime_sha256=hashlib.sha256(runtime.encode()).hexdigest(),
        link_references=sorted(
            f"{src}:{lib}" for src, libs in (a["bytecode"].get("linkReferences") or {}).items() for lib in libs
        ),
    )


def read_kv(path):
    out = {}
    with open(path, encoding="utf-8") as f:
        for line in f:
            k, _, v = line.strip().partition(" ")
            if k:
                out[k] = v
    return out


def build_problems(d, m, build_ok=None):
    """Why the release's CLI is not the binary its build's check-build passed (empty: it is)."""
    probs = []
    p = os.path.join(d, CHECK_BUILD)
    if not os.path.isfile(p):
        return [f"{CHECK_BUILD} missing: no record that check-build passed this CLI (review-fix-supply M-2)"]
    with open(p, encoding="utf-8") as f:
        rec = json.load(f)
    cli = sha256(os.path.join(d, "cli/ferrata")) if os.path.isfile(os.path.join(d, "cli/ferrata")) else "missing"
    if rec.get("result") != "pass":
        probs.append(f"{CHECK_BUILD}: result {rec.get('result')!r}, not pass")
    if (rec.get("binary") or {}).get("sha256") != cli:
        probs.append(f"cli/ferrata (sha256 {cli}) is not the binary check-build passed "
                     f"({(rec.get('binary') or {}).get('sha256')})")
    b = build_ok if build_ok is not None else (m or {}).get("build")
    if not b:
        probs.append("no BUILD-OK: the release was not made from a build whose every step passed (review-fix-supply M-2)")
    else:
        if b.get("cli_sha256") != cli:
            probs.append(f"BUILD-OK's cli_sha256 {b.get('cli_sha256')} is not cli/ferrata's {cli}")
        if b.get("check_build_sha256") != sha256(p):
            probs.append(f"BUILD-OK's check_build_sha256 is not {CHECK_BUILD}'s")
    return probs


def native_section(rec, tool, pins):
    """The native archives the CLI links, as check-build verified them after the build (its record) and the pins
    describe them; toolchains.txt's native lines must say the same."""
    out = []
    got = {x["name"]: x for x in rec.get("archives", [])}
    for a in pins["archives"]:
        r = got.get(a["name"]) or {}
        if (r.get("version"), r.get("sha256")) != (a["version"], a["sha256"]):
            sys.exit(f"make: {CHECK_BUILD} does not record {a['name']} {a['version']} with the pinned sha256: was the CLI "
                     "built and checked by scripts/release.sh build?")
        line = tool.get(f"native_{a['name']}", "").split()
        if len(line) < 3 or line[0] != a["version"] or line[2] != a["sha256"]:
            sys.exit(f"make: toolchains.txt does not record {a['name']} {a['version']} with the pinned sha256 "
                     f"(native_{a['name']}: {' '.join(line) or 'missing'})")
        out.append({"name": a["name"], "version": a["version"], "file": os.path.basename(a["url"]), "url": a["url"],
                    "bytes": a["bytes"], "sha256": a["sha256"], "linked_by": a["build_packages"],
                    "components": [{k: c[k] for k in ("name", "version", "licence", "in", "source", "texts")}
                                   for c in a["components"]]})
    return out


def notices_problems(d, m):
    """Why the release's notices do not cover the native code its manifest records (empty: they do)."""
    if not m.get("native"):
        return ["MANIFEST.json has no `native` section: the release was made before the third-party notices "
                "(review-code-ops M2), and its cli/ferrata must not be passed on as it is"]
    probs = [f"{f} missing" for f in LEGAL + (NOTICES,) if not os.path.isfile(os.path.join(d, f))]
    if NOTICES + " missing" in probs:
        return probs
    with open(os.path.join(d, NOTICES), encoding="utf-8") as f:
        text = f.read()
    for a in m["native"]:
        if f"`{a['file']}`" not in text or a["sha256"] not in text:
            probs.append(f"{NOTICES} does not name {a['file']} with its sha256 {a['sha256']}")
        for c in a["components"]:
            if c["name"] not in text or f"`{c['licence']}`" not in text:
                probs.append(f"{NOTICES} does not list {c['name']!r} under `{c['licence']}`")
            for t in c["texts"]:
                if not os.path.isfile(os.path.join(d, "licenses", t)):
                    probs.append(f"licenses/{t} missing (the licence text of {c['name']!r})")
                elif f"licenses/{t}" not in text:
                    probs.append(f"{NOTICES} does not point to licenses/{t}")
    return probs


def sources_entry(tar, pins):
    """The third-party sources tarball beside the release: its size and sha256, and every member's, measured; each
    pinned source must be in it with its pinned sha256 (review-fix-supply L-7)."""
    want = {os.path.basename(x["url"]): x["sha256"] for x in pins["sources"]}
    got = {}
    with tarfile.open(tar) as t:
        for mem in t.getmembers():
            if mem.isfile():
                h = hashlib.sha256()
                f = t.extractfile(mem)
                for chunk in iter(lambda: f.read(1 << 20), b""):
                    h.update(chunk)
                got[os.path.basename(mem.name)] = (mem.size, h.hexdigest())
    if not set(want) <= set(got):
        sys.exit(f"make: {tar} lacks {sorted(set(want) - set(got))}")
    bad = sorted(n for n in want if got[n][1] != want[n])
    if bad:
        sys.exit(f"make: {tar}: {', '.join(bad)} differ from the pinned sha256")
    return {"file": os.path.basename(tar), "bytes": os.path.getsize(tar), "sha256": sha256(tar),
            "what": "the GMP sources of the source offer (THIRD-PARTY-NOTICES.md), beside the release directory",
            "contents": [{"file": n, "bytes": got[n][0], "sha256": got[n][1]} for n in sorted(got)]}


def make(d, version, commit, sources_tar=None, build_ok=None):
    if not sources_tar:
        sys.exit("make: the third-party sources tarball (fetch_native.py sources, as release.sh package makes it) is required")
    if not build_ok or not os.path.isfile(build_ok):
        sys.exit("make: the build's BUILD-OK is required (scripts/release.sh build writes it only when every step passed)")
    b = read_kv(build_ok)
    probs = build_problems(d, None, b)
    if probs:
        sys.exit("make: refused:\n  " + "\n  ".join(probs))
    with open(os.path.join(d, CHECK_BUILD), encoding="utf-8") as f:
        rec = json.load(f)
    pins = json.load(open(PINS))
    ids = read_ids(d)
    programs = {}
    for f, what in {**LEZ_PROGRAMS, **R0BF_PROGRAMS}.items():
        rel = f"programs/{f}"
        image_id, built_sha = ids[f]
        e = entry(d, rel, what, image_id=image_id)
        if e["sha256"] != built_sha:
            sys.exit(f"make: {rel}: sha256 {e['sha256']} differs from the build's {built_sha}")
        programs[f.removesuffix(".bin")] = e
    tool = {}
    with open(os.path.join(d, "toolchains.txt")) as f:
        for line in f:
            k, _, v = line.strip().partition(" ")
            tool[k] = v
    licences = sorted(f for f in os.listdir(os.path.join(d, "licenses"))) if os.path.isdir(os.path.join(d, "licenses")) else []
    manifest = {
        "name": "ferrata",
        "version": version,
        "commit": commit,
        "created_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "status": "prototype, unaudited: for testnets only",
        "targets": {
            "lez": f"LEZ v0.4 (built against v0.4.0-rc1, commit {tool.get('lez', '?')}); testnet 0.4 when it opens",
            "evm": "any EVM chain with the Cancun opcodes; Sepolia (Glamsterdam, EIP-8037) needs FERRATA_GAS_RULE=eip8037",
        },
        "image_ids_note": "guests built through scripts/guest-rustc: their image ids do not depend on where the tree is "
        "(docs/image-ids.md; scripts/reproduce-image-ids.sh rebuilds them)",
        "licence": {
            "source": "MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE)",
            "cli/ferrata": "GPL-3.0-only as a combined work: it links GPL-3.0 and LGPL-3.0 native code statically "
                           "(THIRD-PARTY-NOTICES.md)",
        },
        "build": {"commit": b.get("commit"), "finished_utc": b.get("finished_utc"), "steps": b.get("steps"),
                  "cli_sha256": b.get("cli_sha256"), "check_build_sha256": b.get("check_build_sha256"),
                  "crate_notices_sha256": b.get("crate_notices_sha256"), "check_build": rec.get("line")},
        "native": native_section(rec, tool, pins),
        "third_party": {
            "notices": entry(d, NOTICES, "notices, licences and the source offer for the native code in cli/ferrata"),
            "crates_notices": entry(d, cn.NOTICES, "notices of the Rust crates in cli/ferrata (scripts/crate_notices.py)"),
            "legal": [entry(d, f, "Ferrata's licence") for f in LEGAL],
            "licenses": [entry(d, f"licenses/{f}", "a licence text THIRD-PARTY-NOTICES.md names") for f in licences],
            "sources": sources_entry(sources_tar, pins),
        },
        "programs": programs,
        "vault": {n: vault_entry(d, n) for n in ("FerrataVault", "FerrataRegistry")},
        "cli": entry(d, "cli/ferrata", "the ferrata command-line tool, x86_64-unknown-linux-gnu"),
        "cli_lock": entry(d, "cli/Cargo.lock", "the lockfile the CLI was built with"),
        "cli_check_build": entry(d, CHECK_BUILD, "check-build's record of the build: the binary it passed, the archives"),
        "cli_crates": entry(d, "cli/" + cn.CRATES, "the crates linked into cli/ferrata (cargo tree of the build)"),
        "deploy": {
            "script": "deploy-testnet.sh" if os.path.exists(os.path.join(d, "deploy-testnet.sh")) else None,
            "vault_script": "vault/script/deploy.sh",
        },
        "toolchains": tool,
    }
    probs = notices_problems(d, manifest) + cn.check(d)
    if probs:
        sys.exit("make: the third-party notices are incomplete:\n  " + "\n  ".join(probs))
    json.dump(manifest, open(os.path.join(d, "MANIFEST.json"), "w"), indent=1, sort_keys=False)
    print(f"MANIFEST.json: {len(programs)} programs, vault, cli (the binary check-build passed), "
          f"{len(manifest['native'])} native archives covered by {NOTICES}, the crates by {cn.NOTICES}")
    for k, e in programs.items():
        print(f"  {k:15} {e['image_id']}  sha256 {e['sha256'][:16]}…  {e['bytes']} B")


def files_of(m):
    for e in m["programs"].values():
        yield e
    for e in m["vault"].values():
        yield e
    yield m["cli"]
    for k in ("cli_lock", "cli_check_build", "cli_crates"):
        if k in m:
            yield m[k]
    tp = m.get("third_party") or {}
    for k in ("notices", "crates_notices"):
        if k in tp:
            yield tp[k]
    for e in tp.get("legal", []) + tp.get("licenses", []):
        yield e


def check(d, strict=False):
    m = json.load(open(os.path.join(d, "MANIFEST.json")))
    bad = 0
    for e in files_of(m):
        p = os.path.join(d, e["file"])
        if not os.path.exists(p):
            print(f"check: {e['file']} missing"); bad += 1; continue
        if os.path.getsize(p) != e["bytes"] or sha256(p) != e["sha256"]:
            print(f"check: {e['file']} differs from the manifest"); bad += 1
    print(f"check: {sum(1 for _ in files_of(m))} files, {bad} bad")
    probs = notices_problems(d, m)
    probs += build_problems(d, m)
    if m.get("native") and os.path.isfile(os.path.join(d, CHECK_BUILD)):
        with open(os.path.join(d, CHECK_BUILD), encoding="utf-8") as f:
            rec = json.load(f)
        if [(a["name"], a["version"], a["sha256"]) for a in rec.get("archives", [])] != \
                [(a["name"], a["version"], a["sha256"]) for a in m["native"]]:
            probs.append(f"the manifest's native archives are not the ones {CHECK_BUILD} verified")
    probs += cn.check(d)
    src = (m.get("third_party") or {}).get("sources")
    if src:
        asset = os.path.join(os.path.dirname(os.path.abspath(d)), src["file"])
        if os.path.isfile(asset):
            if os.path.getsize(asset) != src["bytes"] or sha256(asset) != src["sha256"]:
                probs.append(f"{src['file']} beside the release differs from the manifest")
            else:
                print(f"check: {src['file']} beside the release: as the manifest records it")
        elif strict:
            probs.append(f"{src['file']} is not beside the release directory: the sources asset was not checked")
        else:
            print(f"check: {src['file']} not beside the release directory: not checked")
    for p in probs:
        print(f"check: {p}")
    if not probs:
        print(f"check: cli/ferrata is the binary check-build passed; {NOTICES} covers the {len(m['native'])} native "
              f"archives and their {sum(len(a['components']) for a in m['native'])} components; {cn.NOTICES} matches "
              f"cli/{cn.CRATES} and cli/Cargo.lock")
    return 1 if bad or probs else 0


def params(d, base):
    m = json.load(open(os.path.join(d, "MANIFEST.json")))
    p = json.load(open(base)) if base else {}
    p.pop("_source", None)
    p["checkpointImageIdA"] = m["programs"]["checkpoint"]["image_id"]
    p["checkpointImageIdB"] = m["programs"]["checkpoint_b"]["image_id"]
    p["releaseImageId"] = m["programs"]["release"]["image_id"]
    p["_source"] = f"image ids from release {m['version']} ({m['commit'][:12]}); other fields from {base or 'nothing'}"
    print(json.dumps(p, indent=1))


if __name__ == "__main__":
    cmd, args = sys.argv[1], sys.argv[2:]
    if "--pins" in args:                       # tests: another scripts/native-deps.json
        i = args.index("--pins")
        PINS = args[i + 1]
        del args[i:i + 2]
    if cmd == "make":
        make(*args)
    elif cmd == "check":
        sys.exit(check(args[0], "--strict" in args[1:]))
    elif cmd == "id":
        m = json.load(open(os.path.join(args[0], "MANIFEST.json")))
        print(m["programs"][args[1].removesuffix(".bin")]["image_id"])
    elif cmd == "params":
        params(args[0], args[1] if len(args) > 1 else None)
    else:
        sys.exit(__doc__)
