//! Each iced-parity example is exercised through lumen-agent (canvas examples
//! also assert they render non-trivial pixels).
use lumen_agent::dispatch;
use lumen_core::geometry::Size;
use lumen_widgets::Headless;
use serde_json::{json, Value};

fn rpc(a: &mut Headless, m: &str, p: Value) -> Value {
    dispatch(
        a,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": m, "params": p }),
    )
}
fn tree(a: &mut Headless) -> String {
    rpc(a, "ui.getTree", json!({}))["result"].to_string()
}
fn click(a: &mut Headless, sel: &str) {
    rpc(a, "input.click", json!({ "selector": sel }));
}
fn nonblank(h: &mut Headless) -> bool {
    h.screenshot()
        .pixels()
        .chunks_exact(4)
        .any(|p| p[0] < 250 || p[1] < 250 || p[2] < 250)
}

#[test]
fn counter() {
    let mut a = iced_parity::counter::main_app().run_headless(Size::new(200.0, 160.0));
    a.pump();
    click(&mut a, "#inc");
    click(&mut a, "#inc");
    assert!(tree(&mut a).contains("\"2\""), "counter increments");
}

#[test]
fn todos() {
    let mut a = iced_parity::todos::main_app().run_headless(Size::new(300.0, 240.0));
    a.pump();
    rpc(
        &mut a,
        "input.type",
        json!({ "selector": "#draft", "text": "Buy milk" }),
    );
    click(&mut a, "#add");
    assert!(tree(&mut a).contains("Buy milk"), "task added");
}

#[test]
fn events() {
    let mut a = iced_parity::events::main_app().run_headless(Size::new(240.0, 120.0));
    a.pump();
    click(&mut a, "#target");
    assert!(tree(&mut a).contains("clicked Click me"), "event logged");
}

#[test]
fn tour() {
    let mut a = iced_parity::tour::main_app().run_headless(Size::new(300.0, 200.0));
    a.pump();
    assert!(tree(&mut a).contains("Page 1/4"));
    click(&mut a, "#next");
    assert!(
        tree(&mut a).contains("Page 2/4: Widgets"),
        "navigates pages"
    );
    click(&mut a, "#back");
    assert!(tree(&mut a).contains("Page 1/4"));
}

#[test]
fn clock_canvas() {
    let mut a = iced_parity::clock::main_app().run_headless(Size::new(160.0, 200.0));
    a.pump();
    assert!(nonblank(&mut a), "clock face renders");
    click(&mut a, "#tick");
    assert!(tree(&mut a).contains("00:01"), "tick advances time");
}

#[test]
fn sierpinski_canvas() {
    let mut a = iced_parity::sierpinski::main_app().run_headless(Size::new(180.0, 200.0));
    a.pump();
    assert!(nonblank(&mut a), "fractal renders");
    assert!(tree(&mut a).contains("depth 4"));
    click(&mut a, "#more");
    assert!(tree(&mut a).contains("depth 5"));
}

#[test]
fn color_palette_canvas() {
    let mut a = iced_parity::color_palette::main_app().run_headless(Size::new(260.0, 120.0));
    a.pump();
    assert!(nonblank(&mut a), "palette renders");
    assert!(tree(&mut a).contains("6 colors"));
    click(&mut a, "#more");
    assert!(tree(&mut a).contains("7 colors"));
}

#[test]
fn progress_bar() {
    let mut a = iced_parity::progress_bar::main_app().run_headless(Size::new(260.0, 120.0));
    a.pump();
    assert!(tree(&mut a).contains("30%"));
    click(&mut a, "#more");
    assert!(tree(&mut a).contains("40%"), "progress advances");
}

#[test]
fn gradient_canvas() {
    let mut a = iced_parity::gradient::main_app().run_headless(Size::new(240.0, 120.0));
    a.pump();
    // The gradient produces a spread of distinct colors across the strip.
    let img = a.screenshot();
    let left = {
        let p = img.pixels();
        [p[(80 * 240 + 10) * 4], p[(80 * 240 + 10) * 4 + 2]]
    };
    let right = {
        let p = img.pixels();
        let i = (80 * 240 + 210) * 4;
        [p[i], p[i + 2]]
    };
    assert_ne!(left, right, "gradient varies across the strip");
}

#[test]
fn loading_spinner_canvas() {
    let mut a = iced_parity::loading_spinners::main_app().run_headless(Size::new(120.0, 120.0));
    a.pump();
    let before = a.screenshot().pixels().to_vec();
    click(&mut a, "#advance");
    let after = a.screenshot().pixels().to_vec();
    assert_ne!(before, after, "spinner rotates on advance");
}
