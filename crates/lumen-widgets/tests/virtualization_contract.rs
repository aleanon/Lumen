//! A virtualized collection must tell assistive tech how many items it really
//! has — the `set_size` / `position_in_set` contract (`aria-setsize` /
//! `aria-posinset`, AccessKit's `size_of_set` / `position_in_set`).
//!
//! Culling the tree is the whole point of `VirtualList`: a 100 000-row list
//! materializes ~24 nodes, and that is what makes it fast. But a screen reader
//! reads the tree, so without these two properties it announces "list, 24
//! items" — not a degraded answer, a **wrong** one, with rows 25..100 000
//! simply unreachable. These tests pin the numbers that make the culling
//! honest.

use kurbo::Size;
use lumen_core::semantics::{Role, SemanticsNode};
use lumen_widgets::{App, BuildCx, DataGrid, Element, VirtualList};

const N: usize = 100_000;

fn semantics(build: impl Fn(&mut BuildCx) -> Element + 'static) -> std::rc::Rc<SemanticsNode> {
    let mut h = App::new(build).run_headless(Size::new(400.0, 300.0));
    h.pump();
    h.semantics_elided()
}

/// The first node satisfying `f`, anywhere in the tree.
fn find<'a>(n: &'a SemanticsNode, f: &dyn Fn(&SemanticsNode) -> bool) -> Option<&'a SemanticsNode> {
    if f(n) {
        return Some(n);
    }
    n.children.iter().find_map(|c| find(c, f))
}

fn count(n: &SemanticsNode, f: &dyn Fn(&SemanticsNode) -> bool) -> usize {
    (f(n) as usize) + n.children.iter().map(|c| count(c, f)).sum::<usize>()
}

#[test]
fn a_virtual_list_declares_its_real_length() {
    let root = semantics(|cx| {
        VirtualList::new(cx, "vl", N, 24.0, 300.0, |i| {
            lumen_widgets::widgets::text(format!("Row {i}"))
        })
        .into()
    });
    let list = find(&root, &|n| n.role == Role::List).expect("the list is in the tree");

    // The premise: the tree really is culled. If this ever fails the list
    // stopped virtualizing and the perf claim went with it.
    let materialized = count(&root, &|n| n.position_in_set.is_some());
    assert!(
        materialized < 100,
        "a virtual list must NOT materialize {N} rows; got {materialized}"
    );

    // …and the culling is declared, not hidden.
    assert_eq!(
        list.set_size,
        Some(N),
        "the list reports its true length to assistive tech, not the size of \
         its window ({materialized} materialized rows)"
    );
}

#[test]
fn each_materialized_row_carries_its_true_ordinal() {
    let root = semantics(|cx| {
        VirtualList::new(cx, "vl", N, 24.0, 300.0, |i| {
            lumen_widgets::widgets::text(format!("Row {i}"))
        })
        .into()
    });
    let mut ords: Vec<usize> = Vec::new();
    fn walk(n: &SemanticsNode, out: &mut Vec<usize>) {
        if let Some(p) = n.position_in_set {
            out.push(p);
        }
        n.children.iter().for_each(|c| walk(c, out));
    }
    walk(&root, &mut ords);
    ords.sort_unstable();

    assert!(!ords.is_empty(), "rows carry a position");
    // Unscrolled, the window starts at the top: 1-based and contiguous.
    assert_eq!(ords[0], 1, "positions are 1-based, as ARIA specifies");
    assert!(
        ords.windows(2).all(|w| w[1] == w[0] + 1),
        "the window is a contiguous run of the real index space: {:?}",
        &ords[..ords.len().min(8)]
    );
    assert!(
        *ords.last().unwrap() <= N,
        "no row claims an index past the end"
    );
}

#[test]
fn a_data_grid_declares_its_real_row_count() {
    let root = semantics(|cx| {
        DataGrid::new(cx, "dg", &["A", "B"], N, 22.0, 300.0, |r, c| {
            format!("{r}:{c}")
        })
        .into()
    });
    let table = find(&root, &|n| n.role == Role::Table).expect("the table is in the tree");
    assert_eq!(
        table.set_size,
        Some(N),
        "a million-row grid materializes ~20 rows and must say so"
    );
    let rows = count(&root, &|n| n.role == Role::Row);
    assert!(rows < 100, "the grid really is windowed; got {rows} rows");
}

/// Nothing else grows the property: it is meaningless on a node that is not a
/// window onto a larger set, and a spurious `set_size` would mislead an AT
/// exactly as much as a missing one.
#[test]
fn an_ordinary_column_declares_nothing() {
    let root = semantics(|_cx| {
        Element::column(vec![
            lumen_widgets::widgets::text("a"),
            lumen_widgets::widgets::text("b"),
        ])
    });
    assert_eq!(count(&root, &|n| n.set_size.is_some()), 0);
    assert_eq!(count(&root, &|n| n.position_in_set.is_some()), 0);
}
