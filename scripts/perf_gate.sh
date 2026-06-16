#!/usr/bin/env bash
# T2.6 perf gate: run the criterion benches and fail if any mean exceeds its
# absolute budget. Criterion also tracks per-run change (±%) against its saved
# baseline in target/criterion, giving the ±10% regression signal between runs.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> running benches"
cargo bench -p lumen-benches --bench perf -- --noplot

echo "==> checking budgets"
python3 - <<'PY'
import json, os, sys

# bench name -> budget in nanoseconds
budgets = {
    "layout_10k_dirty_subtree": 2_000_000,   # < 2 ms
    "vlist_1m_scroll": 8_333_333,            # < 8.33 ms (120 fps frame budget)
    "data_grid_1m_scroll": 8_333_333,        # < 8.33 ms (1M-row DataGrid, T4.2)
    "idle_frame": 2_000_000,                 # < 2 ms (idle does no real work)
}

fail = False
for name, budget in budgets.items():
    est = f"target/criterion/{name}/new/estimates.json"
    if not os.path.exists(est):
        print(f"FAIL  {name}: no estimates ({est})")
        fail = True
        continue
    mean = json.load(open(est))["mean"]["point_estimate"]
    ok = mean <= budget
    fail = fail or not ok
    print(f"{'PASS' if ok else 'FAIL'}  {name:<26} {mean/1e6:7.3f} ms  "
          f"(budget {budget/1e6:.3f} ms)")

sys.exit(1 if fail else 0)
PY
