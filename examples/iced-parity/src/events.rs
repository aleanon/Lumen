//! events — log the last pointer/keyboard interaction (observable via the
//! semantic tree, like iced's event inspector), as a centred hero readout.
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

/// Build the events app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let log = cx.signal("log", || "no events yet".to_string());
    let last = log.get(cx.runtime());
    theme::center_screen(theme::panel_centered(widgets::column(vec![
        theme::caption("LAST EVENT"),
        theme::heading(last).id("log"),
        theme::accent_button("Click me", move |rt| {
            log.set(rt, "clicked Click me".to_string())
        })
        .id("target"),
    ])))
}
