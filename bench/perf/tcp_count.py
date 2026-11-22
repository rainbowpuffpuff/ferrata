#!/usr/bin/env python3
"""A loopback TCP proxy that counts the bytes each way (bench/perf: a daemon's bandwidth to one endpoint).
  python3 -I tcp_count.py --listen 13041 --to 127.0.0.1:3040 --stats stats.json
On SIGTERM or SIGINT it writes {"up": client->server bytes, "down": server->client bytes, "connections": n}."""
import argparse, asyncio, json, os, signal

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--listen", type=int, required=True)
    ap.add_argument("--to", required=True)
    ap.add_argument("--stats", required=True)
    a = ap.parse_args()
    host, port = a.to.rsplit(":", 1)
    stats = {"up": 0, "down": 0, "connections": 0}

    async def pipe(r, w, key):
        try:
            while True:
                b = await r.read(65536)
                if not b:
                    break
                stats[key] += len(b)
                w.write(b)
                await w.drain()
        except (ConnectionError, asyncio.CancelledError):
            pass
        finally:
            try:
                w.close()
            except Exception:
                pass

    async def serve(cr, cw):
        stats["connections"] += 1
        try:
            sr, sw = await asyncio.open_connection(host, int(port))
        except OSError:
            cw.close()
            return
        await asyncio.gather(pipe(cr, sw, "up"), pipe(sr, cw, "down"))

    def stop(*_):
        with open(a.stats, "w") as f:
            json.dump(stats, f)
        os._exit(0)

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)

    async def run():
        server = await asyncio.start_server(serve, "127.0.0.1", a.listen)
        async with server:
            await server.serve_forever()

    asyncio.run(run())

if __name__ == "__main__":
    main()
