# N0 measurement results — the node-cost thesis is falsified

*Measured 2026-08-05. Instruments: `benches/benches/nodecost.rs` (new) +
`benches/benches/perf.rs`. Host: i9-13900KF, 32 threads, release, one clean
full-suite run. Android: `lumen34` AVD, x86_64 + KVM, 4 cores, 1536 MB.*

This document exists because `docs/plan-node-cost.md` sequenced an XL phase off a
phase table that could not be true — its rows summed to **994 µs** against a
**773 µs** measured frame, with the raster row silently dropped. A performance
review flagged it as unfalsifiable with the instruments in the repo. N0 was built
to settle it. **Every headline claim in the plan failed.**

---

## 1. Desktop baseline (regenerated, one run, generated not transcribed)

| bench | mean |
|---|---|
| `idle_frame` | 42.70 ns |
| `signal_update_large_vec` | 16.57 ns |
| `id_addressing/1k_rows_typed_key` | 18.83 µs |
| `id_addressing/1k_rows_string_key` | 51.54 µs |
| `scope_memo_one_of_many` (200 nodes, 1 dirty) | 374.74 µs |
| `layout_10k_dirty_subtree` | 406.64 µs |
| `cull_100k` | 577.88 µs |
| **`text_list_changed_frame`** (500 nodes, all dirty) | **776.17 µs** |
| **`text_list_scoped_changed_frame`** (500 nodes, **1** dirty) *(new)* | **1 114.0 µs** |
| `vlist_1m_scroll` | 1 147.7 µs |
| `data_grid_1m_scroll` | 1 774.5 µs |

`cull_100k` and `data_grid_1m_scroll` had **no criterion artifacts** before this
run, so `scripts/perf_gate.sh` was failing both on its missing-estimates branch —
the gate was red as committed. It now has data.

---

## 2. The headline result: memoization is a pessimization

| | frame | nodes | dirty | µs/node |
|---|---|---|---|---|
| `text_list_changed_frame` | 776.2 µs | 500 | **500/500** | 1.552 |
| `text_list_scoped_changed_frame` | 1 114.0 µs | 500 | **1/500** | 2.228 |
| `scope_memo_one_of_many` | 374.7 µs | 200 | **1/200** | 1.874 |

**Rebuilding all 500 rows is 1.44× faster than rebuilding one row and reusing 499
memoized subtrees.** Same node count, same tree shape, same signal traffic.

Allocations confirm it and are deterministic (identical on desktop and Android):

```
flat,   all 500 dirty :  2 952 allocs   5 006 KiB   (5.9 allocs/node)
scoped, 1 of 500 dirty:  5 459 allocs   5 356 KiB   (1.85× MORE)
idle pump             :      0 allocs       0 KiB
```

The "incremental" path allocates **85 % more** than the full rebuild. `copy_node`
(`app.rs:2788`) still performs, per *copied* node, 4 `HashMap::remove` + 4
`insert` + `root_map.insert`, a `LayoutStyle::clone()`, and a fresh taffy node.
Copy-forward avoids re-running the closure and re-resolving the cascade; it does
not avoid the per-node work that actually costs.

This is the single most important finding, and it inverts the plan: **the largest
win available is not making lowering faster, it is making the incremental path
stop costing more than not being incremental at all.** Merely bringing the scoped
path level with the flat path is a 1.44× win — larger than N1's projected
2×-on-lowering delivers end-to-end (§5).

---

## 3. Finer `cx.scope` granularity makes frames slower

600 nodes held constant; only the scope count varies; exactly one scope dirty.

| scopes | nodes/scope | desktop | Android emu | marginal µs per added scope |
|---|---|---|---|---|
| 10 | 60 | 1 027.6 µs | 2 056.4 µs | — |
| **50** | 12 | **804.3 µs** ← min | **1 841.1 µs** | — |
| 100 | 6 | 864.3 µs | 1 982.4 µs | **1.20** |
| 200 | 3 | 1 041.2 µs | 2 355.3 µs | **1.77** |
| 300 | 2 | 1 381.8 µs | 3 480.5 µs | **3.41** |

A U-curve. The left arm is F1 working as designed — finer scopes mean fewer nodes
re-run. Past ~50 scopes it reverses, and **the marginal cost of each added scope
is itself rising (1.20 → 1.77 → 3.41 µs)**, which is the signature of a quadratic
term, not a linear one. That is the `copy_span` filter at `app.rs:2754`:

```rust
let nested: Vec<(IdHash, SpanRec)> = self.prev_spans.iter()
    .filter(|(k, r)| **k != key && prev_nodes.contains(&r.root))
```

run once per memo-hit scope, iterating **all** spans with a linear `contains` —
O(scopes² × span).

**This contradicts the framework's own authoring guidance.** The F-series tells
authors to add `cx.scope` for granularity; past ~50 scopes that makes frames
slower at constant node count. 50 → 300 scopes costs **1.72×** on desktop and
**1.89×** on Android.

---

## 4. What the frame is actually made of

No single phase dominates. Measured, not derived:

| component | how measured | share of 500-node frame |
|---|---|---|
| text shaping + glyph raster | `text_leaves` 794.1 µs − `rect_leaves` 629.0 µs = 165.2 µs | **~21 %** |
| hashing (SipHash in the side tables) | Fx-hasher A/B, §6 | **~9–14 %** |
| allocation | 2 952 allocs × 25 ns ≈ 74 µs | **~10 %** |
| layout (taffy) | inherited, July | ~8 % |
| everything else | residual | ~50 % |

The plan claimed lowering was **58 %** of the frame. Nothing measures above ~25 %.
The cost is broadly distributed, which is the worst possible shape for a plan
built around one dominant phase.

Note both leaf variants are heavily culled (500 rows overflow a 400×400 window),
so this is *not* a raster-dominated comparison — the 629 µs rect frame is
essentially the non-text pipeline cost for 500 nodes.

---

## 5. Amdahl: the plan cannot deliver what it promises

Using the plan's own best case, N1 at exactly 2× on lowering, against the
measured 776 µs frame and the measured ~10 % allocation share:

| | frame | speedup | % of 16.67 ms |
|---|---|---|---|
| today | 776 µs | — | 4.66 % |
| N1 at 2× on a 25 %-of-frame phase | 679 µs | 1.14× | 4.07 % |
| **fix the scoped path to merely match flat** | **776 µs** (from 1 114) | **1.44×** | — |

Fixing the regression in §2 is worth more than the S-plus phase, and far more
than N3's XL.

---

## 6. The hasher A/B (experiment, reverted — not committed)

Every `HashMap` in `app.rs` was switched to an Fx-style hasher via a type alias
(~30 lines, no new dependency, no SoA refactor, no dense/sparse classification, no
generation stamps). Measured, then reverted:

| bench | SipHash | Fx | Δ |
|---|---|---|---|
| `text_list_scoped_changed_frame` | 1 118.5 µs | 1 015.8 µs | **−9.2 %** |
| `scope_scaling/50` | 791.4 µs | 688.1 µs | **−13.1 %** |
| `scope_scaling/100` | 852.6 µs | 732.5 µs | **−14.1 %** |
| `scope_scaling/200` | 1 055.4 µs | 916.5 µs | **−13.2 %** |
| `scope_scaling/300` | 1 391.1 µs | 1 266.6 µs | **−9.0 %** |

Allocation counts unchanged, as expected. **9–14 % for a type alias** — a third of
what a review hypothesised, but a far better ratio of win to risk than N1.1–N1.7.
Not committed: it changes hash iteration order, which needs a golden/semantics
audit first.

---

## 7. Mobile (first-class per project direction)

`nodecost` cross-compiled with `cargo ndk -t x86_64` and run on the `lumen34` AVD:

| bench | desktop | Android emu | ratio |
|---|---|---|---|
| `text_list_scoped_changed_frame` | 1 114.0 µs | 2 428.9 µs | **2.18×** |
| `scope_scaling/50` | 804.3 µs | 1 841.1 µs | 2.29× |
| `scope_scaling/300` | 1 381.8 µs | 3 480.5 µs | **2.52×** |

**Methodological limit, stated plainly:** this AVD is **x86_64 under KVM on the
i9 host**, so the 2.2–2.5× is virtualization, 4-core scheduling and bionic — it is
**not** an ARM CPU gap. A real mid-range ARM phone is plausibly another 2–3×
slower again. This measurement is a **floor**, not an estimate.

Extrapolating with that stated uncertainty, a 500-node scoped frame lands at
roughly **5–7 ms** on a mid-range phone: 30–42 % of a 60 fps budget, **60–84 % of
a 120 fps budget**. Mobile does not make the current numbers comfortable. But the
fix indicated is §2 and §3 — the incremental path — not N1's SoA or N3's retained
graph, because the mobile multiplier applies to the *pessimization* too.

**To actually decide N3, this needs a real ARM device.** The emulator cannot
answer it.

---

## 8. What replaced N1–N5

`docs/plan-incremental-path.md` (CP-series), written from this evidence.
`plan-node-cost.md` is retired in place. The ordering below is what that plan
implements:

1. **Fix `copy_node`/`copy_span`** so a memo hit is cheaper than a rebuild. §2 and
   §3 are the same defect seen two ways, it is worth ~1.44× on the shape the
   framework tells authors to write, and it is bug-shaped, not architecture-shaped.
2. **Hasher swap** (§6) — 9–14 % for a type alias, gated on a golden audit.
3. **Re-measure, then decide** whether anything from N1 survives. On current
   evidence the SoA refactor targets ~10 % and the retained graph (N3, XL) is not
   justified on any hardware measured.
4. **Fix the perf gate** independently: it was red, its budgets are absolute
   nanoseconds baked from one box, and its ±15 % band is ~7× the measured ±2.2 %
   noise floor. Criterion already computes change detection the script's own
   header claims to use and never reads.

## Reproducing

```
cargo bench -p lumen-benches --bench nodecost          # all four instruments
source ~/android-env.sh && emulator -avd lumen34 -no-window -no-audio &
cargo ndk -t x86_64 bench -p lumen-benches --bench nodecost --no-run
adb push target/x86_64-linux-android/release/deps/nodecost-* /data/local/tmp/
adb shell "cd /data/local/tmp && ./nodecost --bench --noplot"   # --bench required
```
