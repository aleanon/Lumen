//! typed_form — a preferences card built with the typed widgets
//! (`Label`/`TextInput`/`CheckBox`/`Slider`/`Button`) and the `row!` macro. Each
//! builds its `Element` in `::new()` and exposes only its relevant modifiers;
//! here they're grouped under muted field labels and themed from `app.lss`.
use lumen_widgets::element::Shadow;
use lumen_widgets::{
    row, widgets, App, BuildCx, Button, CheckBox, Element, Label, Slider, TextInput,
};

use lumen_layout::{Align, Dim, Edges};

/// Build the typed-form app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

fn label(s: &str) -> Element {
    let mut e = widgets::text(s);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = 11.0;
        ts.weight = 800.0;
    }
    e.class("field-label")
}

/// A labelled field group: a small uppercase label above a typed widget.
fn field(name: &str, widget: Element) -> Element {
    let mut c = widgets::column(vec![label(name), widget]);
    c.style.row_gap = Dim::px(6.0);
    c.style.width = Dim::pct(1.0);
    c.style.align_items = Some(Align::Start);
    c
}

fn build(cx: &mut BuildCx) -> Element {
    let name: Element = TextInput::new(cx, "name", "Ada Lovelace").id("name").into();
    let notify: Element = CheckBox::new(cx, "notify", "Email me product updates")
        .id("notify")
        .into();
    let volume: Element = Slider::new(cx, "volume", 0.0, 100.0).id("volume").into();

    let mut name = name;
    name.style.width = Dim::pct(1.0);
    let mut volume = volume;
    volume.style.width = Dim::pct(1.0);

    let buttons = {
        let mut b = row![
            Button::new("Cancel").ghost().id("cancel"),
            Button::new("Save").primary().id("save").on_press(|_| {}),
        ];
        b.style.column_gap = Dim::px(12.0);
        b.style.justify_content = Some(Align::End);
        b.style.width = Dim::pct(1.0);
        b
    };

    let mut card = widgets::column(vec![
        Label::new("Preferences")
            .bold()
            .size(24.0)
            .id("title")
            .into(),
        widgets::text("Authored with the typed builder API.").class("subtitle"),
        field("DISPLAY NAME", name),
        field("NOTIFICATIONS", notify),
        field("VOLUME", volume),
        buttons,
    ])
    .id("card");
    card.style.row_gap = Dim::px(18.0);
    card.style.padding = Edges::all(Dim::px(30.0));
    card.style.width = Dim::px(380.0);
    card.style.align_items = Some(Align::Start);
    card.shadow = Some(Shadow::soft());

    let mut page = widgets::column(vec![card]).id("page");
    page.style.width = Dim::pct(1.0);
    page.style.height = Dim::pct(1.0);
    page.style.align_items = Some(Align::Center);
    page.style.justify_content = Some(Align::Center);
    page
}
