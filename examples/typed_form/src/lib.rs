//! typed_form — a preferences card built with the typed widgets
//! (`Label`/`TextInput`/`CheckBox`/`Slider`/`Button`) and the `row!` macro. Each
//! builds its `Element` in `::new()` and exposes only its relevant modifiers;
//! here they're grouped under muted field labels and themed from `app.lss`.
use lumen_widgets::element::Shadow;
use lumen_widgets::{
    row, widgets, App, BuildCx, Button, CheckBox, Element, Label, Slider, Stack, TextInput,
};

use lumen_layout::{Align, Dim};

/// Build the typed-form app.
pub fn main_app() -> App {
    App::view(build).stylesheet(include_str!("../app.lss"))
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

fn build(cx: &mut BuildCx) -> impl lumen_widgets::Direct {
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

    // E2b: statement form throughout — the card's gap/padding/width/
    // alignment/shadow and the page's centring are all Stack modifiers now.
    // The typed leaves above need `cx`, so they are built eagerly and moved
    // into the body.
    let card = Stack::column(move |c| {
        c.child(Label::new("Preferences").bold().size(24.0).id("title"));
        c.child(widgets::text("Authored with the typed builder API.").class("subtitle"));
        c.child(field("DISPLAY NAME", name));
        c.child(field("NOTIFICATIONS", notify));
        c.child(field("VOLUME", volume));
        c.child(buttons);
    })
    .gap(18.0)
    .padding(30.0)
    .width(Dim::px(380.0))
    .align_items(Align::Start)
    .shadow(Shadow::soft())
    .id("card");

    Stack::column(move |c| {
        c.child(card);
    })
    .width(Dim::pct(1.0))
    .height(Dim::pct(1.0))
    .centered()
    .id("page")
}
