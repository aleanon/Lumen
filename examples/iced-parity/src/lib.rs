//! iced-parity example gallery (executing `08-examples-plan.md`). Each module is
//! one iced example reimplemented in Lumen; each exposes `main_app()` and is
//! covered by `tests/`. Run one with `just run iced-parity` (tests) or pick a
//! module from the gallery index.

pub mod changelog;
pub mod clock;
pub mod color_palette;
pub mod counter;
pub mod events;
pub mod gradient;
pub mod image;
pub mod loading_spinners;
pub mod markdown;
pub mod modal;
pub mod pane_grid;
pub mod progress_bar;
pub mod sierpinski;
pub mod stopwatch;
pub mod styling;
pub mod svg;
pub mod system_information;
pub mod toast;
pub mod todos;
pub mod tour;
pub mod websocket;

use lumen_widgets::App;

/// Every gallery example name, in display order (used by `show`/`win` targets).
pub const EXAMPLES: &[&str] = &[
    "counter",
    "todos",
    "events",
    "tour",
    "clock",
    "sierpinski",
    "color_palette",
    "progress_bar",
    "gradient",
    "loading_spinners",
    "modal",
    "toast",
    "markdown",
    "changelog",
    "pane_grid",
    "svg",
    "styling",
    "stopwatch",
    "image",
    "system_information",
    "websocket",
];

/// Build the [`App`] for a gallery example by `name`, or `None` if unknown.
///
/// Shared by the headless `show` target and the windowed `win` target so the two
/// stay in lock-step with the module list above.
pub fn app_for(name: &str) -> Option<App> {
    Some(match name {
        "counter" => counter::main_app(),
        "todos" => todos::main_app(),
        "events" => events::main_app(),
        "tour" => tour::main_app(),
        "clock" => clock::main_app(),
        "sierpinski" => sierpinski::main_app(),
        "color_palette" => color_palette::main_app(),
        "progress_bar" => progress_bar::main_app(),
        "gradient" => gradient::main_app(),
        "loading_spinners" => loading_spinners::main_app(),
        "modal" => modal::main_app(),
        "toast" => toast::main_app(),
        "markdown" => markdown::main_app(),
        "changelog" => changelog::main_app(),
        "pane_grid" => pane_grid::main_app(),
        "svg" => svg::main_app(),
        "styling" => styling::main_app(),
        "stopwatch" => stopwatch::main_app(),
        "image" => image::main_app(),
        "system_information" => system_information::main_app(),
        "websocket" => websocket::main_app(),
        _ => return None,
    })
}
