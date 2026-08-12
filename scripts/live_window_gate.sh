#!/usr/bin/env bash
# LW: the live-window smoke gate.
#
# Every other gate in this repo runs headless on the CPU renderer, which has no
# swapchain, no texture-dimension concept and no OS event loop. That is a real
# hole: of the five defects found in the week to 2026-08-12, the three that
# reached a user were ALL in the live surface path, and none of the 394 headless
# suites saw any of them —
#
#   * an oversize shadow sprite panicking `create_texture`   (Mercurium)
#   * a window over 2048 px aborting at open                 (inspection)
#   * a resize storm panicking `Surface::configure`          (Mercurium)
#
# plus `DrawCmd::Image.src_rect` being silently dropped on GPU, which painted
# every shadow wrong for months, and an inverted wheel in secondary windows.
#
# So this opens REAL windows on a REAL adapter, drives them through the agent
# endpoint (the same winit event path a user's input takes), abuses them, and
# asserts. It is a smoke gate: liveness, crash-freedom, gross correctness.
# Pixel parity is `cpu_vs_gpu`'s job and stays there.
#
# Usage:  scripts/live_window_gate.sh [--legs a,b,c] [--storm N]
# Needs:  an X display, a wgpu adapter, wmctrl.
set -uo pipefail
cd "$(dirname "$0")/.."
exec python3 scripts/live_window_gate.py "$@"
