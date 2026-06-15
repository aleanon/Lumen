//! T1.6 acceptance: VirtualList windowing (1M items, only visible+overscan
//! materialized) + scroll goldens, and test triples for the new widgets.

use kurbo::{Point, Size, Vec2};
use lumen_core::events::{Event, Modifiers, PointerEvent, WheelEvent};
use lumen_core::semantics::{Role, SemanticsNode, State};
use lumen_render::RgbaImage;
use lumen_widgets::{center, widgets, widgets_m1, App, BuildCx, Element, Headless};
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
fn find_role(n: &SemanticsNode, role: Role) -> Vec<&SemanticsNode> {
    let mut out = Vec::new();
    if n.role == role {
        out.push(n);
    }
    for c in &n.children {
        out.extend(find_role(c, role));
    }
    out
}
fn click_at(h: &mut Headless, p: Point) {
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
}
fn check_golden(name: &str, img: &RgbaImage) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/cpu")
        .join(format!("{name}.png"));
    if std::env::var_os("LUMEN_UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, img.to_png()).unwrap();
        return;
    }
    let bytes = std::fs::read(&path).unwrap_or_else(|_| panic!("missing golden {path:?}"));
    let expected = RgbaImage::from_png(&bytes).unwrap();
    if img != &expected {
        std::fs::write(path.with_extension("actual.png"), img.to_png()).unwrap();
        panic!("golden mismatch {name}: {} px", img.diff_count(&expected));
    }
}

#[test]
fn virtual_list_materializes_only_visible() {
    let mut h = run(180.0, 200.0, |cx| {
        widgets_m1::virtual_list(cx, "vl", 1_000_000, 20.0, 200.0, |i| {
            widgets::text(format!("row {i}"))
        })
    });
    let stats = h.pump();
    // 1M items, but only ~viewport/item_height + overscan nodes exist.
    assert!(
        stats.node_count < 50,
        "materialized {} nodes for 1M items",
        stats.node_count
    );
    let s = sem(&h);
    assert_eq!(s.role, Role::List);
    assert_eq!(s.scroll.unwrap().max_y, 1_000_000.0 * 20.0 - 200.0);
    check_golden("vlist_top", &h.screenshot());
}

#[test]
fn virtual_list_scroll_changes_window() {
    let mut h = run(180.0, 200.0, |cx| {
        widgets_m1::virtual_list(cx, "vl", 1_000_000, 20.0, 200.0, |i| {
            widgets::text(format!("row {i}"))
        })
    });
    h.pump();
    let labels = |h: &Headless| -> Vec<String> {
        find_role(&sem(h), Role::Text)
            .iter()
            .map(|n| n.label.clone())
            .collect()
    };
    // Row 0 visible at top; row 500 is not.
    assert!(labels(&h).contains(&"row 0".to_string()));
    // Scroll down 10,000px -> window centers near row 500.
    h.inject(Event::Wheel(WheelEvent {
        pos: Point::new(50.0, 50.0),
        delta: Vec2::new(0.0, 10_000.0),
        modifiers: Modifiers::empty(),
    }));
    h.pump();
    assert_eq!(sem(&h).scroll.unwrap().y, 10_000.0);
    assert!(labels(&h).contains(&"row 500".to_string()));
    assert!(!labels(&h).contains(&"row 0".to_string()));
    check_golden("vlist_scrolled", &h.screenshot());
}

#[test]
fn switch_toggles() {
    let mut h = run(160.0, 40.0, |cx| {
        widgets_m1::switch(cx, "sw", "Wifi").id("sw")
    });
    assert_eq!(sem(&h).role, Role::Switch);
    assert!(sem(&h).states.contains(&State::Unchecked));
    check_golden("switch_off", &h.screenshot());
    let b = sem(&h).bounds;
    click_at(&mut h, center(b));
    assert!(sem(&h).states.contains(&State::Checked));
}

#[test]
fn tabs_select() {
    let mut h = run(240.0, 40.0, |cx| {
        widgets_m1::tabs(cx, "tabs", &["One", "Two", "Three"])
    });
    assert_eq!(sem(&h).role, Role::TabList);
    let s = sem(&h);
    let tabs = find_role(&s, Role::Tab);
    assert_eq!(tabs.len(), 3);
    assert!(tabs[0].states.contains(&State::Selected));
    let third = tabs[2].bounds;
    drop(s);
    check_golden("tabs", &h.screenshot());
    // click the third tab
    click_at(&mut h, center(third));
    let s = sem(&h);
    let tabs = find_role(&s, Role::Tab);
    assert!(tabs[2].states.contains(&State::Selected));
    assert!(!tabs[0].states.contains(&State::Selected));
}

#[test]
fn stepper_inc_dec() {
    let mut h = run(160.0, 40.0, |cx| widgets_m1::stepper(cx, "st", 0, 10));
    assert_eq!(by_id(&sem(&h), "value").unwrap().label, "0");
    check_golden("stepper", &h.screenshot());
    let inc = by_id(&sem(&h), "inc").unwrap().bounds;
    click_at(&mut h, center(inc));
    click_at(&mut h, center(inc));
    assert_eq!(by_id(&sem(&h), "value").unwrap().label, "2");
    let dec = by_id(&sem(&h), "dec").unwrap().bounds;
    click_at(&mut h, center(dec));
    assert_eq!(by_id(&sem(&h), "value").unwrap().label, "1");
}

#[test]
fn structural_widgets_render() {
    let mut h = run(200.0, 80.0, |_| {
        widgets::row(vec![
            widgets_m1::icon("home"),
            widgets_m1::spacer(),
            widgets_m1::divider(),
            widgets_m1::padding(8.0, widgets::text("x")),
        ])
    });
    h.pump();
    check_golden("structural", &h.screenshot());
}
