//! F3.2: reactive prop bindings on `Element`, evaluated during build. The bound
//! prop tracks its signal, its dependency keys land in the node's semantics
//! `deps`, and the view stays coherent with a fresh rebuild.

use std::cell::Cell;
use std::rc::Rc;

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_core::{Color, Dynamic};
use lumen_widgets::{text, widgets, App, BuildCx};

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

#[test]
fn bg_binding_patches_without_rebuild() {
    // F3.4: a paint-only (background) binding change patches the node + repaints
    // WITHOUT re-running the build — the surgical, model-c path.
    let build_runs = Rc::new(Cell::new(0u32));
    let br = build_runs.clone();
    let mut h = App::new(move |cx: &mut BuildCx| {
        br.set(br.get() + 1);
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

    let runs = build_runs.get();
    assert_eq!(runs, 1, "one build so far");

    // Flip the paint-only binding's signal.
    let on: Signal<bool> = h.runtime().signal("on", || false);
    on.set(h.runtime(), true);
    let stats = h.pump();

    assert_eq!(
        build_runs.get(),
        runs,
        "a background-only change is patched, not rebuilt"
    );
    assert!(stats.painted, "the patch repainted the changed region");
    // And the patched frame equals a fresh rebuild (coherence — this re-runs the
    // build, so check counts first).
    h.assert_view_coherent();

    // A structural change (a signal read in the build body) DOES rebuild.
    let br2 = build_runs.clone();
    let mut h2 = App::new(move |cx: &mut BuildCx| {
        br2.set(br2.get() + 1);
        let s: Signal<i64> = cx.signal("struct", || 0);
        widgets::column(vec![
            widgets::text(format!("{}", s.get(cx.runtime()))).id("t")
        ])
    })
    .run_headless(Size::new(120.0, 60.0));
    let base = build_runs.get();
    let s: Signal<i64> = h2.runtime().signal("struct", || 0);
    s.set(h2.runtime(), 1);
    h2.pump();
    assert!(
        build_runs.get() > base,
        "a structural (in-build) read change rebuilds"
    );
    h2.assert_view_coherent();
}

/// Deterministic LCG.
fn lcg(s: &mut u64) -> u64 {
    *s = s
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *s >> 33
}

#[test]
fn fuzz_mixed_bindings_and_scopes_stay_coherent() {
    // Each row is a scope with a text binding (structural) and a background
    // binding (paint-only, patched). Random writes across all three signal
    // kinds must keep the incremental view equal to a fresh rebuild.
    const N: i64 = 6;
    let app = App::new(|cx: &mut BuildCx| {
        let rows: Vec<_> = (0..N)
            .map(|i| {
                cx.scope(&format!("row-{i}"), move |cx| {
                    let t: Signal<i64> = cx.signal(&format!("t-{i}"), || 0);
                    let g: Signal<i64> = cx.signal(&format!("g-{i}"), || 0);
                    widgets::text("row")
                        .id("row")
                        .bind_text(Dynamic::new(move |rt| format!("row {i}: {}", t.get(rt))))
                        .bind_background(Dynamic::new(move |rt| {
                            Color::srgb8((g.get(rt) & 0xff) as u8, 0, 0, 255)
                        }))
                })
            })
            .collect();
        widgets::column(rows)
    });
    let mut h = app.run_headless(Size::new(300.0, 400.0));

    let mut seed = 0xC0FF_EE12_3456_7890u64;
    for _ in 0..80 {
        let k = (lcg(&mut seed) % 4) as usize;
        for _ in 0..k {
            let i = (lcg(&mut seed) as i64) % N;
            // Half the writes hit a text (structural) signal, half a bg (patch) one.
            let name = if lcg(&mut seed) & 1 == 0 {
                format!("t-{i}")
            } else {
                format!("g-{i}")
            };
            let s: Signal<i64> = h.runtime().signal(&name, || 0);
            s.update(h.runtime(), |v| *v += 1);
        }
        h.pump();
        h.assert_view_coherent();
    }
}

#[test]
fn text_macro_sugar_tracks_signals_and_reports_deps() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let a: Signal<i64> = cx.signal("a", || 1);
        let b: Signal<i64> = cx.signal("b", || 2);
        widgets::column(vec![text!(cx, "{a} + {b}").id("sum")])
    })
    .run_headless(Size::new(200.0, 80.0));

    assert!(h.semantics_json().to_string().contains("1 + 2"));
    let doc = h.semantics_doc();
    let mut deps = find(&doc.root, "sum").and_then(|n| n.deps.clone()).unwrap();
    deps.sort();
    assert_eq!(deps, vec!["a".to_string(), "b".to_string()]);
    drop(doc);
    h.assert_view_coherent();

    let a: Signal<i64> = h.runtime().signal("a", || 1);
    a.set(h.runtime(), 40);
    h.pump();
    assert!(h.semantics_json().to_string().contains("40 + 2"));
    h.assert_view_coherent();
}
