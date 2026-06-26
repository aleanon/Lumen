//! `lumen-widgets` — the element model, the headless app runtime, and (from
//! T0.10) the built-in widget library.
//!
//! The headless runtime ([`App`]/[`Headless`]) is the integration point that
//! ties lumen-core (tree/state/events/semantics), lumen-layout, lumen-render,
//! and lumen-text together (02 §8). It lives here, not in lumen-core, because it
//! depends on those higher crates; the `lumen` facade re-exports it as
//! `lumen::App`.
#![warn(missing_docs)]

pub mod a11y;
pub mod app;
pub mod asset;
pub mod audit;
pub mod boundary;
pub mod button;
pub mod check_box;
pub mod container;
pub mod design;
pub mod element;
pub mod forms;
pub mod i18n;
pub mod label;
mod macros;
pub mod markdown;
pub mod motion;
pub mod nav;
pub mod pick_list;
pub mod progress_bar;
pub mod radio;
pub mod rule;
pub mod scrollable;
pub mod slider;
pub mod space;
pub mod system;
pub mod text_field;
pub mod text_input;
pub mod theme;
pub mod undo;
pub mod wcag;
mod widget;
// ShaderWidget needs the wgpu GPU backend (CPU fallback included), which is not
// built on wasm; on the web, shaders are a WebGPU presenter concern.
#[cfg(not(target_arch = "wasm32"))]
pub mod shader;
pub mod tasks;
pub mod widgets;
pub mod widgets_extra;
pub mod widgets_m1;
pub mod widgets_m3;
pub mod widgets_m4;

pub use app::{center, App, AppSnapshot, FrameStats, Headless, ReloadResult};
pub use element::{BuildCx, Element, Handler, LeafWidget, NodeContent};
/// The data layer: executors + the `Sink` background work pushes results through.
pub use lumen_core::tasks::{InlineSpawner, ManualSpawner, Sink, Spawner};
/// Re-exported so downstream crates can bound on the renderer backend (e.g.
/// `Headless<R>` consumers like `lumen-agent`) without depending on `lumen-render`.
pub use lumen_render::{DefaultRenderer, Renderer, TinySkia};
pub use tasks::{Resource, TaskError};
// The widget library — each builds its `Element` inside `::new()`, in its own
// file. Lower to `Element` via `From`; compose with `col!`/`row!` or `Container`.
pub use button::Button;
pub use check_box::CheckBox;
pub use container::Container;
pub use label::Label;
pub use pick_list::PickList;
pub use progress_bar::ProgressBar;
pub use radio::Radio;
pub use rule::Rule;
pub use scrollable::Scrollable;
pub use slider::Slider;
pub use space::Space;
pub use text_field::TextField;
pub use text_input::TextInput;
