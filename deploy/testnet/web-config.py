#!/usr/bin/env python3
"""The site's configuration on the host (install-web.sh step 5): the bundles' config files filled with this network's
URLs and the programs and instances it deployed (programs.json), and /AGENTS.md assembled. Standard library only.

    web-config.py WWW PRIMARY HERE RANGE_LIMIT [STAGE [SERVED]]

  WWW          the web root (/srv/ferrata/www), its bundles already installed from STAGE
  PRIMARY      the site's name (the domain, else the sslip.io name)
  HERE         deploy/testnet (for host/www/AGENTS-limits.md)
  RANGE_LIMIT  the gateway's getBlockRange limit
  STAGE        the unpacked web tarball (for the program streams' AGENTS sections), with live-configs/: the wallet's
               and the messaging module's config as this run found them (wallet-config.json, msg-config.json, the
               host's hand edits included), which install-web.sh copies there before the bundle replaces them
  SERVED       the same two as served after the last run that succeeded: install-web.sh keeps them outside the stage
               and writes them only after this step succeeds; they fill only what the live copies lack or leave null
               (the bundle's raw configs, which a run stopped half way leaves in place)

The messaging module is the messages program's second version's client: it is pointed at `ferrata_msg_v2`
(programs.json), the first version, live and immutable, named as legacy (the module reads its old inboxes); on a host
without the second version it is left off (program null), never pointed at the first, whose messages it would misread.
The wallet's inbox and names panels are pointed at the program their bundled client is written for, which the client's
own seed domains tell (wallet/msg/ferrata-msg.js, wallet/vendor/names.js): the first versions for the wallet as it is
bundled now, the second ones once its clients are; a panel whose program is not deployed, or whose client is not
recognised, is left off (the wallet says no program is configured). The explorer's catalogue keeps the first
versions, whose IDLs and images it reads. What the host set in a served config is kept where the bundle leaves it
empty (null, or a key it does not have): the wallet's bridge settings, hand edits included (deposits closed with
lezReady false stay closed). The live file's values win; the copy last served fills only what the live file lacks or
leaves null. The module's rpc is the site's own, written each run (absolute: its command-line tool reads the file
outside a browser). Each file, /AGENTS.md included, is written whole (a new file renamed over the old).
"""
import json
import os
import re
import sys


def load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def save_text(path, text):
    """The file written whole: a new file renamed over the old, so a reader never sees half of it."""
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(text)
    os.replace(tmp, path)


def save(path, value):
    save_text(path, json.dumps(value, indent=2))


def keep(new, old):
    """`new` with what the host had set where `new` is empty: null, or a key `new` does not have (dicts merged)."""
    if isinstance(new, dict) and isinstance(old, dict):
        return {k: keep(new.get(k), old.get(k)) if k in new else old[k] for k in [*new, *(k for k in old if k not in new)]}
    return old if new is None else new


def account(progs, name):
    return ((progs.get("programs") or {}).get(name) or {}).get("program_account")


def current(progs, name):
    """(the second version, whose client the messaging module is, or None; the first version, as legacy, or None)."""
    return account(progs, name + "_v2"), account(progs, name)


# The wallet's panels and the client each bundles: (the panel, the client's file under wallet/, (the program it is
# written for, a seed domain only that version's client holds), the second version first: its messages client also
# holds the first's key salt, to read old inboxes).
WALLET_CLIENTS = (("names", "vendor/names.js", (("ferrata_names_v2", "'ferrata-names/v2/name/'"),
                                                ("ferrata_names", "'ferrata-names/v1/name/'"))),
                  ("msg", "msg/ferrata-msg.js", (("ferrata_msg_v2", "'ferrata-msg/v2/'"),
                                                 ("ferrata_msg", "'ferrata-msg/v1'"))))


def written_for(path, markers):
    """The program (its programs.json name) the client at `path` is written for; None if missing or not recognised."""
    try:
        with open(path, encoding="utf-8") as f:
            src = f.read()
    except OSError:
        return None
    return next((name for name, marker in markers if marker in src), None)


def host_values(stage, served, name):
    """What the host had set in a served config: the live file as this run found it (STAGE/live-configs/NAME) first;
    the copy served after the last good run (SERVED/NAME) fills only what the live file lacks or leaves null."""
    def read(d):
        p = os.path.join(d, name) if d else ""
        return load(p) if p and os.path.exists(p) else {}
    return keep(read(os.path.join(stage, "live-configs") if stage else ""), read(served))


def lez_host_tag():
    """The host's LEZ tag: FERRATA_LEZ_TAG (empty counts as unset), else the tag install.sh records on a v0.4.0 host
    (/etc/ferrata/lez-tag), else v0.4.0-rc3; a variable that disagrees with the record is refused (as lez-tag.sh does)."""
    env = os.environ.get("FERRATA_LEZ_TAG") or ""
    path = os.environ.get("FERRATA_LEZ_TAG_FILE") or "/etc/ferrata/lez-tag"
    host = ""
    if os.path.lexists(path):   # a record that exists must name a tag: an empty or damaged one is refused, not taken for rc3
        try:
            with open(path) as f:
                host = "".join(f.read().split())
        except OSError as e:
            sys.exit(f"this host's tag record {path} cannot be read: {e}")
        if host not in ("v0.4.0-rc3", "v0.4.0"):
            sys.exit(f"this host's tag record {path} holds {host!r}, not v0.4.0-rc3 or v0.4.0")
    if env and host and env != host:
        sys.exit(f"FERRATA_LEZ_TAG={env}, but this host runs LEZ {host} ({path})")
    tag = env or host or "v0.4.0-rc3"
    if tag not in ("v0.4.0-rc3", "v0.4.0"):
        sys.exit(f"FERRATA_LEZ_TAG must be v0.4.0-rc3 or v0.4.0 (got {tag!r})")
    return tag


def explorer(www, site, progs, range_limit):
    p = os.path.join(www, "config.json")
    if not os.path.exists(p):
        return
    c = load(p)
    c.update({"public_rpc": f"{site}/rpc", "site": site, "wallet": "/wallet/", "faucet": "/faucet",
              "faucet_api": "POST /faucet {\"account\": \"<base58 public account id>\", \"public_key\": \"<64 hex, for an account never used>\"}",
              "status_api": "/status/status.json", "range_limit": range_limit})
    # the Logos Blockchain node release the host runs per LEZ tag (lez-tag.sh's BEDROCK_RELEASE)
    node_version = "0.4.0-rc.2" if lez_host_tag() == "v0.4.0" else "0.3.5-rc.1"
    c.setdefault("bedrock", {}).update({"anchored": True, "node_version": node_version,
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


def messaging(www, site, progs, stage, served):
    m = os.path.join(www, "msg", "config.json")
    if not os.path.exists(m):
        return
    c = keep(load(m), host_values(stage, served, "msg-config.json"))
    program, legacy = current(progs, "ferrata_msg")
    c["program"] = program  # null without the second version: the module is left off
    if legacy:
        c["legacy_program"] = legacy
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
        save_text(a, t2)


def wallet(www, progs, stage, served):
    w = os.path.join(www, "wallet", "config.json")
    if not os.path.exists(w):
        return
    c = keep(load(w), host_values(stage, served, "wallet-config.json"))
    c.update({"network": "Ferrata LEZ testnet (v0.4.0)" if lez_host_tag() == "v0.4.0"
                  else "Ferrata LEZ testnet (rc3)", "rpc": "/rpc", "faucet": "/faucet", "explorer": "/explorer/"})
    a = account(progs, "ferrata_attestation")
    inst = (progs.get("instances") or {}).get("sepolia")
    if a:
        c["attestation"] = {"program": a, "instances": ([{"id": "0x" + inst["instance_id"].removeprefix("0x"), "chainId": 11155111,
                            "name": "Sepolia"}] if inst and inst.get("instance_id") else [])}
    b = account(progs, "ferrata_bridge_b1")
    if b:
        c.setdefault("bridge", {})["program"] = b
    # The encrypted inbox and names panels turn on when their program is set: the one their bundled client is written
    # for, never another version, whose accounts and messages the client would misread.
    for key, client, markers in WALLET_CLIENTS:
        name = written_for(os.path.join(www, "wallet", client), markers)
        p = c[key] = c[key] if isinstance(c.get(key), dict) else {}
        p["program"] = account(progs, name) if name else None
        legacy = account(progs, "ferrata_msg") if name == "ferrata_msg_v2" else None
        if legacy:
            p["legacyProgram"] = legacy
        else:
            p.pop("legacyProgram", None)
    save(w, c)


def main(argv):
    www, primary, here, range_limit = argv[1], argv[2], argv[3], int(argv[4])
    stage = argv[5] if len(argv) > 5 else ""
    served = argv[6] if len(argv) > 6 else ""
    site = f"https://{primary}"
    progs = load(os.path.join(www, "programs.json"))
    explorer(www, site, progs, range_limit)
    messaging(www, site, progs, stage, served)
    agents(www, primary, here, range_limit, stage)
    wallet(www, progs, stage, served)


if __name__ == "__main__":
    main(sys.argv)
