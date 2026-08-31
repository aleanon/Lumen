//! A Storybook-class **component gallery** (T7.2): built-in widgets alongside a
//! third-party one (`widget_rating::rating`), all self-tested through the agent.
//!
//! E1 (Element-deletion migration): the MIXED form. The root is a
//! statement-form `Stack`; typed widgets (`Label`, `Button`) lower directly,
//! while the cx-coupled helpers (`switch`, `select`, the third-party
//! `rating`) keep their keyed view-local state — the D1 boundary — and are
//! built eagerly, then MOVED into the `FnOnce` body as `Element` children.

use lumen_widgets::{widgets, App, BuildCx, Button, Label, Stack};

/// Build the gallery.
pub fn main_app() -> App {
    App::view(|cx: &mut BuildCx| {
        let switch = widgets::switch(cx, "wifi", "Wi-Fi").id("switch");
        let select = widgets::select(cx, "pick", &["A", "B", "C"]).id("select");
        // A third-party widget, driven exactly like the built-ins.
        let stars = widget_rating::rating(cx, "stars", 5);
        Stack::column(move |c| {
            c.child(Label::new("Component gallery").id("title"));
            c.child(Button::new("Button").on_press(|_| {}).id("button"));
            c.child(switch);
            c.child(select);
            c.child(stars);
        })
    })
}
