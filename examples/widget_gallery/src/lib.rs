//! widget_gallery — every Lumen widget, wired to do something. A Button bumps a
//! counter, a Slider drives a ProgressBar, a CheckBox/Radio/PickList update a
//! live result, and a TextInput (Enter or the Add button) appends to a scrolling
//! to-do list whose rows each have a delete button. Built from the typed widgets
//! (`Label`/`Button`/… each builds its `Element` in `::new()`).
use lumen_widgets::element::Shadow;
use lumen_widgets::{App, BuildCx};
use lumen_widgets::{
    Button, CheckBox, Container, Element, Label, PickList, ProgressBar, Radio, Rule, Scrollable,
    Slider, Space, TextField, TextInput,
};

use lumen_layout::{Align, Dim};

/// Build the gallery app.
pub fn main_app() -> App {
    App::new(build).stylesheet(include_str!("../app.lss"))
}

const CARD_W: f32 = 560.0;
const INNER: f32 = CARD_W - 40.0;
const ITEM_H: f64 = 30.0;
const LIST_VIEW: f64 = 132.0;

/// A section: a bold heading, the control(s), and a live result line.
fn section(heading: &str, body: Vec<Element>) -> Element {
    let mut items = vec![Label::new(heading).weight(700.0).class("heading").into()];
    items.extend(body);
    Container::new(items).gap(8.0).into()
}

fn result(s: impl Into<String>) -> Element {
    Label::new(s).class("result").weight(600.0).into()
}

fn build(cx: &mut BuildCx) -> Element {
    let rt = cx.runtime();

    // --- counter (Button) ---
    let count = cx.signal("count", || 0i32);
    let counter = section(
        "Button",
        vec![{
            let mut r = Container::new(vec![
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
            .row()
            .gap(10.0)
            .align(Align::Center);
            r.element_mut().style.width = Dim::pct(1.0);
            r.into()
        }],
    );

    // --- slider → progress bar ---
    let volume = cx.signal("volume", || 35.0f64);
    let v = volume.get(rt);
    let slider = section(
        "Slider drives ProgressBar",
        vec![
            result(format!("Volume: {v:.0}%")),
            Slider::new(cx, "volume", 0.0, 100.0).id("volume").into(),
            ProgressBar::new(v / 100.0).width(INNER).id("bar").into(),
        ],
    );

    // --- checkbox ---
    let notify = cx.signal("notify", || false);
    let checkbox = section(
        "CheckBox",
        vec![{
            let mut r = Container::new(vec![
                CheckBox::new(cx, "notify", "Email me updates")
                    .id("notify")
                    .into(),
                Space::new().into(),
                result(format!(
                    "Notify: {}",
                    if notify.get(rt) { "on" } else { "off" }
                )),
            ])
            .row()
            .align(Align::Center);
            r.element_mut().style.width = Dim::pct(1.0);
            r.into()
        }],
    );

    // --- radio group ---
    let theme = cx.signal("theme", String::new);
    let cur_theme = theme.get(rt);
    let radios = section(
        "Radio (group)",
        vec![{
            let mut r = Container::new(vec![
                Radio::new(cx, "theme", "Light", "Light")
                    .id("r-light")
                    .into(),
                Radio::new(cx, "theme", "Dark", "Dark").id("r-dark").into(),
                Radio::new(cx, "theme", "Auto", "Auto").id("r-auto").into(),
                Space::new().into(),
                result(if cur_theme.is_empty() {
                    "—".to_string()
                } else {
                    cur_theme
                }),
            ])
            .row()
            .gap(16.0)
            .align(Align::Center);
            r.element_mut().style.width = Dim::pct(1.0);
            r.into()
        }],
    );

    // --- pick list ---
    let fruit = cx.signal("fruit", String::new);
    let cur_fruit = fruit.get(rt);
    let picker = section(
        "PickList",
        vec![{
            let mut r = Container::new(vec![
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
            .row()
            .align(Align::Center);
            r.element_mut().style.width = Dim::pct(1.0);
            r.into()
        }],
    );

    // --- text input → scrolling to-do list ---
    let items = cx.signal("items", Vec::<String>::new);
    let list = items.get(rt);
    let add_input = {
        let mut r = Container::new(vec![
            {
                let mut t = TextInput::new(cx, "draft", "")
                    .id("draft")
                    .on_submit(move |rt, text| items.update(rt, |v| v.push(text.to_string())));
                t.element_mut().style.flex_grow = 1.0;
                t.into()
            },
            Button::new("Add")
                .on_press(move |rt| {
                    let d = cx_signal_get(rt, "draft");
                    if !d.is_empty() {
                        items.update(rt, |v| v.push(d));
                    }
                })
                .id("add")
                .into(),
        ])
        .row()
        .gap(8.0)
        .align(Align::Center);
        r.element_mut().style.width = Dim::pct(1.0);
        r
    };
    let rows: Vec<Element> = if list.is_empty() {
        vec![Label::new("No items yet — type and press Enter")
            .class("muted")
            .into()]
    } else {
        list.iter()
            .enumerate()
            .map(|(i, item)| {
                let mut row = Container::new(vec![
                    {
                        let mut l = Label::new(format!("{}. {item}", i + 1));
                        l.element_mut().style.flex_grow = 1.0;
                        l.into()
                    },
                    Button::new("×")
                        .ghost()
                        .on_press(move |rt| {
                            items.update(rt, |v| {
                                if i < v.len() {
                                    v.remove(i);
                                }
                            })
                        })
                        .into(),
                ])
                .row()
                .align(Align::Center)
                .padding(2.0);
                row.element_mut().style.height = Dim::px(ITEM_H as f32);
                row.element_mut().style.width = Dim::pct(1.0);
                row.into()
            })
            .collect()
    };
    let content_h = (list.len() as f64 * ITEM_H).max(LIST_VIEW);
    let mut scroll = Scrollable::new(cx, "items-scroll", LIST_VIEW, content_h, rows)
        .class("list")
        .id("items");
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

    let card = Container::new(vec![
        Label::new("Widget Playground")
            .bold()
            .size(24.0)
            .class("title")
            .into(),
        Label::new("Every widget, wired up.")
            .class("subtitle")
            .into(),
        Rule::horizontal().into(),
        counter,
        slider,
        Rule::horizontal().into(),
        checkbox,
        radios,
        picker,
        Rule::horizontal().into(),
        todo,
        notes,
    ])
    .gap(14.0)
    .padding(20.0)
    .width(CARD_W)
    .corner_radius(22.0)
    .id("card")
    .background(lumen_core::Color::srgb8(0xff, 0xff, 0xff, 0xff));
    let mut card: Element = card.into();
    card.shadow = Some(Shadow::soft());

    let mut page = Container::new(vec![card])
        .fill()
        .align(Align::Center)
        .justify(Align::Center)
        .id("page");
    page.element_mut().background = Some(lumen_core::Color::srgb8(0xe9, 0xed, 0xf4, 0xff));
    page.into()
}

/// Read the `draft` text signal from within a handler (the Add button needs the
/// current value without holding the `TextInput`'s signal handle).
fn cx_signal_get(rt: &lumen_core::state::Runtime, key: &str) -> String {
    rt.signal::<String>(key, String::new).get(rt)
}
