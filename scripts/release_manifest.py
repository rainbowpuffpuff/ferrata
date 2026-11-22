"""The manifest of a Ferrata release (scripts/release.sh): what each file is, its size, sha256 and image id.

    python3 -I scripts/release_manifest.py make  DIR VERSION COMMIT SOURCES.tar
                                                                      write DIR/MANIFEST.json from DIR's files
    python3 -I scripts/release_manifest.py check DIR                  every file of the manifest: size and sha256;
                                                                      the third-party notices cover the native code
    python3 -I scripts/release_manifest.py id    DIR FILE             the image id the manifest gives programs/FILE
    python3 -I scripts/release_manifest.py params DIR [BASE.json]     vault deployment parameters with this release's
                                                                      image ids (BASE: the other fields, e.g. a chain's)
The CLI links native code that cargo does not build from source (scripts/native-deps.json: rapidsnark with GMP and
ffiasm, the Logos circuits with circom's runtime, circomlib and GMP). `make` records both archives (from the release's
toolchains.txt, which must carry the pinned sha256s) with their components and licences, and refuses a release whose
THIRD-PARTY-NOTICES.md does not name every archive with its sha256 and every component with its licence, or whose
licenses/ lacks a text a component needs; `check` refuses the same, and a manifest without a `native` section (a
release made before the notices: review-code-ops M2). Standard library only.
"""

import datetime
import hashlib
import json
import os
import sys
import tarfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PINS = os.path.join(ROOT, "scripts", "native-deps.json")
NOTICES = "THIRD-PARTY-NOTICES.md"
LEGAL = ("LICENSE-MIT", "LICENSE-APACHE")

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


def native_section(tool, pins):
    """The native archives the CLI links, as the build recorded them (toolchains.txt) and the pins describe them."""
    out = []
    for a in pins["archives"]:
        rec = tool.get(f"native_{a['name']}", "").split()
        if len(rec) < 3 or rec[0] != a["version"] or rec[2] != a["sha256"]:
            sys.exit(f"make: toolchains.txt does not record {a['name']} {a['version']} with the pinned sha256 "
                     f"(native_{a['name']}: {' '.join(rec) or 'missing'}); was the CLI built by scripts/release.sh build?")
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
    """The third-party sources tarball beside the release: its size and sha256, and the pinned files it holds."""
    want = {os.path.basename(x["url"]): x["sha256"] for x in pins["sources"]}
    with tarfile.open(tar) as t:
        names = {os.path.basename(n) for n in t.getnames() if t.getmember(n).isfile()}
    if not set(want) <= names:
        sys.exit(f"make: {tar} lacks {sorted(set(want) - names)}")
    return {"file": os.path.basename(tar), "bytes": os.path.getsize(tar), "sha256": sha256(tar),
            "what": "the GMP sources of the source offer (THIRD-PARTY-NOTICES.md), beside the release directory",
            "contents": [{"file": n, "sha256": h} for n, h in sorted(want.items())]}


def make(d, version, commit, sources_tar=None):
    if not sources_tar:
        sys.exit("make: the third-party sources tarball (fetch_native.py sources, as release.sh package makes it) is required")
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
        "native": native_section(tool, pins),
        "third_party": {
            "notices": entry(d, NOTICES, "notices, licences and the source offer for the third-party code in cli/ferrata"),
            "legal": [entry(d, f, "Ferrata's licence") for f in LEGAL],
            "licenses": [entry(d, f"licenses/{f}", "a licence text THIRD-PARTY-NOTICES.md names") for f in licences],
            "sources": sources_entry(sources_tar, pins),
        },
        "programs": programs,
        "vault": {n: vault_entry(d, n) for n in ("FerrataVault", "FerrataRegistry")},
        "cli": entry(d, "cli/ferrata", "the ferrata command-line tool, x86_64-unknown-linux-gnu"),
        "cli_lock": entry(d, "cli/Cargo.lock", "the lockfile the CLI was built with"),
        "deploy": {
            "script": "deploy-testnet.sh" if os.path.exists(os.path.join(d, "deploy-testnet.sh")) else None,
            "vault_script": "vault/script/deploy.sh",
        },
        "toolchains": tool,
    }
    probs = notices_problems(d, manifest)
    if probs:
        sys.exit("make: the third-party notices are incomplete:\n  " + "\n  ".join(probs))
    json.dump(manifest, open(os.path.join(d, "MANIFEST.json"), "w"), indent=1, sort_keys=False)
    print(f"MANIFEST.json: {len(programs)} programs, vault, cli, {len(manifest['native'])} native archives covered by "
          f"{NOTICES}")
    for k, e in programs.items():
        print(f"  {k:15} {e['image_id']}  sha256 {e['sha256'][:16]}…  {e['bytes']} B")


def files_of(m):
    for e in m["programs"].values():
        yield e
    for e in m["vault"].values():
        yield e
    yield m["cli"]
    if "cli_lock" in m:
        yield m["cli_lock"]
    tp = m.get("third_party") or {}
    if "notices" in tp:
        yield tp["notices"]
    for e in tp.get("legal", []) + tp.get("licenses", []):
        yield e


def check(d):
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
    for p in probs:
        print(f"check: {p}")
    if not probs:
        print(f"check: {NOTICES} covers the {len(m['native'])} native archives and their "
              f"{sum(len(a['components']) for a in m['native'])} components")
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
    if cmd == "make":
        make(*args)
    elif cmd == "check":
        sys.exit(check(args[0]))
    elif cmd == "id":
        m = json.load(open(os.path.join(args[0], "MANIFEST.json")))
        print(m["programs"][args[1].removesuffix(".bin")]["image_id"])
    elif cmd == "params":
        params(args[0], args[1] if len(args) > 1 else None)
    else:
        sys.exit(__doc__)
