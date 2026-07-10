//! T1.7 (app side): the `.lss` cascade applies to widgets and hot-reloads;
//! `get_styles` reflects the change; a broken edit keeps the old style + E0101.

use kurbo::Size;
use lumen_widgets::{widgets, App, ReloadResult};

fn bg_of(styles: &serde_json::Value) -> Option<String> {
    styles
        .get("background")?
        .get("value")?
        .as_str()
        .map(|s| s.to_string())
}

#[test]
fn lss_applies_hot_reloads_and_rejects_broken() {
    let mut h = App::new(|_| widgets::button("Hi", |_| {}).id("b"))
        .stylesheet("button { background: #ff0000ff; }")
        .run_headless(Size::new(200.0, 80.0));
    h.pump();

    // initial style applied (red)
    assert_eq!(bg_of(&h.get_styles("#b")).as_deref(), Some("#ff0000ff"));

    // hot reload to blue
    match h.set_stylesheet("button { background: #0000ffff; }") {
        ReloadResult::Ok => {}
        other => panic!("expected Ok, got {other:?}"),
    }
    assert_eq!(bg_of(&h.get_styles("#b")).as_deref(), Some("#0000ffff"));

    // broken edit: rejected with E0101, previous (blue) stays live
    match h.set_stylesheet("button { background: }") {
        ReloadResult::Failed(diags) => {
            assert!(diags.iter().any(|d| d.code == "E0101"), "{diags:?}");
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    assert_eq!(
        bg_of(&h.get_styles("#b")).as_deref(),
        Some("#0000ffff"),
        "broken edit must keep the old style"
    );
}

#[test]
fn get_styles_unknown_selector_is_null() {
    let mut h =
        App::new(|_| widgets::button("Hi", |_| {}).id("b")).run_headless(Size::new(200.0, 80.0));
    h.pump();
    assert!(h.get_styles("#nope").is_null());
}

/// B.7b (04 §7): `get_styles` carries the winning declaration's source span
/// — an agent can jump from a computed value to the `.lss` line that set it.
#[test]
fn get_styles_reports_the_winning_span() {
    let mut h = App::new(|_| widgets::button("Hi", |_| {}).id("b"))
        .stylesheet("/* comment */\nbutton { background: #ff0000ff; }")
        .run_headless(Size::new(200.0, 80.0));
    h.pump();
    let styles = h.get_styles("#b");
    let span = &styles["background"]["span"];
    assert_eq!(span["line"], serde_json::json!(2), "{styles}");
    assert!(span["col"].as_u64().unwrap() >= 1, "{styles}");
}
