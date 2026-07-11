#!/usr/bin/env bash
# R.6: cold-start + memory gates (01 §9). Headless cold start = process exec
# → first painted frame (windowing excluded — CI has no display; the shell
# adds winit+surface time on top). Budget <300 ms, gated on the min of 5
# runs. The memory leg pumps 300 signal-write frames after warm-up and fails
# on >32 MB RSS growth (a per-frame leak would blow far past this; bounded
# cache fill stays well under).
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build -q -p hello --release
BIN=target/release/coldstart

echo "==> cold start (min of 5)"
best=999999
for _ in 1 2 3 4 5; do
  ms=$("$BIN" | python3 -c 'import json,sys; print(json.load(sys.stdin)["cold_ms"])')
  best=$(python3 -c "print(min($best, $ms))")
done
echo "cold start: ${best} ms (budget 300)"
python3 -c "import sys; sys.exit(0 if $best < 300 else 1)" || {
  echo "FAIL: cold start ${best} ms >= 300 ms"; exit 1; }

echo "==> memory growth (300 frames)"
out=$(LUMEN_MEM_GATE=1 "$BIN")
growth=$(echo "$out" | python3 -c 'import json,sys; print(json.load(sys.stdin)["rss_growth_kb"])')
echo "rss growth: ${growth} kB (budget 32768)"
python3 -c "import sys; sys.exit(0 if $growth < 32768 else 1)" || {
  echo "FAIL: rss growth ${growth} kB >= 32 MB (leak?)"; exit 1; }

echo "cold-start + memory gates OK"
