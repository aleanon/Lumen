# BLOCKED — the last phase-table item needs a public-API decision

Raised 2026-08-28 while working through the `build_node` phase table
(O0.6 → O0.13, all landed). Escalated per `.ai_docs/00-HANDOFF-README.md` §4:
*"if the decision is architectural (public API shape …) stop that task, leave a
`BLOCKED.md` note describing options and your recommendation."*

Two items remain. Both are larger than anything already landed, and each needs
a call that is not mine to make.

---

## 1. `view` — 384 µs/frame (12%), the last build-table row

The view closure constructs one `Element` per node. `Element` is **1072 bytes**,
and **304 of those are the same fourteen rare fields** O0.13 just moved out of
`NodeMeta`: every event handler past `on_click`, caret/selection, scroll state,
shadow. On a label in a list all fourteen are `None`.

The same fix applies and would shrink `Element` by ~28%, helping both `view`
and `build_node`. Estimated **3–4% of a changed frame**.

**Why it is blocked:** `Element`'s fields are `pub`. Roughly 160 sites read or
write them, ~100 of those outside `lumen-app` — widgets, examples and tests do
`e.style.width = …` and similar. Boxing the rare group is a breaking change to
a published surface.

### Options

| | change | frame | cost |
|---|---|---|---|
| **A** | Box the rare fields, accessors alongside | −3–4% | breaking; ~160 sites |
| **B** | Same, but keep the old fields as `#[deprecated]` shims | −3–4% | non-breaking; shims must not reintroduce the bytes, so they can only be methods — i.e. still breaking for direct field *writes* |
| **C** | Full direct lowering — `Element` stops existing on this path | −12% (the whole row) | 222 files, 1801 references; the migration this branch was opened to evaluate |
| **D** | Leave it | 0 | — |

**Recommendation: A, but only bundled with C's decision.** Doing A alone spends
a breaking change on 3–4% and then C would rewrite the same code again. If C is
going to happen, skip A. If C is not going to happen, A is worth it — but it
should land at a deliberate boundary, not mid-branch.

---

## 2. The ambient audit — 858 µs/frame (27%), and not on the phase table at all

Measured after O0.12, with the audit compiled out as the control:

```
with ambient audit      3220 µs
without                 2362 µs      ← 858 µs, 27% of the frame
```

Remaining composition at 4000 rows: `sem_root` 272 µs (rebuilding the semantics
tree), `contrast` 161, `offscreen` 155, `invisible` 91, `audit::lint` 37, rest
<20 each. Unlike the tofu scan (O0.12), these are genuine per-frame geometric
work — bounds move every frame, so there is no "told, not asked" reformulation.

`dev-observability` is a **default** feature, so an ordinary
`cargo build --release` pays this; only `--no-default-features` drops it.

### Options

| | change | cost |
|---|---|---|
| **A** | Throttle the ambient pass (`lumen-core::observe::Throttle` was built for this in O0.2) — e.g. at most every 100 ms | ~6× cheaper at 60fps; a finding that appears *and disappears* inside the window is missed |
| **B** | Make `dev-observability` non-default | release builds stop paying; `cargo run` loses the push channel unless opted in |
| **C** | Optimize the individual passes | unknown; they are already O(n) with small constants |
| **D** | Accept — it is the price of the feature and it is already gated | 0 |

**Recommendation: A.** The ambient audit is a *push* channel for a developer or
agent; its contract is "tell me promptly", not "tell me this exact frame".
Running it 60×/second is far more often than anyone reads it. O0.3's own note
says a frame-cadence throttle was thought unnecessary — that judgement was made
when `lint()` cost 0.34 ms on a 200-label screen, not 858 µs on a real page.

The missed-transient risk is real and is the reason this is a decision and not
an edit. Mitigation if A is chosen: keep the throttle time-based, and force a
pass on the frame a rebuild *stops* (so a finding that appears and is fixed
across a settling animation is still seen at the end).

---

## Where things stand

Landed this session, 4000-row styled changed frame, every node rebuilt:

```
before O0.6    5568 µs
after  O0.13   3218 µs      -42%
```

O0.6 · O0.7 · O0.8 · O0.9 · O0.10 · O0.11 · O0.12 · O0.13 — see
`.ai_docs/06-task-graph.md`. 744 tests pass; clippy, fmt and the lean build are
clean. Nothing here is blocked on anything above; the two items in this file are
additive.
