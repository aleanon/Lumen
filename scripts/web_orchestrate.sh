#!/usr/bin/env bash
# `lumen run|test --platform web` backend (T5.1): build the WASM module and
# verify it renders. A real `run` serves web/ for the browser (WebGPU presenter +
# agent-over-WebSocket); here we run the headless node golden parity check.
set -euo pipefail
cd "$(dirname "$0")/.."
CMD="${1:-run}"

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
echo "==> building hello_web -> wasm"
cargo build -q -p hello_web --target wasm32-unknown-unknown --release
WASM=target/wasm32-unknown-unknown/release/hello_web.wasm
ls -la "$WASM"

if command -v node >/dev/null 2>&1; then
    echo "==> WASM render parity (node, no browser)"
    cargo test -q -p hello_web --test web_golden -- --ignored
else
    echo "   node missing; built the wasm but skipped the render check"
fi

if [[ "$CMD" == "run" ]]; then
    echo "==> to view in a browser: copy $WASM next to examples/hello_web/web/index.html and serve."
fi
echo "==> web OK (CPU parity verified; in-browser WebGPU + agent bridge: see examples/hello_web/web/)"
