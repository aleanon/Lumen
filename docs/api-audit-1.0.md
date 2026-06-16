# Public API audit & 1.0 freeze (T4.5)

The public surface is the `lumen` facade (02 §11); internal crates
(`lumen-core`, `-render`, `-layout`, `-text`, `-style`, `-widgets`, `-shell*`)
are reached only through it, except `lumen-test`/`lumen-agent`/`lumen-cli`
which are their own user-facing tools.

## Frozen `lumen` facade re-exports
- Core: `Color`, `Diagnostic`, `Severity`, `SourceSpan`, `NodeIndex`,
  `StableId`, `codes`, `geometry`, `state`, `events`, `semantics`.
- App: `App`, `AppSnapshot`, `BuildCx`, `Element`, `Handler`, `Headless`,
  `FrameStats`; `widgets` (M0/M1/M3/M4 + `widgets_extra` remaining set);
  `layout`, `render`, `text` namespaces.
- Desktop shell (`cfg(not(android|ios))`): `run`, `RunExt`.

## Audit results
- Every public item carries rustdoc (`#![warn(missing_docs)]` across all
  crates) and `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` is
  clean (no broken links, no collisions).
- Constructors follow one convention: `fn widget(cx?, name, …) -> Element`;
  stateful widgets own a signal keyed by `name`. Diagnostics use stable
  `E####/W####` codes only (ADR-019).
- Naming reviewed for consistency (`set_*`/`get`/`with_*`); no leaking of
  internal types (`taffy`, `tiny-skia`, `wgpu`, `parley`) in the facade.

## semver freeze
This snapshot is the **1.0 public API baseline**. `cargo-semver-checks` gates
every subsequent release against it; because nothing is published yet there is
no prior baseline to diff, so the first publish establishes it. Pre-1.0 the
workspace is lockstep `0.0.0`; the freeze is the contract going forward.
