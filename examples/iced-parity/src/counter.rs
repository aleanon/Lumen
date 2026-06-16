//! counter — increment/decrement a value.
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

/// Build the counter app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    theme::screen("Counter", body(cx))
}

fn body(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i32);
    let v = count.get(cx.runtime());
    widgets::column(vec![
        widgets::button("Increment", move |rt| count.update(rt, |c| *c += 1)).id("inc"),
        widgets::text(format!("{v}")).id("value"),
        widgets::button("Decrement", move |rt| count.update(rt, |c| *c -= 1)).id("dec"),
    ])
}
