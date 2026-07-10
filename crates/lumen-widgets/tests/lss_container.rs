//! B.2b (docs/plan-remediation-2026-07.md): `@media container(…)` — queries
//! test the nearest `.container()` ancestor's laid-out size instead of the
//! window (04 §6). Previously a parse error.

use kurbo::Size;
use lumen_layout::Dim;
use lumen_widgets::{col, widgets, App, Element};

/// A fixed-width `.container()` box holding one button.
fn container_with_button(width: f32, id: &str) -> Element {
    let mut b: Element = widgets::button("", |_| {}).id(id);
    b.style.width = Dim::px(60.0);
    let mut e = Element::default();
    e.style.width = Dim::px(width);
    e.style.height = Dim::px(40.0);
    e.children = vec![b];
    e.container()
}

#[test]
fn container_query_gates_on_the_ancestor_size_not_the_window() {
    // The 400px window would satisfy `width > 200px` for *both* buttons if
    // this were a window query; only the wide container's button goes red.
    let sheet = "@media container(width > 200px) { button { background: #ff0000; } }";
    let mut h = App::new(|_cx| {
        let mut loose: Element = widgets::button("", |_| {}).id("loose");
        loose.style.width = Dim::px(60.0);
        col![
            container_with_button(300.0, "wide"),
            container_with_button(100.0, "narrow"),
            loose // no container ancestor → query fails closed
        ]
    })
    .stylesheet(sheet)
    .run_headless(Size::new(400.0, 300.0));
    h.pump();

    let shot = h.screenshot();
    let px = |id: &str| {
        let b = h.node_bounds_by_id(id).unwrap();
        shot.pixel(b.center().x as u32, b.center().y as u32)
    };
    let wide = px("wide");
    let narrow = px("narrow");
    let loose = px("loose");
    assert!(
        wide[0] > 200 && wide[1] < 60,
        "button in the 300px container matches width > 200px: {wide:?}"
    );
    assert!(
        !(narrow[0] > 200 && narrow[1] < 60),
        "button in the 100px container must not match: {narrow:?}"
    );
    assert!(
        !(loose[0] > 200 && loose[1] < 60),
        "button outside any container fails closed: {loose:?}"
    );
    h.assert_view_coherent();
}

#[test]
fn container_resize_re_resolves() {
    // The container's width follows a signal; crossing the 200px threshold
    // must flip the query (the bounded post-layout re-pass measures it).
    let sheet = "@media container(width > 200px) { button { background: #ff0000; } }";
    let mut h = App::new(|cx| {
        let w = cx.signal("w", || 100.0f64);
        col![container_with_button(w.get(cx.runtime()) as f32, "b")]
    })
    .stylesheet(sheet)
    .run_headless(Size::new(400.0, 300.0));
    h.pump();

    let probe = |h: &mut lumen_widgets::Headless| {
        let b = h.node_bounds_by_id("b").unwrap();
        let shot = h.screenshot();
        shot.pixel(b.center().x as u32, b.center().y as u32)
    };
    let before = probe(&mut h);
    assert!(!(before[0] > 200 && before[1] < 60), "narrow: {before:?}");

    let w = h.runtime().signal("w", || 100.0f64);
    w.set(h.runtime(), 300.0);
    h.pump();
    let after = probe(&mut h);
    assert!(
        after[0] > 200 && after[1] < 60,
        "after widening past 200px the query matches: {after:?}"
    );
}
