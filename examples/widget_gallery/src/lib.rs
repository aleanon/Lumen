//! widget_gallery — every Lumen widget, wired to do something. A Button bumps a
//! counter, a Slider drives a ProgressBar, a CheckBox/Radio/PickList update a
//! live result (the Theme radio actually re-themes the gallery light/dark), and a
//! TextInput (Enter or the Add button) appends to a scrolling to-do list whose
//! rows each have a delete button. Built from the typed widgets.
use lumen_core::Color;
use lumen_widgets::element::Shadow;
use lumen_widgets::{App, BuildCx};
use lumen_widgets::{
    Button, CheckBox, Container, Element, Label, PickList, ProgressBar, Radio, Rule, Scrollable,
    Slider, Space, TextField, TextInput,
};

use lumen_layout::{Align, Dim, Edges};

/// Build the gallery app.
pub fn main_app() -> App {
    App::new(build)
}

const CARD_W: f32 = 560.0;
const INNER: f32 = CARD_W - 40.0;
const ITEM_H: f64 = 30.0;
const LIST_VIEW: f64 = 132.0;

/// The gallery's colour palette, switched live by the Theme radio.
#[derive(Clone, Copy)]
struct Pal {
    page: Color,
    surface: Color,
    ink: Color,
    muted: Color,
    accent: Color,
    field: Color,
    divider: Color,
}

fn light() -> Pal {
    Pal {
        page: Color::srgb8(0xe9, 0xed, 0xf4, 0xff),
        surface: Color::srgb8(0xff, 0xff, 0xff, 0xff),
        ink: Color::srgb8(0x1b, 0x22, 0x30, 0xff),
        muted: Color::srgb8(0x7a, 0x84, 0x99, 0xff),
        accent: Color::srgb8(0x2f, 0x6b, 0xff, 0xff),
        field: Color::srgb8(0xf3, 0xf5, 0xf9, 0xff),
        divider: Color::srgb8(0xe2, 0xe6, 0xee, 0xff),
    }
}

fn dark() -> Pal {
    Pal {
        page: Color::srgb8(0x0b, 0x0f, 0x1a, 0xff),
        surface: Color::srgb8(0x16, 0x1c, 0x2a, 0xff),
        ink: Color::srgb8(0xee, 0xf2, 0xfb, 0xff),
        muted: Color::srgb8(0x8a, 0x93, 0xac, 0xff),
        accent: Color::srgb8(0x6f, 0x9c, 0xff, 0xff),
        field: Color::srgb8(0x0e, 0x14, 0x22, 0xff),
        divider: Color::srgb8(0x25, 0x2d, 0x3e, 0xff),
    }
}

fn build(cx: &mut BuildCx) -> Element {
    let rt = cx.runtime();
    // Seed the held value so the matching radio is selected on first paint (an
    // empty default matches neither "Light" nor "Dark").
    let theme = cx.signal("theme", || "Light".to_string());
    let pal = if theme.get(rt) == "Dark" {
        dark()
    } else {
        light()
    };

    // palette-aware label helpers
    let heading = move |s: &str| Label::new(s).weight(700.0).color(pal.ink);
    let result =
        move |s: String| -> Element { Label::new(s).weight(600.0).color(pal.accent).into() };
    let section = move |head: &str, body: Vec<Element>| -> Element {
        let mut kids = vec![heading(head).into()];
        kids.extend(body);
        Container::new(kids).gap(8.0).into()
    };
    let rule = move || -> Element { Rule::horizontal().background(pal.divider).into() };
    let row_full = |kids: Vec<Element>| -> Container {
        let mut r = Container::new(kids).row().align(Align::Center);
        r.element_mut().style.width = Dim::pct(1.0);
        r
    };

    // --- counter (Button) ---
    let count = cx.signal("count", || 0i32);
    let counter = section(
        "Button",
        vec![row_full(vec![
            Button::new("Add one")
                .on_press(move |rt| count.update(rt, |c| *c += 1))
                .id("add-one")
                .into(),
            Button::new("Reset")
                .ghost()
                .on_press(move |rt| count.set(rt, 0))
                .id("reset")
                .into(),
            Space::new().into(),
            result(format!("Count: {}", count.get(rt))),
        ])
        .gap(10.0)
        .into()],
    );

    // --- slider → progress bar ---
    let volume = cx.signal("volume", || 35.0f64);
    let v = volume.get(rt);
    let slider = section(
        "Slider drives ProgressBar",
        vec![
            result(format!("Volume: {v:.0}%")),
            Slider::new(cx, "volume", 0.0, 100.0).id("volume").into(),
            ProgressBar::new(v / 100.0)
                .width(INNER)
                .fill_color(pal.accent)
                .id("bar")
                .into(),
        ],
    );

    // --- checkbox ---
    let notify = cx.signal("notify", || false);
    let checkbox = section(
        "CheckBox",
        vec![row_full(vec![
            CheckBox::new(cx, "notify", "Email me updates")
                .color(pal.ink)
                .id("notify")
                .into(),
            Space::new().into(),
            result(format!(
                "Notify: {}",
                if notify.get(rt) { "on" } else { "off" }
            )),
        ])
        .into()],
    );

    // --- radio group (re-themes the gallery) ---
    let cur_theme = theme.get(rt);
    let radios = section(
        "Radio (group) — switches theme",
        vec![row_full(vec![
            Radio::new(cx, "theme", "Light", "Light")
                .color(pal.ink)
                .id("r-light")
                .into(),
            Radio::new(cx, "theme", "Dark", "Dark")
                .color(pal.ink)
                .id("r-dark")
                .into(),
            Space::new().into(),
            result(if cur_theme.is_empty() {
                "Light".to_string()
            } else {
                cur_theme
            }),
        ])
        .gap(16.0)
        .into()],
    );

    // --- pick list ---
    let fruit = cx.signal("fruit", String::new);
    let cur_fruit = fruit.get(rt);
    let picker = section(
        "PickList",
        vec![row_full(vec![
            PickList::new(
                cx,
                "fruit",
                "Pick a fruit",
                ["Apple", "Banana", "Cherry", "Mango"],
            )
            .id("fruit")
            .into(),
            Space::new().into(),
            result(if cur_fruit.is_empty() {
                "—".to_string()
            } else {
                cur_fruit
            }),
        ])
        .into()],
    );

    // --- text input → scrolling to-do list ---
    let items = cx.signal("items", Vec::<String>::new);
    let list = items.get(rt);
    let add_input = row_full(vec![
        {
            let mut t = TextInput::new(cx, "draft", "")
                .id("draft")
                .on_submit(move |rt, text| items.update(rt, |v| v.push(text.to_string())));
            t.element_mut().style.flex_grow = 1.0;
            t.into()
        },
        Button::new("Add")
            .on_press(move |rt| {
                let d = draft_text(rt);
                if !d.is_empty() {
                    items.update(rt, |v| v.push(d));
                }
            })
            .id("add")
            .into(),
    ])
    .gap(8.0);
    let rows: Vec<Element> = if list.is_empty() {
        vec![Label::new("No items yet — type and press Enter")
            .color(pal.muted)
            .into()]
    } else {
        list.iter()
            .enumerate()
            .map(|(i, item)| {
                // A compact, theme-aware delete button (no big white square).
                let mut del = Button::new("×")
                    .background(pal.divider)
                    .text_color(pal.muted)
                    .on_press(move |rt| {
                        items.update(rt, |v| {
                            if i < v.len() {
                                v.remove(i);
                            }
                        })
                    });
                {
                    let e = del.element_mut();
                    e.corner_radius = 7.0;
                    e.style.padding = Edges {
                        left: Dim::px(9.0),
                        right: Dim::px(9.0),
                        top: Dim::px(2.0),
                        bottom: Dim::px(2.0),
                    };
                }
                let mut row = row_full(vec![
                    {
                        let mut l = Label::new(format!("{}. {item}", i + 1)).color(pal.ink);
                        l.element_mut().style.flex_grow = 1.0;
                        l.into()
                    },
                    del.into(),
                ])
                .padding(2.0);
                row.element_mut().style.height = Dim::px(ITEM_H as f32);
                row.into()
            })
            .collect()
    };
    let content_h = (list.len() as f64 * ITEM_H).max(LIST_VIEW);
    let mut scroll = Scrollable::new(cx, "items-scroll", LIST_VIEW, content_h, rows)
        .background(pal.field)
        .id("items");
    scroll.element_mut().corner_radius = 10.0;
    scroll.element_mut().style.width = Dim::pct(1.0);
    let todo = section(
        "TextInput adds to a list",
        vec![
            add_input.into(),
            result(format!("{} item(s)", list.len())),
            scroll.into(),
        ],
    );

    // --- multi-line text field ---
    let notes = section(
        "TextField (multi-line)",
        vec![{
            let mut t = TextField::new(cx, "notes", "Type notes…\nEnter adds a line").lines(3);
            t.element_mut().style.width = Dim::px(INNER);
            t.id("notes").into()
        }],
    );

    let mut card: Element = Container::new(vec![
        Label::new("Widget Playground")
            .bold()
            .size(24.0)
            .color(pal.ink)
            .into(),
        Label::new("Every widget, wired up.")
            .color(pal.muted)
            .into(),
        rule(),
        counter,
        slider,
        rule(),
        checkbox,
        radios,
        picker,
        rule(),
        todo,
        notes,
    ])
    .gap(14.0)
    .padding(20.0)
    .width(CARD_W)
    .corner_radius(22.0)
    .background(pal.surface)
    .id("card")
    .into();
    card.shadow = Some(Shadow::soft());

    Container::new(vec![card])
        .fill()
        .align(Align::Center)
        .justify(Align::Center)
        .background(pal.page)
        .id("page")
        .into()
}

/// Read the `draft` text signal from within a handler (the Add button needs the
/// current value without holding the `TextInput`'s signal handle).
fn draft_text(rt: &lumen_core::state::Runtime) -> String {
    rt.signal::<String>("draft", String::new).get(rt)
}
