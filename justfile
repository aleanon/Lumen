# Lumen task runner. `just <recipe>` — run `just` with no args to list recipes.

# List available recipes.
default:
    @just --list

# Run an example by name, e.g. `just run hello` (binary examples run headless;
# library examples — the gauntlets, gallery, mobile/web shells — run their tests).
run name *args:
    #!/usr/bin/env bash
    set -euo pipefail
    name="{{name}}"
    dir="examples/$name"
    if [[ ! -d "$dir" ]]; then
        echo "no example '$name'. available:" >&2
        ls examples >&2
        exit 1
    fi
    if [[ -f "$dir/src/main.rs" ]]; then
        cargo run -p "$name" {{args}}
    else
        echo "→ '$name' is a library example (no binary); running its tests:"
        cargo test -p "$name" {{args}}
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
