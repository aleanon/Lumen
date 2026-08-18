# Lumen task runner. `just <recipe>` — run `just` with no args to list recipes.

# List available recipes.
default:
    @just --list

# Open an example in an interactive desktop window (blocks until closed). For live `.lss` reload use `just run-hot`; for a headless render use `just render`.
run name *args:
    #!/usr/bin/env bash
    set -euo pipefail
    name="{{name}}"
    # Read the lockstep version rather than repeating it: the 0.0.1 bump broke
    # scripts/live_window_gate.py, which had the same pin written out as a
    # literal, while everything else stayed green.
    ver=$(awk '/^\[workspace\.package\]/{f=1;next} f&&/^version = /{gsub(/"/,"",$3); print $3; exit}' Cargo.toml)
    # Release: a debug build of the CPU renderer + text stack is ~35x slower,
    # which shows up as a low animation frame rate and laggy resize.
    # `@$ver` pins the workspace member (pre-1.0 lockstep version): the
    # `image` example would otherwise be ambiguous with the ADR-M1 `image`
    # dependency.
    if [[ -f "examples/$name/examples/win.rs" ]]; then
        cargo run -q --release -p "$name@$ver" --example "$name-win"  # standalone example crate
    elif [[ -d "examples/$name" && -f "examples/$name/src/main.rs" ]]; then
        cargo run -p "$name@$ver" {{args}}                    # binary example (headless smoke)
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
    # Read the lockstep version rather than repeating it: the 0.0.1 bump broke
    # scripts/live_window_gate.py, which had the same pin written out as a
    # literal, while everything else stayed green.
    ver=$(awk '/^\[workspace\.package\]/{f=1;next} f&&/^version = /{gsub(/"/,"",$3); print $3; exit}' Cargo.toml)
    export LUMEN_AGENT_ADDR="{{addr}}"
    # The agent RPC server is behind lumen-shell's default-off `agent` feature.
    if [[ -f "examples/$name/examples/win.rs" ]]; then
        cargo run -q --release -p "$name@$ver" --example "$name-win" --features lumen-shell/agent
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

# LW: the live-window smoke gate — opens REAL windows on a REAL adapter and abuses them.
# Needs an X display and wmctrl. `--legs a,b` runs a subset; `--storm N` sets the resize count.
live-gate *args:
    scripts/live_window_gate.sh {{args}}

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

# P.2 gate: wasm size + node session leg + headless-browser click leg.
web-gate:
    bash scripts/web_gate.sh

# P.1 gate: settings app on the emulator responds to touch + soft keyboard.
android-gate:
    bash scripts/android_input_gate.sh

# Run an example on a device/web target, e.g. `just run-on web` / `android`.
run-on platform:
    cargo run -p lumen-cli -- run --platform {{platform}}

# The CI gate, fast tier — what the pre-push hook runs. `just ci --list` shows every leg.
ci *args:
    bash scripts/ci_local.sh {{args}}

# The CI gate, every leg this machine can run (adds gpu, fonts, perf, live-window, fuzz replay).
ci-full:
    bash scripts/ci_local.sh --full

# Install the tracked git hooks (pre-push runs `just ci`). One-time, per clone.
install-hooks:
    #!/usr/bin/env bash
    set -euo pipefail
    git config core.hooksPath .githooks
    echo "core.hooksPath = .githooks"
    echo "pre-push now runs the fast CI tier. Escape hatches:"
    echo "  git push --no-verify        skip it"
    echo "  LUMEN_PREPUSH=full git push run every leg"

# Alias kept for muscle memory and the older docs — same gate as `just ci`.
check:
    bash scripts/ci_local.sh

# LN0 only: each crate checked alone with default features off (see ci_local.sh).
check-lean:
    bash scripts/ci_local.sh --only lean
