//! Lumen — the public facade crate.
//!
//! User code and examples depend only on `lumen` (and `lumen-test`); nothing
//! imports the internal crates directly (02 §11). This crate re-exports the
//! stable public API.
#![warn(missing_docs)]

#[doc(inline)]
pub use lumen_core::{geometry, Color, Diagnostic, NodeIndex, Severity, SourceSpan, StableId};

/// Diagnostic codes (stable API, ADR-019).
pub use lumen_core::codes;

/// Reactive signals and the state store (02 §4).
pub use lumen_core::state;

/// Events and input (02 §6).
pub use lumen_core::events;

/// The semantic tree, selectors, and JSON export (03 §1–§2).
pub use lumen_core::semantics;

/// The application and headless runtime (02 §8).
#[doc(inline)]
pub use lumen_widgets::{app::FrameStats, App, BuildCx, Element, Handler, Headless};

/// Types named in `Element`'s public builders, so those builders are actually
/// callable through the facade (SD4).
///
/// `Shadow` is `.shadow()`'s parameter and `Dynamic<T>` is the parameter of all
/// three reactive binders (`bind_text`/`bind_background`/`bind_class`). Both
/// were missing, which made five public methods impossible to *name* — not
/// deprecated, not gated, simply unreachable for anyone depending on `lumen`
/// rather than on `lumen-widgets`. Pinned by `tests/facade_complete.rs`.
#[doc(inline)]
pub use lumen_core::{Dynamic, Signal};
#[doc(inline)]
pub use lumen_widgets::element::Shadow;
#[cfg(feature = "snapshot")]
pub use lumen_widgets::{AppSnapshot, Checkpoint};

/// Pick the renderer from `--wgpu`/`--tiny-skia`/`LUMEN_RENDERER` (else `None`).
#[doc(inline)]
pub use lumen_widgets::renderer_override;

/// The built-in widget library (02 §10): the whole catalogue under one
/// `widgets` namespace, the accessibility bridge, and the M5 app-building
/// modules (forms, navigation, undo, i18n, desktop system integration).
///
/// SD2 retired the milestone-named modules (`widgets_m1`/`m3`/`m4`/`extra`).
/// They exposed *when* a widget was added — an internal scheduling fact that
/// meant nothing to a consumer and could not be reorganized without a breaking
/// change. Everything now lives in `widgets`.
pub use lumen_widgets::{forms, i18n, nav, system, theme, undo, widgets};

/// A11Y1: the AccessKit tree builder, present only with the `accessibility`
/// feature (default-on). It is the only part of the accessibility story that is
/// gated — `semantics_json`, `semantics_elided` and selector lookup are the
/// observability contract and are always available.
#[cfg(feature = "accessibility")]
pub use lumen_widgets::a11y;

/// Cached decoded assets, and the hook to release them.
///
/// Exposed for MOB1/MOB2: a platform shell (or an app) must be able to drop
/// derived caches when the OS signals memory pressure. It was previously
/// unreachable through the facade, so an app had no way to respond to a
/// low-memory warning even if it wanted to.
pub use lumen_widgets::asset;

/// The ShaderWidget (GPU; `wgpu` feature, not available on wasm).
#[cfg(all(feature = "wgpu", not(target_arch = "wasm32")))]
pub use lumen_widgets::shader;

/// Layout styling (the typed mirror of the `.lss` layout properties, 04 §3).
pub use lumen_layout as layout;

/// Executor adapters (`exec-tokio` / `exec-smol`): run an app's background work
/// on a real async runtime rather than by blocking a thread-pool thread. Absent
/// unless one of those features is on, so a default build has no runtime crate
/// in its graph. See [`lumen_exec`] for why the built-in `ThreadPoolSpawner` is
/// not enough for reactor-dependent futures.
#[cfg(any(feature = "exec-tokio", feature = "exec-smol"))]
pub use lumen_exec as exec;
/// The display list and CPU renderer (02 §7).
pub use lumen_render as render;

/// Text shaping and layout (ADR-005).
pub use lumen_text as text;

/// The `.lss` style engine (04): `Style`, the parser, and the property
/// registry. `Element::css` takes a `lumen_style::Style`, so without this the
/// facade exposed a builder whose argument type it did not export (SD4).
pub use lumen_style as style;

/// The desktop window shell. `use lumen::RunExt` to call `app.run(size)` (02 §8).
/// Desktop-only; mobile + web targets use their own shells.
#[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
pub use lumen_shell::{run, RunExt};
