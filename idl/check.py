#!/usr/bin/env python3
"""The IDL check (milestones A3, B3): decode every committed borsh vector with the IDL alone.

    python3 idl/check.py            # exit 0 when every check passes

For each program (`idl/<name>.idl.json` with `idl/vectors/<name>.vectors.json`):
1. The IDL has the shape of SPEL's `SpelIdl` (spel-framework-core 0.7.0): every key, and every
   type in SPEL's grammar (primitive, vec, option, defined, array).
2. Every instruction is the variant of `instruction_type` at the index its `discriminator` names,
   with the variant's fields as its args.
3. Every vector decodes, using only the IDL, to exactly its bytes (no byte left, none missing);
   rendered in Rust's `Debug` syntax it equals the `debug` text the Rust wire wrote beside it
   (field names, order, types, variant indices and values); a message vector decodes the same
   through the instruction table; and re-encoding the decoded value gives the same bytes.
4. Coverage: every instruction, every type and every variant of every enum is decoded at least once.
5. The IDL's errors equal the program's codes (`code::ALL`, written into the vectors file by the
   Rust test) and, for the attestation program, the core's and the step API's codes in
   `crates/ferrata-core/src/{error,steps}.rs`.
6. Strictness: each vector with a byte removed, a byte appended, or an out-of-range variant index
   is refused.
7. Mutants: copies of the IDL with one fault each (two fields swapped, a field renamed, a width
   changed, a variant removed, an Option made plain) must each fail check 3.
8. PDAs: every actor kind's account id, written by the wire's own functions
   (`vectors/pda.vectors.json`) and printed by the CLI on real runs (`vectors/pda-recorded.json`), is
   recomputed from what the IDL documents: the domain in its `docs`, the tag and the arguments of the
   account's `pda.seeds`, then LEZ v0.4's `AccountId::for_public_pda`.

The decoder follows SPEL's own (`spel-framework-core/src/decode.rs`): borsh, enums as a one-byte
variant index, `vec` and `string` with a u32 length, `option` with a 0/1 tag, arrays fixed.
Standard library only.
"""
import copy
import hashlib
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
PROGRAMS = ["ferrata_attestation", "ferrata_bridge"]

INTS = {"u8": 1, "u16": 2, "u32": 4, "u64": 8, "u128": 16, "i8": 1, "i16": 2, "i32": 4, "i64": 8, "i128": 16}
PRIMS32 = {"account_id", "AccountId", "[u8; 32]", "[u8;32]", "program_id", "ProgramId", "[u32; 8]", "[u32;8]"}
PRIMITIVES = set(INTS) | {"bool", "string"} | PRIMS32
B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


class DecodeError(Exception):
    pass


def b58(data: bytes) -> str:
    n = int.from_bytes(data, "big")
    out = ""
    while n:
        n, r = divmod(n, 58)
        out = B58[r] + out
    return "1" * (len(data) - len(data.lstrip(b"\0"))) + out


# --- 1. shape -----------------------------------------------------------------------------


def check_shape(idl):
    errs = []

    def need(obj, key, kind, where):
        if key not in obj or not isinstance(obj[key], kind):
            errs.append(f"{where}: missing or wrong {key!r}")

    for key, kind in (("version", str), ("name", str), ("instructions", list)):
        need(idl, key, kind, "idl")
    for key in ("accounts", "types", "errors"):
        if key in idl and not isinstance(idl[key], list):
            errs.append(f"idl: {key} is not a list")
    names = [t["name"] for t in idl.get("types", [])] + [a["name"] for a in idl.get("accounts", [])]
    if len(set(names)) != len(names):
        errs.append("idl: duplicate type names")
    defs = set(names)

    def check_type(t, where):
        if isinstance(t, str):
            if t not in PRIMITIVES:
                errs.append(f"{where}: unknown primitive {t!r}")
        elif isinstance(t, dict) and len(t) == 1:
            (k, inner), = t.items()
            if k in ("vec", "option"):
                check_type(inner, where)
            elif k == "defined":
                if inner not in defs:
                    errs.append(f"{where}: undefined type {inner!r}")
            elif k == "array":
                if not (isinstance(inner, list) and len(inner) == 2 and isinstance(inner[1], int) and inner[1] >= 0):
                    errs.append(f"{where}: bad array {inner!r}")
                else:
                    check_type(inner[0], where)
            else:
                errs.append(f"{where}: unknown type form {k!r}")
        else:
            errs.append(f"{where}: bad type {t!r}")

    def check_def(td, where):
        if td.get("kind") == "struct":
            for fl in td.get("fields", []):
                need(fl, "name", str, where)
                check_type(fl.get("type"), f"{where}.{fl.get('name')}")
        elif td.get("kind") == "enum":
            if not td.get("variants"):
                errs.append(f"{where}: enum without variants")
            for var in td.get("variants", []):
                need(var, "name", str, where)
                for fl in var.get("fields", []):
                    check_type(fl.get("type"), f"{where}::{var.get('name')}.{fl.get('name')}")
        else:
            errs.append(f"{where}: kind must be struct or enum")

    for td in idl.get("types", []):
        check_def(td, f"type {td.get('name')}")
    for acc in idl.get("accounts", []):
        need(acc, "name", str, "account")
        check_def(acc.get("type", {}), f"account {acc.get('name')}")
    for ins in idl["instructions"]:
        w = f"instruction {ins.get('name')}"
        need(ins, "name", str, w)
        need(ins, "accounts", list, w)
        need(ins, "args", list, w)
        for a in ins.get("accounts", []):
            need(a, "name", str, w)
            for flag in ("writable", "signer", "init"):
                if flag in a and not isinstance(a[flag], bool):
                    errs.append(f"{w}: account {a.get('name')}: {flag} not a bool")
            for s in a.get("pda", {}).get("seeds", []) if a.get("pda") else []:
                if s.get("kind") not in ("const", "account", "arg"):
                    errs.append(f"{w}: seed kind {s.get('kind')!r}")
        for a in ins.get("args", []):
            check_type(a.get("type"), f"{w}.{a.get('name')}")
        disc = ins.get("discriminator")
        if disc is not None and not (isinstance(disc, list) and all(isinstance(b, int) and 0 <= b < 256 for b in disc)):
            errs.append(f"{w}: discriminator must be bytes")
        ex = ins.get("execution")
        if ex is not None and not all(isinstance(ex.get(k, False), bool) for k in ("public", "private_owned")):
            errs.append(f"{w}: execution flags")
    codes = [e.get("code") for e in idl.get("errors", [])]
    enames = [e.get("name") for e in idl.get("errors", [])]
    if len(set(codes)) != len(codes) or len(set(enames)) != len(enames):
        errs.append("errors: duplicate code or name")
    if not all(isinstance(c, int) and 0 <= c < 256 for c in codes):
        errs.append("errors: codes must be one byte (a LEZ exit code)")
    return errs


# --- 2. instructions against instruction_type -------------------------------------------------


def typedefs(idl):
    out = {t["name"]: t for t in idl.get("types", [])}
    for a in idl.get("accounts", []):
        out[a["name"]] = a["type"]
    return out


def check_instructions(idl):
    errs = []
    defs = typedefs(idl)
    enum_name = idl.get("instruction_type", "").split("::")[-1]
    en = defs.get(enum_name)
    if not en or en.get("kind") != "enum":
        return [f"instruction_type {idl.get('instruction_type')!r} is not an enum of the IDL"]
    variants = en["variants"]
    seen = set()
    for ins in idl["instructions"]:
        disc = ins.get("discriminator")
        if not disc or len(disc) != 1 or disc[0] >= len(variants):
            errs.append(f"{ins['name']}: discriminator {disc} is not a variant index")
            continue
        var = variants[disc[0]]
        if ins.get("variant") != var["name"]:
            errs.append(f"{ins['name']}: variant {ins.get('variant')} but index {disc[0]} is {var['name']}")
        if ins["args"] != var.get("fields", []):
            errs.append(f"{ins['name']}: args differ from {var['name']}'s fields")
        seen.add(disc[0])
    if seen != set(range(len(variants))):
        errs.append(f"variants without an instruction: {sorted(set(range(len(variants))) - seen)}")
    return errs


# --- 3. decode, render, encode --------------------------------------------------------------


class Decoder:
    def __init__(self, idl):
        self.defs = typedefs(idl)
        self.used_types = set()
        self.used_variants = set()

    def decode(self, data: bytes, type_name: str):
        self.buf, self.pos = data, 0
        value = self.defined(type_name)
        if self.pos != len(self.buf):
            raise DecodeError(f"{len(self.buf) - self.pos} byte(s) left after {type_name}")
        return value

    def take(self, n):
        if self.pos + n > len(self.buf):
            raise DecodeError(f"needs {n} byte(s) at {self.pos}, {len(self.buf) - self.pos} left")
        out = self.buf[self.pos : self.pos + n]
        self.pos += n
        return out

    def u(self, n):
        return int.from_bytes(self.take(n), "little")

    def defined(self, name):
        td = self.defs.get(name)
        if td is None:
            raise DecodeError(f"undefined type {name}")
        self.used_types.add(name)
        if td["kind"] == "struct":
            fields = [(fl["name"], self.value(fl["type"])) for fl in td.get("fields", [])]
            return ("struct", name, fields, td.get("x-rust") == "tuple")
        idx = self.u(1)
        if idx >= len(td["variants"]):
            raise DecodeError(f"{name}: variant index {idx} of {len(td['variants'])}")
        var = td["variants"][idx]
        self.used_variants.add((name, var["name"]))
        fields = [(fl["name"], self.value(fl["type"])) for fl in var.get("fields", [])]
        return ("enum", name, var["name"], fields)

    def value(self, t):
        if isinstance(t, str):
            if t in INTS:
                n = self.u(INTS[t])
                if t.startswith("i") and n >= 1 << (8 * INTS[t] - 1):
                    n -= 1 << (8 * INTS[t])
                return ("int", t, n)
            if t == "bool":
                b = self.u(1)
                if b > 1:
                    raise DecodeError(f"bool byte {b}")
                return ("bool", b == 1)
            if t == "string":
                return ("string", self.take(self.u(4)).decode("utf-8"))
            if t in PRIMS32:
                return ("acct" if "ccount" in t else "bytes32", t, self.take(32))
            raise DecodeError(f"unknown primitive {t}")
        (k, inner), = t.items()
        if k == "vec":
            return ("list", "vec", inner, [self.value(inner) for _ in range(self.u(4))])
        if k == "array":
            return ("list", "array", inner[0], [self.value(inner[0]) for _ in range(inner[1])])
        if k == "option":
            tag = self.u(1)
            if tag > 1:
                raise DecodeError(f"option tag {tag}")
            return ("opt", self.value(inner) if tag else None)
        if k == "defined":
            return self.defined(inner)
        raise DecodeError(f"type form {k}")


def debug(v) -> str:
    """Rust's `{:?}` of the decoded value (derive(Debug); AccountId prints base58)."""
    kind = v[0]
    if kind == "int":
        return str(v[2])
    if kind == "bool":
        return "true" if v[1] else "false"
    if kind == "string":
        return json.dumps(v[1])
    if kind == "acct":
        return b58(v[2])
    if kind == "bytes32":
        return "[" + ", ".join(str(b) for b in v[2]) + "]"
    if kind == "list":
        return "[" + ", ".join(debug(x) for x in v[3]) + "]"
    if kind == "opt":
        return "None" if v[1] is None else f"Some({debug(v[1])})"
    if kind == "struct":
        _, name, fields, tuple_ = v
        if tuple_:
            return "(" + ", ".join(debug(x) for _, x in fields) + ")"
        return name + (" { " + ", ".join(f"{n}: {debug(x)}" for n, x in fields) + " }" if fields else "")
    if kind == "enum":
        _, _, var, fields = v
        if not fields:
            return var
        if all(n.isdigit() for n, _ in fields):
            return var + "(" + ", ".join(debug(x) for _, x in fields) + ")"
        return var + " { " + ", ".join(f"{n}: {debug(x)}" for n, x in fields) + " }"
    raise ValueError(kind)


def encode(v, defs) -> bytes:
    kind = v[0]
    if kind == "int":
        return (v[2] % (1 << (8 * INTS[v[1]]))).to_bytes(INTS[v[1]], "little")
    if kind == "bool":
        return bytes([1 if v[1] else 0])
    if kind == "string":
        b = v[1].encode()
        return len(b).to_bytes(4, "little") + b
    if kind in ("acct", "bytes32"):
        return v[2]
    if kind == "list":
        body = b"".join(encode(x, defs) for x in v[3])
        return (len(v[3]).to_bytes(4, "little") if v[1] == "vec" else b"") + body
    if kind == "opt":
        return b"\0" if v[1] is None else b"\1" + encode(v[1], defs)
    if kind == "struct":
        return b"".join(encode(x, defs) for _, x in v[2])
    if kind == "enum":
        names = [var["name"] for var in defs[v[1]]["variants"]]
        return bytes([names.index(v[2])]) + b"".join(encode(x, defs) for _, x in v[3])
    raise ValueError(kind)


def decode_instruction(idl, data: bytes):
    """The instruction-table path: discriminator, then the args in order."""
    dec = Decoder(idl)
    dec.buf, dec.pos = data, 0
    for ins in idl["instructions"]:
        d = bytes(ins.get("discriminator") or b"")
        if d and data.startswith(d):
            dec.pos = len(d)
            args = [(a["name"], dec.value(a["type"])) for a in ins["args"]]
            if dec.pos != len(data):
                raise DecodeError(f"{ins['name']}: {len(data) - dec.pos} byte(s) left")
            return ins, args
    raise DecodeError("no instruction has this discriminator")


def check_vectors(idl, vectors, strict=True):
    """Checks 3, 4 and 6. Returns (errors, stats)."""
    errs = []
    dec = Decoder(idl)
    defs = typedefs(idl)
    enum_name = idl["instruction_type"].split("::")[-1]
    used_instructions = set()
    for vec_ in vectors:
        data = bytes.fromhex(vec_["hex"])
        try:
            value = dec.decode(data, vec_["type"])
        except (DecodeError, UnicodeDecodeError) as e:
            errs.append(f"{vec_['name']}: {e}")
            continue
        text = debug(value)
        if text != vec_["debug"]:
            i = next((k for k in range(min(len(text), len(vec_["debug"]))) if text[k] != vec_["debug"][k]), min(len(text), len(vec_["debug"])))
            errs.append(f"{vec_['name']}: Debug differs at {i}: IDL {text[max(0, i - 40):i + 40]!r} vs Rust {vec_['debug'][max(0, i - 40):i + 40]!r}")
            continue
        if encode(value, defs) != data:
            errs.append(f"{vec_['name']}: re-encoding differs")
        if vec_["type"] == enum_name:
            try:
                ins, args = decode_instruction(idl, data)
                if ins["variant"] != value[2] or [debug(x) for _, x in args] != [debug(x) for _, x in value[3]]:
                    errs.append(f"{vec_['name']}: instruction table decodes {ins['name']} differently")
                used_instructions.add(ins["name"])
            except DecodeError as e:
                errs.append(f"{vec_['name']}: instruction table: {e}")
        if strict:
            for label, mutated in (("a byte removed", data[:-1]), ("a byte appended", data + b"\0")):
                try:
                    Decoder(idl).decode(mutated, vec_["type"])
                    errs.append(f"{vec_['name']}: decoded with {label}")
                except (DecodeError, UnicodeDecodeError):
                    pass
            td = defs[vec_["type"]]
            if td["kind"] == "enum":
                try:
                    Decoder(idl).decode(bytes([len(td["variants"])]) + data[1:], vec_["type"])
                    errs.append(f"{vec_['name']}: decoded with an out-of-range variant index")
                except DecodeError:
                    pass
    if strict:
        all_types = set(defs)
        if dec.used_types != all_types:
            errs.append(f"types never decoded: {sorted(all_types - dec.used_types)}")
        all_variants = {(n, var["name"]) for n, td in defs.items() if td["kind"] == "enum" for var in td["variants"]}
        if dec.used_variants != all_variants:
            errs.append(f"variants never decoded: {sorted(all_variants - dec.used_variants)}")
        missing = {i["name"] for i in idl["instructions"]} - used_instructions
        if missing:
            errs.append(f"instructions never decoded: {sorted(missing)}")
    stats = {"vectors": len(vectors), "types": len(dec.used_types), "variants": len(dec.used_variants), "instructions": len(used_instructions)}
    return errs, stats


# --- 5. errors ------------------------------------------------------------------------------


def check_errors(name, idl, vfile):
    errs = []
    idl_codes = {e["code"]: e["name"] for e in idl.get("errors", [])}
    for code_name, code in vfile["program_codes"]:
        if idl_codes.get(code) != code_name:
            errs.append(f"program code {code} {code_name}: IDL has {idl_codes.get(code)!r}")
    if name == "ferrata_attestation":
        sys.path.insert(0, str(HERE / "tools"))
        import build_idl  # the same parser that wrote the list

        expected = {e["code"]: e["name"] for e in build_idl.core_errors() + build_idl.step_errors()}
        expected.update({c: n for n, c in vfile["program_codes"]})
        if expected != idl_codes:
            errs.append(f"errors differ from the sources: {sorted(set(expected.items()) ^ set(idl_codes.items()))[:6]}")
    else:
        if set(idl_codes) != {c for _, c in vfile["program_codes"]}:
            errs.append("errors: codes beyond the program's")
    return errs


# --- 8. PDAs --------------------------------------------------------------------------------

# LEZ v0.4.0-rc1, lee/state_machine/core/src/program/mod.rs, `AccountId::for_public_pda`:
# SHA-256(prefix || program account id || seed), the prefix padded to 32 bytes.
LEZ_PDA_PREFIX = b"/LEE/v0.2/AccountId/PDA/" + bytes(8)


def b58decode(text: str) -> bytes:
    n = 0
    for ch in text:
        n = n * 58 + B58.index(ch)
    body = n.to_bytes((n.bit_length() + 7) // 8, "big") if n else b""
    return bytes(len(text) - len(text.lstrip("1"))) + body


def encode_input(inp) -> bytes:
    enc, value = inp["encoding"], inp["value"]
    if enc == "bytes":
        return bytes.fromhex(value)
    if enc == "u64le":
        return int(value).to_bytes(8, "little")
    if enc == "u256be":
        return int(value).to_bytes(32, "big")
    raise ValueError(enc)


def check_pdas(idls):
    """Check 8. Returns (errors, number checked)."""
    errs, n = [], 0
    files = [HERE / "vectors" / "pda.vectors.json", HERE / "vectors" / "pda-recorded.json"]
    for path in files:
        for e in json.loads(path.read_text())["pdas"]:
            n += 1
            idl = idls[e["program"]]
            where = f"{path.name}: {e['program']} {e['actor']}"
            domains = {m for d in idl.get("docs", []) for m in re.findall(r'SHA-256\("(ferrata-lez/v1/[a-z/]*)" \|\| tag', d)}
            if len(domains) != 1:
                errs.append(f"{where}: the IDL's docs name {len(domains)} PDA domains")
                continue
            seeds = next(
                (a["pda"]["seeds"] for ins in idl["instructions"] for a in ins["accounts"] if a["name"] == e["actor"] and a.get("pda")),
                None,
            )
            if seeds is None:
                errs.append(f"{where}: no account of that name with a pda in the IDL")
                continue
            if seeds[0]["kind"] != "const" or any(sd["kind"] != "arg" for sd in seeds[1:]):
                errs.append(f"{where}: seeds are not a tag and arguments")
                continue
            names = [sd["path"].split(" ")[0] for sd in seeds[1:]]
            if names != [i["name"] for i in e["inputs"]]:
                errs.append(f"{where}: the IDL's seed arguments {names} are not the wire's {[i['name'] for i in e['inputs']]}")
                continue
            seed = hashlib.sha256(domains.pop().encode() + seeds[0]["value"].encode() + b"".join(encode_input(i) for i in e["inputs"])).digest()
            program = bytes.fromhex(e["program_id"]) if "program_id" in e else b58decode(e["program_id_base58"])
            account = hashlib.sha256(LEZ_PDA_PREFIX + program + seed).digest()
            want = bytes.fromhex(e["account"]) if "account" in e else b58decode(e["account_base58"])
            if account != want:
                errs.append(f"{where}: derived {account.hex()} but the account is {want.hex()}")
    return errs, n


# --- 7. mutants -----------------------------------------------------------------------------


def mutants(idl):
    """IDL copies with one fault each, every one of which must fail the vectors."""
    out = []
    defs_list = idl["types"] + [a["type"] for a in idl.get("accounts", [])]

    def find(pred):
        for td in defs_list:
            if pred(td):
                return td["name"]
        return None

    def apply(desc, fn):
        m = copy.deepcopy(idl)
        tds = {t["name"]: t for t in m["types"] + [a["type"] for a in m.get("accounts", [])]}
        fn(tds, m)
        out.append((desc, m))

    # two adjacent fields of the same type swapped
    for td in defs_list:
        fl = td.get("fields", [])
        pair = next((i for i in range(len(fl) - 1) if fl[i]["type"] == fl[i + 1]["type"] and not fl[i]["name"].isdigit()), None)
        if pair is not None:
            apply(f"{td['name']}: fields {fl[pair]['name']} and {fl[pair + 1]['name']} swapped",
                  lambda t, m, n=td["name"], i=pair: t[n]["fields"].__setitem__(slice(i, i + 2), [t[n]["fields"][i + 1], t[n]["fields"][i]]))
            break
    first_struct = find(lambda td: td["kind"] == "struct" and td.get("fields") and not td["fields"][0]["name"].isdigit())
    apply(f"{first_struct}: first field renamed", lambda t, m: t[first_struct]["fields"][0].__setitem__("name", "renamed"))
    u64_struct = find(lambda td: td["kind"] == "struct" and any(f["type"] == "u64" for f in td.get("fields", [])))

    def narrow(t, m):
        fl = next(f for f in t[u64_struct]["fields"] if f["type"] == "u64")
        fl["type"] = "u32"

    apply(f"{u64_struct}: a u64 field read as u32", narrow)
    enum_name = idl["instruction_type"].split("::")[-1]
    apply(f"{enum_name}: first variant removed", lambda t, m: t[enum_name]["variants"].pop(0))
    opt_struct = find(lambda td: td["kind"] == "struct" and any(isinstance(f["type"], dict) and "option" in f["type"] for f in td.get("fields", [])))

    def unwrap(t, m):
        fl = next(f for f in t[opt_struct]["fields"] if isinstance(f["type"], dict) and "option" in f["type"])
        fl["type"] = fl["type"]["option"]

    if opt_struct:
        apply(f"{opt_struct}: an Option read as plain", unwrap)
    return out


def main():
    failures = 0
    idls = {}
    for name in PROGRAMS:
        idl = json.loads((HERE / f"{name}.idl.json").read_text())
        idls[name] = idl
        vfile = json.loads((HERE / "vectors" / f"{name}.vectors.json").read_text())
        results = []
        errs = check_shape(idl)
        results.append(("1 SPEL shape", errs, ""))
        errs = check_instructions(idl) if not errs else ["skipped (shape)"]
        results.append(("2 instructions = instruction_type's variants", errs, f"{len(idl['instructions'])} instructions"))
        verrs, stats = check_vectors(idl, vfile["vectors"])
        results.append(("3/4/6 vectors decoded by the IDL alone, coverage, strictness", verrs,
                        f"{stats['vectors']} vectors, {stats['instructions']} instructions, {stats['types']} types, {stats['variants']} enum variants"))
        results.append(("5 errors = the program's codes", check_errors(name, idl, vfile), f"{len(idl.get('errors', []))} codes"))
        merrs = []
        ms = mutants(idl)
        for desc, m in ms:
            e, _ = check_vectors(m, vfile["vectors"], strict=False)
            if not e:
                merrs.append(f"mutant passed: {desc}")
        results.append(("7 IDL mutants rejected", merrs, f"{len(ms) - len(merrs)} of {len(ms)}"))
        print(f"== {name} ({vfile['program']}, {len(vfile['vectors'])} vectors)")
        for label, errs, note in results:
            print(f"  {'ok  ' if not errs else 'FAIL'} {label}{': ' + note if note else ''}")
            for e in errs[:12]:
                print(f"       {e}")
            failures += bool(errs)
    perrs, pn = check_pdas(idls)
    print(f"== PDAs ({pn} accounts: every actor kind from the wire, and real ones from the CLI's runs)")
    print(f"  {'ok  ' if not perrs else 'FAIL'} 8 derived from the IDL's documented seeds and LEZ's for_public_pda")
    for e in perrs[:12]:
        print(f"       {e}")
    failures += bool(perrs)
    print("IDL check:", "passed" if not failures else f"{failures} check(s) failed")
    return 1 if failures else 0


def decode_file(program, type_name, hex_path, debug_path=None):
    """`--decode`: decode one borsh value (hex in a file) with the IDL alone and print its Debug
    text; with a Debug file (as `ferrata-attest --message-out` writes), require the same text."""
    idl = json.loads((HERE / f"{program}.idl.json").read_text())
    data = bytes.fromhex(Path(hex_path).read_text().strip())
    value = Decoder(idl).decode(data, type_name)
    text = debug(value)
    head = value[2] if value[0] == "enum" else type_name
    print(f"{hex_path}: {len(data)} bytes decode as {type_name}::{head} with {program}.idl.json alone")
    if debug_path is None:
        print(text)
        return 0
    want = Path(debug_path).read_text().strip()
    same = text == want
    print(f"  equal to the Rust Debug text in {debug_path}: {'yes' if same else 'NO'} ({len(text)} characters)")
    return 0 if same else 1


def check_recorded(path):
    """`--recorded`: the attestation program's messages taken from recorded LEZ blocks
    (`lez/live/driver/src/bin/idl-recorded.rs`), each decoded with the IDL alone and compared with
    the Rust Debug text written beside it."""
    vfile = json.loads(Path(path).read_text())
    idl = json.loads((HERE / f"{vfile['program']}.idl.json").read_text())
    counts, failures, nbytes = {}, [], 0
    for vec_ in vfile["vectors"]:
        data = bytes.fromhex(vec_["hex"])
        nbytes += len(data)
        try:
            value = Decoder(idl).decode(data, vec_["type"])
        except (DecodeError, UnicodeDecodeError) as e:
            failures.append(f"{vec_['name']}: {e}")
            continue
        if debug(value) != vec_["debug"]:
            failures.append(f"{vec_['name']}: Debug text differs")
            continue
        counts[value[2]] = counts.get(value[2], 0) + 1
    for source in vfile.get("sources", []):
        print(f"  source: {source}")
    print(f"{path}: {len(vfile['vectors'])} recorded messages, {nbytes} bytes; decoded with {vfile['program']}.idl.json alone "
          f"and equal to their Rust Debug text: {sum(counts.values())} ({', '.join(f'{k} {v}' for k, v in sorted(counts.items()))})")
    for f in failures[:12]:
        print(f"  FAIL {f}")
    return 1 if failures or not counts else 0


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--recorded":
        sys.exit(check_recorded(sys.argv[2]))
    if len(sys.argv) > 1 and sys.argv[1] == "--decode":
        if len(sys.argv) not in (5, 6):
            sys.exit("usage: check.py --decode PROGRAM TYPE HEXFILE [DEBUGFILE]")
        sys.exit(decode_file(*sys.argv[2:]))
    sys.exit(main())
