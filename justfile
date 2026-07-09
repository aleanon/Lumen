# Lumen task runner. `just <recipe>` — run `just` with no args to list recipes.

# List available recipes.
default:
    @just --list

# Open an example in an interactive desktop window (blocks until closed). For live `.lss` reload use `just run-hot`; for a headless render use `just render`.
run name *args:
    #!/usr/bin/env bash
    set -euo pipefail
    name="{{name}}"
    # Release: a debug build of the CPU renderer + text stack is ~35x slower,
    # which shows up as a low animation frame rate and laggy resize.
    if [[ -f "examples/$name/examples/win.rs" ]]; then
        cargo run -q --release -p "$name" --example "$name-win"  # standalone example crate
    elif [[ -d "examples/$name" && -f "examples/$name/src/main.rs" ]]; then
        cargo run -p "$name" {{args}}                          # binary example (headless smoke)
    else
        cargo run -q --release -p iced-parity --example win -- "$name"   # gallery example
    fi

# Like `just run`, but with live `.lss` hot reload (defaults to examples/<name>/app.lss; pass a path as the 2nd arg for gallery examples).
run-hot name lss="":
    #!/usr/bin/env bash
    set -euo pipefail
    name="{{name}}"
    lss="{{lss}}"
    if [[ -z "$lss" && -f "examples/$name/app.lss" ]]; then
        lss="examples/$name/app.lss"
    fi
    if [[ -z "$lss" ]]; then
        echo "no stylesheet to watch; pass one: just run-hot $name path/to.lss" >&2
        exit 1
    fi
    export LUMEN_WATCH_LSS="$lss"
    if [[ -f "examples/$name/examples/win.rs" ]]; then
        cargo run -q --release -p "$name" --example "$name-win"
    else
        cargo run -q --release -p iced-parity --example win -- "$name"
    fi

# Like `just run`, but exposes the agent endpoint (JSON-RPC) so an AI can observe + drive the live window. Default addr 127.0.0.1:9230.
run-agent name addr="127.0.0.1:9230":
    #!/usr/bin/env bash
    set -euo pipefail
    name="{{name}}"
    export LUMEN_AGENT_ADDR="{{addr}}"
    # The agent RPC server is behind lumen-shell's default-off `agent` feature.
    if [[ -f "examples/$name/examples/win.rs" ]]; then
        cargo run -q --release -p "$name" --example "$name-win" --features lumen-shell/agent
    else
        cargo run -q --release -p iced-parity --example win --features lumen-shell/agent -- "$name"
    fi

# Cleanly stop a `run-agent` window: ask it to quit over the protocol (falls back to pkill), and clear the discovery file.
stop-agent name="":
    #!/usr/bin/env bash
    set -uo pipefail
    if python3 scripts/agent_client.py call app.quit 2>/dev/null | grep -q '"ok": true'; then
        echo "agent window quit cleanly"
    elif [[ -n "{{name}}" ]]; then
        pkill -x "{{name}}-win" && echo "killed {{name}}-win" || echo "nothing to stop"
    else
        echo "endpoint unreachable; pass the example name: just stop-agent <name>" >&2
    fi
    rm -f target/lumen-agent.addr

# Run an example headlessly (no window): binaries run their smoke main, gallery names render a frame to PNG, library examples run their tests. `just render list` shows the gallery. Pass `--wgpu` for a gallery name to rasterize the linear/GPU picture.
render name *args:
    #!/usr/bin/env bash
    set -euo pipefail
    name="{{name}}"
    dir="examples/$name"
    if [[ -d "$dir" && -f "$dir/src/main.rs" ]]; then
        cargo run -p "$name" {{args}}            # binary / standalone example (headless)
    elif [[ -d "$dir" ]]; then
        echo "→ '$name' is a library example (no binary); running its tests:"
        cargo test -p "$name" {{args}}           # gauntlets / shells / gallery crate
    else
        cargo run -q -p iced-parity --example show -- "$name" {{args}}   # iced-parity gallery example (`--wgpu` = GPU/linear picture)
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
