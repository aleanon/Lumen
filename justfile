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
    if [[ -f "examples/$name/examples/win.rs" ]]; then
        cargo run -q --release -p "$name" --example "$name-win"
    else
        cargo run -q --release -p iced-parity --example win -- "$name"
    fi

# Run an example headlessly (no window): binaries run their smoke main, gallery names render a frame to PNG, library examples run their tests. `just render list` shows the gallery.
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
        cargo run -q -p iced-parity --example show -- "$name"   # iced-parity gallery example
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
