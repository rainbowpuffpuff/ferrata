"""The manifest of a Ferrata release (scripts/release.sh): what each file is, its size, sha256 and image id.

    python3 -I scripts/release_manifest.py make  DIR VERSION COMMIT   write DIR/MANIFEST.json from DIR's files
    python3 -I scripts/release_manifest.py check DIR                  every file of the manifest: size and sha256
    python3 -I scripts/release_manifest.py id    DIR FILE             the image id the manifest gives programs/FILE
    python3 -I scripts/release_manifest.py params DIR [BASE.json]     vault deployment parameters with this release's
                                                                      image ids (BASE: the other fields, e.g. a chain's)
Standard library only.
"""

import datetime
import hashlib
import json
import os
import sys

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


def make(d, version, commit):
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
    manifest = {
        "name": "ferrata",
        "version": version,
        "commit": commit,
        "created_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "status": "prototype, unaudited: for testnets only",
        "targets": {
            "lez": "LEZ v0.4 (built against v0.4.0-rc1, commit 1806e87); testnet 0.4 when it opens",
            "evm": "any EVM chain with the Cancun opcodes; Sepolia (Glamsterdam, EIP-8037) needs FERRATA_GAS_RULE=eip8037",
        },
        "image_ids_note": "guests built without Docker at the crown build path; the ids depend on that path "
        "(scripts/reproduce-image-ids.sh)",
        "programs": programs,
        "vault": {n: vault_entry(d, n) for n in ("FerrataVault", "FerrataRegistry")},
        "cli": entry(d, "cli/ferrata", "the ferrata command-line tool, x86_64-unknown-linux-gnu"),
        "deploy": {
            "script": "deploy-testnet.sh" if os.path.exists(os.path.join(d, "deploy-testnet.sh")) else None,
            "vault_script": "vault/script/deploy.sh",
        },
        "toolchains": tool,
    }
    json.dump(manifest, open(os.path.join(d, "MANIFEST.json"), "w"), indent=1, sort_keys=False)
    print(f"MANIFEST.json: {len(programs)} programs, vault, cli")
    for k, e in programs.items():
        print(f"  {k:15} {e['image_id']}  sha256 {e['sha256'][:16]}…  {e['bytes']} B")


def files_of(m):
    for e in m["programs"].values():
        yield e
    for e in m["vault"].values():
        yield e
    yield m["cli"]


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
    return 1 if bad else 0


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
