# Plan: async/data executor + renderer generics

*Design + build plan, 2026-06-22. Outcome of the async-data-layer and
generics-vs-dyn discussion. Two workstreams that converge on a single
`App<R = CpuRenderer, E = InlineSpawner>` shape.*

## Goals

1. **Data layer.** First-class async/background work that feeds results back into
   app state: one-off tasks, streaming results, and convenient heavy-compute
   (thread) tasks — without breaking the runtime's determinism contract.
2. **Renderer generics.** Replace `renderer: Box<dyn Renderer>` (dynamic
   dispatch) with a defaulted generic `Headless<R = CpuRenderer>`, keeping
   dynamic dispatch available as a *consumer opt-in* (`R = Box<dyn Renderer>`)
   rather than a baked-in default.

Both follow one library principle: **expose the static/generic form; let the
consumer erase to a trait object only where and when they want dynamism.**

## Foundational invariant (do not violate)

`pump()` is a pure function of *(state, queued events, clock)* — this is what
makes goldens bit-exact, agent replay sound, and snapshot/restore coherent
(decision log). Therefore **background work never mutates a signal directly.** It
enqueues a *deferred op*; the next `pump()` drains the queue (UI thread, FIFO)
and applies it. This is the same pattern `cx.animate()`/`cx.wake_at()` already
use via `FrameRequests`: build *records intent*, the runtime *acts after*.

---

# Part A — Renderer: `Box<dyn Renderer>` → `Headless<R = CpuRenderer>`

## Current state

- `lumen_render::Renderer` trait (`render_frame`, `name`) — already object-safe.
- `CpuRenderer` (ZST, `Default`); `GpuRenderer` (stateful: wgpu device/queue),
  `#[cfg(not(target_arch = "wasm32"))]`.
- `Headless` stores `renderer: Box<dyn Renderer>`; `set_renderer`/`renderer_name`
  exist; `paint()` calls `self.renderer.render_frame(...)` (one site).
- `Headless` is referenced in ~53 files; backend-agnostic *library* consumers are
  `lumen-agent` (~15 `&mut Headless` signatures) and `lumen-shell`.

## Target state

```rust
pub struct App<R = CpuRenderer> { root: …, stylesheet: …, renderer: R }
pub struct Headless<R = CpuRenderer> { …, renderer: R, … }   // R: Renderer

impl App<CpuRenderer> {
    pub fn new(root) -> App<CpuRenderer> { /* renderer: CpuRenderer (ZST) */ }
}
impl<R: Renderer> App<R> {
    pub fn with_renderer<R2: Renderer>(self, r: R2) -> App<R2> { … }
    pub fn run_headless(self, size: Size) -> Headless<R> { … }
}
```

- The renderer is carried as a **value** (not via `Default`), because `GpuRenderer`
  needs a constructed device — the shell builds it (post-surface) and hands it in:
  `App::new(build).with_renderer(GpuRenderer::new()?)`. `CpuRenderer` is a free ZST
  default so the headless/test path needs nothing.
- Drop `set_renderer` (runtime hot-swap). Backend is chosen at construction; the
  "GPU if available else CPU" case is a one-time branch at the shell entry point.
  (Runtime swap is recoverable *only if* the consumer opts into `R = Box<dyn
  Renderer>` — then `set_renderer(Box<dyn Renderer>)` is type-stable; see Part C.)

## Blast-radius handling

`Headless` (no params) still resolves to `Headless<CpuRenderer>` via the default,
so **most code is unaffected** — every `run_headless()` test and helper that names
`Headless` keeps compiling against the default instantiation.

The exceptions are *backend-agnostic library code that must accept any
instantiation*:

- **`lumen-agent`** — make its functions generic: `dispatch<R: Renderer>(app:
  &mut Headless<R>, …)`. Mechanical (R is pass-through, never used), ~15
  signatures, zero runtime cost. *Alternative if the signature noise grates:* a
  small object-safe `RuntimeView` trait (the pump/inject/semantics/screenshot
  surface the agent needs) that `Headless<R>` impls, and the agent takes `&mut dyn
  RuntimeView`. Recommend the generic pass-through first; introduce `RuntimeView`
  only if a second backend-agnostic consumer appears.
- **`lumen-shell`** — *pins concrete types* (it chooses the backend). Its field
  becomes `Option<Headless<CpuRenderer>>` (or the GPU type once that lands). Not
  viral — the shell is the top of the tree.
- **`lumen-widgets` helpers** used by the agent (e.g. `center(&Headless)`) — make
  generic over `R` the same way.

## Steps (each independently green)

- **A1.** Add the blanket `impl<R: Renderer + ?Sized> Renderer for Box<R>` (so
  `Box<dyn Renderer>` remains a valid `R`). Land alone; no behavior change.
- **A2.** Make `Headless<R = CpuRenderer>` and `App<R = CpuRenderer>` carry `R`;
  field `renderer: R`; `paint` calls `self.renderer.render_frame`. `run_headless`
  → `Headless<R>`. Remove `set_renderer`/`renderer_name` from the inherent impl
  (keep `renderer_name` via `self.renderer.name()`).
- **A3.** Generic-ize `lumen-agent` + the `lumen-widgets` helpers it uses.
- **A4.** Update `lumen-shell` to pin `Headless<CpuRenderer>` (today's behavior:
  the shell still CPU-rasterizes + blits; the *live* GPU surface backend is a
  separate follow-on, see cross-platform-readiness.md). Add `with_renderer` and a
  shell entry that can select GPU later.
- **A5.** Tests: the CPU↔GPU comparison test calls `CpuRenderer::render(&dl)` vs
  `GpuRenderer::render(&dl)` directly (no `Headless` needed); keep a
  `Headless<Box<dyn Renderer>>` smoke test to prove the dynamic opt-in compiles.

*Acceptance:* `cargo test --workspace` green; `size_of::<Headless>()` unchanged
or smaller (no vtable ptr); a `Headless<Box<dyn Renderer>>` instantiation
compiles and runs; goldens byte-identical (CPU default path untouched).

---

# Part B — Executor + data layer (new crate `lumen-tasks`, re-exported)

## The three layers

### L0 — deferred-op queue + the `Spawner` seam

```rust
/// A unit of state change produced off-thread, applied on the UI thread in pump.
pub enum DeferredOp {
    /// Typed, keyed, *replayable*: set/append a named cell to a recorded value.
    Set   { key: StableId, value: ErasedValue, gen: u64 },
    Push  { key: StableId, value: ErasedValue, gen: u64 },
    /// Escape hatch — flexible, *not* replayable.
    Mutate(Box<dyn FnOnce(&Runtime) + Send>),
}

/// Handed to background work; its only job is "push a result + wake the loop".
/// Carries no executor type — a channel Sender + an optional waker. So task
/// closures and user fetchers never name `E`.
pub struct Sink { tx: Sender<DeferredOp>, waker: Option<Waker>, key: StableId, gen: u64 }
impl Sink {
    pub fn set<T: Send + 'static>(&self, v: T);                 // replayable
    pub fn push<T: Send + 'static>(&self, v: T);                // replayable (stream)
    pub fn mutate(&self, f: impl FnOnce(&Runtime) + Send + 'static); // not replayable
    pub fn fail<E: Send + 'static>(&self, e: E);
}

/// The platform seam. Object-safe (boxed args) so `Box<dyn Spawner>` works.
pub trait Spawner {
    fn spawn(&self, fut: BoxFuture<'static, ()>);            // portable (incl. wasm)
    fn spawn_blocking(&self, f: Box<dyn FnOnce() + Send>);   // native CPU work
}
impl<S: Spawner + ?Sized> Spawner for Box<S> { /* forward */ }
```

`Headless<R, E>` holds `executor: E` and the `Receiver<DeferredOp>`; `pump()`
**drains the receiver first** (before events), applying ops on the UI thread.
A generation guard (`gen`) per key makes stale results no-ops = cancellation.

The **waker**: how a background result wakes the event loop. Host-provided (the
shell wires an `EventLoopProxy`-backed closure via `set_waker`, like it owns wall
time). In headless/tests there's no waker — the inline executor resolves
synchronously, so the op is already queued before `pump` returns.

### L1 — the primitive: keyed spawn (heavy threads + streaming)

`BuildCx` gains `tasks: RefCell<Vec<TaskRequest>>` alongside `requests`/
`continuous`; it **records intent**, never spawns (executor stays out of
`BuildCx`):

```rust
struct TaskRequest { key: StableId, deps_hash: u64, kind: TaskKind /* Future | Blocking */ }

impl BuildCx<'_> {
    pub fn task(&self, key, deps: impl Hash, f: impl FnOnce(Deps, Sink) -> impl Future + Send + 'static);
    pub fn task_blocking(&self, key, deps: impl Hash, f: impl FnOnce(Deps, Sink) + Send + 'static);
}
```

`pump()` (post-build) takes the `TaskRequest`s, bumps each key's `gen`, mints a
`Sink` bound to `(key, gen)`, and dispatches via `self.executor`. One-off =
`sink.set` once; streaming/progress = `sink.set`/`sink.push` repeatedly.

### L2 — ergonomic one-off: `resource`

```rust
pub struct Resource<'a, T, E = TaskError> {  // data + flags (SWR by default)
    pub value: Option<&'a T>,   // last success — survives refetch & error
    pub error: Option<&'a E>,   // last error — cleared on next success
    pub loading: bool,          // a fetch is in flight now
}
impl Resource<'_, T, E> { pub fn phase(&self) -> Phase<&T, &E> { … } } // optional enum view

impl BuildCx<'_> {
    pub fn resource<T,E>(&self, key, deps: impl Hash + …, f: impl FnOnce(Deps)->impl Future<Output=Result<T,E>>) -> Resource<T,E>;
    pub fn resource_blocking<T,E>(&self, key, deps, f: impl FnOnce(Deps)->Result<T,E>) -> Resource<T,E>;
}
```

A resource is a specialized cell in the **state store** keyed by name (like a
signal) holding `{ value, error, loading, deps_hash, gen }` — so resolved values
snapshot/restore normally and survive rebuilds. `resource` reads the cell, and if
the key is new or `deps_hash` changed, flips `loading = true` and records a
`TaskRequest`. Lifecycle = "live only while re-emitted" (same as `FrameRequests`):
a key absent from a build is cancelled/dropped → automatic cleanup. Deps changing
→ bump `gen`, refetch, old result ignored.

### Executor implementations

- **`InlineSpawner`** (default, ZST): `spawn` block-on-polls to completion;
  `spawn_blocking` runs inline. Deterministic, no threads → goldens/tests stable,
  resources resolve "immediately" in virtual time. The default for `run_headless`.
- **`ManualSpawner`** (tests): records pending tasks; `run_pending()` then `pump()`
  lets a test assert intermediate `loading` states deterministically.
- **`ThreadPoolSpawner`** (native, **std only — no new deps**): a small
  `std::thread` pool + channel for `spawn_blocking`; `spawn(future)` polled on a
  pool thread by a tiny hand-rolled block-on (or deferred to the async layer).
  Satisfies one-off / streaming / heavy-thread on desktop + Android.
- **`WasmSpawner`** (later): `spawn` → `wasm_bindgen_futures::spawn_local`;
  `spawn_blocking` → worker or inline. `cfg(target_arch = "wasm32")`.

### Determinism / replay / snapshot

- Typed-keyed `set`/`push` carry **data** (recordable as `(key, value)`), so async
  outcomes record into the input trace and **replay** by re-applying recorded
  values instead of re-running tasks — same model as input events + the virtual
  clock. `mutate(closure)` is the documented non-replayable hatch.
- Snapshot includes resolved cell values (they live in the state store);
  in-flight tasks aren't serialized — on restore, resources re-kick (deps
  re-evaluated). Stated explicitly.
- Replay needs `T: Serialize` *only on the recording/replay path*; the base path
  needs just `T: Send + 'static`.

### HTTP stays out of core

The task layer runs *any* work and feeds results back; HTTP is "what you call
inside the fetcher." So the **entire first slice needs no new dependencies**
(std threads + channels). A blocking client (`ureq`, tiny) or an async runtime +
client (`tokio`/`smol` + `reqwest`) is an **additive, later** layer gated by
ADR-003 — and possibly an optional `lumen-net` convenience crate, never core.

---

# Part C — Unification: `App<R = CpuRenderer, E = InlineSpawner>`

Both capabilities are defaulted generics, consumed only inside the runtime, never
leaking into `build(cx)` or user fetcher signatures:

```rust
pub struct App<R = CpuRenderer, E = InlineSpawner> { … }
pub struct Headless<R = CpuRenderer, E = InlineSpawner> { … }

impl<R: Renderer, E: Spawner> App<R, E> {
    pub fn with_renderer<R2: Renderer>(self, r: R2) -> App<R2, E>;
    pub fn with_executor<E2: Spawner>(self, e: E2) -> App<R, E2>;   // typestate builder
    pub fn run_headless(self, size: Size) -> Headless<R, E>;
}

// blanket impls = the dynamic escape hatch, opt-in by the consumer
impl<R: Renderer + ?Sized> Renderer for Box<R> {}
impl<S: Spawner  + ?Sized> Spawner  for Box<S> {}
```

The spectrum the consumer chooses from (we owe only object-safe traits + the
blanket impls):

| Instantiation | Dispatch | Chosen at |
|---|---|---|
| `App<CpuRenderer, ThreadPoolSpawner>` | static, zero-cost | compile time |
| `App<Box<dyn Renderer>, Box<dyn Spawner>>` | one vtable hop, swappable | runtime |
| consumer's own `DynRuntime(Box<dyn Runtime>)` | erase whole runtime | runtime |

- Default path (`App::new(build).run_headless(size)`): `CpuRenderer` +
  `InlineSpawner` — deterministic, just works, no params typed.
- Shell: `App::new(build).with_executor(ThreadPoolSpawner::new())` (+ GPU renderer
  when that backend lands).
- Object-safety is the one constraint: `Spawner` uses `BoxFuture`/`Box<dyn
  FnOnce>` args; `Renderer` is already object-safe (the codebase already holds
  `Box<dyn Renderer>` today, so `R = Box<dyn Renderer>` is a non-breaking
  reframing of current behavior).

---

# Sequencing

```
A. Renderer generics  (no new deps — pure refactor)
   A1 blanket Box impl → A2 Headless<R>/App<R> → A3 agent generic → A4 shell pins → A5 tests
        │  (independent; can land first and stand alone)
        ▼
B. Data layer (no new deps for the core)
   B0 lumen-tasks: DeferredOp queue + Spawner + Sink + InlineSpawner + ManualSpawner
   B1 BuildCx.tasks (TaskRequest) + pump drain/dispatch + generation/cancel
   B2 L2 resource/resource_blocking (+ Resource data+flags) ; L1 task/task_blocking ; streaming
   B3 ThreadPoolSpawner (std threads) + shell waker wiring (set_waker via EventLoopProxy)
        │
        ▼
C. Unify App<R, E> (fold E in next to R; both defaulted) + the blanket Spawner impl
        │
        ▼
D. (Additive, ADR-003) async layer: WasmSpawner + an async runtime + HTTP client
   (or ureq behind a feature); optional lumen-net. Promotes the websocket example
   off its blocking round-trip; unblocks pokedex/download_progress.
```

Each phase is independently committable with the suite green. A–C ship a fully
usable data layer (one-off, streaming, heavy-thread) on desktop/Android with
deterministic headless tests **and zero new dependencies**. Only D escalates
ADR-003.

# ADR-003 implications

- **A, B, C: none.** Pure-std (threads, channels, futures core via `core::future`
  + a hand-rolled block-on for `InlineSpawner`). `BoxFuture` can use a tiny
  `futures-core`-free alias (`Pin<Box<dyn Future<Output=()> + Send>>`) — no dep.
- **D only:** `wasm-bindgen-futures` (wasm), and an async runtime + HTTP client
  (`tokio`+`reqwest`, or `smol`, or blocking `ureq`) — each a decision-log entry.
  Recommend evaluating `ureq` (blocking, tiny) first since the thread-pool model
  already covers concurrency for a GUI app.

# Risks & mitigations

- **Generic blast radius (agent).** Mitigated by defaulted params (most code
  untouched) + mechanical generic pass-through; `RuntimeView` trait in reserve.
- **`InlineSpawner` block-on on a truly-suspending future** would block the UI
  thread. Mitigation: the default inline executor is for tests/compute; real I/O
  uses `ThreadPoolSpawner`/async (D). Document it; `ManualSpawner` for timing
  tests.
- **Determinism of streams** (many values, arbitrary times): recorded as a
  sequence of typed `push` ops; replay re-applies the sequence. Same model as
  input.
- **Waker plumbing** is host-specific; keep it an `Option<Waker>` set by the shell
  (inert headless), mirroring scale/renderer wiring.

# Acceptance

1. Renderer: workspace green; `Headless<Box<dyn Renderer>>` compiles; CPU goldens
   byte-identical; no `dyn` in the default `Headless` (size check).
2. Data layer (headless, `ManualSpawner`): a `resource` goes
   `loading → ready(value)` across `run_pending()`+`pump`; a dep change refetches
   keeping the stale value (`Resource.value` stays `Some` while `loading`); a
   `task_blocking` streams ≥2 progress values into a signal; a dropped key
   cancels (stale result is a no-op).
3. Determinism: a resource test under `InlineSpawner` is bit-stable across runs;
   the `mutate` hatch is documented as non-replayable.
4. `App<R, E>` defaults compile param-free; `with_renderer`/`with_executor` change
   the type; the blanket boxed impls give the runtime escape hatch.
