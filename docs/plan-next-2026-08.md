# Plan — live-window verification, wgpu upgrade, executors, CP5.1

*Written 2026-08-12. Four independent workstreams; the ordering between them is
not arbitrary (see below). Nothing here is started.*

## Why this order

**LW must land before WG.** The wgpu upgrade's entire risk surface — surface
attach/configure/resize, swapchain acquire, present, texture limits — is the
part of the codebase that **no test touches**. 394 headless suites run on
`TinySkia`, which has no swapchain and no texture-dimension concept. Five real
bugs surfaced in the week to 2026-08-12; the three that reached a user were all
in that surface, and all three were found by a downstream app or by reading
code, never by CI:

| bug | found by | caught by the suite? |
|---|---|---|
| oversize shadow → `create_texture` panic | Mercurium, live window | no |
| `src_rect` ignored on GPU (shadows painted wrong) | reading code | no |
| >2048 px window aborts at open | inspection | no |
| secondary-window wheel inverted | reading code | no |
| resize storm → `Surface::configure` panic | Mercurium, live window | no |

Upgrading wgpu across eight major versions with that hole open means the first
real signal would again come from a downstream app. LW closes it for ~a day of
work using pieces that are all already proven in this repo.

EX and CP5.1 are independent of both and of each other.

---

# LW — Live-window smoke gate

**Goal.** One command that opens real OS windows on a real GPU, drives them
through the same paths a user does, abuses them, and fails loudly. Every
component below has already been run by hand in this repo; this is assembly and
assertion, not research.

The pieces, and where they are proven:

* `just run-agent <example>` — real winit window + JSON-RPC endpoint (M4).
* `scripts/agent_client.py wait-port` / `call <verb> <json>` — used live on
  2026-08-11 to verify click-on-release end to end.
* `wmctrl -i -r <id> -e 0,x,y,w,h` in a loop — reproduced the SR1 crash in 129
  resizes on 2026-08-12.
* Assertion surface already exposed by the agent: `ui.getTree`, `ui.getLayout`,
  `ui.probe` / `ui.probeRegion` (pixels), `ui.screenshot`, `ui.lint`,
  `app.diagnostics`, `app.perf`, `ui.getWindows`, `input.*`.

That last line is the point: a live window can be asserted on as precisely as a
headless test, not just "did the process survive".

## LW1 — `scripts/live_window_gate.sh`, single-window legs

House style: a `scripts/*_gate.sh` that exits non-zero, like `perf_gate.sh` /
`web_gate.sh`. Boots one agent window per leg, asserts, quits via `app.quit`,
and **fails if the process died** at any point (the shell's panic is the failure
mode we care about, and it is invisible to an RPC that simply stops answering —
so poll the pid, not just the socket).

Legs, each mapped to a bug it would have caught:

| leg | what it does | would have caught |
|---|---|---|
| `boot` | window opens; `ui.getTree` non-empty; log says `present = direct-to-surface` | GPU init regressions |
| `resize-storm` | 400 randomized `wmctrl` resizes, 300–1900 px | **SR1** |
| `oversize` | one resize past 2048 px in each axis; assert alive and still presenting (or a clean CPU-readback fallback, not a panic) | **the >2048 px abort** |
| `shadow-ink` | a card with `Shadow::soft()`; `ui.probeRegion` outside the card's own fill is non-background | **the `src_rect` drop** — shadows painted wrong on GPU for months |
| `input` | `input.click` a button, assert state via `ui.getTree`; `input.drag` off it, assert nothing fired | click-on-release regressions |
| `diagnostics` | `app.diagnostics` is empty on a clean example | W0110 false positives |

## LW2 — multi-window leg

`ui.getWindows` + a secondary window driven with `input.scroll`, asserting the
scroll moves **the same direction** as in the primary. This is the wheel
inversion, and it is worth its own leg because the multi-window code path is a
near-copy of the primary one and drifted from it silently.

## LW3 — `just live-gate` + CI job

* `just live-gate [example]` for local use.
* CI job on `ubuntu-latest` with lavapipe **and** `xvfb-run`, so a real X
  display exists. Model it on the existing `gpu` job, which is already blocking
  and already installs `mesa-vulkan-drivers`.
* `LUMEN_REQUIRE_GPU=1`-style strictness: **fail if the display or adapter is
  missing.** A gate that self-skips reports green while proving nothing — the
  lesson already written into the `gpu` job's comment.
* Expect flake pressure: window-manager timing under xvfb is not deterministic.
  Mitigation is bounded retries **per leg with a logged retry count**, never a
  silent one, plus a generous `wait-port` timeout. If a leg proves genuinely
  flaky in CI, it stays in `just live-gate` and is dropped from CI *explicitly
  in the script*, not by deleting the assertion.

## LW4 — document it

`docs/live-window-gate.md`: what each leg proves, which historical bug it
corresponds to, and how to add a leg. Plus a line in the `verifying-apps` skill
so the next agent knows a live gate exists and does not re-derive it.

**Acceptance.** `just live-gate` green on this box; the SR1 commit reverted
makes the `resize-storm` leg fail (the gate is proven against a real bug, the
same discipline as the ablations in TS1/SR1).

**Explicit non-goal.** This is a *smoke* gate: liveness, crash-freedom, gross
correctness. It is not a golden-image system for GPU output — `cpu_vs_gpu`
already owns pixel parity.

---

# WG — wgpu 22 → 30

**Read this first: the upgrade does not fix the SR1 crash class.** Verified
against the wgpu 29 docs — `Surface::configure` still returns `()`:

```rust
fn configure(&self, device: &DispatchDevice, config: &SurfaceConfiguration);
```

so a configure that races a resize is still fatal-by-construction, at
`attach_surface`, `resize_surface` and `Presenter::new` alike. **Do not sell the
upgrade as the fix for that.** The justification is being eight majors behind on
the crate that owns our entire GPU surface: security/driver fixes, newer
Vulkan/Metal backends, and the compounding cost of deferring.

## WG0 — decide the target version

30.0.0 is current. **Only three crates in the lockfile depend on wgpu, all
ours** (`lumen-render`, `lumen-shell`, the `integration` example) — no
third-party pin constrains us, which removes the usual blocker. The real
constraint to check is `raw-window-handle` (we are on 0.6.2 via winit 0.30.13):
if wgpu 30 wants a newer rwh major, winit must move too, and that pulls
`accesskit_winit` 0.23.1 and `softbuffer` 0.4.8 with it. **Check that before
committing to 30**; landing on 29 is an acceptable outcome if 30 forces a winit
major.

## WG1 — mechanical renames

Confirmed from the 29 docs:

* `ImageCopyTexture` → `TexelCopyTextureInfo` (6 uses)
* `ImageDataLayout` → `TexelCopyBufferLayout` (6 uses)
* `request_adapter` returns `Result` (was `Option`) — affects `Wgpu::new`'s
  PRIMARY-then-SECONDARY `find_map`, which is load-bearing (the GL gradient
  defect, `docs/gl-backend-gradient-defect.md`); keep that ordering intact.
* `request_device(&desc)` — single argument, the trace path is gone.

The full surface is ~60 distinct `wgpu::` items across `gpu.rs` and the shell;
the compiler enumerates the rest.

## WG2 — the acquire path, which changes shape

`get_current_texture` no longer returns `Result<_, SurfaceError>`; 29 exposes a
`CurrentSurfaceTexture` enum whose docs state the contract we already implement
by hand: *"If `Outdated`, the surface configuration is invalid — call
`configure()` again and retry. `Lost` means the surface must be recreated."*

This maps **onto `Present::{Done, Skipped, Unavailable}` almost exactly** —
`Outdated` → reconfigure + retry → `Skipped`; `Lost` → `Unavailable` (recreate,
which we cannot do in place, so fall back). The SR1 type was designed for
wgpu 22's `SurfaceError` and turns out to be the right shape for 30's enum,
which is a good sign for it. Re-derive the mapping from the new variants rather
than porting the old match arm by arm.

## WG3 — limits, and the guards that depend on them

`downlevel_defaults().using_resolution(adapter.limits())` and the
`MAX_DIM_CEILING = 8192` clamp are the load-bearing part of the oversize
hardening. **Re-verify that `using_resolution` still copies only the three
`max_texture_dimension_*` fields** — that was checked by reading wgpu-types 22
source and must be re-checked, because a change there would silently weaken
storage/workgroup limits. `texture_limits.rs` covers the behaviour; the
verification is of the *mechanism*.

## WG4 — verification

* `cargo test --workspace` + the GPU suite on **both** drivers (native Vulkan
  and lavapipe via `VK_DRIVER_FILES`), because the GL-vs-Vulkan adapter choice
  has bitten before and a wgpu major is exactly when it would change.
* `cpu_vs_gpu` parity corpus — the perceptual budget is the real regression
  detector for a backend upgrade.
* **`just live-gate` (LW).** This is the payoff for the ordering: the upgrade's
  risk lives in the paths only that gate exercises.
* Re-run `scripts/size_gate.sh` — wgpu majors move binary size, and the lean
  profile has three legs that will notice.

**Rollback.** One commit, one `Cargo.toml` version bump plus mechanical
changes. If the live gate fails and the cause isn't obvious within a session,
revert and file what was learned. There is no partial-migration state worth
holding.

---

# EX — Executor adapters (tokio, smol)

**No HTTP.** Explicitly out of scope, now and prospectively: transport is the
app's choice, and Lumen's job is to run the app's futures on the app's runtime.
This work is the seam that makes "bring your own" actually workable.

## Why the current seam is not enough

`Spawner` is already the right abstraction and already has four implementations
(`InlineSpawner`, `ManualSpawner`, `ThreadPoolSpawner`, `WasmSpawner`), and
`App::with_executor<E2: Spawner>` installs one at compile time. The gap is what
`ThreadPoolSpawner` actually does with a future:

```rust
fn spawn(&self, fut: BoxFuture) -> Box<dyn TaskHandle> {
    self.queue(Box::new(move || block_on(fut)))
}
```

It **blocks a pool thread per future**. Three consequences:

1. Concurrency is capped at the pool size (4 by default, per CACHE1) — four
   in-flight awaits and the fifth waits, however idle they are.
2. Any future needing a reactor — a tokio timer, tokio TCP, anything from the
   tokio ecosystem — does not merely underperform, it **panics or hangs**:
   there is no runtime in the thread's context.
3. Cancellation is cooperative only. `SkipFlag` drops a job that has not started
   and can do nothing once a worker is inside it, as its own doc says.

A user bringing their own HTTP client brings its runtime requirements with it.
That is the concrete blocker.

## EX1 — where the adapters live: a new leaf crate `lumen-exec`

Not features on `lumen-core`. This repo has been bitten twice by exactly that
shape — GX3 (an inherited link re-enabling defaults across a five-crate chain,
a 21.9 MB "lean" wasm module) and CFG1 (four separate `{ workspace = true }`
links each silently keeping wgpu). `lumen-core` is the base of everything, so an
optional `tokio` feature there is one careless unification away from being on
everywhere.

A leaf crate that **nothing else in the workspace depends on** cannot leak by
feature unification. It depends on `lumen-core` (path dep, `default-features =
false`, per the GX3 rule) and carries `tokio` / `smol` as optional features,
both **off by default**. The facade exposes it as `lumen::exec` only when
enabled.

## EX2 — `TokioSpawner`

Three constructors, because the two situations are genuinely different:

* `from_handle(tokio::runtime::Handle)` — the app already has a runtime. The
  expected case, and the cheapest: the adapter borrows, owns nothing.
* `multi_thread()` / `current_thread()` — the adapter builds and owns a runtime
  (`Arc<Runtime>`), for an app that wants tokio without managing it.

Mapping:

* `spawn` → `handle.spawn(fut)`, keep the `JoinHandle`, `TaskHandle::abort()` →
  `JoinHandle::abort()`. **This is a real capability upgrade**: the task stops at
  its next await point instead of waiting for a cooperative flag.
* `spawn_blocking` → `handle.spawn_blocking(f)`. Note honestly that tokio's
  blocking pool cannot interrupt a running closure either — that limitation is
  the runtime's, not ours, and belongs in the doc.

**Two hazards to encode as tests or doc, not discover later:**

* **Dropping a `Runtime` inside async context panics**, and dropping it at all
  blocks until tasks finish. An owned-runtime `TokioSpawner` dropped on the UI
  thread during teardown is a plausible hang. Document the ownership contract;
  consider `shutdown_background()` on drop.
* `Handle::spawn` from a non-runtime thread is fine — but `block_on` inside a
  spawned task is not. Our `Sink` path does not do that today; a test should
  keep it that way.

## EX3 — `SmolSpawner`

* `spawn` → `smol::spawn(fut)` → a `Task`, which **cancels on drop** — so the
  handle can hold the `Task` and `abort()` drops it. Cleaner than tokio's model.
* `spawn_blocking` → `blocking::unblock(f)`.
* smol's global executor needs threads driving it; whether the adapter starts
  them or requires the app to, is the one design decision here. Prefer
  `async-global-executor`'s implicit behaviour and **state which it is** in the
  doc rather than leaving it ambient.

Both adapters are `#[cfg(not(target_arch = "wasm32"))]` — `BoxFuture` is `!Send`
on wasm and `WasmSpawner` already owns that platform.

## EX4 — verification: parameterize the TC1 battery

The strongest available acceptance is **not** new tests: it is running the
existing cancellation suite against each spawner. TC1 already covers scope
death, deps superseding, memo-skip survival, handler-driven abort, and
restart-after-cancel (`lumen-widgets/tests/data_layer.rs`,
`examples/download_progress/tests/smoke.rs`). Extract that battery into a
generic harness over `E: Spawner` and instantiate it for `InlineSpawner`,
`ThreadPoolSpawner`, `TokioSpawner`, `SmolSpawner`.

That produces a real, non-tautological result: the three cooperative-cancel
tests must **behave differently** under tokio/smol (the task actually stops).
Where behaviour legitimately differs, the harness takes a capability parameter
rather than asserting the weakest common denominator — the point is to document
that abort is genuinely stronger on a real runtime.

Plus: a timer test (`tokio::time::sleep` resolves under `TokioSpawner` and
hangs/panics under `ThreadPoolSpawner`) — that single test is the entire
justification for this workstream, made executable.

**Goldens and determinism are untouched:** tests keep `InlineSpawner` /
`ManualSpawner`. The adapters are for apps.

**ADR.** `tokio` and `smol` are ADR-003 escalations even as optional deps of an
opt-in crate. Write the ADR entry with the leaf-crate containment argument; that
is the decision being made, not the code.

---

# CP5.1 — "does a memo hit have to re-lower?"

The measurement the CP5 gate said it owed the record (`docs/cp5-gate-decision.md`).
**Ship nothing.**

`scoped_vs_flat = 0.787` today, and after OB2 the picture moved again: a
memoized rebuild with one dirty row costs ~79% of rebuilding everything, because
`copy_span` re-derives the lowered subtree — rebuilding taffy nodes and
re-inserting side-table entries per node. The incremental architecture is
incremental in *closure evaluation* only.

* **CP5.1a** — prototype: keep the lowered node for an unchanged span instead of
  re-deriving it in `copy_span`. Behind a flag, correctness gated by
  `assert_view_coherent` (incremental ≡ rebuild-fresh) plus goldens.
* **CP5.1b** — report the new ratio and delete the prototype.

The gate's own framing: near **0.49** and CP6 (persisting the arenas — the
campaign's one-way door) has a real case for apps that memoize, to be re-gated
alongside CP4. Near **0.787** and the retained graph is dead **on measurement**
rather than on a quarantined number that was never derived — which is a better
grave than the one it currently has.

Bounded, reversible, and it closes an open record either way. It does **not**
move the egui comparison: BENCH1's workload has no `cx.scope`, so a
retained-lowering win applies to neither side of that ratio.

---

# Sequencing

```
LW1 → LW2 → LW3 → LW4          (gate first; it protects WG)
                  └→ WG0 → WG1 → WG2 → WG3 → WG4
EX1 → EX2 → EX3 → EX4          (independent; needs an ADR)
CP5.1a → CP5.1b                (independent; measurement only)
```

LW is the only hard prerequisite. EX and CP5.1 can run in any order relative to
the others; EX is the one with a user waiting on it, CP5.1 is the one with a
written promise outstanding.
