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
    assert!(a.is_animating(), "clock animates off the virtual clock");
    a.advance(1000.0); // one second of virtual time
    assert!(
        tree(&mut a).contains("00:01"),
        "advancing the clock ticks time"
    );
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
    assert!(a.is_animating(), "spinner animates off the virtual clock");
    let before = a.screenshot().pixels().to_vec();
    a.advance(150.0); // 150ms of virtual time ≈ 45° of rotation
    let after = a.screenshot().pixels().to_vec();
    assert_ne!(before, after, "spinner rotates as the clock advances");
}

#[test]
fn modal_overlay() {
    let mut a = iced_parity::modal::main_app().run_headless(Size::new(300.0, 240.0));
    a.pump();
    assert!(!tree(&mut a).contains("Are you sure?"), "closed by default");
    click(&mut a, "#open");
    assert!(tree(&mut a).contains("Are you sure?"), "dialog shown");
    click(&mut a, "#close");
    assert!(!tree(&mut a).contains("Are you sure?"), "dialog closed");
}

#[test]
fn toast_notification() {
    let mut a = iced_parity::toast::main_app().run_headless(Size::new(240.0, 160.0));
    a.pump();
    assert!(!tree(&mut a).contains("Saved"));
    click(&mut a, "#notify");
    assert!(tree(&mut a).contains("Saved"), "toast appears");
    for _ in 0..3 {
        click(&mut a, "#tick");
    }
    assert!(!tree(&mut a).contains("Saved"), "toast auto-dismisses");
}

#[test]
fn markdown_render() {
    let mut a = iced_parity::markdown::main_app().run_headless(Size::new(320.0, 260.0));
    a.pump();
    let t = tree(&mut a);
    assert!(t.contains("Lumen") && t.contains("Features"), "headings");
    assert!(
        t.contains("•") && t.contains("deterministic rendering"),
        "list item"
    );
    assert!(t.contains("first-class"), "emphasis run");
}

#[test]
fn changelog_scroll() {
    let mut a = iced_parity::changelog::main_app().run_headless(Size::new(320.0, 240.0));
    a.pump();
    assert!(tree(&mut a).contains("Changelog"));
}

#[test]
fn pane_grid_resizes() {
    use kurbo::Point;
    use lumen_core::events::{Event, PointerEvent};
    let mut a = iced_parity::pane_grid::main_app().run_headless(Size::new(300.0, 160.0));
    a.pump();
    // The divider's x marks the split; dragging right moves it right.
    fn divider_x(h: &lumen_widgets::Headless) -> f64 {
        fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<kurbo::Rect> {
            if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
                return Some(n.bounds);
            }
            n.children.iter().find_map(|c| find(c, id))
        }
        find(&h.semantics_doc().root.elided(), "split-divider")
            .unwrap()
            .x0
    }
    let before = divider_x(&a);
    a.inject(Event::PointerDown(PointerEvent::at(Point::new(
        240.0, 80.0,
    ))));
    a.pump();
    assert!(divider_x(&a) > before + 40.0, "split moved right on drag");
}

#[test]
fn svg_asset() {
    let mut a = iced_parity::svg::main_app().run_headless(Size::new(120.0, 120.0));
    a.pump();
    assert!(tree(&mut a).contains("SVG asset") && nonblank(&mut a));
}

#[test]
fn styling_lss() {
    let mut a = iced_parity::styling::main_app().run_headless(Size::new(200.0, 140.0));
    a.pump();
    let styles = rpc(&mut a, "ui.getStyles", json!({ "selector": "#title" }));
    assert!(
        styles["result"].to_string().contains("1a73e8"),
        "title themed by .lss"
    );
}

#[test]
fn stopwatch_runs() {
    let mut a = iced_parity::stopwatch::main_app().run_headless(Size::new(200.0, 200.0));
    a.pump();
    click(&mut a, "#tick"); // not running → no change
    assert!(tree(&mut a).contains("00:00"));
    click(&mut a, "#toggle"); // start
    click(&mut a, "#tick");
    click(&mut a, "#tick");
    assert!(tree(&mut a).contains("00:02"), "ticks while running");
}

#[test]
fn image_viewer() {
    let mut a = iced_parity::image::main_app().run_headless(Size::new(140.0, 120.0));
    a.pump();
    assert!(tree(&mut a).contains("Image viewer") && nonblank(&mut a));
}

#[test]
fn system_information() {
    let mut a = iced_parity::system_information::main_app().run_headless(Size::new(260.0, 160.0));
    a.pump();
    let t = tree(&mut a);
    assert!(t.contains("OS: ") && t.contains("Arch: ") && t.contains("CPUs: "));
}

#[test]
fn websocket_echo() {
    use std::net::TcpListener;
    // A local echo server.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            let mut ws = tungstenite::accept(stream).unwrap();
            while let Ok(msg) = ws.read() {
                if msg.is_text() && ws.send(msg).is_err() {
                    break;
                }
            }
        }
    });
    let reply = iced_parity::websocket::echo_once(&format!("ws://127.0.0.1:{port}/"), "ping");
    assert_eq!(reply.as_deref(), Some("ping"), "websocket round-trip");
}
