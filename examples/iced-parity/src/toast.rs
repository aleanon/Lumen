//! toast — show a transient notification; it auto-dismisses after a few ticks.
use lumen_widgets::{widgets, App, BuildCx, Element};

/// Build the toast app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    // `ttl` > 0 means a toast is showing; ticking decrements it to 0.
    let ttl = cx.signal("ttl", || 0i64);
    let remaining = ttl.get(cx.runtime());

    let mut col = vec![
        widgets::button("Notify", move |rt| ttl.set(rt, 3)).id("notify"),
        widgets::button("tick", move |rt| ttl.update(rt, |t| *t = (*t - 1).max(0))).id("tick"),
    ];
    if remaining > 0 {
        col.push(widgets::text("✓ Saved").id("toast"));
    }
    widgets::column(col).id("root")
}
