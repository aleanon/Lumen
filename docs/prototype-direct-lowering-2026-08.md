# Prototype: lowering widgets straight into the tree

**Branch** `exp/widget-trait` · **Run** 2026-08-25/26 · **Status** three prototypes, no blocker found

Follows `experiment-widget-trait-2026-08.md`, which measured a `Widget` trait
that still produced an `Element`. This records the three prototypes that
followed, testing whether the `Element` can go away entirely.

## The design

Today a widget produces an `Element` — 1072 bytes, uniform — and `build_node`
reads 41 of its fields back out into the two structures that keep the data: the
SoA `Tree` and a per-node `NodeMeta` side table. The `Element` is then dropped.

The prototype removes that staging record. A widget receives the sink and writes
its own fields in:

```rust
pub trait Direct {
    fn lower(self, out: &mut TreeSink, parent: Option<NodeIndex>) -> (NodeIndex, LayoutNode);
}
```

Nothing uniform is materialized, so a widget costs what its own data costs.

**The agent never needed `Element`.** `lumen-agent` has zero references to it —
it reads `SemanticsNode`, derived from the side table. Observability is
unaffected by any of this.

## Prototype 1 — does removing the staging record pay?

`direct.rs` + `lowercost.rs` + `lowerprobe.rs`. Both paths end at the same
destination writes, and `lowered_eq` asserts equivalent trees *before* either is
timed — a faster path that skipped work fails the guard rather than posting a
number. It caught a missing `part("fill")` class on its first run.

| 500 rows / 2501 nodes | via `Element` | direct | |
|---|---:|---:|---|
| allocations | 8 717 | 6 715 | **−23.0%** |
| total bytes | 13.23 MB | 10.47 MB | −20.9% |
| peak live bytes | 9.07 MB | 8.01 MB | −11.7% |
| lowering time, unstyled | — | — | **−9.8%** |
| lowering time, styled | — | — | **−10.9%** |

**This corrected an earlier claim.** The phase split shows why the peak barely
moves: building the staging tree peaks at 2.63 MB, but walking it into the sink
peaks at 5.07 MB. The destination is roughly twice the staging buffer and does
not shrink. The earlier "3.07 MB, 16.8% of RSS, 6.4× reduction" projection
assumed the whole staging cost was recoverable. It is not — most of what an
`Element` holds (label `String`s, `Vec`s, `Rc` handlers) must be allocated
either way; it just moves straight into the side table instead of via a
1072-byte wrapper.

## Prototype 2 — can the `.lss` cascade compose instead of mutate?

`build_node` runs the cascade by writing into the element between the widget and
taffy (`apply_css_to_element(&mut el, &css)`). With no element there is nothing
to write into.

**It composes.** `apply_css_to_element` was already a pure function from `Style`
onto a target; splitting the target into `(LayoutStyle, Meta)` is mechanical.
`tests/direct_cascade.rs` holds a plain rule reaching layout and paint,
descendant selectors (the ancestor chain survives a builder because `resolve`
pushes and `end` pops), inherited `:disabled`, class-vs-id specificity, and
sheet-over-widget override.

| | Element path | direct path |
|---|---:|---:|
| cascade cost | 366 ns/node | 238 ns/node |

**It also found a real regression.** With an `Element` the engine ran the
cascade centrally and a widget could not get the ordering wrong. Writing
straight into the tree hands each widget the obligation to call `resolve()`
after declaring everything a selector matches on. Get it backwards and the node
is **silently unstyled** — no panic, no diagnostic. `ProgressBar` shipped with
exactly that inversion in the prototype.

## Prototype 3a — making the ordering unrepresentable

Two type states, each exposing only what is legal in it:

* **`Declaring`** — cascade inputs (`id`, `class`, `states`, `disabled`, `text`);
  no `child`.
* **`Open`** — `child` and `end`; none of the matchable setters.

`resolve` is the only transition; both guards are `#[must_use]`. Three mistakes
stopped compiling, each pinned by a `compile_fail` doctest: a child before the
cascade runs, a class declared after it, and ending without resolving.

A fourth — beginning a node and never ending it — cannot be caught by type
states, so `assert_balanced()` catches it positively. That failure mode matters:
an unended node sits in the tree with **no side-table record**, so it is
invisible to semantics, the agent and assistive tech.

The guards are compile-time only; the numbers were unchanged with them in.

## Prototype 3b — memoization without a cloneable `Element`

The question that could have killed the design. `cx.scope` memoization is the
one place the engine genuinely *retains* an `Element`: a memo-hit stub carries
`shared: Option<Rc<Element>>`.

**Reading `splice_span` settles it — the fast path never touches `Element`.** A
memo hit is `detach` + `attach_last_child` on the retained tree, both O(1) since
the child list is doubly linked. The `Rc<Element>` exists only for the fallback
when splicing is *refused* (the span's root died, or it contains an animating
node). With no `Element` to re-lower, that fallback becomes "run the closure
again" — which is what a cache miss already does, and is always sound because a
scope closure is pure by ADR-013.

`tests/direct_memo.rs` holds five properties: an unchanged scope is spliced not
rebuilt, only the changed scope rebuilds, **the memoized tree is identical to a
full rebuild**, splicing preserves sibling order, and the tree does not grow
across frames.

Measured, 500 scopes with one dirty:

| | median | frame composition |
|---|---:|---|
| full rebuild | 415 µs | 500 rebuilt, 1501 nodes freed |
| memoized | **11 µs** | 499 spliced, 1 rebuilt, 1497 reused, 4 freed |

**~35× on a one-row change, with genuine O(changed) composition.**

The balance check from prototype 3a immediately earned its keep here: it caught
a leak of 601 ancestor entries, because with no stylesheet loaded `resolve`
pushed an ancestor but stored no style, so `end` never popped.

## Measurement notes

**Three numbers on this branch were artifacts, and the method had to change.**
Both lowering paths allocate ~10 MB per frame, so criterion was timing allocator
residue: the *same* `lower_direct` measured **941 µs and 2.71 ms** in two groups
of one binary. Timing moved to one-arm-per-process binaries (`lowertime`,
`memotime`) with their own warmup, median of many samples, repeated 9×.

Two harness flaws also inverted results before being caught:
* children passed as `Vec<Box<dyn FnOnce>>` — overhead the real design would
  never have; it showed −10.3% where the fixed harness showed −33%.
* the sink reaching records through `meta.get_mut(&n)` on *every* property
  setter — eight hashed lookups for a `Button` — which made direct lowering
  slower than the path it was meant to beat, until an open-node stack replaced it.

Deterministic metrics (allocation counts, frame composition) are the trustworthy
signal here. Timings carry 6–25% run-to-run spread and are directional only.

## Where this leaves the design

No blocker found across three prototypes.

**For:** ~23% fewer allocations, ~10% faster lowering, a cheaper cascade, widgets
carrying only their own data, memoization intact and O(changed), observability
untouched, and the ordering hazard now unrepresentable.

**Against:** `AppSnapshot` and the golden tests that read `Element` fields would
need rework, and the engine conversion is large — `build_node` is ~500 lines
wired into overlays, damage, text measurement and transitions that these
prototypes deliberately excluded.

**Not yet prototyped:** overlay/z routing, damage tracking, text measurement
feeding back into layout, and `@keyframes`/transition blending — all of which
`build_node` currently does against a mutable `Element`.

## Scope

Three widgets (`Label`, `Button`, `ProgressBar`) plus container rows, writing
into the real `lumen-core::Tree` and `lumen-layout::LayoutTree`. `lower_element`
is a faithful reduction of `build_node`'s writes, not `build_node` itself.

Instruments: `benches/benches/lowercost.rs`, `benches/src/bin/lowerprobe.rs`,
`benches/src/bin/lowertime.rs`, `benches/src/bin/memotime.rs`.
Tests: `direct_cascade.rs` (10), `direct_memo.rs` (5), `third_party_widget.rs` (4).
