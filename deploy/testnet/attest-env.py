"""attest-monitor's environment from the day-one kit's KEYS/instance.json: the program account in hex (the monitor
takes 32 bytes; the kit writes LEZ's base58) and the instance id. Prints FERRATA_PROGRAM=0x.. and FERRATA_INSTANCE=0x..
Usage: python3 -I attest-env.py KEYS/instance.json > /etc/ferrata/attest.env"""
import json
import sys

ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


def b58decode32(text):
    n = 0
    for ch in text:
        n = n * 58 + ALPHABET.index(ch)
    return n.to_bytes(32, "big")


inst = json.load(open(sys.argv[1]))
print("FERRATA_PROGRAM=0x" + b58decode32(inst["program"]).hex())
print("FERRATA_INSTANCE=0x" + inst["instance_id"].removeprefix("0x"))
