//! modal — open a centered dialog over the page with a dimmed backdrop (E8.2).
use lumen_widgets::widgets_extra::modal;
use lumen_widgets::{widgets, App, BuildCx, Element};

/// Build the modal app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let open = cx.signal("open", || false);
    let is_open = open.get(cx.runtime());

    let base = widgets::column(vec![
        widgets::text("Main content").id("content"),
        widgets::button("Open dialog", move |rt| open.set(rt, true)).id("open"),
    ]);
    let dialog = widgets::column(vec![
        widgets::text("Are you sure?").id("dialog-text"),
        widgets::button("Close", move |rt| open.set(rt, false)).id("close"),
    ]);
    modal(base, dialog, is_open).id("root")
}
