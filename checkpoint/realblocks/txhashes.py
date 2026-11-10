"""Lists the inscription transaction hashes the lab's sequencer logged ("Submitted tx including
inscription TxHash([...])"), one line per submission: session, UTC time, hash (hex), in log order.
Usage: python3 -I txhashes.py <lez/lab/results/sessions> > txhashes.tsv"""
import gzip, pathlib, re, sys
pat = re.compile(r"^\[(\S+) .*Submitted tx including inscription TxHash\(\[([0-9, ]+)\]\)")
for d in sorted(pathlib.Path(sys.argv[1]).iterdir()):
    log = d / "sequencer.log.gz"
    if not log.exists():
        continue
    for line in gzip.open(log, "rt", errors="replace"):
        m = pat.match(line)
        if m:
            print(f"{d.name}\t{m.group(1)}\t{bytes(int(x) for x in m.group(2).split(',')).hex()}")
