#!/usr/bin/env python3
# Each session against Ethereum, offline: for every session and chain, the finality update the session applied (its
# kept SSZ in record/net/<chain>/updates/) holds a finalized header whose hash_tree_root equals the block root the
# session's second provider reported for that slot (sessions/<id>/fetch-<chain>.json). The header is located by its
# slot (the update's layout differs between Electra and Gloas) and hashed here (BeaconBlockHeader, five fields).
#   python3 -I lez/lab/roots.py lez/lab/results
import csv, hashlib, json, os, struct, sys
R = sys.argv[1]
def h(a, b): return hashlib.sha256(a + b).digest()
def header_root(b):
    slot, prop = struct.unpack_from('<QQ', b, 0)
    leaves = [struct.pack('<Q', slot) + bytes(24), struct.pack('<Q', prop) + bytes(24), b[16:48], b[48:80], b[80:112]] + [bytes(32)] * 3
    while len(leaves) > 1: leaves = [h(leaves[i], leaves[i + 1]) for i in range(0, len(leaves), 2)]
    return slot, leaves[0].hex()
tot = ok = 0; bad = []
for chain in ('sepolia', 'mainnet'):
    jobs = list(csv.DictReader(open(f'{R}/record/net/{chain}/jobs.tsv'), delimiter='\t'))
    files = {int(f.split('.')[0]): f for f in os.listdir(f'{R}/record/net/{chain}/updates')}
    last = {}
    for j in jobs:
        if j['kind'] == 'finality' and j['status'] == 'applied':
            last[j['session']] = (int(j['seq']), int(j['finalized_slot']))
    for sess, (seq, fslot) in sorted(last.items()):
        fj = f'{R}/sessions/{sess}/fetch-{chain}.json'
        if not os.path.exists(fj): bad.append((chain, sess, 'no fetch json')); continue
        want = json.load(open(fj)).get('second_provider_finalized_root', '').removeprefix('0x')
        b = open(f'{R}/record/net/{chain}/updates/{files[seq]}', 'rb').read()
        cands = [q for q in range(0, len(b) - 112) if struct.unpack_from('<Q', b, q)[0] == fslot]
        found = [header_root(b[q:q + 112]) for q in cands]
        slot, root = next(((sl, r) for sl, r in found if r == want), found[0] if found else (None, ''))
        tot += 1
        if slot == fslot and root == want: ok += 1
        else: bad.append((chain, sess, slot, fslot, root[:12], want[:12]))
print(f'{ok} of {tot} sessions: the applied finality update\'s finalized header root equals the second provider\'s')
for x in bad: print('differs:', x)
