#!/usr/bin/env python3
"""A loopback HTTP relay that counts one daemon's traffic to one endpoint (bench/perf: a daemon's bandwidth to a
public HTTPS endpoint, which tcp_count.py cannot see through TLS).

  python3 -I http_relay.py --listen 18601 --to https://host/rpc --stats stats.json [--series series.jsonl]

The daemon is configured with http://127.0.0.1:18601 in place of the endpoint. Each request is sent on to the
endpoint over HTTPS (a request to `/` goes to the endpoint's own path; any other path is appended to it), and its
answer is returned. Counted, at the HTTP level (TLS's own bytes are not): `up`, the request as the daemon sent it
(request line, headers, body); `down`, the answer as the daemon received it (status line, headers, body, with a
chunked answer counted de-chunked); per JSON-RPC method (a POST body's `method`, or each entry of a batch) or per
GET path (its first four segments). Every --every seconds the totals are appended to --series (one JSON line, with
`t`, UNIX seconds), and on SIGTERM or SIGINT --stats is written: {"to", "up", "down", "requests", "errors",
"by": {key: {"n", "up", "down", "errors"}}}. Nothing about the daemon is added to what is sent on: its own headers
(User-Agent included) travel unchanged, except Host and the hop-by-hop headers."""
import argparse, http.client, http.server, json, signal, socketserver, sys, threading, time, urllib.parse

HOP = {"connection", "keep-alive", "proxy-connection", "transfer-encoding", "te", "trailer", "upgrade", "host",
       "content-length", "accept-encoding"}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--listen", type=int, required=True)
    ap.add_argument("--to", required=True, help="https://host[:port][/path] (http:// too)")
    ap.add_argument("--stats", required=True)
    ap.add_argument("--series")
    ap.add_argument("--every", type=float, default=30.0)
    ap.add_argument("--timeout", type=float, default=120.0)
    a = ap.parse_args()
    to = urllib.parse.urlsplit(a.to)
    if to.scheme not in ("http", "https") or not to.hostname:
        sys.exit(f"--to {a.to!r}: http:// or https:// with a host")
    base_path = to.path.rstrip("/")
    lock = threading.Lock()
    stats = {"to": f"{to.scheme}://{to.hostname}{base_path or '/'}", "up": 0, "down": 0, "requests": 0, "errors": 0,
             "by": {}, "started": time.time()}
    local = threading.local()

    def conn():
        c = getattr(local, "c", None)
        if c is None:
            cls = http.client.HTTPSConnection if to.scheme == "https" else http.client.HTTPConnection
            c = cls(to.hostname, to.port, timeout=a.timeout)
            local.c = c
        return c

    def drop():
        c = getattr(local, "c", None)
        if c is not None:
            try:
                c.close()
            except OSError:
                pass
        local.c = None

    def keys_of(method, path, body):
        if method == "POST" and body:
            try:
                v = json.loads(body)
            except ValueError:
                return ["POST (not JSON)"]
            items = v if isinstance(v, list) else [v]
            return [str(i.get("method", "?")) if isinstance(i, dict) else "?" for i in items] or ["POST []"]
        segs = [s for s in urllib.parse.urlsplit(path).path.split("/") if s][:4]
        return [f"{method} /" + "/".join(segs)]

    def account(keys, up, down, err):
        with lock:
            stats["requests"] += 1
            stats["up"] += up
            stats["down"] += down
            stats["errors"] += err
            share = 1.0 / len(keys)
            for k in keys:
                e = stats["by"].setdefault(k, {"n": 0, "up": 0, "down": 0, "errors": 0})
                e["n"] += 1
                e["up"] += round(up * share)
                e["down"] += round(down * share)
                e["errors"] += err

    class Handler(http.server.BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *args):
            pass

        def relay(self):
            n = int(self.headers.get("Content-Length") or 0)
            body = self.rfile.read(n) if n else b""
            up = len(self.requestline) + 2 + sum(len(k) + len(v) + 4 for k, v in self.headers.items()) + 2 + len(body)
            path = base_path + (self.path if self.path != "/" else "") or "/"
            if not path.startswith("/"):
                path = "/" + path
            headers = {k: v for k, v in self.headers.items() if k.lower() not in HOP}
            if body or self.command == "POST":
                headers["Content-Length"] = str(len(body))
            status, reason, rh, data, err = 502, "Bad Gateway", [], b"", 1
            for attempt in (1, 2):  # a kept-alive connection the endpoint closed: once more on a fresh one
                try:
                    c = conn()
                    c.request(self.command, path, body=body if body or self.command == "POST" else None, headers=headers)
                    r = c.getresponse()
                    data = r.read()
                    status, reason, err = r.status, r.reason, 0
                    rh = [(k, v) for k, v in r.getheaders() if k.lower() not in HOP]
                    if r.will_close:
                        drop()
                    break
                except (OSError, http.client.HTTPException) as e:
                    drop()
                    data = f"relay: {type(e).__name__}: {e}".encode()
                    if attempt == 2:
                        status, reason, rh, err = 502, "Bad Gateway", [("Content-Type", "text/plain")], 1
            self.send_response_only(status, reason)
            for k, v in rh:
                self.send_header(k, v)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            line = f"HTTP/1.1 {status} {reason}"
            down = len(line) + 2 + sum(len(k) + len(v) + 4 for k, v in rh) + len("Content-Length") + len(str(len(data))) + 4 + 2 + len(data)
            account(keys_of(self.command, self.path, body), up, down, err)

        do_GET = do_POST = relay

    class Server(socketserver.ThreadingMixIn, http.server.HTTPServer):
        daemon_threads = True
        allow_reuse_address = True

    srv = Server(("127.0.0.1", a.listen), Handler)

    def dump():
        with lock:
            snap = json.loads(json.dumps(stats))
        snap["seconds"] = round(time.time() - snap["started"], 1)
        return snap

    def write_stats(*_):
        with open(a.stats + ".part", "w") as f:
            json.dump(dump(), f, indent=1, sort_keys=True)
        import os
        os.replace(a.stats + ".part", a.stats)

    def series():
        while True:
            time.sleep(a.every)
            if a.series:
                s = dump()
                with open(a.series, "a") as f:
                    f.write(json.dumps({"t": round(time.time(), 1), "up": s["up"], "down": s["down"],
                                        "requests": s["requests"], "errors": s["errors"]}) + "\n")
            write_stats()

    def stop(*_):
        write_stats()
        threading.Thread(target=srv.shutdown, daemon=True).start()

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    threading.Thread(target=series, daemon=True).start()
    write_stats()
    srv.serve_forever()
    write_stats()


if __name__ == "__main__":
    main()
