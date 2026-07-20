#!/usr/bin/env bash
# P.2 gate: the web shell is interactive. Three legs:
#   1. wasm size gate (release hello_web.wasm within budget);
#   2. node session leg (mandatory): the SAME app.mjs loader the browser
#      uses — agent bridge + pointer path drive state 0→1→2;
#   3. headless-Chromium leg (best-effort: needs a chromium-family binary):
#      real browser, real DOM events via CDP, asserted via window.lumenAgent.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> building hello_web (wasm release)"
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
cargo build -q -p hello_web --target wasm32-unknown-unknown --release
WASM=target/wasm32-unknown-unknown/release/hello_web.wasm

# 1. Size gate. Budget 24 MB: the default profile embeds the 15.5 MB
#    pan-unicode font (T.4) — the lean profile is the small one; this gate
#    catches unnoticed dependency growth, not font policy.
SIZE=$(stat -c%s "$WASM")
echo "wasm size: $((SIZE / 1024 / 1024)) MB (budget 24)"
[[ "$SIZE" -le $((24 * 1024 * 1024)) ]] || { echo "FAIL: wasm exceeds 24 MB"; exit 1; }

# 2. Node session leg.
command -v node >/dev/null || { echo "FAIL: node required for the session leg"; exit 1; }
node examples/hello_web/web/session_check.mjs "$WASM"

# 3. Headless browser leg (best-effort).
BROWSER=""
for c in chromium chromium-browser google-chrome; do
    command -v "$c" >/dev/null && BROWSER="$c" && break
done
if [[ -z "$BROWSER" ]] && flatpak info com.brave.Browser >/dev/null 2>&1; then
    BROWSER="flatpak run com.brave.Browser"
fi
if [[ -z "$BROWSER" ]]; then
    echo "SKIP: no chromium-family browser for the in-browser leg"
    exit 0
fi
python3 scripts/web_browser_leg.py "$BROWSER" || {
    echo "FAIL: browser leg"; exit 1; }
echo "web gate OK"
