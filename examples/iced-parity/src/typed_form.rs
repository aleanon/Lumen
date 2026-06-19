//! typed_form — showcases the typed widget builders + `col!`/`row!` macros (the
//! ElementBuilder API, 02 §3): each widget exposes only its relevant modifiers,
//! and `col!`/`row!` mix typed widgets freely. Compare to the field-set style of
//! the other examples — same result, more type-safe authoring.
use lumen_widgets::{
    col, row, theme, App, BuildCx, Button, Checkbox, Element, Slider, Text, TextField,
};

/// Build the typed-form app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    theme::center_screen(theme::panel_centered(col![
        Text::new("Preferences").bold().size(24.0).id("title"),
        TextField::new(cx, "name", "Ada Lovelace").id("name"),
        Checkbox::new(cx, "notify", "Email me updates").id("notify"),
        Slider::new(cx, "volume", 0.0, 100.0).id("volume"),
        {
            let mut buttons = row![
                Button::new("Cancel").ghost().id("cancel"),
                Button::new("Save").primary().id("save").on_press(|_| {}),
            ];
            buttons.style.column_gap = lumen_layout::Dim::px(12.0);
            buttons
        },
    ]))
}
