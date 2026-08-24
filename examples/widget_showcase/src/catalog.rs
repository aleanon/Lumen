//! The catalog: one [`Entry`] per built-in widget, each seeded with data that
//! shows what the widget *does* rather than merely that it exists.
//!
//! Entries are plain `fn` pointers in a `static` table, so the whole catalog is
//! a read-only slice — the picker iterates it to build the dropdown, and the
//! stage calls exactly one `build` per frame. Adding a widget is one entry.
use lumen_core::semantics::Role;
use lumen_core::state::Runtime;
use lumen_core::Color;
use lumen_layout::{Align, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use lumen_text::TextEditor;
use lumen_widgets::widgets::{Run, TreeRow};
use lumen_widgets::{markdown, BuildCx, Element};
use lumen_widgets::{
    Accordion, AlignBox, AppBar, Avatar, Badge, BarChart, BottomNav, Button, Canvas, Card,
    CheckBox, Chip, ColorPicker, Combobox, Container, DataGrid, DatePicker, Drawer, FilePicker,
    FindReplaceBar, Grid, Icon, Image, Label, LineChart, Menu, Modal, NavigationRail, Pagination,
    PaneGrid, PickList, PieChart, PieSlice, Popover, ProgressBar, PullToRefresh, Radio,
    RangeSlider, RgbaImage, RichText, RichTextEditor, Rule, Scrollable, SearchField, Select, Sheet,
    Skeleton, Slider, Space, Spinner, SplitPane, Stepper, Switch, Tabs, TextField, TextInput,
    TimePicker, Toast, ToastKind, Tooltip, Tree, VirtualList, Wrap,
};

/// Where the stage parks the demo inside the area below the header.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Natural width, centred on both axes — the default.
    Center,
    /// Stretched across the stage width, centred vertically: bars, sliders,
    /// tab strips, anything that reads as a full-width control.
    Wide,
    /// A full-window layer (`Sheet`, `Drawer`, `Modal`): the stage drops its
    /// padding and the demo positions itself against the window origin, so the
    /// scrim covers what it would cover in a real app.
    Overlay,
    /// Centred horizontally but anchored near the top — for widgets whose open
    /// state hangs an absolute panel *downwards* (dropdowns, combo boxes,
    /// pickers). Centring those would push the panel off the bottom edge.
    Top,
}

/// One widget in the gallery.
pub struct Entry {
    /// Display name, as it appears in the dropdown.
    pub name: &'static str,
    /// Stable slug — the dropdown row's id (`#opt-<slug>`) and the test handle.
    pub slug: &'static str,
    /// One line on what the seeded demo is showing.
    pub blurb: &'static str,
    /// Stage placement.
    pub place: Place,
    /// Builds the seeded demo.
    pub build: fn(&mut BuildCx) -> Element,
    /// Optional live state readout, rendered in the header through a binding so
    /// it patches instead of rebuilding the subtree.
    pub status: Option<fn(&Runtime) -> String>,
}

/// A titled run of entries in the dropdown.
pub struct Group {
    /// Section heading.
    pub title: &'static str,
    /// Its widgets.
    pub entries: &'static [Entry],
}

// ---------------------------------------------------------------- palette ---
// `Color::srgb8` converts gamma-encoded bytes to linear floats, so it is not a
// `const fn` — the palette is a set of tiny functions rather than constants.

fn ink() -> Color {
    Color::srgb8(0x1b, 0x22, 0x30, 0xff)
}
fn muted() -> Color {
    Color::srgb8(0x6b, 0x74, 0x88, 0xff)
}
fn accent() -> Color {
    Color::srgb8(0x2f, 0x6b, 0xff, 0xff)
}
fn green() -> Color {
    Color::srgb8(0x18, 0xa0, 0x5c, 0xff)
}
fn amber() -> Color {
    Color::srgb8(0xe8, 0x9c, 0x1a, 0xff)
}
fn red() -> Color {
    Color::srgb8(0xe0, 0x40, 0x4b, 0xff)
}
fn tint() -> Color {
    Color::srgb8(0xe8, 0xed, 0xf7, 0xff)
}

/// A small muted caption — the note used across the demos.
fn note(s: impl Into<String>) -> Element {
    Label::new(s.into()).size(12.0).color(muted()).into()
}

/// A tinted, padded box, so a fill-the-parent widget gets visible bounds.
fn framed(child: Element, w: f32, h: f32) -> Element {
    let mut e: Element = Container::new(vec![child]).padding(10.0).into();
    e.background = Some(tint());
    e.corner_radius = 10.0;
    e.style.width = Dim::px(w);
    e.style.height = Dim::px(h);
    e
}

/// [`framed`], but only the width is fixed — the box hugs its content height.
fn framed_auto(child: Element, w: f32) -> Element {
    let mut e: Element = Container::new(vec![child]).padding(14.0).into();
    e.background = Some(tint());
    e.corner_radius = 10.0;
    e.style.width = Dim::px(w);
    e
}

/// A vertical stack of demo pieces, centred, with breathing room.
fn stack(kids: Vec<Element>) -> Element {
    Container::new(kids)
        .column()
        .gap(12.0)
        .align(Align::Center)
        .into()
}

/// A window-sized host for the widgets that render a full-screen layer.
///
/// `Sheet`, `Drawer` and `Modal` position their scrim with `inset: 0` against
/// the nearest positioned ancestor — and *every* element is one, since the
/// layout default is `Position::Relative`. Dropped into a centred flex stack,
/// their scrim therefore lands on that stack's box instead of the window. This
/// host is absolutely positioned at the window origin and sized to it, so the
/// layer resolves against the window the way it would in a real app. Entries
/// using it are [`Place::Overlay`], which drops the stage's padding.
fn window_layer(cx: &BuildCx, page: Element, layer: Element) -> Element {
    let win = cx.size();
    let mut page = page;
    page.style.width = Dim::pct(1.0);
    page.style.height = Dim::pct(1.0);
    Element {
        role: Role::Generic,
        elide_semantics: true,
        style: LayoutStyle {
            position: Position::Absolute,
            inset: Edges {
                left: Dim::px(0.0),
                top: Dim::px(0.0),
                ..Edges::AUTO
            },
            width: Dim::px(win.width as f32),
            height: Dim::px(win.height as f32),
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            ..LayoutStyle::default()
        },
        children: vec![page, layer],
        ..Element::default()
    }
}

/// A horizontal run of demo pieces.
fn line(kids: Vec<Element>) -> Element {
    Container::new(kids)
        .row()
        .gap(12.0)
        .align(Align::Center)
        .into()
}

// ------------------------------------------------------------ text/display ---

fn d_label(_cx: &mut BuildCx) -> Element {
    stack(vec![
        Label::new("Ship it.")
            .size(30.0)
            .weight(700.0)
            .color(ink())
            .into(),
        Label::new(
            "Typography is Rust: size, weight, colour, family, line height and letter \
             spacing are builder methods on Label.",
        )
        .size(14.0)
        .color(muted())
        .line_height(1.45)
        .width(420.0)
        .into(),
        Label::new("SPACED  OUT")
            .size(13.0)
            .weight(600.0)
            .letter_spacing(3.0)
            .color(accent())
            .into(),
    ])
}

fn d_rich_text(_cx: &mut BuildCx) -> Element {
    let runs = [
        Run {
            text: "Build ",
            color: ink(),
            size: 20.0,
        },
        Run {
            text: "fast",
            color: accent(),
            size: 26.0,
        },
        Run {
            text: ", stay ",
            color: ink(),
            size: 20.0,
        },
        Run {
            text: "correct",
            color: green(),
            size: 26.0,
        },
        Run {
            text: ".",
            color: ink(),
            size: 20.0,
        },
    ];
    stack(vec![
        RichText::new(&runs).into(),
        note("One paragraph, five differently-styled runs laid out in a row."),
    ])
}

fn d_markdown(_cx: &mut BuildCx) -> Element {
    framed_auto(
        markdown::render(
            "# Release 1.2\n\
             Ships the *reactive* text path and a faster `glyph atlas`.\n\
             \n\
             ## Highlights\n\
             - Bindings patch instead of rebuilding\n\
             - Damage-tracked repaint\n\
             - Zero-copy font registration\n\
             \n\
             ```\n\
             just run widget_showcase\n\
             ```\n",
        ),
        460.0,
    )
}

fn d_icon(_cx: &mut BuildCx) -> Element {
    let glyphs = [
        "search", "check", "plus", "close", "menu", "home", "gear", "star",
    ];
    stack(vec![
        line(glyphs.iter().map(|g| Icon::new(g).into()).collect()),
        note(
            "Vector glyphs drawn by the renderer — search, check, plus, close, menu, home, \
             gear, and the default star.",
        ),
    ])
}

fn d_image(_cx: &mut BuildCx) -> Element {
    // A 200x120 dusk gradient, generated so the example needs no asset file.
    let (w, h) = (200u32, 120u32);
    let px: Vec<u8> = (0..w * h)
        .flat_map(|i| {
            let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
            [
                (0x2b as f32 + x * 180.0) as u8,
                (0x3a as f32 + y * 90.0) as u8,
                (0xd0 as f32 - y * 60.0) as u8,
                0xff,
            ]
        })
        .collect();
    stack(vec![
        Image::new(RgbaImage::from_raw(w, h, px)).into(),
        note("An RGBA buffer rendered at its own pixel size (real apps decode PNG/JPEG bytes)."),
    ])
}

fn d_avatar(_cx: &mut BuildCx) -> Element {
    stack(vec![
        line(vec![
            Avatar::new("Ada Lovelace", 56.0).into(),
            Avatar::new("Grace Hopper", 56.0).into(),
            Avatar::new("Alan Turing", 56.0).into(),
        ]),
        note(
            "Initials come from the name and the tint is hashed from it, so a person keeps \
             the same colour everywhere.",
        ),
    ])
}

fn d_badge(_cx: &mut BuildCx) -> Element {
    // A badge's pill sits at `top: -9, right: -14` of its own wrapper by design,
    // so it always overhangs — the visual lint reports it as W0103 and no
    // wrapper padding can absorb it (insets resolve against the border box).
    // The padding here is purely so neighbouring badges do not collide; the
    // lint exemption lives in `tests/showcase.rs`.
    let pad = |b: Badge| -> Element {
        let mut e: Element = b.into();
        e.style.padding = lumen_layout::Edges {
            top: Dim::px(9.0),
            right: Dim::px(14.0),
            left: Dim::px(0.0),
            bottom: Dim::px(0.0),
        };
        e
    };
    stack(vec![
        line(vec![
            pad(Badge::new(Icon::new("home").into(), "3")),
            pad(Badge::new(
                Label::new("Inbox").size(16.0).color(ink()).into(),
                "12",
            )),
            pad(Badge::new(Icon::new("gear").into(), "").dot().color(red())),
        ]),
        note("A count, a longer count, and a bare dot — each pinned to its target's corner."),
    ])
}

fn d_card(cx: &mut BuildCx) -> Element {
    let opened = cx.signal("card-opened", || 0i64);
    line(vec![
        Card::new(vec![
            Label::new("$1,240.00")
                .size(26.0)
                .weight(700.0)
                .color(ink())
                .into(),
            note("+12.4% vs last month"),
        ])
        .title("Total balance")
        .into(),
        Card::new(vec![note("Click this card — it is pressable.")])
            .title("Pressable")
            .on_press(move |rt| opened.update(rt, |n| *n += 1))
            .id("card-pressable")
            .into(),
        Card::new(vec![note("No shadow, just a hairline.")])
            .title("Flat")
            .flat()
            .into(),
    ])
}

fn d_chip(cx: &mut BuildCx) -> Element {
    let picked = cx.signal("chip-picked", || 1usize);
    let hidden = cx.signal("chip-hidden", || false);
    let cur = picked.get(cx.runtime());
    let mut kids: Vec<Element> = ["Rust", "GPU", "Layout"]
        .iter()
        .enumerate()
        .map(|(i, s)| {
            Chip::new(*s)
                .selected(i == cur, move |rt| picked.set(rt, i))
                .id(format!("chip-{i}"))
                .into()
        })
        .collect();
    if !hidden.get(cx.runtime()) {
        kids.push(
            Chip::new("Removable")
                .icon("close")
                .on_remove(move |rt| hidden.set(rt, true))
                .id("chip-removable")
                .into(),
        );
    }
    stack(vec![
        Wrap::new(kids).into(),
        note("Click a chip to select it; the last one removes itself."),
    ])
}

fn d_rule(_cx: &mut BuildCx) -> Element {
    let mut col: Element = Container::new(vec![
        note("Above the rule"),
        Rule::horizontal()
            .background(Color::srgb8(0xc7, 0xce, 0xdd, 0xff))
            .into(),
        note("Below the rule"),
        Rule::horizontal()
            .thickness(3.0)
            .background(accent())
            .into(),
        line(vec![
            note("left"),
            Rule::vertical().background(muted()).into(),
            note("right"),
        ]),
    ])
    .column()
    .gap(12.0)
    .into();
    col.style.width = Dim::px(320.0);
    col
}

fn d_space(_cx: &mut BuildCx) -> Element {
    let mut row: Element = Container::new(vec![
        Label::new("left").size(15.0).color(ink()).into(),
        Space::new().into(),
        Label::new("pushed apart").size(15.0).color(accent()).into(),
        Space::horizontal(48.0).into(),
        Label::new("right").size(15.0).color(ink()).into(),
    ])
    .row()
    .align(Align::Center)
    .into();
    row.style.width = Dim::pct(1.0);
    stack(vec![
        framed(row, 470.0, 62.0),
        note("A bare Space() grows to eat the slack; Space::horizontal(48) is a fixed gap."),
    ])
}

fn d_skeleton(cx: &mut BuildCx) -> Element {
    stack(vec![
        Skeleton::new(cx, 260.0, 18.0).into(),
        Skeleton::new(cx, 320.0, 12.0).into(),
        Skeleton::new(cx, 300.0, 12.0).into(),
        Skeleton::new(cx, 180.0, 12.0).into(),
        note("A shimmering placeholder for content that has not arrived yet."),
    ])
}

fn d_spinner(cx: &mut BuildCx) -> Element {
    stack(vec![
        line(vec![
            Spinner::new(cx, 20.0).into(),
            Spinner::new(cx, 34.0).into(),
            Spinner::colored(cx, 48.0, green()).into(),
        ]),
        note("Animated by the frame clock — it keeps spinning without app state."),
    ])
}

fn d_toast(cx: &mut BuildCx) -> Element {
    let retried = cx.signal("toast-retried", || 0i64);
    stack(vec![
        Toast::new(ToastKind::Success, "Saved", "Your changes are stored.").into(),
        Toast::new(ToastKind::Info, "Syncing", "3 files left to upload.").into(),
        Toast::new(ToastKind::Warn, "Low disk", "Under 1 GB free on /data.").into(),
        Toast::new(ToastKind::Danger, "Upload failed", "Connection reset.")
            .action("Retry", move |rt| retried.update(rt, |n| *n += 1))
            .id("toast-retry")
            .into(),
    ])
}

fn d_tooltip(cx: &mut BuildCx) -> Element {
    stack(vec![
        Tooltip::new(
            cx,
            "tip",
            Button::new("Hover me").on_press(|_| {}).into(),
            "Tooltips are hover-gated and cause no layout shift.",
        )
        .into(),
        note("Hover the button — the tip paints as an overlay above it."),
    ])
}

// ------------------------------------------------------- buttons and input ---

fn d_button(cx: &mut BuildCx) -> Element {
    let count = cx.signal("button-count", || 0i64);
    stack(vec![
        line(vec![
            Button::new("Primary")
                .primary()
                .on_press(move |rt| count.update(rt, |n| *n += 1))
                .id("btn-primary")
                .into(),
            Button::new("Default")
                .on_press(move |rt| count.update(rt, |n| *n += 1))
                .id("btn-default")
                .into(),
            Button::new("Ghost")
                .ghost()
                .on_press(move |rt| count.update(rt, |n| *n += 1))
                .id("btn-ghost")
                .into(),
            Button::new("Disabled")
                .on_press(move |rt| count.update(rt, |n| *n += 1))
                .disabled(true)
                .id("btn-disabled")
                .into(),
        ]),
        note(
            "The disabled button dims itself, takes no clicks, no hover and no Tab, and \
             drops the actions it advertises — so the agent refuses it too.",
        ),
    ])
}

fn d_check_box(cx: &mut BuildCx) -> Element {
    cx.signal("cb-ship", || true);
    Container::new(vec![
        CheckBox::new(cx, "cb-ship", "Ship to billing address")
            .id("cb-ship")
            .into(),
        CheckBox::new(cx, "cb-gift", "This is a gift")
            .id("cb-gift")
            .into(),
        CheckBox::new(cx, "cb-news", "Email me release notes")
            .id("cb-news")
            .into(),
        CheckBox::new(cx, "cb-locked", "Locked by policy")
            .disabled(true)
            .id("cb-locked")
            .into(),
    ])
    .column()
    .gap(10.0)
    .into()
}

fn d_switch(cx: &mut BuildCx) -> Element {
    cx.signal("sw-wifi", || true);
    Container::new(vec![
        Switch::new(cx, "sw-wifi", "Wi-Fi").id("sw-wifi").into(),
        Switch::new(cx, "sw-bt", "Bluetooth").id("sw-bt").into(),
        Switch::new(cx, "sw-dnd", "Do not disturb")
            .id("sw-dnd")
            .into(),
    ])
    .column()
    .gap(12.0)
    .into()
}

fn d_radio(cx: &mut BuildCx) -> Element {
    cx.signal("radio-plan", || "standard".to_string());
    Container::new(vec![
        Radio::new(cx, "radio-plan", "economy", "Economy — 5–7 days")
            .id("radio-economy")
            .into(),
        Radio::new(cx, "radio-plan", "standard", "Standard — 2–3 days")
            .id("radio-standard")
            .into(),
        Radio::new(cx, "radio-plan", "express", "Express — next day")
            .id("radio-express")
            .into(),
    ])
    .column()
    .gap(10.0)
    .into()
}

fn d_slider(cx: &mut BuildCx) -> Element {
    cx.signal("slider-vol", || 65.0f64);
    cx.signal("slider-zoom", || 2.0f64);
    Container::new(vec![
        note("Volume — continuous"),
        Slider::new(cx, "slider-vol", 0.0, 100.0)
            .id("slider-vol")
            .into(),
        note("Zoom — stepped by 0.5"),
        Slider::new(cx, "slider-zoom", 1.0, 4.0)
            .step(0.5)
            .id("slider-zoom")
            .into(),
        note("Focus a slider and use the arrow keys, Home/End or PageUp/PageDown."),
    ])
    .column()
    .gap(10.0)
    .into()
}

fn d_range_slider(cx: &mut BuildCx) -> Element {
    cx.signal("range.lo", || 240.0f64);
    cx.signal("range.hi", || 780.0f64);
    Container::new(vec![
        note("Price range (kr)"),
        RangeSlider::new(cx, "range", 0.0, 1000.0)
            .id("range")
            .into(),
    ])
    .column()
    .gap(10.0)
    .into()
}

fn d_stepper(cx: &mut BuildCx) -> Element {
    cx.signal("stepper-qty", || 3i64);
    stack(vec![
        line(vec![
            note("Quantity"),
            Stepper::new(cx, "stepper-qty", 0, 10)
                .id("stepper-qty")
                .into(),
        ]),
        note("Clamped to 0–10: the − and + buttons disable at the ends."),
    ])
}

fn d_text_input(cx: &mut BuildCx) -> Element {
    Container::new(vec![
        note("Name"),
        TextInput::new(cx, "ti-name", "Ada Lovelace")
            .id("ti-name")
            .into(),
        note("Email — placeholder only, still empty"),
        TextInput::new(cx, "ti-mail", "")
            .placeholder("you@example.com")
            .id("ti-mail")
            .into(),
        note("Password — masked in the semantic tree too, so it never leaks to the agent"),
        TextInput::new(cx, "ti-pw", "hunter2")
            .password('•')
            .id("ti-pw")
            .into(),
        note("Read-only"),
        TextInput::new(cx, "ti-ro", "LUM-2026-0042")
            .read_only(true)
            .id("ti-ro")
            .into(),
    ])
    .column()
    .gap(6.0)
    .into()
}

fn d_text_field(cx: &mut BuildCx) -> Element {
    stack(vec![
        TextField::new(
            cx,
            "tf-bio",
            "Lumen renders through a uniform Element tree.\nEdit this text — arrows, \
             Home/End, selection and IME all work.",
        )
        .lines(6)
        .width(440.0)
        .id("tf-bio")
        .into(),
        note("A multi-line editor over the same TextEditor state as TextInput."),
    ])
}

fn d_search_field(cx: &mut BuildCx) -> Element {
    // Seeded, not empty: the field is more interesting with the clear button
    // showing — and `SearchField` drops the placeholder it is handed, so an
    // empty one has no accessible name at all (W0301).
    cx.signal("sf-q", || TextEditor::new("virtual"));
    // The editor is the source of truth, but widgets and `TextInput::text_of`
    // read the plain-string mirror — seeding one without the other leaves the
    // readout claiming the field is empty.
    cx.signal("sf-q.text", || "virtual".to_string());
    stack(vec![
        SearchField::new(cx, "sf-q", "Search widgets…")
            .id("sf-q")
            .into(),
        note("Type something — a clear (×) button appears once the field is non-empty."),
    ])
}

fn d_rich_text_editor(cx: &mut BuildCx) -> Element {
    stack(vec![
        RichTextEditor::new(
            cx,
            "rte-doc",
            "# Meeting notes\nDiscussed the **damage tracker** and the *glyph atlas*.",
        )
        .into(),
        note("Markdown-ish source, styled runs on screen."),
    ])
}

fn d_find_replace(cx: &mut BuildCx) -> Element {
    stack(vec![
        RichTextEditor::new(
            cx,
            "fr-doc",
            "the quick brown fox jumps over the lazy dog; the fox is quick",
        )
        .into(),
        FindReplaceBar::new(cx, "fr-bar", "fr-doc").into(),
        note("Type \"fox\" in Find and a replacement, then press Replace."),
    ])
}

fn d_color_picker(cx: &mut BuildCx) -> Element {
    cx.signal("cp-brand", || "#18a05c".to_string());
    stack(vec![
        note("Click the swatch to open the preset palette; the choice lands as a hex string."),
        ColorPicker::new(cx, "cp-brand").id("cp-brand").into(),
    ])
}

fn d_date_picker(cx: &mut BuildCx) -> Element {
    cx.signal("dp-when.year", || 2026i64);
    cx.signal("dp-when.month", || 8i64);
    cx.signal("dp-when.day", || 24i64);
    DatePicker::new(cx, "dp-when").id("dp-when").into()
}

fn d_time_picker(cx: &mut BuildCx) -> Element {
    cx.signal("tp-at.hour", || 14i64);
    cx.signal("tp-at.minute", || 45i64);
    TimePicker::new(cx, "tp-at").id("tp-at").into()
}

fn d_file_picker(cx: &mut BuildCx) -> Element {
    stack(vec![
        FilePicker::new(cx, "fp-doc", "Choose an image…", ["png", "jpg", "webp"])
            .preview(cx, 260.0)
            .id("fp-doc")
            .into(),
        note(
            "Queues a SystemRequest::OpenFile on the host mailbox; the shell opens the native \
             dialog, replies into fp-doc.path, and the picker shows what you chose.",
        ),
    ])
}

// ----------------------------------------------------------------- pickers ---

fn d_pick_list(cx: &mut BuildCx) -> Element {
    cx.signal("pl-city", || "Trondheim".to_string());
    stack(vec![
        note("Focus the trigger and use ↑/↓, Home/End or Escape — no pointer required."),
        PickList::new(
            cx,
            "pl-city",
            "Pick a city…",
            ["Oslo", "Bergen", "Trondheim", "Tromsø", "Stavanger"],
        )
        .into(),
    ])
}

fn d_combobox(cx: &mut BuildCx) -> Element {
    cx.signal("cb-fruit", || TextEditor::new("ap"));
    cx.signal("cb-fruit.text", || "ap".to_string());
    // Rendered with the filtered list showing — that is the interesting state.
    cx.signal("cb-fruit.open", || true);
    stack(vec![
        note("Type to filter the list — \"ap\" narrows it to Apple and Apricot."),
        Combobox::new(
            cx,
            "cb-fruit",
            [
                "Apple",
                "Apricot",
                "Banana",
                "Blackberry",
                "Cherry",
                "Clementine",
                "Fig",
            ],
        )
        .into(),
    ])
}

fn d_select(cx: &mut BuildCx) -> Element {
    cx.signal("sel-size", || 1usize);
    stack(vec![
        note("The compact, index-keyed cousin of PickList."),
        Select::new(cx, "sel-size", &["Small", "Medium", "Large", "X-Large"])
            .id("sel-size")
            .into(),
    ])
}

fn d_menu(cx: &mut BuildCx) -> Element {
    let last = cx.signal("menu-last", || -1i64);
    stack(vec![
        note("Click File — the panel floats over the page and closes on a choice or a click away."),
        Menu::button(
            cx,
            "menu-file",
            "File ▾",
            &["New file", "Open…", "Save", "Save as…", "Quit"],
            move |rt, i| last.set(rt, i as i64),
        )
        .into(),
    ])
}

// ------------------------------------------------------------------ layout ---

fn d_container(_cx: &mut BuildCx) -> Element {
    let cell = |s: &str, c: Color| -> Element {
        let mut e: Element =
            Container::new(vec![Label::new(s).size(14.0).color(Color::WHITE).into()])
                .padding(14.0)
                .into();
        e.background = Some(c);
        e.corner_radius = 8.0;
        e
    };
    stack(vec![
        Container::new(vec![
            cell("row", accent()),
            cell("gap 10", green()),
            cell("pad 10", amber()),
        ])
        .row()
        .gap(10.0)
        .padding(10.0)
        .corner_radius(12.0)
        .background(tint())
        .into(),
        Container::new(vec![cell("column", accent()), cell("gap 10", red())])
            .column()
            .gap(10.0)
            .padding(10.0)
            .corner_radius(12.0)
            .background(tint())
            .into(),
        note(
            "row/column/stack, gap, padding, align, justify, corner radius — the flex box you \
             reach for first.",
        ),
    ])
}

fn d_align_box(_cx: &mut BuildCx) -> Element {
    let cell = |label: &str, a: Align, j: Align| -> Element {
        let mut e: Element =
            AlignBox::new(Label::new(label).size(13.0).color(ink()).into(), a, j).into();
        e.background = Some(tint());
        e.corner_radius = 8.0;
        e.style.width = Dim::px(150.0);
        e.style.height = Dim::px(80.0);
        e
    };
    stack(vec![
        line(vec![
            cell("top-left", Align::Start, Align::Start),
            cell("centre", Align::Center, Align::Center),
            cell("bottom-right", Align::End, Align::End),
        ]),
        note("AlignBox fills its parent and parks one child anywhere in it."),
    ])
}

fn d_wrap(_cx: &mut BuildCx) -> Element {
    let tags = [
        "layout",
        "renderer",
        "signals",
        "damage",
        "glyph-atlas",
        "taffy",
        "wgpu",
        "accessibility",
        "hot-reload",
        "snapshot",
        "headless",
        "agent",
    ];
    let mut w: Element = Wrap::new(tags.iter().map(|t| Chip::new(*t).into()).collect()).into();
    w.style.width = Dim::pct(1.0);
    stack(vec![
        framed_auto(w, 470.0),
        note("Children flow onto the next line when they run out of width."),
    ])
}

fn d_grid(cx: &mut BuildCx) -> Element {
    const REGION: [&str; 8] = [
        "Nord", "Vest", "Sør", "Øst", "Midt", "Svalbard", "Total", "Δ",
    ];
    // A grid virtualizes during build, before layout has run, so it must be
    // told the window-space rect it will occupy — the default is the whole
    // surface, which would lay out cells for a box twice this one's size (and
    // send drag coordinates to the wrong column). This entry is `Wide`, so the
    // rect it gets is exactly the stage's content box.
    let (x, y, w, h) = crate::stage_content_rect(cx.size());
    let mut g = Grid::new("grid-sheet", 40, 8, 112.0, 30.0)
        .viewport(x, y, w, h)
        .col_header(28.0, |c| {
            Label::new(["Region", "Q1", "Q2", "Q3", "Q4", "Total", "Δ %", "Rank"][c as usize % 8])
                .size(12.0)
                .weight(700.0)
                .color(Color::WHITE)
                .into()
        })
        .row_header(36.0, |r| {
            Label::new(format!("{}", r + 1))
                .size(12.0)
                .color(Color::srgb8(0x9a, 0xa4, 0xbb, 0xff))
                .into()
        })
        .cell(|_, c| {
            Some(if c.col == 0 {
                Label::new(REGION[c.row as usize % 8])
                    .size(13.0)
                    .color(Color::WHITE)
                    .into()
            } else {
                Label::new(format!("{}", 120 + c.row * 37 + c.col * 11))
                    .size(13.0)
                    .color(Color::srgb8(0xb6, 0xc0, 0xd6, 0xff))
                    .into()
            })
        })
        .resizable(true)
        .zoomable(true)
        .build(cx);
    g.style.width = Dim::px(w as f32);
    g.style.height = Dim::px(h as f32);
    g
}

fn d_split_pane(_cx: &mut BuildCx) -> Element {
    let pane = |title: &str, body: &str, bg: Color| -> Element {
        let mut e: Element = Container::new(vec![
            Label::new(title)
                .size(14.0)
                .weight(700.0)
                .color(ink())
                .into(),
            Label::new(body)
                .size(12.0)
                .color(muted())
                .line_height(1.4)
                .into(),
        ])
        .column()
        .gap(6.0)
        .padding(12.0)
        .into();
        e.background = Some(bg);
        e.style.width = Dim::pct(1.0);
        e.style.height = Dim::pct(1.0);
        e
    };
    let mut split: Element = SplitPane::new(
        pane(
            "Explorer",
            "35% of the width",
            Color::srgb8(0xdd, 0xe6, 0xf7, 0xff),
        ),
        pane(
            "Editor",
            "the remaining 65%",
            Color::srgb8(0xe4, 0xf0, 0xdd, 0xff),
        ),
        0.35,
    )
    .into();
    split.style.height = Dim::px(240.0);
    split
}

fn d_pane_grid(cx: &mut BuildCx) -> Element {
    let pane = |t: &str, bg: Color| -> Element {
        let mut e: Element = Container::new(vec![Label::new(t).size(14.0).color(ink()).into()])
            .padding(14.0)
            .into();
        e.background = Some(bg);
        e.style.width = Dim::pct(1.0);
        e.style.height = Dim::pct(1.0);
        e
    };
    let mut pg: Element = PaneGrid::new(
        cx,
        "pg-demo",
        pane("Pane A", Color::srgb8(0xdd, 0xe6, 0xf7, 0xff)),
        pane("Pane B", Color::srgb8(0xf7, 0xe9, 0xdd, 0xff)),
    )
    .into();
    pg.style.height = Dim::px(240.0);
    stack(vec![
        pg,
        note("Drag the divider — the split ratio is app state, so it survives a snapshot."),
    ])
}

fn d_scrollable(cx: &mut BuildCx) -> Element {
    // Deliberately under ~100 nodes: past that the lint (W0108) rightly says
    // to reach for `VirtualList`, which is its own entry in this catalog.
    const LINES: usize = 24;
    let rows: Vec<Element> = (0..LINES)
        .map(|i| {
            let mut r: Element = Container::new(vec![
                Label::new(format!("{:02}", i + 1))
                    .size(12.0)
                    .color(muted())
                    .into(),
                Label::new(format!(
                    "Log line {} — pointer, wheel and keyboard all scroll",
                    i + 1
                ))
                .size(13.0)
                .color(ink())
                .into(),
            ])
            .row()
            .gap(10.0)
            .padding(6.0)
            .into();
            if i % 2 == 0 {
                r.background = Some(tint());
            }
            r
        })
        .collect();
    let mut sc: Element =
        Scrollable::new(cx, "scroll-log", 260.0, LINES as f64 * 34.0, rows).into();
    sc.style.width = Dim::px(440.0);
    stack(vec![
        sc,
        note("Wheel, drag the bar, or focus it and use arrows / PageUp / Home / End."),
    ])
}

fn d_accordion(cx: &mut BuildCx) -> Element {
    cx.signal("acc-what", || true);
    let mut col: Element = Container::new(vec![
        Accordion::new(cx, "acc-what", "What is Lumen?")
            .body([Label::new(
                "An AI-first GUI framework: one uniform Element tree that renders, lays \
                 out, exposes semantics and drives itself headlessly.",
            )
            .size(12.0)
            .color(muted())
            .line_height(1.4)
            .width(420.0)
            .into()])
            .id("acc-what")
            .into(),
        Accordion::new(cx, "acc-state", "Where does state live?")
            .body([
                Label::new("In signals, keyed by a scope path. Handlers mutate; build reads.")
                    .size(12.0)
                    .color(muted())
                    .width(420.0)
                    .into(),
            ])
            .id("acc-state")
            .into(),
        Accordion::new(cx, "acc-style", "How do I style it?")
            .body([Label::new(
                "Layout and typography in Rust; colours, themes and tokens in .lss.",
            )
            .size(12.0)
            .color(muted())
            .width(420.0)
            .into()])
            .id("acc-style")
            .into(),
    ])
    .column()
    .gap(8.0)
    .into();
    col.style.width = Dim::px(460.0);
    col
}

// -------------------------------------------------------------------- data ---

const CITIES: [(&str, &str, i32); 12] = [
    ("Oslo", "Viken", 709_037),
    ("Bergen", "Vestland", 289_330),
    ("Trondheim", "Trøndelag", 214_565),
    ("Stavanger", "Rogaland", 149_048),
    ("Bærum", "Viken", 129_874),
    ("Kristiansand", "Agder", 116_986),
    ("Drammen", "Viken", 102_138),
    ("Asker", "Viken", 96_262),
    ("Lillestrøm", "Viken", 88_182),
    ("Fredrikstad", "Viken", 83_193),
    ("Sandnes", "Rogaland", 80_732),
    ("Tromsø", "Troms", 77_544),
];

fn d_virtual_list(cx: &mut BuildCx) -> Element {
    let mut vl: Element = VirtualList::new(cx, "vl-cities", 100_000, 30.0, 280.0, |i| {
        let (name, region, pop) = CITIES[i % CITIES.len()];
        let mut r: Element = Container::new(vec![
            Label::new(format!("#{i:06}"))
                .size(12.0)
                .color(muted())
                .into(),
            Label::new(format!("{name}, {region}"))
                .size(13.0)
                .color(ink())
                .into(),
            Space::new().into(),
            Label::new(format!("{pop}"))
                .size(13.0)
                .color(accent())
                .into(),
        ])
        .row()
        .gap(12.0)
        .align(Align::Center)
        .padding(6.0)
        .into();
        r.style.width = Dim::pct(1.0);
        r
    })
    .into();
    vl.style.width = Dim::px(440.0);
    stack(vec![
        vl,
        note(
            "100 000 rows — only the visible window exists in the tree, so cost is independent \
             of the count.",
        ),
    ])
}

fn d_data_grid(cx: &mut BuildCx) -> Element {
    let mut dg: Element = DataGrid::new(
        cx,
        "dg-cities",
        &["City", "Region", "Population"],
        1_000,
        28.0,
        280.0,
        |r, c| {
            let (name, region, pop) = CITIES[r % CITIES.len()];
            match c {
                0 => format!("{name} {}", r / CITIES.len() + 1),
                1 => region.to_string(),
                _ => format!("{pop}"),
            }
        },
    )
    .into();
    dg.style.width = Dim::px(460.0);
    stack(vec![
        dg,
        note("A sticky header over a virtualized body — 1 000 rows here, 1M works the same."),
    ])
}

fn d_tree(cx: &mut BuildCx) -> Element {
    use std::collections::HashSet;
    cx.signal("tree-fs", || {
        HashSet::from(["crates".to_string(), "core".to_string()])
    });
    let rows = [
        TreeRow {
            id: "crates",
            label: "crates/",
            depth: 0,
            has_children: true,
        },
        TreeRow {
            id: "core",
            label: "lumen-core/",
            depth: 1,
            has_children: true,
        },
        TreeRow {
            id: "core-state",
            label: "state.rs",
            depth: 2,
            has_children: false,
        },
        TreeRow {
            id: "core-sem",
            label: "semantics.rs",
            depth: 2,
            has_children: false,
        },
        TreeRow {
            id: "widgets",
            label: "lumen-widgets/",
            depth: 1,
            has_children: true,
        },
        TreeRow {
            id: "w-button",
            label: "button.rs",
            depth: 2,
            has_children: false,
        },
        TreeRow {
            id: "w-slider",
            label: "slider.rs",
            depth: 2,
            has_children: false,
        },
        TreeRow {
            id: "examples",
            label: "examples/",
            depth: 0,
            has_children: true,
        },
        TreeRow {
            id: "ex-showcase",
            label: "widget_showcase/",
            depth: 1,
            has_children: false,
        },
    ];
    let mut t: Element = Tree::new(cx, "tree-fs", &rows).into();
    t.style.width = Dim::px(320.0);
    stack(vec![
        t,
        note("Click a parent to expand or collapse it; descendants hide with it."),
    ])
}

fn d_pagination(cx: &mut BuildCx) -> Element {
    const PER_PAGE: usize = 4;
    const PAGES: i64 = 5;
    cx.signal("pg.page", || 2i64);
    // A pager with nothing to page through shows only that the number changes.
    // This one actually slices a list, which is the whole point of the widget.
    let page = cx.signal("pg.page", || 1i64).get(cx.runtime()).max(1) as usize;
    let rows: Vec<Element> = (0..PER_PAGE)
        .map(|i| {
            let n = (page - 1) * PER_PAGE + i;
            let (name, region, pop) = CITIES[n % CITIES.len()];
            let mut r: Element = Container::new(vec![
                Label::new(format!("{:02}", n + 1))
                    .size(12.0)
                    .color(muted())
                    .into(),
                Label::new(format!("{name}, {region}"))
                    .size(13.0)
                    .color(ink())
                    .into(),
                Space::new().into(),
                Label::new(format!("{pop}"))
                    .size(13.0)
                    .color(accent())
                    .into(),
            ])
            .row()
            .gap(12.0)
            .align(Align::Center)
            .padding(8.0)
            .into();
            r.style.width = Dim::pct(1.0);
            if i % 2 == 0 {
                r.background = Some(tint());
            }
            r
        })
        .collect();
    let mut list: Element = Container::new(rows).column().gap(2.0).into();
    list.style.width = Dim::px(380.0);
    stack(vec![
        list,
        Pagination::new(cx, "pg", PAGES).into(),
        note("Five pages of four cities each — the page number slices the list."),
    ])
}

// ------------------------------------------------------ charts and progress ---

fn d_bar_chart(_cx: &mut BuildCx) -> Element {
    stack(vec![
        BarChart::new(&[12.0, 19.0, 8.0, 22.0, 17.0, 25.0, 14.0], 420.0, 200.0).into(),
        note("Seven values, auto-scaled to the tallest bar."),
    ])
}

fn d_line_chart(_cx: &mut BuildCx) -> Element {
    let mut chart = LineChart::element(
        vec![3.0, 7.0, 5.0, 9.0, 6.0, 11.0, 8.0],
        ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    );
    chart.style.width = Dim::px(440.0);
    chart.style.height = Dim::px(220.0);
    stack(vec![chart, note("Frame time p95 (ms) over a week.")])
}

fn d_pie_chart(_cx: &mut BuildCx) -> Element {
    let mut chart = PieChart::element(vec![
        PieSlice {
            label: "Layout".into(),
            value: 34.0,
            color: accent(),
        },
        PieSlice {
            label: "Paint".into(),
            value: 28.0,
            color: green(),
        },
        PieSlice {
            label: "Text".into(),
            value: 21.0,
            color: amber(),
        },
        PieSlice {
            label: "Build".into(),
            value: 11.0,
            color: red(),
        },
        PieSlice {
            label: "Other".into(),
            value: 6.0,
            color: muted(),
        },
    ]);
    chart.style.width = Dim::px(240.0);
    chart.style.height = Dim::px(240.0);
    stack(vec![
        chart,
        note("Five slices, each carrying its own colour and label."),
    ])
}

fn d_progress_bar(cx: &mut BuildCx) -> Element {
    Container::new(vec![
        note("Download — 65%"),
        ProgressBar::new(0.65)
            .width(420.0)
            .fill_color(accent())
            .into(),
        note("Disk usage — 92%"),
        ProgressBar::new(0.92)
            .width(420.0)
            .height(14.0)
            .fill_color(red())
            .into(),
        note("Indeterminate — no known total"),
        ProgressBar::indeterminate(cx)
            .width(420.0)
            .fill_color(green())
            .into(),
    ])
    .column()
    .gap(8.0)
    .into()
}

fn d_canvas(cx: &mut BuildCx) -> Element {
    use kurbo::{Point, Rect};
    use lumen_render::Brush;

    // Live controls, so the canvas is something you *drive* rather than a
    // static picture that happens to be drawn in code.
    cx.signal("canvas-speed", || 1.0f64);
    cx.signal("canvas-waves", || 3.0f64);
    let speed = cx.signal("canvas-speed", || 1.0f64).get(cx.runtime());
    let waves = cx.signal("canvas-waves", || 3.0f64).get(cx.runtime());

    // `animate()` asks for a frame every tick; `now_ms()` is the virtual clock,
    // so the same code is deterministic under a headless test.
    cx.animate();
    let t = cx.now_ms() / 1000.0 * speed;

    let canvas = Canvas::new(440.0, 200.0, move |f, size| {
        f.fill_rect(
            Rect::new(0.0, 0.0, size.width, size.height),
            Brush::Solid(Color::srgb8(0x10, 0x18, 0x2c, 0xff)),
        );
        let mid = size.height * 0.5;
        let wave = |x: f64, phase: f64| {
            mid + ((x / size.width * std::f64::consts::TAU * waves) + phase).sin()
                * size.height
                * 0.3
        };
        // Three phase-shifted traces, plotted column by column.
        for (k, c) in [accent(), green(), amber()].into_iter().enumerate() {
            let phase = t + k as f64 * 0.7;
            for i in 0..(size.width as i32) {
                let x = i as f64;
                let y = wave(x, phase);
                f.fill_rect(Rect::new(x, y - 1.2, x + 2.0, y + 1.2), Brush::Solid(c));
            }
            let head = (t * 60.0 * speed) % size.width;
            f.fill_circle(Point::new(head, wave(head, phase)), 7.0, c);
        }
    });

    Container::new(vec![
        canvas.into(),
        note("Speed"),
        Slider::new(cx, "canvas-speed", 0.0, 3.0).step(0.25).id("canvas-speed").into(),
        note("Waves"),
        Slider::new(cx, "canvas-waves", 1.0, 6.0).step(1.0).id("canvas-waves").into(),
        note("Immediate mode: `draw` paints into a Frame every frame; cx.animate() keeps them coming."),
    ])
    .column()
    .gap(8.0)
    .align(Align::Center)
    .into()
}

// -------------------------------------------------------------- navigation ---

fn d_tabs(cx: &mut BuildCx) -> Element {
    let which = cx.signal("tabs-section", || 1usize).get(cx.runtime());
    let body = [
        "Overview shows the summary.",
        "Activity lists recent events.",
        "Settings holds the knobs.",
    ];
    Container::new(vec![
        Tabs::new(cx, "tabs-section", &["Overview", "Activity", "Settings"])
            .id("tabs-section")
            .into(),
        framed(note(body[which.min(2)]), 460.0, 90.0),
        note("←/→ move between tabs when the strip has focus."),
    ])
    .column()
    .gap(12.0)
    .into()
}

fn d_app_bar(cx: &mut BuildCx) -> Element {
    let saved = cx.signal("appbar-saved", || 0i64);
    // An AppBar is the *screen's* header strip — the bar across the top of a
    // page carrying its title and that page's actions. Material calls it a top
    // app bar; iOS calls it a navigation bar. It is not the OS window title bar
    // (that is `system::WindowDesc`) and not the application menu bar (that is
    // `system::MenuModel`); it belongs to the screen, so it changes as you
    // navigate.
    let mut page: Element = Container::new(vec![
        AppBar::new(
            "Inbox",
            vec![
                Button::new("Edit")
                    .ghost()
                    .on_press(|_| {})
                    .id("appbar-edit")
                    .into(),
                Button::new("Save")
                    .primary()
                    .on_press(move |rt| saved.update(rt, |n| *n += 1))
                    .id("appbar-save")
                    .into(),
            ],
        )
        .into(),
        framed(
            Label::new(
                "The screen's own header strip: its title on the left, that screen's \
                 actions on the right. Not the OS title bar (system::WindowDesc) and not \
                 the app menu bar (system::MenuModel) — this one changes as you navigate.",
            )
            .size(12.0)
            .color(muted())
            .line_height(1.45)
            .width(420.0)
            .into(),
            460.0,
            120.0,
        ),
    ])
    .column()
    .gap(12.0)
    .into();
    page.style.width = Dim::px(460.0);
    page
}

fn d_bottom_nav(cx: &mut BuildCx) -> Element {
    cx.signal("bn-tab", || 0usize);
    let mut nav: Element = BottomNav::new(cx, "bn-tab", &["Home", "Search", "Library", "Me"])
        .id("bn-tab")
        .into();
    nav.style.width = Dim::px(460.0);
    stack(vec![
        nav,
        note("The mobile tab bar — one selected destination at a time."),
    ])
}

fn d_navigation_rail(cx: &mut BuildCx) -> Element {
    cx.signal("rail-dest", || 2usize);
    line(vec![
        NavigationRail::new(cx, "rail-dest", &["Home", "Files", "Chat", "Config"])
            .id("rail-dest")
            .into(),
        framed(
            Label::new("The rail is the desktop and tablet counterpart of BottomNav.")
                .size(12.0)
                .color(muted())
                .line_height(1.4)
                .width(290.0)
                .into(),
            330.0,
            220.0,
        ),
    ])
}

fn d_pull_to_refresh(cx: &mut BuildCx) -> Element {
    let ticks = cx.signal("ptr.ticks", || 0i64);
    let refreshing = cx.signal("ptr.feed.refreshing", || false);
    let n = ticks.get(cx.runtime());
    // The feed grows on each refresh, so the gesture has a visible result. The
    // widget leaves `{name}.refreshing` for the app to clear — that flag is how
    // it knows the work is still in flight, and never clearing it is why the
    // busy state used to stick on forever.
    let rows: Vec<Element> = (0..8)
        .map(|i| note(format!("Feed item {}", n * 8 + 8 - i)))
        .collect();
    let mut p: Element = PullToRefresh::new(
        cx,
        "ptr.feed",
        60.0,
        move |rt| {
            ticks.update(rt, |k| *k += 1);
            refreshing.set(rt, false);
        },
        rows,
    )
    .into();
    p.style.width = Dim::px(420.0);
    stack(vec![
        framed(p, 450.0, 270.0),
        note("Scroll UP past the top of the list (or pull down on a touch screen) to refresh."),
    ])
}

// ---------------------------------------------------------------- overlays ---

fn d_modal(cx: &mut BuildCx) -> Element {
    let open = cx.signal("modal-open", || false);
    let is_open = open.get(cx.runtime());
    let base = stack(vec![
        Button::new("Delete project…")
            .on_press(move |rt| open.set(rt, true))
            .id("modal-open")
            .into(),
        note("The dialog stacks a scrim over the page and traps the click."),
    ]);
    let dialog: Element = Container::new(vec![
        Label::new("Delete “lumen-shell”?")
            .size(17.0)
            .weight(700.0)
            .color(ink())
            .into(),
        note("This cannot be undone."),
        Container::new(vec![
            Button::new("Cancel")
                .ghost()
                .on_press(move |rt| open.set(rt, false))
                .id("modal-cancel")
                .into(),
            Button::new("Delete")
                .primary()
                .on_press(move |rt| open.set(rt, false))
                .id("modal-confirm")
                .into(),
        ])
        .row()
        .gap(10.0)
        .into(),
    ])
    .column()
    .gap(12.0)
    .padding(20.0)
    .corner_radius(12.0)
    .background(Color::WHITE)
    .into();
    let win = cx.size();
    let mut modal: Element = Modal::new(AlignBox::center(base).into(), dialog, is_open).into();
    modal.style.position = Position::Absolute;
    modal.style.inset = Edges {
        left: Dim::px(0.0),
        top: Dim::px(0.0),
        ..Edges::AUTO
    };
    modal.style.width = Dim::px(win.width as f32);
    modal.style.height = Dim::px(win.height as f32);
    modal
}

fn d_popover(cx: &mut BuildCx) -> Element {
    let content: Element = Container::new(vec![
        Label::new("Signed in as").size(12.0).color(muted()).into(),
        Label::new("ada@lumen.dev")
            .size(14.0)
            .weight(600.0)
            .color(ink())
            .into(),
        Rule::horizontal().into(),
        note("Click anywhere outside to dismiss."),
    ])
    .column()
    .gap(6.0)
    .into();
    stack(vec![
        Popover::new(
            cx,
            "pop-acct",
            Button::new("Account ▾").on_press(|_| {}).into(),
            content,
        )
        .into(),
        note("A light-dismiss anchored panel; .side(Above) flips it."),
    ])
}

fn d_sheet(cx: &mut BuildCx) -> Element {
    let open = cx.signal("sheet-demo.open", || false);
    let content: Element = Container::new(vec![
        Label::new("Share this file")
            .size(17.0)
            .weight(700.0)
            .color(ink())
            .into(),
        line(vec![
            Chip::new("Copy link").into(),
            Chip::new("Email").into(),
            Chip::new("Export PDF").into(),
        ]),
        Button::new("Close")
            .ghost()
            .on_press(move |rt| open.set(rt, false))
            .id("sheet-close")
            .into(),
    ])
    .column()
    .gap(12.0)
    .into();
    let page = AlignBox::center(stack(vec![
        Button::new("Open bottom sheet")
            .primary()
            .on_press(move |rt| open.set(rt, true))
            .id("sheet-open")
            .into(),
        note("The panel slides up over a scrim; click the scrim or press Escape to dismiss."),
    ]))
    .into();
    window_layer(cx, page, Sheet::new(cx, "sheet-demo", content).into())
}

fn d_drawer(cx: &mut BuildCx) -> Element {
    let open = cx.signal("drawer-demo.open", || false);
    let content: Element = Container::new(vec![
        Label::new("Navigation")
            .size(16.0)
            .weight(700.0)
            .color(ink())
            .into(),
        note("Inbox"),
        note("Starred"),
        note("Drafts"),
        note("Archive"),
        Button::new("Close")
            .ghost()
            .on_press(move |rt| open.set(rt, false))
            .id("drawer-close")
            .into(),
    ])
    .column()
    .gap(10.0)
    .into();
    let page = AlignBox::center(stack(vec![
        Button::new("Open drawer")
            .primary()
            .on_press(move |rt| open.set(rt, true))
            .id("drawer-open")
            .into(),
        note("A 300 px side panel over a scrim; .side(Right) mirrors it."),
    ]))
    .into();
    window_layer(cx, page, Drawer::new(cx, "drawer-demo", content).into())
}

// --------------------------------------------------------------- readouts ---

fn s_button(rt: &Runtime) -> String {
    format!("presses: {}", rt.signal("button-count", || 0i64).get(rt))
}
fn s_check_box(rt: &Runtime) -> String {
    let on = |k: &str| rt.signal(k, || false).get(rt);
    format!(
        "ship={} gift={} news={}",
        on("cb-ship"),
        on("cb-gift"),
        on("cb-news")
    )
}
fn s_switch(rt: &Runtime) -> String {
    let on = |k: &str| rt.signal(k, || false).get(rt);
    format!(
        "wifi={} bluetooth={} dnd={}",
        on("sw-wifi"),
        on("sw-bt"),
        on("sw-dnd")
    )
}
fn s_radio(rt: &Runtime) -> String {
    format!("plan = {}", rt.signal("radio-plan", String::new).get(rt))
}
fn s_slider(rt: &Runtime) -> String {
    format!(
        "volume {:.0}%   zoom {:.1}×",
        rt.signal("slider-vol", || 0.0f64).get(rt),
        rt.signal("slider-zoom", || 1.0f64).get(rt)
    )
}
fn s_range(rt: &Runtime) -> String {
    format!(
        "{:.0} – {:.0} kr",
        rt.signal("range.lo", || 0.0f64).get(rt),
        rt.signal("range.hi", || 0.0f64).get(rt)
    )
}
fn s_stepper(rt: &Runtime) -> String {
    format!("quantity = {}", rt.signal("stepper-qty", || 0i64).get(rt))
}
fn s_text_input(rt: &Runtime) -> String {
    format!(
        "name = {:?}   email = {:?}",
        TextInput::text_of(rt, "ti-name"),
        TextInput::text_of(rt, "ti-mail")
    )
}
fn s_text_field(rt: &Runtime) -> String {
    let t = TextInput::text_of(rt, "tf-bio");
    format!("{} chars, {} lines", t.chars().count(), t.lines().count())
}
fn s_search(rt: &Runtime) -> String {
    format!("query = {:?}", TextInput::text_of(rt, "sf-q"))
}
fn s_color(rt: &Runtime) -> String {
    format!("brand = {}", rt.signal("cp-brand", String::new).get(rt))
}
fn s_date(rt: &Runtime) -> String {
    let g = |k: &str| rt.signal(k, || 0i64).get(rt);
    format!(
        "{:04}-{:02}-{:02}",
        g("dp-when.year"),
        g("dp-when.month"),
        g("dp-when.day")
    )
}
fn s_time(rt: &Runtime) -> String {
    let g = |k: &str| rt.signal(k, || 0i64).get(rt);
    format!("{:02}:{:02}", g("tp-at.hour"), g("tp-at.minute"))
}
fn s_file(rt: &Runtime) -> String {
    let p = rt.signal("fp-doc.path", String::new).get(rt);
    if p.is_empty() {
        "no file chosen yet".into()
    } else {
        format!("chose {p}")
    }
}
fn s_pick_list(rt: &Runtime) -> String {
    format!("city = {}", rt.signal("pl-city", String::new).get(rt))
}
fn s_combobox(rt: &Runtime) -> String {
    format!(
        "typed {:?}, selected {:?}",
        TextInput::text_of(rt, "cb-fruit"),
        rt.signal("cb-fruit.selected", String::new).get(rt)
    )
}
fn s_select(rt: &Runtime) -> String {
    format!("index = {}", rt.signal("sel-size", || 0usize).get(rt))
}
fn s_menu(rt: &Runtime) -> String {
    match rt.signal("menu-last", || -1i64).get(rt) {
        -1 => "nothing chosen yet".into(),
        i => format!("chose item {i}"),
    }
}
fn s_tabs(rt: &Runtime) -> String {
    format!(
        "tab index = {}",
        rt.signal("tabs-section", || 0usize).get(rt)
    )
}
fn s_bottom_nav(rt: &Runtime) -> String {
    format!("destination = {}", rt.signal("bn-tab", || 0usize).get(rt))
}
fn s_rail(rt: &Runtime) -> String {
    format!(
        "destination = {}",
        rt.signal("rail-dest", || 0usize).get(rt)
    )
}
fn s_pagination(rt: &Runtime) -> String {
    format!("page {} of 5", rt.signal("pg.page", || 1i64).get(rt))
}
fn s_ptr(rt: &Runtime) -> String {
    format!(
        "refreshed {} times",
        rt.signal("ptr-ticks", || 0i64).get(rt)
    )
}
fn s_card(rt: &Runtime) -> String {
    format!(
        "card presses: {}",
        rt.signal("card-opened", || 0i64).get(rt)
    )
}
fn s_toast(rt: &Runtime) -> String {
    format!("retries: {}", rt.signal("toast-retried", || 0i64).get(rt))
}
fn s_appbar(rt: &Runtime) -> String {
    format!("saves: {}", rt.signal("appbar-saved", || 0i64).get(rt))
}
fn s_chip(rt: &Runtime) -> String {
    format!(
        "selected chip = {}",
        rt.signal("chip-picked", || 0usize).get(rt)
    )
}
fn s_scroll(rt: &Runtime) -> String {
    format!(
        "offset = {:.0} px",
        rt.signal("scroll-log", || 0.0f64).get(rt)
    )
}

// ----------------------------------------------------------------- catalog ---

macro_rules! entry {
    ($name:literal, $slug:literal, $place:ident, $build:ident, $blurb:literal) => {
        Entry {
            name: $name,
            slug: $slug,
            blurb: $blurb,
            place: Place::$place,
            build: $build,
            status: None,
        }
    };
    ($name:literal, $slug:literal, $place:ident, $build:ident, $blurb:literal, $status:ident) => {
        Entry {
            name: $name,
            slug: $slug,
            blurb: $blurb,
            place: Place::$place,
            build: $build,
            status: Some($status as fn(&Runtime) -> String),
        }
    };
}

static TEXT: &[Entry] = &[
    entry!(
        "Label",
        "label",
        Center,
        d_label,
        "Styled text — typography is a Rust surface."
    ),
    entry!(
        "RichText",
        "rich-text",
        Center,
        d_rich_text,
        "Several styled runs in one paragraph."
    ),
    entry!(
        "Markdown",
        "markdown",
        Center,
        d_markdown,
        "A CommonMark subset rendered to real elements."
    ),
    entry!(
        "Icon",
        "icon",
        Center,
        d_icon,
        "The built-in vector glyph set."
    ),
    entry!(
        "Image",
        "image",
        Center,
        d_image,
        "A decoded RGBA buffer at its own pixel size."
    ),
    entry!(
        "Avatar",
        "avatar",
        Center,
        d_avatar,
        "Initials with a colour hashed from the name."
    ),
    entry!(
        "Badge",
        "badge",
        Center,
        d_badge,
        "A count or dot pinned to any element's corner."
    ),
    entry!(
        "Card",
        "card",
        Center,
        d_card,
        "A titled surface — plain, pressable and flat.",
        s_card
    ),
    entry!(
        "Chip",
        "chip",
        Center,
        d_chip,
        "Compact, selectable, removable tags.",
        s_chip
    ),
    entry!(
        "Rule",
        "rule",
        Center,
        d_rule,
        "Horizontal and vertical separators."
    ),
    entry!(
        "Space",
        "space",
        Center,
        d_space,
        "Fixed gaps and greedy slack between siblings."
    ),
    entry!(
        "Skeleton",
        "skeleton",
        Center,
        d_skeleton,
        "Shimmering placeholders for pending content."
    ),
    entry!(
        "Spinner",
        "spinner",
        Center,
        d_spinner,
        "A frame-clock driven busy indicator."
    ),
    entry!(
        "Toast",
        "toast",
        Center,
        d_toast,
        "The four toast kinds, one with an action.",
        s_toast
    ),
    entry!(
        "Tooltip",
        "tooltip",
        Center,
        d_tooltip,
        "Hover-gated help that causes no layout shift."
    ),
];

static INPUT: &[Entry] = &[
    entry!(
        "Button",
        "button",
        Center,
        d_button,
        "Primary, default, ghost and disabled.",
        s_button
    ),
    entry!(
        "CheckBox",
        "check-box",
        Center,
        d_check_box,
        "Independent booleans, one disabled.",
        s_check_box
    ),
    entry!(
        "Switch",
        "switch",
        Center,
        d_switch,
        "Settings toggles with an on/off track.",
        s_switch
    ),
    entry!(
        "Radio",
        "radio",
        Center,
        d_radio,
        "One choice out of a named group.",
        s_radio
    ),
    entry!(
        "Slider",
        "slider",
        Center,
        d_slider,
        "Continuous and stepped, both keyboard-driveable.",
        s_slider
    ),
    entry!(
        "RangeSlider",
        "range-slider",
        Center,
        d_range_slider,
        "Two thumbs over one track.",
        s_range
    ),
    entry!(
        "Stepper",
        "stepper",
        Center,
        d_stepper,
        "A clamped integer with − and + buttons.",
        s_stepper
    ),
    entry!(
        "TextInput",
        "text-input",
        Center,
        d_text_input,
        "Placeholder, password and read-only.",
        s_text_input
    ),
    entry!(
        "TextField",
        "text-field",
        Center,
        d_text_field,
        "A multi-line editor with a caret.",
        s_text_field
    ),
    entry!(
        "SearchField",
        "search-field",
        Center,
        d_search_field,
        "Magnifier, editor and clear button.",
        s_search
    ),
    entry!(
        "RichTextEditor",
        "rich-text-editor",
        Center,
        d_rich_text_editor,
        "Editable text with styled runs."
    ),
    entry!(
        "FindReplaceBar",
        "find-replace-bar",
        Center,
        d_find_replace,
        "Find and replace wired to an editor."
    ),
    entry!(
        "ColorPicker",
        "color-picker",
        Top,
        d_color_picker,
        "A full editor: SV plane, hue bar, alpha bar, presets.",
        s_color
    ),
    entry!(
        "DatePicker",
        "date-picker",
        Top,
        d_date_picker,
        "A month calendar with a selected day.",
        s_date
    ),
    entry!(
        "TimePicker",
        "time-picker",
        Top,
        d_time_picker,
        "A clock face for hours and minutes.",
        s_time
    ),
    entry!(
        "FilePicker",
        "file-picker",
        Center,
        d_file_picker,
        "Queues a native open-file dialog.",
        s_file
    ),
];

static CHOICE: &[Entry] = &[
    entry!(
        "PickList",
        "pick-list",
        Top,
        d_pick_list,
        "A dropdown over string options.",
        s_pick_list
    ),
    entry!(
        "Combobox",
        "combobox",
        Top,
        d_combobox,
        "Type to filter, then pick.",
        s_combobox
    ),
    entry!(
        "Select",
        "select",
        Top,
        d_select,
        "The compact index-keyed dropdown.",
        s_select
    ),
    entry!(
        "Menu",
        "menu",
        Top,
        d_menu,
        "A button that opens a floating command list.",
        s_menu
    ),
];

static LAYOUT: &[Entry] = &[
    entry!(
        "Container",
        "container",
        Center,
        d_container,
        "Rows, columns, gaps and padding."
    ),
    entry!(
        "AlignBox",
        "align-box",
        Center,
        d_align_box,
        "Park one child anywhere in a box."
    ),
    entry!(
        "Wrap",
        "wrap",
        Center,
        d_wrap,
        "Children flow onto the next line."
    ),
    entry!(
        "Grid",
        "grid",
        Wide,
        d_grid,
        "A resizable, zoomable, scrolling sheet."
    ),
    entry!(
        "SplitPane",
        "split-pane",
        Wide,
        d_split_pane,
        "Two panes at a fixed ratio."
    ),
    entry!(
        "PaneGrid",
        "pane-grid",
        Wide,
        d_pane_grid,
        "Two panes with a draggable divider."
    ),
    entry!(
        "Scrollable",
        "scrollable",
        Center,
        d_scrollable,
        "A clipped viewport with a drag bar.",
        s_scroll
    ),
    entry!(
        "Accordion",
        "accordion",
        Center,
        d_accordion,
        "Disclosure panels, the first seeded open."
    ),
];

static DATA: &[Entry] = &[
    entry!(
        "VirtualList",
        "virtual-list",
        Center,
        d_virtual_list,
        "100 000 rows, a dozen of them real."
    ),
    entry!(
        "DataGrid",
        "data-grid",
        Center,
        d_data_grid,
        "A sticky header over a virtualized body."
    ),
    entry!(
        "Tree",
        "tree",
        Center,
        d_tree,
        "Expand and collapse a file hierarchy."
    ),
    entry!(
        "Pagination",
        "pagination",
        Center,
        d_pagination,
        "Page through a result set — the page number slices the list.",
        s_pagination
    ),
];

static VISUAL: &[Entry] = &[
    entry!(
        "BarChart",
        "bar-chart",
        Center,
        d_bar_chart,
        "Weekly commits as bars."
    ),
    entry!(
        "LineChart",
        "line-chart",
        Center,
        d_line_chart,
        "A labelled series with axes."
    ),
    entry!(
        "PieChart",
        "pie-chart",
        Center,
        d_pie_chart,
        "Where a frame's time goes."
    ),
    entry!(
        "ProgressBar",
        "progress-bar",
        Center,
        d_progress_bar,
        "Determinate and indeterminate."
    ),
    entry!(
        "Canvas",
        "canvas",
        Center,
        d_canvas,
        "A 60 fps animation you can drive with the sliders."
    ),
];

static NAV: &[Entry] = &[
    entry!(
        "Tabs",
        "tabs",
        Center,
        d_tabs,
        "A tab strip that swaps the body.",
        s_tabs
    ),
    entry!(
        "AppBar",
        "app-bar",
        Center,
        d_app_bar,
        "The screen's header strip — title plus that screen's actions.",
        s_appbar
    ),
    entry!(
        "BottomNav",
        "bottom-nav",
        Center,
        d_bottom_nav,
        "The mobile destination bar.",
        s_bottom_nav
    ),
    entry!(
        "NavigationRail",
        "navigation-rail",
        Center,
        d_navigation_rail,
        "The desktop side rail.",
        s_rail
    ),
    entry!(
        "PullToRefresh",
        "pull-to-refresh",
        Center,
        d_pull_to_refresh,
        "Scroll up past the top to refresh; the feed grows.",
        s_ptr
    ),
];

static OVERLAY: &[Entry] = &[
    entry!("Modal", "modal", Overlay, d_modal, "A dialog over a scrim."),
    entry!(
        "Popover",
        "popover",
        Center,
        d_popover,
        "A light-dismiss anchored panel."
    ),
    entry!(
        "Sheet",
        "sheet",
        Overlay,
        d_sheet,
        "A bottom sheet over the whole window."
    ),
    entry!(
        "Drawer",
        "drawer",
        Overlay,
        d_drawer,
        "A side panel over the whole window."
    ),
];

static GROUPS: &[Group] = &[
    Group {
        title: "Text & display",
        entries: TEXT,
    },
    Group {
        title: "Buttons & input",
        entries: INPUT,
    },
    Group {
        title: "Choice",
        entries: CHOICE,
    },
    Group {
        title: "Layout",
        entries: LAYOUT,
    },
    Group {
        title: "Lists & data",
        entries: DATA,
    },
    Group {
        title: "Charts & progress",
        entries: VISUAL,
    },
    Group {
        title: "Navigation",
        entries: NAV,
    },
    Group {
        title: "Overlays",
        entries: OVERLAY,
    },
];

/// Every group, in dropdown order.
pub fn groups() -> &'static [Group] {
    GROUPS
}

/// Every entry, flattened.
pub fn all() -> impl Iterator<Item = &'static Entry> {
    GROUPS.iter().flat_map(|g| g.entries.iter())
}

/// Look an entry up by [`Entry::name`], falling back to the first one.
pub fn find(name: &str) -> &'static Entry {
    all().find(|e| e.name == name).unwrap_or(&TEXT[0])
}
