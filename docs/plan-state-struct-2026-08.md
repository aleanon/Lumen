# Plan — the state-struct model (`#[derive(Reactive)]`)

**Status: proposed, not started.** Written 2026-08-30, after R7–R10 and C1.

## What is being proposed

App state becomes a struct whose fields are the reactive units:

```rust
#[derive(Reactive)]
struct App {
    inbox: Inbox,
    settings: Settings,
}
```

A view reads `state.inbox.unread` instead of `cx.signal("unread").get(rt)`. The
derive gives each field a store entry keyed by its **compile-time field path**;
reads are tracked as they are today.

## Why — and the honest size of the prize

Sized before committing, because the first estimate in discussion was ~30% and
the measurement (R9) said otherwise.

### Performance: 8.8%, and it is a floor

`storelookup.rs`, N=50 000, collector open (recording only happens inside a
build, so a closed-collector figure understates it):

| layer | µs | ns/read | removed by field paths? |
|---|---:|---:|---|
| addressing (key → `SignalId`) | 510.3 | 10.2 | **yes** |
| slot lookup + downcast | 288.6 | 5.8 | **yes**, if the field *is* the storage |
| read recording | 466.7 | 9.3 | **no** |
| plain field access | 3.1 | 0.1 | — |
| **total** | **1 268.7** | **25.4** | |

Against C1's 9 047 µs frame: store cost 14.0%, **removable 8.8%**, read
recording 5.2% survives any representation change. The arms use `rt.signal(i)`
while a build uses `cx.signal(i)` — which also folds the enclosing scope prefix
— so real in-build addressing is *higher* and 8.8% is a floor.

**This does not compete with C1 or `For`.** Those set rebuild granularity (R10:
54 523 → 9 148 → 1 524 µs); this sets how a dependency is named and read. Adopt
it *instead* of them and you trade 79% for 9%.

### Correctness and ergonomics: the real case

1. **Identity becomes compile-time.** The field path is the key: allocation-free
   and collision-free. Two bugs hit *in this session's own benchmarks* become
   unrepresentable — a `format!("r{i}")` key (the exact anti-pattern ADR-021
   exists to kill, worth 5 ms of 36), and `cx2.signal(i)` inside a scope
   silently addressing a scope-local slot instead of the one being written.
   Both were caught only because an equivalence guard was in place.
2. **`Component::deps` gets a correct automatic default.** C1 had to make `deps`
   a *required* method because omitting captured data memo-hits forever and
   renders frozen content silently. With field reads tracked, that whole failure
   mode disappears.
3. **Better dependency names for the agent** (ADR-009): `inbox.unread` beats a
   signal key string.
4. **Typed snapshots** (ADR-011) instead of a heterogeneous store.

## What must be decided before any code

These are the reasons this is a plan and not a patch.

### D1 — view-local state needs a second mechanism

Hover, caret position, dropdown-open, scroll offset: none belong in app state.
SwiftUI needs both `@State` and `@Observable` for exactly this. **The keyed
store does not go away**; it stops being the only thing. Decide whether
`cx.signal` stays as the view-local mechanism (likely yes) and what the
authoring rule is for choosing between them.

### D2 — hot reload

ADR-007 records that the central store is *what makes hot reload possible*. A
typed struct changes shape when code changes, turning reload into a schema
migration the keyed store tolerates today. **This is the strongest argument
against**, and it must be answered before D3.

### D3 — collections

A `Vec<Row>` field gives either one scope (whole list) or one per element —
R10 measured those at 91 110 µs and 42 346 µs against an optimum of 9 098 µs.
The state shape does **not** supply a good grain. `For` and `VirtualList` remain
the answer, and the derive must not fight them.

### D4 — what the derive generates

Accessors (`state.count()`), a `Deref` wrapper, or field-path constants
consumed by a read tracker. Affects ergonomics and whether `&mut` field access
can be intercepted at all.

## Phases

Each phase ships alone and is reversible. No phase begins before its
predecessor's measurement is in.

| # | phase | exit criterion |
|---|---|---|
| **S0** | Answer D1 and D2 in the decision log. No code. | Both recorded with a decision, or the plan stops here. |
| **S1** | `#[derive(Reactive)]` in `lumen-macros`, generating field-path keys into the **existing** store. No API change for readers. | An app builds and runs identically; `storelookup` shows the addressing arm dropping toward the field-read floor. |
| **S2** | `Component::deps` gains a default derived from tracked field reads; `deps` becomes optional where the component reads only `Reactive` state. | `tests/component.rs` passes with `deps` removed from a component that reads only state fields. |
| **S3** | Field storage: the field *is* the slot, removing lookup+downcast. | The `read only` arm approaches the `field read` arm; no regression in `sparse` or `for_list`. |
| **S4** | Migrate examples and the `lumen new` scaffold; the keyed store stays for view-local state per D1. | Whole workspace green; `sparse` unregressed. |

## What this plan explicitly does not do

* Replace `Component` (C1) or `For` — orthogonal, and 9× larger in effect.
* Remove `cx.signal` — D1 needs it.
* Claim a performance win as the motivation. **The motivation is correctness and
  ergonomics; 8.8% is a bonus.** Any write-up that leads with the speed number
  is misrepresenting the measurement.

## Measurement discipline for this work

Non-negotiable, and each rule here exists because it caught something in R7–R10:

* One arm per process; min of many; interleave arms.
* An **equivalence guard** on every benchmark arm — assert the update actually
  landed. It caught three of this session's own bugs, including two where a
  broken arm looked *fastest*.
* Do not build a benchmark on a shape already documented as unrealistic. `sparse`
  was built on a flat 50 000-row column six commits after the benchmark report
  disclosed that no real app is written that way, and two features were then
  justified against it (R10).
