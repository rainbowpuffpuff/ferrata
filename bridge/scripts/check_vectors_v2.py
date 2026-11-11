#!/usr/bin/env python3
"""Independent check of vectors/*-v2.json (SPEC-v2): recomputes every output from its inputs with the
Python standard library (hashlib, hmac) and the layouts written in SPEC-v2 / the README, sharing no code
with the Rust crate. Run: python3 -I scripts/check_vectors_v2.py [vectors-dir]"""

import hashlib
import hmac
import json
import pathlib
import sys

checks = 0


def b(x):
    return bytes.fromhex(x[2:] if x.startswith("0x") else x)


def sha(data):
    return hashlib.sha256(data).digest()


def prf(k, msg):
    return hmac.new(k, msg, hashlib.sha256).digest()


def u64(n):
    return int(n).to_bytes(8, "big")


def u32(n):
    return int(n).to_bytes(4, "big")


def word(n):
    return int(n).to_bytes(32, "big")


def addr_word(a):
    return bytes(12) + a


def eq(label, got, want):
    global checks
    checks += 1
    if got != want:
        sys.exit(f"MISMATCH {label}: computed {got.hex()} file {want.hex()}")


LEAF_TAG = sha(b"ferrata/vault/leaf/v2")
BURN_TAG = sha(b"ferrata/burn/leaf/v2")
RELEASE_DOMAIN = sha(b"ferrata/vault/release/v2")
CHECKPOINT_DOMAIN = sha(b"ferrata/vault/checkpoint/v2")


def keys(seed):
    return prf(seed, b"ferrata/auth/v1"), prf(seed, b"ferrata/view/v1")


def config_id(c):
    pre = (b"ferrata/config/v1" + b(c["bedrock_genesis_id"]) + b(c["channel_id"]) + b(c["bridge_header"])
           + b(c["bridge_image_id"]) + b(c["evm_chain_id"]) + b(c["vault"]) + b(c["attestation_root_id"]) + b(c["salt"]))
    return pre, sha(pre)


def deposit_leaf(index, depositor, token, amount, c):
    return sha(b"\x00" + LEAF_TAG + u64(index) + depositor + token + amount + c)


def burn_leaf(cfg, index, token, amount, c_out, r):
    return sha(b"\x02" + BURN_TAG + cfg + u64(index) + token + amount + c_out + r)


def node(left, right):
    return sha(b"\x01" + left + right)


def zero_hashes():
    z = [bytes(32)]
    for _ in range(32):
        z.append(node(z[-1], z[-1]))
    return z


def root_of(leaves):
    z = zero_hashes()
    level = list(leaves)
    for h in range(32):
        level = [node(level[i], level[i + 1] if i + 1 < len(level) else z[h]) for i in range(0, len(level), 2)]
    return level[0] if level else z[32]


def root_from_path(lf, index, path):
    assert path["directions"] == index, "direction bits must equal the leaf index"
    for h, sib in enumerate(path["siblings"]):
        sib = b(sib)
        lf = node(lf, sib) if (index >> h) & 1 == 0 else node(sib, lf)
    return lf


def frontier(leaves):
    """Canonical frontier: f[h] = root of the completed left-edge subtree of height h if bit h of the
    count is 1, else zero."""
    n, z, f, start = len(leaves), zero_hashes(), [bytes(32)] * 32, 0
    for h in range(31, -1, -1):
        if (n >> h) & 1:
            sub = leaves[start:start + (1 << h)]
            level = list(sub)
            for k in range(h):
                level = [node(level[i], level[i + 1]) for i in range(0, len(level), 2)]
            f[h] = level[0]
            start += 1 << h
    return f


def frontier_hash(f):
    return sha(b"ferrata/burn-frontier/v1" + b"".join(f))


def key_set_hash(ks):
    return sha(b"ferrata/keyset/v1" + u32(len(ks)) + b"".join(ks))


def check_tags(v):
    for t in v["hashed"]:
        eq("tag " + t["tag"], sha(t["tag"].encode()), b(t["sha256"]))
    for t in v["prefix"]:
        eq("prefix " + t["tag"], t["tag"].encode(), b(t["bytes"]))


def check_keys_config(v):
    for k in v["keys"]:
        ka, kv = keys(b(k["bridge_seed"]))
        eq("kAuth", ka, b(k["k_auth"]))
        eq("kView", kv, b(k["k_view"]))
    for c in v["configs"]:
        pre, cid = config_id(c)
        eq("configId preimage", pre, b(c["preimage"]))
        eq("configId", cid, b(c["config_id"]))


def check_deposit_tree(v):
    z = zero_hashes()
    for h, zh in enumerate(v["zero_hashes"]):
        eq(f"zero[{h}]", z[h], b(zh))
    leaves = []
    for e in v["leaves"]:
        pre = b"\x00" + LEAF_TAG + u64(e["leaf_index"]) + b(e["depositor"]) + b(e["token"]) + b(e["amount"]) + b(e["commitment"])
        eq("deposit leaf preimage", pre, b(e["preimage"]))
        eq("deposit leaf", sha(pre), b(e["leaf"]))
        leaves.append(sha(pre))
        eq("deposit root after", root_of(leaves), b(e["root_after"]))
    eq("deposit final root", root_of(leaves), b(v["final_root"]))
    for i, p in enumerate(v["paths"]):
        eq("deposit path", root_from_path(leaves[i], i, p), b(v["final_root"]))


def check_burn_tree(v):
    cfg = b(v["config_id"])
    leaves = []
    for e in v["leaves"]:
        pre = b"\x02" + BURN_TAG + cfg + u64(e["burn_index"]) + b(e["token"]) + b(e["amount"]) + b(e["C_out"]) + b(e["r"])
        eq("burn leaf preimage", pre, b(e["preimage"]))
        eq("burn leaf", sha(pre), b(e["leaf"]))
        leaves.append(sha(pre))
        eq("burn root after", root_of(leaves), b(e["root_after"]))
        f = frontier(leaves)
        for h in range(32):
            eq("frontier entry", f[h], b(e["frontier_after"][h]))
        eq("frontier hash", frontier_hash(f), b(e["frontier_hash_after"]))
    eq("burn final root", root_of(leaves), b(v["final_root"]))
    for i, p in enumerate(v["paths"]):
        eq("burn path", root_from_path(leaves[i], i, p), b(v["final_root"]))
    return leaves


def check_checkpoint(v, burn_leaves):
    for k in v["keysets"]:
        eq("keySetHash", key_set_hash([b(x) for x in k["keys"]]), b(k["key_set_hash"]))
    # State 0: empty burn tree; state 1: the 8-leaf burn tree of burn-tree-v2.json.
    eq("state 0 frontier hash", frontier_hash(frontier([])), b(v["states"][0]["burn_frontier_hash"]))
    eq("state 1 frontier hash", frontier_hash(frontier(burn_leaves)), b(v["states"][1]["burn_frontier_hash"]))
    for s in v["states"]:
        pre = (b"ferrata/lez-state/v1" + u64(s["tip_id"]) + b(s["tip_hash"]) + b(s["key_set_hash"]) + b(s["config_tip_hash"])
               + u64(s["burn_count"]) + b(s["burn_frontier_hash"]) + u64(s["imported_leaf_count"]) + bytes([1 if s["sunset"] else 0]))
        eq("S preimage", pre, b(s["preimage"]))
        eq("S", sha(pre), b(s["S"]))
    assert v["count"] == len(v["journals"])
    for j in v["journals"]:
        bs = (CHECKPOINT_DOMAIN + b(j["chainId"]) + addr_word(b(j["vault"])) + b(j["lezConfigId"]) + word(j["nMin"]) + word(j["tMin"])
              + b(j["parentState"]) + b(j["newState"]) + b(j["burnRoot"]) + word(j["burnCount"]) + word(j["importedLeafCount"])
              + word(1 if j["sunset"] else 0) + b(j["echoNonce"]))
        eq("checkpoint journal", bs, b(j["journal"]))
        eq("checkpoint journal digest", sha(bs), b(j["digest"]))


def check_inbound(v):
    for c in v["cases"]:
        ka, _ = keys(b(c["bridge_seed"]))
        eq("kAuth", ka, b(c["k_auth"]))
        chain, vault, dep = b(c["evm_chain_id"]), b(c["vault"]), b(c["depositor"])
        msg = b"ferrata/s/v1" + chain + vault + dep + u64(c["j"])
        eq("s message", msg, b(c["s_message"]))
        s = prf(ka, msg)
        eq("s", s, b(c["s"]))
        cpre = b"ferrata/C/v1" + s + bytes([c["kind"]]) + b(c["dest"])
        eq("C preimage", cpre, b(c["C_preimage"]))
        eq("C", sha(cpre), b(c["C"]))
        lf = deposit_leaf(c["leaf_index"], dep, b(c["token"]), b(c["amount"]), sha(cpre))
        eq("leaf", lf, b(c["leaf"]))
        fpre = b"ferrata/fact/acc-leaf/v1" + chain + vault + bytes(32) + u64(c["leaf_index"]) + lf
        eq("fid preimage", fpre, b(c["fid_preimage"]))
        eq("fid", sha(fpre), b(c["fid"]))
        npre = b"ferrata/nf/v1" + s + sha(fpre)
        eq("N preimage", npre, b(c["N_preimage"]))
        eq("N", sha(npre), b(c["N"]))
    c = v["claim"]
    ka, _ = keys(b(c["bridge_seed"]))
    chain, vault, dep, token, amount = b(c["evm_chain_id"]), b(c["vault"]), b(c["depositor"]), b(c["token"]), b(c["amount"])
    s = prf(ka, b"ferrata/s/v1" + chain + vault + dep + u64(c["j"]))
    eq("claim s", s, b(c["s"]))
    lf = deposit_leaf(c["leaf_index"], dep, token, amount, sha(b"ferrata/C/v1" + s + bytes([c["kind"]]) + b(c["dest"])))
    leaves = [b(x) for x in c["leaves"]]
    eq("claim leaf in tree", leaves[c["leaf_index"]], lf)
    eq("claim root", root_of(leaves), b(c["root"]))
    eq("claim path", root_from_path(lf, c["leaf_index"], c["path"]), b(c["root"]))
    n = sha(b"ferrata/nf/v1" + s + sha(b"ferrata/fact/acc-leaf/v1" + chain + vault + bytes(32) + u64(c["leaf_index"]) + lf))
    eq("claim N", n, b(c["N"]))
    p = c["path"]
    gi = (b(c["root"]) + chain + vault + dep + token + amount + u64(c["leaf_index"]) + u32(p["directions"])
          + b"".join(b(x) for x in p["siblings"]) + s + bytes([c["kind"], 0, 0, 0]) + b(c["dest"]))
    eq("claim guest input", gi, b(c["guest_input"]))
    eq("claim guest journal", b(c["root"]) + chain + vault + n + token + amount, b(c["guest_journal"]))


def check_outbound(v):
    for c in v["cases"]:
        ka, kv = keys(b(c["bridge_seed"]))
        eq("kAuth", ka, b(c["k_auth"]))
        eq("kView", kv, b(c["k_view"]))
        cfg, chain, vault = b(c["config_id"]), b(c["evm_chain_id"]), b(c["vault"])
        rmsg = b"ferrata/burn-tag/v1" + cfg + u64(c["i"])
        eq("r message", rmsg, b(c["r_message"]))
        r = prf(kv, rmsg)
        eq("r", r, b(c["r"]))
        smsg = b"ferrata/s-out/v1" + cfg + r
        eq("sOut message", smsg, b(c["s_out_message"]))
        s_out = prf(ka, smsg)
        eq("sOut", s_out, b(c["s_out"]))
        eq("memo message", b"ferrata/memo/v1" + r, b(c["memo_message"]))
        pad = prf(kv, b"ferrata/memo/v1" + r)[:20]
        eq("memo pad", pad, b(c["memo_pad"]))
        eq("memo", bytes(x ^ y for x, y in zip(b(c["recipient"]), pad)), b(c["memo"]))
        cpre = b"ferrata/Cout/v1" + s_out + chain + vault + b(c["recipient"])
        eq("COut preimage", cpre, b(c["C_out_preimage"]))
        c_out = sha(cpre)
        eq("COut", c_out, b(c["C_out"]))
        mmsg = b"ferrata/burn-mac/v2" + r + b(c["memo"]) + b(c["token"]) + b(c["amount"]) + c_out  # D12
        eq("mac message", mmsg, b(c["mac_message"]))
        eq("mac", prf(kv, mmsg)[:16], b(c["mac"]))
        lpre = b"\x02" + BURN_TAG + cfg + u64(c["burn_index"]) + b(c["token"]) + b(c["amount"]) + c_out + r
        eq("burn leaf preimage", lpre, b(c["burn_leaf_preimage"]))
        lf = sha(lpre)
        eq("burn leaf", lf, b(c["burn_leaf"]))
        bpre = b"ferrata/burn/v1" + cfg + u64(c["burn_index"]) + lf
        eq("burnId preimage", bpre, b(c["burn_id_preimage"]))
        eq("burnId", sha(bpre), b(c["burn_id"]))
        npre = b"ferrata/nf-out/v2" + s_out  # D13: no burnId, so no burn index
        eq("NOut preimage", npre, b(c["N_out_preimage"]))
        eq("NOut", sha(npre), b(c["N_out"]))
    c = v["release"]
    ka, kv = keys(b(c["bridge_seed"]))
    cfg, chain, vault, token, amount = b(c["config_id"]), b(c["evm_chain_id"]), b(c["vault"]), b(c["token"]), b(c["amount"])
    r = prf(kv, b"ferrata/burn-tag/v1" + cfg + u64(c["i"]))
    eq("release r", r, b(c["r"]))
    s_out = prf(ka, b"ferrata/s-out/v1" + cfg + r)
    eq("release sOut", s_out, b(c["s_out"]))
    recipient = bytes(x ^ y for x, y in zip(b(c["memo"]), prf(kv, b"ferrata/memo/v1" + r)[:20]))
    eq("release recipient from memo", recipient, b(c["recipient"]))
    c_out = sha(b"ferrata/Cout/v1" + s_out + chain + vault + recipient)
    eq("release COut", c_out, b(c["C_out"]))
    eq("release mac", prf(kv, b"ferrata/burn-mac/v2" + r + b(c["memo"]) + token + amount + c_out)[:16], b(c["mac"]))
    lf = burn_leaf(cfg, c["burn_index"], token, amount, c_out, r)
    leaves = [b(x) for x in c["leaves"]]
    eq("release leaf in tree", leaves[c["burn_index"]], lf)
    eq("burn root", root_of(leaves), b(c["burn_root"]))
    eq("release path", root_from_path(lf, c["burn_index"], c["path"]), b(c["burn_root"]))
    n_out = sha(b"ferrata/nf-out/v2" + s_out)
    eq("release NOut", n_out, b(c["N_out"]))
    journal = (RELEASE_DOMAIN + chain + addr_word(vault) + cfg + b(c["burn_root"]) + n_out + addr_word(token) + amount
               + addr_word(recipient) + addr_word(b(c["relayer"])) + b(c["relay_fee"]))
    eq("release journal", journal, b(c["journal"]))
    eq("release journal sha256", sha(journal), b(c["journal_sha256"]))
    p = c["path"]
    gi = (b(c["burn_root"]) + u64(c["burn_index"]) + u32(p["directions"]) + b"".join(b(x) for x in p["siblings"]) + s_out + r
          + chain + vault + cfg + token + amount + recipient + b(c["relayer"]) + b(c["relay_fee"]))
    eq("release guest input", gi, b(c["guest_input"]))


def check_release_journals(v):
    assert v["count"] == len(v["cases"])
    for c in v["cases"]:
        j = (RELEASE_DOMAIN + b(c["chainId"]) + addr_word(b(c["vault"])) + b(c["lezConfigId"]) + b(c["burnRoot"]) + b(c["nullifier"])
             + addr_word(b(c["token"])) + b(c["amount"]) + addr_word(b(c["recipient"])) + addr_word(b(c["relayer"])) + b(c["relayFee"]))
        eq("release journal", j, b(c["journal"]))
        eq("release journal digest", sha(j), b(c["digest"]))


def main():
    d = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else pathlib.Path(__file__).resolve().parent.parent / "vectors")
    load = lambda n: json.loads((d / n).read_text())
    check_tags(load("tags-v2.json"))
    check_keys_config(load("keys-config-v2.json"))
    check_deposit_tree(load("deposit-tree-v2.json"))
    burn_leaves = check_burn_tree(load("burn-tree-v2.json"))
    check_checkpoint(load("checkpoint-v2.json"), burn_leaves)
    check_inbound(load("inbound-v2.json"))
    check_outbound(load("outbound-v2.json"))
    check_release_journals(load("release-journal-v2.json"))
    print(f"v2 vectors OK: {checks} values recomputed independently in {d}")


if __name__ == "__main__":
    main()
