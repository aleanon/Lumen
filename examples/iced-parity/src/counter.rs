//! counter — increment/decrement a value, shown as a big bold figure with the
//! controls on a row beneath, centred in a soft-shadowed panel.
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

/// Build the counter app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i32);
    let v = count.get(cx.runtime());
    theme::center_screen(theme::panel_centered(widgets::column(vec![
        theme::caption("COUNT"),
        theme::display(format!("{v}")).id("value"),
        theme::button_row(vec![
            theme::ghost_button("–", move |rt| count.update(rt, |c| *c -= 1)).id("dec"),
            theme::accent_button("+", move |rt| count.update(rt, |c| *c += 1)).id("inc"),
        ]),
    ])))
}
