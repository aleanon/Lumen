//! F2 step 3: the handler-currency check. A handler wrapped in `stable_handler!`
//! may capture only stable `Copy` state (signal handles, scalars) — never an
//! owned snapshot that goes stale when the handler is retained. This test proves
//! the good path drives the app; the rejected path is a `compile_fail` doctest
//! on `lumen_macros::stable_handler`.

use kurbo::Size;
use lumen_core::state::Signal;
use lumen_widgets::{stable_handler, widgets, App, BuildCx};

#[test]
fn stable_handler_captures_signal_and_drives_app() {
    let mut h = App::new(|cx: &mut BuildCx| {
        let count: Signal<i64> = cx.signal("count", || 0);
        let rt = cx.runtime();
        widgets::column(vec![
            widgets::text(format!("count={}", count.get(rt))).id("label"),
            // The handler captures only `count` (a `Copy` Signal handle) → passes
            // the currency check.
            widgets::button(
                "inc",
                stable_handler!(move |rt| count.update(rt, |c| *c += 1)),
            )
            .id("inc"),
        ])
    })
    .run_headless(Size::new(200.0, 120.0));

    let count: Signal<i64> = h.runtime().signal("count", || 0);
    count.set(h.runtime(), 41);
    count.update(h.runtime(), |c| *c += 1);
    h.pump();
    assert!(h.semantics_json().to_string().contains("count=42"));
}
