#!/usr/bin/env python3
"""The site's configuration on the host (install-web.sh step 5): the bundles' config files filled with this network's
URLs and the programs and instances it deployed (programs.json), and /AGENTS.md assembled. Standard library only.

    web-config.py WWW PRIMARY HERE RANGE_LIMIT [STAGE]

  WWW          the web root (/srv/ferrata/www), its bundles already installed from STAGE
  PRIMARY      the site's name (the domain, else the sslip.io name)
  HERE         deploy/testnet (for host/www/AGENTS-limits.md)
  RANGE_LIMIT  the gateway's getBlockRange limit
  STAGE        the unpacked web tarball (for the program streams' AGENTS sections), and STAGE/served-before/, where
               install-web.sh keeps the wallet's and the messaging module's config as they were served before

Where a program has a second version deployed beside the first (programs.json `ferrata_msg_v2`, `ferrata_names_v2`;
the first stays live and immutable), the messaging module and the wallet are pointed at the second, the first named as
legacy (the messaging module reads its old inboxes); the explorer's catalogue keeps the first, whose IDL and image it
reads. A value the host set in a served config and the bundle leaves empty (null, or a key it does not have) is kept
(the wallet's bridge settings, the messaging module's rpc), so that installing the site again loses nothing.
"""
import json
import os
import re
import sys


def load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def save(path, value):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(value, f, indent=2)


def keep(new, old):
    """`new` with what the host had set where `new` is empty: null, or a key `new` does not have (dicts merged)."""
    if isinstance(new, dict) and isinstance(old, dict):
        return {k: keep(new.get(k), old.get(k)) if k in new else old[k] for k in [*new, *(k for k in old if k not in new)]}
    return old if new is None else new


def account(progs, name):
    return ((progs.get("programs") or {}).get(name) or {}).get("program_account")


def current(progs, name):
    """(the program a client should use, the first version when a second is deployed, else None)."""
    v1, v2 = account(progs, name), account(progs, name + "_v2")
    return (v2, v1) if v2 else (v1, None)


def served_before(stage, name):
    p = os.path.join(stage, "served-before", name)
    return load(p) if stage and os.path.exists(p) else {}


def explorer(www, site, progs, range_limit):
    p = os.path.join(www, "config.json")
    if not os.path.exists(p):
        return
    c = load(p)
    c.update({"public_rpc": f"{site}/rpc", "site": site, "wallet": "/wallet/", "faucet": "/faucet",
              "faucet_api": "POST /faucet {\"account\": \"<base58 public account id>\", \"public_key\": \"<64 hex, for an account never used>\"}",
              "status_api": "/status/status.json", "range_limit": range_limit})
    c.setdefault("bedrock", {}).update({"anchored": True, "node_version": "0.3.5-rc.1",
                                        "channel_id": "0101010101010101010101010101010101010101010101010101010101010101"})
    accounts = {k: (v or {}).get("program_account") for k, v in (progs.get("programs") or {}).items()}
    # The catalogue's entries are the programs the explorer reads (their image and IDL): the first versions.
    key_of = {"ferrata_attestation": "attestation", "ferrata_bridge_b1": "bridge", "ferrata_consumer": "consumer",
              "ferrata_holdings": "holdings", "ferrata_msg": "msg"}
    for prog in c.get("programs", []):
        for name, acct in accounts.items():
            if key_of.get(name) == prog.get("key") and acct:
                prog["account"] = acct
    inst = (progs.get("instances") or {}).get("sepolia")
    if inst and inst.get("instance_id"):
        c["instances"] = [{"label": "Sepolia", "program": accounts.get("ferrata_attestation"), "instance_id": inst.get("instance_id"),
                           "account": inst.get("instance_account"), "chain_id": 11155111}]
    save(p, c)


def messaging(www, site, progs, stage):
    m = os.path.join(www, "msg", "config.json")
    if not os.path.exists(m):
        return
    c = keep(load(m), served_before(stage, "msg-config.json"))
    program, legacy = current(progs, "ferrata_msg")
    if program:
        c["program"] = program
    if legacy:
        c["legacy_program"] = legacy
    if not c.get("rpc"):
        c["rpc"] = f"{site}/rpc"
    save(m, c)


def agents(www, primary, here, range_limit, stage):
    a = os.path.join(www, "AGENTS.md")
    if not os.path.exists(a):
        return
    with open(a, encoding="utf-8") as f:
        t = f.read()
    t2 = t.replace("{{HOST}}", primary)
    t2 = re.sub(r"at most \d+ blocks per call", f"at most {range_limit} blocks per call", t2)
    # The program streams' own AGENTS sections (messaging, names) and this host's limits, appended once each.
    for sp in (os.path.join(stage, "msg/AGENTS-section.md"), os.path.join(stage, "names/AGENTS.md"),
               os.path.join(here, "host/www/AGENTS-limits.md")):
        if stage and os.path.exists(sp):
            with open(sp, encoding="utf-8") as f:
                body = f.read().strip().replace("{{HOST}}", primary)
            first = next((line for line in body.splitlines() if line.strip()), "")
            if first and first not in t2:
                t2 = t2.rstrip() + "\n\n" + body + "\n"
    if t2 != t:
        with open(a, "w", encoding="utf-8") as f:
            f.write(t2)


def wallet(www, progs, stage):
    w = os.path.join(www, "wallet", "config.json")
    if not os.path.exists(w):
        return
    c = keep(load(w), served_before(stage, "wallet-config.json"))
    c.update({"network": "Ferrata LEZ testnet (rc3)", "rpc": "/rpc", "faucet": "/faucet", "explorer": "/explorer/"})
    a = account(progs, "ferrata_attestation")
    inst = (progs.get("instances") or {}).get("sepolia")
    if a:
        c["attestation"] = {"program": a, "instances": ([{"id": "0x" + inst["instance_id"].removeprefix("0x"), "chainId": 11155111,
                            "name": "Sepolia"}] if inst and inst.get("instance_id") else [])}
    b = account(progs, "ferrata_bridge_b1")
    if b:
        c.setdefault("bridge", {})["program"] = b
    # The encrypted inbox and names panels turn on when their program is set: the second version where one is deployed.
    for key, prog in (("msg", "ferrata_msg"), ("names", "ferrata_names")):
        program, legacy = current(progs, prog)
        if program:
            c.setdefault(key, {})["program"] = program
        if legacy and key == "msg":
            c[key]["legacyProgram"] = legacy
    save(w, c)


def main(argv):
    www, primary, here, range_limit = argv[1], argv[2], argv[3], int(argv[4])
    stage = argv[5] if len(argv) > 5 else ""
    site = f"https://{primary}"
    progs = load(os.path.join(www, "programs.json"))
    explorer(www, site, progs, range_limit)
    messaging(www, site, progs, stage)
    agents(www, primary, here, range_limit, stage)
    wallet(www, progs, stage)


if __name__ == "__main__":
    main(sys.argv)
