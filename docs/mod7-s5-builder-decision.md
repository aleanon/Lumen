# MOD7 S5 — the extra builder is declined. Reasoned, not assumed.

*2026-08-24. In the shape MOD6 and CP5 established: "stop" is a permitted
outcome, and the reasoning is written down rather than left implicit.*

S5 asked for a typestate builder for configuration — the iced-style
`Lumen::builder().renderer(..).text(..).build()`. The plan deferred the call
until after S2 on the grounds that a builder is sugar over whatever parameter
list survives, and cannot replace parameters, only hide them. S2 has landed, so
here is the call.

## What a consumer writes today, after S0–S4

| goal | spelling | lines |
|---|---|---:|
| defaults | `App::new(view)` | 1 |
| a shipped preset | `ConfiguredApp::<Desktop>::with_config(view)` | 1 |
| swap the renderer only | `App::new(view).with_renderer(r)` | 1 |
| swap the executor only | `App::new(view).with_executor(e)` | 1 |
| swap layout and/or text | `impl PlatformConfig for Mine { … }` + `with_platform` | ~5 |
| swap all four | `impl AppConfig for Mine { … }` + `with_config` | ~10 |

Five entry points, each covering a distinct case, none redundant.

## Why a sixth does not help

**A builder cannot remove the type declaration, which is the only real cost
left.** The two multi-line rows above are multi-line because associated types
need a *named type* to attach to. `impl AppConfig for Mine` is that name. No
builder syntax removes it, because the requirement is Rust's, not the API's.

**The value-shaped alternative would trade away the thing this whole plan was
for.** A builder that takes engines as *values* — `App::new(view).text(engine)`
— would have to store `Box<dyn TextEngineApi>`, moving text behind dynamic
dispatch. The measured prize of MOD7 is 5.87 MB in a windowed binary, and it
comes from the linker proving the default engine is never instantiated. Putting
the engine behind a trait object makes that proof harder in exactly the case it
matters, and adds a vtable hop to `shaped_run`, which the F2 profiling put on
the hot path. Spending the headline result on syntax is a bad trade.

**Each added parameter is a chance for a builder method to silently drop one,
and that already happened.** MOD7 S0 fixed `with_renderer` returning
`App<R2, E>` — the platform parameter simply absent, defaulting, so a custom
bundle reverted to the shipped one and the app ran on engines the author had
replaced. It compiled. Nothing caught it until someone tried to use the seam.
`with_renderer`/`with_executor` *are* a typestate builder — their doc comments
say so — and D1 is what that pattern costs when a parameter is added later.
More builder surface is more of that risk, for no new capability.

## What would change this

* **A second layout or text implementation actually shipping.** Today the
  presets can only vary the choices *around* the engines, because there is no
  second engine to name. If one existed, `Lean` could name it and the
  multi-line rows above would collapse to a preset — which is a preset problem,
  not a builder problem.
* **Evidence that the ~10-line `impl AppConfig` is a real barrier.** It is
  boilerplate, but it is boilerplate a consumer writes once per application. If
  that turns out to be the thing keeping people on the defaults, a derive macro
  would address it far more directly than a builder, and without touching
  dispatch.

## Status

S0, S1, S2, S3 and S4 landed. S5 is closed as **declined**, revisitable on
either trigger above.
