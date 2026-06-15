//! The Lumen "hello" example: a styled counter app.
//!
//! Exposes `main_app()` by the convention `lumen new` scaffolds and
//! `lumen-test`/`lumen test` build from.

use lumen::widgets::{button, column, text};
use lumen::{App, Color};

/// Build the counter application.
pub fn main_app() -> App {
    App::new(|cx| {
        let count = cx.signal("count", || 0i32);
        let value = count.get(cx.runtime());
        column(vec![
            text(format!("Count: {value}")).id("count"),
            button("+1", move |rt| count.update(rt, |c| *c += 1)).id("increment"),
        ])
        .background(Color::srgb8(0xff, 0xff, 0xff, 0xff))
    })
}
