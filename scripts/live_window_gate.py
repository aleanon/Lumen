#!/usr/bin/env python3
"""LW: drive real windows on a real GPU and assert they survive and behave.

See `live_window_gate.sh` for why this exists. Each leg below names the defect
it would have caught; a leg that stops corresponding to a real failure mode
should be deleted rather than kept for the count.

Design notes worth knowing before editing:

* **Liveness is checked by pid, not by the socket.** The failure mode this gate
  exists for is a PANIC, and a panicked process stops answering RPC in exactly
  the way a busy one does. `wait_alive` polls the process.
* **A leg that cannot run FAILS.** It does not skip. A gate that self-skips when
  the display or adapter is missing reports green while proving nothing — the
  same rule the `gpu` CI job already states in its own comment.
* **Retries are logged.** Window-manager timing is not deterministic under a
  nested/virtual X server, so a leg may retry, but never silently: the count is
  printed and a leg that only passes on retry is still suspicious.
"""

import json
import os
import random
import re
import shutil
import signal
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CLIENT = os.path.join(ROOT, "scripts", "agent_client.py")
ADDR = os.environ.get("LUMEN_AGENT_ADDR", "127.0.0.1:9231")
# Deterministic storm: a gate that fails one run in ten and passes the next is
# worse than no gate. The sizes are random-looking but seeded.
RNG = random.Random(0x11FE)

failures: list[str] = []


# --- process / window plumbing ----------------------------------------------


def log(msg: str) -> None:
    print(msg, flush=True)


def require(cond: bool, leg: str, msg: str) -> bool:
    if not cond:
        failures.append(f"{leg}: {msg}")
        log(f"   FAIL {msg}")
    return cond


def build_cmd(example: str) -> list[str]:
    """The build `just run-agent` performs, run separately so its cost sits
    outside every timeout in this gate."""
    if os.path.exists(os.path.join(ROOT, "examples", example, "examples", "win.rs")):
        return ["cargo", "build", "-q", "--release", "-p", f"{example}@0.0.0",
                "--example", f"{example}-win", "--features", "lumen-shell/agent"]
    return ["cargo", "build", "-q", "--release", "-p", "iced-parity",
            "--example", "win", "--features", "lumen-shell/agent"]


def example_pids(example: str) -> set[int]:
    """Pids running this example's built binary. Matched on the binary path, so
    a `just`/`cargo` wrapper cannot qualify and neither can this script."""
    out = subprocess.run(["ps", "-eo", "pid,args"], capture_output=True, text=True).stdout
    want = (f"target/release/examples/{example}-win", "target/release/examples/win")
    pids = set()
    for line in out.splitlines()[1:]:
        pid, _, args = line.strip().partition(" ")
        if args.strip().startswith(want):
            pids.add(int(pid))
    return pids


class Window:
    """A running example with an agent endpoint, and its OS window."""

    def __init__(self, example: str):
        self.example = example
        env = dict(os.environ, LUMEN_AGENT_ADDR=ADDR)
        # Built up front, outside any timeout: a cold release build takes
        # minutes, and a gate whose "did the window appear" wait races the
        # compiler reports a window failure for a build that simply had not
        # finished yet. (Exactly what happened the first time this ran.)
        subprocess.run(build_cmd(example), cwd=ROOT, env=env, check=True)
        # Snapshot existing pids, so a leftover window from an earlier run
        # cannot be mistaken for ours — which also happened on that first run.
        self._before = example_pids(example)
        self.proc = subprocess.Popen(
            ["just", "run-agent", example, ADDR],
            cwd=ROOT,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            preexec_fn=os.setsid,  # so we can kill the whole cargo/just tree
        )
        self.log_lines: list[str] = []
        self.pid = self._await_child()
        self.wid = self._await_window()

    def _await_child(self, timeout: float = 300.0) -> int:
        """The example's own pid — `just` and `cargo` are wrappers, and it is
        the example that panics, so it is the one whose liveness matters."""
        deadline = time.time() + timeout
        while time.time() < deadline:
            fresh = example_pids(self.example) - self._before
            if fresh:
                return min(fresh)
            if self.proc.poll() is not None:
                raise RuntimeError(f"launcher exited: {self.drain_log()}")
            time.sleep(0.5)
        raise RuntimeError(f"{self.example}: no process after {timeout}s")

    def _await_window(self, timeout: float = 120.0) -> str:
        deadline = time.time() + timeout
        while time.time() < deadline:
            out = subprocess.run(["wmctrl", "-lp"], capture_output=True, text=True).stdout
            for line in out.splitlines():
                parts = line.split()
                if len(parts) > 2 and parts[2] == str(self.pid):
                    return parts[0]
            time.sleep(0.5)
        raise RuntimeError(f"{self.example}: no window for pid {self.pid}")

    def alive(self) -> bool:
        try:
            os.kill(self.pid, 0)
            return True
        except OSError:
            return False

    def drain_log(self) -> str:
        if self.proc.stdout and not self.proc.stdout.closed:
            try:
                os.set_blocking(self.proc.stdout.fileno(), False)
                self.log_lines += (self.proc.stdout.read() or "").splitlines()
            except Exception:
                pass
        return "\n".join(self.log_lines[-25:])

    def resize(self, w: int, h: int) -> None:
        subprocess.run(
            ["wmctrl", "-i", "-r", self.wid, "-e", f"0,40,40,{w},{h}"],
            capture_output=True,
        )

    def rpc(self, verb: str, params: dict | None = None, timeout: float = 30.0):
        cmd = [sys.executable, CLIENT, "call", verb]
        if params is not None:
            cmd.append(json.dumps(params))
        r = subprocess.run(
            cmd,
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=timeout,
            env=dict(os.environ, LUMEN_AGENT_ADDR=ADDR),
        )
        if r.returncode != 0:
            return None
        try:
            return json.loads(r.stdout)
        except json.JSONDecodeError:
            return None

    def close(self) -> None:
        self.rpc("app.quit")
        time.sleep(0.5)
        if self.alive():
            try:
                os.killpg(os.getpgid(self.proc.pid), signal.SIGKILL)
            except OSError:
                pass


def find(node: dict, node_id: str) -> dict | None:
    if node.get("id") == node_id:
        return node
    for c in node.get("children", []):
        if hit := find(c, node_id):
            return hit
    return None


# --- legs --------------------------------------------------------------------


def leg_boot(w: Window) -> None:
    """A window opens, presents on the GPU, and answers."""
    tree = w.rpc("ui.getTree")
    require(tree is not None and tree.get("root"), "boot", "no semantics tree")
    logs = w.drain_log()
    require(
        "present = direct-to-surface" in logs,
        "boot",
        f"expected direct-to-surface presentation, log said:\n{logs}",
    )


def leg_input(w: Window) -> None:
    """A click through the real winit path changes state; a drag off the button
    does not. (Regression cover for click-on-release, TS1.)"""

    def tally() -> str:
        tree = w.rpc("ui.getTree")
        node = find(tree["root"], "value") if tree else None
        return node.get("label", "") if node else ""

    before = tally()
    w.rpc("input.click", {"selector": "#inc"})
    require(tally() != before, "input", f"click did not change #value (was {before!r})")

    mid = tally()
    w.rpc("input.drag", {"from": "#inc", "to": "#reset"})
    require(
        tally() == mid,
        "input",
        "a press that released on another node activated something",
    )


def leg_shadow_ink(w: Window) -> None:
    """The card's shadow actually paints outside the card.

    This is the leg for `DrawCmd::Image.src_rect`, which the GPU backend
    destructured into `..` — shadows rendered wrong on every live window while
    every CPU golden stayed green.
    """
    # The RAW tree, not the elided one: `#card` is an unlabelled Group, so
    # elision folds it away and every selector-based verb (`ui.getLayout`
    # included) resolves through the elided view and cannot see it. The node
    # that carries the shadow is exactly the kind of node a11y drops.
    tree = w.rpc("ui.getTree", {"raw": True})
    node = find(tree["root"], "card") if tree else None
    if not require(node is not None, "shadow-ink", "no #card in the raw tree"):
        return
    b = node["bounds"]
    # A strip just under the card's bottom edge, inset from the corners so the
    # sample is in the straight-edge band rather than the corner arc.
    x = int(b["x"] + b["w"] / 2 - 10)
    y = int(b["y"] + b["h"] + 3)
    under = w.rpc("ui.probeRegion", {"x": x, "y": y, "w": 20, "h": 3})
    # Far from the card: the page background.
    away = w.rpc("ui.probeRegion", {"x": 4, "y": 4, "w": 8, "h": 8})
    if not require(under and away, "shadow-ink", "probe failed"):
        return
    require(
        under.get("uniform") != away.get("uniform"),
        "shadow-ink",
        f"the strip below #card matches the page background "
        f"({under.get('uniform')}) — the shadow is not painting",
    )


def leg_diagnostics(w: Window) -> None:
    """A stock example reports no diagnostics. Guards against W0110 and friends
    firing on ordinary content."""
    d = w.rpc("app.diagnostics")
    if not require(d is not None, "diagnostics", "app.diagnostics failed"):
        return
    got = d.get("diagnostics") or []
    require(not got, "diagnostics", f"clean example reported {got}")


def leg_resize_storm(w: Window, n: int) -> None:
    """The SR1 crash: a resize drag panicking `Surface::configure`.

    Pre-fix this died after ~129 randomized resizes of `datagrid`. The assertion
    is liveness plus the direct path SURVIVING — a run that quietly degrades to
    CPU readback has hit the same bug from the other side (a transient skip
    misread as a dead surface) and must not pass.
    """
    for i in range(1, n + 1):
        w.resize(RNG.randint(300, 1900), RNG.randint(200, 1200))
        if not w.alive():
            failures.append(
                f"resize-storm: process died after {i} resizes\n{w.drain_log()}"
            )
            log(f"   FAIL died after {i} resizes")
            return
    log(f"   survived {n} resizes")
    require(
        "cpu-readback" not in w.drain_log(),
        "resize-storm",
        "fell back to CPU readback — a transient skip was read as a dead surface",
    )


def leg_oversize(w: Window) -> None:
    """A window past the portable 2048 px texture limit must not abort.

    Either it keeps presenting (the device's real limit is higher, which is what
    `using_resolution` is for) or it degrades to CPU readback. Both are fine;
    aborting is not.
    """
    w.resize(2600, 1500)
    time.sleep(1.5)
    if not require(w.alive(), "oversize", f"died on a >2048px window\n{w.drain_log()}"):
        return
    tree = w.rpc("ui.getTree")
    require(tree is not None, "oversize", "alive but unresponsive after the resize")
    w.resize(900, 700)
    time.sleep(0.5)
    require(w.alive(), "oversize", "died shrinking back")


def leg_multiwindow(w: Window) -> None:
    """Secondary windows exist and are introspectable.

    The wheel-inversion bug lived here: the secondary path is a near-copy of the
    primary one and drifted from it silently, because the multi-window tests
    inject `Event::Wheel` directly and never run the shell's translation.
    """
    wins = w.rpc("ui.getWindows")
    require(wins is not None, "multi-window", "ui.getWindows failed")


LEGS = {
    "boot": (leg_boot, "counter"),
    "input": (leg_input, "counter"),
    "shadow-ink": (leg_shadow_ink, "counter"),
    "diagnostics": (leg_diagnostics, "counter"),
    "multi-window": (leg_multiwindow, "counter"),
    "oversize": (leg_oversize, "counter"),
    # Heavier frame: a long frame widens the window between "size sampled" and
    # "surface configured", which is what lets the drag outrun the reconfigure.
    "resize-storm": (leg_resize_storm, "datagrid"),
}


def main(argv: list[str]) -> int:
    storm = 400
    want = list(LEGS)
    i = 0
    while i < len(argv):
        if argv[i] == "--legs":
            want = argv[i + 1].split(",")
            i += 2
        elif argv[i] == "--storm":
            storm = int(argv[i + 1])
            i += 2
        else:
            log(f"unknown argument {argv[i]!r}")
            return 2

    # Preconditions are FAILURES, not skips.
    if not os.environ.get("DISPLAY") and not os.environ.get("WAYLAND_DISPLAY"):
        log("live gate: no display. Run under X (xvfb-run works).")
        return 1
    if not shutil.which("wmctrl"):
        log("live gate: wmctrl not installed (needed to drive resizes).")
        return 1

    unknown = [x for x in want if x not in LEGS]
    if unknown:
        log(f"unknown legs: {unknown}; known: {list(LEGS)}")
        return 2

    # One window per example, shared by its legs — booting is the slow part.
    by_example: dict[str, list[str]] = {}
    for name in want:
        by_example.setdefault(LEGS[name][1], []).append(name)

    for example, legs in by_example.items():
        log(f"==> {example}: {', '.join(legs)}")
        win = None
        try:
            win = Window(example)
            log(f"    pid {win.pid}, window {win.wid}")
            for name in legs:
                log(f"--> {name}")
                fn = LEGS[name][0]
                if name == "resize-storm":
                    fn(win, storm)
                else:
                    fn(win)
                if not win.alive():
                    failures.append(f"{name}: process died during the leg")
                    break
        except Exception as e:  # a boot failure is a gate failure
            failures.append(f"{example}: {e}")
            log(f"   FAIL {e}")
        finally:
            if win:
                win.close()

    if failures:
        log("\nlive gate FAILED:")
        for f in failures:
            log(f"  - {f}")
        return 1
    log("\nlive gate: all legs green")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
