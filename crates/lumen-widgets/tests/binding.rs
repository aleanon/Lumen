//! F3.2: reactive prop bindings on `Element`, evaluated during build. The bound
//! prop tracks its signal, its dependency keys land in the node's semantics
//! `deps`, and the view stays coherent with a fresh rebuild.

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_core::{Color, Dynamic};
use lumen_widgets::{widgets, App, BuildCx};

fn find<'a>(
    n: &'a lumen_core::semantics::SemanticsNode,
    id: &str,
) -> Option<&'a lumen_core::semantics::SemanticsNode> {
    if n.id.as_ref().map(|s| s.0.as_str()) == Some(id) {
        return Some(n);
    }
    n.children.iter().find_map(|c| find(c, id))
}

#[test]
fn bound_text_tracks_its_signal_and_reports_deps() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let n: Signal<i64> = cx.signal("n", || 0);
        widgets::column(vec![widgets::text("placeholder")
            .id("lbl")
            .bind_text(Dynamic::new(move |rt| format!("n={}", n.get(rt))))])
    })
    .run_headless(Size::new(200.0, 80.0));

    // Initial build evaluated the binding.
    assert!(h.semantics_json().to_string().contains("n=0"));
    // The node reports its reactive dependency.
    let doc = h.semantics_doc();
    let want = ["n".to_string()];
    assert_eq!(
        find(&doc.root, "lbl").and_then(|n| n.deps.as_deref()),
        Some(want.as_slice())
    );
    drop(doc);
    h.assert_view_coherent();

    // A write updates the bound text.
    let n: Signal<i64> = h.runtime().signal("n", || 0);
    n.set(h.runtime(), 7);
    h.pump();
    assert!(h.semantics_json().to_string().contains("n=7"));
    h.assert_view_coherent();
}

#[test]
fn bound_background_tracks_its_signal() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let on: Signal<bool> = cx.signal("on", || false);
        widgets::column(vec![widgets::text("box").id("b").bind_background(
            Dynamic::new(move |rt| {
                if on.get(rt) {
                    Color::srgb8(0, 200, 0, 255)
                } else {
                    Color::srgb8(200, 0, 0, 255)
                }
            }),
        )])
    })
    .run_headless(Size::new(120.0, 60.0));

    let doc = h.semantics_doc();
    let want = ["on".to_string()];
    assert_eq!(
        find(&doc.root, "b").and_then(|n| n.deps.as_deref()),
        Some(want.as_slice())
    );
    drop(doc);
    h.assert_view_coherent();

    let on: Signal<bool> = h.runtime().signal("on", || false);
    on.set(h.runtime(), true);
    h.pump();
    h.assert_view_coherent();
}
