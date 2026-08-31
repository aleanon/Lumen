//! A virtualized collection must be operable without a mouse.
//!
//! This is the other half of the virtualization contract. `set_size` tells an
//! AT the list holds 100 000 rows (see `virtualization_contract.rs`); it is a
//! description of an *inaccessible* control unless something can also move the
//! window. Both virtualized widgets shipped wheel-only — no `focusable`, no
//! `on_key` — so a keyboard user could not move them at all, and a screen
//! reader could reach only the ~24 rows in the opening window. Virtualizing a
//! list therefore *removed* access, since a plain column at least puts every
//! row in the tree.
//!
//! W3 fixed exactly this for `Scrollable` and did not propagate. `Scrollable`
//! is kept here as a control: if it ever breaks, these tests are measuring the
//! harness rather than the widgets.

use kurbo::Size;
use lumen_core::events::{Event, Key, KeyEvent, Modifiers, NamedKey};
use lumen_core::state::Signal;
use lumen_widgets::{widgets, App, BuildCx, DataGrid, Element, Headless, Scrollable, VirtualList};

fn key(h: &mut Headless, named: NamedKey) {
    h.inject(Event::KeyDown(KeyEvent {
        key: Key::Named(named),
        modifiers: Modifiers::empty(),
        repeat: false,
    }));
    h.pump();
}

/// **No `.id()` anywhere in these tests, deliberately.** Focus is stored as an
/// id (`move_focus` sets `focused_id = meta.id`), so a focusable node without
/// one is found by the traversal and dropped on the same line — `focusable:
/// true` alone is inert. An earlier version of this fix "passed" only because
/// the probe happened to set `.id("vl")`. The widgets derive their own id from
/// the state name so they are operable as shipped.
const N: usize = 100_000;

#[test]
fn a_virtual_list_is_keyboard_operable() {
    let mut h = App::new(|cx: &mut BuildCx| {
        VirtualList::new(cx, "vl", N, 24.0, 300.0, |i| {
            widgets::text(format!("Row {i}"))
        })
        .into()
    })
    .run_headless(Size::new(400.0, 300.0));
    h.pump();
    let off: Signal<f64> = h.runtime().signal("vl", || 0.0);
    key(&mut h, NamedKey::Tab);

    key(&mut h, NamedKey::ArrowDown);
    assert!(off.get(h.runtime()) > 0.0, "↓ scrolls a line");

    key(&mut h, NamedKey::End);
    assert_eq!(
        off.get(h.runtime()),
        N as f64 * 24.0 - 300.0,
        "End reaches the true bottom of 100 000 rows — the whole point of \
         declaring set_size is that this is reachable"
    );

    key(&mut h, NamedKey::Home);
    assert_eq!(off.get(h.runtime()), 0.0, "Home returns");

    key(&mut h, NamedKey::PageDown);
    assert!(off.get(h.runtime()) > 0.0, "PageDown scrolls");
    h.assert_view_coherent();
}

/// Scrolling must actually *materialize* new rows — the realize half of the
/// contract. Moving the offset without changing what is in the tree would
/// leave an AT reading the same 24 labels forever.
#[test]
fn scrolling_realizes_rows_that_were_not_in_the_tree() {
    let mut h = App::new(|cx: &mut BuildCx| {
        VirtualList::new(cx, "vl", N, 24.0, 300.0, |i| {
            widgets::text(format!("Row {i}"))
        })
        .into()
    })
    .run_headless(Size::new(400.0, 300.0));
    h.pump();

    fn labels(h: &Headless) -> Vec<String> {
        fn walk(n: &lumen_core::semantics::SemanticsNode, out: &mut Vec<String>) {
            if !n.label.is_empty() {
                out.push(n.label.clone());
            }
            n.children.iter().for_each(|c| walk(c, out));
        }
        let mut v = Vec::new();
        walk(&h.semantics_elided(), &mut v);
        v
    }

    let before = labels(&h);
    assert!(
        before.iter().any(|l| l == "Row 0"),
        "the window opens at the top"
    );
    assert!(
        !before.iter().any(|l| l == "Row 99999"),
        "and does not contain the last row — it is virtualized"
    );

    key(&mut h, NamedKey::Tab);
    key(&mut h, NamedKey::End);
    let after = labels(&h);
    assert!(
        after.iter().any(|l| l == "Row 99999"),
        "the last row is realized once scrolled to; an AT that follows the \
         set_size hint can actually get there. Got: {:?}",
        &after[..after.len().min(4)]
    );
    assert!(
        !after.iter().any(|l| l == "Row 0"),
        "and the top is released again — still a window, not a leak"
    );
}

#[test]
fn a_data_grid_is_keyboard_operable() {
    let mut h = App::new(|cx: &mut BuildCx| {
        DataGrid::new(cx, "dg", &["A", "B"], N, 22.0, 300.0, |r, c| {
            format!("{r}:{c}")
        })
        .into()
    })
    .run_headless(Size::new(400.0, 600.0));
    h.pump();
    let off: Signal<f64> = h.runtime().signal("dg", || 0.0);
    key(&mut h, NamedKey::Tab);
    key(&mut h, NamedKey::End);
    assert_eq!(
        off.get(h.runtime()),
        N as f64 * 22.0 - 300.0,
        "End reaches the bottom of the grid"
    );
    h.assert_view_coherent();
}

/// The control. W3 gave `Scrollable` this behaviour; these tests exist because
/// it never propagated to the widgets that need it most.
#[test]
fn scrollable_still_works_as_the_control() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let rows: Vec<Element> = (0..40).map(|i| widgets::text(format!("row {i}"))).collect();
        Scrollable::new(cx, "sc", 100.0, 800.0, rows).into()
    })
    .run_headless(Size::new(220.0, 140.0));
    h.pump();
    let off: Signal<f64> = h.runtime().signal("sc", || 0.0);
    key(&mut h, NamedKey::Tab);
    key(&mut h, NamedKey::End);
    assert_eq!(off.get(h.runtime()), 700.0);
}
