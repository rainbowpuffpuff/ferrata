"""key-ids.py DID: the two names Logos Basecamp 0.4.0-rc.2 shows for a did:jwk Ed25519 signer, as this run read them on
its screens: the key (first 8 bytes of SHA-256 of the raw public key, in groups of four hex digits; the install
prompt and Trusted Publishers) and an unnamed publisher's name ("publisher-" and the first 8 bytes of SHA-256 of the
DID string). Derived here from the catalog's DID and compared with the screenshots by eye."""
import base64, hashlib, json, sys

did = sys.argv[1]
b = did.split(":", 2)[2]
jwk = json.loads(base64.urlsafe_b64decode(b + "=" * (-len(b) % 4)))
pk = base64.urlsafe_b64decode(jwk["x"] + "=" * (-len(jwk["x"]) % 4))
k = hashlib.sha256(pk).hexdigest()[:16].upper()
print("key:", " ".join(k[i:i + 4] for i in range(0, 16, 4)))
print("publisher:", "publisher-" + hashlib.sha256(did.encode()).hexdigest()[:16])
