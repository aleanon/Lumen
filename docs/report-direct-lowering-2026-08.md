# Report: direct lowering, and the three optimizations it unblocked

**Branch** `exp/widget-trait` · **2026-08-25/26** · **1128 workspace tests, clippy clean**

Two rounds of prototyping asked whether `Element` — the uniform 1072-byte record
a widget produces and `build_node` immediately reads back apart — can be
removed, with widgets writing straight into the retained SoA `Tree` and its
per-node side table. **No blocker was found.**

A third round then did what the first two only argued for: because the
widget→engine contract had become "call these methods" rather than "produce
this struct", the *destination* could be changed without touching a single
widget. Those three changes are each larger than direct lowering itself.

---

## Headline

| | before | after |
|---|---:|---:|
| lowering, 2501 nodes | 1256 µs | **949 µs** (−24.4%) |
| allocations per node (id + class) | 5.13 | **0.13** (38×) |
| side table, bytes/node | 672 | **179** (3.75×) |
| semantics walk, 20k nodes | 189.4 µs | **14.6 µs** (13×) |
| memoized frame, 500 scopes 1 dirty | 445 µs full | **15–30 µs** |

---

## The idea

```
before:  widget → Element (1072 B, uniform) → build_node reads 41 fields → Tree + side table
after:   widget → TreeSink → Tree + side table
```

`Element` was pure marshalling. Every field it held was copied into the SoA
tree, taffy, or the side table, then dropped.

**The agent never read it.** `lumen-agent` has zero references to `Element`; it
reads `SemanticsNode`, derived from the side table. Observability was never at
stake in any of this.

---

## Round 1–2: is the removal possible?

Seven prototypes. Each load-bearing behaviour survived; two only after a design
was wrong first.

| | finding |
|---|---|
| lowering | works — allocations −23%, and it is the *enabling* change |
| cascade | composes; `apply_css_to_element` was already a pure function onto a target |
| ordering | a real hazard (silently unstyled nodes) made **unrepresentable** by type states |
| memoization | survives — the splice fast path never touched `Element` |
| text measurement | works — three inputs meet at `end()` instead of in a mutated element |
| overlay / memo context | **found a real bug** — spans reused across changed surroundings |
| transitions | **found a deadlock** — the refusal that prevents a freeze caused one |
| damage | unaffected — it diffs display lists, downstream of the tree |
| hot reload | works, **found a real bug**, and costs one extra frame |

### The constraint worth carrying forward

Transitions coupled to memoization deadlocked: nodes were marked `animating`
during `resolve`, but a node only resolves if its span was *not* spliced, and a
span is only refused if the node is marked. The first memoized frame spliced the
animating node and the transition froze at frame zero — the exact failure the
check exists to prevent, caused by the check.

The engine's `span_has_running_anim` avoids it by testing the **retained meta's
id against an animation registry held in engine state**, populated by whatever
started the transition — never by the build.

> **Animation state must be keyed independently of the build.** Derive it from
> the build and it cannot bootstrap.

### Hot reload: a structural cost, not a bug

`set_stylesheet` carries the line that decides it:

```rust
self.style_memo.clear();   // "scope caches stay: cached Elements are pre-styling"
```

In the `Element` model a memoized scope holds **unstyled** elements, so a sheet
edit invalidates only the resolution cache and no closure re-runs. Direct
lowering inverts that: a retained span is finished, already-styled nodes, so an
edit makes every span stale.

Demonstrated before fixing — without a sheet generation in the splice guard, all
rows spliced and stayed blue after the sheet said red. **Hot reload silently
doing nothing**, which is the worst failure mode for a fast-iteration workflow.
The guard now includes the sheet's revision, hashed from its *source* so a no-op
save costs nothing.

| 500 scopes | median |
|---|---:|
| memoized frame | 15–30 µs |
| full rebuild | 445 µs |
| reload frame | 665 µs |

A reload frame is ~1.5× a full rebuild, and the next frame is memoized again. At
0.67 ms for 2500 nodes it is far below perception for a save-triggered action —
but it is a standing cost of this architecture, not something to optimize away.

---

## Round 3: what the removal unblocked

### Step 2 — identity without allocation · 4.09 → 0.10 allocs/node

Attribution **contradicted the plan**. "Intern the strings" assumes storage is
the cost; it is not:

```
bare node (floor)      0.09 allocs/node
+ STATIC short id      0.09    ← the sink stores it for FREE
+ format!-minted id    2.09    ← all 2.00 is the CALLER's String
+ one class            4.09    ← 2.00 for the class
```

A short `StableId` inlines into its `SmolStr`, so an id table would have bought
exactly zero. The two halves needed different fixes:

* **Ids → structured.** `NodeId { name: Sym, index: u32 }` — 8 bytes, no string
  minted. The `("row", 5)` shape ADR-021 already uses for scope keys, rendered
  to `"row5"` only when a selector, a test or the agent asks.
* **Classes → interned.** There the `String` *and* its `Vec` are both real.

Interning alone still left 1.00 — the `Vec<Sym>` buffer, one per node per frame,
holding a single 4-byte symbol. `ClassSet` inlines three and spills past that.

### Step 3 — the side table, columnar · 13× faster agent walk

`Meta` was 656 bytes of uniform record in a `HashMap<NodeIndex, Meta>` — the same
problem `Element` was, one layer down. Two costs: every property read hashes a
`NodeIndex`, and a node pays for `caret_byte`, twelve handler slots and a `label`
`String` whether or not it is a text field.

Hot fields became dense columns indexed by arena slot; the rare half moved behind
a per-node `ColdMeta` allocated only on first use — **0 of 20 000 nodes needed
one** in the measured tree.

The walk is the right headline: it is what the agent does constantly, so it is
exactly where *"observability akin to a human looking at the screen"* is paid for.

**Correctness was the real risk.** A dense array indexed by arena slot is only
safe if a *stale* `NodeIndex` reads absent rather than returning whatever now
occupies its slot — otherwise the agent gets one node's semantics under
another's identity, silently. Slots carry a generation; a test frees a node, lets
the arena reuse the slot, and checks the old handle.

### Step 4 — `LayoutStyle` split by measured occupancy · 339 → 179 bytes/node

The third uniform record in a row. Measured first, because step 2 showed what
guessing a split costs. Over 1801 real nodes:

```
padding          44.4%      width / height / gaps  22.2%
flex_direction   11.2%      align_items            11.1%
…and TWENTY fields set by 0.0%, including every grid field,
   margin, inset, and all four min/max dimensions
```

`margin` and `inset` alone are 64 of the 256 bytes. `CompactStyle` keeps the hot
fields inline and moves what is **both large and structurally rare** behind one
`Option<Box<RareStyle>>`.

The cold set is chosen slightly more conservatively than the data alone
justifies: `position`, `flex_grow` and `justify_content` measured 0% here but are
obviously used by absolute overlays, spacers and centred rows this probe does not
model, and at 1–4 bytes they are too small to be worth a pointer chase.

The walk doubled again (28.2 → 14.6 µs) purely from cache density — half the
column, twice the nodes per line.

---

## The pattern

```
Element     1072 B  →  removed
Meta         656 B  →  columnar, 179 B/node
LayoutStyle  256 B  →  split, hot fields inline
```

Three uniform records in a row, each one layer down, each fixed the same way:
**measure occupancy, keep the hot fields dense, put the rare tail behind a
pointer.** It is one habit, not three bugs.

What remains per node is 179 bytes of genuinely-used data and 0.13 allocations —
against a structural floor of 0.09.

---

## Bugs the prototypes found

Six, five of them in designs that looked right:

1. **Cascade ordering** — a widget resolving before declaring its classes is
   silently unstyled. `ProgressBar` shipped with it. Now unrepresentable: three
   mistakes fail to compile, pinned by `compile_fail` doctests.
2. **Unbalanced ancestor stack** — with no stylesheet, `resolve` pushed an
   ancestor but stored no style, so `end` never popped; 601 leaked entries.
3. **Missing context guard** — spliced spans reused across changed surroundings.
4. **Animation deadlock** — the refusal that prevented freezing caused it.
5. **Hot reload staleness** — spliced spans kept pre-edit styling.
6. **Mismatched benchmark roots** — the two arms had never built the same root
   node; found only when a stricter comparison started checking packed flags.

Plus two **harness** flaws that inverted results before being caught: boxed child
closures, and per-property `meta.get_mut` hashing that made direct lowering
*slower* than the path it was meant to beat.

---

## Method, and the numbers that were wrong

This workload allocates ~10 MB per frame, so **criterion timed allocator residue
rather than code**: the same `lower_direct` measured **941 µs and 2.71 ms** in two
groups of one binary. Timing moved to one-arm-per-process binaries with their own
warmup and median-of-many, repeated 7–9×.

Three figures reported early on this branch were artifacts. The rules adopted
after, and kept since:

* **deterministic metrics first** — frame composition, node counts, allocation
  counts; timings directional only, with spreads reported;
* **demonstrate the bug before fixing it**, or a passing test proves nothing —
  this is what made findings 3, 4 and 5 real rather than assumed;
* **`assert_balanced()` after every frame**, and an equivalence guard between
  benchmark arms. Every time a comparison was made stricter, it found something.

An earlier version of this report claimed a **6.4×** peak-memory reduction from
removing `Element`. That was wrong: the phase split shows the staging tree peaks
at 2.63 MB while the destination peaks at 5.50 MB, so `Element` was the smaller
half. Real peak reduction is ~10%. The case was never footprint; it was
allocation churn and, above all, what the change unblocked.

---

## Assessment

**Direct lowering alone** is ~24% faster lowering and −18.5% allocations for a
large conversion. On its own that is not worth it.

**Direct lowering as step one of four** is what makes steps 2–4 cost days rather
than another 57-widget migration — because the destination can change without the
widgets knowing. Steps 2 and 3 are each a bigger win than step 1, and step 4
doubled step 3 again. That compounding is the argument.

**Against the framework's stated intent:**

| requirement | status |
|---|---|
| maximally performant | 38× fewer allocs/node, 13× faster agent walk, 3.75× smaller side table |
| tunable by generics / feature flags | a sink can be generic over what it collects; `Element` could not |
| small and gigantic apps | 0.09 allocs/node floor — the ceiling is not structural |
| cross-platform | nothing platform-specific; writes to `Tree` / taffy |
| agent observability | proven unaffected; the walk it depends on is 13× faster |
| hot reloading | works; +1 frame at reload |

**Still unprototyped:** `@keyframes` timelines (property transitions only),
container queries (`MediaContext.container` feeds the context hash from the
previous layout), `AppSnapshot` restore, code hot reload (the `fixtures/hot_*`
dylib path), and the engine conversion itself — `build_node`'s interaction with
hidden subtrees and error boundaries.

---

## Artifacts

**Prototype** `crates/lumen-widgets/src/direct.rs` — `TreeSink`, `Direct`,
typestate guards, cascade, memoization, text measurement, overlay, transitions,
hot reload, `Symbols`/`NodeId`/`ClassSet`, `MetaStore`, `CompactStyle`.

**Tests (59)** `direct_cascade` 10 · `direct_text` 6 · `direct_symbols` 6 ·
`direct_memo` 5 · `direct_anim` 5 · `direct_soa` 5 · `direct_compact_style` 5 ·
`direct_overlay` 4 · `direct_damage` 4 · `direct_hotreload` 4 ·
`third_party_widget` 4 · `composition_showcase` 1

**Instruments** `benches/benches/lowercost.rs` · `benches/src/bin/` —
`lowerprobe` (allocations, peak), `lowertime` / `memotime` (timing, one arm per
process), `floorprobe` (per-node attribution), `soaprobe` (columns vs records),
`styleprobe` (field occupancy)

**Prior docs** `experiment-widget-trait-2026-08.md` ·
`prototype-direct-lowering-2026-08.md` · `plan-direct-lowering-unknowns.md`
