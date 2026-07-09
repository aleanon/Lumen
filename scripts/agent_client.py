#!/usr/bin/env python3
"""Client for the Lumen live-window agent endpoint (03 §3).

Newline-delimited JSON-RPC 2.0 over plain TCP — the protocol `just run-agent
<example>` serves on LUMEN_AGENT_ADDR (default 127.0.0.1:9230). Stdlib only.

Library use (what the skills' snippets import):

    from agent_client import AgentClient, wait_for_port
    wait_for_port()                          # poll until the endpoint is up
    c = AgentClient()
    c.screenshot("/tmp/before.png")
    c.rpc("input.click", selector='#save')
    node = c.wait_until(lambda t: c.find(t, id="save"),
                        lambda n: "disabled" not in n["states"])
    c.screenshot("/tmp/after.png")

CLI use:

    agent_client.py wait-port [--timeout 120]
    agent_client.py call <method> ['{"json":"params"}']
    agent_client.py tree [--raw]             # compact one-line-per-node dump
    agent_client.py click '<selector>'
    agent_client.py type '<selector>' '<text>'
    agent_client.py key '<chord>'            # e.g. Enter, Ctrl+Shift+P
    agent_client.py screenshot <out.png> [--selector S] [--scale N]
    agent_client.py lint

Gotchas this client bakes in (see the verifying-apps skill):
- Live actions do NOT auto-wait: after acting, re-query the tree (or use
  `wait_until`) until the expected state appears.
- `node-N` ids returned by ui.getTree are NOT valid selectors — select by
  `#id`, role, class, or `:text-contains("…")`.
- ui.screenshot returns `image_base64`; the element form takes `selector`
  (+ optional `scale`, `overlay`).
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import socket
import sys
import time

def _default_addr() -> str:
    """LUMEN_AGENT_ADDR, else the discovery file a `:0`-bound shell wrote
    (C.8a), else the fixed default."""
    if addr := os.environ.get("LUMEN_AGENT_ADDR"):
        return addr
    path = os.environ.get("LUMEN_AGENT_ADDR_FILE", "target/lumen-agent.addr")
    try:
        with open(path, encoding="utf-8") as f:
            if addr := f.read().strip():
                return addr
    except OSError:
        pass
    return "127.0.0.1:9230"


DEFAULT_ADDR = _default_addr()


def _split(addr: str) -> tuple[str, int]:
    host, _, port = addr.rpartition(":")
    return host or "127.0.0.1", int(port)


def wait_for_port(addr: str = DEFAULT_ADDR, timeout: float = 120.0) -> None:
    """Poll until the agent endpoint accepts connections (readiness has no
    handshake — the port opening is the signal)."""
    host, port = _split(addr)
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=2):
                return
        except OSError:
            time.sleep(0.5)
    raise TimeoutError(f"agent endpoint {addr} not up after {timeout:.0f}s")


class AgentError(RuntimeError):
    """A JSON-RPC error reply (code + message from the runtime)."""

    def __init__(self, code: int, message: str):
        super().__init__(f"agent error {code}: {message}")
        self.code = code
        self.message = message


class AgentClient:
    """One TCP connection; one JSON object per line, one reply per request."""

    def __init__(self, addr: str = DEFAULT_ADDR, timeout: float = 30.0):
        host, port = _split(addr)
        self._sock = socket.create_connection((host, port), timeout=timeout)
        self._file = self._sock.makefile("rw", encoding="utf-8")
        self._id = 0

    def close(self) -> None:
        self._sock.close()

    def __enter__(self) -> "AgentClient":
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    def rpc(self, method: str, _params: dict | None = None, **params):
        """Call `method`; returns the `result` or raises AgentError."""
        self._id += 1
        req = {"jsonrpc": "2.0", "id": self._id, "method": method}
        merged = dict(_params or {}, **params)
        if merged:
            req["params"] = merged
        self._file.write(json.dumps(req) + "\n")
        self._file.flush()
        reply = json.loads(self._file.readline())
        if "error" in reply:
            err = reply["error"]
            raise AgentError(err.get("code", -1), err.get("message", "?"))
        return reply.get("result")

    # -- observation helpers -------------------------------------------------

    def tree(self, raw: bool = False) -> dict:
        """The semantics doc root (elided unless `raw`)."""
        params = {"raw": True} if raw else {}
        return self.rpc("ui.getTree", params)["root"]

    @staticmethod
    def flatten(node: dict) -> list[dict]:
        """Depth-first list of a tree's nodes."""
        out = [node]
        for child in node.get("children", []):
            out.extend(AgentClient.flatten(child))
        return out

    @staticmethod
    def find(node: dict, id: str | None = None, role: str | None = None,
             label_contains: str | None = None) -> dict | None:
        """First node matching every given criterion, or None."""
        for n in AgentClient.flatten(node):
            if id is not None and n.get("id") != id:
                continue
            if role is not None and n.get("role") != role:
                continue
            if (label_contains is not None
                    and label_contains.lower() not in n.get("label", "").lower()):
                continue
            return n
        return None

    def wait_until(self, get, pred, timeout: float = 5.0, interval: float = 0.1):
        """Poll `pred(get(tree))` until truthy — the auto-wait the live
        protocol doesn't have yet (plan C.1). Returns the value; raises on
        timeout with the last tree attached for diagnosis."""
        deadline = time.monotonic() + timeout
        last = None
        while time.monotonic() < deadline:
            tree = self.tree()
            last = get(tree)
            if last is not None and pred(last):
                return last
            time.sleep(interval)
        raise TimeoutError(f"condition not met in {timeout}s (last: {last!r})")

    def screenshot(self, path: str, selector: str | None = None,
                   scale: float | None = None) -> dict:
        """Write a PNG of the frame (or a zoomed element crop) to `path`;
        returns the metadata (width/height/box)."""
        params: dict = {}
        if selector is not None:
            params["selector"] = selector
            if scale is not None:
                params["scale"] = scale
        result = self.rpc("ui.screenshot", params)
        with open(path, "wb") as f:
            f.write(base64.b64decode(result.pop("image_base64")))
        return result


# -- CLI ----------------------------------------------------------------------


def _cmd_tree(client: AgentClient, args) -> None:
    for n in client.flatten(client.tree(raw=args.raw)):
        bits = [n["node"], n["role"]]
        if n.get("id"):
            bits.append(f"#{n['id']}")
        if n.get("label"):
            bits.append(json.dumps(n["label"]))
        if n.get("states"):
            bits.append(":" + ",".join(n["states"]))
        if n.get("actions"):
            bits.append("!" + ",".join(n["actions"]))
        b = n["bounds"]
        bits.append(f"[{b['x']:.0f},{b['y']:.0f} {b['w']:.0f}x{b['h']:.0f}]")
        print("  ".join(bits))


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--addr", default=DEFAULT_ADDR)
    sub = p.add_subparsers(dest="cmd", required=True)

    sp = sub.add_parser("wait-port", help="block until the endpoint is up")
    sp.add_argument("--timeout", type=float, default=120.0)

    sp = sub.add_parser("call", help="raw JSON-RPC call")
    sp.add_argument("method")
    sp.add_argument("params", nargs="?", default="{}")

    sp = sub.add_parser("tree", help="compact semantic-tree dump")
    sp.add_argument("--raw", action="store_true")

    sp = sub.add_parser("click")
    sp.add_argument("selector")

    sp = sub.add_parser("type")
    sp.add_argument("selector")
    sp.add_argument("text")

    sp = sub.add_parser("key")
    sp.add_argument("chord")

    sp = sub.add_parser("screenshot")
    sp.add_argument("path")
    sp.add_argument("--selector")
    sp.add_argument("--scale", type=float)

    sub.add_parser("lint")

    args = p.parse_args(argv)

    if args.cmd == "wait-port":
        wait_for_port(args.addr, args.timeout)
        print("ready")
        return 0

    with AgentClient(args.addr) as client:
        if args.cmd == "call":
            print(json.dumps(client.rpc(args.method, json.loads(args.params)),
                             indent=2))
        elif args.cmd == "tree":
            _cmd_tree(client, args)
        elif args.cmd == "click":
            print(client.rpc("input.click", selector=args.selector))
        elif args.cmd == "type":
            print(client.rpc("input.type", selector=args.selector,
                             text=args.text))
        elif args.cmd == "key":
            print(client.rpc("input.key", keys=args.chord))
        elif args.cmd == "screenshot":
            meta = client.screenshot(args.path, args.selector, args.scale)
            print(json.dumps({"path": args.path, **meta}))
        elif args.cmd == "lint":
            print(json.dumps(client.rpc("ui.lint"), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
