# Lumen task runner. `just <recipe>` — run `just` with no args to list recipes.

# List available recipes.
default:
    @just --list

# Run an example by name, e.g. `just run counter` or `just run hello`. The
# iced-parity gallery examples (counter, clock, todos, …) render a frame to a
# PNG; binary examples (hello/inspector/settings) run headless; the gauntlet/
# shell library examples run their tests. `just run list` shows the gallery.
run name *args:
    #!/usr/bin/env bash
    set -euo pipefail
    name="{{name}}"
    dir="examples/$name"
    if [[ -d "$dir" && -f "$dir/src/main.rs" ]]; then
        cargo run -p "$name" {{args}}            # binary example (hello/inspector/settings)
    elif [[ -d "$dir" ]]; then
        echo "→ '$name' is a library example (no binary); running its tests:"
        cargo test -p "$name" {{args}}           # gauntlets / shells / gallery crate
    else
        cargo run -q -p iced-parity --example show -- "$name"   # iced-parity gallery example
    fi

# Open an example in a real interactive desktop window (winit + wgpu); blocks
# until closed. Standalone example crates with their own `examples/win.rs` (e.g.
# chrono-stopwatch) run themselves; everything else is an iced-parity gallery
# name. `just win list` shows the gallery names.
win name:
    #!/usr/bin/env bash
    set -euo pipefail
    name="{{name}}"
    # Release: a debug build of the CPU renderer + text stack is ~35x slower,
    # which shows up as a low animation frame rate and laggy resize.
    if [[ -f "examples/$name/examples/win.rs" ]]; then
        cargo run -q --release -p "$name" --example win        # standalone example crate
    else
        cargo run -q --release -p iced-parity --example win -- "$name"   # gallery example
    fi

# Open an example window with the embedded agent endpoint enabled, so an AI can
# observe + drive the live window over JSON-RPC. Default addr 127.0.0.1:9230.
win-agent name addr="127.0.0.1:9230":
    #!/usr/bin/env bash
    set -euo pipefail
    name="{{name}}"
    export LUMEN_AGENT_ADDR="{{addr}}"
    if [[ -f "examples/$name/examples/win.rs" ]]; then
        cargo run -q --release -p "$name" --example win
    else
        cargo run -q --release -p iced-parity --example win -- "$name"
    fi

# List the example packages.
examples:
    @ls examples

# Run an example's tests, e.g. `just test gallery`.
test name *args:
    cargo test -p {{name}} {{args}}

# Run an example on a device/web target, e.g. `just run-on web` / `android`.
run-on platform:
    cargo run -p lumen-cli -- run --platform {{platform}}

# The full check gate (what CI runs).
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
