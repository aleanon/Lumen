# PROF1 — where Lumen's 8× against iced actually goes (2026-08-19)

BENCH2 established the gap and localised it to "text caching in the steady
state, not the pipeline". That was half right and the wrong half was load
bearing. This is the profile.

**Result in one line:** the gap is not text and not the layout algorithm — it
is that **Lumen materialises every node from scratch on every frame**, and its
incremental path, when engaged, still materialises them.

---

## Method, and its limits

No sampling profiler on this box: `perf_event_paranoid` is 4 (blocks
user-space `perf` without a sysctl change) and valgrind is not installed. So
the method is **subtractive plus primitive pricing**:

1. Read the framework's own counters (`FrameStats.nodes_rebuilt` /
   `nodes_copied`).
2. Build view variants that each remove one candidate cost, and diff the frame
   time.
3. Price the individual operations the hot path performs, standalone, and see
   how much of the measured floor they account for.

What this **cannot** do is attribute the last ~45% of the per-node floor to
named functions. That needs `perf` or instrumentation inside `lumen-app`, and
is flagged as the open item rather than guessed at. Everything below is
measured; nothing is inferred from reading code alone.

All figures: 3000 rows, 400×800, one row's text changing per frame,
`NullRenderer`, release with the workspace profile, idle machine.

---

## Finding 1 — Lumen rebuilds 100% of nodes for a one-row change

`FrameStats`, steady state:

| rows | nodes | rebuilt | copied | rebuilt |
|-----:|------:|--------:|-------:|--------:|
| 100 | 101 | 101 | 0 | 100% |
| 1000 | 1001 | 1001 | 0 | 100% |
| 3000 | 3001 | 3001 | 0 | 100% |

The retained copy-forward path never fires. The reason is structural:
`build_node` only considers `copy_span` when the element is a **memo-hit stub**
(`Element::shared`), and only `cx.scope` produces one. A plain view has none,
so every node is lowered fresh, every frame, forever.

**iced does not have an equivalent opt-in.** Its widget `Tree` persists across
frames and each `Text` widget's shaped `Paragraph` lives in `tree.state`;
`layout()` reuses it unless the text or limits changed. The app author writes
nothing to get that.

## Finding 2 — engaging Lumen's incremental path buys 10%

Same view with `cx.scope_with_deps` per row:

| rows | rebuilt | copied |
|-----:|--------:|-------:|
| 3000 | **2** | **2999** |

The mechanism works exactly as designed — 1500× fewer nodes "rebuilt". The
frame time:

| rows | plain | memoized | gain |
|-----:|------:|---------:|-----:|
| 100 | 114.7 µs | 111.9 µs | 2% |
| 1000 | 796.5 µs | 708.1 µs | 11% |
| 3000 | 2655.0 µs | 2377.1 µs | **10%** |

**Skipping 99.9% of the "rebuild" work makes the frame 10% faster.** That is
the central result of this profile, and it says the label is misleading:
`copy_node` does not reuse a node, it **re-materialises** one. Per copied node
it allocates a fresh `NodeIndex`, mints a **fresh taffy node**, and re-keys
nine side tables from `prev_*` to the new index. What memoization actually
skips is re-running the view closure and re-resolving styles — real, but a
minority of the per-node cost.

## Finding 3 — where the per-node cost goes

Subtractive, 3000 rows:

| variant | frame | per row | Δ |
|---|------:|--------:|---|
| A. text rows (baseline) | 2626.6 µs | 0.876 µs | — |
| B. + explicit width/height | 2496.1 µs | 0.832 µs | text **measurement** = 5% |
| C. empty fixed boxes, no text | 1547.9 µs | **0.516 µs** | all text work = 41% |

So **59% of the frame is per-node cost with no content at all.** Pricing the
operations that floor is made of:

| operation | per node | share of the 0.516 µs floor |
|---|------:|---:|
| taffy `new_leaf` + `Style` clone | 0.108 µs | 21% |
| taffy `compute_layout` | 0.100 µs | 19% |
| 9 hashmap ops (FxHash) | 0.046 µs | 9% |
| `Element` construct + drop | 0.029 µs | 6% |
| **accounted** | **0.282 µs** | **55%** |
| unaccounted (tree arena, `NodeMeta`, style memo, paint emission) | 0.234 µs | 45% |

Two things stand out.

**Taffy is 40% of the floor**, and Lumen rebuilds the whole taffy tree every
frame — including on the memoized path, where `copy_node` calls
`layout.leaf(&lstyle)` per copied node. `new_leaf` alone (0.108 µs) is
**roughly iced's entire per-row cost**.

**The hashmaps are not the problem.** BENCH1 hypothesised that `copy_node`'s
"8 hashmap ops per memo hit" was the constant factor. Measured, they are 9% of
the floor and under 5% of the frame. That hypothesis is retired.

## Finding 4 — what iced does instead

`iced_core::layout::Node` is:

```rust
pub struct Node { bounds: Rectangle, children: Vec<Node> }
```

A plain recursive Vec. No arena, no generational indices, no side tables, no
external constraint solver. A leaf costs one `Rectangle` and an empty `Vec`
(no allocation). The shaped paragraph is not in it at all — it lives in the
persistent `tree.state`, and `layout()` takes `&mut State<Paragraph>` and
re-measures only when the content or limits changed.

Steady-state per row: downcast persistent state, observe nothing changed,
emit a `Node`. **0.107 µs.**

Lumen's, for the same row: construct an `Element` (1072 bytes), insert an
arena node, write nine side tables, mint a taffy node, run the taffy solver
over it, emit paint commands. **0.876 µs.**

---

## Recommendations

Ordered by return, and the ordering matters more than the individual items.

### R1 — Stop re-minting taffy nodes every frame *(largest single win, contained)*

`copy_node` calls `layout.leaf(&lstyle)` / `layout.container(..)` for every
copied node. On a memoized frame the layout style is *known unchanged* — it was
just `remove`d from `prev_layout_style` and re-inserted verbatim. Reusing the
previous taffy node instead of minting a new one removes 0.108 µs/node.

Expected: **~21% of the floor, ~12% of a text frame.** Contained to the copy
path; does not require stable `NodeIndex`.

### R2 — Retained node graph: stable `NodeIndex` across frames *(the real fix)*

This is ADR-007's **F2**, descoped because "incremental layout across disjoint
taffy subtrees is the blocker". This profile is the argument for reopening it:
with a stable index, an unchanged subtree needs *no* work — no arena insert, no
nine-table re-key, no taffy re-mint, no `Element` construction. That is the
only change that gets Lumen to iced's shape rather than a fraction closer to
its constant.

Expected: on a memoized view, unchanged rows approach zero cost, which is what
turns the current 10% memoization gain into something like the 54× iced gets.
Highest risk item here by a wide margin, and R1 is a strict subset of it — do
R1 first and keep it if R2 stalls.

### R3 — Dense side arrays instead of hashmaps

`node_style`, `node_computed`, `node_layout_style`, `meta` and friends are all
keyed by `NodeIndex`, which is a slot number. `Vec<Option<T>>` indexed by slot
turns nine hash lookups into nine array writes.

Expected: **~9% of the floor.** Mechanical, low risk, no architectural
commitment — but do not expect it to move the headline; it is a cleanup that
happens to pay.

### R4 — Make memoization automatic, but **not before R1/R2**

The obvious reading of Finding 1 is "make `cx.scope` implicit". Finding 2 says
that is worth 10% today, because the copy path re-materialises anyway. Doing R4
first would spend the API-design budget for a tenth of the available win, and
would make the framework's default path depend on a mechanism that is not yet
carrying its weight. After R1+R2 it becomes the thing that unlocks them.

### R5 — Do **not** chase text

Text is 41% of the frame, which makes it the tempting target. It is the wrong
one: BENCH2's cache-denied measurement already showed Lumen at parity with
iced and 6% faster at 3000 rows when both caches are cold, so the text
pipeline itself is competitive. Of the 41%, layout measurement is 5%; the rest
is shaping-cache lookup and glyph-run emission, which scale with what is
actually drawn. Optimising here would move a number that is already sound.

---

## Reproducing

```sh
cd benches-competitive
cargo run --release --bin probe_stats    # Findings 1 and 2 (node counters)
cargo run --release --bin probe_phases   # Finding 3 (subtractive)
cargo run --release --bin probe_ops      # Finding 3 (primitive pricing)
cargo bench --bench vs_iced              # the frame times, incl. lumen_memoized
```

## Open

* **The unaccounted 45%** of the per-node floor — tree arena, `NodeMeta`
  construction, style-memo lookup, paint-command emission. Splitting it needs
  `perf` (`sysctl kernel.perf_event_paranoid=1`) or temporary phase timers
  inside `lumen-app`. Everything above is measured; this is the one number
  this profile could not attribute, and it is the largest single bucket.
* Whether R1 is safe when a copied node's *parent* changed size — taffy nodes
  carry parent links, so reuse may need the parent unchanged too. Not
  investigated.
