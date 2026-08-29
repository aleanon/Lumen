# Plan: close the gap to Qt/GTK

Execution plan for the five options in
`investigation-closing-the-gap-2026-08.md`. Ordered by measured value per unit
of risk, not by ambition.

## T1 — Line metrics without shaping  *(prerequisite for T2)*

`lumen-text` can only tell you a line's height by shaping a string. A text
node's height, for single-line unwrapped text, is a property of the **font and
size**, not of the glyphs — so it can be answered once per distinct
`TextStyle` and cached.

* `TextEngine::line_height_of(&mut self, style) -> f32`, memoised on the same
  key shape `ShapeKey` uses minus the text.
* Cost is O(distinct styles), which is a handful, not O(nodes).
* **Done when** the value equals `shaped(<any text>).height()` for the same
  style, asserted over a range of sizes and weights.

## T2 — Deferred text measurement  *(the 86%)*

Skip shaping at layout time for a text node whose intrinsic size nothing
consumes; let paint shape the visible rows.

Qualifies when **all** hold:
* single line — no wrap width, no ellipsis, no `\n`;
* the node's width is **decided by an ancestor**, not by its content.

The second is the whole difficulty and is the same definite/indefinite
containing-block question L1's rejected workaround failed on. Propagate during
lowering:

```
definite_width(n) = width is Px
                 || (width is Percent && parent_definite)
                 || (parent_stretches_width && parent_definite)
```
where `parent_stretches_width` is a flex column with stretch cross-alignment,
and the root is definite only if it declares a width.

* Height from T1; width left `Auto` so the parent's stretch decides it.
* **Done when** painted output is pixel-identical on the corpus and the
  benchmark shows shapes/frame collapsing from N to the visible-row count.
* **Risk, already measured:** applied without the guard this fails 121/1173
  tests. The guard is what makes it correct, so the test corpus is the
  acceptance criterion, not a formality.

## T3 — ~~Parallel shaping~~ → **report the cliff**  *(revised on evidence)*

The plan said parallel shaping. A coverage census after T2 changed the answer,
and the plan follows the measurement rather than the other way round.

Instrumenting *why* each text node still shapes: under a container with a
definite width, **100% of labels defer and none shape**. Under a content-sizing
container, **0% defer and all of them shape — for one reason, an indefinite
containing block.** T2's coverage is not a gradient to be squeezed with
parallelism; it is a cliff with a single cause.

That makes parallel shaping the wrong next move. It is a large change — parley
borrows `&mut FontContext` to shape, so it needs one font collection per thread,
plus a pool that `rayon` would supply and ADR-003's whitelist does not — and it
would speed up work that should not be happening.

Making the root fill the viewport by default was tested and **rejected**: it
takes the benchmark to 100% coverage but breaks **57 tests**, because what the
root does is the author's decision, not the framework's.

So T3 is: **make the cliff visible.** `W0404` reports, once per frame with a
count, that N labels were shaped during layout because a container above them
sizes to its content — and names the one-line fix. The cost is otherwise
undetectable: the layout is correct, the tree looks healthy, and the app is
simply slower with nothing saying why. That is precisely the class of defect the
observability tier exists for.

* **Done when** a content-sized list of 200 reports once with the count, a
  definite-width list reports nothing, and a four-item shrink-to-fit box is not
  treated as noise.

## T3-old — Parallel shaping  *(deferred, not dropped)*

Wrapped and content-sized text still shapes at layout time. Shaping is pure, so
parallelism cannot change the result — ADR-002's determinism holds by
construction.

* Collect the frame's unshaped runs, shape them on a pool, then proceed.
* **Done when** a wrapped-text benchmark improves and goldens are byte-identical.

## T4 — Viewport culling  *(what closes 100 000 nodes)*

After T2 the remaining cost is ordinary lowering, ~1 µs/node. Qt and GTK avoid
it by not painting offscreen widgets.

Constraint from principle 2: the semantics tree is the accessibility tree and
the agent's view, so **offscreen nodes must still exist**. Only layout and
paint may be skipped, and layout is what determines what is offscreen — so this
needs incremental top-down layout that stops resolving precisely once past the
viewport. Deferred to its own investigation; the design is not obvious.

## T5 / T6 — Already in flight

Direct lowering (O0.16–O0.24) is costed at 9–11% of a frame and half-built.
Retained subtrees already exist as scope memoisation; this benchmark defeats it
deliberately by changing every row.

---

**Order: T1 → T2 → T3.** T4 and beyond only after T2's win is banked, because
T2 changes what the remaining profile looks like.
