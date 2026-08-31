# Plan: maximum performance — build once, mutate (2026-08-30)

**Mandate (user, 2026-08-30):** absolutely maximum performance — speed and
resource efficiency. Widgets built once and mutated thereafter. Architecture
and API changes are on the table. Two things must survive: reloadability
(losing the fast `.lss` restyle path is acceptable) and agent observability.

This document is the investigation that mandate asked for. It consolidates
every measurement this branch has produced (PROF1–1.1, R1–R10, F2.x, F3.x,
O0.x, T1/T2, L1, C1/C2, S0–S3, BENCH2 sparse) into one cost model, states the
target frame contract, and lays out the program — **MUT0–MUT9** — in the order
the measurements justify.

## 1. Where a changed frame goes today

The reference workload is the sparse frame — N rows retained, K=1 changed —
because that is the workload every real frame has. Two anchor points:

**N = 50 000, K = 1, best current authoring (chunked scopes), R7:**

| phase | µs | what it is |
|---|---:|---|
| view | 2 736 | scope calls (~0.28 µs each) + store reads (1 269 µs, R9) |
| layout | 2 724 | flat root flex re-solves all children; rebuilt spans start cold |
| bounds/clip walk (F2.2) | 1 162 | O(N) over every live node, every rebuild frame |
| paint | 2 039 | full `build_display_list()` + `damage_between` O(N) diff |
| sweep | 3 | fixed by chunking |
| **frame** | **9 223** | |

**N = 3 000 memoized full-rebuild bench:** Lumen 946 µs vs iced 290 µs (3.3×,
was 8.3× when PROF1 started). GTK/Qt sit below iced because they never build:
a value change mutates a retained widget, queues a resize up the parent chain,
and repaints the damage.

Three standing facts frame everything below:

1. **Retention already exists on three of four axes.** The node arena keeps its
   `NodeIndex` across frames (F2.2 splice), memo-hit spans keep their taffy
   nodes (F2.1), the display list and CPU frame are retained (R2), and the
   engine skips work entirely on an idle pump. What is *not* retained is the
   work of a **changed** frame: a dirty scope re-runs its closure, re-mints its
   nodes, and then the frame-level O(N) passes run regardless of K.
2. **The mutation path exists but is broken and partial.** F3 `bind` measured
   **2.7× slower than the rebuild it replaces** with `nodes_rebuilt = 10 001`
   at N=10 000 — it does not avoid the rebuild, and the trigger term
   (`force_rebuild || time_driven || (write_changed && !structural_current)`,
   app.rs:1654) was never isolated. `patch_text_bindings` is all-or-nothing:
   one non-patchable binding aborts the entire pass to a full rebuild
   (app.rs:4929), and a successful patch still invalidates the whole semantics
   tree and re-diffs the whole display list.
3. **The "irreducible" floor is a rebuild-semantics artifact.** R7 called the
   bounds walk + display-list diff (~0.064 µs/node, 3.2 ms at 50k) genuinely
   irreducible. That was true *under rebuild semantics*, where the engine does
   not know what changed and must discover it by walking. Under mutation
   semantics the patch engine knows exactly which nodes changed; both passes
   become O(K). R7's framing is hereby revised, not retracted — its numbers
   stand.

## 2. The target frame contract

> **cost(frame) = O(K bindings evaluated) + O(dirty-layout subtree) +
> O(damaged pixels) + O(structural churn). Idle = 0.**

No term is O(live nodes). This is GTK's contract (`queue_resize` /
`queue_draw`), and every mechanism needed to meet it already has a measured
prototype in this branch:

| contract term | mechanism | status | evidence |
|---|---|---|---|
| O(K) value change | property bindings, patched in place | text ✓ / background ✓, broken trigger, all-or-nothing | F3.5, F3.6, O1-patch |
| O(dirty subtree) layout | taffy `mark_dirty` + per-node cache | taffy 0.14 has it; Lumen discards it for rebuilt nodes | R6✗: one dirty leaf 590 µs vs 11 861 cold at D=8 (20×) |
| O(damage) paint | patch-driven damage, retained DL | DL retained but re-built + re-diffed O(N) | paint(): `damage_between` every painted frame |
| O(churn) structure | scoped rebuild + splice | **done** | F2.2: −47.8%; C1: −79%; C2 `For`; R10 `VirtualList` O(1) |
| idle = 0 | `Damage::None` early-out | **done** | app.rs:1743 |

## 3. The five pillars

### P1 — Mutation replaces rebuild for value changes
A signal write resolves through a **reverse index** `SignalId → [BindingId]`
(today the patch path scans every binding's `ReadSet` per pump). Each binding
is `(node, property, closure, deps)`, registered at lowering time. Commit is
**per-binding**: a binding whose new value is layout-neutral patches the node
and contributes its rect to damage; one whose measurement changed falls back to
marking **its owning component** dirty — a scoped rebuild + splice, not a
global abort. That fallback is exactly the "child asks its parent for space"
callback discussed on 2026-08-27, expressed through machinery that already
exists (`Component` dirty → rebuild → F2.2 splice → taffy `mark_dirty`).

### P2 — The frame-level passes become change-driven
Bounds/clip walk only under layout-dirty roots; display list retained and
patched by stable `NodeIndex` (F2.2 gives stability), damage accumulated from
the patch set — `damage_between` becomes the fallback for rebuild frames, not
the steady state; semantics **patched** per node instead of
`*sem_root = None` whole-tree invalidation. Layout skipped entirely when no
node is layout-dirty, computed from warm caches otherwise.

### P3 — Widgets lower once, directly (finish Direct, delete `Element`)
The architecture is complete (O0.16–O0.24: `Sink`/`NodeWriter`, children as
callbacks, context imposition, arena, statement-form authoring, every widget
has a `Direct` impl; the cascade composes — `direct_cascade.rs`). Remaining is
mechanical: default the engine to the Direct path, migrate the authoring
signature (~180 files), delete `Element` (784 B/node of churn, ~5% of frame per
R8 — R8 supersedes O0.19's 9–11%). Direct lowering is also *the natural
registration point for bindings*: `Label::new(bind!(..))` writes a binding
record, not a value. Plus R5's second half: the shaped paragraph lives **on the
node** (a retained node makes it a slot, not a cache) — the shape-cache lookup
was 14.3% of a frame before R1/R2 trimmed it; per-node storage removes the
lookup entirely, iced's model.

### P4 — State: the struct is the storage (S3-deep, reopened)
S3 was deferred because its cheap form bought −4.1 µs and its real form needed
the largest API change in the plan. **This mandate is that change's
justification arriving.** `App::new(state, |cx, &state| ..)`: fields are the
slots, per-field version counters, a read is a field access + `note_read` —
removes the three `RefCell` borrows and two map lookups per read (8.8% of
frame, a floor per R9's caveat). `#[derive(Reactive)]` (S1) already provides
the field paths; `Component::deps` (S2) already derives itself. Follow-up:
components whose reads all go through derived accessors get a **compile-time
dep mask**, replacing runtime read-recording (the remaining 5.2%) in the
common case.

### P5 — What is preserved, by construction
**Observability:** the agent reads the *retained* tree — patches keep it
current instead of invalidating it, so observability gets cheaper, not
narrower. `dev-observability` gating (A11Y3) unchanged; `set_size`/
`position_in_set` (A11Y2) unchanged; **`assert_view_coherent` is the oracle
for every mutation path** — each patched frame must equal what a from-scratch
rebuild would produce. That discipline already caught the F2.2 slot-identity
narrowing and is the reason mutation semantics can be trusted at all.
**Reloadability:** the state struct derives `Serialize`/`Deserialize` with
`#[serde(default)]` — the user's proven iced recipe: serialize, swap code,
deserialize, one `force_rebuild` re-registers every binding closure. The keyed
store stays for view-local state (D1). The `.lss` fast-restyle path may
degrade to rebuild-priced; accepted by the mandate.

## 4. The program

| phase | what | size (measured basis) |
|---|---|---|
| **MUT0** | Diagnose the F3 rebuild trigger — which of the three terms at app.rs:1654 fires on a bound-only write, and why `bind` costs 2.7× `plain` | blocks P1; known debt |
| **MUT1** | Reverse index `SignalId → bindings`; per-binding commit with component-scoped fallback (kill the all-or-nothing abort) | turns K=1 into O(K) view work |
| **MUT2** | Patch-driven damage + retained display list (no `build_display_list`/`damage_between` on patch frames) | ~2.0 ms at 50k |
| **MUT3** | Incremental bounds/clip walk (layout-dirty roots only) | ~1.2 ms at 50k |
| **MUT4** | Warm layout for rebuilt spans: reuse taffy nodes for unchanged children of a rebuilt scope, `mark_dirty` the changed; skip compute when nothing is dirty | 20× at D=8 (R6✗) |
| **MUT5** | Generalize bindings: color, opacity, transform, value/progress, visual state — registered at lowering | completes P1 coverage |
| **MUT6** | Semantics patching per node; drop whole-tree invalidation | removes lazy-rebuild spikes; observability stays current |
| **MUT7** | Direct-only engine: migrate ~180 files, delete `Element`; per-node shaped paragraph | ~5% (R8) + shape-lookup bucket + 784 B/node |
| **MUT8** | S3-deep: state instance threaded, fields as slots, serde reload | 8.8% floor (R9) + kills both S-series bug classes |
| **MUT9** | Compile-time dep masks where all reads are derived accessors | part of the remaining 5.2% |

**Ordering rationale:** MUT0–3 fix the engine's own floor (the broken patch
path plus 3.2 ms of walks that only exist because the engine forgets what
changed); MUT4 is the largest single layout effect ever measured on this
branch; MUT5–6 complete mutation coverage; MUT7–9 are the constant-factor and
API tail, each individually costed and none load-bearing for the contract.

**Exit criteria, per the house discipline:** every phase demonstrates its bug
before fixing it, lands with a benchmark arm and an equivalence guard, and the
sparse matrix (N ∈ {1k, 10k, 50k}, K ∈ {1, 16}) plus `assert_view_coherent`
run as gates. Target hypothesis to verify, not promise: sparse K=1 at
N=50 000 from 9.2 ms to **under 1 ms** (bindings O(K) + warm layout ~0.6 ms +
patch damage), which crosses below iced's model — iced positionally diffs its
whole widget tree every frame; a working patch path does not.

## 5. Program status (2026-08-31 — complete)

MUT0–MUT8 landed; MUT9 closed on measurement (recording fell to ~2 ns/read
as a side effect of MUT8 — the mask's prize is <0.1% of a frame; see the
task-graph entry). Deferred with rationale along the way: opacity/transform
bindings (no node-level property exists — animations rework), the Element
deletion's final stages (~180 authoring files + the `build_node` rewrite,
staged behind `Element::direct`), DL segment splicing for rebuild frames
(now below the priority line at ~400 µs), RTL layout pruning. Scoreboard:
patch frame 215 µs flat in N (was ~90 ms broken); structural rebuild 3.0 ms
(was 9.3); decline cliff 5.5 ms (was 320); observer tax 0 (was 15 ms);
state read 3.4 ns (was 26.5).

## 6. Risks

- **Coherence surface grows.** Every new patch path is a new way to drift from
  rebuild semantics. Mitigation is the existing oracle + the F3.6 precedent
  (bindings carried across splices, guarded by the same tests).
- **Retained DL memory.** Keyed paint runs cost memory per node; measure
  against the A11Y3 RSS gate.
- **`Element` removal is a 1.0 authoring break** (~180 files). O0.22's
  `Element::direct` boundary makes it incremental; it stays last for a reason.
- **S3-deep changes the reload story** from "store survives" to "state
  serializes". The user has shipped this exact model for iced; D1 keeps the
  store for view-local state, so nothing is orphaned.
