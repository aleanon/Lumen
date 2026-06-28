//! T0.10 acceptance: per-widget golden + semantic-tree + interaction tests.

use kurbo::{Point, Size, Vec2};
use lumen_core::events::{
    Event, Key, KeyEvent, Modifiers, NamedKey, PointerEvent, TextInputEvent, WheelEvent,
};
use lumen_core::semantics::{Role, SemanticsNode, State};
use lumen_render::RgbaImage;
use lumen_widgets::{center, widgets, App, BuildCx, Element, Headless};
use std::path::PathBuf;

fn run(w: f64, h: f64, build: impl Fn(&mut BuildCx) -> Element + 'static) -> Headless {
    App::new(build).run_headless(Size::new(w, h))
}

fn sem(h: &Headless) -> SemanticsNode {
    h.semantics_doc().root.elided()
}

fn by_id<'a>(n: &'a SemanticsNode, id: &str) -> Option<&'a SemanticsNode> {
    if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
        return Some(n);
    }
    n.children.iter().find_map(|c| by_id(c, id))
}

fn click_at(h: &mut Headless, p: Point) {
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}

fn key(named: NamedKey) -> Event {
    Event::KeyDown(KeyEvent {
        key: Key::Named(named),
        modifiers: Modifiers::empty(),
        repeat: false,
    })
}

// --- golden helper ----------------------------------------------------------

fn check_golden(name: &str, img: &RgbaImage) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/cpu")
        .join(format!("{name}.png"));
    if std::env::var_os("LUMEN_UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, img.to_png()).unwrap();
        return;
    }
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|_| panic!("missing golden {path:?}; run with LUMEN_UPDATE_GOLDENS=1"));
    let expected = RgbaImage::from_png(&bytes).unwrap();
    if img != &expected {
        let actual = path.with_extension("actual.png");
        std::fs::write(&actual, img.to_png()).unwrap();
        panic!(
            "golden mismatch for {name}: {} px differ",
            img.diff_count(&expected)
        );
    }
}

// --- static widgets: golden + semantics -------------------------------------

#[test]
fn w_text() {
    let mut h = run(160.0, 40.0, |_| widgets::text("Hello"));
    assert_eq!(sem(&h).role, Role::Text);
    check_golden("w_text", &h.screenshot());
}

#[test]
fn w_border() {
    // A bordered, rounded box (Element::border) on a small canvas, inset so the
    // centered stroke isn't clipped at the edge.
    let mut h = run(80.0, 60.0, |_| Element {
        background: Some(lumen_core::Color::srgb8(0xf0, 0xf2, 0xf6, 0xff)),
        border: Some(lumen_render::Border {
            width: 3.0,
            color: lumen_core::Color::srgb8(0x1a, 0x73, 0xe8, 0xff),
        }),
        corner_radius: 10.0,
        style: lumen_layout::LayoutStyle {
            margin: lumen_layout::Edges::all(lumen_layout::Dim::px(8.0)),
            width: lumen_layout::Dim::px(60.0),
            height: lumen_layout::Dim::px(40.0),
            ..Default::default()
        },
        ..Default::default()
    });
    check_golden("w_border", &h.screenshot());
}

#[test]
fn w_image() {
    let img = RgbaImage::from_raw(
        2,
        2,
        vec![
            220, 40, 40, 255, 250, 210, 60, 255, 250, 210, 60, 255, 220, 40, 40, 255,
        ],
    );
    let mut h = run(80.0, 80.0, move |_| {
        let mut e = widgets::image(img.clone());
        e.style.width = lumen_layout::Dim::px(64.0);
        e.style.height = lumen_layout::Dim::px(64.0);
        e
    });
    assert_eq!(sem(&h).role, Role::Image);
    check_golden("w_image", &h.screenshot());
}

#[test]
fn w_row_and_column() {
    let mut h = run(220.0, 60.0, |_| {
        widgets::row(vec![
            widgets::button("A", |_| {}).id("a"),
            widgets::button("B", |_| {}).id("b"),
        ])
    });
    let s = sem(&h);
    assert!(by_id(&s, "a").is_some() && by_id(&s, "b").is_some());
    check_golden("w_row", &h.screenshot());

    let mut h = run(120.0, 120.0, |_| {
        widgets::column(vec![widgets::text("top"), widgets::text("bottom")])
    });
    check_golden("w_column", &h.screenshot());
}

#[test]
fn w_stack_overlays() {
    let mut h = run(120.0, 120.0, |_| {
        let a = Element::default()
            .background(lumen_core::Color::srgb8(0x33, 0x88, 0xff, 0xff))
            .style(lumen_layout::LayoutStyle {
                width: lumen_layout::Dim::px(80.0),
                height: lumen_layout::Dim::px(80.0),
                ..Default::default()
            });
        let b = Element::default()
            .background(lumen_core::Color::srgb8(0xff, 0x88, 0x33, 0xff))
            .style(lumen_layout::LayoutStyle {
                width: lumen_layout::Dim::px(40.0),
                height: lumen_layout::Dim::px(40.0),
                ..Default::default()
            });
        widgets::stack(vec![a, b])
    });
    check_golden("w_stack", &h.screenshot());
}

// --- interactive widgets: golden + semantics + interaction ------------------

#[test]
fn w_button_click_increments() {
    let mut h = run(120.0, 80.0, |cx| {
        let n = cx.signal("n", || 0i32);
        let v = n.get(cx.runtime());
        widgets::column(vec![
            widgets::text(format!("{v}")).id("n"),
            widgets::button("inc", move |rt| n.update(rt, |x| *x += 1)).id("b"),
        ])
    });
    check_golden("w_button", &h.screenshot());
    assert_eq!(by_id(&sem(&h), "n").unwrap().label, "0");
    let b = by_id(&sem(&h), "b").unwrap().bounds;
    click_at(&mut h, center(b));
    assert_eq!(by_id(&sem(&h), "n").unwrap().label, "1");
}

#[test]
fn w_checkbox_space_toggles() {
    let mut h = run(160.0, 40.0, |cx| {
        widgets::checkbox(cx, "cb", "Accept").id("cb")
    });
    assert_eq!(sem(&h).role, Role::Checkbox);
    assert!(sem(&h).states.contains(&State::Unchecked));
    check_golden("w_checkbox", &h.screenshot());
    // Tab to focus, Space to toggle.
    h.inject(key(NamedKey::Tab));
    h.pump();
    h.inject(key(NamedKey::Space));
    h.pump();
    assert!(
        sem(&h).states.contains(&State::Checked),
        "space should check it"
    );
    h.inject(key(NamedKey::Space));
    h.pump();
    assert!(
        sem(&h).states.contains(&State::Unchecked),
        "space should uncheck it"
    );
}

#[test]
fn w_slider_drag_sets_value() {
    let mut h = run(220.0, 40.0, |cx| widgets::slider(cx, "s", 0.0, 100.0));
    assert_eq!(sem(&h).role, Role::Slider);
    assert_eq!(sem(&h).value.as_deref(), Some("0"));
    check_golden("w_slider", &h.screenshot());
    // Press at the slider's horizontal center -> ~50%.
    let b = sem(&h).bounds;
    click_at(
        &mut h,
        Point::new(b.x0 + b.width() / 2.0, b.y0 + b.height() / 2.0),
    );
    assert_eq!(sem(&h).value.as_deref(), Some("50"), "center press -> 50");
}

#[test]
fn w_scroll_wheel_updates_semantics() {
    let mut h = run(160.0, 100.0, |cx| {
        let lines: Vec<Element> = (0..10)
            .map(|i| widgets::text(format!("line {i}")))
            .collect();
        widgets::scroll(cx, "sc", 100.0, 300.0, lines)
    });
    assert_eq!(sem(&h).role, Role::ScrollArea);
    assert_eq!(sem(&h).scroll.map(|s| s.y), Some(0.0));
    check_golden("w_scroll", &h.screenshot());
    h.inject(Event::Wheel(WheelEvent {
        pos: Point::new(40.0, 40.0),
        delta: Vec2::new(0.0, 40.0),
        modifiers: Modifiers::empty(),
    }));
    h.pump();
    assert_eq!(
        sem(&h).scroll.map(|s| s.y),
        Some(40.0),
        "wheel scrolls content"
    );
}

#[test]
fn w_text_field_accepts_input() {
    let mut h = run(200.0, 40.0, |cx| {
        widgets::text_field_basic(cx, "tf", "").id("tf")
    });
    assert_eq!(sem(&h).role, Role::TextInput);
    check_golden("w_text_field", &h.screenshot());
    // Focus by clicking, then commit text.
    let b = sem(&h).bounds;
    click_at(&mut h, center(b));
    h.inject(Event::TextInput(TextInputEvent {
        text: "Hi".to_string(),
    }));
    h.pump();
    assert_eq!(sem(&h).value.as_deref(), Some("Hi"));
}
