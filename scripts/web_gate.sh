#!/usr/bin/env bash
# P.2 gate: the web shell is interactive. Four legs:
#   1. wasm size gate (release hello_web.wasm within budget);
#   2. node session leg (mandatory): the SAME app.mjs loader the browser
#      uses — agent bridge + pointer path drive state 0→1→2;
#   3. headless-Chromium leg (best-effort: needs a chromium-family binary):
#      real browser, real DOM events via CDP, asserted via window.lumenAgent.
#   4. LEAN wasm size gate (GX3): the lean profile must actually BE lean.
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
# 4. Lean wasm size gate (GX3).
#
# This leg exists because `--no-default-features` did NOT produce a lean wasm
# module, and nothing noticed. Reaching `lumen` from a platform shell crosses
# five crates, and ONE `{ workspace = true }` link anywhere in that chain
# silently re-enables defaults for everything downstream — cargo ignores
# `default-features = false` on workspace-inherited deps. All five had to be
# converted to direct path deps before the 15.5 MB pan-Unicode face actually
# dropped out: lumen-shell-web -> lumen-shell-core -> lumen -> lumen-widgets,
# plus lumen-agent -> lumen-widgets. Fixing four of five bought ~0.1 MB; the
# fifth took it from 21.9 MB to 6.4 MB.
#
# So this is not a size budget, it is a REGRESSION DETECTOR for that trap: a
# single reverted link puts it straight back over the threshold.
echo "==> lean wasm size gate"
LTMP=$(mktemp -d)
trap 'rm -rf "$LTMP"' EXIT
mkdir -p "$LTMP/lean-wasm/src"
cat > "$LTMP/lean-wasm/Cargo.toml" <<TOML
[package]
name = "lean-wasm"
version = "0.0.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
lumen = { path = "$(pwd)/crates/lumen", default-features = false }
lumen-shell-web = { path = "$(pwd)/crates/lumen-shell-web", default-features = false }

[profile.release]
strip = true
opt-level = "z"
lto = true

[workspace]
TOML
cat > "$LTMP/lean-wasm/src/lib.rs" <<'RS'
use lumen::{widgets, BuildCx, Element};
pub fn app(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i32);
    let v = count.get(cx.runtime());
    widgets::column(vec![
        widgets::text(format!("Hello Lumen — {v}")).id("hello"),
        widgets::button("Tap", move |rt| count.update(rt, |c| *c += 1)).id("tap"),
    ])
    .id("screen")
}
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn lumen_web_render(w: u32, h: u32) -> *const u8 {
    let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
    lumen_shell_web::render_into(app, w, h, None, &mut buf);
    let ptr = buf.as_ptr();
    std::mem::forget(buf);
    ptr
}
RS
# Build via --manifest-path from the repo root, NOT `cd $LTMP && cargo build`:
# rustup resolves the toolchain from the invocation directory, so cd-ing into a
# temp dir outside the repo drops rust-toolchain.toml and lands on a default
# toolchain with no wasm32 std ("can't find crate for `core`").
cargo build -q --release --target wasm32-unknown-unknown --manifest-path "$LTMP/lean-wasm/Cargo.toml"
LWASM="$LTMP/lean-wasm/target/wasm32-unknown-unknown/release/lean_wasm.wasm"
LSIZE=$(stat -c%s "$LWASM")
echo "lean wasm size: $(echo "scale=1; $LSIZE/1048576" | bc -l) MB (budget 9, measured 6.4)"
[[ "$LSIZE" -le $((9 * 1024 * 1024)) ]] || {
  echo "FAIL: lean wasm exceeds 9 MB — a default-featured dependency link has"
  echo "      almost certainly crept back in. Check with:"
  echo "      cargo tree --target wasm32-unknown-unknown -e features -i lumen-text"
  exit 1
}

echo "web gate OK"
