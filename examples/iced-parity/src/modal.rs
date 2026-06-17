//! modal — open a centered dialog over the page with a dimmed backdrop (E8.2),
//! both the page and the dialog presented as soft-shadowed panels.
use lumen_widgets::widgets_extra::modal;
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

/// Build the modal app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let open = cx.signal("open", || false);
    let is_open = open.get(cx.runtime());

    let base = theme::center_screen(theme::panel_centered(widgets::column(vec![
        theme::caption("MODAL DEMO"),
        theme::heading("Main content").id("content"),
        theme::accent_button("Open dialog", move |rt| open.set(rt, true)).id("open"),
    ])));
    let dialog = theme::panel_centered(widgets::column(vec![
        theme::heading("Are you sure?").id("dialog-text"),
        theme::caption("This action can't be undone."),
        theme::button_row(vec![
            theme::ghost_button("Cancel", move |rt| open.set(rt, false)).id("close"),
            theme::accent_button("Confirm", move |rt| open.set(rt, false)).id("confirm"),
        ]),
    ]));
    modal(base, dialog, is_open).id("root")
}
