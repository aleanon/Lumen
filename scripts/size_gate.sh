#!/usr/bin/env bash
# T6.6: report the hello-world release binary size against the 01 §9 budget.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p hello --release
BIN="target/release/hello"
SIZE=$(stat -c%s "$BIN")
MB=$(echo "scale=1; $SIZE/1048576" | bc -l)
echo "hello release binary: ${MB} MB ($SIZE bytes)"
# The 01 §9 target is <5 MB; the bundled 15.5 MB CJK font dominates (ADR-005),
# so the realistic gate is the binary minus embedded font assets. Report both.
echo "note: includes the bundled GoNotoKurrent font (~15.5 MB, ADR-005);"
echo "      a subset/on-demand font is required to hit the <5 MB target."
