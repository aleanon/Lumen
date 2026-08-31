#!/usr/bin/env bash
# BENCH5 runner. See common.md for the contract every harness obeys.
#
# Builds all four binaries, runs the full (framework x mode x N) matrix pinned
# to one P-core, and writes one TSV row per configuration to stdout.
#
# Each configuration is run REPS times and the minimum of the per-run minima is
# kept. A single min-of-200 still varied ~40% run-to-run on this box during
# development; min-of-mins over 3 runs is stable to a few percent, which is the
# resolution any conclusion here actually needs.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/../.."          # benches-competitive/
ROOT=$PWD
OUT=${OUT:-/tmp/bench5}
mkdir -p "$OUT"
CPU=${CPU:-2}
REPS=${REPS:-3}
NS=${NS:-"100 1000 3000 10000"}
PIN="taskset -c $CPU"

if [ "${SKIP_BUILD:-0}" != 1 ]; then
  echo "# building..." >&2
  # Two Lumen binaries: the DEFAULT build (what `cargo run` gives, including
  # the O0.1 per-frame ambient audit) and the RELEASE build without it. They
  # are different enough that quoting one as "Lumen" would be wrong.
  cargo build --release --bin probe_bench5 -q                    || exit 1
  cp target/release/probe_bench5 "$OUT/lumen_dev"
  cargo build --release --bin probe_bench5 -q --no-default-features || exit 1
  cp target/release/probe_bench5 "$OUT/lumen_rel"
  gcc -O2 -g -o "$OUT/gtk" harnesses/bench5/gtk.c $(pkg-config --cflags --libs gtk4) || exit 1
  cmake -S harnesses/bench5 -B "$OUT/qtb" -DCMAKE_BUILD_TYPE=RelWithDebInfo >/dev/null || exit 1
  cmake --build "$OUT/qtb" -j8 >/dev/null || exit 1
fi

# Refuse to measure on a busy box. A user process at 100% CPU has twice during
# this project moved a reading by 12-38%.
busy=$(ps -eo pcpu= --sort=-pcpu | head -2 | tail -1 | tr -d ' ')
if [ "${busy%%.*}" -gt 50 ] 2>/dev/null; then
  echo "# WARNING: a process is at ${busy}% CPU - numbers will be noisy" >&2
fi

# run <label> <cmd...>   -> emits "key value" pairs from the best repetition
run_best() {
  local label="$1"; shift
  local best="" bestval=""
  for _ in $(seq "$REPS"); do
    local o; o=$("$@" 2>/dev/null)
    local t; t=$(echo "$o" | awk -F'\t' '$1=="total_us"{print $2}')
    [ -z "$t" ] && continue
    if [ -z "$bestval" ] || awk "BEGIN{exit !($t < $bestval)}"; then bestval=$t; best=$o; fi
  done
  [ -z "$best" ] && { echo "# FAILED: $label $*" >&2; return; }
  echo "$best" | awk -F'\t' -v L="$label" '
    $1=="total_us"||$1 ~ /^stage\./||$1 ~ /^rss\./||$1 ~ /^frame\./||$1 ~ /^nodes\./ {print L"\t"$1"\t"$2}'
}

for n in $NS; do
  for mode in point churn; do
    it=200; [ "$mode" = churn ] && it=50
    tag="n=$n mode=$mode"
    run_best "$tag\tlumen-rel-patch"   $PIN "$OUT/lumen_rel" "$n" "$it" "$mode" patch
    run_best "$tag\tlumen-rel-rebuild" $PIN "$OUT/lumen_rel" "$n" "$it" "$mode" rebuild
    run_best "$tag\tlumen-dev-patch"   $PIN "$OUT/lumen_dev" "$n" "$it" "$mode" patch
    run_best "$tag\tlumen-dev-rebuild" $PIN "$OUT/lumen_dev" "$n" "$it" "$mode" rebuild
    run_best "$tag\tqt6"               $PIN "$OUT/qtb/bench5_qt" "$n" "$it" "$mode"
    DISPLAY=${DISPLAY:-:0} run_best "$tag\tgtk4" $PIN "$OUT/gtk" "$n" "$it" "$mode"
  done
done
