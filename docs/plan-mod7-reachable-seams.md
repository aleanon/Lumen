# MOD7 — make the seams reachable (2026-08-24)

> **Status: implemented.** S0–S4 landed; S5 declined with reasons in
> `docs/mod7-s5-builder-decision.md`. Outcome, measured on the same two
> binaries the plan opens with, now both **windowed**:
> **lumen-lean-app 15.96 MB → lumen-stubtext-app 10.09 MB, a 5.87 MB saving
> available to an app that opens a window.** Before MOD7 that number was
> headless-only.
>
> Two corrections to this document, kept rather than edited away:
> * **S1's acceptance criterion was wrong when written.** It said the stub
>   binary "stays under 3 MB", which took the 1.41 MB headless figure and
>   forgot that linking the shell adds ~8.7 MB whatever text engine is chosen.
>   10.09 MB against a 15.96 MB baseline is the same claim, stated honestly.
> * **S1 also promised to honour a caller-supplied executor.** That moved to
>   S2, which folds `E` into the config anyway — doing it twice would have been
>   work S2 immediately undid. It landed there as `run_with`.

Lumen already has seven swap axes and they are real: each has a second
implementation, and `platform_config.rs` proves the bundle is consulted rather
than merely declared. This plan is not about adding seams. **It is about the
fact that almost none of them can be reached by an app that opens a window.**

## The measured prize

Two binaries, identical source, differing in one associated type
(`benches-competitive/harnesses/apps/lumen-{stub,default}text`):

| | `.text` | `.rodata` | total | font | cjdict |
|---|---:|---:|---:|---|---|
| `type Text = StubEngine` | 1.21 MB | 0.06 MB | **1.41 MB** | no | no |
| `type Text = TextEngine` | 2.68 MB | 4.46 MB | **7.45 MB** | yes | yes |

**6.04 MB from one associated type.** With LTO the linker drops parley, swash,
skrifa, harfrust, the ICU tables and the embedded fonts once nothing
instantiates the default engine — more than the 3.62 MB the `complex-scripts`
feature bought (LN3). A seam is a size lever, not only a flexibility one.

Both probes are self-verifying: they lay out `ABCDEFGH` and print 64.0×18.0 px
under the stub against 84.0×21.0 px under parley, so the size figure is
attributed rather than asserted.

**That 6.04 MB is currently unreachable.** The probe runs headless because it
has to.

## Three defects, found by trying to use the seams

**D1 — the builder discards the platform.** `with_renderer` and
`with_executor` are typed `-> App<R2, E>` and `-> App<R, E2>`; the third
parameter is absent, so it falls back to `DefaultPlatform`. Compiling

```rust
let a = App::<_, _, MyPlatform>::with_platform(view);
let b = a.with_renderer(TinySkia::default());
```

and annotating `b` yields `expected App<_, _, MyPlatform>, found App<_, _,
DefaultPlatform>`. **Without the annotation it compiles**, and the app silently
runs on the default text and layout engines. A custom platform therefore cannot
be combined with a custom renderer or executor at all.

**D2 — the shell pins all three axes.**

```rust
pub fn run(app: App, size: Size)                  // fully-defaulted App
impl RunExt for App                               // only that instantiation
type ShellApp = App<Box<dyn Renderer>, ThreadPoolSpawner>;   // P never threaded
```

So a windowed app gets: the default layout and text engines, an executor the
shell overwrites with `ThreadPoolSpawner`, and a renderer chosen internally by
adapter presence. Five sites in `lumen-shell/src/lib.rs` name the concrete
types; ten public signatures across the workspace mention `App` at all, so the
blast radius is small.

**D3 — there is no tuning axis.** The knobs that decide "low memory vs fast
frames" are hardcoded `const`s and are not part of any seam:

```
GLYPH_CACHE_CAP      8192     SHAPE_CACHE_CAP       2048
RUN_CACHE_CAP        4096     SHAPE_CACHE_HARD_CAP 16384
MAX_CACHED_IMAGES      64     RUN_CACHE_HARD_CAP   32768
MAX_CACHED_ANIMATIONS  32     MAX_DEFAULT_THREADS      4
```

## Three mechanisms, and picking the wrong one wastes the work

Established by measurement this month, not by taste:

* **Type parameter** — when the *algorithm* differs. Worth **6.04 MB** on text.
  Costs compile time, not bytes: one instantiation monomorphises to exactly
  what hardcoding the same types produces. (A `Box<dyn>` seam is the one that
  costs, through vtables and lost inlining.)
* **Cargo feature** — when the cost is *code and data presence* and no
  alternative implementation exists. `complex-scripts`: 3.62 MB for a manifest
  edit, no new code (LN3).
* **Runtime value** — when it is policy. The consts above. Wants a `const` on
  the config, not a trait.

## Stages

### S0 — the builder preserves the platform *(prerequisite, hours)*
Retype `with_renderer` / `with_executor` as `-> App<R2, E, P>` / `-> App<R, E2, P>`.
Pure type-level; the bodies already carry `PhantomData`. **Accept:** a
compile-fail test asserting a custom platform survives a `with_renderer` chain
— the defect above is silent without one.

### S1 — thread `P` through the shell *(the unlock, 1–2 days)*
Generalise `run`, `RunExt`, `ShellApp`, `ShellHeadless` and the window-open
path over `P: PlatformConfig`, and honour a caller-supplied executor instead of
overwriting it. **Accept:** `lumen-stubtext` builds *windowed* and the live
gate opens it; its binary stays under 3 MB. This is what converts 6.04 MB from
a measurement into a shipping option, and it is worth more than every later
stage combined.

### S2 — fold `R` and `E` into the config *(2–3 days)*
`Headless<R, E, P>` → `Headless<C: AppConfig>` with `Renderer`, `Spawner`,
`Layout`, `Text` as associated types — the `Runtime<MyAppConfig>` shape. One
parameter to name instead of three, which also removes the return-position
inference problem `01-architecture.md` records as the reason `App::new` is not
generic. **Risk:** `with_renderer`-style incremental swapping gets harder —
changing one axis means naming a whole config. Mitigated by S4.
**Do S0/S1 first regardless**; they are useful whether or not S2 lands.

### S3 — tuning on the config *(1 day)*
`const TUNING: Tuning` (or `fn tuning()`) carrying the D3 consts. Covers the
memory-vs-speed axis nothing covers today, and composes with the bundle rather
than competing. **Accept:** a config with quartered caches measurably lowers
idle RSS on the BENCH4 harness.

### S4 — named presets *(1 day)*
`Lean`, `Balanced`, `Desktop`. The common cases become one word and a custom
config is the escape hatch rather than the entry fee. Only worth doing after
S2, or the presets have three parameters to set.

### S5 — builder ergonomics *(evaluate, may decline)*
A typestate builder is sugar over whatever parameter list survives S2 — it
cannot replace the parameters, only hide them. `with_renderer`/`with_executor`
are already a typestate builder (their doc comments say so) and D1 is what that
pattern costs when a parameter is added later. **Decide after S2, not before.**

## What this plan will not do

**Not swapping the state store.** MOD6 measured it: keeping `ui.getDeps`
attribution out of third-party hands forces two map lookups where there is now
one — **+117.6% on signal writes**, on the hottest path in the framework.
`docs/mod6-state-store-decision.md`. The precedent that matters is procedural:
each new seam is measured before it is promised.

**Not promising a smaller binary from S1 alone.** S1 makes the 6.04 MB
reachable; it does not deliver it. Someone must supply a text engine that is
both smaller and real — the probe's `StubEngine` is 8-px-per-char fiction. The
honest claim is "the lever exists and can now be pulled", not "apps get 6 MB".

## Open questions

* Does a genuinely smaller *real* text engine exist, or would one have to be
  written? Without an answer, S1's value is optionality rather than bytes.
* Should `Style` join the bundle? MOD4 chose runtime registration
  (`register_property`) over a type swap, so there is nothing for an associated
  type to name — revisit only if a second cascade implementation appears.
* Does `Box<dyn Renderer>` stay a first-class option after S2? It is what the
  shell uses today, and it is the one shape where the generic seam costs size
  rather than saving it.
