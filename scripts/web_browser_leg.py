#!/usr/bin/env python3
"""P.2 browser leg: drive the Lumen web session in a real headless
chromium-family browser over CDP — real DOM input events, assertions through
the same agent bridge (window.lumenAgent)."""
import asyncio, json, shutil, socket, subprocess, sys, tempfile, time, urllib.request
import http.server, threading, functools, os

import websockets

BROWSER = sys.argv[1] if len(sys.argv) > 1 else "chromium"
ROOT = os.path.join(os.path.dirname(__file__), "..")
WEB = os.path.join(ROOT, "examples/hello_web/web")
WASM = os.path.join(ROOT, "target/wasm32-unknown-unknown/release/hello_web.wasm")


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def serve_web(port):
    # Serve web/ plus the wasm (copied next to index.html via a symlink dir).
    d = tempfile.mkdtemp()
    for f in os.listdir(WEB):
        os.symlink(os.path.join(WEB, f), os.path.join(d, f))
    os.symlink(WASM, os.path.join(d, "hello_web.wasm"))
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=d)
    httpd = http.server.HTTPServer(("127.0.0.1", port), handler)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd


async def main():
    http_port = free_port()
    cdp_port = free_port()
    serve_web(http_port)
    profile = tempfile.mkdtemp()
    cmd = BROWSER.split() + [
        "--headless=new", f"--remote-debugging-port={cdp_port}",
        f"--user-data-dir={profile}", "--no-first-run", "--disable-gpu",
        f"http://127.0.0.1:{http_port}/index.html",
    ]
    proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        ws_url = None
        for _ in range(60):
            try:
                pages = json.load(urllib.request.urlopen(
                    f"http://127.0.0.1:{cdp_port}/json", timeout=1))
                for p in pages:
                    if p.get("type") == "page" and "index.html" in p.get("url", ""):
                        ws_url = p["webSocketDebuggerUrl"]
                        break
            except Exception:
                pass
            if ws_url:
                break
            time.sleep(0.5)
        assert ws_url, "no CDP page endpoint"

        async with websockets.connect(ws_url, max_size=20 * 1024 * 1024) as ws:
            mid = 0

            async def cdp(method, **params):
                nonlocal mid
                mid += 1
                await ws.send(json.dumps({"id": mid, "method": method, "params": params}))
                while True:
                    msg = json.loads(await ws.recv())
                    if msg.get("id") == mid:
                        return msg.get("result", {})

            async def js(expr):
                r = await cdp("Runtime.evaluate", expression=expr, returnByValue=True)
                return r.get("result", {}).get("value")

            # Wait for the session to boot.
            for _ in range(60):
                if await js("typeof window.lumenAgent === 'function'"):
                    break
                await asyncio.sleep(0.5)
            assert await js("typeof window.lumenAgent === 'function'"), "session did not boot"

            def rpc_expr(method, params="{}"):
                return (f"window.lumenAgent(JSON.stringify({{jsonrpc:'2.0',id:1,"
                        f"method:'{method}',params:{params}}}))")

            tree = json.loads(await js(rpc_expr("ui.getTree")))
            blob = json.dumps(tree)
            assert "— 0" in blob or "\\u2014 0" in blob, f"unexpected start state"

            # Find the button center from the agent, then REAL browser input.
            def find(n, want):
                if n.get("id") == want:
                    return n
                for c in n.get("children", []):
                    r = find(c, want)
                    if r:
                        return r
            tap = find(tree["result"]["root"], "tap")
            x = tap["bounds"]["x"] + tap["bounds"]["w"] / 2
            y = tap["bounds"]["y"] + tap["bounds"]["h"] / 2
            for t in ("mousePressed", "mouseReleased"):
                await cdp("Input.dispatchMouseEvent", type=t, x=x, y=y,
                          button="left", clickCount=1)
            await asyncio.sleep(0.5)

            tree = json.loads(await js(rpc_expr("ui.getTree")))
            blob = json.dumps(tree)
            assert "— 1" in blob or "\\u2014 1" in blob, "browser click did not increment"
            print("browser leg OK: boot, agent bridge, real click 0→1")
    finally:
        proc.terminate()


asyncio.run(main())
