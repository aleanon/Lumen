# MOD6 — the state store stays non-swappable. Measured, not asserted.

*2026-08-08. A written yes/no, in the shape CP5 established: "stop" is a
permitted outcome, and the inputs are numbers rather than judgement.*

MOD6 asked for the state store's **storage** to be swappable while its
**attribution** — the exact signal→scope dependency data `ui.getDeps` reports —
stayed fixed. The plan already scoped it down once, for the observability
pillar. Measuring it found a second reason, and this one is disqualifying.

## Two structural constraints, before any measurement

**1. `Runtime` cannot become generic.** It appears in **70 public signatures**
across the workspace and in handler types the *user* writes (`Fn(&Runtime)`,
14 sites in the widget catalogue alone). A type parameter there would infect
every closure an application passes to a button. So the only available shape is
dynamic dispatch inside `Inner`.

**2. "Attribution not swappable" forces two lookups.** Today one `HashMap` lookup
returns a `Slot` holding the value *and* `subs`/`version`/`owner` together. If a
third party may supply the value storage but must not be able to supply the
attribution, the two have to live in different maps — the runtime keeps
attribution, the store keeps values. That is not an implementation detail that
can be optimised away; it is what the requirement means.

## The measurement

A `Slot`-shaped microbenchmark, min-of-7 with the variants interleaved (the
first attempt was noise-dominated: `direct` moved 6.07 → 9.87 µs between runs
and the dyn variant came out *faster*, which is impossible — worth recording,
because a single-shot version of this would have produced a confident wrong
answer).

| shape | per 1000 signal writes | vs today |
|---|---:|---:|
| direct — today | 4.68 µs | — |
| dyn, one map (attribution inside the swappable store) | 5.97 µs | **+27.6%** |
| **dyn + split — what MOD6 requires** | **10.18 µs** | **+117.6%** (+5.50 ns/access) |

## What that costs on the real path

`benches/identity.rs` measures re-addressing 1000 per-row signals at
**18.24 µs/frame** (`1k_rows_typed_key`). That figure is ADR-021's headline
result: it replaced a 51.4 µs string-keyed path, a 2.8× win that justified
changing reactive identity across the codebase.

At +5.5 ns/access, MOD6 adds roughly **5.5 µs/frame** to that same path —
**about a third of the ADR-021 win handed back**, permanently, on the framework's
most-optimised hot path (`Signal::update` was tuned from 780 µs to 16 ns).

## Decision: stop

Both available shapes are unattractive, and for different reasons:

* **Split maps** (attribution safe): +117.6%. Buys a modularity checkbox by
  giving back a third of the reactive-identity win.
* **One map** (attribution inside the swappable store): +27.6%, *and* it puts
  `subs`/`version`/`owner` inside the component a third party replaces — which
  is precisely the trade the plan already rejected, because `ui.getDeps`'
  exact-attribution guarantee is the observability pillar, not a feature.

The framework's stated bars are peak performance **and** complete agent
observability. MOD6 as specified can only be bought with one of them. So the
state store keeps its concrete storage, and the modularity claim in `01 §9a`
lists seven axes rather than eight — accurately.

## What this does not close

* The **snapshot/restore** seam (ADR-011) already lets an application move state
  in and out of the store. Persistence — the use case most often meant by
  "swappable storage" — is served there, without touching the read path.
* If a future workload makes the store access no longer hot (a retained graph
  that re-reads far fewer signals per frame, say), the arithmetic changes and
  this decision should be re-run. It is gated on a number, not a principle.

## Caveat, stated plainly

The table is a **microbenchmark proxy**, not the runtime. It models the access
pattern (hash lookup, boxed value, attribution fields touched per write) but not
the surrounding work. It is strong enough to gate on because the cost driver is
structural — one lookup becomes two — rather than an artefact of how the probe
was written. A full prototype would refine the percentage; it would not change
the sign.
