#!/usr/bin/env python3
"""P.2 dev relay: bridges the desktop agent tooling (newline-delimited
JSON-RPC over TCP — what `scripts/agent_client.py` speaks) to a browser
session (WebSocket client opened by app.mjs via `?agent=ws://…`).

    python3 scripts/web_agent_relay.py [--tcp 9230] [--ws 9231]

Open the app as http://…/index.html?agent=ws://127.0.0.1:9231 — then every
agent_client call round-trips: client → TCP → this relay → WS → wasm
dispatch → back.
"""
import argparse, asyncio, json
import websockets

parser = argparse.ArgumentParser()
parser.add_argument("--tcp", type=int, default=9230)
parser.add_argument("--ws", type=int, default=9231)
args = parser.parse_args()

browser = None          # the one connected browser session
pending = asyncio.Queue()  # replies from the browser


async def on_ws(ws):
    global browser
    browser = ws
    print("relay: browser connected")
    try:
        async for msg in ws:
            await pending.put(msg)
    finally:
        browser = None
        print("relay: browser gone")


async def on_tcp(reader, writer):
    while True:
        line = await reader.readline()
        if not line:
            break
        if browser is None:
            writer.write(json.dumps({
                "jsonrpc": "2.0", "id": None,
                "error": {"code": -32603, "message": "no browser connected"},
            }).encode() + b"\n")
            await writer.drain()
            continue
        await browser.send(line.decode().strip())
        reply = await pending.get()
        writer.write(reply.encode() + b"\n")
        await writer.drain()


async def main():
    ws_srv = await websockets.serve(on_ws, "127.0.0.1", args.ws)
    tcp_srv = await asyncio.start_server(on_tcp, "127.0.0.1", args.tcp)
    print(f"relay: agent TCP on 127.0.0.1:{args.tcp}, browser WS on 127.0.0.1:{args.ws}")
    async with ws_srv, tcp_srv:
        await asyncio.Future()


asyncio.run(main())
