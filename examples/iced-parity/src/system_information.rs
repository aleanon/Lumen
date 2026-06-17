//! system_information — basic OS/arch/CPU info as a tidy key/value card.
use lumen_widgets::system::system_info;
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

/// Build the system-information app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;
    let info = system_info();
    let row = |key: &str, val: String, id: &str| {
        theme::button_row(vec![
            theme::fixed_width(theme::caption(key), 70.0),
            theme::heading(val),
        ])
        .id(id)
    };
    theme::center_screen(theme::panel_centered(widgets::column(vec![
        theme::heading("System information").id("title"),
        row("OS:", info.os, "os"),
        row("Arch:", info.arch, "arch"),
        row("CPUs:", info.cpus.to_string(), "cpus"),
    ])))
}
