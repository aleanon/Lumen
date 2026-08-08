#!/usr/bin/env bash
# T2.6 + CP0 perf gate.
#
# Two kinds of budget, because they fail for different reasons:
#
#   * ABSOLUTE budgets (T2.6) — "a frame must fit in 8.33 ms". Semantic
#     ceilings, meaningful only on a machine whose speed you know.
#   * RATIO budgets (CP0) — "the memoized path must not be slower than the
#     full rebuild it replaces". Machine-independent, because both sides move
#     together, so these are the ones that mean anything on a shared runner.
#
# CP0 exists because the gate previously ran ONLY `--bench perf`, whose five
# budgets cover none of the campaign's actual subject matter. The framework's
# own measurement (docs/results-node-cost-n0.md) found the "incremental" scoped
# path 1.44x SLOWER than rebuilding everything — a regression of the flagship
# feature that no gate could have caught, because the benches that measure it
# were never run in CI.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> running benches"
cargo bench -p lumen-benches --bench perf -- --noplot
cargo bench -p lumen-benches --bench nodecost -- --noplot
cargo bench -p lumen-benches --bench identity -- --noplot

echo "==> checking budgets"
python3 - <<'PY'
import json, os, sys

def mean(path):
    """Criterion mean in ns, or None if the bench didn't run."""
    est = f"target/criterion/{path}/new/estimates.json"
    if not os.path.exists(est):
        return None
    with open(est) as f:
        return json.load(f)["mean"]["point_estimate"]

fail = False

# --- absolute budgets (T2.6): bench name -> ns -------------------------------
absolute = {
    "layout_10k_dirty_subtree": 2_000_000,   # < 2 ms
    "vlist_1m_scroll": 8_333_333,            # < 8.33 ms (120 fps frame budget)
    "data_grid_1m_scroll": 8_333_333,        # < 8.33 ms (1M-row DataGrid, T4.2)
    "cull_100k": 5_000_000,                  # < 5 ms (100k-node parallel cull, T6.6)
    "idle_frame": 2_000_000,                 # < 2 ms (idle does no real work)
}

print("-- absolute --")
for name, budget in absolute.items():
    m = mean(name)
    if m is None:
        print(f"FAIL  {name}: no estimates")
        fail = True
        continue
    ok = m <= budget
    fail = fail or not ok
    print(f"{'PASS' if ok else 'FAIL'}  {name:<26} {m/1e6:7.3f} ms  "
          f"(budget {budget/1e6:.3f} ms)")

# --- ratio budgets (CP0) -----------------------------------------------------
#
# Seeded just above the values measured on 2026-08-08, so the gate lands green
# and catches regressions from here. They are NOT targets — they are ceilings
# on today's behavior. Each CP phase is expected to tighten them; the campaign
# plan's exit criterion for M-C is scoped_vs_flat <= 1.0.
ratios = [
    # (label, numerator, denominator, ceiling, note)
    ("scoped_vs_flat",
     "text_list_scoped_changed_frame", "text_list_changed_frame", 0.90,
     "memoizing 499 of 500 rows vs rebuilding all 500. >1.0 means the "
     "incremental path costs more than it saves, which is where this campaign "
     "started: 1.442. After OB1/OB3 1.389, after the FxHash swap 1.307, and "
     "after CP1 removed copy_span's O(S^2*C) nested-span scan, 0.79 — the "
     "memoized path is finally FASTER than the full rebuild it replaces. "
     "Ceiling tightened to 0.90 to lock that in; M-C's <=1.0 target is met"),
    ("scope_scaling_300_over_50",
     "scope_scaling_600_nodes/300_scopes", "scope_scaling_600_nodes/50_scopes", 1.55,
     "same node count, 6x the scopes; >1.0 means finer granularity costs more. "
     "1.72 at campaign start, 1.44 after CP1. Still >1.0: the residual is "
     "copy_node's per-node bookkeeping (8 hashmap ops + a LayoutStyle clone "
     "per memo hit), which CP2.1/CP2.2 target"),
]

print("-- ratios --")
for label, num_name, den_name, ceiling, note in ratios:
    num, den = mean(num_name), mean(den_name)
    if num is None or den is None:
        missing = num_name if num is None else den_name
        print(f"FAIL  {label}: no estimates for {missing}")
        fail = True
        continue
    r = num / den
    ok = r <= ceiling
    fail = fail or not ok
    print(f"{'PASS' if ok else 'FAIL'}  {label:<26} {r:5.3f}  (ceiling {ceiling:.2f})")
    if not ok:
        print(f"      {note}")

# --- allocation assertions ---------------------------------------------------
#
# identity.rs asserts zero allocations for typed-key addressing via a counting
# allocator, and panics if that regresses. Running the bench at all is the
# check; this just confirms it ran, since a silently-skipped bench would let a
# regression through as "no news".
print("-- allocation guards --")
for name in ("id_alloc/1k_rows_typed_key_allocs", "allocs/scoped_one_dirty_allocs"):
    if mean(name) is None:
        print(f"FAIL  {name}: did not run (its in-bench assertions were skipped)")
        fail = True
    else:
        print(f"PASS  {name} ran")

sys.exit(1 if fail else 0)
PY
