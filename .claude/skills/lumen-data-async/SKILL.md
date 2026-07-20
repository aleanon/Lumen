---
name: lumen-data-async
description: Use when a Lumen app needs network/IO data — HTTP fetches, downloads with progress, streams, or any async work. Encodes ADR-M2 (the framework ships NO HTTP client; you bring the transport through the executor seam), the three canonical recipes (ureq on the thread pool, tokio/reqwest via a custom Spawner, browser fetch on wasm), the Sink re-entry contract, and the transport-injection pattern that keeps tests offline.
---

# Data & async in Lumen (ADR-M2: bring your own client)

The framework ships the **executor seam**, not a transport. `cx.resource` /
`cx.task` run your closure off the UI thread on whatever `Spawner` the app
was built with; results re-enter through a `Sink` (the only handle that
crosses threads). Nothing HTTP ships in the framework tree — pick the client
that fits your runtime.

## The seam

| Piece | What it is |
|---|---|
| `Spawner` (lumen-core::tasks) | `spawn(BoxFuture)` + `spawn_blocking(FnOnce)` — Inline (deterministic default), ThreadPool (the shell's default), Manual (tests step through loading states), WasmSpawner (browser; RAF-driven) |
| `MaybeSend` | the platform bound: `Send` on native, nothing on wasm (`fetch` futures are `!Send`; no threads to cross). One generic API fits tokio, the pool, and `spawn_local`-style executors |
| `Sink` | completion channel back to the UI thread — `sink.set(sig, v)` / `sink.update` / `sink.mutate`. **The framework never drives foreign wakers**: your future completes on YOUR executor, only the result crosses |
| generation guard | `cx.resource` stamps each fetch; a late result from a superseded generation is discarded (tested: `data_layer.rs::stale_generation_result_is_discarded`) |

## Recipe 1 — blocking client on the thread pool (simplest; desktop)

`ureq` in *your* app (or dev-deps for examples), through `resource_blocking`:

```rust
let user = cx.resource_blocking::<User, String, _>("user", id, move |id| {
    ureq::get(&format!("https://api.example.com/users/{id}"))
        .call().map_err(|e| e.to_string())?
        .into_json::<User>().map_err(|e| e.to_string())
});
// user.loading / user.value / user.error — stale-while-revalidate on dep change
```

## Recipe 2 — async client on your runtime (tokio + reqwest)

Hand the app a `Spawner` that forwards to your runtime:

```rust
struct TokioSpawner(tokio::runtime::Handle);
impl lumen_core::tasks::Spawner for TokioSpawner {
    fn spawn(&self, fut: lumen_core::tasks::BoxFuture) { self.0.spawn(fut); }
    fn spawn_blocking(&self, f: Box<dyn FnOnce() + Send>) { self.0.spawn_blocking(f); }
}
let app = App::new(build).with_executor(TokioSpawner(handle));
// then cx.resource with an async reqwest closure — the future runs on tokio.
```

## Recipe 3 — wasm (browser fetch)

The web shell drives `WasmSpawner` from the RAF loop (`pump_wasm_tasks`).
Your fetch future comes from the JS glue (`!Send` is fine — `MaybeSend` is
empty on wasm); complete through the `Sink` exactly like native.

## Transport injection (offline tests — the pokedex pattern)

Don't call the client in `build`. Take the transport as a parameter and pick
it at the edge:

```rust
pub fn app(fetch: impl Fn(&str) -> Result<String, String> + MaybeSend + Clone + 'static) -> App
// win.rs:   app(|url| ureq::get(url).call()...)        // live client (dev-dep)
// tests:    app(|_| Ok(CANNED_JSON.into()))            // deterministic
```

`examples/pokedex` (fetch + decode + render) and
`examples/download_progress` (task_blocking streaming progress through Sink)
are the worked versions of these recipes.

## Gotchas

- `Sink` closures must NOT re-enter the runtime synchronously from another
  thread — they queue and apply on the next pump.
- `spawn_blocking` on wasm runs inline (single thread): keep jobs small.
- `Signal::update` closures must stay pure (no runtime re-entry) — same rule
  as everywhere.
- Waiting in tests: `ManualSpawner::run_pending()` steps the loading states
  deterministically; the agent's `ui.waitSettled` covers live apps.
