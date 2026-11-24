# The relayer job {"kind": "checkpoint", "journal", "seal"} of zone-1225-3767, rebuilt from its on-chain submitCandidate:
# the 17-word journal = domain, chain id, vault, lezConfigId, then the call's 13-word Checkpoint; the seal = its bytes.
import hashlib, json, sys, urllib.request
RPC = sys.argv[1]; TX = "0xcaa00efa1ec41cf7dfe7ebc20e35368784535fc0e6ec49934644148e13aea0d1"
VAULT = "0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787"; CID = "3773b58836d2fa242e16b3eb580a15630de2a437218a432f52e5b01152ae67cd"
def rpc(m, p):
    q = json.dumps({"jsonrpc": "2.0", "id": 1, "method": m, "params": p}).encode()
    with urllib.request.urlopen(urllib.request.Request(RPC, data=q, headers={"Content-Type": "application/json", "User-Agent": "ferrata-repro"}), timeout=30) as r:
        return json.load(r)["result"]
args = bytes.fromhex(rpc("eth_getTransactionByHash", [TX])["input"][10:])
c = args[:13 * 32]; off = int.from_bytes(args[13 * 32:14 * 32], "big"); n = int.from_bytes(args[off:off + 32], "big")
seal = args[off + 32:off + 32 + n]
journal = (hashlib.sha256(b"ferrata/vault/checkpoint/v3").digest() + (11155111).to_bytes(32, "big")
           + bytes(12) + bytes.fromhex(VAULT[2:]) + bytes.fromhex(CID) + c)
assert len(journal) == 544 and n == 260
print(json.dumps({"kind": "checkpoint", "journal": "0x" + journal.hex(), "seal": "0x" + seal.hex()}))
print("journal sha256 0x" + hashlib.sha256(journal).hexdigest(), file=sys.stderr)
