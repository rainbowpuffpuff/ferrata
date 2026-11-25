"""The stand-in prover of the client's tests: waits STANDIN_DELAY seconds and answers the journal it was asked for (a
request named wrong-*: another journal), with a placeholder seal. No proof is made."""
import json
import os
import sys
import time

meta = json.load(open(os.environ["META"]))
delay = float(os.environ.get("STANDIN_DELAY", "0"))
time.sleep(delay)
if os.environ.get("STANDIN_FAIL"):
    sys.exit(int(os.environ["STANDIN_FAIL"]))  # the prover's process failed (the server's fail-next)
journal = "00" * 32 if meta.get("name", "").startswith("wrong") else meta["journal"]
print(json.dumps({"ok": True, "journal_hex": journal, "seal_hex": "5ea15ea1", "exit_code": 0, "wall_s": delay}))
