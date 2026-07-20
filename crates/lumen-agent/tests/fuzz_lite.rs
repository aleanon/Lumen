//! E.3 fuzz-lite: agent dispatch never panics on arbitrary JSON (bounded,
//! every gate; the libFuzzer `agent_json` target goes deeper nightly).
use kurbo::Size;
use proptest::prelude::*;
use serde_json::json;

fn app() -> lumen_widgets::Headless {
    let mut h = lumen_widgets::App::new(|_| lumen_widgets::widgets::text("fuzz").id("t"))
        .run_headless(Size::new(100.0, 100.0));
    h.pump();
    h
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]
    #[test]
    fn dispatch_never_panics_on_noise(method in ".{0,24}", sel in ".{0,24}", n in any::<i64>()) {
        let mut h = app();
        let req = json!({ "jsonrpc": "2.0", "id": n, "method": method,
                          "params": { "selector": sel, "timeout_ms": 1 } });
        let _ = lumen_agent::dispatch(&mut h, &req);
    }

    #[test]
    fn dispatch_never_panics_on_shape_violations(v in proptest::collection::vec(any::<u8>(), 0..64)) {
        let mut h = app();
        // Arbitrary bytes as a JSON string in random positions.
        let s = String::from_utf8_lossy(&v).into_owned();
        for req in [
            json!(s),
            json!({ "method": { "nested": s } }),
            json!({ "method": "ui.getTree", "params": s }),
            json!([s]),
        ] {
            let _ = lumen_agent::dispatch(&mut h, &req);
        }
    }
}
