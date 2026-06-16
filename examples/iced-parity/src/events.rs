//! events — log the last pointer/keyboard interaction (observable via the
//! semantic tree, like iced's event inspector).
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

/// Build the events app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    theme::screen("Events", body(cx))
}

fn body(cx: &mut BuildCx) -> Element {
    let log = cx.signal("log", || "no events yet".to_string());
    let last = log.get(cx.runtime());
    widgets::column(vec![
        widgets::button("Click me", move |rt| {
            log.set(rt, "clicked Click me".to_string())
        })
        .id("target"),
        widgets::text(format!("last: {last}")).id("log"),
    ])
}
