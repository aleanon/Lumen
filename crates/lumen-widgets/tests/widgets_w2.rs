//! W.2 (docs/plan-remediation-2026-07.md): Combobox, ColorPicker, Skeleton,
//! Avatar, Pagination, RangeSlider, FilePicker, LineChart/PieChart,
//! AlignBox — headless behavior per the writing-widgets pattern.

use kurbo::{Point, Size};
use lumen_core::events::{Event, PointerEvent, TextInputEvent};
use lumen_core::state::Signal;
use lumen_widgets::{
    center, col, widgets, AlignBox, App, Avatar, BuildCx, ColorPicker, Combobox, FilePicker,
    LineChart, Pagination, PieChart, PieSlice, RangeSlider, Skeleton,
};

fn find_label(n: &lumen_core::semantics::SemanticsNode, label: &str) -> Option<kurbo::Rect> {
    if n.label == label {
        return Some(n.bounds);
    }
    n.children.iter().find_map(|c| find_label(c, label))
}

fn click_at(h: &mut lumen_widgets::Headless, p: Point) {
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}

#[test]
fn combobox_filters_and_selects() {
    let mut h = App::new(|cx: &mut BuildCx| {
        col![Combobox::new(cx, "fruit", ["Apple", "Banana", "Cherry"]).id("cb")]
    })
    .run_headless(Size::new(400.0, 300.0));
    h.pump();

    // Click the input → opens with all options.
    let p = center(h.node_bounds_by_id("fruit-input").unwrap());
    click_at(&mut h, p);
    let sem = h.semantics_json().to_string();
    assert!(sem.contains("Apple") && sem.contains("Cherry"), "{sem}");

    // Typing filters.
    h.inject(Event::TextInput(TextInputEvent { text: "ban".into() }));
    h.pump();
    let root = h.semantics_doc().root.elided();
    assert!(find_label(&root, "Banana").is_some());
    let sem = h.semantics_json().to_string();
    assert!(!sem.contains("Cherry"), "filtered out: {sem}");

    // Picking stores the selection and closes.
    let b = find_label(&h.semantics_doc().root.elided(), "Banana").unwrap();
    click_at(&mut h, center(b));
    let sel: Signal<String> = h.runtime().signal("fruit.selected", String::new);
    assert_eq!(sel.get(h.runtime()), "Banana");
    h.assert_view_coherent();
}

#[test]
fn color_picker_selects_a_preset() {
    let mut h = App::new(|cx: &mut BuildCx| col![ColorPicker::new(cx, "accent").id("cp")])
        .run_headless(Size::new(400.0, 300.0));
    h.pump();

    let root = h.semantics_doc().root.elided();
    let trig = find_label(&root, "color #1a73e8").expect("trigger labelled");
    click_at(&mut h, center(trig));
    let root = h.semantics_doc().root.elided();
    let cell = find_label(&root, "#d32f2f").expect("palette open");
    click_at(&mut h, center(cell));

    let v: Signal<String> = h.runtime().signal("accent", String::new);
    assert_eq!(v.get(h.runtime()), "#d32f2f");
    assert!(
        find_label(&h.semantics_doc().root.elided(), "#188a42").is_none(),
        "panel closed after pick"
    );
    h.assert_view_coherent();
}

#[test]
fn pagination_clamps_and_pages() {
    let mut h = App::new(|cx: &mut BuildCx| col![Pagination::new(cx, "pg", 3).id("pg")])
        .run_headless(Size::new(500.0, 200.0));
    h.pump();

    let p2 = center(h.node_bounds_by_id("pg-p2").unwrap());
    click_at(&mut h, p2);
    let page: Signal<i64> = h.runtime().signal("pg.page", || 1);
    assert_eq!(page.get(h.runtime()), 2);

    let next = center(h.node_bounds_by_id("pg-next").unwrap());
    click_at(&mut h, next);
    assert_eq!(page.get(h.runtime()), 3);
    let next2 = center(h.node_bounds_by_id("pg-next").unwrap());
    click_at(&mut h, next2);
    assert_eq!(page.get(h.runtime()), 3, "clamped at the last page");
    h.assert_view_coherent();
}

#[test]
fn range_slider_drags_the_nearer_thumb() {
    let mut h = App::new(|cx: &mut BuildCx| col![RangeSlider::new(cx, "r", 0.0, 100.0).id("rs")])
        .run_headless(Size::new(400.0, 200.0));
    h.pump();

    let b = h.node_bounds_by_id("rs").unwrap();
    // Press near the left end and drag to 30%: the LO thumb follows.
    let start = Point::new(b.x0 + 2.0, b.center().y);
    let end = Point::new(b.x0 + b.width() * 0.3, b.center().y);
    h.inject(Event::PointerDown(PointerEvent::at(start)));
    h.inject(Event::PointerMove(PointerEvent::at(end)));
    h.inject(Event::PointerUp(PointerEvent::at(end)));
    h.pump();
    let lo: Signal<f64> = h.runtime().signal("r.lo", || 0.0);
    let hi: Signal<f64> = h.runtime().signal("r.hi", || 100.0);
    assert!(
        (lo.get(h.runtime()) - 30.0).abs() < 5.0,
        "lo followed: {}",
        lo.get(h.runtime())
    );
    assert_eq!(hi.get(h.runtime()), 100.0, "hi untouched");
    h.assert_view_coherent();
}

#[test]
fn file_picker_queues_a_system_request() {
    let mut h = App::new(|cx: &mut BuildCx| {
        col![FilePicker::new(cx, "doc", "Open…", ["png", "jpg"]).id("fp")]
    })
    .run_headless(Size::new(400.0, 200.0));
    h.pump();
    assert!(h.system_requests().is_empty());

    let fp = center(h.node_bounds_by_id("fp").unwrap());
    click_at(&mut h, fp);
    let reqs = h.system_requests();
    assert_eq!(reqs.len(), 1, "{reqs:?}");
    assert!(
        matches!(&reqs[0], lumen_widgets::system::SystemRequest::OpenFile { filters }
            if filters == &vec!["png".to_string(), "jpg".to_string()]),
        "{reqs:?}"
    );
    h.assert_view_coherent();
}

#[test]
fn passive_widgets_render_with_semantics() {
    let mut h = App::new(|cx: &mut BuildCx| {
        col![
            Skeleton::new(cx, 200.0, 16.0).id("sk"),
            Avatar::new("Ada Lovelace", 40.0).id("av"),
            AlignBox::center(widgets::text("centered").id("mid")).id("al"),
            LineChart::element(
                vec![1.0, 3.0, 2.0],
                vec!["a".into(), "b".into(), "c".into()]
            )
            .id("lc"),
            PieChart::element(vec![
                PieSlice {
                    label: "rust".into(),
                    value: 3.0,
                    color: lumen_core::Color::srgb8(0x1a, 0x73, 0xe8, 0xff),
                },
                PieSlice {
                    label: "other".into(),
                    value: 1.0,
                    color: lumen_core::Color::srgb8(0xd3, 0x2f, 0x2f, 0xff),
                },
            ])
            .id("pc")
        ]
    })
    .run_headless(Size::new(500.0, 600.0));
    h.pump();

    let sem = h.semantics_json().to_string();
    assert!(sem.contains("Ada Lovelace"), "{sem}");
    assert!(sem.contains("Line chart, 3 points"), "{sem}");
    assert!(sem.contains("rust 75%"), "{sem}");
    assert!(h.is_time_driven(), "skeleton pulses");

    // Avatar initials render (AL over the hashed background).
    assert!(sem.contains("AL"), "{sem}");
    // AlignBox centers its child horizontally within the window.
    let mid = h.node_bounds_by_id("mid").unwrap();
    assert!(
        mid.center().x > 100.0,
        "centered child sits away from the left edge: {mid:?}"
    );
    h.assert_view_coherent();
}
