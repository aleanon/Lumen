#!/usr/bin/env bash
# BENCH5 stage profiling.
#
# The C and C++ harnesses time their own stages directly, which is strictly
# better than sampling. Lumen cannot: its stages are private functions inside
# `pump()`, and instrumenting them would mean editing the framework to measure
# it. So Lumen's split comes from `perf` INCLUSIVE attribution instead, which is
# non-invasive and uses the debuginfo the release profile already emits
# (benches-competitive/Cargo.toml sets `debug = true`; debuginfo does not change
# codegen, so the profiled binary is the one the benchmarks time).
#
# `--children` is the important flag: it reports each symbol's INCLUSIVE cost,
# so a stage root accounts for everything beneath it. Sampling is at 2 kHz with
# DWARF unwinding, because thin-LTO inlines aggressively and frame-pointer
# unwinding loses the stage roots entirely.
set -uo pipefail
export LC_ALL=C
OUT=${OUT:-/tmp/bench5}
CPU=${CPU:-2}
mkdir -p "$OUT/perf"

# Stage roots, in pipeline order. The regexes are anchored on the symbols the
# Lumen pipeline actually goes through; see crates/lumen-app/src/app.rs.
LUMEN_STAGES=(
  "build-closure|probe_bench5.*closure"
  "lower(build_node)|build_node"
  "layout(taffy)|taffy::|compute_layout|lumen_layout"
  "text-shape|lumen_text|parley|swash|skrifa"
  "display-list(paint)|Headless.*::paint|emit_"
  "semantics|build_semantics|semantics"
  "ambient-audit(lint)|ambient_audit|::lint|audit_"
  "patch-bindings|patch_text_bindings|settle_bindings"
)

profile_lumen() {
  local bin="$1" tag="$2"; shift 2
  local data="$OUT/perf/$tag.data"
  perf record -q -F 2000 --call-graph=dwarf,16384 -o "$data" -- \
      taskset -c "$CPU" "$bin" "$@" >/dev/null 2>&1
  echo "## $tag  ($*)"
  local rpt; rpt=$(perf report -i "$data" --children --sort symbol --stdio --percent-limit 0 2>/dev/null)
  local total; total=$(echo "$rpt" | grep -c . )
  for entry in "${LUMEN_STAGES[@]}"; do
    local name="${entry%%|*}" re="${entry#*|}"
    # Take the LARGEST inclusive percentage among matching symbols rather than
    # summing: summing would double-count a caller and its callee, which are
    # both inclusive and both match a broad regex.
    local pct
    pct=$(echo "$rpt" | grep -E "$re" | awk '{gsub("%","",$1); if ($1+0 > m) m=$1+0} END {printf "%.1f", m}')
    printf "  %-24s %6s%%\n" "$name" "${pct:-0.0}"
  done
  echo
}

# Same instrument on the C/C++ side, at library granularity. Distribution Qt and
# GTK ship stripped, so symbol-level attribution is not available; DSO-level
# still answers "where does the frame go", which is the question.
profile_dso() {
  local tag="$1"; shift
  local data="$OUT/perf/$tag.data"
  perf record -q -F 2000 -o "$data" -- taskset -c "$CPU" "$@" >/dev/null 2>&1
  echo "## $tag  ($*)"
  perf report -i "$data" --sort dso --stdio --percent-limit 1 2>/dev/null \
    | grep -E '^\s+[0-9]' | head -10 | sed 's/^/  /'
  echo
}

N=${N:-3000}
echo "===== Lumen stage attribution (perf --children, inclusive) ====="
profile_lumen "$OUT/lumen_rel" "lumen-rel-point-rebuild" "$N" 300 point rebuild
profile_lumen "$OUT/lumen_rel" "lumen-rel-point-patch"   "$N" 300 point patch
profile_lumen "$OUT/lumen_rel" "lumen-rel-churn-rebuild" "$N" 60  churn rebuild
profile_lumen "$OUT/lumen_dev" "lumen-dev-point-patch"   "$N" 300 point patch
profile_lumen "$OUT/lumen_dev" "lumen-dev-churn-rebuild" "$N" 60  churn rebuild

echo "===== Qt / GTK, shared-object attribution ====="
profile_dso "qt-point"   "$OUT/qtb/bench5_qt" "$N" 200 point
profile_dso "qt-churn"   "$OUT/qtb/bench5_qt" "$N" 50  churn
DISPLAY=${DISPLAY:-:0} profile_dso "gtk-point" "$OUT/gtk" "$N" 200 point
DISPLAY=${DISPLAY:-:0} profile_dso "gtk-churn" "$OUT/gtk" "$N" 50  churn
