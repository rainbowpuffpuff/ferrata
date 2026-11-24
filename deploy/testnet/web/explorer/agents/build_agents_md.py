#!/usr/bin/env python3
"""Write the bundle's AGENTS.md from config.json and recorded RPC examples. Standard library only.

    python3 -I deploy/testnet/web/explorer/agents/build_agents_md.py --record SITE   # call SITE's RPC, keep examples
    python3 -I deploy/testnet/web/explorer/agents/build_agents_md.py                 # write AGENTS.md from what is kept

--record calls every read method of LEZ v0.4.0-rc3's sequencer RPC once (never sendTransaction, never
requeueCrossZoneDeadLetter) with parameters taken from the chain itself (the head, a user transaction, a Ferrata
account), and keeps each request and response, trimmed for reading, in agents/examples.json with the site and the
time. The endpoint, the programs and the instances printed in AGENTS.md come from config.json (`public_rpc`, `site`,
`programs`, `instances`, `faucet`, `bridge`); the scripts are agents/ferrata_check.{py,mjs}, copied in full.
"""
import argparse
import base64
import datetime
import json
import sys
import urllib.request
from pathlib import Path
from urllib.parse import urljoin

HERE = Path(__file__).resolve().parent
BUNDLE = HERE.parent
UA = {"User-Agent": "ferrata-agents-md/1", "Content-Type": "application/json"}

# Every method of lez/sequencer/service/rpc/src/lib.rs at v0.4.0-rc3, in its order, with its parameters.
METHODS = [
    ("sendTransaction", "tx: LeeTransaction (base64 of borsh)", "the transaction's hash (hex)", "Submit a signed transaction. Not called here: build and sign with LEZ's wallet library or CLI; the answer is the hash, not the outcome."),
    ("getFeeState", "", "the fee market at the head", "Base fees (execution per cycle, storage per byte), the band the next block's can move in, the per-block gas caps: size `max_fee` from this."),
    ("checkHealth", "", "null", "Answers null when the sequencer is up."),
    ("getBlock", "block_id: u64", "base64(borsh(Block)) or null", "One block, with its Bedrock status as the last byte."),
    ("getBlockRange", "start_block_id: u64, end_block_id: u64", "[base64(borsh(Block))]", "Blocks start..=end; ask for at most 100 at a time."),
    ("getLastBlockId", "", "u64", "The head."),
    ("getAccountBalance", "account_id: base58", "u128", "The native balance."),
    ("getTransaction", "tx_hash: hex", "[base64(borsh(LeeTransaction)), block_id] or null", "An included transaction and its block; null for one not (yet) in a block. No outcome: see the block's `reverted`."),
    ("getAccountsNonces", "account_ids: [base58]", "[u128]", "The accounts' nonces, in order (sign the next transaction with these)."),
    ("getAccountView", "actor: {account_id, program_account_id}", "{nonce, data: {actor_states: {program: [bytes]}}} or null", "One actor state of an account: the account's state under one program."),
    ("getPublications", "from_position: u64, limit: u32", "[[position, Publication]]", "Messages published into the commitment tree (cross-actor messages, clear or sealed)."),
    ("getMessagePath", "position: u64", "[position, [hash]] or null", "The membership path of a published message."),
    ("getRecoveryBinding", "address: base58", "EncryptedNote or null", "The recovery note bound to a private address."),
    ("getProofsAndRoot", "commitments: [[u8;32]], message_positions: [u64]", "[[path or null], [path or null], root]", "Membership paths of commitments and message leaves under one root (private transactions use these)."),
    ("getAccount", "account_id: base58", "{nonce, data: {actor_states}} or null", "Every actor state of an account."),
    ("getProgramIds", "", "{name: [u32; 8]}", "Image ids the node reports (on rc3: the privacy-preserving circuit's)."),
    ("getChannelId", "", "hex", "The zone's Bedrock channel id: what names this network."),
    ("getCrossZoneDeadLetters", "", "{total_retired, retained}", "Cross-zone deliveries this sequencer gave up on."),
    ("requeueCrossZoneDeadLetter", "message_key: hex", "requeued | already_pending | not_found | not_retained", "Operator action, unauthenticated in LEZ; this network's gateway may refuse it. Not called here."),
]
NOT_CALLED = {"sendTransaction", "requeueCrossZoneDeadLetter"}


def http_json(url, body=None):
    req = urllib.request.Request(url, data=json.dumps(body).encode() if body is not None else None, headers=UA)
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.loads(r.read())


def trim(v, depth=0):
    if isinstance(v, str) and len(v) > 160:
        return v[:120] + f"…({len(v)} chars)"
    if isinstance(v, list):
        if len(v) > 24 and all(isinstance(x, int) for x in v):
            return v[:16] + [f"…({len(v)} bytes)"]
        return [trim(x, depth + 1) for x in v[:6]] + ([f"…({len(v) - 6} more)"] if len(v) > 6 else [])
    if isinstance(v, dict):
        return {k: trim(x, depth + 1) for k, x in v.items()}
    return v


def b58encode(b):
    alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
    n = int.from_bytes(b, "big")
    s = ""
    while n:
        n, r = divmod(n, 58)
        s = alphabet[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + s


def record(site, label, tx_override=None, account_override=None):
    try:
        cfg = http_json(urljoin(site, "config.json"))
    except Exception:  # noqa: BLE001 - a site that does not serve this bundle yet: the bundle's own config
        cfg = json.loads((BUNDLE / "config.json").read_text())
    rpc_url = urljoin(site, cfg.get("rpc") or "rpc")
    calls = {}

    def call(method, params):
        body = {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}
        try:
            r = http_json(rpc_url, body)
        except Exception as e:  # noqa: BLE001
            r = {"transport_error": str(e)}
        calls[method] = {"request": body, "response": r}
        return r.get("result")

    head = call("getLastBlockId", [])
    call("checkHealth", [])
    call("getFeeState", [])
    call("getChannelId", [])
    call("getProgramIds", [])
    block = call("getBlock", [head])
    call("getBlockRange", [max(1, head - 1), head])
    # a transaction with signers, searched backwards
    tx_hash, signer = None, None
    for bid in range(head, max(0, head - 300), -1):
        b = base64.b64decode(http_json(rpc_url, {"jsonrpc": "2.0", "id": 1, "method": "getBlock", "params": [bid]})["result"])
        found = first_user_tx(b)
        if found:
            tx_hash, signer = found
            break
    if tx_override:
        tx_hash = tx_override
    if account_override:
        signer = account_override
    if tx_hash:
        call("getTransaction", [tx_hash])
    account = None
    for inst in cfg.get("instances", []):
        if inst.get("account"):
            account = (inst["account"], inst["program"])
            break
    if account:
        call("getAccountView", [{"account_id": account[0], "program_account_id": account[1]}])
        call("getAccount", [account[1]])
    elif signer:
        call("getAccountView", [{"account_id": signer, "program_account_id": "11111111111111111111111111111111"}])
        call("getAccount", [signer])
    if signer:
        call("getAccountBalance", [signer])
        call("getAccountsNonces", [[signer]])
        call("getRecoveryBinding", [signer])
    call("getPublications", [0, 2])
    call("getMessagePath", [0])
    call("getProofsAndRoot", [[], []])
    call("getCrossZoneDeadLetters", [])
    now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d %H:%MZ")
    # a method the endpoint does not serve (a replay, a gateway's allowlist) is left unrecorded rather than shown failing
    kept = {k: v for k, v in calls.items() if not (isinstance(v["response"].get("error"), dict) and v["response"]["error"].get("code") == -32601) and "transport_error" not in v["response"]}
    out = {"source": label or site, "recorded": now, "head": head, "calls": {k: {"request": v["request"], "response": trim(v["response"])} for k, v in kept.items()}}
    (HERE / "examples.json").write_text(json.dumps(out, indent=1) + "\n")
    print(f"recorded {len(kept)} of {len(calls)} methods from {rpc_url} at {now} (head {head})")


def first_user_tx(b):
    """(hash hex, signer base58) of the first transaction in a block that has a witness, via the explorer's layout."""
    import hashlib

    pos = 80
    n = int.from_bytes(b[pos:pos + 4], "little")
    pos += 4
    # Walking borsh by hand is the explorer's job (assets/lez.js); here only public transactions are needed: their
    # signers sit at the end, so take each public transaction's bytes by decoding just enough.
    for _ in range(n):
        tag = b[pos]
        start = pos + 1
        try:
            end, witnesses = skip_public(b, start) if tag == 0 else (None, None)
        except IndexError:
            return None
        if end is None:
            return None
        if witnesses:
            pk = witnesses[0]
            prefix = b"/LEE/v0.3/AccountId/Public/".ljust(32, b"\0")
            return hashlib.sha256(b[start:end]).hexdigest(), b58encode(hashlib.sha256(prefix + pk).digest())
        pos = end
    return None


def skip_public(b, p):
    u32 = lambda at: int.from_bytes(b[at:at + 4], "little")  # noqa: E731
    n = u32(p); p += 4 + 64 * n          # actors
    n = u32(p); p += 4 + 32 * n          # authorized accounts
    n = u32(p); p += 4 + 8 * n           # promotions
    p += 64                               # root call: actor
    n = u32(p); p += 4 + n               # root call: message
    if b[p] == 1:
        p += 1 + 32 + 8 + 8 + 16          # fee declaration
    else:
        p += 1
    n = u32(p); p += 4 + 48 * n          # nonces
    n = u32(p); p += 4
    for _ in range(n):                    # admission evidence
        p += 1 + (32 if b[p] == 0 else 64)
    n = u32(p); p += 4
    witnesses = []
    for _ in range(n):
        witnesses.append(bytes(b[p + 64:p + 96]))
        p += 96
    return p, witnesses


def pretty(v, ind=0):
    """JSON with objects one key per line and lists of numbers or short strings kept on one line."""
    pad = " " * ind
    if isinstance(v, dict):
        if not v:
            return "{}"
        inner = ",\n".join(f"{pad} {json.dumps(k)}: {pretty(x, ind + 1)}" for k, x in v.items())
        return "{\n" + inner + "\n" + pad + "}"
    if isinstance(v, list):
        if all(not isinstance(x, (dict, list)) for x in v):
            return "[" + ", ".join(json.dumps(x, ensure_ascii=False) for x in v) + "]"
        inner = ",\n".join(f"{pad} {pretty(x, ind + 1)}" for x in v)
        return "[\n" + inner + "\n" + pad + "]"
    return json.dumps(v, ensure_ascii=False)


def fence(lang, text):
    return f"```{lang}\n{text.rstrip()}\n```\n"


def render():
    cfg = json.loads((BUNDLE / "config.json").read_text())
    ex = json.loads((HERE / "examples.json").read_text()) if (HERE / "examples.json").exists() else {"calls": {}}
    # no host is written into the repository: the host's installer replaces {{HOST}} with its public name
    site = cfg.get("site") or "https://{{HOST}}/"
    rpc = cfg.get("public_rpc") or urljoin(site, "rpc")
    channel = (cfg.get("bedrock") or {}).get("channel_id")
    faucet = cfg.get("faucet") or "announced on the landing page when it opens"
    o = []
    w = o.append
    w("# AGENTS.md: the Ferrata LEZ test network\n\n")
    w("For AI agents and the people who run them. Reading needs no key and no account: every request below is a plain "
      "JSON-RPC call you can paste into a shell. "
      + (f"Each response shown was recorded from {ex.get('source')} on {ex.get('recorded')} (head block {ex.get('head')}); "
         "run the request again for today's values.\n\n" if ex.get("recorded") else "\n\n"))
    w("## Rules\n\n")
    w("1. **Test network only.** Its tokens have no value, and it may be reset (block numbers then start again).\n"
      "2. **Never put a private key in a prompt.** Do not paste, print, log, commit or send a private key, seed phrase "
      "or wallet password: not to this site, not to any agent, chat or issue. Generate keys with LEZ's wallet on the "
      "machine that signs and keep them there. Nothing here asks for a key; anything that does is not us.\n"
      f"3. **Be gentle.** At most about 10 requests a second; read history with `getBlockRange` (at most {cfg.get('range_limit', 100)} blocks per call).\n"
      "4. **Verify, do not trust**, the sequencer included: recompute block hashes, check the light clients' headers "
      "against Ethereum's beacon APIs, re-verify attested facts. The scripts at the end do the first two; the explorer "
      "does all three in your browser.\n"
      "5. **No receipts on LEZ v0.4.0-rc3.** A transaction's outcome is: included in a block (`getTransaction`) and "
      "not in that block's `reverted` list. Events are not served by the sequencer's RPC.\n")
    w("\n## The network\n\n")
    w("| | |\n|---|---|\n")
    w(f"| JSON-RPC endpoint | `{rpc}` (POST, JSON-RPC 2.0, CORS open) |\n")
    w(f"| Software | LEZ {cfg.get('lez', 'v0.4.0-rc3')} (Logos Execution Zone) sequencer |\n")
    w(f"| Anchored to | its own Logos Blockchain node ({(cfg.get('bedrock') or {}).get('node_version', '0.3.5-rc.1')}): every block is inscribed into the zone's Bedrock channel; a block's `bedrock_status` (Pending, Safe, Finalized) says how far |\n")
    w(f"| Chain id | LEZ transactions carry no chain id; the zone is named by its Bedrock channel id (`getChannelId`){f": `{channel}`" if channel else ""}. The Ethereum chain the light clients follow: Sepolia (11155111) |\n")
    w("| Blocks | every 10 s; at most 20 user transactions and 1 MiB per block |\n")
    nat = cfg.get("native") or {}
    if nat.get("symbol"):
        w(f"| Native token | {nat['symbol']}, {nat.get('decimals', 0)} decimals: balances and fees are integers of 10^-{nat.get('decimals', 0)} {nat['symbol']} |\n")
    w(f"| Explorer, status, programs | {urljoin(site, 'explorer/')} · {urljoin(site, 'status/')} · {urljoin(site, 'registry/')} |\n")
    w(f"| Faucet | {faucet} |\n")
    if cfg.get("faucet_api"):
        w(f"| Faucet API | `{cfg['faucet_api']}` |\n")
    if cfg.get("wallet"):
        w(f"| Web wallet | {urljoin(site, cfg['wallet'])} |\n")
    w("\n### Ferrata's programs\n\n| Program | Account (base58) | Image id | IDL |\n|---|---|---|---|\n")
    for p in cfg.get("programs", []):
        idl = f"{urljoin(site, p['idl'])}" if p.get("idl") else "—"
        w(f"| {p['name']} ({p.get('rfp', '')}) | `{p.get('account') or 'not deployed yet'}` | `0x{p['image_id']}` | {idl} |\n")
    w("\nEach image id is the hash of the exact binary, which builds reproducibly from the repository "
      f"({cfg.get('repo', '')}: `scripts/reproduce-image-ids.sh build`). A program's account holds a loader header "
      "naming its image id: `getAccountView` with `program_account_id` `" + b58encode(bytes([0xFE] * 32)) + "` (LEZ's program loader).\n")
    insts = cfg.get("instances", [])
    w("\n### Light-client instances\n\n")
    if insts:
        w("| Instance | Chain | Instance account | Instance id |\n|---|---|---|---|\n")
        for i in insts:
            w(f"| {i['label']} | {i.get('chain_id', '')} | `{i.get('account', '')}` | `{i.get('instance_id', '')}` |\n")
    else:
        w("Listed here once created; meanwhile the status page finds them from their `init` transactions.\n")
    w("\n## Encodings\n\n")
    w("- **Account ids**: 32 bytes in base58, as the RPC takes them (LEZ's wallet shows them as `Public/<base58>`).\n"
      "- **Hashes**: 64 hex digits, no `0x`.\n"
      "- **Blocks and transactions**: base64 of borsh. A block is `header {block_id u64, prev_block_hash [32], hash [32], "
      "timestamp u64 (ms)}`, `body {transactions Vec<LeeTransaction>, reverted Vec<u64>}`, then `bedrock_status u8` "
      "(0 Pending, 1 Safe, 2 Finalized), which is outside the hash.\n"
      "- **Block hash** = SHA-256(`/LEE/v0.3/Message/Block/` zero-padded to 32 bytes ‖ bytes 0..40 ‖ bytes 72..80 ‖ the body).\n"
      "- **Transaction hash** = SHA-256 of the inner transaction's borsh: the `LeeTransaction` without its first "
      "(variant) byte; variant 0 public, 1 privacy-preserving.\n"
      "- **A signer's account** = SHA-256(`/LEE/v0.3/AccountId/Public/` zero-padded to 32 ‖ its 32-byte public key).\n"
      "- **Big integers** (u128 balances and nonces) are bare JSON numbers: parse them without rounding (Python's "
      "`json` is exact; in JavaScript use a lossless parser).\n"
      "- **Account state**: `{\"nonce\": n, \"data\": {\"actor_states\": {\"<program account>\": [bytes]}}}`. The native "
      "balance is the little-endian u128 under the all-zero program `11111111111111111111111111111111`.\n"
      f"- **A full decoder** (blocks, public and private transactions, messages through an IDL) in plain ES modules: "
      f"{urljoin(site, 'assets/lez.js')}, {urljoin(site, 'assets/idl.js')}.\n")
    w("\n## Every method\n\n")
    for name, params, returns, what in METHODS:
        w(f"### `{name}`\n\n{what} Parameters: {params or 'none'}. Returns: {returns}.\n\n")
        c = ex["calls"].get(name)
        if name in NOT_CALLED or not c:
            sample = {"sendTransaction": ["<base64 of borsh(LeeTransaction)>"], "requeueCrossZoneDeadLetter": ["<message_key hex>"]}.get(name, [])
            body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": name, "params": sample})
            w(fence("sh", f"curl -s {rpc} -H 'content-type: application/json' --data '{body}'"))
            w("(No recorded response.)\n\n" if name not in NOT_CALLED else "\n")
            continue
        body = json.dumps(c["request"], separators=(",", ":"))
        w(fence("sh", f"curl -s {rpc} -H 'content-type: application/json' \\\n  --data '{body}'"))
        w(fence("json", pretty(c["response"])))
        w("\n")
    w(SECTIONS.replace("{site}", site).replace("{rpc}", rpc).replace("{repo}", cfg.get("repo", "")).replace("{vault}", (cfg.get("bridge") or {}).get("vault") or "to be announced"))
    # sections the names and messages streams wrote for this file (their programs, as deployed here)
    for sec in ("names", "msg"):
        f = HERE / "sections" / f"{sec}.md"
        if f.exists():
            w("\n" + f.read_text().strip() + "\n")
    w("\n## Minimal scripts\n\nBoth read the head block and recompute its hash, read each light-client instance's finalized "
      "Ethereum header from its account, and compare its block root with public beacon APIs. Read-only; no key.\n\n")
    w(f"### Python 3 (standard library): {urljoin(site, 'agents/ferrata_check.py')}\n\n")
    w(fence("python", (HERE / "ferrata_check.py").read_text()))
    w(f"\n### JavaScript (Node 18+): {urljoin(site, 'agents/ferrata_check.mjs')}\n\n")
    w(fence("js", (HERE / "ferrata_check.mjs").read_text()))
    w(ERRORS)
    (BUNDLE / "AGENTS.md").write_text("".join(o))
    print(f"wrote {BUNDLE / 'AGENTS.md'} ({sum(len(x) for x in o)} characters)")


SECTIONS = """
## Read Ferrata's state

- **Decode with the IDLs**: {site}idl/ferrata_attestation.idl.json and {site}idl/ferrata_bridge.idl.json (SPEL's IDL
  format). A message is its instruction's one-byte discriminator followed by the borsh of its arguments; an actor state
  decodes as one of the IDL's `accounts` types.
- **Ferrata's accounts** are PDAs: seed = SHA-256(`ferrata-lez/v1/` ‖ tag ‖ parts) (the bridge: `ferrata-lez/v1/bridge/`),
  account = SHA-256(`/LEE/v0.2/AccountId/PDA/` zero-padded to 32 ‖ program account ‖ seed). Tags of the attestation
  program: `instance` (instance id), `job` (instance id, job id), `header` (instance id, slot as u64 little-endian).
- **An instance** (`InstanceState`): its configuration, `active`, `applied`, and `store`, which is ferrata-core's
  `CompactStore`: fork byte (0 Altair … 4 Gloas), the finalized beacon header (112 bytes of SSZ), the current sync
  committee's summary (80 bytes), a flag and the next committee's summary.
- **A record** (`RecordState`) at `header(instance id, slot)`: written once for each finalized header; its `record` is
  `HeaderRecord`: the beacon header (112 bytes), the execution root, a layout byte (0 none; 1 payload: block number u64,
  block hash, parent hash, state root, receipts root, timestamp u64; 2 Gloas: the block hash only).
- **Check a record yourself**: hash its beacon header (SSZ `hash_tree_root`) and compare with
  `GET <beacon API>/eth/v1/beacon/headers/<slot>` → `data.root`; compare the execution block hash with
  `eth_getBlockByNumber` on any Ethereum RPC. The status page ({site}status/) does both for every instance.

## Attest an Ethereum fact, and read the statement

1. **Choose the record**: the instance's newest, `header(instance id, finalized slot)` (read the slot from the
   instance's store). Its execution block is the block your proofs must be for.
2. **Fetch proofs** at that block from any Ethereum RPC: `eth_getProof(address, [slots], block)` for accounts and
   storage slots; the block's receipts for logs. For a Gloas record (block hash only) include the block's RLP header,
   which must hash to the record's block hash. The program checks every proof against the record, so the RPC need not
   be trusted.
3. **Build the message**: `attest` (discriminator 7) with a `Request {instance_id, chain_id, header, hops, accounts,
   logs, leaves, relations}` (IDL). Each `AccountQuery` carries its account proof, optional conditions (`field`, `cmp`,
   32-byte `rhs`) and `SlotQuery`s; each `LogQuery` a receipt proof and conditions on the emitter, a topic or a data word.
4. **Send** a public transaction whose root call goes to actor (the record account, the attestation program), signed
   with LEZ's wallet library, fee declared from `getFeeState`. The repository's `ferrata attest`
   ({repo}/blob/main/lez/ATTEST-CLI.md) builds them: `ferrata attest --lez-rpc {rpc} --program <account> …
   attest --instance <id> --record-slot <slot> --eth <Ethereum RPC> --block <n> --storage ADDRESS:SLOT[:eq:VALUE]`
   computes the program's own verdict first and sends only with `--send --yes`. (It is built against LEZ
   v0.4.0-rc1's libraries and has not yet been run against this rc3 network.)
5. **Read the outcome**: included, and not in the block's `reverted` list, means the program verified every proof and
   condition and emitted the statements (event `FRT-ATT1`); reverted means it refused (exit codes in the IDL, e.g. 130
   `PredicateFailed`). The rc3 RPC does not serve events, so read the statement from what you proved: the explorer's
   transaction page re-verifies the request in your browser (Keccak, RLP, the state, storage and receipt tries, the
   conditions) and shows each fact's value and whether its verdict agrees with the chain's.
6. **A program that needs the statement on chain** sends `attest` as a message instead and receives an `Attestation`
   reply (`instance_id, chain_id, slot, block_number, block_hash, hops, statements[{id, fact}]`): see the reference
   consumer ({repo}/tree/main/lez/guests/consumer) and the consumer guide ({repo}/blob/main/docs/guides/consumer-guide.md).

## Use the bridge (Sepolia ⇄ LEZ)

The bridge moves ETH and registered ERC-20s from a vault on Sepolia to private (default) or public holdings on LEZ and
back. Vault on Sepolia: `{vault}`. The steps, each with the `ferrata` tool ({repo}/tree/main/cli; the user guide:
{repo}/blob/main/docs/guides/user-guide.md):

1. `ferrata keys` — your bridge seed, kept locally (never share it).
2. `ferrata deposit` — on Sepolia, into the vault, committed to your LEZ destination (or the web depositor page,
   {repo}/tree/main/web/depositor, from MetaMask).
3. Root import — anyone imports the vault's deposit root to LEZ through the attestation program.
4. `ferrata claim` — on LEZ, private by default: mints your holding without linking it to the deposit.
5. `ferrata burn` — on LEZ, to an Ethereum address; then the checkpoint is proven and, after its delay,
   `ferrata release` (or a relayer) pays out on Sepolia.
6. `ferrata recover` — rebuild everything from the seed alone.
"""

ERRORS = """
## Errors

| Where | Code | Meaning |
|---|---|---|
| JSON-RPC | -32601 | method not found (or not allowed by this network's gateway) |
| JSON-RPC | -32602 | invalid params (wrong count or type; account ids are base58, hashes hex) |
| JSON-RPC | -31900 | the mempool is full: retry later |
| JSON-RPC | -31901 | a private transaction's fee reserve is below the head's: price it again from `getFeeState` |
| Attestation program | 1-61, 80-99, 100-199, 200-219 | its exit codes, each named in the IDL's `errors` (e.g. 130 `PredicateFailed`, 211 `WRONG_INSTANCE`) |
| Bridge program | 240-255 | its exit codes, in the bridge IDL's `errors` |

A refused transaction is still included and charged: it appears in its block's `reverted` list.
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--record", metavar="SITE")
    ap.add_argument("--label", help="how AGENTS.md names the recording's source (default: the site)")
    ap.add_argument("--tx", help="the transaction hash getTransaction is shown with (default: the newest signed one)")
    ap.add_argument("--account", help="the account the balance, nonce and account examples use (default: its signer)")
    a = ap.parse_args()
    if a.record:
        record(a.record.rstrip("/") + "/", a.label, a.tx, a.account)
    render()


if __name__ == "__main__":
    sys.exit(main())
