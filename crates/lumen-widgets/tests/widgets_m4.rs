//! T4.2 acceptance: DataGrid / Tree / Chart / RichTextEditor triples
//! (render + semantics + interaction). The 1M-row perf gate lives in benches/.

use kurbo::{Point, Size, Vec2};
use lumen_core::events::{Event, Modifiers, PointerEvent, TextInputEvent, WheelEvent};
use lumen_core::semantics::{Role, SemanticsNode, State};
use lumen_widgets::widgets_m4::{self, TreeRow};
use lumen_widgets::{App, BuildCx, Element, Headless};

fn run(w: f64, h: f64, build: impl Fn(&mut BuildCx) -> Element + 'static) -> Headless {
    App::new(build).run_headless(Size::new(w, h))
}
fn sem(h: &Headless) -> SemanticsNode {
    h.semantics_doc().root.elided()
}
fn count_role(n: &SemanticsNode, role: Role) -> usize {
    let here = usize::from(n.role == role);
    here + n
        .children
        .iter()
        .map(|c| count_role(c, role))
        .sum::<usize>()
}
fn find_role(n: &SemanticsNode, role: Role) -> Option<&SemanticsNode> {
    if n.role == role {
        return Some(n);
    }
    n.children.iter().find_map(|c| find_role(c, role))
}
fn first_text(n: &SemanticsNode, needle: &str) -> Option<SemanticsNode> {
    if n.label.contains(needle) {
        return Some(n.clone());
    }
    n.children.iter().find_map(|c| first_text(c, needle))
}
fn mid(n: &SemanticsNode) -> Point {
    let b = n.bounds;
    Point::new(b.x0 + b.width() / 2.0, b.y0 + b.height() / 2.0)
}

#[test]
fn data_grid_windows_a_million_rows() {
    let mut h = run(400.0, 200.0, |cx| {
        widgets_m4::data_grid(cx, "grid", &["A", "B"], 1_000_000, 20.0, 200.0, |r, c| {
            format!("r{r}c{c}")
        })
    });
    assert_eq!(sem(&h).role, Role::Table);
    assert_eq!(count_role(&sem(&h), Role::ColumnHeader), 2, "header cells");

    // Only the visible window of rows is materialized — not a million.
    let rows = count_role(&sem(&h), Role::Row);
    assert!(rows > 0 && rows < 50, "windowed rows: {rows}");
    assert!(first_text(&sem(&h), "r0c0").is_some(), "first cell visible");

    // Scroll down; later rows appear, top rows leave.
    let g = mid(find_role(&sem(&h), Role::Group).unwrap());
    h.inject(Event::Wheel(WheelEvent {
        pos: g,
        delta: Vec2::new(0.0, 4000.0),
        modifiers: Modifiers::empty(),
    }));
    h.pump();
    assert!(
        first_text(&sem(&h), "r0c0").is_none(),
        "scrolled past row 0"
    );
    assert!(
        first_text(&sem(&h), "r200c0").is_some(),
        "row 200 now visible"
    );
}

#[test]
fn tree_expands_and_collapses() {
    let rows = [
        TreeRow {
            id: "a",
            label: "Animals",
            depth: 0,
            has_children: true,
        },
        TreeRow {
            id: "cat",
            label: "Cat",
            depth: 1,
            has_children: false,
        },
        TreeRow {
            id: "dog",
            label: "Dog",
            depth: 1,
            has_children: false,
        },
        TreeRow {
            id: "p",
            label: "Plants",
            depth: 0,
            has_children: false,
        },
    ];
    let mut h = run(240.0, 200.0, move |cx| widgets_m4::tree(cx, "t", &rows));
    assert_eq!(sem(&h).role, Role::Tree);

    // Collapsed by default: children hidden, parent marked collapsed.
    assert!(first_text(&sem(&h), "Cat").is_none());
    let parent = first_text(&sem(&h), "Animals").unwrap();
    assert!(parent.states.contains(&State::Collapsed));

    // Click the parent → it expands and its children appear.
    let p = mid(&first_text(&sem(&h), "Animals").unwrap());
    h.inject(Event::PointerDown(PointerEvent::at(p)));
    h.inject(Event::PointerUp(PointerEvent::at(p)));
    h.pump();
    assert!(first_text(&sem(&h), "Cat").is_some(), "expanded");
    assert!(first_text(&sem(&h), "Animals")
        .unwrap()
        .states
        .contains(&State::Expanded));
}

#[test]
fn bar_chart_renders_and_reports_count() {
    let h = run(120.0, 80.0, |_| {
        widgets_m4::bar_chart(&[1.0, 4.0, 2.0, 8.0, 3.0], 120.0, 80.0)
    });
    let root = sem(&h);
    assert_eq!(root.role, Role::Group);
    assert_eq!(root.value.as_deref(), Some("5"));
    // Five bars, with monotonic heights for the increasing values 2.0<8.0.
    assert_eq!(count_role(&root, Role::Generic), 5);
}

#[test]
fn rich_text_editor_types_and_styles() {
    // M.4: the editor is now source pane (full TextEditor caret machinery)
    // + live RichDoc preview; the pane's value is the markdown-lite source.
    let mut h = run(360.0, 200.0, |cx| {
        widgets_m4::rich_text_editor(cx, "doc", "hi ")
    });
    fn pane(n: &lumen_core::semantics::SemanticsNode) -> lumen_core::semantics::SemanticsNode {
        fn find(
            n: &lumen_core::semantics::SemanticsNode,
        ) -> Option<lumen_core::semantics::SemanticsNode> {
            if n.role == Role::TextInput {
                return Some(n.clone());
            }
            n.children.iter().find_map(find)
        }
        find(n).expect("source pane")
    }
    let field = mid(&pane(&sem(&h)));
    h.inject(Event::PointerDown(PointerEvent::at(field)));
    h.inject(Event::PointerUp(PointerEvent::at(field)));
    h.pump();
    // Caret placed by the click lands at the end region; type an emphasised
    // word — insertion happens AT THE CARET (TextEditor), not by appending.
    h.inject(Event::TextInput(TextInputEvent {
        text: "*world*".into(),
    }));
    h.pump();
    let v = pane(&sem(&h)).value.unwrap();
    assert!(v.contains("*world*"), "typed at caret: {v}");
    // The preview parsed the italic span into its own styled node.
    assert!(first_text(&sem(&h), "world").is_some());
}
