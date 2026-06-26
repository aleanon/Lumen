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
pub use lumen_widgets::{app::FrameStats, App, AppSnapshot, BuildCx, Element, Handler, Headless};

/// Pick the renderer from `--wgpu`/`--tiny-skia`/`LUMEN_RENDERER` (else `None`).
#[doc(inline)]
pub use lumen_widgets::renderer_override;

/// The built-in widget library (02 §10): M0 primitives plus the M1/M3/M4 and
/// remaining widget sets, the accessibility bridge, and the M5 app-building
/// modules (forms, navigation, undo, i18n, desktop system integration).
pub use lumen_widgets::{
    a11y, forms, i18n, nav, system, undo, widgets, widgets_extra, widgets_m1, widgets_m3,
    widgets_m4,
};

/// The ShaderWidget (GPU; `wgpu` feature, not available on wasm).
#[cfg(all(feature = "wgpu", not(target_arch = "wasm32")))]
pub use lumen_widgets::shader;

/// Layout styling (the typed mirror of the `.lss` layout properties, 04 §3).
pub use lumen_layout as layout;

/// The display list and CPU renderer (02 §7).
pub use lumen_render as render;

/// Text shaping and layout (ADR-005).
pub use lumen_text as text;

/// The desktop window shell. `use lumen::RunExt` to call `app.run(size)` (02 §8).
/// Desktop-only; mobile + web targets use their own shells.
#[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
pub use lumen_shell::{run, RunExt};
