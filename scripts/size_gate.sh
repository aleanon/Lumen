#!/usr/bin/env bash
# R.4/R.6/T.4: binary-size gates (01 §9). Two legs:
#  - default hello (pan-Unicode face embedded): regression guard at 24 MB.
#  - LEAN hello (facade `default-features = false, features = ["wgpu"]` —
#    the shipped profile: 355 KB Latin+symbols subset, no snapshot/serde):
#    gated at 8 MB. Measured 7.5 MB at T.4; the 01 §9 <5 MB target needs a
#    further dependency diet (tracked in backlog) — this gate stops
#    regressions from today's baseline.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> default profile"
cargo build -q -p hello --release
SIZE=$(stat -c%s target/release/hello)
echo "hello (default): $(echo "scale=1; $SIZE/1048576" | bc -l) MB"
[ "$SIZE" -lt $((24 * 1048576)) ] || { echo "FAIL: default > 24 MB"; exit 1; }

echo "==> lean profile (scaffolded app, facade lean features)"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
WS_ROOT=$(pwd)
mkdir -p "$TMP/lean-app/src"
cat > "$TMP/lean-app/Cargo.toml" <<TOML
[package]
name = "lean-app"
version = "0.0.0"
edition = "2021"

[dependencies]
lumen = { path = "$WS_ROOT/crates/lumen", default-features = false, features = ["wgpu"] }

[profile.release]
strip = true
opt-level = "z"
lto = true

[workspace]
TOML
cat > "$TMP/lean-app/src/main.rs" <<'RS'
use lumen::widgets::{button, column, text};
use lumen::App;
fn main() {
    let mut h = App::new(|cx| {
        let n = cx.signal("n", || 0i64);
        column(vec![
            text(format!("Count: {}", {
                n.get(cx.runtime())
            })),
            button("+", move |rt| n.update(rt, |v| *v += 1)),
        ])
    })
    .run_headless(lumen::geometry::Size::new(300.0, 200.0));
    h.pump();
}
RS
(cd "$TMP/lean-app" && cargo build -q --release)
LSIZE=$(stat -c%s "$TMP/lean-app/target/release/lean-app")
echo "lean-app: $(echo "scale=1; $LSIZE/1048576" | bc -l) MB"
[ "$LSIZE" -lt $((8 * 1048576)) ] || { echo "FAIL: lean > 8 MB"; exit 1; }

echo "size gates OK"
