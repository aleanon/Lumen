//! The Lumen "hello" example: a styled counter app.
//!
//! Exposes `main_app()` by the convention `lumen new` scaffolds and
//! `lumen-test`/`lumen test` build from.
//!
//! E1 (Element-deletion migration): authored in the statement form with one
//! `#[derive(Reactive)]` state struct — no `Element` in this file. The old
//! form this replaced read a keyed signal and built `column(vec![...])`.

use lumen::{App, Button, Color, Label, Reactive, Stack};

/// App state: one struct, one field per piece of state (MUT8).
#[derive(Default, Reactive, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct HelloState {
    count: i32,
}

/// Build the counter application.
pub fn main_app() -> App {
    App::with_state(HelloState::default(), |cx, s: &HelloState| {
        let value = *s.count(cx);
        Stack::column(move |c| {
            c.child(Label::new(format!("Count: {value}")).id("count"));
            c.child(
                Button::new("+1")
                    .on_press(|rt| HelloState::update_count(rt, |c| *c += 1))
                    .id("increment"),
            );
        })
        .background(Color::srgb8(0xff, 0xff, 0xff, 0xff))
    })
}
