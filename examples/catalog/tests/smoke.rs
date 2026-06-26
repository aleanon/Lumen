//! The 1000-row virtualized catalog: rows fetch async (per-row resource), show a
//! spinner while loading, then render their record; navigable by wheel, a
//! draggable scrollbar, and the keyboard. Driven deterministically by the
//! ManualSpawner.

use lumen_core::events::{Event, KeyEvent, NamedKey, PointerEvent, WheelEvent};
use lumen_core::events::{Key, Modifiers};
use lumen_core::geometry::{Point, Rect, Size, Vec2};
use lumen_core::semantics::SemanticsNode;
use lumen_core::tasks::ManualSpawner;
use lumen_widgets::{Headless, TinySkia};

type App = Headless<TinySkia, ManualSpawner>;

fn app() -> App {
    catalog::main_app()
        .with_executor(ManualSpawner::new())
        .run_headless(Size::new(520.0, 640.0))
}

fn settle(a: &mut App) {
    a.executor().run_pending();
    a.pump();
}

fn node_count(n: &SemanticsNode) -> usize {
    1 + n.children.iter().map(node_count).sum::<usize>()
}

fn header(a: &App) -> String {
    fn find(n: &SemanticsNode, out: &mut Option<String>) {
        if n.label.starts_with("Rows ") {
            *out = Some(n.label.clone());
        }
        n.children.iter().for_each(|c| find(c, out));
    }
    let mut s = None;
    find(&a.semantics_doc().root, &mut s);
    s.unwrap_or_default()
}

fn bounds(a: &App, id: &str) -> Rect {
    fn f(n: &SemanticsNode, id: &str) -> Option<Rect> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.bounds);
        }
        n.children.iter().find_map(|c| f(c, id))
    }
    f(&a.semantics_doc().root, id).unwrap_or_else(|| panic!("no node {id}"))
}

fn pointer(a: &mut App, ev: impl Fn(PointerEvent) -> Event, p: Point) {
    a.inject(ev(PointerEvent::at(p)));
}

fn key(a: &mut App, k: NamedKey) {
    a.inject(Event::KeyDown(KeyEvent {
        key: Key::Named(k),
        modifiers: Modifiers::empty(),
        repeat: false,
    }));
    a.pump();
}

#[test]
fn virtualized_only_builds_visible_rows() {
    let mut a = app();
    settle(&mut a);
    let n = node_count(&a.semantics_doc().root);
    assert!(
        n < 200,
        "only the visible window is in the tree (got {n} nodes)"
    );
    let records = a.semantics_json().to_string().matches("GAIA DR3").count();
    assert_eq!(
        records, 11,
        "the visible window (+1 smooth-scroll row) of records"
    );
}

#[test]
fn rows_load_from_spinner_to_record() {
    let mut a = app();
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(
        t.contains("acquiring signal"),
        "rows start loading (spinner)"
    );
    assert!(!t.contains("GAIA DR3"), "no records yet");
    settle(&mut a);
    assert!(
        a.semantics_json().to_string().contains("GAIA DR3"),
        "records render"
    );
}

#[test]
fn wheel_scrolls_the_window() {
    let mut a = app();
    settle(&mut a);
    assert_eq!(header(&a), "Rows 1–10 of 1000");
    let vp = bounds(&a, "viewport");
    let c = Point::new((vp.x0 + vp.x1) / 2.0, (vp.y0 + vp.y1) / 2.0);
    a.inject(Event::Wheel(WheelEvent {
        pos: c,
        delta: Vec2::new(0.0, 46.0 * 5.0), // 5 rows down
        modifiers: Modifiers::empty(),
    }));
    a.pump();
    assert_eq!(header(&a), "Rows 6–15 of 1000", "window advanced 5 rows");
}

#[test]
fn keyboard_navigates_when_focused() {
    let mut a = app();
    settle(&mut a);
    // Focus the viewport by clicking it.
    let vp = bounds(&a, "viewport");
    let c = Point::new((vp.x0 + vp.x1) / 2.0, (vp.y0 + vp.y1) / 2.0);
    pointer(&mut a, Event::PointerDown, c);
    pointer(&mut a, Event::PointerUp, c);
    a.pump();

    key(&mut a, NamedKey::End);
    assert_eq!(
        header(&a),
        "Rows 991–1000 of 1000",
        "End jumps to the bottom"
    );
    key(&mut a, NamedKey::Home);
    assert_eq!(header(&a), "Rows 1–10 of 1000", "Home jumps to the top");
    key(&mut a, NamedKey::PageDown);
    assert_eq!(
        header(&a),
        "Rows 11–20 of 1000",
        "PageDown advances one page"
    );
    key(&mut a, NamedKey::ArrowDown);
    assert_eq!(
        header(&a),
        "Rows 12–21 of 1000",
        "ArrowDown advances one row"
    );
}

#[test]
fn scrollbar_drag_jumps_anywhere() {
    let mut a = app();
    settle(&mut a);
    let sb = bounds(&a, "scrollbar");
    // Press at the vertical middle of the scrollbar → jump to ~the middle.
    let mid = Point::new((sb.x0 + sb.x1) / 2.0, (sb.y0 + sb.y1) / 2.0);
    pointer(&mut a, Event::PointerDown, mid);
    a.pump();
    let h = header(&a);
    assert!(
        h.contains("of 1000") && !h.starts_with("Rows 1–"),
        "jumped off the top: {h}"
    );
    let first: usize = h
        .trim_start_matches("Rows ")
        .split('–')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        (460..=540).contains(&first),
        "drag-to-middle landed near row 500 (got {first})"
    );

    // Drag past the bottom of the track → clamps to the end, and (the bug we
    // fixed) no panic as rows — hence node indices — change under the drag.
    pointer(&mut a, Event::PointerMove, Point::new(mid.x, sb.y1 + 20.0));
    a.pump();
    assert_eq!(header(&a), "Rows 991–1000 of 1000", "dragged to the end");
    pointer(&mut a, Event::PointerUp, mid);
    a.pump();
}
