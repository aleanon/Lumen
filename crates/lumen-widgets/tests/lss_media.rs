//! B.2 (docs/plan-remediation-2026-07.md): `@media` blocks gate on the live
//! window context — before B.2 their rules applied unconditionally at any
//! size — and a resize re-resolves them.

use kurbo::Size;
use lumen_widgets::{col, widgets, App};

fn bg_of(styles: &serde_json::Value) -> Option<String> {
    styles
        .get("background")?
        .get("value")?
        .as_str()
        .map(str::to_string)
}

const SHEET: &str = "#b { background: #00ff00ff; } \
                     @media (width < 500) { #b { background: #ff0000ff; } }";

#[test]
fn media_rules_gate_on_window_size() {
    // Narrow window: the media rule applies.
    let mut narrow = App::new(|_cx| col![widgets::button("Hi", |_| {}).id("b")])
        .stylesheet(SHEET)
        .run_headless(Size::new(400.0, 300.0));
    narrow.pump();
    assert_eq!(
        bg_of(&narrow.get_styles("#b")).as_deref(),
        Some("#ff0000ff"),
        "media rule active at 400px"
    );

    // Wide window: it must NOT (the pre-B.2 bug applied it anyway).
    let mut wide = App::new(|_cx| col![widgets::button("Hi", |_| {}).id("b")])
        .stylesheet(SHEET)
        .run_headless(Size::new(800.0, 300.0));
    wide.pump();
    assert_eq!(
        bg_of(&wide.get_styles("#b")).as_deref(),
        Some("#00ff00ff"),
        "media rule inactive at 800px"
    );
    wide.assert_view_coherent();
}

#[test]
fn resize_re_resolves_media_rules() {
    let mut h = App::new(|_cx| col![widgets::button("Hi", |_| {}).id("b")])
        .stylesheet(SHEET)
        .run_headless(Size::new(800.0, 300.0));
    h.pump();
    assert_eq!(bg_of(&h.get_styles("#b")).as_deref(), Some("#00ff00ff"));

    h.prepare_resize(Size::new(400.0, 300.0), 1.0);
    h.pump();
    assert_eq!(
        bg_of(&h.get_styles("#b")).as_deref(),
        Some("#ff0000ff"),
        "shrinking past the breakpoint activates the media rule"
    );
    h.assert_view_coherent();
}
