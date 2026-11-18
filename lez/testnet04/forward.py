"""A TCP forwarder for the rehearsal: LISTEN_HOST LISTEN_PORT TARGET_HOST TARGET_PORT.

The rehearsal (rehearse.sh) gives day1.sh the zone's RPC under this machine's own non-loopback address, so that the
script and the daemon reach it as they would reach a public endpoint (by URL, through the send guard), while the
sequencer itself listens on loopback as LEZ's local network does. Nothing listens beyond this machine's own address.
"""
import asyncio
import sys


async def pipe(reader, writer):
    try:
        while True:
            data = await reader.read(65536)
            if not data:
                break
            writer.write(data)
            await writer.drain()
    except Exception:
        pass
    finally:
        try:
            writer.close()
        except Exception:
            pass


async def main(lhost, lport, thost, tport):
    async def handle(cr, cw):
        try:
            sr, sw = await asyncio.open_connection(thost, tport)
        except Exception:
            cw.close()  # the sequencer is down (a restart): the caller sees a refused connection, as from a public one
            return
        await asyncio.gather(pipe(cr, sw), pipe(sr, cw))

    server = await asyncio.start_server(handle, lhost, lport)
    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    asyncio.run(main(sys.argv[1], int(sys.argv[2]), sys.argv[3], int(sys.argv[4])))
