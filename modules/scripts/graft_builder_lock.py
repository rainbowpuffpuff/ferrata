#!/usr/bin/env python3
"""Give a Logos module's flake.lock the logos-module-builder of another lock, with that builder's whole subtree.

    graft_builder_lock.py TARGET_LOCK SOURCE_LOCK > NEW_LOCK

Why: `nix build MODULE --override-input logos-module-builder github:logos-co/logos-module-builder/5f74cdb` relocks the
builder input but keeps the module's old lock entries for the builder's own inputs (nix reuses the old node's children
when their references are unchanged): the build then pairs builder 5f74cdb's Nix code with the module's older
logos-cpp-sdk, whose logos-cpp-generator has no `--normalize-lidl`, and the install of every module whose package
carries a dependency's or an interface's contract stops at "Plugin file does not exist: --normalize-lidl" (keystore_module,
eth_rpc_module: modules/LOGOS-CORE-MODULES.md). Grafting the subtree nix wrote for builder 5f74cdb itself (SOURCE_LOCK:
a lock that pins it, e.g. modules/ferrata_bridge_module/flake.lock) builds the module with the builder as the builder
pins its own inputs, while every other input of the module stays as its lock pins it.

Follows paths in a lock file are input paths from the top-level root (`["logos-module-builder", "logos-nix",
"nixpkgs"]`), so the grafted subtree's follows hold unchanged as long as the builder is the root's input
`logos-module-builder` in both locks. The builder node keeps the TARGET's `original` (its flake.nix's reference), with
SOURCE's `locked`. Nodes no longer reachable from the root are dropped.
"""
import json
import sys

BUILDER = "logos-module-builder"


def reachable(nodes, start):
    seen, todo = set(), [start]
    while todo:
        n = todo.pop()
        if n in seen:
            continue
        seen.add(n)
        for v in nodes[n].get("inputs", {}).values():
            if isinstance(v, str):
                todo.append(v)
    return seen


def main():
    target = json.load(open(sys.argv[1]))
    source = json.load(open(sys.argv[2]))
    tn, sn = target["nodes"], source["nodes"]
    t_root, s_root = tn[target["root"]], sn[source["root"]]
    if BUILDER not in t_root.get("inputs", {}) or BUILDER not in s_root.get("inputs", {}):
        sys.exit(f"both locks need the root input {BUILDER}")
    old = t_root["inputs"][BUILDER]
    if not isinstance(old, str):
        sys.exit(f"the target's {BUILDER} is a follows, not a node")
    sub = reachable(sn, s_root["inputs"][BUILDER])
    rename = {n: f"mb5f-{n}" for n in sub}
    for n in sub:
        node = json.loads(json.dumps(sn[n]))
        node["inputs"] = {k: (rename[v] if isinstance(v, str) else v) for k, v in node.get("inputs", {}).items()}
        tn[rename[n]] = node
    new_builder = rename[s_root["inputs"][BUILDER]]
    tn[new_builder]["original"] = tn[old]["original"]
    t_root["inputs"][BUILDER] = new_builder
    keep = reachable(tn, target["root"])
    target["nodes"] = {k: v for k, v in tn.items() if k in keep}
    json.dump(target, sys.stdout, indent=2, sort_keys=True)
    sys.stdout.write("\n")
    print(f"grafted {len(sub)} nodes under {BUILDER} ({tn[new_builder]['locked'].get('rev', '?')[:12]}); "
          f"dropped {len(tn) - len(keep)} unreachable nodes", file=sys.stderr)


if __name__ == "__main__":
    main()
