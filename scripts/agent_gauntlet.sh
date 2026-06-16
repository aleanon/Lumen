#!/usr/bin/env bash
# M4-exit release gate: the agent gauntlet, end to end, zero human intervention.
#
#   1. scaffold an app through the CLI
#   2. verify the multi-screen styled UI + custom shader on the desktop
#      (this leg also exports a passing test from the agent's own session and
#       detects + fixes an injected layout bug via structured diagnostics)
#   3. verify on the Android emulator
#   4. verify on iOS (headless here; Simulator on macOS)
set -euo pipefail
cd "$(dirname "$0")/.."

echo "== [1/4] scaffold via the CLI =="
ws="$(pwd -P)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
( cd "$tmp" && LUMEN_LOCAL_PATH="$ws" \
    cargo run -q --manifest-path "$ws/crates/lumen-cli/Cargo.toml" -- new demo --json ) \
    | grep -q '"ok":true'
test -f "$tmp/demo/Cargo.toml" && test -f "$tmp/demo/tests/app.rs"
echo "   scaffold ok (demo/ with main_app + test)"

echo "== [2/4] desktop: styled UI + shader + session export + diagnostics fix =="
cargo test -q -p agent-gauntlet --test gauntlet

echo "== [3/4] Android emulator =="
if [[ -n "${ANDROID_HOME:-}" || -f "$HOME/android-env.sh" ]]; then
    bash scripts/android_orchestrate.sh test
else
    echo "   no Android SDK; skipping (set ANDROID_HOME / source android-env.sh)"
fi

echo "== [4/4] iOS =="
bash scripts/ios_orchestrate.sh test

echo "== AGENT GAUNTLET PASSED =="
