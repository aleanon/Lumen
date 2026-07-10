//! W.3 (docs/plan-remediation-2026-07.md): `cx.memo` / `cx.effect` — the
//! 02 §4 conveniences, previously reachable only via `cx.runtime()`.

use kurbo::Size;
use lumen_core::state::{ReadCx, Signal};
use lumen_widgets::{col, widgets, App};

#[test]
fn cx_memo_derives_and_updates() {
    let mut h = App::new(|cx| {
        cx.signal("n", || 2i64);
        let doubled = cx.memo("doubled", |scope| {
            let n: Signal<i64> = scope.runtime().signal("n", || 2i64);
            n.get(scope) * 2
        });
        col![widgets::text(format!("2n={}", doubled.get(cx.runtime()))).id("out")]
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    assert!(h.semantics_json().to_string().contains("2n=4"));

    let n: Signal<i64> = h.runtime().signal("n", || 2i64);
    n.set(h.runtime(), 5);
    h.pump();
    assert!(
        h.semantics_json().to_string().contains("2n=10"),
        "memo recomputed on dependency change"
    );
    h.assert_view_coherent();
}

#[test]
fn cx_effect_reruns_on_dependency_change() {
    let mut h = App::new(|cx| {
        cx.signal("src", || 0i64);
        cx.signal("mirror", || -1i64);
        cx.effect("mirror-src", |scope| {
            let src: Signal<i64> = scope.runtime().signal("src", || 0i64);
            let mirror: Signal<i64> = scope.runtime().signal("mirror", || -1i64);
            let v = src.get(scope);
            mirror.set(scope.runtime(), v);
        });
        col![widgets::text("app").id("t")]
    })
    .run_headless(Size::new(300.0, 200.0));
    h.pump();
    let mirror: Signal<i64> = h.runtime().signal("mirror", || -1i64);
    assert_eq!(mirror.get(h.runtime()), 0, "effect ran immediately");

    let src: Signal<i64> = h.runtime().signal("src", || 0i64);
    src.set(h.runtime(), 42);
    h.pump();
    assert_eq!(
        mirror.get(h.runtime()),
        42,
        "effect re-ran when its dependency changed"
    );
}
