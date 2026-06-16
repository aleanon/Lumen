//! `cargo run -p iced-parity --example show -- <name>` renders one iced-parity
//! example one frame headless and writes a PNG you can open. `show list` prints
//! the names. Wired to `just run <name>`.
use iced_parity::*;
use lumen_core::geometry::Size;
use lumen_widgets::App;

const NAMES: &[&str] = &[
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

fn app_for(name: &str) -> Option<App> {
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

fn main() {
    let name = std::env::args().nth(1).unwrap_or_default();
    if name.is_empty() || name == "list" {
        println!("iced-parity examples:");
        for n in NAMES {
            println!("  {n}");
        }
        return;
    }
    match app_for(&name) {
        Some(app) => {
            let mut h = app.run_headless(Size::new(480.0, 360.0));
            let path = format!("/tmp/lumen-{name}.png");
            std::fs::write(&path, h.screenshot().to_png()).unwrap();
            println!(
                "rendered '{name}' ({} nodes) -> {path}",
                h.semantics_doc().root.elided().children.len()
            );
        }
        None => {
            eprintln!(
                "unknown example '{name}'. try: cargo run -p iced-parity --example show -- list"
            );
            std::process::exit(1);
        }
    }
}
