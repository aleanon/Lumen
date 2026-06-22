//! The 1000-row virtualized catalog: rows fetch async (per-row resource), show a
//! spinner while loading, then render their record. Driven deterministically by
//! the ManualSpawner.

use lumen_core::events::{Event, WheelEvent};
use lumen_core::geometry::{Point, Size, Vec2};
use lumen_core::semantics::SemanticsNode;
use lumen_core::tasks::ManualSpawner;
use lumen_widgets::{CpuRenderer, Headless};

type App = Headless<CpuRenderer, ManualSpawner>;

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

fn count_matches(json: &str, needle: &str) -> usize {
    json.matches(needle).count()
}

#[test]
fn virtualized_only_builds_visible_rows() {
    let mut a = app();
    settle(&mut a);
    // 1000 rows conceptually, but only the visible window (~11) is built.
    let n = node_count(&a.semantics_doc().root);
    assert!(
        n < 200,
        "only the visible window is in the tree (got {n} nodes)"
    );
    let records = count_matches(&a.semantics_json().to_string(), "GAIA DR3");
    assert_eq!(
        records, 11,
        "exactly the visible window of records is built"
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
    let t = a.semantics_json().to_string();
    assert!(
        t.contains("GAIA DR3"),
        "records render after fetch resolves"
    );
    assert!(
        t.contains("all in view loaded"),
        "header reflects loaded state"
    );
}

#[test]
fn scrolling_loads_new_rows() {
    let mut a = app();
    settle(&mut a);
    assert!(a.semantics_json().to_string().contains("Rows 1–11"));

    fn viewport(a: &App) -> lumen_core::geometry::Rect {
        fn f(n: &SemanticsNode) -> Option<lumen_core::geometry::Rect> {
            if n.id.as_ref().map(|i| i.as_str()) == Some("viewport") {
                return Some(n.bounds);
            }
            n.children.iter().find_map(f)
        }
        f(&a.semantics_doc().root).expect("viewport")
    }
    let b = viewport(&a);
    a.inject(Event::Wheel(WheelEvent {
        pos: Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0),
        delta: Vec2::new(0.0, 46.0 * 5.0), // scroll ~5 rows
        modifiers: Default::default(),
    }));
    a.pump();
    let t = a.semantics_json().to_string();
    assert!(t.contains("Rows 6–16"), "window advanced after scroll");
    assert!(t.contains("acquiring signal"), "new rows fetch on scroll");

    settle(&mut a);
    assert!(
        a.semantics_json().to_string().contains("GAIA DR3"),
        "new rows resolve"
    );
}
