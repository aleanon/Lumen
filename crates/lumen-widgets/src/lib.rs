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
pub mod accordion;
pub mod app;
pub mod asset;
pub mod audit;
pub mod boundary;
pub mod button;
pub mod charts;
pub mod check_box;
pub mod color_picker;
pub mod combobox;
pub mod container;
/// Design-spec (JSON) → `.lss` import — an agent/tooling surface, so it lives
/// behind `snapshot` (drops `serde_json` in a lean build).
#[cfg(feature = "snapshot")]
pub mod design;
pub mod element;
/// W.1 promotions: Toast, Spinner, Chip.
pub mod feedback;
pub mod file_picker;
pub mod forms;
pub mod grid;
pub mod i18n;
pub mod label;
mod macros;
pub mod markdown;
/// W.2 small widgets: Skeleton, Avatar, Pagination, AlignBox.
pub mod misc_w2;
pub mod motion;
pub mod nav;
pub mod pick_list;
pub mod popover;
pub mod progress_bar;
pub mod radio;
pub mod range_slider;
pub mod rule;
pub mod scrollable;
pub mod search_field;
pub mod sheet;
pub mod slider;
pub mod space;
pub mod system;
pub mod text_field;
pub mod text_input;
pub mod theme;
pub mod undo;
pub mod wcag;
mod widget;
// ShaderWidget needs the wgpu GPU backend (`wgpu` feature), which is not built on
// wasm; on the web, shaders are a WebGPU presenter concern.
#[cfg(all(feature = "wgpu", not(target_arch = "wasm32")))]
pub mod shader;
pub mod tasks;
pub mod widgets;
pub mod widgets_extra;
pub mod widgets_m1;
pub mod widgets_m3;
pub mod widgets_m4;

pub use app::{center, App, FrameStats, Headless, ReloadResult};
#[cfg(feature = "snapshot")]
pub use app::{AppSnapshot, Checkpoint};
pub use element::{BuildCx, Element, Handler, LeafWidget, NodeContent};
/// The data layer: executors + the `Sink` background work pushes results through.
pub use lumen_core::tasks::{InlineSpawner, ManualSpawner, Sink, Spawner};
/// Compile-time handler-currency check (F2): a handler may only capture stable
/// `Copy` state (signal/memo handles, scalars), never owned snapshots that go
/// stale when the handler is retained. See [`lumen_macros::stable_handler`].
///
/// A handler capturing only a `Signal` handle (which is `Copy`) passes:
/// ```
/// use lumen_core::state::{Runtime, Signal};
/// let rt = Runtime::new();
/// let count: Signal<i64> = rt.signal("c", || 0);
/// let handler = lumen_widgets::stable_handler!(move |rt: &Runtime| count.update(rt, |c| *c += 1));
/// handler(&rt);
/// assert_eq!(count.get(&rt), 1);
/// ```
///
/// Capturing an owned `String` snapshot is rejected at compile time:
/// ```compile_fail
/// use lumen_core::state::{Runtime, Signal};
/// let rt = Runtime::new();
/// let items: Signal<Vec<String>> = rt.signal("v", Vec::new);
/// let draft = String::from("stale snapshot");
/// // `draft` is a non-`Copy` owned value → the handler isn't `Copy` → rejected.
/// let handler = lumen_widgets::stable_handler!(move |rt: &Runtime| {
///     items.update(rt, |v| v.push(draft.clone()));
/// });
/// ```
pub use lumen_macros::stable_handler;
/// F3 binding sugar: `text!(cx, "Count: {count}")` → a reactive text element
/// whose string tracks the interpolated signals. See [`lumen_macros::text`].
pub use lumen_macros::text;
/// Re-exported so downstream crates can bound on the renderer backend (e.g.
/// `Headless<R>` consumers like `lumen-agent`) without depending on `lumen-render`.
pub use lumen_render::{DefaultRenderer, Renderer, RgbaImage, TinySkia};
pub use tasks::{Resource, TaskError};

/// An explicit renderer choice from the command line (`--wgpu` / `--tiny-skia`)
/// or the `LUMEN_RENDERER=wgpu|tiny-skia` environment variable, ready to install
/// with [`App::with_renderer`]. Returns `None` when nothing is specified, so the
/// caller keeps its own default (the shell defaults to GPU-with-fallback,
/// headless previews to the deterministic CPU).
///
/// `wgpu` yields a [`WgpuFallbackTinySkia`](lumen_render::WgpuFallbackTinySkia) —
/// the GPU when an adapter exists, else the CPU fallback. Built without the
/// `wgpu` feature, a `wgpu` request logs a notice and falls back to `TinySkia`.
/// CLI flags take precedence over the env var.
pub fn renderer_override() -> Option<Box<dyn Renderer>> {
    enum Choice {
        Wgpu,
        TinySkia,
    }
    let from_args = std::env::args().skip(1).find_map(|a| match a.as_str() {
        "--wgpu" => Some(Choice::Wgpu),
        "--tiny-skia" => Some(Choice::TinySkia),
        _ => None,
    });
    let choice = from_args.or_else(|| match std::env::var("LUMEN_RENDERER").ok().as_deref() {
        Some("wgpu") => Some(Choice::Wgpu),
        Some("tiny-skia") | Some("cpu") => Some(Choice::TinySkia),
        _ => None,
    })?;
    Some(match choice {
        Choice::TinySkia => Box::new(TinySkia),
        Choice::Wgpu => {
            #[cfg(all(feature = "wgpu", not(target_arch = "wasm32")))]
            {
                Box::new(lumen_render::WgpuFallbackTinySkia::new())
            }
            #[cfg(not(all(feature = "wgpu", not(target_arch = "wasm32"))))]
            {
                eprintln!(
                    "lumen: renderer `wgpu` requested but this build has no wgpu \
                     backend; using tiny-skia"
                );
                Box::new(TinySkia)
            }
        }
    })
}
// The widget library — each builds its `Element` inside `::new()`, in its own
// file. Lower to `Element` via `From`; compose with `col!`/`row!` or `Container`.
pub use accordion::Accordion;
pub use button::Button;
pub use charts::{LineChart, PieChart, PieSlice};
pub use check_box::CheckBox;
pub use color_picker::ColorPicker;
pub use combobox::Combobox;
pub use container::Container;
pub use feedback::{Chip, Spinner, Toast, ToastKind};
pub use file_picker::FilePicker;
pub use grid::{CellRef, Grid, GridStyle};
pub use label::Label;
pub use misc_w2::{AlignBox, Avatar, Pagination, Skeleton};
pub use pick_list::PickList;
pub use popover::{Popover, PopoverSide};
pub use progress_bar::ProgressBar;
pub use radio::Radio;
pub use range_slider::RangeSlider;
pub use rule::Rule;
pub use scrollable::Scrollable;
pub use search_field::SearchField;
pub use sheet::{Drawer, DrawerSide, Sheet};
pub use slider::Slider;
pub use space::Space;
pub use text_field::TextField;
pub use text_input::TextInput;
