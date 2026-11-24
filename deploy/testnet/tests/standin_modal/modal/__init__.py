"""A stand-in for the few calls of Modal's client that host/bin/ferrata-bridge-prover makes (Function.from_name, spawn,
remote, hydrate, FunctionCall.from_id, get), served by server.py beside it (FERRATA_STANDIN_MODAL_URL). For tests only:
it lets a test kill the client, the prover or the connection between them and watch what the client does."""
import base64
import json
import os
import time
import urllib.error
import urllib.request

from . import exception  # noqa: F401

__all__ = ["Function", "FunctionCall", "exception"]


def _url():
    return os.environ["FERRATA_STANDIN_MODAL_URL"].rstrip("/")


def _req(method, path, body=None, timeout=60):
    data = json.dumps(body).encode() if body is not None else None
    r = urllib.request.Request(_url() + path, data=data, method=method, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(r, timeout=timeout) as resp:
            return resp.status, json.loads(resp.read() or b"{}")
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read() or b"{}")


class FunctionCall:
    def __init__(self, object_id):
        self.object_id = object_id

    @classmethod
    def from_id(cls, object_id):
        return cls(object_id)

    def get(self, timeout=None):
        deadline = None if timeout is None else time.time() + timeout
        while True:
            status, body = _req("GET", f"/get?id={self.object_id}&wait=5", timeout=30)
            if status == 200:
                return body["result"]
            if status == 202:
                if deadline is not None and time.time() > deadline:
                    raise TimeoutError(f"call {self.object_id}: no result in {timeout} s")
                continue
            if status == 404:
                raise exception.OutputExpiredError(f"call {self.object_id}: unknown to the server")
            raise exception.RemoteError(body.get("error", f"HTTP {status}"))


class _Function:
    def __init__(self, app, name):
        self.app, self.name = app, name

    def spawn(self, *args, **kwargs):
        enc = [base64.b64encode(a).decode() if isinstance(a, bytes) else a for a in args]
        status, body = _req("POST", "/spawn", {"app": self.app, "function": self.name, "args": enc, "kwargs": kwargs})
        if status != 200:
            raise exception.RemoteError(body.get("error", f"HTTP {status}"))
        return FunctionCall(body["call_id"])

    def remote(self, *args, **kwargs):
        return self.spawn(*args, **kwargs).get()

    def hydrate(self):
        _req("GET", "/health")
        return self


class Function:
    @staticmethod
    def from_name(app, name):
        return _Function(app, name)
