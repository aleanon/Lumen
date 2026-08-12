#!/usr/bin/env bash
# R.4/R.6/T.4: binary-size gates (01 §9). Three legs:
#  - default hello (LN2: Latin subset): regression guard at 9 MB, plus an
#    explicit pan-unicode leg at 24 MB so the opt-in face still has a ceiling.
#  - LEAN HEADLESS hello (facade `default-features = false, features =
#    ["wgpu"]` — 355 KB Latin+symbols subset, no snapshot/serde): gated at
#    8 MB.
#  - LEAN WINDOWED hello: the same feature set, but referencing `lumen::run`.
#
# CFG1 (2026-08-08) added the third leg, because the first two did not measure
# what a user ships. The lean leg calls `run_headless`, so LTO drops the entire
# wgpu presentation path (`Presenter`, ~275 lines and the whole wgpu surface
# stack) as dead code. Same features, same profile, measured back to back:
#
#     lean headless   6.8 MB      lean WINDOWED  14.0 MB
#
# So the gate was reporting roughly half the shipped size, and 01 §9's
# "hello-world binary <5 MB" — a budget that plainly means a GUI app with a
# window — was being checked against a binary with no window in it. Same defect
# class as the parity suite that self-skipped without an adapter: green, and
# asserting less than it appears to.
set -euo pipefail
cd "$(dirname "$0")/.."

# rustc SIGSEGVs compiling `moxcms` (pulled by `image`) under this gate's
# `opt-level=z` + linker-plugin-lto profile — a const-eval stack overflow, with
# rustc's own diagnostic suggesting exactly this. Not caused by anything in this
# repo: the crate does not depend on us and the same version compiles fine at
# opt-level 0/3. Remove when rustc or moxcms fixes it.
export RUST_MIN_STACK="${RUST_MIN_STACK:-16777216}"

# LN2 re-baseline. The default no longer embeds the pan-Unicode face, so the
# old 24 MB threshold would have passed whatever happened — including the face
# coming back by accident, which is the one regression this leg exists to catch.
echo "==> default profile (LN2: Latin+symbols subset, no pan-Unicode face)"
cargo build -q -p hello --release
SIZE=$(stat -c%s target/release/hello)
echo "hello (default): $(echo "scale=1; $SIZE/1048576" | bc -l) MB"
[ "$SIZE" -lt $((9 * 1048576)) ] || {
    echo "FAIL: default > 9 MB — has the 15 MB pan-Unicode face come back?"; exit 1; }

# The opt-in face still needs a ceiling, or it could grow unnoticed now that
# nothing else measures it.
echo "==> pan-unicode profile (opt-in CJK/RTL/Indic face)"
cargo build -q -p hello --release --features lumen/pan-unicode
PSIZE=$(stat -c%s target/release/hello)
echo "hello (pan-unicode): $(echo "scale=1; $PSIZE/1048576" | bc -l) MB"
[ "$PSIZE" -lt $((24 * 1048576)) ] || { echo "FAIL: pan-unicode > 24 MB"; exit 1; }
[ "$PSIZE" -gt "$SIZE" ] || {
    echo "FAIL: the pan-unicode build is not larger than the default — the"
    echo "      feature is not reaching the font. A lean gate that measures the"
    echo "      wrong build reports a number and proves nothing."; exit 1; }

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

echo "==> lean profile, WINDOWED (what a user actually ships)"
mkdir -p "$TMP/win-app/src"
sed 's/name = "lean-app"/name = "win-app"/' "$TMP/lean-app/Cargo.toml" > "$TMP/win-app/Cargo.toml"
cat > "$TMP/win-app/src/main.rs" <<'RS'
use lumen::widgets::{button, column, text};
use lumen::App;
fn main() {
    let app = App::new(|cx| {
        let n = cx.signal("n", || 0i64);
        column(vec![
            text(format!("Count: {}", { n.get(cx.runtime()) })),
            button("+", move |rt| n.update(rt, |v| *v += 1)),
        ])
    });
    // Never true under the gate, but opaque to the linker, so the windowed
    // shell + wgpu presentation path are linked in. The gate builds only; it
    // never opens a window, so this stays headless-CI safe.
    if std::env::var("LUMEN_SIZE_GATE_RUN").is_ok() {
        lumen::run(app, lumen::geometry::Size::new(300.0, 200.0));
    }
}
RS
(cd "$TMP/win-app" && cargo build -q --release)
WSIZE=$(stat -c%s "$TMP/win-app/target/release/win-app")
echo "win-app:  $(echo "scale=1; $WSIZE/1048576" | bc -l) MB"
# 16 MB is a REGRESSION guard at today's measured 14.0 MB, not a target. The
# 01 §9 goal is <5 MB; reaching it needs a CPU presentation path so wgpu can be
# dropped (see docs/constrained-profile.md) — wgpu is currently an
# unconditional dependency of lumen-shell.
[ "$WSIZE" -lt $((16 * 1048576)) ] || { echo "FAIL: lean windowed > 16 MB"; exit 1; }

echo "==> NO-GPU windowed (softbuffer presentation, ADR-003 amendment)"
mkdir -p "$TMP/nogpu-app/src"
sed 's/name = "win-app"/name = "nogpu-app"/; s/features = \["wgpu"\]//' \
    "$TMP/win-app/Cargo.toml" > "$TMP/nogpu-app/Cargo.toml"
cp "$TMP/win-app/src/main.rs" "$TMP/nogpu-app/src/main.rs"
(cd "$TMP/nogpu-app" && cargo build -q --release)
NSIZE=$(stat -c%s "$TMP/nogpu-app/target/release/nogpu-app")
echo "nogpu-app: $(echo "scale=1; $NSIZE/1048576" | bc -l) MB"
# Must be materially smaller than the wgpu build, or the GPU stack did not
# actually leave the graph — which it did not, four separate times, until every
# `{ workspace = true }` link to lumen-render was converted to a path dep.
[ "$NSIZE" -lt "$WSIZE" ] || {
  echo "FAIL: the no-GPU build is not smaller than the wgpu one ($NSIZE vs $WSIZE)."
  echo "      Check with: cargo tree -p lumen-shell --no-default-features -e normal --invert wgpu"
  exit 1
}
[ "$NSIZE" -lt $((13 * 1048576)) ] || { echo "FAIL: no-GPU > 13 MB"; exit 1; }

echo "size gates OK"
